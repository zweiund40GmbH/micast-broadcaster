
use log::{info};
use micast_broadcaster::PlaybackClient;


fn main() -> Result<(), Box<anyhow::Error>> {
    env_logger::init();

    info!("Broadcast player tester");

    let main_loop = glib::MainLoop::new(None, false);

    // now we crate secondly the direct receiver client
    let player = PlaybackClient::new("224.1.1.1", "127.0.0.1", 5000,5001,5007, 8555, None).unwrap();
    player.start();

    main_loop.run();

    Ok(())
}