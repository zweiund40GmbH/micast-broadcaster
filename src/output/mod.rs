
use micast_rodio::{new_gstreamer, Mp3Streamer};
use std::sync::Arc; //, atomic::AtomicBool};
use log::warn;

pub struct Output {
    streamer: Arc<Mp3Streamer>,
    thread_id: Option<std::thread::JoinHandle<()>>,
}

impl Output {
    pub fn new_from_broadcaster(broadcaster: &super::Broadcast, default_uri: &str, xml: Option<String>) -> Self {
        let appsrc = broadcaster.appsrc.clone();
        let streamer = new_gstreamer(&appsrc, Some(default_uri.to_string()), 1.0, 0.5, 0.5);

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
        let _  = self.streamer.set_stream(Some(uri.to_string()));
    }

    pub fn set_timetable(&self, xml: &str) {
        let _ = self.streamer.set_xml(xml.to_string());
    }

    pub fn is_restarted(&self) -> bool {
        self.streamer.is_restarted()
    }

    
}