/// main work here
///
mod network;
mod spots;
mod builder;
mod mixer_bin;
mod volume;
mod whitenoise;
mod methods;
mod fallback;

pub use builder::Builder;

use gst::prelude::*;
use gst::glib;
use chrono::prelude::*;

use crate::helpers::*;
use crate::sleep_ms;


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

#[derive(Debug, Default)]
struct SchedulerState {
    run_id: Mutex<Option<glib::SourceId>>,
    scheduler: Mutex<Option<crate::Scheduler>>,
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
    
    fallback: fallback::Fallback,

    volumecontroller_mainmixer_spots: volume::Control,

    running_time: RwLock<gst::ClockTime>,
    current_spot: RwLock<Option<spots::Item>>,

    net_clock: Mutex<gst_net::NetTimeProvider>,

    rate: Option<i32>,

    scheduler: SchedulerState,
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
        clock_port: i32,
        broadcast_ip: Option<String>,
    ) -> Result<
        Self,
        anyhow::Error,
    > {



        debug!("init gstreamer audiorate: {}", rate);
        let _ = gst::init();

        let default_caps = gst::Caps::builder("audio/x-raw")
            .field("rate", &rate)
            .field("channels", &2i32)
            .build();

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
        
        let clock = gst::SystemClock::obtain();
        let net_clock = gst_net::NetTimeProvider::new(&clock, None, clock_port);
        clock.set_property("clock-type", &gst::ClockType::Realtime);
        pipeline.use_clock(Some(&clock));
        


        // setup audiomixer for broadcast schedule notifications or advertising

        debug!("create the mainmixer");
        let mainmixer = mixer_bin::Mixer::new("mainmixer", Some("adder"),Some(default_caps.clone()), true)?;
        //let mainmixer = mixer_bin::Mixer::new("mainmixer", Some("audiomixer"),None, false)?;
        mainmixer.add_to_pipeline(&pipeline)?;
            
        debug!("create the streammixer");

        let streammixer = mixer_bin::Mixer::new("streammixer", Some("audiomixer"), Some(default_caps.clone()),false)?;
        streammixer.add_to_pipeline(&pipeline)?;

        let fallback_helper = fallback::Fallback::new(&pipeline, &streammixer)?;

        let silence = whitenoise::Silence::new(rate)?;
        silence.add_to_pipeline(&pipeline)?;
        silence.attach_to_mixer(&streammixer)?;

        

        // Volume control for_ spot playback
        let volumecontroller_mainmixer_spots = volume::Control::new();


        

        // global resample
        let mainresampler = make_element("audioresample", Some("mainresampler"))?;
        pipeline.add(&mainresampler)?;

        let maincapfilter_caps = gst::Caps::builder("audio/x-raw")
            .field("rate", &44100i32)
            .field("channels", &2i32)
            .build();
        let maincapfilter = make_element("capsfilter", Some("maincapsfilter"))?;
        maincapfilter.try_set_property("caps", &maincapfilter_caps)?;
        pipeline.add(&maincapfilter)?;

        mainmixer.link_pads(Some("src"), &mainresampler, Some("sink"))?;
        mainresampler.link_pads(Some("src"), &maincapfilter, Some("sink"))?;



        // here we change tcp_output to rtpbin
        //let tcp_output = make_element("tcpserversink", Some("tcp_output"))?;
        //tcp_output.try_set_property("host", &server_ip)?;
        //tcp_output.try_set_property("port", &tcp_port)?;
        //tcp_output.try_set_property("sync", &true)?;
        //tcp_output.try_set_property("async", &false)?;

        let network_bin = network::create_bin(
            tcp_port + 3, // rtcp_receiver_port
            tcp_port + 2, // rtcp_send_port
            tcp_port,     // rtp_send_port
            &broadcast_ip.unwrap_or(server_ip.to_string()),      // server_address
            None)?;

        let network_element: gst::Element = network_bin.upcast();
        //pipeline.add(&tcp_output);
        pipeline.add(&network_element)?;

        // output of mainmixer goes to input of tcp_output
        debug!("link mainmixer src with tcp_output sink");
        //maincapfilter.link_pads(Some("src"), &tcp_output, Some("sink"))?;
        maincapfilter.link_pads(Some("src"), &network_element, Some("sink")).expect("error on linking maincapfilter to network_bin");




        let bus = pipeline.bus().expect("Pipeline without bus should never happen");
        let _cmd_tx = commands_tx.clone();


        let broadcast = Broadcast(Arc::new(BroadcastInner {
            pipeline,
            commands_tx,

            mainmixer,
            streammixer,

            silence,

            fallback: fallback_helper,
            scheduler: SchedulerState::default(),

            volumecontroller_mainmixer_spots,

            net_clock: Mutex::new(net_clock),

            running_time: RwLock::new(gst::ClockTime::ZERO),
            current_spot: RwLock::new(None),
            rate: Some(rate),
        }));


        
        let broadcast_weak = broadcast.downgrade();
        bus.set_sync_handler(move |_, msg| {
            use gst::MessageView;
            

            let broadcast = match broadcast_weak.upgrade() {
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
                    let src = match err.src().and_then(|s| s.downcast::<gst::Element>().ok()) {
                        None => {
                            warn!("could not handle error cause no element found");
                            return gst::BusSyncReply::Pass;
                        },
                        Some(src) => src,
                    };
                    warn!("error comes from: {:?}", src.name());

                    if src.has_as_ancestor(&broadcast.fallback.bin) {
                        warn!("error comes from fallback");
                        let _ = broadcast.fallback.handle_error();

                        
                    }


                }
                _ => (),
            };
    
            gst::BusSyncReply::Pass
        });

        broadcast.add_streammixer()?;
    
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
    

        let broadcast_weak = broadcast.downgrade();
        glib::timeout_add(std::time::Duration::from_secs(10), move || {
            let broadcast = match broadcast_weak.upgrade() {
                Some(broadcast) => broadcast,
                None => return Continue(false)
            };

            if let Err(e) = broadcast.change_ips(Some("224.1.1.20"), None) {
                warn!("could not change ip {}", e);
            }

            Continue(false)
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

    pub fn change_ips(&self, broadcast_ip: Option<&str>, clock_ip: Option<&str>) -> Result<(), anyhow::Error> {

        
        // rtp_udp_sink - host - network_rtp_sink
        // rtcp_udp_sink - host - network_rtcp_sink
        // rtcp_udp_src - address - network_rtcp_src
        self.pipeline.set_state(gst::State::Paused)?;


        if let Some(broadcast_ip) = broadcast_ip {
            let rtp_udp_sink = self.pipeline.by_name("network_rtp_sink").unwrap();
            let old_ip: String = rtp_udp_sink.property("host");

            info!("change broadcast ip from {} to {}", old_ip, broadcast_ip);

            let rtcp_udp_sink = self.pipeline.by_name("network_rtcp_sink").unwrap();
            let rtcp_udp_src = self.pipeline.by_name("network_rtcp_src").unwrap();
    
            rtp_udp_sink.try_set_property("host", broadcast_ip)?;
            rtcp_udp_sink.try_set_property("host", broadcast_ip)?;
            rtcp_udp_src.try_set_property("address", broadcast_ip)?;
        }

        if let Some(clock_ip) = clock_ip {
            let mut net_clock = self.net_clock.lock().unwrap();
            let old_ip: String = net_clock.address().unwrap().to_string();
            let port: i32 = net_clock.port();
            let clock = net_clock.clock().unwrap();
            
            info!("change clock ip from {} to {}", old_ip, clock_ip);

            let new_net_clock = gst_net::NetTimeProvider::new(&clock, Some(clock_ip), port);
            *net_clock = new_net_clock;
            self.pipeline.use_clock(Some(&clock));
            drop(net_clock);
        }
        
       

        self.pipeline.set_state(gst::State::Playing)?;

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

    pub fn play(&self, uri: &str) -> Result<(), anyhow::Error> {
        info!("start playing: {}", uri);
        self.fallback.start(Some(uri))?;
        Ok(())
    }

    pub fn set_scheduler(&self, scheduler: crate::Scheduler) {
        let state = &self.scheduler;

        let mut run_id = state.run_id.lock().unwrap();

        if let Some(id) = run_id.take() {
            id.remove();
            *run_id = None;
        }

        drop(run_id);

        let mut scheduler_guard = state.scheduler.lock().unwrap();
        *scheduler_guard = Some(scheduler);
        drop(scheduler_guard);

        self.spot_runner();

    }

    fn spot_runner(&self) {
        //let self_weak = self.downgrade();
        let broadcast_clone = self.downgrade();
        let id = glib::timeout_add(std::time::Duration::from_millis(5000), move || {
            let broadcast = upgrade_weak!(broadcast_clone, Continue(true));

            let mut scheduler_guard = broadcast.scheduler.scheduler.lock().unwrap();
            let deref_scheduler = scheduler_guard.take();
            if let Some(mut scheduler) = deref_scheduler {
                if !broadcast.spot_is_running() {
                    if let Ok(spot) = scheduler.next(Local::now()) {
                        if let Err(e) = broadcast.play_spot(&spot.uri, Some(0.8)) {
                            warn!("error on play next spot... {:?}", e);
                        }
                    }
                }
                *scheduler_guard = Some(scheduler);
            } else {
                warn!("no scheduler in broadcaster found");
            }
            
            drop(scheduler_guard);
            
            Continue(true)
        });

        let mut run_id = self.scheduler.run_id.lock().unwrap();
        *run_id = Some(id);
        drop(run_id);
    }

    fn spot_is_running(&self) -> bool {
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
    fn play_spot(&self, uri: &str, spot_volume: Option<f64>) -> Result<(), anyhow::Error> {

        info!("play a spot {}", uri);

        let mixer = &self.mainmixer;
        
        let broadcast_clone = self.downgrade();
        let item = self::spots::Item::new(uri, broadcast_clone, self.rate)?;
        self.activate_item(&item, &mixer)?;
        
        let start_time = self.running_time.read().unwrap();
        warn!("what is the current running time? {:?}", start_time);
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
            //#[cfg(all(target_os = "macos"))]
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
