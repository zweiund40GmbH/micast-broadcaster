
use micast_rodio::{new_gstreamer, Mp3Streamer};
use std::sync::Arc; //, atomic::AtomicBool};
use log::warn;
use micast_rodio::StreamType;

pub use micast_rodio::Volume;

pub struct Output {
    streamer: Arc<Mp3Streamer>,
    thread_id: Option<std::thread::JoinHandle<()>>,
}

impl Output {
    pub fn new_from_broadcaster(broadcaster: &super::Broadcast, default_uri: &str, xml: Option<String>, emergency_playlist: Vec<String>) -> Self {
        let appsrc = broadcaster.appsrc.clone();
        let streamer = new_gstreamer(&appsrc, Some(default_uri.to_string()), emergency_playlist, 1.0, 0.5, 0.5, 0.0);

        if let Some(xml) = xml {
            let _ = streamer.set_xml(xml);
        }

        Output {
            //appsrc,
            streamer: Arc::new(streamer),
            thread_id: None,
        }
    }

    pub fn new_from_rtspserver(appsrc: &gst_app::AppSrc, default_uri: &str, xml: Option<String>, emergency_playlist: Vec<String>) -> Self {
        let streamer = new_gstreamer(&appsrc, Some(default_uri.to_string()), emergency_playlist, 1.0, 0.5, 0.5, 0.0);

        if let Some(xml) = xml {
            let _ = streamer.set_xml(xml);
        }

        Output {
            //appsrc,
            streamer: Arc::new(streamer),
            thread_id: None,
        }
    }

    pub fn run(&mut self) {
        // self.alive.store(true, std::sync::atomic::Ordering::SeqCst);
        // let alive = self.alive.clone();

        let cloned_streamer = self.streamer.clone();

        let thread_id = std::thread::spawn(move || {
            cloned_streamer.run();
            warn!("Streamer thread ended!");
        });

        self.thread_id = Some(thread_id);

    }

    pub fn play(&self, uri: &str) {
        // https://itcoops.de/streambeamteam/bla.xml -> das ist kein online stream sondern ne playlist
        // file://home/pi/bla.xml -> das ist ne lokale playlist
        // datei.mp3#b4e7d5 alles nach der raute die filesize als hexstring


        /*if uri.ends_with(".xml") {
            let _  = self.streamer.set_stream(StreamType::Offline(Some(uri.to_string())));
            return;
        }*/
        let _  = self.streamer.set_stream(StreamType::Online(Some(uri.to_string())));
        //let _  = self.streamer.set_stream(Some(uri.to_string()));
    }

    pub fn set_timetable(&self, xml: &str) {
        let _ = self.streamer.set_xml(xml.to_string());
    }

    pub fn set_volume(&self, volume: Volume) {
        let _ = self.streamer.set_volume(volume);
    }

    //DEPRECATED: doesnt need to call
    pub fn is_restarted(&self) -> bool {
        //self.streamer.is_restarted()
        
        false
    }

    
}