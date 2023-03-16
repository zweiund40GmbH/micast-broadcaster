/// main work here
///
mod network;
mod builder;
mod mixer_bin;
mod local;

use std::sync::mpsc::Sender;

pub use builder::Builder;

use gst::prelude::*;
use gst::glib;

use crate::helpers::*;
use crate::sleep_ms;


use std::{
    sync::{Arc, Weak},
};

use parking_lot::Mutex;

use log::{debug, warn, info};

pub(crate) const ENCRYPTION_ENABLED:bool = true;


#[derive(Debug, Clone)]
pub enum OutputMode {
    Local(Option<String>),
    Network,
}

impl Default for OutputMode {
    fn default() -> Self {
        OutputMode::Local(None)
    }
}

// Strong reference to our broadcast server state
#[derive(Debug, Clone)]
pub struct Broadcast(Arc<BroadcastInner>);

// Weak reference to our broadcast server state
#[derive(Debug, Clone)]
pub(crate) struct BroadcastWeak(Weak<BroadcastInner>);

// Actual broadcast server state
#[derive(Debug)]
pub struct BroadcastInner {
    pub pipeline: gst::Pipeline,
    pub appsrc: gst_app::AppSrc,

    //current_spot: RwLock<Option<spots::Item>>,

    network_bin: gst::Element,
    local_bin: Mutex<Option<gst::Element>>,
    tee_bin: gst::Element,

    net_clock: Mutex<gst_net::NetTimeProvider>,

    rate: Option<i32>,

