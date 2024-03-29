pub(crate) mod local_player;

use std::str::FromStr;
use std::sync::atomic::{Ordering, AtomicBool};
use std::sync::{Arc, Weak};
use std::time::Duration;

use parking_lot::Mutex;
use anyhow::anyhow;

use gst::prelude::*;
use gst::glib;
use log::{debug,warn, info, trace};

use crate::helpers::{make_element, upgrade_weak};
use crate::sleep_ms;
use crate::services;

/// Default latency for Playback
const LATENCY:i32 = 1500;

#[allow(unused)]
const ENCRYPTION_ENABLED:bool = false;
const DEFAULT_AUDIO_RATE:i32 = 48000;


struct State {
    #[allow(unused)]
    rtpbin: gst::Element,
    source: gst::Element,
    audio_in_src: gst::Pad,
    recv_rtp_src: Option<gst::Pad>,
    current_output_device: String,
    current_output_element: String,
    sender_clock_address: String,
}

#[derive(Clone)]
pub struct PlaybackClientWeak(Weak<PlaybackClientInner>);

impl std::ops::Deref for PlaybackClient {
    type Target = PlaybackClientInner;

    fn deref(&self) -> &PlaybackClientInner {
        &self.0
    }
}

impl PlaybackClientWeak {
    // Try upgrading a weak reference to a strong one
    pub fn upgrade(&self) -> Option<PlaybackClient> {
        self.0.upgrade().map(PlaybackClient)
    }
}


/// Simple Playback Client for Playback RTP Server Stream
pub struct PlaybackClientInner {
    pub pipeline: gst::Pipeline,
    #[allow(unused)]
    convert: gst::Element,
    #[allow(unused)]
    rtpdepayload: gst::Element,
    #[allow(unused)]
    audio_rate: i32,
    
    timeout_error_handling_is_active: AtomicBool,
    state: Arc<Mutex<State>>,
    //last_broadcast: Arc<Mutex<Option<Instant>>>,
}

#[derive(Clone)]
pub struct PlaybackClient(Arc<PlaybackClientInner>);


impl PlaybackClient {

    // Downgrade the strong reference to a weak reference
    pub fn downgrade(&self) -> PlaybackClientWeak {
        PlaybackClientWeak(Arc::downgrade(&self.0))
    }

