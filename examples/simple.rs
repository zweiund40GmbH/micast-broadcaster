use async_std::prelude::*;
use micast_broadcaster::{broadcast, scheduler::Scheduler };

use chrono::prelude::*;

use log::{debug, warn};


use std::error::Error;


#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let mut scheduler = Scheduler::new();
    scheduler.from_file("/Users/nico/project_micast/dev/micast-broadcaster/spots/timetable.xml")?;
    scheduler.load_files();
    
    //broadcaster.set_playlist(vec![
    //    "file:///Users/nico/project_micast/dev/micast-dj/abfb.mp3", 
    //    "http://sunshinelive.hoerradar.de/sunshinelive-live-mp3-hq",
    //    "file:///Users/nico/project_micast/dev/micast-dj/m1.mp3", 
    //    "file:///Users/nico/project_micast/dev/micast-dj/m2.mp3", 
    //])?;

    let broadcaster = broadcast::Broadcaster::new()?;

    broadcaster.play("http://sunshinelive.hoerradar.de/sunshinelive-live-mp3-hq".to_string());


    debug!("at the end");
    Ok(())
}
