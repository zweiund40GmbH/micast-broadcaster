/// main work here
///
mod network;
mod spots;
mod builder;
mod mixer_bin;
mod volume;
mod whitenoise;
mod methods;
mod playlist;

pub use builder::Builder;

use gstreamer as gst;
use gst::prelude::*;

use crate::helpers::*;
use crate::sleep_ms;

use crate::gst_plugins;

use std::{
    sync::{Arc, Mutex, RwLock, Weak},
};

use anyhow::bail;
use log::{debug, warn, info};

// Strong reference to our broadcast server state
#[derive(Debug, Clone)]
pub struct Broadcast(Arc<BroadcastInner>);

// Weak reference to our broadcast server state
#[derive(Debug, Clone)]
pub(crate) struct BroadcastWeak(Weak<BroadcastInner>);

#[allow(dead_code)]
struct SenderCommand {
    uri: String,
    volume: f32,
}

// Actual broadcast server state
#[derive(Debug)]
pub struct BroadcastInner {
    pub pipeline: gst::Pipeline,

    #[allow(dead_code)]
    commands_tx: glib::Sender<SenderCommand>,
    
    mainmixer: mixer_bin::Mixer,
    pub streammixer: mixer_bin::Mixer,

    #[allow(dead_code)]
    silence: whitenoise::Silence,

    plylsts: Arc<Mutex<Vec<playlist::Playlist>>>,

    volumecontroller_mainmixer_spots: volume::Control,

    running_time: RwLock<gst::ClockTime>,
    current_spot: RwLock<Option<spots::Item>>,
}

// To be able to access the App's fields directly
impl std::ops::Deref for Broadcast {
    type Target = BroadcastInner;

    fn deref(&self) -> &BroadcastInner {
        &self.0
    }
}

impl BroadcastWeak {
    // Try upgrading a weak reference to a strong one
    fn upgrade(&self) -> Option<Broadcast> {
        self.0.upgrade().map(Broadcast)
    }
}


impl Broadcast {
    // Downgrade the strong reference to a weak reference
    fn downgrade(&self) -> BroadcastWeak {
        BroadcastWeak(Arc::downgrade(&self.0))
    }

    /// Creates the **Broadcast Server** to Send / Stream Audio. 
    /// 
    /// - Need to add Decoded Things to the adder Thing.. (more docs...)
    ///
    /// # Arguments
    ///
    /// * `server_ip` - the Address where the broadcaster ist listening for incoming clients
    /// * `tcp_port` - Port where the tcpserversink ist put out the stream
    ///
    pub fn new(
        server_ip: &str, 
        tcp_port: i32,
        rate: i32,
    ) -> Result<
        Self,
        anyhow::Error,
    > {


        debug!("init gstreamer");
        let _ = gst::init();
        gst_plugins::plugin_register_static()?;


        // Get a main context...
        let main_context = glib::MainContext::default();
        // ... and make it the main context by default so that we can then have a channel to send the
        // commands we received from the terminal.
        let _guard = main_context.acquire().unwrap();

        // Build the channel to get the terminal inputs from a different thread.
        let (commands_tx, _ready_rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);


        debug!("create pipeline, add adder as mixer, and audioconverter for preconvert");
        // create pipeline
        let pipeline = gst::Pipeline::new(None);
        
        // setup clock for synchronization
        /*
        let clock = gst::SystemClock::obtain();
        let clock_provider = gst_net::NetTimeProvider::new(&clock, None, clock_port);
        clock.set_property("clock-type", &gst::ClockType::Realtime)?;
        pipeline.use_clock(Some(&clock));
        */


        // setup audiomixer for broadcast schedule notifications or advertising
        let caps = gst::Caps::builder("audio/x-raw")
            .field("rate", &rate)
            .field("channels", &2i32)
            //.field("channel-mask", gst::Bitmask(0x0000000000000000))
            .build();
        debug!("create the mainmixer");
        let mainmixer = mixer_bin::Mixer::new("mainmixer", Some("adder"),Some(caps), true)?;
        //let mainmixer = mixer_bin::Mixer::new("mainmixer", Some("audiomixer"),None, false)?;
        mainmixer.add_to_pipeline(&pipeline)?;
            
        debug!("create the streammixer");
        let streammixer = mixer_bin::Mixer::new("streammixer", Some("audiomixer"), None,false)?;
        streammixer.add_to_pipeline(&pipeline)?;

        let silence = whitenoise::Silence::new()?;
        silence.add_to_pipeline(&pipeline)?;
        silence.attach_to_mixer(&streammixer)?;

        // Volume control for_ spot playback
        let volumecontroller_mainmixer_spots = volume::Control::new();


        let tcp_output = make_element("tcpserversink", Some("tcp_output"))?;
        tcp_output.try_set_property("host", &server_ip)?;
        tcp_output.try_set_property("port", &tcp_port)?;
        tcp_output.try_set_property("sync", &true)?;
        tcp_output.try_set_property("async", &false)?;

        pipeline.add(&tcp_output)?;

        // output of mainmixer goes to input of tcp_output
        debug!("link mainmixer src with tcp_output sink");
        mainmixer.link_pads(Some("src"), &tcp_output, Some("sink"))?;

        let bus = pipeline.bus().expect("Pipeline without bus should never happen");
        let _cmd_tx = commands_tx.clone();
        let broadcast = Broadcast(Arc::new(BroadcastInner {
            pipeline,
            commands_tx,

            mainmixer,
            streammixer,

            silence,

            plylsts: Arc::new(Mutex::new(Vec::new())),

            volumecontroller_mainmixer_spots,

            running_time: RwLock::new(gst::ClockTime::ZERO),
            current_spot: RwLock::new(None),
        }));


        
        let broadcast_weak = broadcast.downgrade();
        bus.set_sync_handler(move |_, msg| {
            use gst::MessageView;
            

            let _broadcast = match broadcast_weak.upgrade() {
                Some(broadcast) => broadcast,
                None => return gst::BusSyncReply::Pass,
            };
            //let pipeline = &broadcast.pipeline;

            match msg.view() {
                MessageView::Eos(..) => {
                    warn!("received eos");
                    // An EndOfStream event was sent to the pipeline, so we tell our main loop
                    // to stop execution here.
                }
                MessageView::Error(err) => {
                    warn!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                }
                _ => (),
            };
    
            gst::BusSyncReply::Pass
        });


        broadcast.add_streammixer()?;
    
        let broadcast_weak = broadcast.downgrade();
        glib::timeout_add(std::time::Duration::from_secs(5), move || {
            let _broadcast = match broadcast_weak.upgrade() {
                Some(broadcast) => broadcast,
                None => return Continue(true)
            };
            //broadcaster.print_graph();
            Continue(true)
        });

        let broadcast_weak = broadcast.downgrade();
        glib::timeout_add(std::time::Duration::from_millis(5000), move || {
            let broadcast = match broadcast_weak.upgrade() {
                Some(broadcast) => broadcast,
                None => return Continue(true)
            };

            // look for removeable items
            let mut can_remove = false;
            {
                let s = broadcast.current_spot.read().unwrap();
                if let Some(spot) = &*s {
                    if spot.state() == spots::ItemState::Removed {
                        can_remove = true;
                        spot.cleanup();
                    }
                } 
            }

            if can_remove == true {
                let mut s = broadcast.current_spot.write().unwrap();
                *s = None;
            }

            Continue(true)
        });
    


        Ok(
            broadcast
        )
    }