    current_output: Mutex<OutputMode>,

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
    /// * `server_ip` - the Address where the clock Server ist listening for incoming clients
    /// * `tcp_port` - Port where the tcpserversink ist put out the stream
    /// * `rate` - The Audio rate defaults top 44100
    /// * `clock_port` - the TCP Port for the Clock Server (default 8555)
    /// * `broadcast_ip` - the IP / Adress / BroadcastIP for the streaming RTP RTCP server
    /// * `spot_volume` - The Volume of a playing spot
    /// * `broadcast_volume` - the Volume of the running stream while Spot is playing
    /// * `crossfade_time` - the time while crossfading to a spot and from a spot to the stream
    ///
    pub fn new(
        server_ip: &str, 
        tcp_port: i32,
        rate: i32,
        clock_port: i32,
        broadcast_ip: Option<String>,
        current_output: OutputMode,
    ) -> Result<
        Self,
        anyhow::Error,
    > {



        debug!("init gstreamer audiorate: {}", rate);
        let _ = gst::init();

        let pipeline = gst::Pipeline::new(Some("pipe"));

        let maincaps = gst::Caps::builder("audio/x-raw")
            .field("format", &"F32LE")
            .field("rate", &44100i32)
            .field("channels", &2i32)
            .field("layout", &"interleaved")
            .build();
            
        let src = gst::ElementFactory::make_with_name("appsrc", None).unwrap();
        src.set_property("is-live", &true);
        src.set_property("block", &false);
        src.set_property("format", &gst::Format::Time);
        src.set_property("caps", &maincaps);

        let audioconvert = gst::ElementFactory::make_with_name("audioconvert", None).unwrap();
        //let audiosink = gst::ElementFactory::make("autoaudiosink", None).unwrap();
        //let queue = gst::ElementFactory::make("queue2", None).unwrap();
        //queue.set_property("max-size-time", &32000u64);

        pipeline.add_many(&[&src, &audioconvert]).unwrap();
        gst::Element::link_many(&[&src, &audioconvert]).unwrap();

        // setup clock for synchronization
        let clock = gst::SystemClock::obtain();
        debug!("add net clock server {} port {}", server_ip, clock_port);
        let net_clock = gst_net::NetTimeProvider::new(&clock, None, clock_port)?;
        clock.set_property("clock-type", &gst::ClockType::Realtime);

        pipeline.use_clock(Some(&clock));
        
        crate::services::clock_server::service()?;


        // global resample
        let mainresampler = make_element("audioresample", Some("mainresampler"))?;
        pipeline.add(&mainresampler)?;

        audioconvert.link(&mainresampler)?;


        let tee_bin = make_element("tee", Some("teebin"))?;
        pipeline.add(&tee_bin)?;
        mainresampler.link(&tee_bin)?;

        let network_bin = network::create_bin(
            tcp_port + 3, // rtcp_receiver_port
            tcp_port + 2, // rtcp_send_port
            tcp_port,     // rtp_send_port
            &broadcast_ip.unwrap_or(server_ip.to_string()),      // server_address
            None)?;

        let network_element: gst::Element = network_bin.upcast();
        let mut local_bin = None;

        match &current_output {
            OutputMode::Network => {
                pipeline.add(&network_element)?;
                debug!("link mainmixer src with tcp_output sink");
                tee_bin.link(&network_element)?;
            },
            OutputMode::Local(device) => {

                let local_output: gst::Element = local::create_bin(device.clone())?.upcast();
                pipeline.add(&local_output)?;
                tee_bin.link(&local_output)?;
                local_bin = Some(local_output);

            }
        };

        let bus = pipeline.bus().expect("Pipeline without bus should never happen");

        // set the spot settings

        let appsrc = src
            .dynamic_cast::<gst_app::AppSrc>()
            .expect("Source element is expected to be an appsrc!");

        //pipeline.set_base_time(gst::ClockTime::ZERO);
        //pipeline.set_start_time(gst::ClockTime::NONE);

        let broadcast = Broadcast(Arc::new(BroadcastInner {
            pipeline,
            appsrc,
            current_output: Mutex::new(current_output),
            network_bin: network_element,
            local_bin: Mutex::new(local_bin),
            tee_bin,
            net_clock: Mutex::new(net_clock),
            rate: Some(rate),
        }));


        
        let broadcast_weak = broadcast.downgrade();
        bus.add_signal_watch();

        bus.connect("message::error", false, move |v| {
            let broadcast = match broadcast_weak.upgrade() {
                Some(broadcast) => broadcast,
                None => return None
            };
            let err_msg = v[1].get::<gst::Message>().unwrap();
            let src = match err_msg.src().and_then(|s| s.clone().downcast::<gst::Element>().ok()) {
                None => {
                    warn!("could not handle error cause no element found");
                    return None;
                },
                Some(src) => src,
            };

            warn!("error from bus {:#?} -> {:#?}",err_msg,src);

            let weak_pipeline = broadcast.pipeline.downgrade();
            glib::timeout_add(std::time::Duration::from_secs(5), move || {
                
                let pipeline = match weak_pipeline.upgrade() {
                    Some(pipeline) => pipeline,
                    None => return Continue(true),
                };
                warn!("set pipeline to null and than to playing");
                let _ = pipeline.set_state(gst::State::Null);
                sleep_ms!(500);
                let _ = pipeline.set_state(gst::State::Playing);

                Continue(false)
            }); 

            None
        });


        Ok(
            broadcast
        )
    }

