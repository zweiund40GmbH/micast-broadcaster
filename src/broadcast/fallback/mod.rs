use gst::glib;
use gst::prelude::*;
use log::{debug,warn,info};
use crate::helpers::{make_element, upgrade_weak};
use parking_lot::{Mutex, RwLock};
use anyhow::{anyhow, Result};
use std::sync::{Arc, Weak};
use crate::sleep_ms;

#[derive(Clone, PartialEq, Debug)]
enum CurrentState {
    DoNothing,
    PlaySource,
    Retry,
    WaitForDecoderSrcPad,
    ChangeUri,
}


#[derive(Clone, PartialEq, Debug)]
enum ErrorState {
    None,
    WatchdogError,
    NetworkError,
}


#[derive(Debug)]
struct State {
    uri: Option<String>,
    source: Option<gst::Element>,
    converter_bin: gst::Bin,
    watchdog: gst::Element,
    bin_src: gst::Pad,

    source_pad: Option<gst::Pad>,
    has_mixer_connected: bool,
    current_state: CurrentState,
    error_state: ErrorState,
}


#[derive(Clone)]
pub(crate) struct FallbackInner {
    pub(crate) bin: gst::Bin,
    mixer: super::mixer_bin::Mixer,
    state: Arc<Mutex<State>>,
    //silence: gst::Element,
    
    rate: Option<i32>,
    pipeline: gst::Pipeline,
    running_time: Arc<RwLock<gst::ClockTime>>,
}

#[derive(Clone)]
pub(crate) struct Fallback(Arc<FallbackInner>);

use std::fmt;
impl fmt::Debug for Fallback {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Fallback Item with URL")
    }
}


#[derive(Clone)]
pub(crate) struct FallbackWeak(Weak<FallbackInner>);

impl std::ops::Deref for Fallback {
    type Target = FallbackInner;

    fn deref(&self) -> &FallbackInner {
        &self.0
    }
}

impl FallbackWeak {
    // Try upgrading a weak reference to a strong one
    pub fn upgrade(&self) -> Option<Fallback> {
        self.0.upgrade().map(Fallback)
    }
}


impl Fallback {

    // Downgrade the strong reference to a weak reference
    pub fn downgrade(&self) -> FallbackWeak {
        FallbackWeak(Arc::downgrade(&self.0))
    }

    pub fn new(pipeline: &gst::Pipeline, mixer: &super::mixer_bin::Mixer) -> Result<Fallback> {

        // setup bin
        let bin = gst::Bin::new(Some("fallbackbin"));

        let (converter_bin, watchdog) = create_converter_bin(Some("fallbackconvertbin"), None)?;
        bin.add(&converter_bin)?;

        let src_pad = gst::GhostPad::with_target(
            Some("src"), 
            &converter_bin.static_pad("src").unwrap()
        )?;

        bin.add_pad(&src_pad)?;

        let bin_src = bin.static_pad("src").unwrap();

        pipeline.add(&bin)?;

        let state = State { 
            uri: None,
            source: None,
            converter_bin,
            watchdog,
            bin_src: bin_src.clone(),
            source_pad: None,
            current_state: CurrentState::DoNothing,
            error_state: ErrorState::None,
            has_mixer_connected: false,
        };

        

        let fallback = Fallback(Arc::new(FallbackInner {
            mixer: mixer.clone(),
            rate: None,
            bin,
            //silence,
            running_time: Arc::new(RwLock::new(gst::ClockTime::ZERO)),
            state: Arc::new(Mutex::new(state)),
            pipeline: pipeline.clone(),
        }));

        Ok(fallback)

    }

    pub fn triggered_error_from_bus(&self) -> Result<()> {
        
        let mut state = self.state.lock();
        info!("triggered an error from bus, current_state: {:?}, error_state: {:?} pipeline state: {:#?}", state.current_state, state.error_state, self.pipeline.state(None));
        state.error_state = ErrorState::NetworkError;
        
        // at first disable watchdog

        state.watchdog.set_property("timeout", &0i32);
        //if CurState::HandleCurlError == state.pl_state {
        //    info!("triggered error from bus, but state is already in handleError mode, skipping");
        //    return Ok(())
        //}

        state.error_state = ErrorState::NetworkError;


        if state.has_mixer_connected {
            self.mixer.release_pad(state.bin_src.peer().unwrap());
            state.has_mixer_connected = false;
        }
        


        drop(state);
        sleep_ms!(5000);

        let weak_self = self.downgrade();
        self.bin.call_async(move |_bin| {
            
            // hier sollten wir erstmal bin auf paused // null setzen
            //let state_change_result = bin.set_state(gst::State::Null);
            //if let Ok(result) = state_change_result {
            //    if result == gst::StateChangeSuccess::Success {
            //        info!("on triggered_error_from_bus, before start again, we set the bin state successfull to Null");
            //    } else {
            //        warn!("on triggered_error_from_bus, before start again, we not successfull set state to Null :(");
            //    }
            //}
            
            let this = upgrade_weak!(weak_self);




            info!("try async restart...");
            if let Err(e) = this.start(None) {
                warn!("error on retry : {}", e)
            }
        });
        //self.handle_error()?;

        Ok(())
    }

