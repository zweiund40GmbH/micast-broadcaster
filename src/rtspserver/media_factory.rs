use gst_rtsp_server::prelude::*;
use gst_rtsp_server::subclass::prelude::*;
use gst::glib;
use gst::prelude::*;

use std::sync::Mutex;

use super::*;

// In the imp submodule we include the actual implementation
mod imp {
    use super::*;

    // This is the private data of our factory
    #[derive(Default)]
    pub struct Factory {
        pub(super) proxysink: Mutex<Option<gst::Element>>,
    }


    // This trait registers our type with the GObject object system and
    // provides the entry points for creating a new instance and setting
    // up the class data
    #[glib::object_subclass]
    impl ObjectSubclass for Factory {
        const NAME: &'static str = "RsRTSPMediaFactory";
        type Type = super::Factory;
        type ParentType = gst_rtsp_server::RTSPMediaFactory;
    }

    // Implementation of glib::Object virtual methods
    impl ObjectImpl for Factory {
        fn constructed(&self) {
            self.parent_constructed();

            let factory = self.obj();
            // All media created by this factory are our custom media type. This would
            // not require a media factory subclass and can also be called on the normal
            // RTSPMediaFactory.
            factory.set_media_gtype(super::media::Media::static_type());
            println!("factory constructed");
        }
    }

    // Implementation of gst_rtsp_server::RTSPMediaFactory virtual methods
    impl RTSPMediaFactoryImpl for Factory {
        fn create_element(&self, _url: &gst_rtsp::RTSPUrl) -> Option<gst::Element> {
            // Create a simple VP8 videotestsrc input
            let bin = gst::Bin::default();

            let proxysink_lock = self.proxysink.lock().unwrap();
            let proxysink = proxysink_lock.clone().unwrap();
            let proxysrc = gst::ElementFactory::make_with_name("proxysrc", None).unwrap();
            let conv = gst::ElementFactory::make_with_name("audioconvert", None).unwrap();
            let pay = gst::ElementFactory::make_with_name("rtpL24pay", Some("pay0")).unwrap();

            proxysrc.set_property("proxysink", &proxysink);
            bin.add_many(&[&proxysrc, &conv, &pay]).unwrap();
            gst::Element::link_many(&[&proxysrc, &conv, &pay]).unwrap();

            println!("element created");
            Some(bin.upcast())
        }
    }
}

// This here defines the public interface of our factory and implements
// the corresponding traits so that it behaves like any other RTSPMediaFactory
glib::wrapper! {
    pub struct Factory(ObjectSubclass<imp::Factory>) @extends gst_rtsp_server::RTSPMediaFactory;
}

impl Default for Factory {
    // Creates a new instance of our factory
    fn default() -> Factory {
        glib::Object::new()
    }
}

impl Factory {
    pub fn new(proxysink: &gst::Element) -> Factory {
        let factory: Factory = glib::Object::new();

        let imp = factory.imp();
        *imp.proxysink.lock().unwrap() = Some(proxysink.clone());

        factory
    }
}