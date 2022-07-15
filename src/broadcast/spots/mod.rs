

use log::{debug,warn,info};
use gst::prelude::*;
use crate::helpers::{make_element, upgrade_weak};
use super::methods;

use std::sync::{Arc, RwLock, Weak};


#[allow(dead_code)]
#[derive(Clone, Copy, Eq, PartialEq,Debug)]
pub enum ItemState {
    New,
    Prepared,   // gstreamer / decodebin has loaded the item, and assign all neccesary elements to it
    Activate,   // item is playing, means this is the currently playing item
    GoingEOS,   // item is on the way to EOS (time to load next item)
    Eos,        // item is EOS 
    EarlyEos,   // manualy triggered EOS
    Error,      // if item gots an error we end here 
    Unknown,    // Item is newley created and has no state
    Removed,    // item can removed (by clean fn in queue)
}

// Strong reference to our broadcast server state
#[derive(Debug, Clone)]
pub(crate) struct Item(Arc<ItemInner>);

// Weak reference to our broadcast server state
#[derive(Debug, Clone)]
pub(crate) struct ItemWeak(Weak<ItemInner>);

#[derive(Debug)]
struct ShareableValues {
    audio_pad: Option<gst::Pad>,
    mixer_pad: Option<gst::Pad>,
    state: ItemState,
    running_time: gst::ClockTime,
    fade_queue_sinkpad: Option<gst::Pad>,
    audio_pad_probe_going_eos_id: Option<gst::PadProbeId>,
    audio_pad_probe_block_id: Option<gst::PadProbeId>,
}

impl Default for ShareableValues {
    fn default() -> Self {
        ShareableValues {
            audio_pad: None,
            mixer_pad: None,
            state: ItemState::Unknown,
            running_time: gst::ClockTime::ZERO,
            fade_queue_sinkpad: None,
            audio_pad_probe_going_eos_id: None,
            audio_pad_probe_block_id: None,
        }
    }
}


#[derive(Debug)]
pub struct ItemInner {
    pub(crate) bin: gst::Bin,
    //pub(crate) dec: Option<gst::Element>,
    pub(crate) uri: String,
    pub(crate) rate: Option<i32>,
    pub(crate) broadcast_clone: Option<super::BroadcastWeak>,
    values: RwLock<ShareableValues>,
}

impl Default for ItemInner {
    fn default() -> Self {
        ItemInner {
            bin: gst::Bin::new(None),
            //dec: None,
            uri: "".to_string(),
            rate: None,
            broadcast_clone: None,
            values: RwLock::new(ShareableValues::default()),
        }
    }
}

// To be able to access the App's fields directly
impl std::ops::Deref for Item {
    type Target = ItemInner;

    fn deref(&self) -> &ItemInner {
        &self.0
    }
}

impl ItemWeak {
    // Try upgrading a weak reference to a strong one
    pub fn upgrade(&self) -> Option<Item> {
        self.0.upgrade().map(Item)
    }
}


impl Item {
    // Downgrade the strong reference to a weak reference
    pub fn downgrade(&self) -> ItemWeak {
        ItemWeak(Arc::downgrade(&self.0))
    }

    /// ## Hold items for GStreamer
    /// 
    /// - `location` is an url for the specific item, can be an local file url (file://) or http 
    /// - `broadcast` is a WeakRef to Broadcast Element
    /// - `fixed_length` is a Optionally duration for the element in seconds (important for endless radio streams)
    /// - `is_spot` handles this item as a spot (spots gets buffered)
    pub fn new(
        location: &str, 
        broadcast: super::BroadcastWeak,
        rate: Option<i32>,
    ) -> Result<Item, anyhow::Error> {

        // create a bin that hold the decoder
        let bin = gst::Bin::new(None);

        // create element for decode the uri
        let dec = make_element("uridecodebin", None)?;
        dec.try_set_property("uri", &location)?;

        info!("create new spot {} with audio rate {}", location, rate.unwrap_or(44100));
        dec.try_set_property("use-buffering", &true)?;

        // add element to bin
        bin.add(&dec)?;

        let item = Item(Arc::new(ItemInner {
            bin: bin.clone(),
            //dec: Some(dec.clone()),
            uri: location.to_string(),
            broadcast_clone: Some(broadcast.clone()),
            rate: rate,
            ..Default::default()
        }));
       
        // if the decoder is created and the sink is linked with a source element, 
        // the decoder auto-generates needed elements.
        // When the generation is finished, the output (src) elements triggers the signal
        // "pad_added"
        let item_clone = item.downgrade();
        dec.connect_pad_added(move |_uridecodebin, pad| {
            let item = upgrade_weak!(item_clone);
            if let Err(e) = item.decoder_pad_added(pad) {
                warn!("error on add decoder pad: {}", e);
            }
        });

        // this gets triggered if the decoder added all pads
        // after called state jumps to prepared... important on the broadcaster queue
        let item_clone = item.downgrade();
        dec.connect_no_more_pads(move |_| {
            let item = upgrade_weak!(item_clone);
            let mut values = item.values.write().unwrap();
            values.state = ItemState::Prepared;
            info!("item {} is prepared", item.uri);
        });
        

        let broadcast_clone = broadcast.clone().upgrade().unwrap();
        broadcast_clone.pipeline.add(&bin)?;
        bin.sync_state_with_parent()?;
        
        Ok(item)

    } 

