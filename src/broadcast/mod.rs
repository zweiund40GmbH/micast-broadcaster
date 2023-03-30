/// main work here
mod local;

use gst::prelude::*;
use gst::glib;

use crate::helpers::*;
use crate::sleep_ms;
use crate::services::dedector_server;
use crate::rtpserver;

use std::{
    sync::{Arc, Weak},
};

use parking_lot::Mutex;

use log::{debug, warn, trace};

//pub(crate) const ENCRYPTION_ENABLED:bool = true;


#[derive(Debug, Clone, PartialEq)]
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
    #[allow(dead_code)]
    net_clock: gst_net::NetTimeProvider,

    rtpserver: Mutex<Option<rtpserver::RTPServer>>,
    local_bin: Mutex<Option<gst::Element>>,
    tee_bin: gst::Element,

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
    /// # Arguments
    ///
    /// * `server_address` - the Address where the clock Server ist listening for incoming clients
    /// * `service_port` - the Port where the clock Server ist listening for incoming clients
    /// * `current_output` - current output device
    ///
    pub fn new(
        start_port: u32,
        current_output: OutputMode,
    ) -> Result<
        Self,
        anyhow::Error,
    > {
        let _ = gst::init();

        // setup and init NetTime Provider (aka NTPServer)
        let clock = gst::SystemClock::obtain();
        let net_clock = gst_net::NetTimeProvider::new(&clock, None, 8555)?;
        clock.set_property("clock-type", &gst::ClockType::Realtime);

        let pipeline = gst::Pipeline::new(None);
        pipeline.use_clock(Some(&clock));
        
        // add ip broadcaster (currently wrong name, not only for clock although for server address)
        dedector_server::service(start_port)?;

        // caps for AppSrc element from rodio
        let maincaps = gst::Caps::builder("audio/x-raw")
            .field("format", &"F32LE")
            .field("rate", &48000i32)
            .field("channels", &2i32)
            .field("layout", &"interleaved")
            .build();
            
        let src = gst::ElementFactory::make_with_name("appsrc", None).unwrap();
            src.set_property("is-live", &true);
            src.set_property("block", &false);
            src.set_property("format", &gst::Format::Time);
            src.set_property("caps", &maincaps);

        let audioconvert = gst::ElementFactory::make_with_name("audioconvert", None).unwrap();

        pipeline.add_many(&[&src, &audioconvert]).unwrap();
        gst::Element::link_many(&[&src, &audioconvert]).unwrap();


        let mainresampler = make_element("audioresample", Some("mainresampler"))?;
        pipeline.add(&mainresampler)?;
        audioconvert.link(&mainresampler)?;

        // the pipeline at this point looks like this:
        // appsrc -> audioconvert -> audioresample -> tee   -> tcp_output
        //                                                  -> local_output
        let tee_bin = make_element("tee", Some("teebin"))?;
        pipeline.add(&tee_bin)?;
        mainresampler.link(&tee_bin)?;

        let local_rtpserver = rtpserver::RTPServer::new(true, true)?;

        let mut rtpserver: Option<rtpserver::RTPServer> = Some(local_rtpserver.clone());
        // set listening addresses... 
        // later set also to switch output
        //local_rtpserver.add_client((server_address, start_port))?;
        local_rtpserver.add_client(("127.0.0.1", start_port))?;
        local_rtpserver.set_listen_for_rtcp_packets(start_port as i32 + 2)?;
        local_rtpserver.check_clients();



        let mut local_bin = None;

        match &current_output {
            OutputMode::Network => {
                debug!("starting in network mode, connect pipeline to rtpserver and link with tee_bin");
                pipeline.add(&local_rtpserver.get_element())?;
                tee_bin.link(&local_rtpserver.get_element())?;
                rtpserver = Some(local_rtpserver);
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

        pipeline.set_base_time(gst::ClockTime::ZERO);
        pipeline.set_start_time(gst::ClockTime::NONE);

        let broadcast = Broadcast(Arc::new(BroadcastInner {
            pipeline,
            appsrc,
            current_output: Mutex::new(current_output),
            rtpserver: Mutex::new(rtpserver),
            local_bin: Mutex::new(local_bin),
            tee_bin,
            net_clock,
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
                // always reset base and start time on restart
                pipeline.set_base_time(gst::ClockTime::ZERO);
                pipeline.set_start_time(gst::ClockTime::NONE);

                sleep_ms!(500);
                let _ = pipeline.set_state(gst::State::Playing);

                Continue(false)
            }); 

            None
        });

        let weak_pipeline = broadcast.pipeline.downgrade();
        glib::timeout_add(std::time::Duration::from_secs(5), move || {
            let pipeline = match weak_pipeline.upgrade() {
                Some(pipeline) => pipeline,
                None => return Continue(true),
            };
            let state = pipeline.state(gst::ClockTime::from_mseconds(1000));
            debug!("CURRENT PIPELINESTATE: {:?}",state);

            Continue(true)
        }); 

        

        Ok(
            broadcast
        )
    }

    /// # start
    ///
    /// Starts the GStreamer Pipeline by simple update state to Playing
    /// start rtspserver if current_output is Network
    /// 
    pub fn start(&self) -> Result<(), anyhow::Error> {
        // realy important reset start and base time before playing
        self.pipeline.set_base_time(gst::ClockTime::ZERO);
        self.pipeline.set_start_time(gst::ClockTime::NONE);

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

                if *current_output == new_output {
                    debug!("current output is already network");
                    return Ok(());
                }

                debug!("switch to network mode");
                self.attach_rtpserver();

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

                        let _ = local_bin.set_state(gst::State::Null);
                        let _ = this.pipeline.remove(&local_bin);
                        let _ = this.tee_bin.release_request_pad(&inner_teepad);

                        gst::PadProbeReturn::Remove
                    });

                }
            },
            OutputMode::Local(ref device) => {
                if let OutputMode::Local(current_device) = &*current_output {
                    // if device not the same as current_device, first remove the local output
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
                    //
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
                    if let Some(element) = self.pipeline.by_name("RTPServer0") {
                        let ghostpad = element.static_pad("sink").unwrap();
                        let teepad = ghostpad.peer().unwrap();
                        let weak_self = self.downgrade();
                        let inner_teepad = teepad.clone();
    
                        let weak_element = element.downgrade();
                        trace!("add probe to remove network connection");
                        teepad.add_probe(gst::PadProbeType::BLOCK, move |pad, info| {
                            pad.remove_probe(info.id.take().unwrap());
                            let this = upgrade_weak!(weak_self, gst::PadProbeReturn::Remove);
                            let element = upgrade_weak!(weak_element, gst::PadProbeReturn::Remove);
                            let _ = element.set_state(gst::State::Null);
                            let _ = this.pipeline.remove(&element);
                            let _ = this.tee_bin.release_request_pad(&inner_teepad);
    
                            gst::PadProbeReturn::Remove
                        });
                    }
                    
                }

            }
        };

        *current_output = new_output.clone();

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


    /// set rtpserver if not already set
    /// 
    /// inside, try to attach proxysink to pipeline and in idle_add attach to pipeline
    /// IMPORTANT: does not start the rtspserver
    fn attach_rtpserver(&self) {
        let locked_rtpserver = self.rtpserver.lock();
        let element = locked_rtpserver.as_ref().unwrap().get_element();
        let weak_elemenet = element.downgrade();
        drop(locked_rtpserver);

        let weak_self = self.downgrade();
        glib::idle_add(move || {
            let this = upgrade_weak!(weak_self, Continue(false));
            let element = upgrade_weak!(weak_elemenet, Continue(false));
            debug!("idle_add attach network rtp bin sink");
            this._attach_networksink(&element);
            Continue(false)
        }); 
    }

    /// # attach_proxysink to pipeline, skip if already attached
    fn _attach_networksink(&self, element: &gst::Element) {
        // check if proxysink is already linked && attached
        if self.pipeline.by_name("RTPServer0").is_some() {
            trace!("rtpserver already attached");
        } else {
            trace!("attach rtpserver");
            let _ = element.sync_state_with_parent();
            let _ = self.pipeline.add(element);
        }
        let _ = self.tee_bin.sync_state_with_parent();
        let response_tee_bin_link = self.tee_bin.link(element);
        if response_tee_bin_link.is_err() {
            trace!("tee_bin already linked with rtpserver");
        } else {
            trace!("tee bin linked with rtpserver");
        }
    }


}
