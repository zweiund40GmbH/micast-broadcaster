use gst_rtsp_server::subclass::prelude::*;

use gst::glib;

// In the imp submodule we include the actual implementation
mod imp {
    use super::*;

    // This is the private data of our media
    #[derive(Default)]
    pub struct Media {}

    // This trait registers our type with the GObject object system and
    // provides the entry points for creating a new instance and setting
    // up the class data
    #[glib::object_subclass]
    impl ObjectSubclass for Media {
        const NAME: &'static str = "RsRTSPMedia";
        type Type = super::Media;
        type ParentType = gst_rtsp_server::RTSPMedia;
    }

    // Implementation of glib::Object virtual methods
    impl ObjectImpl for Media {}

    // Implementation of gst_rtsp_server::RTSPMedia virtual methods
    impl RTSPMediaImpl for Media {
        fn setup_sdp(
            &self,
            sdp: &mut gst_sdp::SDPMessageRef,
            info: &gst_rtsp_server::subclass::SDPInfo,
        ) -> Result<(), gst::LoggableError> {
            self.parent_setup_sdp(sdp, info)?;

            sdp.add_attribute("my-custom-attribute", Some("has-a-value"));

            Ok(())
        }
    }
}

// This here defines the public interface of our factory and implements
// the corresponding traits so that it behaves like any other RTSPMedia
glib::wrapper! {
    pub struct Media(ObjectSubclass<imp::Media>) @extends gst_rtsp_server::RTSPMedia;
}