    /// ## Add a Mixer for Streamplayback
    ///
    fn add_streammixer(&self) -> Result<(), anyhow::Error> {
        
        if let Some((sinkpad, original_sinkpad)) = self.mainmixer.request_new_sink() { 

            let broadcast_clone = self.downgrade();
            original_sinkpad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
                let broadcast = upgrade_weak!(broadcast_clone, gst::PadProbeReturn::Pass);
                methods::pad_helper::running_time_method(pad, info, |clock| {
                    let mut w = broadcast.running_time.write().unwrap();
                    *w = *clock;
                    drop(w);
                })
            });

            self.volumecontroller_mainmixer_spots.attach_to(&original_sinkpad, "volume")?; 

            if let Some(srcpad) = self.streammixer.src_pad() {
                srcpad.link(&sinkpad)?;
            } else {
                warn!("could not get static srcpad from audiomixer_convert! - possible no output");
            }
        } else {
            warn!("could not get a requested sink_pad from mainmixer! - possible no output");
        }

        Ok(())
    }

    pub fn start(&self) -> Result<(), anyhow::Error> {
        self.pipeline.set_state(gst::State::Playing)?;

        Ok(())
    }

    pub fn pause(&self) -> Result<(), anyhow::Error> {
        self.pipeline.set_state(gst::State::Paused)?;

        Ok(())
    }

    pub fn stop(&self) -> Result<(), anyhow::Error> {
        self.pipeline.set_state(gst::State::Null)?;

        Ok(())
    }

    pub fn set_playlist(&self, list: Vec<&str>) -> Result<(), anyhow::Error> {
        
        info!("set the playlist: {:#?}", list);

        let mut p = self.plylsts.lock().unwrap();

        
        
        let plylst = if let Some(current_playlist) = p.pop() {
            let string_uris: Vec<String> = list.iter().map(|&s|s.into()).collect();
            info!("change the playlist");
            current_playlist.playlist.set_property("uris", string_uris );
            current_playlist.cleanup();
            current_playlist
        } else {
            playlist::Playlist::new(&self.pipeline, &self.streammixer, list)?
        };
        p.push(plylst);

    
        Ok(())
    }


    pub fn spot_is_running(&self) -> bool {
        let s = self.current_spot.read().unwrap();

        if let Some(spot) = &*s {
            if spot.state() != spots::ItemState::Removed && spot.state() != spots::ItemState::Eos {
                return true
            }
        } 

        return false
    }

    fn end_of_spot(&self, queue_size: u64) {
        let start_time = self.running_time.read().unwrap();
        let c = start_time.clone();
        drop(start_time);
        self.volumecontroller_mainmixer_spots.set_value(crate::MIN_VOLUME_BROADCAST, 1.0, gst::ClockTime::from_nseconds(queue_size), c);
    }

    // play a spot
    pub fn play_spot(&self, uri: &str, spot_volume: Option<f64>) -> Result<(), anyhow::Error> {

        info!("play a spot {}", uri);

        let mixer = &self.mainmixer;
        
        let broadcast_clone = self.downgrade();
        let item = self::spots::Item::new(uri, broadcast_clone)?;
        self.activate_item(&item, &mixer)?;
        
        let start_time = self.running_time.read().unwrap();
        let c = start_time.clone();
        drop(start_time);

        let crossfade_time_as_clock = crate::CROSSFADE_TIME_MS * gst::ClockTime::MSECOND;

        let _ = item.set_volume(spot_volume.unwrap_or(crate::MAX_VOLUME_SPOT));
        self.volumecontroller_mainmixer_spots.set_value(1.0, crate::MIN_VOLUME_BROADCAST, crossfade_time_as_clock, c);
        
        let s = c.nseconds() as i64;


        item.set_offset(s - (crossfade_time_as_clock.nseconds() as i64) / 2);

        // current spot needs to resist in memory for accessible by pad events
        let mut w = self.current_spot.write().unwrap();
        *w = Some(item);
        drop(w);

        Ok(())
    }


    fn put_item_to_mixer(&self, pad: &gst::Pad, mixer: &mixer_bin::Mixer) -> Result<gst::Pad, anyhow::Error> {
        
        if let Some((sinkpad, _)) = mixer.request_new_sink() { 
            // link audio_pad from item decoder to sinkpad of the mixer

            pad.link(&sinkpad)?;

            return Ok(sinkpad)
        }

        bail!("Couldnt add thing to mixer")
    }

    /// ## Activate Item
    /// 
    /// - item needs to prepared (usual items gets activatet on pushing to queue)
    /// - item needs to have a audipad, which get created on pushing to queue, where decoder_pad_added gets called when gstreamer
    ///   determine the pipeline to decode the item
    /// - gets the mixer from main pipeline and request a sink_pad and link both together
    /// - sets all callbacks (current runningtime over buffer event) & downstream event for checking eos on item
    /// - at least, if all was successfull, set state to Activate
    fn activate_item(&self, item: &spots::Item, mixer: &mixer_bin::Mixer) -> Result<(), anyhow::Error> {
        let mut retry_count = 0;
        let mut delay_between = 100;
        while item.state() != spots::ItemState::Prepared {
            
            retry_count += 1;
            if retry_count >= 10 && delay_between == 100 {
                retry_count = 0;
                delay_between = 1000;
            }
            if retry_count >= 10 && delay_between == 1000 {
                retry_count = 0;
                delay_between = 2000;
            }
            if retry_count >= 10 && delay_between == 2000 {
                bail!("try multiply time to run this item {}, but it doesnt work, i give up", item.uri);
            }
            sleep_ms!(delay_between);
        }
        if item.audio_pad().is_none() {
            bail!("Item has no AudioPad");
        }


        let audio_pad = item.audio_pad().unwrap();

        if let Ok(sinkpad) = self.put_item_to_mixer(&audio_pad, mixer) {
        
            debug!("link audio_pad from {} to mixer {}", item.uri, sinkpad.name());

            let item_clone = item.downgrade();
            // on every buffer pad event, set the running time for the item
            sinkpad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
                let item = upgrade_weak!(item_clone, gst::PadProbeReturn::Pass);
                item.pad_probe_running_time(pad, info)
            });


            let item_clone = item.downgrade();

            // when the mixer sink has a downstream event call pad_probe_going_eos for the item, where we check if the item has an eos
            
            
            sinkpad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |pad, probe_info| {
                let item = upgrade_weak!(item_clone, gst::PadProbeReturn::Pass);
                item.pad_probe_eos(pad, probe_info)
            });

            // REMOVE BLOCKING
            #[cfg(all(target_os = "macos"))]
            if !item.has_block_id() {
                warn!("Item has no Blocked Pad");
            } else {
                item.remove_block()?;
            }

            item.set_mixer_pad(sinkpad);
            item.set_state(spots::ItemState::Activate);
            return Ok(());    
        }

        
        bail!("Item couldnt get a mixer request sink pad")
    }


    pub fn print_graph(&self) {
        
        use std::path::Path;
        debug!("print graph");
        gst::debug_bin_to_dot_file_with_ts(
            &self.pipeline,
            gst::DebugGraphDetails::all(),
            Path::new("pipeline_micast_broadcaster.dot")
        );
        debug!("graph is printed");
    }

}
