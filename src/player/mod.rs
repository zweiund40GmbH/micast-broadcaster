
use gstreamer as gst;
use gst::prelude::*;
use gstreamer_net as gst_net;
use log::{debug,warn};
use anyhow::{anyhow};


use std::str::FromStr;
use std::sync::{Arc, RwLock};
// playback client

use crate::helpers::make_element;


/// Default latency for Playback
const LATENCY:i32 = 700;


/// Simple Playback Client for Playback RTP Server Stream
pub struct PlaybackClient {
    pub pipeline: gst::Pipeline,
    output: gst::Element,
    convert: gst::Element,
    clock: gst_net::NetClientClock,
    clock_bus: gst::Bus,
}


impl PlaybackClient {

    /// Create a Playback Client
    /// 
    /// - `server_ip` the IP Adress of the rtp server (can also be a multicast IP address)
    /// - `clock_ip` the IP Adress of the clock provider (should be the IP of the Server)
    /// - ...
    /// - latency of the playback (set higher on lower bandwith devices), if non LATENCY is used which is 700
    pub fn new(
        server_ip: &str,
        clock_ip: &str,
        rtp_port: i32,
        rtcp_recv_port: i32,
        rtcp_send_port: i32,
        clock_port: i32,
        latency: Option<i32>,
    ) -> Result<PlaybackClient, anyhow::Error> {

        gst::init()?;

        debug!("init playback client");

        let (pipeline, clock, convert, output, clock_bus) = create_pipeline(
            clock_ip,
            server_ip, 
            clock_port,
            rtp_port, 
            rtcp_recv_port,
            rtcp_send_port,
            latency,
        )?;

        let pipeline_weak = pipeline.downgrade();

        let bus = pipeline.bus().unwrap();

        bus.add_watch(move |_, msg| {
            use gst::MessageView;
    
            let pipeline = match pipeline_weak.upgrade() {
                Some(pipeline) => pipeline,
                None => return glib::Continue(false),
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

                    // currently we need to panic here.
                    // the program who use this lib, would then automatically restart.
                    // the main problem is that the pipeline stops streaming audio over rtp if any element got an error, also if we restart the pipeline (meaning: set state to stopped, and the to play)
                    panic!("got an error, quit here");
                    
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
                    warn!("ClockLost... get a new clock");
                    // Get a new clock.
                    let _ = pipeline.set_state(gst::State::Paused);
                    let _ = pipeline.set_state(gst::State::Playing);
                }
    
                _ => (),
            };
    
            // Tell the mainloop to continue executing this callback.
            glib::Continue(true)
        })
        .expect("Failed to add bus watch");

