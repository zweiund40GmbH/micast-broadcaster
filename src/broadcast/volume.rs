/// simple Volume Control helper
///
///
///

use gstreamer_controller as gst_controller;
use gst_controller::prelude::*;
use anyhow;

use gstreamer as gst;

#[derive(Debug)]
pub struct Control{
    source: gst_controller::InterpolationControlSource,
}


impl Control {

    /// # new
    ///
    /// intantiate simple Control Source for property Control
    pub fn new() -> Control {
        let controller = gst_controller::InterpolationControlSource::new();
        controller.set_mode(gst_controller::InterpolationMode::Linear);

        Control {
            source: controller,
        }
    }
    
    /// # attach_to
    ///
    /// Attach a Property to This Control struct
    ///
    /// - `pad` the pad which we want to control
    /// - `property` the property which we want to manipulate
    pub fn attach_to(&self, pad: &gst::Pad, property: &str ) -> Result<(), anyhow::Error> {
        let dcb = gst_controller::DirectControlBinding::new_absolute(pad, property, &self.source);
        pad.add_control_binding(&dcb)?;
        Ok(())
    }
   
    /// # set_value
    ///
    /// set a value in a given time from a specific value to a specific value
    ///
    /// - `from` an float 64 value 
    /// - `to`an float 64 value
    /// - `duration` the duration as _gst::ClockTime_ where the value gets from _from_ to _to_
    /// - `current_time` is the current running time of the pipeline / stream / element / mixer...
    pub fn set_value(&self, from: f64, to: f64, duration: gst::ClockTime, current_time: gst::ClockTime) {
        let a = self.source.upcast_ref::<gst_controller::TimedValueControlSource>();
        a.set(current_time, from);
        a.set(current_time + duration, to);
    }
}
