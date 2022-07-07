use micast_broadcaster::{broadcast, scheduler::Scheduler };

use chrono::prelude::*;

use log::{debug, warn};

use glib::{self, Continue};

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    debug!("Start test Broadcaster");
    let main_loop = glib::MainLoop::new(None, false);


    let mut scheduler = Scheduler::new();
    scheduler.from_file("/Users/nico/project_micast/dev/micast-broadcaster/spots/timetable.xml")?;
    scheduler.load_files();
    
    let broadcaster = broadcast::Builder::new()
        .set_server_ip("127.0.0.1")
        .build_server()?;

    

    broadcaster.start()?;
    broadcaster.set_playlist(vec![
        "file:///Users/nico/project_micast/dev/micast-dj/abfb.mp3", 
        "http://sunshinelive.hoerradar.de/sunshinelive-live-mp3-hq",
        "file:///Users/nico/project_micast/dev/micast-dj/m1.mp3", 
        "file:///Users/nico/project_micast/dev/micast-dj/m2.mp3", 
    ])?;


    let mut only_onetime = true;
    let bc = broadcaster.clone();
    glib::timeout_add(std::time::Duration::from_millis(15000), move || {

        if only_onetime {
            only_onetime = false;
            let _ = bc.set_playlist(vec![
                "file:///Users/nico/project_micast/dev/micast-dj/m1.mp3", 
                "file:///Users/nico/project_micast/dev/micast-dj/m2.mp3", 
            ]);
        }

        Continue(true)
    });

    glib::timeout_add(std::time::Duration::from_millis(5000), move || {

        //broadcaster.print_graph();
        if !broadcaster.spot_is_running() {
            if let Ok(spot) = scheduler.next(Local::now()) {
                if let Err(e) = broadcaster.play_spot(&spot.uri, Some(0.8)) {
                    warn!("error on play next spot... {:?}", e);
                }
            }
        }
        Continue(true)
    });


    main_loop.run();

    Ok(())
}
