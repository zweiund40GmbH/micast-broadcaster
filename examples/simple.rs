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
    scheduler.from_file("/Users/nico/project_micast/dev/micast-broadcaster/spots/timetable.xml")?;
    scheduler.load_files();
    
    let broadcaster = broadcast::Builder::new()
        //.set_server_ip("127.0.0.1")
        .set_server_ip("127.0.0.1")
        .set_broadcast_ip("127.0.0.1")
        //.set_audiorate(44100 / 2)
        .set_audiorate(44100)
        .set_spot_volume(0.3)
        .set_broadcast_volume(0.5)
        .set_crossfade_time(std::time::Duration::from_secs(1))
        .build_server()?;

    broadcaster.set_scheduler(scheduler);

    broadcaster.start()?;
    
    broadcaster.play("http://server35757.streamplus.de/stream.mp3")?;



    main_loop.run();

    Ok(())
}
