
use gst::prelude::*;
use crate::helpers::*;

//use log::{debug, info};

use std::fmt;

#[allow(dead_code)]
pub fn create_bin<T: Into<String> + Clone + fmt::Debug + fmt::Display>( 
    audio_device: Option<T>,
) -> Result<gst::Bin, anyhow::Error,> {

    //info!("setup gstbin for Local Output audio device: {:?}", audio_device.clone().unwrap_or(String::from("autodedected")));

    let bin = gst::Bin::new(Some("local_output"));

    let resample = make_element("audioresample", None)?;
    bin.add(&resample)?;
    
    let converter = make_element("audioconvert", None)?;
    bin.add(&converter)?;

    resample.link(&converter)?;

    let capfilter = make_element("capsfilter", None)?;
    bin.add(&capfilter)?;

    converter.link(&capfilter)?;

    let audiosink = if let Some(audiodevice) = &audio_device {
        let a = make_element("alsasink", Some("audiosink"))?;
        let device: String = audiodevice.to_string();
        a.set_property("device",device);
        a
    } else {
        make_element("autoaudiosink", Some("audiosink"))?
    };

    bin.add(&audiosink)?;
    capfilter.link(&audiosink)?;

    let ghost_pad = gst::GhostPad::with_target(Some("sink"), &resample.static_pad("sink").unwrap())?;
    bin.add_pad(&ghost_pad)?;

    Ok(bin)
}