    /// Create a Playback Client
    /// 
    /// * `server_address`  - the Address of the Server to send RTCP control Packets and sync own NTP Clock
    ///                       can set to 0.0.0.0 to search for the ip via broadcast
    ///                       can not be a multicast address
    /// * `rtp_port` - port where the rtp stream gets received 
    ///                normaly is 5000 to send RTP, 
    ///                5001 to send server RTCP, 
    ///                5002 to receiver RTCP to the server
    /// * `clock_port` - port where the NTP Server ist listen on per default 8555
    /// * `audio_rate` - audio rate of the stream per default 44100
    /// * `latency` - latency of the stream per default 700
    pub fn new(
        server_address: &str,
        rtp_port: i32,
        clock_port: Option<i32>,
        audio_rate: Option<i32>,
        latency: Option<i32>,
        audio_device: Option<String>,
        //existing_clock: Option<gst_net::NetClientClock>,
    ) -> Result<PlaybackClient, anyhow::Error> {

        gst::init()?;

        debug!("init playback client");

        let re_server_address = if server_address == "0.0.0.0" {
            None
        } else {
            Some(server_address.to_string())
        };

        // this function only search via broadcast for an ip if required (rtp_receiver_address == 0.0.0.0)
        let clock_rtcp_server_address = if re_server_address.is_none() {
            let remote_address = Self::search_for_ip(
                re_server_address, 
                Duration::from_secs(30)
            );
            services::confirm(&remote_address);
            remote_address
        } else {
            warn!("start in localhost mode");
            re_server_address.unwrap()
        };


        let clock = create_clock(&clock_rtcp_server_address, clock_port.unwrap_or(8555))?;
        let _ = clock.wait_for_sync(Some(5 * gst::ClockTime::SECOND));
        info!("send rtcp data and NTP Clock to {} & recive rtp data to 0.0.0.0", clock_rtcp_server_address);

        let use_sync_on_buffer_mode = std::env::var("USE_BUFFER_MODE_SYNC").unwrap_or("1".to_string()) == "1";

        let pipeline = gst::Pipeline::new(Some("playerpipeline"));
        pipeline.use_clock(Some(&clock));
        pipeline.set_latency(Some(gst::ClockTime::from_mseconds(LATENCY as u64)));

        let (convert, source, rtpbin, rtpdepayload, rtp_src) = create_pipeline(
            &pipeline,
            rtp_port, 
            &clock_rtcp_server_address,
            latency,
            !use_sync_on_buffer_mode,
            audio_device.clone(),
        )?;


        let pipeline_weak = pipeline.downgrade();
        let pipeline_2weak = pipeline.downgrade();

        let bus = pipeline.bus().unwrap();
        let audio_in_src = convert.static_pad("src").unwrap();
        let weak_rtpbin = rtpbin.downgrade();
        let weak_pipeline_for_confirmation = pipeline.downgrade();

        let state = State { 
            rtpbin: rtpbin,
            source,
            audio_in_src,
            recv_rtp_src: None,
            current_output_element: "alsasink".to_string(),
            current_output_device: audio_device.unwrap_or("".to_string()),
            sender_clock_address:  server_address.to_string(),
        };


        let playbackclient = PlaybackClient(Arc::new(PlaybackClientInner { 
            pipeline,
            convert,
            rtpdepayload,
            audio_rate: audio_rate.unwrap_or(DEFAULT_AUDIO_RATE),
            state: Arc::new(Mutex::new(state)),
            timeout_error_handling_is_active: AtomicBool::new(false),
        }));

        glib::timeout_add(Duration::from_millis(services::RECONFIRMATIONTIME_IN_MS), move || {
            let pipeline = match weak_pipeline_for_confirmation.upgrade() {
                Some(pipeline) => {
                    pipeline
                },
                None => return Continue(true),
            };
            
            // only send confirmation if we not in localhost mode
            if let Some(rtcp) = pipeline.by_name("rtcp_senden") {
                let hostaddress = rtcp.property::<String>("host");
                if hostaddress != "127.0.0.1" && hostaddress != "0.0.0.0" {
                    debug!("resend confirmation to: {}", hostaddress);
                    services::confirm(&hostaddress)
                }
            }


            Continue(true)

        });

        glib::timeout_add(std::time::Duration::from_secs(20), move || {
            let pipeline = match pipeline_2weak.upgrade() {
                Some(pipeline) => pipeline,
                None => return glib::Continue(true),
            };
 
            info!("player - current pipeline state: {:?}", pipeline.state(Some(gst::ClockTime::from_seconds(1))));

            Continue(true)
        });

        let weak_playbackclient = playbackclient.downgrade();
        let rtpbin = upgrade_weak!(weak_rtpbin, Err(anyhow!("rtpbin is not available")));
        rtpbin.connect_pad_added(move |_rtpbin, pad| {
            let name = pad.name().to_string();
            
            let pbc = upgrade_weak!(weak_playbackclient);
            let decoder = &pbc.rtpdepayload;
            let decoder_sink = decoder.static_pad("sink").unwrap();

            debug!("rtpbin pad_added: {} - {:?}", name, decoder);
    
            if name.contains("recv_rtp_src") {

                let mut state_guard = pbc.state.lock();
                
                if let Some(recv_rtp_src) = state_guard.recv_rtp_src.as_ref() {
                    info!("already initiate a recv_rtp pad {}. unlink it from rtpdepayload sink", recv_rtp_src.name());
                    let _ = recv_rtp_src.unlink(&decoder_sink);
                    info!("src_pad from rtpdepayload sink removed");
                }

                state_guard.recv_rtp_src = Some(pad.clone());
                drop(state_guard);
    
                info!("link newley created pad {} to rtpdepayload sink", pad.name());
                debug!("pad: {:#?}", pad);
                pad.link(&decoder_sink).expect("link of rtpbin pad to rtpdepayload sink should work");
    
            }

        });

        let weak_playbackclient = playbackclient.downgrade();
        // Bus for error handling
        bus.add_watch(move |_, msg| {
            use gst::MessageView;
    
            let pipeline = match pipeline_weak.upgrade() {
                Some(pipeline) => pipeline,
                None => {
                    warn!("bus add watch failed to upgrade pipeline");
                    return glib::Continue(true)
                },
            };

            
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

                    let src = match err.src().and_then(|s| s.clone().downcast::<gst::Element>().ok()) {
                        None => {
                            warn!("could not handle error cause no element found");
                            return glib::Continue(true);
                        },
                        Some(src) => src,
                    };

                    warn!("receive an error from {:?}", src.name());

                    if src.name() == "rtp_eingang" {
                        let weak_pipeline = pipeline.downgrade();
                        glib::timeout_add(std::time::Duration::from_secs(5), move || {

                            let pipeline = match weak_pipeline.upgrade() {
                                Some(pipeline) => pipeline,
                                None => {
                                    warn!("cannot get upgraded weak ref from pipeline inside, handle_error for rtp_eingang stops");
                                    return Continue(true)
                                }
                            };

                            pipeline.call_async(move |pipeline| {
                                let _ = pipeline.set_state(gst::State::Null);
                                //pipeline.set_start_time(gst::ClockTime::NONE);
                                pipeline.set_base_time(gst::ClockTime::ZERO);
                                sleep_ms!(200);
                                if let Err(e) = pipeline.set_state(gst::State::Playing) {
                                    warn!("error on call start pipeline inside rtp_eingang error : {}", e)
                                }

                            });
                 
                            
                
                            Continue(false)
                        });
                    }
                    
                }
                MessageView::ClockLost(_) => {
                    warn!("received clock lost");
                    // The pipeline's clock was lost, so we need to set a new one. We do this
                    // by setting the pipeline to READY (which stops the pipeline) and then
                    // to PLAYING again (which restarts the pipeline with a new clock chosen
                    // by the pipeline).
                    pipeline.set_state(gst::State::Null).unwrap();
                    sleep_ms!(200);
                    //pipeline.set_start_time(gst::ClockTime::NONE);
                    pipeline.set_base_time(gst::ClockTime::ZERO);
                    pipeline.set_state(gst::State::Playing).unwrap();
                }
                MessageView::Warning(warning) => {
                    warn!("Warning: \"{}\"", warning.debug().unwrap());
                }
                MessageView::Element(e) => {
                    if let Some(obj) = e.src() {
                        if obj.name() == "rtp_eingang" {
                            if let Some(inner_struct) = e.structure() {
                                if inner_struct.name() == "GstUDPSrcTimeout" {
                                    warn!("rtp_eingang timeout, try restart pipeline");
                                    let pbc = upgrade_weak!(weak_playbackclient, glib::Continue(true));
                                    pbc.try_reconnect();
                                }
                            }
                        }
                    } else {
                        trace!("unhandled element message: {:?}", e);
                    }
                }
                e => {
                    warn!("unhandled message: {:?}", e);
                }
            };
    
            // Tell the mainloop to continue executing this callback.
            glib::Continue(true)
        })
        .expect("Failed to add bus watch");
        Ok(
          playbackclient  
        )
    }

    /// Start the player
    ///
    /// befor start with set_state(gst::State::Playing) the start_time is set to gst::ClockTime::NONE
    pub fn start(&self) {
        info!("player - want to start playback");
        //self.pipeline.set_start_time(gst::ClockTime::NONE);
        //self.pipeline.set_base_time(gst::ClockTime::ZERO);
        if let Err(e) =  self.pipeline.set_state(gst::State::Playing) {
            warn!(" error on start playback for palyer {:?}", e);
            
        }
    }


    /// Stops the player
    pub fn stop(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }

    // currently does not work.. hang async 
    fn try_reconnect(&self) {
        if self.timeout_error_handling_is_active.load(Ordering::Relaxed) {
            warn!("skip reconnect, because timeout error handling is active");
            return;
        }

        let pbc = self.downgrade();

        glib::idle_add_local(move || {
            let pbc = upgrade_weak!(pbc, glib::Continue(true));
            let current_sender_clock_address  = {
                let state_guard = pbc.state.try_lock_for(Duration::from_millis(800));
                if state_guard.is_none() {
                    drop(state_guard);
                    warn!("try_reconnect: cannot get lock for state");
                    return glib::Continue(false);
                }
                let state_guard = state_guard.unwrap();
                let rets = state_guard.sender_clock_address.clone();
                drop(state_guard);
                rets
            };

            let sender_clock_address = if current_sender_clock_address == "0.0.0.0" {
                None
            } else {
                Some(current_sender_clock_address)
            };
            let _ = pbc.change_server(sender_clock_address);
            glib::Continue(false)
        });

    }

    /// Change Server and clock address
    /// 
    /// # Arguments
    /// 
    /// * `sender_clock_address` - IP Address / Hostname of the clock provider, should not be a multicast address
    ///                            if None we will try to find a broadcast message
    pub fn change_server(&self, sender_clock_address: Option<String>) -> Result<(), anyhow::Error> {
        let l_sender_clock_address  = 
            Self::search_for_ip(
                sender_clock_address.clone(), 
                Duration::from_secs(30)
            );
        
        let mut state = self.state.lock();
        if state.sender_clock_address == l_sender_clock_address {
            info!("player - change_server - no change in address clock_rtcp_sender:{}", l_sender_clock_address);
            return Ok(())
        }

        // always send a confirm message
        //if &l_sender_clock_address != "127.0.0.1" {
            info!("send confirm message to {}", l_sender_clock_address);
            services::confirm(&l_sender_clock_address);
        //}

        if let Err(e) = self.pipeline.set_state(gst::State::Null) {
            warn!("error on call stop pipeline inside change_server error : {}", e)
        }
        // lock for a broadcast message because address is 0.0.0.0
        if state.sender_clock_address != l_sender_clock_address {
            if sender_clock_address.is_some() {
                state.sender_clock_address = l_sender_clock_address.to_string();
            }
            warn!("change clock and rtcpsender set {}", l_sender_clock_address);
            let clock = create_clock(&l_sender_clock_address, 8555)?;
            self.pipeline.use_clock(Some(&clock));
            change_ip(&self.pipeline, "rtcp_senden", &l_sender_clock_address, true)?;
        }
        
        drop(state);

        //self.pipeline.set_start_time(gst::ClockTime::NONE);
        self.pipeline.set_base_time(gst::ClockTime::ZERO);
        

        if let Err(e) = self.pipeline.set_state(gst::State::Playing) {
            warn!("error on call Playing pipeline inside change_server error : {}", e)
        }
        self.timeout_error_handling_is_active.store(false, Ordering::Relaxed);

        Ok(()) 
    }

    /// Search for a broadcast message and return the address of the server and the RTP Receiver Address
    /// 
    /// # Arguments
    /// * `rtp_receiver_address` - current IP Address / Hostname of the RTP Stream provider, can also be a multicast address
    /// * `sender_clock_address` - current IP Address / Hostname of the clock provider, should not be a multicast address
    /// * `timeout` - timeout for the broadcast message
    /// 
    /// # Return
    /// * (sender_clock_address, rtp_receiver_address)
    fn search_for_ip(sender_clock_address: Option<String>, timeout: Duration) -> String {
        if sender_clock_address.is_some() {
            warn!("search_for_ip: we have a sender_clock_address: {:?}", sender_clock_address);
            sender_clock_address.unwrap()
        } else {
            let search_result = services::wait_for_broadcast(timeout).map_or(
                "127.0.0.1".into(), 
                |r| {
                    trace!("we got a broadcast message");
                    r.0.to_string()
                }
            );

            sender_clock_address.unwrap_or(search_result)
        }
    }

    ///
    /// Change the output device
    /// 
    /// # Arguments
    /// * `element` - Name of the element, e.g. alsasink
    /// * `device` - Optional device name, e.g. hw:0,0
    /// 
    pub fn change_output(&self, element: &str, device: Option<&str>) -> Result<(), anyhow::Error> {
        info!("CHANGE_OUTPUT");
        let inner_state = self.state.lock();
        if 
            &inner_state.current_output_device == device.unwrap_or("alsasink") && 
            &inner_state.current_output_element == element  
        {
            info!("player - device not changed, skip change_output {} {:?}", element, device);
            return Ok(()); 
        }

        drop(inner_state);


        self.stop();

        info!("player - change_output, creates new element {}, with : {:?}", element, device);
        let source = gst::ElementFactory::make_with_name(element, None)?;

        if let Some(d) = device {
            source.set_property("device", d);
        }

        let mut state_guard = self.state.lock();
        let old_sink_pad = state_guard.source.static_pad("sink").unwrap();


        debug!("unlink audio sink from converter source");
        let _ = state_guard.audio_in_src.unlink(&old_sink_pad);
        
        debug!("remove audio source element");
        self.pipeline.remove(&state_guard.source)?;
        
        sleep_ms!(200);

        debug!("add and link new output");
        self.pipeline.add(&source)?;
        state_guard.audio_in_src.link(&source.static_pad("sink").unwrap())?;
        state_guard.source = source;
        state_guard.current_output_device = device.unwrap_or("alsasink").to_string();
        state_guard.current_output_element = element.to_string();
        drop(state_guard);


        self.start();
        Ok(())
    
    }

}


