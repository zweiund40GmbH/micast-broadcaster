use gstreamer as gst;
use gst::prelude::*;

use log::{debug, warn};
use std::str::FromStr;

use anyhow;
use crate::helpers::{make_element};
use crate::sleep_ms;

pub struct LocalPlayer {
    pipeline: gst::Pipeline,
}

impl LocalPlayer {
    

    /// #new 
    ///
    /// creates a new Localplayer
    ///
    /// - `port` tcp port to connect to
    /// - `audiosink` sink where device spits out its audio
    /// - `rate` audio rate (44100)
    ///
    pub fn new(port: i32, audiodevice: Option<&str>, rate: i32) -> Result<LocalPlayer, anyhow::Error> {
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
                    sleep_ms!(100);
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
                                sleep_ms!(2400);
                                let _ = pipeline.set_state(gst::State::Ready);
                                let _ = pipeline.set_state(gst::State::Playing);
                            }
                        } else {
                            warn!("received error {:#?}", err);
                            sleep_ms!(4600);
                            let _ = pipeline.set_state(gst::State::Ready);
                            let _ = pipeline.set_state(gst::State::Playing);

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
        tcp_client.try_set_property("port", &port)?;
        tcp_client.try_set_property("host", "127.0.0.1")?;

        let srcpad = tcp_client.static_pad("src").unwrap();


        //srcpad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |pad, info| {
        //    if let Some(data) = &info.data {
        //        match data {
        //            gst::PadProbeData::Buffer(..) => {
        //                
        //            }
        //            gst::PadProbeData::Event(evt) => {
        //               if evt.type_() != gst::EventType::Latency {
        //                    //debug!("received event upstream: {:#?}", info);
        //               }
        //               if evt.type_() == gst::EventType::Eos {
        //                    debug!("received EOS on tcp src probe, restart pipeline!");
        //                    //sleep_ms!(700);
        //                    return gst::PadProbeReturn::Remove;
        //               }
        //            }
        //            _ => {
        //            
        //            }
        //        }
        //    }
        //    gst::PadProbeReturn::Ok
        //});
        
        let caps_element = make_element("capsfilter", Some("caps_element"))?;
        let caps = gst::Caps::from_str(&format!(r#"
                audio/x-raw,
                rate=(int){},
                channels=(int)2,
                format=(string)S16LE,
                layout=(str)interleaved
                "#, rate))?;
        caps_element.try_set_property("caps", &caps)?;

        pipeline.add(&caps_element)?;
        pipeline.add(&tcp_client)?;
        tcp_client.link(&caps_element)?;


        Self::set_output(&pipeline, &caps_element, audiodevice)?;

        Ok(LocalPlayer {
            pipeline, 
        })
    }

    pub fn play(&self) -> Result<(), anyhow::Error> {
        if let Err(e) = self.pipeline.set_state(gst::State::Playing) {
            debug!("could not switch to playing, cause of {:?}", e);
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

    /// change_output
    ///
    /// change the output device
    ///
    pub fn change_output(&self, device: Option<&str>) -> Result<(), anyhow::Error> {

        if let Some(caps) = self.pipeline.by_name("caps_element") {
            self.stop()?;

            if let Some(audiosink) = self.pipeline.by_name("audiosink") {
                
                // unlink at first
                caps.unlink(&audiosink);
                self.pipeline.remove(&audiosink)?;

                Self::set_output(&self.pipeline, &caps, device)?;

                sleep_ms!(200);
                
                self.play()?;
            }
        }

        Ok(())
    }

    fn set_output(pipeline: &gst::Pipeline, caps_element: &gst::Element, audiodevice: Option<&str>) -> Result<(), anyhow::Error> {
        let audiosink = if let Some(audiodevice) = audiodevice {
            let a = make_element("alsasink", Some("audiosink"))?;
            a.try_set_property("device", audiodevice)?;
            a
        } else {
            make_element("autoaudiosink", Some("audiosink"))?
        };

        pipeline.add(&audiosink)?;
        caps_element.link(&audiosink)?;

        Ok(())
    }

}
