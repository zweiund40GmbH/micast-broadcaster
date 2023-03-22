use micast_broadcaster::{broadcast, output, rtspserver, services};

use gst_app::prelude::*;

use log::debug;

use gst::glib;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    gst::init()?;

    debug!("Start test Broadcaster");
    let main_loop = glib::MainLoop::new(None, false);

    let clock = gst::SystemClock::obtain();
    //debug!("add net clock server {} port {}", server_ip, clock_port);
    let net_clock = gst_net::NetTimeProvider::new(&clock, None, 8555)?;
    clock.set_property("clock-type", &gst::ClockType::Realtime);

    let pipeline = gst::Pipeline::new(None);
    pipeline.use_clock(Some(&clock));
    
    services::clock_server::service()?;

    let maincaps = gst::Caps::builder("audio/x-raw")
        .field("format", &"F32LE")
        .field("rate", &44100i32)
        .field("channels", &2i32)
        .field("layout", &"interleaved")
        .build();
        
    let src = gst::ElementFactory::make_with_name("appsrc", None).unwrap();
    src.set_property("is-live", &true);
    src.set_property("block", &false);
    src.set_property("format", &gst::Format::Time);
    src.set_property("caps", &maincaps);

    let appsrc = src
            .dynamic_cast::<gst_app::AppSrc>()
            .expect("Source element is expected to be an appsrc!");
    pipeline.add(&appsrc).unwrap();

    let sink = gst::ElementFactory::make_with_name("proxysink", None).unwrap();
    let _ = pipeline.add(&sink);
    let _ = appsrc.link(&sink);

    let mut server = rtspserver::RTPServer::new(&sink, &clock);
    //server.set_server_ip("224.1.1.42");

    let mut output = output::Output::new_from_rtspserver(&appsrc, "https://icecast.radiobremen.de/rb/bremenvier/live/mp3/64/stream.mp3", Some("./spots/pocking_timetable.xml".to_string()));
    output.run();

    pipeline.set_state(gst::State::Playing).unwrap();
    server.start(None);
    main_loop.run();

    Ok(())
}