    // sets the volume of the item
    pub fn set_volume(&self, volume: f64) -> bool {

        if let Some(audio_pad) = self.audio_pad() {

            if let Some(mixer_pad) = audio_pad.peer() {
                let broadcast = self.broadcast_clone.as_ref().unwrap().upgrade().unwrap();
                broadcast.mainmixer.set_volume(volume,mixer_pad);
                
                return true;
            } else {
                warn!("no peer for audio_pad found. (mixer not loaded)")
            }
        } else {
            warn!("audio_pad not loaded");
        }

        return false;

    }
  
    /// get triggered if the used encoder __uridecodebin__ added a pad
    /// (normaly after initialization)
    /// 
    /// generates a complete pipeline to ensure decodin, and converting is done
    fn decoder_pad_added(&self, pad: &gst::Pad) -> Result<(), anyhow::Error> {
        let crossfade_time_as_clock = crate::CROSSFADE_TIME_MS * gst::ClockTime::MSECOND; 
    
        // idea behinde queue:
        //
        // add so much time in the buffer like the duration for a crossfade time duration
        // so that when the main audio pad triggering eos we got enough audio data for the
        // crossfade duration
        let queue = make_element("queue", Some("fade-queue-%u")).unwrap();
        queue.try_set_property("max-size-buffers", 0 as u32)?;
        queue.try_set_property("max-size-time", &(crossfade_time_as_clock.nseconds()))?;
    
        {
            let mut values = self.values.write().unwrap();
            values.fade_queue_sinkpad = Some(queue.static_pad("sink").unwrap());
        }
    
        let audioconvert = make_element("audioconvert", None)?;
        let audioresample = make_element("audioresample", None)?;
        let capsfilter = make_element("capsfilter", None)?;

        let caps = gst::Caps::builder("audio/x-raw")
            .field("rate", &self.rate.unwrap_or(44100i32))
            .field("channels", &2i32)
            .build();
        capsfilter.try_set_property("caps", &caps)?;     
        
        self.bin.add_many(&[&audioresample, &capsfilter, &audioconvert, &queue])?;
    
        let sinkpad = audioconvert.static_pad("sink").unwrap();
        pad.link(&sinkpad)?;
    
        gst::Element::link_many(&[&audioconvert, &audioresample, &capsfilter, &queue ])?;
        
        let srcpad = queue.static_pad("src").unwrap();
    
        audioresample.sync_state_with_parent()?;
        capsfilter.sync_state_with_parent()?;
        audioconvert.sync_state_with_parent()?;
        queue.sync_state_with_parent()?;
    
        let audio_pad = gst::GhostPad::with_target(Some("src"), &srcpad)?;
        audio_pad.set_active(true)?;
        self.bin.add_pad(&audio_pad)?;
        
        {
            let mut values = self.values.write().unwrap();
            values.audio_pad = Some(self.bin.static_pad("src").unwrap());
        }


        //#[cfg(all(target_os = "macos"))]
        //{
            let block_probe_type = gst::PadProbeType::BLOCK | gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST;
            
            let item_clone = self.downgrade();
            audio_pad.add_probe(block_probe_type, move |pad, probe_info| {
                let item = upgrade_weak!(item_clone, gst::PadProbeReturn::Ok);
                //warn!("joooooo");
                item.pad_probe_blocked(pad, probe_info)
            });
        //}


    
        let item_clone = self.downgrade();
        let fade_queue_sinkpad = queue.static_pad("sink").unwrap();
        
        let audio_pad_probe_going_eos_id = fade_queue_sinkpad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |pad, probe_info| {
            let item = upgrade_weak!(item_clone, gst::PadProbeReturn::Pass);
            item.pad_probe_going_eos(pad, probe_info)
        });

        {
            let mut values = self.values.write().unwrap();
            values.audio_pad_probe_going_eos_id = audio_pad_probe_going_eos_id; 
        }


