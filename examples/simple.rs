use micast_broadcaster::{broadcast, scheduler::Scheduler, output };

use gst::prelude::*;

use chrono::prelude::*;

use log::{debug, warn};

use gst::glib;

use std::error::Error;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    debug!("Start test Broadcaster");
    let main_loop = glib::MainLoop::new(None, false);
    
    let broadcaster = broadcast::Builder::new()
        .set_server_ip("127.0.0.1")
        //.set_server_ip("224.1.1.43")
        .set_broadcast_ip("224.1.1.43")
        //.set_server_ip("127.0.0.1")
        //.set_broadcast_ip("127.0.0.1")
        .set_audiorate(44100)
        .set_startup_output(broadcast::OutputMode::Local(None))
        //.set_startup_output(broadcast::OutputMode::Network)
        .build_server()?;

    //broadcaster.set_scheduler(scheduler);


    let mut output = output::Output::new_from_broadcaster(&broadcaster, "https://icecast.radiobremen.de/rb/bremenvier/live/mp3/64/stream.mp3", Some("./spots/pocking_timetable.xml".to_string()));
    output.run();

    broadcaster.start()?;
    //broadcaster.play("http://server35757.streamplus.de/stream.mp3")?;
    //broadcaster.play("http://itcoops.de:8000/drumyourass.mp3")?;
    //broadcaster.play("https://icecast.radiobremen.de/rb/bremenvier/live/mp3/64/stream.mp3")?;
    //let _ = broadcaster.play("http://icecast.radiobremen.de/rb/bremenvier/live/mp3/128/stream.mp3");     

    // https://icecast.radiobremen.de/rb/bremenvier/live/mp3/64/stream.mp3

    // https://wdr-1live-live.sslcast.addradio.de/wdr/1live/live/mp3/128/stream.mp3

    //let output = Arc::new(output);
    //glib::timeout_add(std::time::Duration::from_secs(20), move || {
    //    //let _ = bc_clone.switch_output(broadcast::OutputMode::Local(None));
    //    let _ = output.play("https://icecast.radiobremen.de/rb/bremenvier/live/mp3/64/stream.mp3");
    //    //let _ = bc_clone.play("https://antnds.streamabc.net/ands-antndsxmas-mp3-128-3716776");
    //    Continue(true)
    //});



    debug!("bin hier");
    main_loop.run();

    Ok(())
}
