use micast_broadcaster::broadcast;

use log::{debug};

use glib;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    debug!("Start test Broadcaster");
    let main_loop = glib::MainLoop::new(None, false);

    let broadcaster = broadcast::Builder::new()
        .set_server_ip("224.1.1.1")
        .set_clock_port(8555)
        .set_rtp_sender_port(5000)
        .set_rtcp_sender_port(5001)
        .set_rtcp_receive_port(5007)
        .build_server()?;

    
    broadcaster.schedule_next("file:///Users/nico/project_micast/dev/micast-dj/abfb_44100.mp3", broadcast::ScheduleState::AfterCurrent, None)?;
    std::thread::sleep(std::time::Duration::from_millis(2000));
    broadcaster.schedule_next("file:///Users/nico/project_micast/dev/micast-dj/m11.mp3", broadcast::ScheduleState::AfterCurrent, None)?;
    broadcaster.early_crossfade();

    
    broadcaster.schedule_next("https://icecast.radiobremen.de/rb/bremenvier/live/mp3/128/stream.mp3", broadcast::ScheduleState::AfterCurrent, None)?;
    
    std::thread::sleep(std::time::Duration::from_millis(20000));
    broadcaster.schedule_next("file:///Users/nico/project_micast/dev/micast-dj/m13.mp3", broadcast::ScheduleState::AfterCurrent, None)?;
    broadcaster.early_crossfade();
    
    


    
    //std::thread::sleep(std::time::Duration::from_millis(2000));
    //broadcaster.schedule_next("file:///Users/nico/project_micast/dev/micast-dj/abfb_44100.mp3", broadcast::ScheduleState::AfterCurrent, None)?;
    //broadcaster.schedule_next("file:///Users/nico/project_micast/dev/micast-dj/m13.mp3", broadcast::ScheduleState::AfterCurrent, None)?;

    std::thread::sleep(std::time::Duration::from_millis(2000));
    //broadcaster.early_crossfade();

    main_loop.run();

    Ok(())
}