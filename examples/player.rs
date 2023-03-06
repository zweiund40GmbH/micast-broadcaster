
use std::net::UdpSocket;

use gst::glib;
use log::{info,warn};
use micast_broadcaster::PlaybackClient;

// simple thread sleep helper
macro_rules! sleep {
    ($r:expr) => {{
        std::thread::sleep(std::time::Duration::from_millis($r * 1000));
    }};
}

// simple thread sleep helper
macro_rules! sleep_ms {
    ($r:expr) => {{
        std::thread::sleep(std::time::Duration::from_millis($r));
    }};
}

fn main() -> Result<(), Box<anyhow::Error>> {
    env_logger::init();

    info!("Broadcast player tester");

    use std::net::IpAddr;
    use local_ip_address::list_afinet_netifas;

    let ifas = list_afinet_netifas().unwrap();

    for (name, ipaddr) in ifas {
        if matches!(ipaddr, IpAddr::V4(_)) && (!name.contains("lo") || ipaddr.is_loopback() == false ) && ipaddr.is_ipv4() {
            println!("This is your local IP address: {:?}, {}", ipaddr, name);



        }
    }
    let main_loop = glib::MainLoop::new(None, false);

    // now we crate secondly the direct receiver client
    //let mut player = PlaybackClient::new(
    //    "224.1.1.1", "10.211.55.2", 5000,5001,5007, 8555, None, Some("bridge100".to_string())).unwrap();

    //let mut player = PlaybackClient::new(
    //    "224.1.1.1", "10.211.55.2", 5000,5001,5007, 8555, None, Some("eth0".to_string())).unwrap();

    let mut player = PlaybackClient::new(
        //"127.0.0.1", 
        //"127.0.0.1", 
        "224.1.1.42",
        //"10.211.55.4", 
       //"224.1.1.42",
        "224.1.1.42",
        3333, // rtp in
        3335, // rtcp recv
        3336, // rtcp send
        8555, // RTP Clock Source Port
        Some(44100), // audio_rate
        None, // latency
        None, // multicas_interface
        None, // audiodevice 
    ).unwrap();        


    sleep_ms!(2);
    player.start();

    //player.change_clock("10.42.200.76")?;
    
    main_loop.run();

    Ok(())
}