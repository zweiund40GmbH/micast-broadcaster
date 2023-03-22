use gst_rtsp_server::subclass::prelude::*;
use gst::glib;

mod imp {
    use super::*;

    // This is the private data of our mount points
    #[derive(Default)]
    pub struct MountPoints {}

    // This trait registers our type with the GObject object system and
    // provides the entry points for creating a new instance and setting
    // up the class data
    #[glib::object_subclass]
    impl ObjectSubclass for MountPoints {
        const NAME: &'static str = "RsRTSPMountPoints";
        type Type = super::MountPoints;
        type ParentType = gst_rtsp_server::RTSPMountPoints;
    }

    // Implementation of glib::Object virtual methods
    impl ObjectImpl for MountPoints {}

    // Implementation of gst_rtsp_server::RTSPClient virtual methods
    impl RTSPMountPointsImpl for MountPoints {
        fn make_path(&self, url: &gst_rtsp::RTSPUrl) -> Option<glib::GString> {
            println!("Make path called for {url:?} ");
            self.parent_make_path(url)
        }
    }
}

glib::wrapper! {
    pub struct MountPoints(ObjectSubclass<imp::MountPoints>) @extends gst_rtsp_server::RTSPMountPoints;
}

impl Default for MountPoints {
    // Creates a new instance of our factory
    fn default() -> Self {
        glib::Object::new()
    }
}
