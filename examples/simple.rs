use micast_broadcaster::{broadcast, scheduler::Scheduler };

use chrono::prelude::*;

use log::{debug, warn};

use gst::glib;
use glib::Continue;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    debug!("Start test Broadcaster");
    let main_loop = glib::MainLoop::new(None, false);


    let mut scheduler = Scheduler::new();
    //scheduler.from_file("/media/psf/Home/project_micast/dev/micast-broadcaster/spots/timetable.xml")?;
    scheduler.from_file("/Users/nico/project_micast/dev/micast-broadcaster/spots/pocking_timetable.xml")?;
    scheduler.load_files();
    
    let broadcaster = broadcast::Builder::new()
        .set_server_ip("127.0.0.1")
        //.set_server_ip("224.1.1.43")
        .set_broadcast_ip("224.1.1.43")
        //.set_server_ip("127.0.0.1")
        //.set_broadcast_ip("127.0.0.1")
        //.set_audiorate(44100 / 2)
        .set_audiorate(44100)
        .set_spot_volume(Some(0.3))
        .set_broadcast_volume(Some(0.5))
        .set_crossfade_time(Some(std::time::Duration::from_secs(1)))
        .build_server()?;

    broadcaster.set_scheduler(scheduler);

    broadcaster.start()?;
    
    //broadcaster.play("http://server35757.streamplus.de/stream.mp3")?;
    broadcaster.play("http://itcoops.de:8000/drumyourass.mp3")?;
    //broadcaster.play("https://icecast.radiobremen.de/rb/bremenvier/live/mp3/64/stream.mp3")?;
    //let _ = broadcaster.play("http://icecast.radiobremen.de/rb/bremenvier/live/mp3/128/stream.mp3");     

    // https://icecast.radiobremen.de/rb/bremenvier/live/mp3/64/stream.mp3

    // https://wdr-1live-live.sslcast.addradio.de/wdr/1live/live/mp3/128/stream.mp3

    //let bc_clone = broadcaster.clone();
    //glib::timeout_add(std::time::Duration::from_secs(60), move || {
    //    let _ = bc_clone.play("http://icecast.radiobremen.de/rb/bremenvier/live/mp3/128/stream.mp3");     
    //    Continue(true)
    //});



    main_loop.run();

    Ok(())
}
