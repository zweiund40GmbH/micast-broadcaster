use gst_rtsp_server::subclass::prelude::*;
use gst_rtsp_server::prelude::*;
use gst::prelude::*;
use gst::glib;

use super::*;

// In the imp submodule we include the actual implementation
mod imp {
    use super::*;

    // This is the private data of our server
    #[derive(Default)]
    pub struct Server {}

    // This trait registers our type with the GObject object system and
    // provides the entry points for creating a new instance and setting
    // up the class data
    #[glib::object_subclass]
    impl ObjectSubclass for Server {
        const NAME: &'static str = "RsRTSPServer";
        type Type = super::Server;
        type ParentType = gst_rtsp_server::RTSPServer;
    }

    // Implementation of glib::Object virtual methods
    impl ObjectImpl for Server {}

    // Implementation of gst_rtsp_server::RTSPServer virtual methods
    impl RTSPServerImpl for Server {
        fn create_client(&self) -> Option<gst_rtsp_server::RTSPClient> {
            let server = self.obj();
            let client = super::client::Client::default();

            // Duplicated from the default implementation
            client.set_session_pool(server.session_pool().as_ref());
            client.set_mount_points(server.mount_points().as_ref());
            client.set_auth(server.auth().as_ref());
            client.set_thread_pool(server.thread_pool().as_ref());

            Some(client.upcast())
        }

        fn client_connected(&self, client: &gst_rtsp_server::RTSPClient) {
            self.parent_client_connected(client);
            println!("Client {client:?} connected");
        }
    }
}

// This here defines the public interface of our factory and implements
// the corresponding traits so that it behaves like any other RTSPServer
glib::wrapper! {
    pub struct Server(ObjectSubclass<imp::Server>) @extends gst_rtsp_server::RTSPServer;
}

impl Default for Server {
    // Creates a new instance of our factory
    fn default() -> Server {
        glib::Object::new()
    }
}