    /// # Change the IPs of the running system 
    ///
    /// paused the current playback and fast switch the ips in the pipeline elements
    /// 
    /// ## Arguments
    ///
    /// * `broadcast_ip` - the IP / Broadcast / Host Adress of this server
    /// * `clock_ip` - The IP Where the clock server should listen (can also be a broadcast IP)
    ///
    pub fn change_ips(&self, broadcast_ip: Option<&str>, clock_ip: Option<&str>) -> Result<(), anyhow::Error> {

        // rtp_udp_sink - host - network_rtp_sink
        // rtcp_udp_sink - host - network_rtcp_sink
        // rtcp_udp_src - address - network_rtcp_src
        
        let mut change_something = false;


        if let Some(broadcast_ip) = broadcast_ip {

            let rtp_udp_sink = self.pipeline.by_name("network_rtp_sink");
            if let Some(rtp_udp_sink) = rtp_udp_sink {
                let old_ip: String = rtp_udp_sink.property("host");
                if broadcast_ip != old_ip {
                    change_something = true;

                    let cloned_broadcast_ip = format!("{}", broadcast_ip.clone());
                    let pipeline = self.pipeline.clone();
                    glib::idle_add(move || {
                        let _ = pipeline.set_state(gst::State::Paused);

                        info!("change broadcast ip from {} to {}", old_ip, cloned_broadcast_ip);

                        let rtcp_udp_sink = pipeline.by_name("network_rtcp_sink").unwrap();
                        let rtcp_udp_src = pipeline.by_name("network_rtcp_src").unwrap();
                
                        let _ = rtp_udp_sink.set_property("host", &cloned_broadcast_ip);
                        let _ = rtcp_udp_sink.set_property("host", &cloned_broadcast_ip);
                        let _ = rtcp_udp_src.set_property("address", &cloned_broadcast_ip);                

                        Continue(false)
                    });
                }
            }
            

        }

        if change_something == true {
            let pipeline = self.pipeline.clone();
            glib::idle_add(move || {
                let _ = pipeline.set_state(gst::State::Playing);

                Continue(false)
            });

        }

        Ok(())
    }

    /// # start
    ///
    /// Starts the GStreamer Pipeline by simple update state to Playing
    /// 
    pub fn start(&self) -> Result<(), anyhow::Error> {
        self.pipeline.set_state(gst::State::Playing)?;
        Ok(())
    }