    pub fn triggered_watchdog(&self) -> Result<()> {
        

        let mut state = self.state.lock();

        info!("triggered an error from watchdog, current_state: {:?}, error_state: {:?} pipeline state: {:#?}", state.current_state, state.error_state, self.pipeline.state(None));
    
        // at first disable watchdog
        state.watchdog.set_property("timeout", &0i32);
        state.error_state = ErrorState::WatchdogError;
        state.current_state = CurrentState::Retry;

        if state.has_mixer_connected {
            self.mixer.release_pad(state.bin_src.peer().unwrap());
            state.has_mixer_connected = false;
        }
        
        drop(state);

        sleep_ms!(2000);

        let weak_self = self.downgrade();

        self.bin.call_async(move |_bin| {

            // hier sollten wir erstmal bin auf paused // null setzen
            //let state_change_result = bin.set_state(gst::State::Null);
            //if let Ok(result) = state_change_result {
            //    if result == gst::StateChangeSuccess::Success {
            //        info!("on triggered_watchdog, before start again, we set the bin state successfull to Null");
            //    } else {
            //        warn!("on triggered_watchdog, before start again, we not successfull set state to Null :(");
            //    }
            //}
            
            let this = upgrade_weak!(weak_self);

            info!("try async restart...");
            if let Err(e) = this.start(None) {
                warn!("error on retry : {}", e)
            }
        });
        
        
        //let _ = self.handle_error();
        Ok(())

        //if CurState::WaitForDecoderSrcPad == state.pl_state {
        //    drop(state);
        //    warn!("okay, watchdog gets triggered and we wait for the decodersrcpad... let us try");
        //    let _ = self.handle_error();
        //}
        //Err(anyhow!("Not in PlaySource State, Watchdog error can not triggered"))
    }


    


    fn set_watchdog(&self, enabled: bool) {
        let state = self.state.lock();
        if enabled {
            state.watchdog.set_property("timeout", &4000i32);
        } else {
            state.watchdog.set_property("timeout", &0i32);
        }
        drop(state);
    }
    

    pub fn stop_playback(&self) -> Result<()> {
        info!("stop current playback of stream");

        let state = self.state.lock();

        if let Some(source) = state.source.as_ref() {
            
            let sourcepad = state.source_pad.clone();
            if sourcepad.is_none() {
                return Err(anyhow!("cannot remove stream cause Sourcepad is empty"));
            }

            // we crate a probe for triggering an EOS and call this callback
            let sourcepad = sourcepad.unwrap();
            let self_downgrade = self.downgrade();
            sourcepad.add_probe(gst::PadProbeType::BLOCK_DOWNSTREAM, move |pad, probe_info| {
                info!("add probe blockdownstream triggered");
                let fallback = upgrade_weak!(self_downgrade, gst::PadProbeReturn::Ok);
                fallback.pad_eos_cb(pad, probe_info).map_err(|e| {
                    warn!("pad_eos_cb triggered an error: {}", e);
                    gst::PadProbeReturn::Ok
                }).unwrap()
            });
        } else {
            info!("did not remove source (add_probe BLOCK_STREAM to sourcepad), cause source is empty");
        }
        
        let convertsink = state.converter_bin.static_pad("sink").unwrap();
        drop(state);


        // after creating the probe we send eos
        info!("send eos event task_state: {:#?}, last_flow_result: {:#?}", convertsink.task_state(), convertsink.last_flow_result());
        let _re = convertsink.send_event(gst::event::Eos::new());
        //convertsink.push_event(gst::event::Eos::new());
        
        Ok(())
    }

