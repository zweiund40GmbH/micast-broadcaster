
use gst::prelude::*;
use anyhow;

use crate::helpers::make_element;
use log::{debug, warn, info};

/// this is the mixer
/// 
/// 
#[derive(Debug, Clone)]
pub struct Mixer {
    inner: gst::Bin,
    elements: Vec<gst::Element>,
    name: &'static str
}


impl Mixer {

    /// #new
    ///
    /// creates a new mixer bin, add a converter on the output (src) tail
    /// by default mixer_element is a _audiomixer_ element
    pub fn new(name: &'static str, mixer_element: Option<&'static str>, caps: Option<gst::Caps>, with_queue: bool) -> Result<Self, anyhow::Error> {
        
        let bin = gst::Bin::new(Some(name));
        let mut pipe = Vec::new(); 

        // general mixer
        // one could be the sink for noise (silence) to keep runnin
        // one input (sink channel) is the "spot"
        // the other is the "stream"
        let mixer = make_element(mixer_element.unwrap_or("audiomixer"), Some("master_mixer"))?;
        bin.add(&mixer)?;
        
        mixer.connect_pad_removed(move |element, pad| {
            debug!("mixer pad {:?} gets remove element {:?}", pad, element)
        });

        pipe.push(mixer);

        if let Some(caps) = caps {
            debug!("add caps to mixer");
            let converter = make_element("audioconvert", Some("master_mixer__converter"))?;
            bin.add(&converter)?;
            pipe.push(converter);

            let capsfilter = make_element("capsfilter", Some("master_mixer__capsfilter"))?; 
            capsfilter.try_set_property("caps",&caps)?;
            bin.add(&capsfilter)?;
            pipe.push(capsfilter);
        }

        if with_queue {
            debug!("add queue to mixer {}", name);
            let queue = make_element("queue", Some("master_mixer__queue"))?; 
            bin.add(&queue)?;
            pipe.push(queue);
        }


        debug!("linking all mixer elements...");
        gst::Element::link_many(pipe.iter().map(|e| e).collect::<Vec<&gst::Element>>().as_slice())?;

        for element in pipe.iter() {
            element.sync_state_with_parent()?;
        }


        debug!("create ghostpad for mixer bin {}", name); 
        let last_element = &pipe[pipe.len() - 1];
        let src_pad = gst::GhostPad::with_target(Some("src"), &last_element.static_pad("src").unwrap())?;
        bin.add_pad(&src_pad)?;

        Ok(Mixer {
            inner: bin,
            elements: pipe,
            name,
        })
    }



    /// request_new_sink
    ///
    /// make a pad request for mixer_element to generate a sink
    pub fn request_new_sink(&self) -> Option<(gst::Pad, gst::Pad)> {
        if let Some(sink_pad_from_mixer) = self.elements[0].request_pad_simple("sink_%u") {
            debug!("got new sinkpad from {} : {}", self.name, &sink_pad_from_mixer.name());
            if let Ok(sink_pad) = gst::GhostPad::with_target(Some(&sink_pad_from_mixer.name()), &sink_pad_from_mixer) {
                if self.inner.add_pad(&sink_pad).is_err() {
                    return None
                }
                info!("add_pad to {}", self.name);

                let converted_pad: gst::Pad = sink_pad.upcast::<gst::Pad>();
                return Some((converted_pad, sink_pad_from_mixer))
            }
        }
        None
    }

    /// set volume
    /// 
    /// sets the volume for a specific mixer pad
    pub(super) fn set_volume(&self, volume: f64, pad: gst::Pad) {
        let ghost_pad: gst::GhostPad = pad.downcast().unwrap();
        let real_pad = ghost_pad.target().unwrap();

        if let Err(e) = real_pad.try_set_property("volume", volume){
            warn!("could not set volume for spot: {:?}", e);
        }
    }

    pub fn release_pad(&self, pad: gst::Pad) {

        // upcast this pad to the real ghostpad

        let ghost_pad: gst::GhostPad = pad.downcast().unwrap();
        let real_pad = ghost_pad.target().unwrap();

        if let Err(e) = self.inner.remove_pad(&ghost_pad) {
            warn!("error remove ghost_pad: {:#?}", e);
        }

        self.elements[0].release_request_pad(&real_pad);

        //if let Err(e) = self.elements[0].remove_pad(&real_pad) {
        //    warn!("error on removing pad from mixer: {:?}", e);
        //}
    }

    /// connect a src to a new generated sink on this mixer
    ///
    /// - `target` is a pad from a src which should connected to this mixer
    ///
    /// return the new generated pad
    pub fn connect_new_sink(&self, target: &gst::Pad) -> Result<gst::Pad, anyhow::Error> {
       
        if let Some((new_sink, _)) = self.request_new_sink() {
           
            target.link(&new_sink)?;
            return Ok(new_sink)
        }


        anyhow::bail!("Error on connect sink {} to target {}", self.name, target.name())
    }

    /// #src_pad
    ///
    /// returns the inner bin src pad (output)
    pub fn src_pad(&self) -> Option<gst::Pad> {
        self.inner.static_pad("src")
    }


    pub fn link_pads(&self, target: Option<&str>, element: &gst::Element, element_pad: Option<&str>) -> Result<(), anyhow::Error> {
        self.inner.link_pads(target, element, element_pad)?;    

        Ok(())
    }

    /// #add_to_pipeline
    ///
    /// ad this mixer to the pipeline
    pub fn add_to_pipeline(&self, pipeline: &gst::Pipeline) -> Result<(), anyhow::Error> {
       pipeline.add(&self.inner)?;
       Ok(())
    }

    pub fn get_mixer_pad(&self) -> Option<gst::Pad> {
        self.elements[0].static_pad("src")
    }
}

