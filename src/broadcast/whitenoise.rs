
/// this struct works like endless white noise
///
///

use gst::prelude::*;

use anyhow::{bail, Result};
use crate::{helpers::make_element};

use log::debug;

/// silence is gold
///
/// simple silence src to provide a continues stream of data, even if it is silence
#[derive(Debug)]
pub struct Silence {
   inner: gst::Bin, 
}


impl Silence {
    pub fn new(rate: i32) -> Result<Silence> {

        let bin = gst::Bin::new(None);

        let tsrc = make_element("audiotestsrc", None)?;
        tsrc.set_property_from_str("wave", "silence");
        //tsrc.set_property_from_str("wave", "sine");
        //tsrc.set_property_from_str("wave", "silence");
        //tsrc.set_property_from_str("wave", "ticks");
        //tsrc.set_property("freq", 2000.0f64.to_value())?;
        tsrc.set_property_from_str("freq", "2000");
        tsrc.try_set_property("is-live", &false)?;

        bin.add(&tsrc)?;

    
        let capsfilter = make_element("capsfilter", None)?;

        let caps = gst::Caps::builder("audio/x-raw")
            .field("rate", &rate)
            .field("channels", &2i32)
            .build();
        capsfilter.try_set_property("caps", &caps)?;     
        
        bin.add(&capsfilter)?;
        tsrc.link(&capsfilter)?;

        let queue = make_element("queue", None)?;

        bin.add(&queue)?;
        capsfilter.link(&queue)?;

        
        let srcpad = queue.static_pad("src").unwrap();

        //methods::pad_helper::running_time(&srcpad, |clock| {
        //    debug!("sets runnging kk: {}", clock);
        //})?;

        let audio_pad = gst::GhostPad::with_target(Some("src"), &srcpad)?;
        audio_pad.set_active(true)?;
        bin.add_pad(&audio_pad)?;
        
        debug!("silence is prepared");

        Ok(Silence {
           inner: bin, 
        })
    }

    ///
    /// attach this silence to a mixer
    ///
    /// return an error if not successfull
    pub fn attach_to_mixer(&self, mixer: &super::mixer_bin::Mixer) -> Result<()>  {
        if let Some(static_pad) = self.inner.static_pad("src") {
            debug!("attach silence to mixer {}", static_pad.name());
            mixer.connect_new_sink(&static_pad)?;
            return Ok(())
        }

        bail!("couldnt attach silence to mixer!")
    }

    /// add this mixer to the pipeline
    ///
    pub fn add_to_pipeline(&self, pipeline: &gst::Pipeline) -> Result<()> {
       pipeline.add(&self.inner)?;
       Ok(())
    }
}
