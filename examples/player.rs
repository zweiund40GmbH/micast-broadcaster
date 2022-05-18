
use log::{info,warn};
use micast_broadcaster::PlaybackClient;

// simple thread sleep helper
macro_rules! sleep {
    ($r:expr) => {{
        std::thread::sleep(std::time::Duration::from_millis($r));
    }};
}

fn main() -> Result<(), Box<anyhow::Error>> {
    env_logger::init();

    info!("Broadcast player tester");

    let main_loop = glib::MainLoop::new(None, false);

    // now we crate secondly the direct receiver client
    let mut player = PlaybackClient::new("224.1.1.1", "127.0.0.1", 5000,5001,5007, 8555, None).unwrap();


    //player.start();


    
    if let Err(e) = player.change_output("autoaudiosink", None) {
        warn!("failed to change output");
    }
    info!("start..");
    player.start();
    info!("next...");

    /*
    DOESNT WORK!!

    sleep!(5000);
    info!("change clock ip");
    player.change_clock_address("127.0.0.1")?;

    sleep!(10000);
    info!("change server address");
    player.change_server_address("224.1.1.1")?;
    */
    info!("normal playback...");
    //sleep!(10000);
    
    /*if let Err(e) = player.change_output("autoaudiosink", None) {
        warn!("failed to change output");
    }*/
    //player.start();

    main_loop.run();

    Ok(())
}