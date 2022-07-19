/// simple Volume Control helper
///
///
///

use gst_controller::prelude::*;
use parking_lot::{Mutex};
use anyhow::{Result};

#[derive(Debug)]
struct PadProperty {
    pad: gst::Pad, 
    property: String
}

#[derive(Debug)]
pub struct Control{
    source: gst_controller::InterpolationControlSource,
    padprop: Mutex<Option<PadProperty>>,
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
            padprop: Mutex::new(None)
        }
    }
    
    /// # attach_to
    ///
    /// Attach a Property to This Control struct
    ///
    /// - `pad` the pad which we want to control
    /// - `property` the property which we want to manipulate
    pub fn attach_to(&self, pad: &gst::Pad, property: &str ) -> Result<()> {
        let dcb = gst_controller::DirectControlBinding::new_absolute(pad, property, &self.source);
        pad.add_control_binding(&dcb)?;

        let mut a = self.padprop.lock();
        *a = Some(PadProperty{
            pad: pad.clone(),
            property: property.to_string(),
        });
        drop(a);
        
        

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
    pub fn set_value(&self, from: Option<f64>, to: f64, duration: gst::ClockTime, current_time: gst::ClockTime) {
        let a = self.source.upcast_ref::<gst_controller::TimedValueControlSource>();
        
        let prop_guard = self.padprop.lock();
        
        let prop = prop_guard.as_ref().unwrap();
        let current_value:f64 = prop.pad.property(&prop.property);

        a.set(current_time, from.unwrap_or(current_value));
        a.set(current_time + duration, to);
    }
}