        Ok(())
    }

    fn pad_probe_blocked(&self, _pad: &gst::GhostPad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        let mut values = self.values.write().unwrap();

        debug!("pad probe blocked");
        if let Some(id) = info.id.take() {
            values.audio_pad_probe_block_id = Some(id);
        }
        drop(values);
    
        return gst::PadProbeReturn::Ok
    }


    /// ## check if item is going eos
    pub fn pad_probe_going_eos(&self, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        if let Some(data) = &info.data {
            if let gst::PadProbeData::Event(event) = data {

                if event.type_() == gst::EventType::Eos && self.state() != ItemState::EarlyEos  {
                    
                    let queue = pad.parent_element().unwrap();
                    let queue_size = queue.property::<u64>("current-level-time");
                    
                    drop(queue);
                    
                    {
                        let mut values = self.values.write().unwrap();
                        values.state = ItemState::GoingEOS;
                    }

                    let broadcast_clone = self.broadcast_clone.as_ref().unwrap();
                    let broadcast = upgrade_weak!(broadcast_clone, gst::PadProbeReturn::Pass);
                    broadcast.end_of_spot(queue_size);
                    
                    return gst::PadProbeReturn::Remove;
                }
            }
        }

        gst::PadProbeReturn::Pass
    }

    pub fn pad_probe_eos(&self, _pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        if let Some(data) = &info.data {
            if let gst::PadProbeData::Event(event) = data {
                if event.type_() == gst::EventType::Eos   {
                    if self.state() == ItemState::GoingEOS {
                        self.set_state(ItemState::Removed);

                        
                        return gst::PadProbeReturn::Remove;    
                    }
                }
            }
        }

        gst::PadProbeReturn::Pass
    }
    
    /// ## set the current running time for the _item_
    /// 
    /// - lock values to **write**
    pub fn pad_probe_running_time(&self, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        methods::pad_helper::running_time_method(pad, info, move |clock| {
            let mut values = self.values.write().unwrap();
            values.running_time = *clock;
            drop(values);
        })
    }

    /// # returns current state
    /// 
    /// return the state of the item
    /// - internally it locks the values for read, clones the state and return the state
    pub fn state(&self) -> ItemState {
        let v = self.values.read().unwrap();
        v.state.clone()
    }

    pub fn set_state(&self, state: ItemState) {
        let mut v = self.values.write().unwrap();
        v.state = state;
        drop(v);
    }

    /// # get the audio_pad
    /// 
    /// get the current audio_pad from the item
    pub fn audio_pad(&self) -> Option<gst::Pad> {
        
        let mut audio_pad_downgraded: Option<gst::Pad> = None;

        let values = self.values.read().unwrap();
        if let Some(audio_pad) = &values.audio_pad {
            let downgraded = audio_pad.downgrade();
            audio_pad_downgraded = downgraded.upgrade();
            drop(downgraded);
        } 

        drop(values);

        audio_pad_downgraded
    }


    /// ## set_offset
    /// 
    /// sets the duration playback offset, cause a rtp stream is continusly,
    /// if track 1 goes 2 minutes, the next track will start at 2 minutes, so if track 2 is 2 minutes and 10 seconds,
    /// the track starts playing at the 2 minute, so goes only 10 seconds and then stops.
    /// 
    /// with the audio_pad offset, of the current rtp stream duration, we ensure that the next track starts at 0.
    pub fn set_offset(&self, offset_nseconds: i64) {
        let values = self.values.read().unwrap();
        let audio_pad = values.audio_pad.as_ref().unwrap();
        audio_pad.set_offset(offset_nseconds);
    }

    //audio_pad_probe_block_id
    pub fn has_block_id(&self) -> bool {
        
        let values = self.values.read().unwrap();
        let has = values.audio_pad_probe_block_id.is_some();
        drop(values);
        has
    }

    pub fn remove_block(&self) -> Result<(), anyhow::Error> {

        let mut values = self.values.write().unwrap();
        let audio_pad_probe_block_id = values.audio_pad_probe_block_id.take().unwrap();
        values.audio_pad_probe_block_id = None;
        let audio_pad = values.audio_pad.as_ref().unwrap();
        audio_pad.remove_probe(audio_pad_probe_block_id);

        Ok(())
    }

    pub fn set_mixer_pad(&self, pad: gst::Pad) {
        let mut values = self.values.write().unwrap();
        values.mixer_pad = Some(pad);
    }

    pub fn mixer_pad(&self) -> Option<gst::Pad> {
        let mut mixer_pad_dg: Option<gst::Pad> = None;

        let values = self.values.read().unwrap();
        if let Some(mixer_pad) = &values.mixer_pad {
            let downgraded = mixer_pad.downgrade();
            mixer_pad_dg = downgraded.upgrade();
            drop(downgraded);
        } 

        drop(values);

        mixer_pad_dg
    }

    pub fn cleanup(&self) {
        info!("clean a removeable spot {}", self.uri);
        let sink = &self.bin;

        let audio_pad = self.audio_pad().unwrap();
        let mixer_pad = self.mixer_pad().unwrap();

        {
            let mut values = self.values.write().unwrap();
            values.fade_queue_sinkpad = None;
            values.audio_pad_probe_going_eos_id = None;
            values.audio_pad_probe_block_id = None;
            values.audio_pad = None;
            values.mixer_pad = None;
        }

        let block_probe = audio_pad.add_probe(gst::PadProbeType::BLOCK | gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST, move |_pad, _probe_info| {
            gst::PadProbeReturn::Ok
        });

        let _ = sink.set_state(gst::State::Null);   
        let _ = audio_pad.unlink(&mixer_pad);


        let broadcast_clone = self.broadcast_clone.as_ref().unwrap().upgrade().unwrap();

        broadcast_clone.mainmixer.release_pad(mixer_pad);
        broadcast_clone.pipeline.remove(sink).unwrap();
        
        if let Some(probe) = block_probe {
            audio_pad.remove_probe(probe);
        }

    }

}
