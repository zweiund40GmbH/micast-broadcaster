use crossbeam_channel::{Sender, Receiver};
pub use std::net::IpAddr;

use std::time::{Instant, Duration};

use crate::sleep_ms;
use log::{info, trace, warn};

#[derive(Clone)]
pub struct ClockService {
    recv: Option<Receiver<IpAddr>>,
    stop_sender: Option<Sender<bool>>,
    stop_time: Option<Instant>,
}

impl ClockService {

    
    pub fn new() -> Result<ClockService, anyhow::Error>{
        //let (stop_sender, receiver) = super::informip::dedect_server_ip();
        Ok(ClockService {
            recv: None,
            stop_sender: None,
            stop_time: None,
        })
    }

    pub fn run(&mut self) -> Result<(), anyhow::Error> {
        if let Some(stop_time) = self.stop_time {
            let elapsed = stop_time.elapsed();
            if elapsed < Duration::from_secs(2) {
                let rest = Duration::from_secs(2) - elapsed;
                trace!("stop time is set, we check if we need to sleep to wait if thread is closed: {:?}", rest);
                if rest.as_millis() > 0 {
                    sleep_ms!(rest.as_millis() as u64);
                }
            }
        }
        let (stop_sender, receiver) = super::informip::dedect_server_ip();
        self.recv = Some(receiver);
        self.stop_sender = Some(stop_sender);
        self.stop_time = None;
        Ok(())
    }

    pub fn stop(&mut self) {
        trace!("send stop clock service");
        if let Some(stop_sender) = &self.stop_sender {
            let _ = stop_sender.send(false);
            self.stop_time = Some(Instant::now());
        }
    }

    pub fn restart(&mut self) {
        trace!("restart receive clock");
        self.stop();
        self.run();
        
    }

    pub fn wait_for_clock(&self, duration: Duration) -> Option<(String, u16)> {
        if self.recv.is_none() {
            warn!("reciver is not set, we can not wait for clock");
            return None
        }
        let instant_timer = Instant::now();
        while instant_timer.elapsed() < duration {
            let r = self.try_recv_clock();
            if r.is_some() {
                return r
            }
            sleep_ms!(100);
        }
        None
    }

    pub fn try_recv_clock(&self) -> Option<(String, u16)> {
        if let Some(recv) = &self.recv {
            if let Ok(clock) = recv.try_recv() {
                info!("got clock address from server: {:?}", clock);
                return Some((clock.to_string(), 8555));
            }
        }
        None
    }

}