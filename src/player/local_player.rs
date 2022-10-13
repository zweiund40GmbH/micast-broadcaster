
use gst::prelude::*;
use gst::glib;

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
                    let src = match err.src().and_then(|s| s.downcast::<gst::Element>().ok()) {
                        None => {
                            warn!("could not handle error cause no element found");
                            return glib::Continue(true);
                        },
                        Some(src) => src,
                    };
                    if src.name() == "local_tcpclient" {
                        //warn!("element what makes the error ist local_tcpclient: {:?}", src.path_string());
                        

                        if let Some(a) = err.error().kind::<gst::ResourceError>() {
                            match a {
                                gst::ResourceError::OpenRead => { 
                                    warn!("Local tcp client cannot open the stream, we want to restart it");
                                    sleep_ms!(500);
                                    let _ = pipeline.set_state(gst::State::Ready);
                                    let _ = pipeline.set_state(gst::State::Playing);
                                }
                                _ => {
                                    warn!("error not handled: {:?}", a);
                                }
                            }
                        } else {
                            warn!("error: {:?}", err.error());
                        }
                    } else {
                       
                        if let Some(parent) = src.parent() {
                            if parent.name() == "audiosink" {
                                sleep_ms!(1000);
                                let _ = pipeline.set_state(gst::State::Ready);
                                let _ = pipeline.set_state(gst::State::Playing);
                                return glib::Continue(true);
                            }
                        }

                        warn!("receive an error from {:?}", src.name());
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
        //tcp_client.try_set_property("host", "10.42.200.43")?;

        let _srcpad = tcp_client.static_pad("src").unwrap();

        
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
            self.pipeline.set_state(gst::State::Playing)?;
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
