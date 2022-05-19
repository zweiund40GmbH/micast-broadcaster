use micast_broadcaster::{broadcast, Scheduler};

use chrono::prelude::*;

use log::{debug};

use glib;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    debug!("Start test Broadcaster");
    let _main_loop = glib::MainLoop::new(None, false);

    
    //let s = r#"<TimeTable>
    //        <spots uri="file:///Users/nico/project_micast/dev/micast-broadcaster/spots/rp1.mp3" start="2022-05-14T17:00:00Z" end="2022-06-01T23:59:00Z">
    //            <schedules start="19:24" end="23:59" weekdays="Mon,Tue" interval="2h"/>
    //        </spots>
    //        <spots uri="file:///Users/nico/project_micast/dev/micast-broadcaster/spots/sch15min.mp3" start="2022-05-14T17:00:00Z" end="2022-06-01T23:59:00Z">
    //            <schedules start="19:27" end="23:59" weekdays="Mon,Tue" interval="2h"/>
    //        </spots>
    //    </TimeTable>"#;
    //    
    //let scheduler = Scheduler::from_str(&s, Local::now())?;

    println!("spots timezone: {:?}",Local::now());

    let mut scheduler = Scheduler::new();
    scheduler.from_file("/Users/nico/project_micast/dev/micast-broadcaster/spots/timetable.xml")?;
    
    
    let mut broadcaster = broadcast::Builder::new()
        .set_server_ip("224.1.1.1")
        .set_clock_port(8555)
        .set_rtp_sender_port(5000)
        .set_rtcp_sender_port(5001)
        .set_rtcp_receive_port(5007)
        .build_server()?;

    broadcaster.start()?;

    broadcaster.schedule_next("https://server35757.streamplus.de/stream.mp3", broadcast::ScheduleState::AfterCurrent, None)?;
    //broadcaster.schedule_next("https://icecast.radiobremen.de/rb/bremenvier/live/mp3/128/stream.mp3", broadcast::ScheduleState::AfterCurrent, None)?;
    
    /*
    std::thread::sleep(std::time::Duration::from_millis(10000));
    broadcaster.set_server_address("224.1.2.1")?;

    std::thread::sleep(std::time::Duration::from_millis(2000));
    broadcaster.set_server_address("224.1.1.1")?;
    */
    
    //std::thread::sleep(std::time::Duration::from_millis(5000));
    //broadcaster.play_spot("file:///Users/nico/project_micast/dev/micast-broadcaster/spots/rp1.mp3")?;
    //std::thread::sleep(std::time::Duration::from_millis(5000));

    

    println!("start spot list:");
    loop {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        if !broadcaster.spot_is_running() {
            if let Ok(spot) = scheduler.next(Local::now()) {
                broadcaster.play_spot(&spot.uri)?;
            }
        }
    }

    //main_loop.run();

    Ok(())
}