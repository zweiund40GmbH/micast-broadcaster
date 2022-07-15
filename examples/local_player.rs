use micast_broadcaster::LocalPlayer;
use log::{debug};

use gst::glib;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    debug!("Start local Player");
    let main_loop = glib::MainLoop::new(None, false);

    //let player =  LocalPlayer::new(3333, None, 44100)?;
    let player =  LocalPlayer::new(3333, None, 44100)?;
    //let player =  LocalPlayer::new(3333, None, 48000)?;

    //std::thread::sleep(std::time::Duration::from_millis(10000));
    debug!("start playback localplayer");
    let _ = player.play();

    main_loop.run();

    Ok(())
}
