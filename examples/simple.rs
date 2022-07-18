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
        .set_server_ip("0.0.0.0")
        .set_broadcast_ip("224.1.1.10")
        //.set_audiorate(44100 / 2)
        .set_audiorate(44100)
        .build_server()?;

    broadcaster.set_scheduler(scheduler);

    broadcaster.start()?;
    
    broadcaster.play("http://server35757.streamplus.de/stream.mp3")?;



    main_loop.run();

    Ok(())
}
