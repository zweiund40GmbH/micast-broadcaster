use log::{debug,warn};
use gstreamer as gst;
use gst::prelude::*;
use crate::helpers::{make_element, upgrade_weak};


#[derive(Debug)]
pub(crate) struct Silence {
    decoder: gst::Bin,
    pub audio_pad: gst::Pad,
}

impl Silence {
    pub fn new(pipeline: &gst::Pipeline) -> Result<Silence, anyhow::Error> {
        
        let bin = gst::Bin::new(Some("Silencer"));
        
        let src = make_element("audiotestsrc", None)?;
        src.set_property("num-buffers", &400)?;
        //src.set_property("is-live", &true)?;
        src.set_property("volume", &0.8)?;
        /*let pw = src.find_property("wave").unwrap();
        let a = glib::EnumClass::new(pw.value_type()).unwrap().value(4).unwrap().to_value();

        src.set_property("wave",a)?;*/
        
        bin.add(&src)?;

        let queue = make_element("queue", Some("fade-queue-%u")).unwrap();
        queue.set_property("max-size-buffers", 0 as u32)?;
        queue.set_property("max-size-time", &(2 * crate::CROSSFADE_TIME_MS * gst::ClockTime::MSECOND.mseconds()))?;
    
        //bin.add(&queue)?;

        let audioresample = make_element("audioresample", None)?;
        let audioconvert = make_element("audioconvert", None)?;
        let capsfilter = make_element("capsfilter", None)?;
        
        bin.add_many(
            &[
                &audioresample, 
                &audioconvert, 
                &capsfilter
            ]
        )?;
    
        gst::Element::link_many(
            &[
                &src, 
                &audioresample, 
                &audioconvert, 
                &capsfilter,
                //&queue,
            ]
        )?;
        
        //src.sync_state_with_parent()?;
        audioresample.sync_state_with_parent()?;
        audioconvert.sync_state_with_parent()?;
        capsfilter.sync_state_with_parent()?;
        //queue.sync_state_with_parent()?;
        
        let srcpad = capsfilter.static_pad("src").unwrap();
        let audio_pad = gst::GhostPad::with_target(Some("src"), &srcpad)?;
    
        audio_pad.set_active(true)?;
        bin.add_pad(&audio_pad)?;
        
        pipeline.add(&bin)?;
        
        let pad = bin.static_pad("src").unwrap();

        Ok(Silence {
            decoder: bin,
            audio_pad: pad,
        })
    }
}