
pub(crate) mod local_player;
use gst::prelude::*;
use gst::glib;
use log::{debug,warn, info};
use anyhow::{anyhow};

use std::str::FromStr;
use std::sync::{Arc, Weak};
use parking_lot::{Mutex, RwLock};
// playback client

use crate::helpers::{make_element, upgrade_weak};

use crate::sleep_ms;


/// Default latency for Playback
const LATENCY:i32 = 900;

const ENCRYPTION_ENABLED:bool = true;
const DEFAULT_AUDIO_RATE:i32 = 44100;


struct State {
    rtpbin: gst::Element,
    source: gst::Element,
    audio_in_src: gst::Pad,
    recv_rtp_src: Option<gst::Pad>,
    clock: gst_net::NetClientClock,
    current_output_device: String,
    current_output_element: String,
    current_clock_ip: String,
    current_server_ip: String,
    //clock_bus: gst::Bus,
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
    convert: gst::Element,
    rtpdepayload: gst::Element,
    audio_rate: i32,
    state: Arc<Mutex<State>>,
    clock_port: i32,
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
    /// - `clock_ip`  * the IP Adress of the clock provider (should be the IP of the Server)
    /// - `rtp_port`  * port where the stream gets received
    /// - latency of the playback (set higher on lower bandwith devices), if non LATENCY is used which is 700
    pub fn new(
        server_ip: &str,
        clock_ip: &str,
        rtp_port: i32,
        rtcp_recv_port: i32,
        rtcp_send_port: i32,
        clock_port: i32,
        audio_rate: Option<i32>,
        latency: Option<i32>,
        multicast_interface: Option<String>,
        audio_device: Option<String>,
    ) -> Result<PlaybackClient, anyhow::Error> {

        gst::init()?;

        debug!("init playback client");

        let (pipeline, convert, source, rtpbin, rtpdepayload) = create_pipeline(
            clock_ip,
            server_ip, 
            clock_port,
            rtp_port, 
            rtcp_recv_port,
            rtcp_send_port,
            latency,
            multicast_interface,
            audio_device.clone(),
        )?;

        //let (clock, clock_bus) = create_net_clock(&pipeline, clock_ip, clock_port)?;
        let clock= create_net_clock(&pipeline, clock_ip, clock_port)?;

        let pipeline_weak = pipeline.downgrade();
        let pipeline_2weak = pipeline.downgrade();

        let bus = pipeline.bus().unwrap();

        let audio_in_src = convert.static_pad("src").unwrap();

        let state = State { 
            rtpbin: rtpbin.clone(),
            source,
            audio_in_src,
            recv_rtp_src: None,
            clock,
            current_output_element: "alsasink".to_string(),
            current_output_device: audio_device.unwrap_or("".to_string()),
            current_clock_ip: clock_ip.to_string(),
            current_server_ip: server_ip.to_string(),
            //clock_bus,
        };

        let playbackclient = PlaybackClient(Arc::new(PlaybackClientInner { 
            pipeline,
            convert,
            rtpdepayload,
            audio_rate: audio_rate.unwrap_or(DEFAULT_AUDIO_RATE),
            state: Arc::new(Mutex::new(state)),
            clock_port,
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
        rtpbin.connect_pad_added(move |el, pad| {
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



        bus.add_watch(move |_, msg| {
            use gst::MessageView;
    
            let pipeline = match pipeline_weak.upgrade() {
                Some(pipeline) => pipeline,
                None => {
                    warn!("bus add watch failed to upgrade pipeline");
                    return glib::Continue(false)
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

                    let src = match err.src().and_then(|s| s.downcast::<gst::Element>().ok()) {
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
                                sleep_ms!(200);
                                if let Err(e) = pipeline.set_state(gst::State::Playing) {
                                    warn!("error on call start pipeline inside rtp_eingang error : {}", e)
                                }

                                //pipeline.set_start_time(gst::ClockTime::NONE);
                            });
                 
                            
                
                            Continue(false)
                        });
                    }
                    
                }
                /*MessageView::Buffering(buffering) => {
                    // If the stream is live, we do not care about buffering.
                    /*if is_live {
                        return glib::Continue(true);
                    }*/
    
                    // Wait until buffering is complete before start/resume playing.
                    let percent = buffering.percent();
                    if percent < 100 {
                        let _ = pipeline.set_state(gst::State::Paused);
                    } else {
                        let _ = pipeline.set_state(gst::State::Playing);
                    }
                    // /* */ buffering_level.lock().unwrap() = percent;
                }*/    
                MessageView::ClockLost(_) => {
                    warn!("player - ClockLost... get a new clock");
                    // Get a new clock.
                    
                    pipeline.call_async(move |pipeline| {
                        let _ = pipeline.set_state(gst::State::Null);
                        sleep_ms!(200);
                        if let Err(e) = pipeline.set_state(gst::State::Playing) {
                            warn!("error on call start pipeline after clock sync error : {}", e)
                        }
                        //pipeline.set_start_time(gst::ClockTime::NONE);
                    });
                }
    
                _ => (),
            };
    
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

    /// Change Clock and Server address
    /// 
    /// # Arguments
    /// 
    /// * `clock` - IP Address / Hostname of the clock provider
    /// * `server` - IP Address / Hostname of the RTP Stream provider, can also be a multicast address
    /// 
    pub fn change_clock_and_server(&self, clock_ip: &str, server: &str) -> Result<(), anyhow::Error> {
 

        


        let inner_state = self.state.lock();
        if &inner_state.current_clock_ip == clock_ip && &inner_state.current_server_ip == server {
            info!("player - do not change ip, cause ip is not changed {} {}", clock_ip, server);
            return Ok(())
        }
        drop(inner_state);

        //info!("player - change_clock_and_server - stop playback");
        //self.stop();
        
        let weak_self = self.downgrade();
        let server = server.to_string();
        let clock_ip = clock_ip.to_string();
        self.pipeline.call_async(move |pipeline| {

            let this = upgrade_weak!(weak_self);

            pipeline.set_state(gst::State::Null);

            let mut state_guard = this.state.lock();
            let clock = create_net_clock(&pipeline, &clock_ip, this.clock_port).unwrap();
            state_guard.clock = clock;
            drop(state_guard);

            //debug!("wait 5 seconds");
            //sleep_ms!(15000);

            
            

            let rtcp_eingang = match pipeline.by_name("rtcp_eingang") {
                Some(elem) => elem,
                None => { 
                    return
                    //return Err(anyhow!("rtp_sink not found"))
                }
            };

            let rtcp_senden = match pipeline.by_name("rtcp_senden"){
                Some(elem) => elem,
                None => { 
                    return
                    //return Err(anyhow!("rtcp_sink not found"))
                }
            };

            let rtp_eingang = match pipeline.by_name("rtp_eingang"){
                Some(elem) => elem,
                None => { 
                    return
                    //return Err(anyhow!("rtcp_src not found"))
                }
            };

            info!("player - change_clock_and_server - rtp_eingang changed");

            rtcp_eingang.try_set_property( "address", &server);
            rtcp_senden.try_set_property("host", &server);
            rtp_eingang.try_set_property( "address", &server);
            

            let mut state_guard = this.state.lock();
            state_guard.current_server_ip = server.to_string();
            state_guard.current_clock_ip = clock_ip.to_string();
            drop(state_guard);


            sleep_ms!(200);

            info!("player - change_clock_and_server - now start player again");
            //self.start();
            pipeline.set_state(gst::State::Playing);
            info!("player - change_clock_and_server - started");

        });
        


        Ok(())
    }

    /// Change Clock address
    /// 
    /// # Arguments
    /// 
    /// * `clock` - IP Address / Hostname of the clock provider
    /// 
    //pub fn change_clock(&self, clock: &str) -> Result<(), anyhow::Error> {
    //    self.stop();
    //    
    //    //debug!("change current clock address {} to {}", self.clock.address().unwrap_or(glib::GString::from("-unknown-")), clock);
    //    
    //    let (clock, clock_bus) = create_net_clock(&self.pipeline, clock, 8555)?;
    //    //drop(self.clock);
    //    //drop(self.clock_bus);
    //    self.clock = clock;
    //    self.clock_bus = clock_bus;
    //    debug!("created a clock and wait 5 seconds now...");
    //    sleep_ms!(5000);
    //    //self.clock.set_address(Some(clock));
    //    debug!("finished change clock...");
    //    sleep_ms!(200);
    //    self.start();
    //    
    //    Ok(())
    //}



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
        let source = gst::ElementFactory::make(element, None)?;

        if let Some(d) = device {
            source.try_set_property("device", d)?;
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


/// crates a simple playback pipeline and a net clock client
fn create_pipeline(
    clock_ip: &str, 
    server_ip: &str, 
    clock_port: i32, 
    rtp_port: i32, 
    rtcp_recv_port: i32, 
    rtcp_send_port: i32,
    latency: Option<i32>,
    multicast_interface: Option<String>,
    audio_device: Option<String>,
) ->  Result<(gst::Pipeline, gst::Element, gst::Element, gst::Element, gst::Element), anyhow::Error> {

    let pipeline = gst::Pipeline::new(Some("playerpipeline"));

    debug!("create playback pipeline");

    let rtp_src = make_element("udpsrc", Some("rtp_eingang"))?;

    let caps = if ENCRYPTION_ENABLED && std::env::var("BC_ENCRYPTION_DISABLED").ok().is_none() {
        gst::Caps::from_str("application/x-srtp,channels=(int)2,format=(string)S16LE,media=(string)audio,payload=(int)96,clock-rate=(int)44100,encoding-name=(string)L24")
    } else {
        gst::Caps::from_str("application/x-rtp,channels=(int)2,format=(string)S16LE,media=(string)audio,payload=(int)96,clock-rate=(int)44100,encoding-name=(string)L24")
    }?;

    rtp_src.try_set_property("caps", &caps)?;
    rtp_src.try_set_property("port", rtp_port as i32)?;
    rtp_src.try_set_property("address", &server_ip)?;

    let rtcp_src = make_element("udpsrc", Some("rtcp_eingang"))?;
    //rtcp_src.try_set_property("caps", &crate::encryption::simple_encryption_cap(Some(0)).unwrap())?;
    rtcp_src.try_set_property("port", rtcp_recv_port as i32)?;
    rtcp_src.try_set_property("address", &server_ip)?;

    let rtcp_sink = make_element("udpsink", Some("rtcp_senden"))?;
    rtcp_sink.try_set_property("port", rtcp_send_port as i32)?;
    rtcp_sink.try_set_property("host", &server_ip)?;

    rtcp_sink.try_set_property("async", &false)?; 
    rtcp_sink.try_set_property("sync", &false)?;

    //rtcp_sink.try_set_property("bind-address", &server_ip)?;

    // this is in newest gstreamer version depricated.. but at least, i will try it
    rtcp_sink.try_set_property("force-ipv4", &true)?;

    // rtcp_sink.try_set_property("multicast-iface", &"enp5s0,wlp1s0")?;

    let rtpbin = make_element("rtpbin", Some("rtpbin"))?;
    rtpbin.try_set_property_from_str("buffer-mode", "synced")?;
    rtpbin.try_set_property("latency", latency.unwrap_or(LATENCY) as u32)?;
    rtpbin.try_set_property_from_str("ntp-time-source", "clock-time")?;
    rtpbin.try_set_property("ntp-sync", &true)?;
    rtpbin.try_set_property("autoremove", &true)?;

    if ENCRYPTION_ENABLED && std::env::var("BC_ENCRYPTION_DISABLED").ok().is_none() {
        crate::encryption::client_encryption(&rtpbin)?;
    }
    

    // put all in the pipeline
    pipeline.add(&rtp_src)?;
    pipeline.add(&rtcp_src)?;
    pipeline.add(&rtcp_sink)?;
    pipeline.add(&rtpbin)?;

    rtp_src.link_pads(Some("src"), &rtpbin, Some("recv_rtp_sink_%u"))?;
    rtcp_src.link_pads(Some("src"), &rtpbin, Some("recv_rtcp_sink_%u"))?;
    rtpbin.link_pads(Some("send_rtcp_src_%u"), &rtcp_sink, Some("sink"))?;
    

    let rtpdepayload = make_element("rtpL24depay", Some("rtpdepayload"))?;
    let convert = make_element("audioconvert", Some("convert"))?;


    let sink = if let Some(device) = audio_device {
        let sink = make_element("alsasink", Some("sink"))?;
        sink.try_set_property("device", device)?;
        sink
    } else {
        make_element("autoaudiosink", Some("sink"))?
    };

    pipeline.add(&rtpdepayload)?;
    pipeline.add(&convert)?;
    pipeline.add(&sink)?;

    sink.try_set_property("sync", &true)?;
    

    gst::Element::link_many(&[&rtpdepayload, &convert, &sink])?;
    

    pipeline.set_latency(Some(latency.unwrap_or(LATENCY) as u64 * gst::ClockTime::MSECOND));

    

    Ok((pipeline, convert, sink, rtpbin, rtpdepayload))
}

fn create_net_clock(pipeline: &gst::Pipeline, address: &str, port: i32) -> Result<gst_net::NetClientClock, anyhow::Error> {
    let clock = gst_net::NetClientClock::new(None, address, port, 0 * gst::ClockTime::MSECOND);
    debug!("clock address: {}", address);
    //let clock_bus = gst::Bus::new();
    //clock.try_set_property("bus", &clock_bus)?;
    clock.try_set_property("timeout", 2 as u64)?;

    /*clock_bus.add_watch(move |_, msg| {
        //use gst::MessageView;

        //debug!("msg: {:?}", msg);

        match msg.view() {
            /*MessageView::Element(src) => {
                if let Some(net_clock_struct) = src.structure() {
                    if net_clock_struct.synchronised == true {

                    }
                }
            }*/
            
            _ => (),
        };

        // Tell the mainloop to continue executing this callback.
        glib::Continue(true)
    })
    .expect("Failed to add bus watch");*/
    
    pipeline.use_clock(Some(&clock));

    Ok(clock)
}