    /// # switch_output
    /// 
    /// can dynamically switch output while playing
    /// 
    pub fn switch_output(&self, new_output: OutputMode) -> Result<(), anyhow::Error> {

        let mut current_output = self.current_output.lock();
        match &new_output {
            OutputMode::Network => {
                if let OutputMode::Network = &*current_output {
                } else {

                    let weak_self = self.downgrade();
                    glib::idle_add(move || {
                        debug!("add network to pipeline");
                        let this = upgrade_weak!(weak_self, Continue(false));
                        this.pipeline.add(&this.network_bin).expect("can not add network_bin to pipeline");
                        let _ = this.network_bin.sync_state_with_parent().expect("cannot sync parents state of network bin");
                        let _ = this.tee_bin.link(&this.network_bin).expect("want to link network bin with tee, but doenst work");
                        debug!("tee bin linked with networkbin");
                        Continue(false)
                    });


                    // try remove current local 
                    let local_bin_lock = self.local_bin.lock();
                    if let Some(local_bin) = &*local_bin_lock {
                        let ghostpad = local_bin.static_pad("sink").unwrap();
                        let teepad = ghostpad.peer().unwrap();
                        let weak_self = self.downgrade();
                        let weak_local_bin = local_bin.downgrade();
                        let inner_teepad = teepad.clone();
                        debug!("add probe to remove local connection");
                        teepad.add_probe(gst::PadProbeType::BLOCK, move |pad, info| {
                            pad.remove_probe(info.id.take().unwrap());
                            let this = upgrade_weak!(weak_self, gst::PadProbeReturn::Remove);
                            let local_bin = upgrade_weak!(weak_local_bin, gst::PadProbeReturn::Remove);

                            debug!("set local bin state to null");
                            let _ = local_bin.set_state(gst::State::Null);
                            debug!("remove local bin from pipeline");
                            let _ = this.pipeline.remove(&local_bin);

                            debug!("remove local bin from tee");
                            let _ = this.tee_bin.release_request_pad(&inner_teepad);

                            gst::PadProbeReturn::Remove
                        });

                    }

                }
            },
            OutputMode::Local(ref device) => {

                if let OutputMode::Local(current_device) = &*current_output {
                    if device != current_device {
                        // currently we already stream to local output but currently the outputs are not the same
                        let local_bin_lock = self.local_bin.lock();
                        let cloned_local_bin_lock = local_bin_lock.clone();
                        drop(local_bin_lock);
                        if let Some(local_bin) = cloned_local_bin_lock {
                            let ghostpad = local_bin.static_pad("sink").unwrap();
                            let teepad = ghostpad.peer().unwrap();
                            let weak_self = self.downgrade();
                            let inner_teepad = teepad.clone();
                            let weak_local_bin = local_bin.downgrade();
                            let cloned_device = device.clone(); 

                            debug!("add probe to remove local connection and add new connection");
                            teepad.add_probe(gst::PadProbeType::BLOCK, move |pad, info| {
                                pad.remove_probe(info.id.take().unwrap());
                                let new_device = match &cloned_device {
                                    Some(d) => Some(d),
                                    None => None,
                                };
                                let this = upgrade_weak!(weak_self, gst::PadProbeReturn::Remove);
                                let local_bin = upgrade_weak!(weak_local_bin, gst::PadProbeReturn::Remove);

                                let local_output: gst::Element = local::create_bin(new_device).unwrap().upcast();
                                let _ = this.pipeline.add(&local_output);
                                let _ = local_output.sync_state_with_parent();
                                let _ = this.tee_bin.link(&local_output);

                                let mut local_bin_lock = this.local_bin.lock();
                                *local_bin_lock = Some(local_output);

                                let _ = local_bin.set_state(gst::State::Null);
                                let _ = this.pipeline.remove(&local_bin);
                                let _ = this.tee_bin.release_request_pad(&inner_teepad);

                                gst::PadProbeReturn::Remove
                            });

                        }

                    }

                } else {

                    debug!("add local connection");
                    let new_device = match &device {
                        Some(d) => Some(d),
                        None => None,
                    };
                    let local_output: gst::Element = local::create_bin(new_device).unwrap().upcast();

                    let weak_self = self.downgrade();
                    let cloned_local_output = local_output.clone();
                    glib::idle_add(move || {
                        let this = upgrade_weak!(weak_self, Continue(false));
                        let _ = this.pipeline.add(&cloned_local_output);
                        let _ = cloned_local_output.sync_state_with_parent();
                        let _ = this.tee_bin.link(&cloned_local_output);
                        Continue(false)
                    });

                    debug!("add local connection to class");
                    let mut local_bin_lock = self.local_bin.lock();
                    *local_bin_lock = Some(local_output);

                    // we are currently stream to network and want to change to the local output
                    debug!("remove network connection");
                    let ghostpad = self.network_bin.static_pad("sink").unwrap();
                    let teepad = ghostpad.peer().unwrap();
                    let weak_self = self.downgrade();
                    let inner_teepad = teepad.clone();


                    debug!("add probe to remove network connection");
                    teepad.add_probe(gst::PadProbeType::BLOCK, move |pad, info| {
                        pad.remove_probe(info.id.take().unwrap());
                        let this = upgrade_weak!(weak_self, gst::PadProbeReturn::Remove);

                        debug!("set state of network output to null");
                        let _ = this.network_bin.set_state(gst::State::Null);
                        debug!("remove network output from pipeline and from tee");
                        let _ = this.pipeline.remove(&this.network_bin);
                        let _ = this.tee_bin.release_request_pad(&inner_teepad);

                        gst::PadProbeReturn::Remove
                    });
                    
                }

            }
        };

        *current_output = new_output.clone();

        Ok(())
    }

    /// # pause
    ///
    /// Pause the Gstreamer Pipeline
    /// 
    pub fn pause(&self) -> Result<(), anyhow::Error> {
        self.pipeline.set_state(gst::State::Paused)?;

        Ok(())
    }

    /// # stop
    ///
    /// Stops the Gstreamer Pipeline by set state to Null
    /// 
    pub fn stop(&self) -> Result<(), anyhow::Error> {
        self.pipeline.set_state(gst::State::Null)?;

        Ok(())
    }




}
