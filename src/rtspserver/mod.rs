
use gst_rtsp_server::prelude::*;
use gst::glib;
use log::{info, warn};

mod client;
mod media_factory;
mod media;
mod mount_points;
mod server;


pub struct RTPServer {
    server: server::Server,
    factory: media_factory::Factory,
    source_id: Option<glib::SourceId>,
}

impl std::fmt::Debug for RTPServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RTPServer")
            .field("server", &self.server)
            .field("factory", &self.factory)
            .field("source_id", &self.source_id)
            .finish()
    }
}

impl RTPServer {
    pub fn new(proxysink: &gst::Element, clock: &gst::Clock) -> Self {

        let server = server::Server::default();

        let mounts = mount_points::MountPoints::default();
        server.set_mount_points(Some(&mounts));

        //server.set_address("224.1.1.42");

        // Much like HTTP servers, RTSP servers have multiple endpoints that
        // provide different streams. Here, we ask our server to give
        // us a reference to his list of endpoints, so we can add our
        // test endpoint, providing the pipeline from the cli.
        let mounts = server.mount_points().unwrap();

        // Next, we create our custom factory for the endpoint we want to create.
        // The job of the factory is to create a new pipeline for each client that
        // connects, or (if configured to do so) to reuse an existing pipeline.
        let factory = media_factory::Factory::new(proxysink);
        // This setting specifies whether each connecting client gets the output
        // of a new instance of the pipeline, or whether all connected clients share
        // the output of the same pipeline.
        // If you want to stream a fixed video you have stored on the server to any
        // client, you would not set this to shared here (since every client wants
        // to start at the beginning of the video). But if you want to distribute
        // a live source, you will probably want to set this to shared, to save
        // computing and memory capacity on the server.
        //factory.set_suspend_mode(gst_rtsp_server::RTSPSuspendMode::None);
        //factory.set_stop_on_disconnect(false);
        factory.set_publish_clock_mode(gst_rtsp_server::RTSPPublishClockMode::Clock);
        //factory.set_latency(600);
        factory.set_shared(true);
        factory.set_clock(Some(clock));

        //let pool = gst_rtsp_server::RTSPAddressPool::new();

        //pool.add_range("224.1.1.43", "224.1.1.52", 5000, 5010, 16).unwrap();
        //factory.set_address_pool(Some(&pool));
        //factory.set_protocols(gst_rtsp::RTSPLowerTrans::UDP_MCAST);

        // Now we add a new mount-point and tell the RTSP server to serve the content
        // provided by the factory we configured above, when a client connects to
        // this specific path.
        mounts.add_factory("/micast-dj", factory.clone());



        RTPServer {
            server, 
            factory,
            source_id: None,
        }
    }

    pub fn start(&mut self, _base_time: Option<gst::ClockTime>) {
        if let Some(id) = self.source_id.take() {
            id.remove();
            self.source_id = None;
        }
        // should i set the basetime?!?
        //self.factory.set_basetime(base_time);
        if let Ok(id) = self.server.attach(None) {
            self.source_id = Some(id);

            info!(
                "Stream ready at rtsp://127.0.0.1:{}/test",
                self.server.bound_port()
            );

        } else {
            warn!("Failed to attach server to main context");
        }
    }

    pub fn stop(&mut self) {
        if let Some(id) = self.source_id.take() {
            id.remove();
            self.source_id = None;
        }

    }
}

