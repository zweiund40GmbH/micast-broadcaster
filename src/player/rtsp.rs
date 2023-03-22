use gst::prelude::*;
use gst::glib;
use log::{debug,warn, info};

use std::str::FromStr;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Weak};
use parking_lot::Mutex;

use crate::helpers::{make_element, upgrade_weak};

use crate::sleep_ms;

use crate::services::dedector_client::ClockService;


/// Default latency for Playback
const LATENCY:i32 = 900;

#[allow(unused)]
const ENCRYPTION_ENABLED:bool = false;
const DEFAULT_AUDIO_RATE:i32 = 44100;


struct State {
    #[allow(unused)]
    rtspsrc: gst::Element,
    source: gst::Element,
    audio_in_src: gst::Pad,
    recv_rtp_src: Option<gst::Pad>,
    current_output_device: String,
    current_output_element: String,
    current_server_ip: String,
}

/// Clock State
struct ClockState {
    //clock: Option<gst_net::NetClientClock>,
    address: String,
    port: i32,
    clock_service: ClockService,
}

#[derive(Clone)]
pub(crate) struct PlaybackClientWeak(Weak<PlaybackClientInner>);

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
    state: Arc<Mutex<State>>,
    clock_state: Arc<Mutex<ClockState>>,
    error_recover: Arc<Mutex<bool>>,
}

#[derive(Clone)]
pub struct PlaybackClient(Arc<PlaybackClientInner>);


impl PlaybackClient {

    // Downgrade the strong reference to a weak reference
    pub(crate) fn downgrade(&self) -> PlaybackClientWeak {
        PlaybackClientWeak(Arc::downgrade(&self.0))
    }

    /// Create a Playback Client
    /// 
    /// - `server_ip` * the IP Adress of the rtp server (can also be a multicast IP address)
    /// - `rtp_port`  * port where the stream gets received
    /// - `rtcp_recv_port` * port where the rtcp stream gets received
    /// - `rtcp_send_port` * port where the rtcp stream gets sent
    /// - `clock_address` * tuple of the clock address and port
    /// - latency of the playback (set higher on lower bandwith devices), if non LATENCY is used which is 700
    pub fn new(
        server_ip: &str,
        clock_address: (&str, i32),
        audio_rate: Option<i32>,
        latency: Option<i32>,
        audio_device: Option<String>,
    ) -> Result<PlaybackClient, anyhow::Error> {

        gst::init()?;

        debug!("init playback client");


        let mut clock_service = ClockService::new()?;

        let clock_address_from_service = if clock_address.0 == "127.0.0.1" {
            info!("skip using broadcast founded clock, cause localhost is set");
            None
        } else {
            let _ = clock_service.run();
            let mut clock_service_response = None;
            loop {
                clock_service_response = clock_service.wait_for_clock(std::time::Duration::from_secs(30));
                if  clock_service_response.is_some() {
                    break;
                }
                sleep_ms!(1000);
                info!("wait 30 seconds for a clock retry now");
            }
            clock_service_response
        };


        let clock = if let Some(clock_from_service) = clock_address_from_service.clone() {
            info!("got clock from service: {:?}, stop lock for clock", clock_from_service);
            clock_service.stop();
            create_clock(clock_from_service.0.as_str(), clock_from_service.1.into())
        } else {
            create_clock(clock_address.0, clock_address.1)
        }?;


        // if server_ip is localhost use this ip, else use the clock ip if received by remote broadcast
        let server_address = if server_ip.contains("127.0.0.1") {
            server_ip.to_string()
        } else {
            if let Some(clock_from_service) = clock_address_from_service.clone() {
                clock_from_service.0
            } else {
                server_ip.to_string()
            }
        };

        let (pipeline, convert, source, rtspsrc, rtpdepayload) = create_pipeline(
            &server_address, 
            latency,
            audio_device.clone(),
        )?;

        // set the clock of pipeline based on ntp time
        pipeline.use_clock(Some(&clock));
        pipeline.set_latency(gst::ClockTime::from_seconds(2));

        let clock_state = ClockState {
            //clock: Some(clock),
            address: clock_address.0.to_string(),
            port: clock_address.1,
            clock_service,
        };



        let pipeline_weak = pipeline.downgrade();
        let pipeline_2weak = pipeline.downgrade();

        let bus = pipeline.bus().unwrap();

        let audio_in_src = convert.static_pad("src").unwrap();

        let state = State { 
            rtspsrc: rtspsrc.clone(),
            source,
            audio_in_src,
            recv_rtp_src: None,
            current_output_element: "alsasink".to_string(),
            current_output_device: audio_device.unwrap_or("".to_string()),
            current_server_ip: server_ip.to_string(),
        };

        let playbackclient = PlaybackClient(Arc::new(PlaybackClientInner { 
            pipeline,
            convert,
            rtpdepayload,
            audio_rate: audio_rate.unwrap_or(DEFAULT_AUDIO_RATE),
            state: Arc::new(Mutex::new(state)),
            clock_state: Arc::new(Mutex::new(clock_state)),
            error_recover: Arc::new(Mutex::new(false)),
        }));

        glib::timeout_add(std::time::Duration::from_secs(10), move || {
            let pipeline = match pipeline_2weak.upgrade() {
                Some(pipeline) => pipeline,
                None => return glib::Continue(true),
            };
 
            info!("player - current pipeline state: {:?}", pipeline.state(Some(gst::ClockTime::from_seconds(1))));

            Continue(true)
        });

        let weak_playbackclient = playbackclient.downgrade();
        rtspsrc.connect_pad_added(move |_el, pad| {
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
                pad.link(&decoder_sink).expect("link of rtpbin pad to rtpdepayload sink should work");
    
                //pbc.pipeline.set_start_time(gst::ClockTime::NONE);
    
            }

        });



