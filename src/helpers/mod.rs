use gstreamer as gst;
//use gst::prelude::*;
use anyhow::{anyhow};

// upgrade weak reference or return
macro_rules! upgrade_weak {
    ($x:ident, $r:expr) => {{
        match $x.upgrade() {
            Some(o) => o,
            None => return $r,
        }
    }};
    ($x:ident) => {
        upgrade_weak!($x, ())
    };
}

// simple thread sleep helper
#[macro_export]
macro_rules! sleep_ms {
    ($r:expr) => {{
        std::thread::sleep(std::time::Duration::from_millis($r));
    }};
}

pub(crate) use upgrade_weak;

// help make to help faster elements
pub fn make_element(
    factory_name: &'static str,
    element_name: Option<&str>,
) -> Result<gst::Element, anyhow::Error> {
    match gst::ElementFactory::make(factory_name, element_name) {
        Ok(elem) => Ok(elem),
        Err(_) => { 
            Err(anyhow!("Missing element: {}", factory_name))
        }
    }
}
