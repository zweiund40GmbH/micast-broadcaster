
mod network;
mod item;
mod queue;
mod builder;

pub use builder::Builder;
use queue::Queue;
use item::{Item, ItemState};

use gstreamer as gst;
use gstreamer_net as gst_net;
use crate::helpers::*;

use std::sync::{Arc, Weak};
use gst::prelude::*;
use anyhow::{bail};

use log::{debug, warn};

// Strong reference to our broadcast server state
#[derive(Debug, Clone)]
pub struct Broadcast(Arc<BroadcastInner>);

// Weak reference to our broadcast server state
#[derive(Debug, Clone)]
pub(crate) struct BroadcastWeak(Weak<BroadcastInner>);

// Actual broadcast server state
#[derive(Debug)]
pub struct BroadcastInner {
    pipeline: gst::Pipeline,
    //audio_mixer: gst::Element,
    #[allow(unused_parens, dead_code)]
    clock_provider: gst_net::NetTimeProvider,
    playback_queue: Queue,
}

// To be able to access the App's fields directly
impl std::ops::Deref for Broadcast {
    type Target = BroadcastInner;

    fn deref(&self) -> &BroadcastInner {
        &self.0
    }
}

impl BroadcastWeak {
    // Try upgrading a weak reference to a strong one
    fn upgrade(&self) -> Option<Broadcast> {
        self.0.upgrade().map(Broadcast)
    }
}


impl Broadcast {
    // Downgrade the strong reference to a weak reference
    fn downgrade(&self) -> BroadcastWeak {
        BroadcastWeak(Arc::downgrade(&self.0))
    }

    /// Creates the **Broadcast Server** to Send / Stream Audio. 
    /// 
    /// - Need to add Decoded Things to the adder Thing.. (more docs...)
    ///
    /// # Arguments
    ///
    /// * `server_ip` - The Server Address where the clients connect to this Server (could be a broadcast address)
    /// * `rtp_sender_port` - Port to send Mediadata to all connected clients
    /// * `rtcp_sender_port` - Port for sending rtcp Informations to all connected clients
    /// * `rtcp_receive_port` - Port for receive rtcp Informations from connected clients
    /// * `clock_port` - Port where the clock gets streamed (should 8555)
    ///
    pub fn new(
        server_ip: &str, 
        rtp_sender_port: i32, 
        rtcp_sender_port: i32, 
        rtcp_receive_port: i32, 
        clock_port:i32, 
    ) -> Result<
        Self,
        anyhow::Error,
    > {

        debug!("init gstreamer");
        gst::init()?;

        debug!("create pipeline, add clock, add adder as mixer, and audioconverter for preconvert");
        // create pipeline
        let pipeline = gst::Pipeline::new(None);


        let bus = pipeline.bus().expect("Pipeline without bus should never happen");
        
        bus.add_watch(move |_, msg| {
            use gst::MessageView;
    
            match msg.view() {
                MessageView::Eos(..) => {
                    warn!("received eos");
                    // An EndOfStream event was sent to the pipeline, so we tell our main loop
                    // to stop execution here.
                }
                MessageView::Error(err) => {
                    warn!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );

                    // currently we need to panic here.
                    // the program who use this lib, would then automatically restart.
                    // the main problem is that the pipeline stops streaming audio over rtp if any element got an error, also if we restart the pipeline (meaning: set state to stopped, and the to play)
                    panic!("got an error, quit here");
                    
                }
                _ => (),
            };
    
            // Tell the mainloop to continue executing this callback.
            glib::Continue(true)
        })
        .expect("Failed to add bus watch");
    


        // setup clock for synchronization
        let clock = gst::SystemClock::obtain();
        let clock_provider = gst_net::NetTimeProvider::new(&clock, None, clock_port);
        clock.set_property("clock-type", &gst::ClockType::Realtime)?;
        pipeline.use_clock(Some(&clock));

        // setup the audio mixer as input for Pipeline
        let audiomixer = make_element("adder", Some("mixer"))?;
        let audiomixer_convert = make_element("audioconvert", None)?;
        // -- add mixer and converter to element
        pipeline.add(&audiomixer)?;
        pipeline.add(&audiomixer_convert)?;
        // -- linkt this elements
        gst::Element::link_many(&[&audiomixer, &audiomixer_convert])?;

        // create network things
        let sender_bin = self::network::create_bin(
            rtcp_receive_port, 
            rtcp_sender_port, 
            rtp_sender_port,
            server_ip
        )?;
        // -- add senderbin to pipeline
        pipeline.add(&sender_bin)?;
        // -- link the output of audiomixer to input of the sender_bin
        audiomixer_convert.link_pads(Some("src"), &sender_bin, Some("sink"))?;

        pipeline.set_state(gst::State::Playing)?;


        let broadcast = Broadcast(Arc::new(BroadcastInner {
            pipeline,
            clock_provider,
            playback_queue: Queue::new(),
        }));

        
        /*
        if let Err(e) = broadcast.put_item_to_mixer(&broadcast.silence.audio_pad) {
            warn!("error on put item to mixer: {:?}", e);
        }
        */
        