        let weak_playbackclient = playbackclient.downgrade();
        bus.add_watch(move |_, msg| {
            use gst::MessageView;
    
            let pipeline = match pipeline_weak.upgrade() {
                Some(pipeline) => pipeline,
                None => {
                    warn!("bus add watch failed to upgrade pipeline");
                    return glib::Continue(false)
                },
            };

            let playbackclient = match weak_playbackclient.upgrade() {
                Some(playbackclient) => playbackclient,
                None => {
                    warn!("bus add watch failed to upgrade playbackclient");
                    return glib::Continue(false)
                },
            };


            let mut recoverable_error = false;
            match msg.view() {
                MessageView::Eos(..) => {
                    warn!("received eos");
                    // An EndOfStream event was sent to the pipeline, so we tell our main loop
                    // to stop execution here.
                    recoverable_error = true;
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

                    if src.name() == "rtspsrc0" {
                        //let weak_pipeline = pipeline.downgrade();
                        recoverable_error = true;
                    }
                    
                }
                MessageView::ClockLost(_) => {
                    warn!("player - ClockLost... get a new clock");
                    // Get a new clock.
                    
                    pipeline.call_async(move |pipeline| {
                        let _ = pipeline.set_state(gst::State::Null);
                        sleep_ms!(1000);
                        pipeline.set_start_time(gst::ClockTime::NONE);
                        if let Err(e) = pipeline.set_state(gst::State::Playing) {
                            warn!("error on call start pipeline after clock sync error : {}", e)
                        }
                    });
                }
    
                _ => (),
            };
    
            if recoverable_error {
                info!("got an recoverable error, try to restart pipeline");
                let weak_playbackclient = playbackclient.downgrade();
                glib::timeout_add(std::time::Duration::from_secs(10), move || {

                    // we need to check to always trigger this error once...
                    


                    info!("retry after error...");
                    /*let pipeline = match weak_pipeline.upgrade() {
                        Some(pipeline) => pipeline,
                        None => {
                            warn!("cannot get upgraded weak ref from pipeline inside, handle_error for rtp_eingang stops");
                            return Continue(true)
                        }
                    };*/
                    let playbackclient = match weak_playbackclient.upgrade() {
                        Some(playbackclient) => playbackclient,
                        None => {
                            warn!("bus add watch failed to upgrade playbackclient");
                            return glib::Continue(false)
                        },
                    };

                    // MAKE THIS SMARTER AN NICER
                    let error_recover_lock = playbackclient.error_recover.try_lock();
                    if let Some(error_recover) = error_recover_lock {
                        if *error_recover {
                            warn!("error recover is already in progress");
                            return Continue(true);
                        }
                        drop(error_recover);
                    } else {
                        warn!("error recover is already in progress");
                        return Continue(true);
                    }
                    

                    let state = playbackclient.clock_state.lock();
                    let last_address = state.address.clone();
                    drop(state);
                    let _ = playbackclient.change_server(&last_address, Some(std::time::Duration::from_secs(2)));
                    sleep_ms!(1000);
                    pipeline.call_async(move |pipeline| {
                        debug!("call async pipeline restart");
                        let _ = pipeline.set_state(gst::State::Null);
                        sleep_ms!(400);
                        pipeline.set_start_time(gst::ClockTime::NONE);
                        if let Err(e) = pipeline.set_state(gst::State::Playing) {
                            warn!("error on call start pipeline inside rtp_eingang error : {}", e)
                        }
                    });
        
                    
                    sleep_ms!(1000);
                    // MAKE THIS SMARTER AN NICER
                    let error_recover_lock = playbackclient.error_recover.try_lock();
                    if let Some(mut error_recover) = error_recover_lock {
                        *error_recover = false;
                        debug!("disable recover state");
                    }
        
                    Continue(false)
                });
            }

            // Tell the mainloop to continue executing this callback.
            glib::Continue(true)
        })
        .expect("Failed to add bus watch");

