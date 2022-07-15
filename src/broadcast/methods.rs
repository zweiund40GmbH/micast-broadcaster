
pub mod pad_helper {

    use gst::prelude::*;
    
    //if let Some(ev) = pad.sticky_event::<gst::event::StreamStart>(0) {
    //    stream_type = ev.stream().map(|s| s.stream_type());
    //}


    pub fn running_time_method<'a, F: 'a + Fn(&gst::ClockTime) + Sync + Send >(pad: &'a gst::Pad, info: &mut gst::PadProbeInfo, f: F) -> gst::PadProbeReturn {
        
        //println!("DAAASS HIIIER GETS CALLLED");
        
        let pts: Option<gst::ClockTime> = match info.data {
            Some(gst::PadProbeData::Buffer(ref buffer)) => buffer.pts(),
            Some(gst::PadProbeData::Event(ref ev)) => match ev.view() {
                gst::EventView::Gap(ev) => Some(ev.get().0),
                _ => return gst::PadProbeReturn::Pass,
            },
            _ => unreachable!(),
        };

        let segment = match pad.sticky_event::<gst::event::Segment>(0) {
            Some(ev) => ev.segment().clone(),
            None => {
                println!("no segment yet");
                return gst::PadProbeReturn::Pass
            }
        };

        let segment = segment.downcast::<gst::ClockTime>().map_err(|_| {
            println!("no time segment");
            return gst::PadProbeReturn::Pass
        }).unwrap();


        let running_time = if let Some((_, start)) =
            pts.zip(segment.start()).filter(|(pts, start)| pts < start)
        {
            segment.to_running_time(start)
        } else if let Some((_, stop)) = pts.zip(segment.stop()).filter(|(pts, stop)| pts >= stop) {
            segment.to_running_time(stop)
        } else {
            segment.to_running_time(pts)
        };


        f(&running_time.unwrap()); 

        

        gst::PadProbeReturn::Pass

    }

}
