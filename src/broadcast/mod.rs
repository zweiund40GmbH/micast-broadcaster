
mod network;
mod item;
mod queue;
mod builder;

pub use builder::Builder;
use queue::Queue;
use item::{Item, ItemState};

use gstreamer as gst;
use gstreamer_net as gst_net;
use gstreamer_controller as gst_controller;

use crate::helpers::*;
use crate::sleep_ms;

use std::sync::{Arc, Weak, RwLock};
use gst::prelude::*;
use gst_controller::prelude::*;


use anyhow::{bail, anyhow};

use log::{debug, warn, info};

// Strong reference to our broadcast server state
#[derive(Debug, Clone)]
pub struct Broadcast(Arc<BroadcastInner>);

// Weak reference to our broadcast server state
#[derive(Debug, Clone)]
pub(crate) struct BroadcastWeak(Weak<BroadcastInner>);


struct SenderCommand {
    uri: String,
    volume: f32,
}



// Actual broadcast server state
#[derive(Debug)]
pub struct BroadcastInner {
    pipeline: gst::Pipeline,
    //audio_mixer: gst::Element,
    //#[allow(unused_parens, dead_code)]
    //clock_provider: gst_net::NetTimeProvider,
    playback_queue: Queue,

    //network_bin: gst::Bin,

