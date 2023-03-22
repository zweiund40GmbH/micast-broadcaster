
use gst::glib;
use log::info;
use micast_broadcaster::PlaybackClient;

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

    let player = PlaybackClient::new(
        "127.0.0.1",
        "127.0.0.1", // rtp in
        5000,
        Some(8555),
        Some(44100), 
        Some(1000), 
        None, 
        None,
    ).unwrap();        
    player.start();

    /// NEVER CLONE !!!! ALWAYS DOWNGRADE!!!!
    //let downcasted_player = player.downgrade();
    //glib::timeout_add(std::time::Duration::from_secs(30), move || {
    //    let player = downcasted_player.upgrade().unwrap();
    //    info!("CHAAAAANGE IP!");
    //    player.change_server(Some("127.0.0.1".to_string()), Some("127.0.0.1".to_string()));
    //    //player.change_output("autoaudiosink", None);
    //    glib::Continue(false)
    //});
    //let downcasted_player = player.downgrade();
    //glib::timeout_add(std::time::Duration::from_secs(60), move || {
    //    let player = downcasted_player.upgrade().unwrap();
    //    info!("CHAAAAANGE IP to find it by broadcast!");
    //    player.change_server(None, None);
    //    //player.change_output("autoaudiosink", None);
    //    glib::Continue(false)
    //});
    
    main_loop.run();

    Ok(())
}