        Ok(
            broadcast
        )
    }


    /// Schedule Next Item
    /// 
    /// sets a new uri element for playback
    pub fn schedule_next(&self, uri: &str, state: ScheduleState, fixed_length: Option<u32>) -> Result<(), anyhow::Error> {
        match state {
            ScheduleState::AfterCurrent => {

            },
            ScheduleState::Now => {

            },
            ScheduleState::Interrupt => {

            }
        }

        debug!("add item to schedule");
        // create an item to hold all required Informations pad's etc
        let broadcast_clone = self.downgrade();
        let item = self::item::Item::new(uri, broadcast_clone, fixed_length)?;
        let current = self.playback_queue.current();
        if current.is_none() {
            debug!("queue got nothing, so activate next item");
            self.activate_item(&item)?;
        }

        self.playback_queue.push(item);
        
        drop(current);

        Ok(())
        
    }
    
    /// ## Schedule Crossfade on EOS to next item in queue
    /// 
    /// get current **item** get playback time,
    /// pop next item from queue, prepare and set as next item
    /// 
    /// * `item` - current playing item that are  short before EOS
    fn schedule_crossfade(&self, item: &item::Item, queue_size: u64) {

        debug!("schedule_crossfade is triggered, so item {} is going eos, we start with next item", item.uri);
        let next_item_result = self.activate_next_item();
        if let Err(e) = next_item_result {
            warn!("could not activate next item cause of {}", e);
        } else {
            debug!(" set offset for next item");
            let next_item = next_item_result.unwrap();
            next_item.set_offset(gst::ClockTime::from_nseconds(queue_size));
        }

        debug!("end of schedule crossfade");
    }

    pub fn early_crossfade(&self) {
        debug!("make early_crossfade");
        let current = self.playback_queue.current();

        debug!("for early_crossfade we wait if a next item is prepared");
        let next = self.playback_queue.next();

        if let Some(current) = current {
            if next.is_some() {
                debug!("trigger prepare crossfade:");
                current.prepare_for_early_end();
            } else {
                debug!("could not make early_crossfade, no next items");
            }
        }
        
    }

    /// ## Activate next item from queue
    /// 
    /// - pop item from queue,
    /// - call activate
    fn activate_next_item(&self) -> Result<Arc<item::Item>, anyhow::Error> {

        self.playback_queue.clean();

        // get item from queue
        let next = self.playback_queue.next();

        if let Some(next) = next {
            // try to activate item from queue
            self.activate_item(&next)?;

            return Ok(next)
        }

        bail!("no next item found")
        
    }


    fn put_item_to_mixer(&self, pad: &gst::Pad) -> Result<gst::Pad, anyhow::Error> {
        
        let mixer = self.pipeline.by_name("mixer").unwrap();
        if let Some(sinkpad) = mixer.request_pad_simple("sink_%u") { 
            // link audio_pad from item decoder to sinkpad of the mixer
            pad.link(&sinkpad)?;

            return Ok(sinkpad)
        }

        bail!("Couldnt add thing to mixer")
    }

    /// ## Activate Item
    /// 
    /// - item needs to prepared (usual items gets activatet on pushing to queue)
    /// - item needs to have a audipad, which get created on pushing to queue, where decoder_pad_added gets called when gstreamer
    ///   determine the pipeline to decode the item
    /// - gets the mixer from main pipeline and request a sink_pad and link both together
    /// - sets all callbacks (current runningtime over buffer event) & downstream event for checking eos on item
    /// - at least, if all was successfull, set state to Activate
    fn activate_item(&self, item: &item::Item) -> Result<(), anyhow::Error> {

        let mut retry_count = 0;
        while item.state() != item::ItemState::Prepared {
            
            retry_count += 1;
            debug!("retry prepared {} time / times 50 ms", retry_count);
            if retry_count >= 20 {
                bail!("after retry multiple times: Item is not Prepared");
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        if item.audio_pad().is_none() {
            bail!("Item has no AudioPad");
        }

        if !item.has_block_id() {
            bail!("Item has no Blocked Pad");
        }

        let audio_pad = item.audio_pad().unwrap();

        if let Ok(sinkpad) = self.put_item_to_mixer(&audio_pad) {
            debug!("link audio_pad from {} to mixer {}", item.uri, sinkpad.name());

            let item_clone = item.downgrade();
            // on every buffer pad event, set the running time for the item
            sinkpad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
                let item = upgrade_weak!(item_clone, gst::PadProbeReturn::Pass);
                item.pad_probe_running_time(pad, info)
            });


            let item_clone = item.downgrade();

            // when the mixer sink has a downstream event call pad_probe_going_eos for the item, where we check if the item has an eos
            sinkpad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |pad, probe_info| {
                let item = upgrade_weak!(item_clone, gst::PadProbeReturn::Pass);
                item.pad_probe_eos(pad, probe_info)
            });


            item.remove_block()?;

            debug!("set state to activate");
            item.set_state(item::ItemState::Activate);
            return Ok(());    
        }

        
        bail!("Item couldnt get a mixer request sink pad")
    }

}


/// Not implemented yet
pub enum ScheduleState {
    AfterCurrent, // Nach dem aktuellen Song
    Now, // Jetzt Sofort
    Interrupt, // kurze Unterbrechung
}