    commands_tx: glib::Sender<SenderCommand>,
    mainmixer: gst::Element,
    running_time: RwLock<gst::ClockTime>,
    current_spot: RwLock<Option<item::Item>>,
    volumecontroller_audiomixer_stream: gst_controller::InterpolationControlSource,
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
        /*
        rtp_sender_port: i32, 
        rtcp_sender_port: i32, 
        rtcp_receive_port: i32, 
        clock_port:i32, 
        multicast_interface: Option<String>,
        */
    ) -> Result<
        Self,
        anyhow::Error,
    > {


        debug!("init gstreamer");
        let _ = gst::init();


        // Get a main context...
        let main_context = glib::MainContext::default();
        // ... and make it the main context by default so that we can then have a channel to send the
        // commands we received from the terminal.
        let _guard = main_context.acquire().unwrap();

        // Build the channel to get the terminal inputs from a different thread.
        let (commands_tx, ready_rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);


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
        let mainmixer = make_element("audiomixer", Some("main_mixer"))?;
        let mainmixer_converter = make_element("audioconvert", Some("main_converter"))?;
        let audiomixer_queue = make_element("queue", Some("audiomixer_queue"))?;

        let capsfilter = make_element("capsfilter", Some("mainmixer_capsfilter"))?;
        let caps = gst::Caps::builder("audio/x-raw")
            .field("rate", &rate)
            .field("channels", &2i32)
            //.field("rate", &44100i32)
            .build();
        capsfilter.set_property("caps", &caps).unwrap();  
        pipeline.add(&capsfilter)?;

        pipeline.add(&mainmixer)?;
        pipeline.add(&mainmixer_converter)?;
        pipeline.add(&audiomixer_queue)?;
        gst::Element::link_many(&[&mainmixer, &mainmixer_converter])?;
        gst::Element::link_many(&[&mainmixer_converter, &capsfilter, &audiomixer_queue])?;

        mainmixer.sync_state_with_parent()?;
        mainmixer_converter.sync_state_with_parent()?;
        audiomixer_queue.sync_state_with_parent()?;

        // setup the audio mixer as input for Pipeline
        let audiomixer = make_element("adder", Some("mixer"))?;
        let audiomixer_convert = make_element("audioconvert", Some("adder_audioconverter"))?;

        // -- add mixer and converter to element
        pipeline.add(&audiomixer)?;
        pipeline.add(&audiomixer_convert)?;
        
        // -- linkt this elements
        gst::Element::link_many(&[&audiomixer,&audiomixer_convert])?;

        audiomixer.sync_state_with_parent()?;
        audiomixer_convert.sync_state_with_parent()?;

        // Volume control for_ spot playback
        let volumecontroller_audiomixer_stream = gst_controller::InterpolationControlSource::new();
        volumecontroller_audiomixer_stream.set_mode(gst_controller::InterpolationMode::Linear);

        // create network things
        /*
        let sender_bin = self::network::create_bin(
            rtcp_receive_port, 
            rtcp_sender_port, 
            rtp_sender_port,
            server_ip,
            multicast_interface,
        )?;
        */


        //let tcp_queue = make_element("queue", None)?;
        //pipeline.add(&tcp_queue)?;
        let tcp_output = make_element("tcpserversink", Some("tcp_output"))?;
        tcp_output.set_property("host", &server_ip)?;
        tcp_output.set_property("port", &3333)?;
        tcp_output.set_property("sync", &true)?;
        tcp_output.set_property("async", &false)?;

        pipeline.add(&tcp_output)?;

        

        audiomixer_queue.link_pads(Some("src"), &tcp_output, Some("sink"))?;

        // downgrade pipeline for ready_rx receiver for sendercommands
        let pipeline_weak = pipeline.downgrade();

        let bus = pipeline.bus().expect("Pipeline without bus should never happen");

        let broadcast = Broadcast(Arc::new(BroadcastInner {
            pipeline,
            //clock_provider,
            //network_bin: sender_bin,
            playback_queue: Queue::new(),
            commands_tx,
            volumecontroller_audiomixer_stream: volumecontroller_audiomixer_stream.clone(),
            mainmixer: mainmixer.clone(),
            running_time: RwLock::new(gst::ClockTime::ZERO),
            current_spot: RwLock::new(None),

        }));

        
        let broadcast_weak = broadcast.downgrade();
        //bus.add_signal_watch();
        bus.set_sync_handler(move |_, msg| {
            use gst::MessageView;
            
            let broadcast = match broadcast_weak.upgrade() {
                Some(broadcast) => broadcast,
                None => return gst::BusSyncReply::Pass,
            };

            //info!("msg received: {:?}", msg);

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

                    warn!("error throwing object is: {:?}", err.src());
                    if let Some(obj) = err.src() {
                        // try to convert object to element
                       
                        warn!("convert obj to element: {:?}", obj);

                        broadcast.playback_queue.search_for_element(&obj);
                        
                    }

                    // currently we need to panic here.
                    // the program who use this lib, would then automatically restart.
                    // the main problem is that the pipeline stops streaming audio over rtp if any element got an error, also if we restart the pipeline (meaning: set state to stopped, and the to play)
                    warn!("got an error, quit here");
                    //let _ = broadcast.pipeline.set_state(gst::State::Paused);
                    //let _ = broadcast.pipeline.set_state(gst::State::Playing);
                }
                _ => (),
            };
    
            // Tell the mainloop to continue executing this callback.
            // glib::Continue(true)
            gst::BusSyncReply::Pass
        });
        //.expect("Failed to add bus watch");

        // request pad for adding the audiomixer_convert to mainmixer
        if let Some(sinkpad) = mainmixer.request_pad_simple("sink_%u") { 
            // add a runningtime_probe........
            //gst_pad_add_probe (sinkpad, GST_PAD_PROBE_TYPE_BUFFER,
            //    (GstPadProbeCallback) crossfade_item_pad_probe_running_time, item, NULL);
            let broadcast_clone = broadcast.downgrade();
            sinkpad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
                let broadcast = upgrade_weak!(broadcast_clone, gst::PadProbeReturn::Pass);
                if let Some(event) = pad.sticky_event(gst::EventType::Segment, 0) { 
                    if let Some(data) = &info.data {
                        if let gst::PadProbeData::Buffer(buffer) = data {
                            if let gst::EventView::Segment(segment) = event.view() {
                                match segment.segment().to_running_time(buffer.pts().unwrap()) {
                                    gst::GenericFormattedValue::Time(Some(clock)) => {
                                        //debug!("sets main mixer running_time: {}", clock);
                                        let mut w = broadcast.running_time.write().unwrap();
                                        *w = clock;
                                        drop(w);
                                    },
                                    _ => {}
                                }
                            }
                        }
                    }
                }
        
                gst::PadProbeReturn::Pass

            });
            
            debug!("get sinkpad volume: {:?}", sinkpad.property("volume"));
            let dcb = gst_controller::DirectControlBinding::new_absolute(&sinkpad, "volume", &volumecontroller_audiomixer_stream);
            sinkpad.add_control_binding(&dcb)?;



            if let Some(srcpad) = audiomixer_convert.static_pad("src") {
                srcpad.link(&sinkpad)?;
            } else {
                warn!("could not get static srcpad from audiomixer_convert! - possible no output");
            }
        } else {
            warn!("could not get a requested sink_pad from mainmixer! - possible no output");
        }
        

        ready_rx.attach(Some(&main_context), move |cmd: SenderCommand| {
            let pipeline = match pipeline_weak.upgrade() {
                Some(pipeline) => pipeline,
                None => return glib::Continue(true),
            };

            // receive command
            debug!("helloworld: {}", cmd.uri);
    
            glib::Continue(true)
        });


        Ok(
            broadcast
        )
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

    /// Schedule Next Item
    /// 
    /// sets a new uri element for playback
    pub fn schedule_next(&self, uri: &str, state: ScheduleState, fixed_length: Option<u32>) -> Result<(), anyhow::Error> {
        match state {
            ScheduleState::AfterCurrent => {

            },
            ScheduleState::Now => {

            },
            ScheduleState::Interrupt => {

            }
        }

        debug!("add item to schedule");
        // create an item to hold all required Informations pad's etc
        let broadcast_clone = self.downgrade();
        let item = self::item::Item::new(uri, broadcast_clone, fixed_length, false)?;
        let current = self.playback_queue.current();
        if current.is_none() {
            debug!("queue got nothing, so activate next item");
            self.activate_item(&item, &self.pipeline.by_name("mixer").unwrap())?;
        }

        self.playback_queue.push(item);
        
        drop(current);

        Ok(())
        
    }

    pub fn push_silence(&self) -> Result<(), anyhow::Error> {
        let broadcast_clone = self.downgrade();
        let silence = item::Item::new_silence(broadcast_clone)?;

        self.activate_item(&silence, &self.pipeline.by_name("mixer").unwrap())?;
        self.playback_queue.push(silence);
        Ok(())
    }

    pub fn spot_is_running(&self) -> bool {
        let s = self.current_spot.read().unwrap();

        if let Some(spot) = &*s {
            if spot.state() != item::ItemState::Removed && spot.state() != item::ItemState::Eos {
                return true
            }
        } 

        return false
    }

    fn end_of_spot(&self, queue_size: u64) {

        debug!("end of spot... get louder!");
        let start_time = self.running_time.read().unwrap();
        let c = start_time.clone();
        drop(start_time);

        let a = self.volumecontroller_audiomixer_stream.clone().upcast::<gst_controller::TimedValueControlSource>();
        a.set(c, crate::MIN_VOLUME_BROADCAST);
 
        a.set(c + gst::ClockTime::from_nseconds(queue_size), 1.0);


    }

    // play a spot
    pub fn play_spot(&self, uri: &str) -> Result<(), anyhow::Error> {
  

        let mixer = &self.mainmixer;
        
        let a = self.volumecontroller_audiomixer_stream.clone().upcast::<gst_controller::TimedValueControlSource>();
        
        let broadcast_clone = self.downgrade();
        let item = self::item::Item::new(uri, broadcast_clone, None, true)?;
        self.activate_item(&item, &mixer)?;
        
        
        let start_time = self.running_time.read().unwrap();
        let c = start_time.clone();
        drop(start_time);

        let crossfade_time_as_clock = crate::CROSSFADE_TIME_MS * gst::ClockTime::MSECOND;

        let _ = item.set_volume(crate::MAX_VOLUME_SPOT);

        a.set(c, 1.0);
        a.set(c + crossfade_time_as_clock, crate::MIN_VOLUME_BROADCAST);
        let s = c.nseconds() as i64;

        item.set_offset(s - (crossfade_time_as_clock.nseconds() as i64) / 2);

        // current spot needs to resist in memory for accessible by pad events
        let mut w = self.current_spot.write().unwrap();
        *w = Some(item);
        drop(w);

        Ok(())
    }

    
    /// ## Schedule Crossfade on EOS to next item in queue
    /// 
    /// get current **item** get playback time,
    /// pop next item from queue, prepare and set as next item
    /// 
    /// * `item` - current playing item that are  short before EOS
    fn schedule_crossfade(&self, item: &item::Item, queue_size: u64) {

        debug!("schedule_crossfade is triggered, so item {} is going eos, we start with next item", item.uri);
        let next_item_result = self.activate_next_item();
        if let Err(e) = next_item_result {
            warn!("could not activate next item cause of {}", e);
        } else {
            debug!(" set offset for next item");
            let next_item = next_item_result.unwrap();
            next_item.set_offset(queue_size as i64);
        }

        debug!("end of schedule crossfade");
    }

    pub fn early_crossfade(&self) {
        debug!("make early_crossfade");
        let current = self.playback_queue.current();

        debug!("for early_crossfade we wait if a next item is prepared");
        let next = self.playback_queue.next();

        if let Some(current) = current {
            if next.is_some() {
                debug!("trigger prepare crossfade:");
                current.prepare_for_early_end();
            } else {
                debug!("could not make early_crossfade, no next items");
            }
        }


        
    }

    /// ## Activate next item from queue
    /// 
    /// - pop item from queue,
    /// - call activate
    fn activate_next_item(&self) -> Result<Arc<item::Item>, anyhow::Error> {

        self.playback_queue.clean();

        // get item from queue
        let next = self.playback_queue.next();

        if let Some(next) = next {
            // try to activate item from queue
            self.activate_item(&next, &self.pipeline.by_name("mixer").unwrap())?;

            return Ok(next)
        }
        bail!("no next item found")
    }


    fn put_item_to_mixer(&self, pad: &gst::Pad, mixer: &gst::Element) -> Result<gst::Pad, anyhow::Error> {
        
        if let Some(sinkpad) = mixer.request_pad_simple("sink_%u") { 
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
    fn activate_item(&self, item: &item::Item, mixer: &gst::Element) -> Result<(), anyhow::Error> {

        let mut retry_count = 0;
        while item.state() != item::ItemState::Prepared {
            
            retry_count += 1;
            debug!("retry prepared {} time / times 50 ms", retry_count);
            if retry_count >= 20 {
                bail!("after retry multiple times: Item is not Prepared");
            }
            sleep_ms!(100);
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

            
            
            debug!("set state to activate");
            
            item.set_state(item::ItemState::Activate);
            return Ok(());    
        }

        
        bail!("Item couldnt get a mixer request sink pad")
    }

}


/// Not implemented yet
pub enum ScheduleState {
    AfterCurrent, // Nach dem aktuellen Song
    Now, // Jetzt Sofort
    Interrupt, // kurze Unterbrechung
}
