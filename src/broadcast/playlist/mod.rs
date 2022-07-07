use log::{info, warn};
use gstreamer as gst;
use gst::prelude::*;
use crate::helpers::{make_element, upgrade_weak};

use crate::sleep_ms;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
};

#[derive(Debug)]
pub struct Playlist {
    pub playlist: gst::Element,
    #[allow(dead_code)]
    sink_bins: Arc<Mutex<HashMap<String, gst::Bin>>>,

    block_probe_ids: Arc<RwLock<HashMap<String, gst::PadProbeId>>>
}

impl Playlist {

    pub(crate) fn new(pipeline: &gst::Pipeline, mixer: &super::mixer_bin::Mixer, uris: Vec<&str>) -> Result<Playlist, anyhow::Error> {

        let string_uris: Vec<String> = uris.iter().map(|&s|s.into()).collect();

        let sink_bins = Arc::new(Mutex::new(HashMap::new()));
        let sink_bins_clone_add = sink_bins.clone();
        let sink_bins_clone_remove = sink_bins.clone();
        let sink_bins_clone_block = sink_bins.clone();

        let block_probe_ids = Arc::new(RwLock::new(HashMap::new()));
        let block_probe_ids_add = block_probe_ids.clone();
        let block_probe_ids_remove = block_probe_ids.clone();

        let playlist = make_element("uriplaylistbin", None)?;
        playlist.set_property("iterations", &0u32);
        playlist.set_property("uris", &string_uris);

        pipeline.add(&playlist)?;

        let pipeline_weak = pipeline.downgrade();
        let mixer = mixer.clone();

        playlist.connect_pad_added(move |_playlist, src_pad| {
            info!("connected_pad_added for playlist called");
            let pipeline = upgrade_weak!(pipeline_weak);
            let pad_name = src_pad.name();
    
            let sink = Self::create_bin(&pipeline).unwrap();

            let sink_pad = sink.static_pad("sink").unwrap();

            info!("block probe");
            let block = src_pad.add_probe(gst::PadProbeType::BLOCK | gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST, move |_pad, _probe_info| {
                gst::PadProbeReturn::Ok
            }).unwrap();

            src_pad.link(&sink_pad).unwrap();

            let sink_src_pad = sink.static_pad("src").unwrap();
            let _ = mixer.connect_new_sink(&sink_src_pad);

            sink_bins_clone_add.lock().unwrap().insert(pad_name.to_string(), sink);

            info!("wait before un-block...");
            sleep_ms!(500);

            src_pad.remove_probe(block);
            info!("connect playlist src with audioresample");
    
        });

        let pipeline_weak = pipeline.downgrade();
        playlist.connect_no_more_pads(move |p| {

            info!("no mire pads gets called");
            {
                let mut blocks = block_probe_ids_remove.write().unwrap();
                let sinks = sink_bins_clone_block.lock().unwrap();
                for (name, block_id) in blocks.drain() {
                    
                    if let Some(sink) = sinks.get(&name) {
                        info!("remove block from {}", name);
                        let pad = sink.static_pad("sink").unwrap();
                        pad.remove_probe(block_id);
                    }
                }
            }

        });


        let pipeline_weak = pipeline.downgrade();
        playlist.connect_pad_removed(move |_playlist, pad| {

            info!("remove playlist element");
            let pipeline = upgrade_weak!(pipeline_weak);
            

            // remove sink bin that was handling the pad
            let sink_bins = sink_bins_clone_remove.lock().unwrap();
            let sink = sink_bins.get(&pad.name().to_string()).unwrap();
            //pipeline.remove(sink).unwrap();
            //let _ = sink.set_state(gst::State::Null);
        });

        let (_,current_state,_) = pipeline.state(None);
        let _ = playlist.set_state(current_state);

        Ok(Playlist { playlist, sink_bins, block_probe_ids })
    }

    pub(crate) fn cleanup(&self ) {
        // we want to cleanup the playlist...
        info!("start cleanup");

        let sink_bins = self.sink_bins.lock().unwrap();
        for (_, sinkbin) in sink_bins.iter() {
            let pad = sinkbin.static_pad("src").unwrap();
            let _ = sinkbin.set_state(gst::State::Null);   

            //let block_id = pad.add_probe(gst::PadProbeType::BLOCK_DOWNSTREAM, move |_pad, _probe_info| {
            //    gst::PadProbeReturn::Ok
            //}).unwrap();
        }


    }

    fn create_bin(pipeline: &gst::Pipeline) -> Result<gst::Bin, anyhow::Error> {

        // create a BIN
        let bin = gst::Bin::new(None);
        let audioconvert = make_element("audioconvert", None)?;
        let audioresample = make_element("audioresample", None)?;
        let capsfilter = make_element("capsfilter", None)?;
        let caps = gst::Caps::builder("audio/x-raw")
            .field("rate", &44100i32)
            .field("channels", &2i32)
            .build();
        capsfilter.set_property("caps", &caps);     
        
        bin.add_many(&[&audioresample, &capsfilter, &audioconvert ])?;
        gst::Element::link_many(&[&audioresample, &audioconvert, &capsfilter ])?;

        let bin_srcpad = capsfilter.static_pad("src").unwrap();
        let ghost_srcpad = gst::GhostPad::with_target(Some("src"), &bin_srcpad)?;
        bin.add_pad(&ghost_srcpad)?;

        let bin_sinkpad = audioresample.static_pad("sink").unwrap();
        let ghost_sinkpad = gst::GhostPad::with_target(Some("sink"), &bin_sinkpad)?;
        bin.add_pad(&ghost_sinkpad)?;

        pipeline.add(&bin).unwrap();
        bin.sync_state_with_parent().unwrap();

        Ok(bin)
    }
}