        //let weak_pbc = playbackclient.downgrade();
        //glib::timeout_add(std::time::Duration::from_secs(10), move || {
        //    let playbackclient = match weak_pbc.upgrade() {
        //        Some(playbackclient) => playbackclient,
        //        None => {
        //            
        //            return Continue(true)
        //        }
        //    };
        //    
        //    playbackclient.change_clock_and_server("10.42.200.179","224.1.1.48");
        //    
        //    Continue(false)
        //});


        Ok(
          playbackclient  
        )

        
    }

    /// Start the player
    pub fn start(&self) {
        info!("player - want to start playback");
        if let Err(e) =  self.pipeline.set_state(gst::State::Playing) {
            warn!(" error on start playback for palyer {:?}", e);
            
        }
        //let _ = self.clock.wait_for_sync(Some(5 * gst::ClockTime::SECOND));
        //self.pipeline.set_start_time(gst::ClockTime::NONE);

    }


    /// Stops the player
    pub fn stop(&self) {

        let _ = self.pipeline.set_state(gst::State::Paused);
        sleep_ms!(200);
        let _ = self.pipeline.set_state(gst::State::Null);

    }

    /// Change Server and clock address
    /// 
    /// # Arguments
    /// 
    /// * `address` - IP Address / Hostname of the RTP Stream provider, can also be a multicast address
    /// 
    pub fn change_server(&self, address: &str, timeout: Option<std::time::Duration>) -> Result<(), anyhow::Error> {

        let mut ip_changed = false;

        //let inner_state = self.state.lock();
        //if &inner_state.current_server_ip == address {
        //    info!("player - do not change ip, cause ip is not changed {}", address);
        //    return Ok(())
        //}
        //drop(inner_state);

        let mut state = self.clock_state.lock();
        if &state.address == address && address != "0.0.0.0" && address != "127.0.0.1" {
            info!("dont change the clock!, ip does not changed");
            return Ok(())
        }

        // lock for a broadcast message because address is 0.0.0.0
        let clock = if address == "0.0.0.0" {
            info!("address is 0.0.0.0 try receive broadcast");
            state.clock_service.restart();
            if let Some((address, port)) = state.clock_service.wait_for_clock(timeout.unwrap_or(std::time::Duration::from_secs(30))) {
                if address != state.address || port as i32 != state.port {
                    info!("player - change_server - change clock to {}:{} ", address, port);

                    let clock = create_clock(&address, port as i32)?;
                    //state.clock = Some(clock);
                    state.address = address.to_string();
                    state.port = port.into();
                    ip_changed = true;
                    clock
                } else {
                    info!("player - change_server - clock is already set to {}:{} ", address, port);
                    return Ok(())
                }
            } else {
                warn!("could not find ip address for clock with broadcaster... doesnt know what i need to do now");
                return Ok(())
            }

        } else {
            let clock = create_clock(address, state.port)?;
            state.address = address.to_string();
            ip_changed = true;
            clock 
        };

        //let cloned_clock_for_async = state.clock.clone().unwrap();
        
        drop(state);

        info!("call async pipeline to change clock and ip");
        self.pipeline.call_async(move |pipeline| {
            let _ = pipeline.set_state(gst::State::Null);
            pipeline.use_clock(Some(&clock));
            if ip_changed == true {
                let rtspsrc = pipeline.by_name("rtspsrc0").unwrap();
                rtspsrc.set_property("location", &format!("rtsp://{}:8554/micast-dj", clock.address().unwrap()));
            }
            pipeline.set_start_time(gst::ClockTime::NONE);
            sleep_ms!(1000);
            let _ = pipeline.set_state(gst::State::Playing);
            info!("async called finished, clock and ip changed");
        });

        Ok(()) 
    }

    ///
    /// Change the output device
    /// 
    /// # Arguments
    /// * `element` - Name of the element, e.g. alsasink
    /// * `device` - Optional device name, e.g. hw:0,0
    /// 
    pub fn change_output(&self, element: &str, device: Option<&str>) -> Result<(), anyhow::Error> {
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


/// crates a rtp playback pipeline 
fn create_pipeline(
    server_ip: &str, 
    latency: Option<i32>,
    audio_device: Option<String>,
) ->  Result<(gst::Pipeline, gst::Element, gst::Element, gst::Element, gst::Element), anyhow::Error> {

    let pipeline = gst::Pipeline::new(Some("playerpipeline"));

    debug!("create playback pipeline");

    let rtspsrc = make_element("rtspsrc", Some("rtspsrc0"))?;
    rtspsrc.set_property("location", &format!("rtsp://{}:8554/micast-dj", server_ip));
    rtspsrc.set_property("do-rtcp", &true);
    rtspsrc.set_property("latency", latency.unwrap_or(LATENCY) as u32); 
    rtspsrc.set_property_from_str("ntp-time-source", "clock-time");
    rtspsrc.set_property("use-pipeline-clock", true);
    rtspsrc.set_property_from_str("buffer-mode", "synced");
    rtspsrc.set_property("ntp-sync", true); 
    // put all in the pipeline
    pipeline.add(&rtspsrc)?;


    let rtpdepayload = make_element("rtpL24depay", Some("rtpdepayload"))?;
    let convert = make_element("audioconvert", Some("convert"))?;


    let sink = if let Some(device) = audio_device {
        let sink = make_element("alsasink", Some("sink"))?;
        sink.set_property("device", device);
        sink
    } else {
        make_element("autoaudiosink", Some("sink"))?
    };

    pipeline.add(&rtpdepayload)?;
    pipeline.add(&convert)?;
    pipeline.add(&sink)?;

    sink.set_property("sync", &true);
    

    gst::Element::link_many(&[&rtpdepayload, &convert, &sink])?;
    

    pipeline.set_latency(Some(latency.unwrap_or(LATENCY) as u64 * gst::ClockTime::MSECOND));

    Ok((pipeline, convert, sink, rtspsrc, rtpdepayload))
}


/// creates a net clock client
fn create_clock(address: &str, port: i32) -> Result<gst_net::NetClientClock, anyhow::Error> {
    let clock = gst_net::NetClientClock::new(None, address, port, gst::ClockTime::ZERO);
    clock.set_property("timeout", 2 as u64);
    //clock.set_property("property_nae", value)
    Ok(clock)
}