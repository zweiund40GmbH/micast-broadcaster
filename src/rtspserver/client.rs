use gst_rtsp_server::subclass::prelude::*;
use gst::glib;

// In the imp submodule we include the actual implementation
mod imp {
    use super::*;

    // This is the private data of our server
    #[derive(Default)]
    pub struct Client {}

    // This trait registers our type with the GObject object system and
    // provides the entry points for creating a new instance and setting
    // up the class data
    #[glib::object_subclass]
    impl ObjectSubclass for Client {
        const NAME: &'static str = "RsRTSPClient";
        type Type = super::Client;
        type ParentType = gst_rtsp_server::RTSPClient;
    }

    // Implementation of glib::Object virtual methods
    impl ObjectImpl for Client {}

    // Implementation of gst_rtsp_server::RTSPClient virtual methods
    impl RTSPClientImpl for Client {
        fn closed(&self) {
            let client = self.obj();
            self.parent_closed();
            println!("Client {client:?} closed");
        }
    }
}

// This here defines the public interface of our factory and implements
// the corresponding traits so that it behaves like any other RTSPClient
glib::wrapper! {
    pub struct Client(ObjectSubclass<imp::Client>) @extends gst_rtsp_server::RTSPClient;
}

impl Default for Client {
    // Creates a new instance of our factory
    fn default() -> Client {
        glib::Object::new()
    }
}
