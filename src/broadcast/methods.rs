
pub mod pad_helper {

    use gst::prelude::*;
    
    //if let Some(ev) = pad.sticky_event::<gst::event::StreamStart>(0) {
    //    stream_type = ev.stream().map(|s| s.stream_type());
    //}


    pub fn running_time_method<'a, F: 'a + Fn(&gst::ClockTime) + Sync + Send >(pad: &'a gst::Pad, info: &mut gst::PadProbeInfo, f: F) -> gst::PadProbeReturn {
        
        if let Some(event) = pad.sticky_event::<gst::event::Segment<gst::Event>>(0) { 
            if let Some(data) = &info.data {
                if let gst::PadProbeData::Buffer(buffer) = data {
                    if let gst::EventView::Segment(segment) = event.view() {
                        match segment.segment().to_running_time(buffer.pts().unwrap()) {
                            gst::GenericFormattedValue::Time(Some(clock)) => {
                               f(&clock); 
                            },
                            _ => {}
                        }
                    }
                }
            }
        }

        gst::PadProbeReturn::Pass

    }

}