/// create_pipeline creates a new GStreamer pipeline with the given parameters for receiving RTP Streams and playing them
/// 
/// # Arguments
/// * `rtp_port` - Port for the RTP Stream (usually 5000)
/// * `rtcp_sender_clock_address` - IP Address / Hostname of the clock provider, should not be a multicast address
/// * `latency` - Latency in ms
/// * `buffe_mode_as_slave` - If true, the buffer-mode on rtpbin / jitterbuffer is slave. else its synced
/// * `audio_device` - Optional audio device name, e.g. hw:0,0
/// 
fn create_pipeline(
    pipeline: &gst::Pipeline,
    rtp_port: i32, 
    rtcp_sender_clock_address: &str,
    latency: Option<i32>,
    buffe_mode_as_slave: bool,
    audio_device: Option<String>,
) ->  Result<(gst::Element, gst::Element, gst::Element, gst::Element, gst::Element), anyhow::Error> {

    //let caps = gst::Caps::from_str("application/x-rtp,channels=(int)2,format=(string)S16LE,media=(string)audio,payload=(int)96,clock-rate=(int)44100,encoding-name=(string)L24")?;
    let caps = gst::Caps::from_str("application/x-rtp,channels=(int)2,format=(string)S16LE,media=(string)audio,payload=(int)96,clock-rate=(int)48000,encoding-name=(string)OPUS")?;
    let rtcp_caps = gst::Caps::from_str("application/x-rtcp")?;

    warn!("create playback pipeline with rtp port: {}, rtcp sender clock address: {}, latency: {:?}, use slave in buffer-mode: {}, audio_device: {:?}", rtp_port, rtcp_sender_clock_address, latency, buffe_mode_as_slave, audio_device);

    let rtp_src = make_element("udpsrc", Some("rtp_eingang"))?;

    rtp_src.set_property("timeout", gst::ClockTime::from_seconds(10).nseconds());
    rtp_src.set_property("caps", &caps);
    rtp_src.set_property("port", rtp_port as i32);
    //rtp_src.set_property("address", &rtp_and_rtcp_receiver_address);
    // immer der eigene host, da der rtp stream über den eigenen host kommt
    rtp_src.set_property("address", &"0.0.0.0");

    let rtcp_src = make_element("udpsrc", Some("rtcp_eingang"))?;
    rtcp_src.set_property("caps",&rtcp_caps);
    rtcp_src.set_property("port", &((rtp_port + 1) as i32));
    rtcp_src.set_property("address", &"0.0.0.0");

    trace!("create a udpsink for sending rtcp packets to server address {}", rtcp_sender_clock_address);
    let rtcp_sink = make_element("udpsink", Some("rtcp_senden"))?;
    rtcp_sink.set_property("port", (rtp_port + 2) as i32);
    rtcp_sink.set_property("host", &rtcp_sender_clock_address);
    rtcp_sink.set_property("async", false); 
    rtcp_sink.set_property("sync", false);

    let rtpbin = make_element("rtpbin", Some("rtpbin"))?;

    // currently it doesnt work
    //let sdes = gst::Structure::builder("application/x-rtp-source-sdes")
    //    .field("cname", "ajshfhausd@192.168.0.3")
    //    .field("tool", "micast-dj")
    //    .build();
    //rtpbin.set_property("sdes", sdes);

    rtpbin.set_property("latency", 40u32);
    //rtpbin.set_property("add-reference-timestamp-meta", &true); 
    rtpbin.set_property_from_str("ntp-time-source", "clock-time");
    //rtpbin.set_property("drop-on-latency", true);

    //if std::env::var("USE_RFC7273_SYNC").unwrap_or("1".to_string()) == "1" {
    //    rtpbin.set_property("rfc7273-sync", true);
    //}

    if buffe_mode_as_slave {
        rtpbin.set_property_from_str("buffer-mode", "slave");
    } else {
        rtpbin.set_property_from_str("buffer-mode", "synced");
    }
    rtpbin.set_property("ntp-sync", true);

    // put all in the pipeline
    pipeline.add(&rtpbin)?;

    pipeline.add(&rtp_src)?;
    pipeline.add(&rtcp_src)?;
    pipeline.add(&rtcp_sink)?;

    rtp_src.link_pads(Some("src"), &rtpbin, Some("recv_rtp_sink_%u"))?;
    rtcp_src.link_pads(Some("src"), &rtpbin, Some("recv_rtcp_sink_%u"))?;
    rtpbin.link_pads(Some("send_rtcp_src_%u"), &rtcp_sink, Some("sink"))?;
    

    let rtpdepayload = make_element("rtpopusdepay", Some("rtpopusdepay"))?;
    let dec = make_element("opusdec", Some("opusdec"))?;
    let convert = make_element("audioconvert", Some("convert"))?;

    let sink = if let Some(device) = audio_device {
        let sink = make_element("alsasink", Some("sink"))?;
        sink.set_property("device", device);
        sink
    } else {
        make_element("autoaudiosink", Some("sink"))?
    };

    pipeline.add(&rtpdepayload)?;
    pipeline.add(&dec)?;
    pipeline.add(&convert)?;
    pipeline.add(&sink)?;

    sink.set_property("sync", true);

    gst::Element::link_many(&[&rtpdepayload, &dec, &convert, &sink])?;

    Ok((convert, sink, rtpbin, rtpdepayload, rtp_src))
}


/// creates a net clock client
/// 
/// # Arguments
/// * `address` - the ip address of the clock server
/// * `port` - the port of the clock server
/// 
fn create_clock(address: &str, port: i32) -> Result<gst_net::NetClientClock, anyhow::Error> {
    let clock = gst_net::NetClientClock::new(None, address, port, gst::ClockTime::ZERO);

    Ok(clock)
}


/// change the ip address of a udpsrc or udpsink element 
/// 
/// # Arguments
/// * `pipeline` - the pipeline containing the element
/// * `element` - the name of the element
/// * `address` - the new ip address
/// * `sink` - true if the element is a udpsink, false if it is a udpsrc
fn change_ip(pipeline: &gst::Pipeline, element: &str, address: &str, sink: bool) -> Result<(), anyhow::Error> {
    match pipeline.by_name(element) {
        Some(elem) => {
            if sink {
                elem.set_property( "host", &address);
            } else {
                elem.set_property( "address", &"0.0.0.0");
            }
        },
        None => { 
            return Err(anyhow!("element {} not found", element))
        }
    };
    Ok(())
}