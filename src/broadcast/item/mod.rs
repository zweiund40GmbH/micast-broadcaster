

use log::{debug,warn};
use gstreamer as gst;
use gst::prelude::*;
use crate::helpers::{make_element, upgrade_weak};

use std::sync::{Arc, Mutex, RwLock, Weak};
use std::sync::mpsc::Sender;

#[allow(dead_code)]
#[derive(Clone, Copy, Eq, PartialEq,Debug)]
pub enum ItemState {
    New,
    Prepared, /// gstreamer / decodebin has loaded the item, and assign all neccesary elements to it
    Activate, /// item is playing, means this is the currently playing item
    GoingEOS, /// item is on the way to EOS (time to load next item)
    Eos, // item is EOS 
    EarlyEos, // manualy triggered EOS
    Unknown, /// Item is newley created and has no state
    Removed, // item can removed (by clean fn in queue)
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
    audio_pad_probe_block_id: Option<gst::PadProbeId>,
    decoder: Option<gst::Bin>,
    state: ItemState,
    running_time: gst::ClockTime,
    fadeout_end_stream_time: gst::ClockTime,
    fade_queue_sinkpad: Option<gst::Pad>,
    audio_pad_probe_going_eos_id: Option<gst::PadProbeId>,
}

impl Default for ShareableValues {
    fn default() -> Self {
        ShareableValues {
            audio_pad: None,
            audio_pad_probe_block_id: None,
            decoder: None,
            state: ItemState::Unknown,
            running_time: gst::ClockTime::ZERO,
            fadeout_end_stream_time: gst::ClockTime::ZERO,
            fade_queue_sinkpad: None,
            audio_pad_probe_going_eos_id: None,
        }
    }
}


#[derive(Debug)]
pub struct ItemInner {
    bin: gst::Bin,
    pub(crate) uri: String,
    broadcast_clone: Option<super::BroadcastWeak>,
    values: RwLock<ShareableValues>,
    max_duration: u32,
    is_spot: bool,
}

