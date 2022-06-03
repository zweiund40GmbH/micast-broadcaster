use gstreamer as gst;
use gst::prelude::*;

use glib;
use glib::prelude::*;


use log::{debug, info, warn};
use std::str::FromStr;

use anyhow::{anyhow};
use crate::helpers::{make_element, upgrade_weak};
use crate::sleep_ms;

pub struct LocalPlayer {
    pipeline: gst::Pipeline,
}

impl LocalPlayer {
    

    pub fn new(port: i32) -> Result<LocalPlayer, anyhow::Error> {
        let _ = gst::init();

        debug!("init local player");

        let pipeline = gst::Pipeline::new(Some("local_player_pipeline"));

        let bus = pipeline.bus().unwrap();

        let pipeline_weak = pipeline.downgrade();
        bus.add_watch(move |_, msg| {
            let pipeline = {
                let w = pipeline_weak.upgrade();
                if w.is_none() {
                    return glib::Continue(true);
                }
                w.unwrap()
            };

            use gst::MessageView;

            match msg.view() {
                MessageView::Eos(..) => {
                    debug!("received eos");
                    let _ = pipeline.set_state(gst::State::Ready);
                    let _ = pipeline.set_state(gst::State::Playing);
                }
                MessageView::Error(err) => {
                    if let Some(element) = err.src() {
                        if element.name() == "local_tcpclient" {
                            let raw_error = err.error().into_raw();
                            let error_code = unsafe {
                                (*raw_error).code
                            };
                            if error_code == 5 {
                                warn!("connection could not get working... retry");
                                sleep_ms!(2300);
                                let _ = pipeline.set_state(gst::State::Ready);
                                let _ = pipeline.set_state(gst::State::Playing);
                            }
                        }
                    } else {
                        warn!("received an error {:#?}", err);
                    }
                }
                _ => {
                    //info!("received something else {:#?}", msg);
                }
            }

            glib::Continue(true)
        })?;
       
        let tcp_client = make_element("tcpclientsrc", Some("local_tcpclient"))?;
        tcp_client.set_property("port", &port)?;

        let srcpad = tcp_client.static_pad("src").unwrap();
        let bus_clone = bus.clone();

        srcpad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |pad, info| {

            if let Some(data) = &info.data {
                match data {
                    gst::PadProbeData::Buffer(..) => {
                        
                    }
                    gst::PadProbeData::Event(evt) => {
                       if evt.type_() != gst::EventType::Latency {
                            //debug!("received event upstream: {:#?}", info);
                       }
                       if evt.type_() == gst::EventType::Eos {
                            debug!("received EOS on tcp src probe, restart pipeline!");
                            sleep_ms!(300);
                            //bus_clone.post(&gst::message::Eos::new());
                            //let _ = pipeline.set_state(gst::State::Ready);
                            //let _ = pipeline.set_state(gst::State::Playing);
                       }
                    }
                    _ => {
                    
                    }
                }
            }

            gst::PadProbeReturn::Ok
        });
        
        let caps_element = make_element("capsfilter", Some("caps_element"))?;
        let caps = gst::Caps::from_str(r#"
                audio/x-raw,
                rate=(int)44100,
                channels=(int)2,
                format=(string)S16LE
                "#)?;
        caps_element.set_property("caps", &caps)?;

        pipeline.add(&caps_element)?;
        pipeline.add(&tcp_client)?;

        let audiosink = make_element("autoaudiosink", Some("audiosink"))?;
        pipeline.add(&audiosink)?;

        tcp_client.link(&caps_element)?;
        caps_element.link(&audiosink)?;

        Ok(LocalPlayer {
            pipeline, 
        })
    }

    pub fn play(&self) -> Result<(), anyhow::Error> {
        if let Err(e) = self.pipeline.set_state(gst::State::Playing) {
            debug!("could not switch to playing, cause of {:?}", e);
            sleep_ms!(300);
            self.pipeline.set_state(gst::State::Ready)?;
        }

        Ok(())
    }

    pub fn stop(&self) -> Result<(), anyhow::Error> {
        self.pipeline.set_state(gst::State::Paused)?;
        sleep_ms!(200);
        self.pipeline.set_state(gst::State::Null)?;

        Ok(())
    }

}