        Ok(
            PlaybackClient { 
                pipeline,
                clock,
                convert,
                output,
                clock_bus,
            }
        )

        
    }

    pub fn start(&self) {

        let _ = self.pipeline.set_state(gst::State::Playing);
        let _ = self.clock.wait_for_sync(gst::ClockTime::NONE);
        self.pipeline.set_start_time(gst::ClockTime::NONE);

    }


    pub fn stop(&self) {

        let _ = self.pipeline.set_state(gst::State::Paused);
        let _ = self.pipeline.set_state(gst::State::Null);

    }

    pub fn change_clock_address(&self, address: &str) -> Result<(), anyhow::Error> {
        self.pipeline.set_state(gst::State::Null)?;
        self.pipeline.set_state(gst::State::Ready)?;
        println!("address: {:?}", self.clock.address());
        self.clock.set_address(Some(address));
        self.pipeline.use_clock(Some(&self.clock));
        self.start();

        Ok(())
    }

    pub fn change_server_address(&self, server_address: &str) -> Result<(), anyhow::Error> {

 
        let rtcp_eingang = match self.pipeline.by_name("rtcp_eingang") {
            Some(elem) => elem,
            None => { 
                return Err(anyhow!("rtp_sink not found"))
            }
        };
  
        let rtcp_senden = match self.pipeline.by_name("rtcp_senden"){
            Some(elem) => elem,
            None => { 
                return Err(anyhow!("rtcp_sink not found"))
            }
        };

        let rtp_eingang = match self.pipeline.by_name("rtp_eingang"){
            Some(elem) => elem,
            None => { 
                return Err(anyhow!("rtcp_src not found"))
            }
        };

        self.pipeline.set_state(gst::State::Null)?;
        self.pipeline.set_state(gst::State::Ready)?;

        rtcp_eingang.set_property( "address", server_address)?;
        rtcp_senden.set_property("host", server_address)?;
        rtp_eingang.set_property( "address", server_address)?;
        
        self.pipeline.set_state(gst::State::Playing)?;
        
        Ok(())
    }

    pub fn change_output(&mut self, element: &str, device: Option<&str>) -> Result<(), anyhow::Error> {
        
        debug!("change_output, creates new element {}, with : {:?}", element, device);
        let sink = gst::ElementFactory::make(element, None)?;

        if let Some(d) = device {
            sink.set_property("device", d)?;
        }

        debug!("set pipeline to paused");
        self.pipeline.set_state(gst::State::Paused)?;

        self.pipeline.set_state(gst::State::Null)?;


        debug!("unlink and remove old output");
        self.convert.unlink(&self.output);
        self.pipeline.remove(&self.output)?;

        debug!("add and link new output");
        self.pipeline.add(&sink)?;
        self.convert.link(&sink)?;

        self.output = sink;

        Ok(())
    
    }

    /*pub fn drop_elements(&mut self) -> Result<(), anyhow::Error> {
        self.pipeline.set_state(gst::State::Paused)?;
        self.pipeline.set_state(gst::State::Null)?;

        // remove all network elements...
        if let Some(rtp_eingang) = self.pipeline.by_name("rtp_eingang") {
            self.pipeline.remove(&rtp_eingang)?;
        }

        if let Some(rtcp_eingang) = self.pipeline.by_name("rtcp_eingang") {
            self.pipeline.remove(&rtcp_eingang)?;
        }

        if let Some(rtcp_senden) = self.pipeline.by_name("rtcp_senden") {
            self.pipeline.remove(&rtcp_senden)?;
        }

        if let Some(rtpbin) = self.pipeline.by_name("rtpbin") {
            self.pipeline.remove(&rtpbin)?;
        }

        if let Some(rtpdepayload) = self.pipeline.by_name("rtpdepayload") {
            self.pipeline.remove(&rtpdepayload)?;
        }

        if let Some(convert) = self.pipeline.by_name("convert") {
            self.pipeline.remove(&convert)?;
        }

        if let Some(sink) = self.pipeline.by_name("sink") {
            self.pipeline.remove(&sink)?;
        }

        Ok(())
    }*/
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
) ->  Result<(gst::Pipeline, gst_net::NetClientClock, gst::Element, gst::Element, gst::Bus), anyhow::Error> {

    let pipeline = gst::Pipeline::new(Some("playerpipeline"));

    debug!("create playback pipeline");

    let rtp_src = make_element("udpsrc", Some("rtp_eingang"))?;

    let caps = gst::Caps::from_str("application/x-rtp,channels=(int)2,format=(string)S16LE,media=(string)audio,payload=(int)96,clock-rate=(int)44100,encoding-name=(string)L24")?;

    rtp_src.set_property("caps", &caps)?;
    rtp_src.set_property("port", rtp_port as i32)?;
    rtp_src.set_property("address", &server_ip)?;

    let rtcp_src = make_element("udpsrc", Some("rtcp_eingang"))?;
    rtcp_src.set_property("port", rtcp_recv_port as i32)?;
    rtcp_src.set_property("address", &server_ip)?;

    let rtcp_sink = make_element("udpsink", Some("rtcp_senden"))?;
    rtcp_sink.set_property("port", rtcp_send_port as i32)?;
    rtcp_sink.set_property("host", &server_ip)?;
    rtcp_sink.set_property("async", &false)?; 
    rtcp_sink.set_property("sync", &false)?;

    let rtpbin = make_element("rtpbin", Some("rtpbin"))?;
    rtpbin.set_property_from_str("buffer-mode", "synced");
    rtpbin.set_property("latency", latency.unwrap_or(LATENCY) as u32)?;
    rtpbin.set_property_from_str("ntp-time-source", "clock-time");
    rtpbin.set_property("ntp-sync", &true)?;
    rtpbin.set_property("autoremove", &true)?;
    rtpbin.set_property("max-rtcp-rtp-time-diff", 100)?;

    // put all in the pipeline
    pipeline.add(&rtp_src)?;
    pipeline.add(&rtcp_src)?;
    pipeline.add(&rtcp_sink)?;
    pipeline.add(&rtpbin)?;

    rtp_src.link_pads(Some("src"), &rtpbin, Some("recv_rtp_sink_0"))?;
    rtcp_src.link_pads(Some("src"), &rtpbin, Some("recv_rtcp_sink_0"))?;
    rtpbin.link_pads(Some("send_rtcp_src_0"), &rtcp_sink, Some("sink"))?;

    let rtpdepayload = make_element("rtpL24depay", Some("rtpdepayload"))?;
    let convert = make_element("audioconvert", Some("convert"))?;
    let sink = make_element("autoaudiosink", Some("sink"))?;

    pipeline.add(&rtpdepayload)?;
    pipeline.add(&convert)?;
    pipeline.add(&sink)?;

    sink.set_property("sync", &true)?;

    gst::Element::link_many(&[&rtpdepayload, &convert, &sink])?;


    let last_pad_name: Arc< RwLock< Option<String>>> = Arc::new(RwLock::new(None));


    let play_element_downgraded = rtpdepayload.downgrade();

    rtpbin.connect_pad_added(move |el, pad| {
        let name = pad.name().to_string();
        let play_element = play_element_downgraded.upgrade().unwrap();
        if name.contains("recv_rtp_src") {
            {
                if last_pad_name.read().unwrap().is_some() {
                    el.unlink(&play_element);
                }
            }

            el.link_pads(Some(&name), &play_element, None).expect("link should work");

            {
                let mut w = last_pad_name.write().unwrap();
                *w = Some(name);
            }
            
        }
    });

    let clock = gst_net::NetClientClock::new(Some("clock0"), clock_ip, clock_port, 0 * gst::ClockTime::MSECOND);
        
    let clock_bus = gst::Bus::new();
    clock.set_property("bus", &clock_bus)?;
    clock.set_property("timeout", 1000 as u64)?;
    clock_bus.add_signal_watch();

    pipeline.use_clock(Some(&clock));
    pipeline.set_latency(Some(latency.unwrap_or(LATENCY) as u64 * gst::ClockTime::MSECOND));

    Ok((pipeline, clock, convert, sink, clock_bus))
}