impl Default for ItemInner {
    fn default() -> Self {
        ItemInner {
            bin: gst::Bin::new(None),
            uri: "".to_string(),
            broadcast_clone: None,
            values: RwLock::new(ShareableValues::default()),
            max_duration: 0, // zero means no max duration based on this value, take the real element length
            is_spot: false,
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
    pub fn new(
        location: &str, 
        broadcast: super::BroadcastWeak,
        fixed_length: Option<u32>,
        is_spot: bool,

        //sender: Sender<gst::ClockTime>
    ) -> Result<Item, anyhow::Error> {

        debug!("create new item {}", location);

        // create a bin that hold the decoder
        let bin = gst::Bin::new(None);

        // create element for decode the uri
        let dec = make_element("uridecodebin", None)?;
        dec.set_property("uri", &location)?;

        // add element to bin
        bin.add(&dec)?;

        let item = Item(Arc::new(ItemInner {
            bin: bin.clone(),
            uri: location.to_string(),
            broadcast_clone: Some(broadcast.clone()),
            max_duration: fixed_length.unwrap_or(0),
            is_spot: is_spot,
            ..Default::default()
        }));
        
        let item_clone = item.downgrade();
        dec.connect_pad_added(move |_uridecodebin, pad| {
            let item = upgrade_weak!(item_clone);
            if let Err(e) = item.decoder_pad_added(pad) {
                warn!("error on add decoder pad: {}", e);
            }
        });

        let item_clone = item.downgrade();
        // this gets triggered if the decoder added all pads
        // after called state jumps to prepared... important on the broadcaster queue
        dec.connect_no_more_pads(move |_| {
            let item = upgrade_weak!(item_clone);
            let mut values = item.values.write().unwrap();
            values.state = ItemState::Prepared;
            debug!("item {} is prepared", item.uri);
        });

        let broadcast_clone = broadcast.clone().upgrade().unwrap();
        broadcast_clone.pipeline.add(&bin)?;
        bin.sync_state_with_parent()?;

        {
            let mut values = item.values.write().unwrap();
            values.decoder = Some(bin);
        }


        Ok(item)

    } 

    // sets the volume of the item
    pub fn set_volume(&self, volume: f64) -> bool {

        if let Some(audio_pad) = self.audio_pad() {

            if let Some(mixer_pad) = audio_pad.peer() {
                let _mixer = mixer_pad.set_property("volume", volume);
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
        
        
        if let Some(caps) = pad.current_caps() {
            if let Some(structure) = caps.structure(0) {
                debug!("new pad: {:?}, caps: {:?}", pad, structure);
            }
        }
    
        let queue = make_element("queue", Some("fade-queue-%u")).unwrap();
        queue.set_property("max-size-buffers", 0 as u32)?;
        queue.set_property("max-size-time", &(2*crate::CROSSFADE_TIME_MS * gst::ClockTime::MSECOND.nseconds()))?;
    
        {
            let mut values = self.values.write().unwrap();
            values.fade_queue_sinkpad = Some(queue.static_pad("sink").unwrap());
        }
        
        // g_object_set (queue, "max-size-buffers", 0, "max-size-time", 2 * CROSSFADE_TIME_MS * GST_MSECOND, NULL);
        //item->fade_queue_sinkpad = gst_element_get_static_pad (queue, "sink");
    
        if let Err(e) = self.bin.add(&queue) {
            warn!("could not add queue to bin: {:?}", e);
        }
    
        let audioresample = make_element("audioresample", None)?;
        let audioconvert = make_element("audioconvert", None)?;
        let capsfilter = make_element("capsfilter", None)?;

        let caps = gst::Caps::builder("audio/x-raw")
            //.field("rate", &48000i32)
            .field("rate", &44100i32)
            .build();
        capsfilter.set_property("caps", &caps)?;     
        
        self.bin.add_many(&[&audioresample, &capsfilter, &audioconvert, ])?;
    
        let sinkpad = audioresample.static_pad("sink").unwrap();
        pad.link(&sinkpad)?;
    
        gst::Element::link_many(&[&audioresample, &capsfilter, &audioconvert, &queue ])?;
        
        let srcpad = queue.static_pad("src").unwrap();
    
        audioresample.sync_state_with_parent()?;
        audioconvert.sync_state_with_parent()?;
        capsfilter.sync_state_with_parent()?;
        queue.sync_state_with_parent()?;
    
        let audio_pad = gst::GhostPad::with_target(Some("src"), &srcpad)?;
    
        audio_pad.set_active(true)?;
        self.bin.add_pad(&audio_pad)?;
        
        {
            let mut values = self.values.write().unwrap();
            values.audio_pad = Some(self.bin.static_pad("src").unwrap());
        }
  
    
        let block_probe_type = gst::PadProbeType::BLOCK | gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST;
        
        let item_clone = self.downgrade();
        audio_pad.add_probe(block_probe_type, move |pad, probe_info| {
            let item = upgrade_weak!(item_clone, gst::PadProbeReturn::Ok);

            item.pad_probe_blocked(pad, probe_info)
        });
    
    
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

                //debug!("event {:?}", event);
                if event.type_() == gst::EventType::Eos && self.state() != ItemState::EarlyEos  {
                    
                    // get the parent element of the audiopad (this is the crossfade queue) &
                    // get the current-level-time (the duration) of the queue and schedule the crossfade
                    let queue = pad.parent_element().unwrap();
                    
                    let queue_size = if let Ok(q) = queue.property("current-level-time") {
                        q.get().unwrap()
                    } else {
                        0
                    };
                    
                    //debug!("Queue size: {:?} for {} event {:?}", queue_size, self.uri,  event.type_());
                    drop(queue);
                    
                    debug!("going eos {}", self.uri);
                    {
                        let mut values = self.values.write().unwrap();
                        values.state = ItemState::GoingEOS;
                    }


                    // here we make some nasty trick to call schedule_crossfade to make a crossfade on the broadcaster
                    let item_clone = self.downgrade();
                    
                    let broadcast_clone = self.broadcast_clone.as_ref().unwrap();
                    if self.is_spot {
                        debug!("is a spot... we go to end_of_spot");
                        let broadcast = upgrade_weak!(broadcast_clone, gst::PadProbeReturn::Pass);
                        broadcast.end_of_spot(queue_size)
                    } else {
                        let broadcast = upgrade_weak!(broadcast_clone, gst::PadProbeReturn::Pass);
                        let item = upgrade_weak!(item_clone, gst::PadProbeReturn::Pass);
                        broadcast.schedule_crossfade(&item, queue_size);
                    }
                    
                    

                    return gst::PadProbeReturn::Remove;
                }
            }
        }

        gst::PadProbeReturn::Pass
    }

    pub fn pad_probe_eos(&self, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        if let Some(data) = &info.data {
            if let gst::PadProbeData::Event(event) = data {


                //debug!("event {:?}", event);
                if event.type_() == gst::EventType::Eos   {
                    
                    if self.state() == ItemState::GoingEOS {
                        //self.set_state(ItemState::Eos);
                        debug!("so we at EOS for {}. We trigger remove", self.uri);
                        self.set_state(ItemState::Removed);
                        return gst::PadProbeReturn::Remove;    
                    }
                }
            }
        }

        gst::PadProbeReturn::Pass
    }

    pub fn pad_probe_when_eos(&self, _pad: &gst::Pad, _info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        debug!("manual probe when eos...");

        self.set_state(ItemState::GoingEOS);

        let audio_pad = self.audio_pad().unwrap();
        audio_pad.push_event(gst::event::Eos::new());

        gst::PadProbeReturn::Pass
    }
    
    /// ## set the current running time for the _item_
    /// 
    /// - lock values to **write**
    pub fn pad_probe_running_time(&self, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        if let Some(event) = pad.sticky_event(gst::EventType::Segment, 0) { 
            if let Some(data) = &info.data {
                if let gst::PadProbeData::Buffer(buffer) = data {
                    if let gst::EventView::Segment(segment) = event.view() {
                        match segment.segment().to_running_time(buffer.pts().unwrap()) {
                            gst::GenericFormattedValue::Time(Some(clock)) => {

                                let mut values = self.values.write().unwrap();
                                values.running_time = clock;
                                //debug!("sets running_time: {}", clock);
                                drop(values);
                                

                            },
                            _ => {}
                        }
                    }
                }
            }
        }

        gst::PadProbeReturn::Pass
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
        
        drop(audio_pad);
        drop(values);

        Ok(())
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
        drop(audio_pad);
        drop(values);
    }


    pub fn remove_going_eos_probe(&self) -> Result<(), anyhow::Error> {

        
        let mut values = self.values.write().unwrap();

        let audio_pad_probe_going_eos_id = values.audio_pad_probe_going_eos_id.take().unwrap();
        values.audio_pad_probe_going_eos_id = None;

        let fade_queue_sinkpad = values.fade_queue_sinkpad.as_ref().unwrap();
        
        debug!("remove going eos probe {} id: {:?}", self.uri, audio_pad_probe_going_eos_id);

        fade_queue_sinkpad.remove_probe(audio_pad_probe_going_eos_id);
        
        drop(fade_queue_sinkpad);
        drop(values);

        Ok(())
    }


    pub fn prepare_for_early_end(&self) {
        warn!("early eos");
        self.set_state(ItemState::EarlyEos);

        if let Err(e) = self.remove_going_eos_probe() {
            warn!("Error: {}", e);
        }

    
        let audio_pad = self.audio_pad().unwrap();
        let sinkpad = audio_pad.peer().unwrap();

        let item_clone = self.downgrade();
        sinkpad.add_probe(gst::PadProbeType::BUFFER, move |pad, probe_info| {
            debug!("trigger peer on sinkpad for buffer");
            let item = upgrade_weak!(item_clone, gst::PadProbeReturn::Pass);
            item.pad_probe_when_eos(pad, probe_info)
        });


        if self.is_spot {

        } else {
            let broadcast_clone = self.broadcast_clone.as_ref().unwrap();
            let broadcast = upgrade_weak!(broadcast_clone);
            broadcast.schedule_crossfade(&self, 0);
        }

    }

}
