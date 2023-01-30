
use micast_rodio::{new_gstreamer, Mp3Streamer};
use gst_app::prelude::*;
use std::sync::{Arc, atomic::AtomicBool};
use log::{warn,debug};

pub struct Output {
    appsrc: gst_app::AppSrc,
    streamer: Arc<Mp3Streamer>,
    thread_id: Option<std::thread::JoinHandle<()>>,
    alive: Arc<AtomicBool>,
}

impl Output {
    pub fn new_from_broadcaster(broadcaster: &super::Broadcast, default_uri: &str, xml: &str) -> Self {
        let appsrc = broadcaster.appsrc.clone();
        let streamer = new_gstreamer(&appsrc, default_uri.to_string(), xml, 1.0, 0.5, 0.5);
        Output {
            appsrc,
            streamer: Arc::new(streamer),
            thread_id: None,
            alive: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn run(&mut self) {
        debug!("want to run streamer!");
        self.alive.store(true, std::sync::atomic::Ordering::SeqCst);

        let alive = self.alive.clone();

        let cloned_streamer = self.streamer.clone();

        let thread_id = std::thread::spawn(move || {
            cloned_streamer.run();
            warn!("Streamer thread ended!");
        });

        debug!("Thread with streamer created!");
        self.thread_id = Some(thread_id);


    }

    pub fn play(&self, uri: &str) {
        self.streamer.set_stream(uri.to_string());
    }

    
}