    pub fn start(&self, uri: Option<&str>) -> Result<()> {
        info!("called start");
        let mut state = self.state.lock();
        let source = state.source.clone();

        let uri_changed = if let Some(uri) = uri {
            
            let change_uri = if let Some(current_uri) = &state.uri {
                if current_uri == uri {
                    warn!("dont switch uri to current uri, cause it is the same");
                    false
                } else {
                    true
                }
            } else {
                true
            };

            if change_uri {
                state.uri = Some(uri.to_string());
                state.current_state = CurrentState::ChangeUri;
            }

            change_uri
        } else {
            false
        };

        if state.uri.is_none() {
            return Err(anyhow!("cannot start new cause of: Uri is unset"));
        }

        if uri_changed == false && uri.is_some() {
            info!("in start / change uri function we stop, cause uri_changed is false and uri is Some");
            return Err(anyhow!("cannot start new cause of: Uri is not changed"));
        }

        // hier sollten wir erstmal bin auf paused // null setzen
        //let state_change_result = self.bin.set_state(gst::State::Null);
        //if let Ok(result) = state_change_result {
        //    if result == gst::StateChangeSuccess::Success {
        //        info!("on play, right before manipulation, we set the bin state successfull to Null");
        //    } else {
        //        warn!("on play, rightt before manipulation, we not successfull set state to Null :(");
        //    }
        //}


        // wir gehen hier davon aus das es keine source gibt
        if source.is_some() {

            
            if let Some(source) = &state.source {
                info!("start: source is not empty, remove source from bin and set to None. wait 300ms and the continue");

                let _ = self.bin.remove(source);
                sleep_ms!(150);
                //source.set_state(gst::State::Null);
                
                drop(source);
                state.source = None;


                sleep_ms!(750);

            }

        }

        

        
        let source = make_element("uridecodebin", None)?;
        info!("add decoderbin {} uridecodebin name: {:?}", state.uri.as_ref().map(|x| &**x).unwrap(), source.name());
        //info!("current playback state is {:?}", self.pipeline.state(None));

        source.set_property("uri", state.uri.as_ref().map(|x| &**x).unwrap());
        source.set_property("use-buffering", &false);

        //source.connect("source-setup", false, |r| {
        //    let ins = r[1].get::<gst::Element>().unwrap();
        //    ins.set_property("proxy", "http://127.0.0.1:9090");
        //
        //    None
        //});

        let s = self.clone();
        let self_downgrade = s.downgrade();

        source.connect_pad_added(move |src, pad| {
            let fb = upgrade_weak!(self_downgrade);
            info!("Source decoder name is: {:?}", src.name());
            if None == src.parent() {
                return
            }
            if let Err(e) = fb.pad_added_cb(pad) {
                warn!("error on add pad from decoder: {}", e);

                {
                    debug!("remove source from state, cause of error");
                    let mut state = fb.state.lock();
                    if let Some(source) = &state.source {
                        let _ = fb.bin.remove(source);
                        state.source = None;
                    }
                }
            } else {
                info!("enable, watchdog");
                fb.set_watchdog(true);
            }
        });

        sleep_ms!(2000);
        self.bin.add(&source)?;

        source.sync_state_with_parent()?;
        self.bin.sync_state_with_parent()?;

        state.current_state = CurrentState::WaitForDecoderSrcPad;
        state.source = Some(source);
        drop(state);
        

        //self.set_watchdog(true);

        // if running time...
        info!("set source and pipeline to playing to playing");
        
        sleep_ms!(100);

        info!("current state: {:#?}", self.pipeline.state(None));
        let state_change_result = self.pipeline.set_state(gst::State::Playing);
        if let Ok(result) = state_change_result {
            if result == gst::StateChangeSuccess::Success {
                let mut state = self.state.lock();
                state.error_state = ErrorState::None;
                drop(state);
            }
        }

        info!("current playback state after setting state to playing is {:?}", self.pipeline.state(None));

        Ok(())
    }

    fn pad_eos_cb(&self, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> Result<gst::PadProbeReturn> {
        if let Some(gst::PadProbeData::Event(ref event)) = info.data {
            if event.type_() == gst::EventType::Eos {
                info!("we received an EOS event");

                pad.remove_probe(info.id.take().unwrap());
                
                let mut state = self.state.lock();

                let src = state.source_pad.as_ref().unwrap();
                let sink = state.converter_bin.static_pad("sink").unwrap();


                let source = state.source.as_ref().unwrap();
                let bin_src = state.bin_src.clone();

                
                if let Err(e) = src.unlink(&sink) {
                    warn!("want to unlink src pad from sink {:?}.unlink({:?}) but got an error: {:?}", src.name(), sink.name(), e);
                }

                if state.has_mixer_connected {
                    self.mixer.release_pad(bin_src.peer().unwrap());
                }
                
                if let Err(e) = self.bin.remove(source) {
                    warn!("want remove {:?} from Fallbackbin, but got an error: {:?}", source.name(), e);
                }

                info!("remove source and source pad");
                state.has_mixer_connected = false;
                state.source = None;
                state.source_pad = None;
  
                drop(state);

                
                return Ok(gst::PadProbeReturn::Drop)
            }
        }

        // Ok or Pass ?
        Ok(gst::PadProbeReturn::Pass)
    }


    fn pad_added_cb(&self, pad: &gst::Pad) -> Result<()> {
        let mut state = self.state.lock();
        let converter_sink = state.converter_bin.static_pad("sink").unwrap();

        let converter_bin_parent = state.converter_bin.parent();
        info!("converter_bin_parent is: {:#?}", converter_bin_parent.unwrap().name());
        info!("converter sink is: {:#?}", converter_sink.name());
        info!("pad name is is: {:#?}", pad.name());

        pad.link(&converter_sink)?;

        /*
        let running_time = self.running_time.read();
        pad.set_offset(running_time.nseconds() as i64);
        */

        if let Some(running_time) = self.pipeline.current_running_time() {
            pad.set_offset(running_time.nseconds() as i64);
        } else {
            warn!("could not set current running time cause of no running time on pipeline found");
        }
        

        if !state.has_mixer_connected {
            info!("has no mixer connected, connect new one");
            self.mixer.connect_new_sink(&state.bin_src)?;
            state.has_mixer_connected = true;
        }

        // if everythink works well, we add the sourcepad to state
        state.source_pad = Some(pad.clone());
        sleep_ms!(200);
        info!("Pad Added, set State to PlaySource");
        state.current_state = CurrentState::PlaySource;
        drop(state);
        info!("dropped state");

        Ok(())
    }

}


fn create_converter_bin(name: Option<&str>, rate: Option<i32>) -> Result<(gst::Bin, gst::Element)> {


    let caps = gst::Caps::builder("audio/x-raw")
        .field("rate", &rate.unwrap_or(44100))
        .field("channels", &2i32)
        .build();

    let bin = gst::Bin::new(name);
    let mut elements = Vec::new();

    let watchdog = make_element("watchdog", name.and_then(|n: &str| Some( format!("{}_watchdog", n)) ).as_ref().map(|x| &**x) )?;
    watchdog.set_property("timeout", &0i32);
    bin.add(&watchdog)?;
    elements.push(watchdog.clone());


    let resampler = make_element("audioresample", name.and_then(|n: &str| Some( format!("{}_audioresample", n)) ).as_ref().map(|x| &**x) )?;
    bin.add(&resampler)?;
    elements.push(resampler);
    
    let converter = make_element("audioconvert", name.and_then(|n: &str| Some(format!("{}_audioconvert", n)) ).as_ref().map(|x| &**x) )?;
    bin.add(&converter)?;
    elements.push(converter);
    
    let capsfilter = make_element("capsfilter", name.and_then(|n: &str| Some(format!("{}_capsfilter", n)) ).as_ref().map(|x| &**x) )?;
    capsfilter.set_property("caps", &caps);
    bin.add(&capsfilter)?;
    elements.push(capsfilter);


    let clocksync_queue = gst::ElementFactory::make("queue", None).expect("No queue found");
    clocksync_queue.set_properties(&[
        ("max-size-buffers", &0u32),
        ("max-size-bytes", &0u32),
        ("max-size-time", &(5 * gst::ClockTime::SECOND)),
    ]);

    bin.add(&clocksync_queue)?;
    elements.push(clocksync_queue);
    
    
    gst::Element::link_many(elements.iter().map(|e| e).collect::<Vec<&gst::Element>>().as_slice())?;

    for element in elements.iter() {
        element.sync_state_with_parent()?;
    }

    let last_element = elements.last().unwrap();
    let src_pad = gst::GhostPad::with_target(Some("src"), &last_element.static_pad("src").unwrap())?;
    bin.add_pad(&src_pad)?;

    let first_element = elements.first().unwrap();
    let sink_pad = gst::GhostPad::with_target(Some("sink"), &first_element.static_pad("sink").unwrap())?;
    bin.add_pad(&sink_pad)?;

    Ok((bin, watchdog))
}


fn setup_silence() -> Result<gst::Element> {

    let caps = gst::Caps::builder("audio/x-raw")
        .field("rate", 44100)
        .field("channels", &2i32)
        .build();

    let bin = gst::Bin::new(None);

    let silence = make_element("audiotestsrc", None)?;
    silence.set_property_from_str("wave", "sine");
    //silence.set_property_from_str("wave", "silence");
    //tsrc.set_property_from_str("wave", "ticks");
    //tsrc.set_property("freq", 2000.0f64.to_value())?;
    //silence.set_property_from_str("freq", "800");
    silence.try_set_property("is-live", &false)?;


    let audioconvert = make_element("audioconvert", None)?;
    let audioresample = make_element("audioresample", None)?;
    let capsfilter = make_element("capsfilter", None)?;
    capsfilter.set_property("caps", &caps);

    bin.add(&silence)?;
    bin.add(&audioconvert)?;
    bin.add(&audioresample)?;
    bin.add(&capsfilter)?;

    silence.link_pads(Some("src"), &audioconvert, Some("sink"))?;
    audioconvert.link_pads(Some("src"), &audioresample, Some("sink"))?;
    audioresample.link_pads(Some("src"), &capsfilter, Some("sink"))?;

    let src_pad = gst::GhostPad::with_target(Some("src"), &capsfilter.static_pad("src").unwrap())?;
    bin.add_pad(&src_pad)?;

    Ok(bin.upcast())
}