
use gstreamer as gst;
use gst::prelude::*;
use crate::helpers::*;

use log::{debug};

/// Returns a gstreamer bin with all required elements for RTP Streaming
///
/// # Arguments
///
/// * `rtcp_receiver_port` - Port for reciving rtcp Informations from connected clients
/// * `rtcp_send_port` - Port for sending rtcp Informations to all connected clients
/// * `rtp_send_port` - Port to send Mediadata to all connected clients
/// * `server_address` - The Server Address where the clients connect to this Server (could be a broadcast address)
///
/// # Examples
///
/// ```
/// // You can have rust code between fences inside the comments
/// // If you pass --test to `rustdoc`, it will even test it for you!
/// ```
pub fn create_bin( 
    rtcp_receiver_port: i32, 
    rtcp_send_port: i32, 
    rtp_send_port: i32, 
    server_address: &str
) -> Result<gst::Bin, anyhow::Error,> {

    debug!("setup gstbin for RTP Stream Networking");
    let bin = gst::Bin::new(Some("rtpbinbin"));

    // filter and payloader
    let capsfilter = make_element("capsfilter", None)?;
    let payloader = make_element("rtpL24pay", Some("pay0"))?;
    bin.add_many(&[&capsfilter, &payloader])?;
    let caps = gst::Caps::builder("audio/x-raw")
        .field("rate", &48000i32)
        .build();
    capsfilter.set_property("caps", &caps).unwrap();     
    gst::Element::link_many(&[&capsfilter, &payloader])?;

    // network and transport
    let rtpbin = make_element("rtpbin", None)?;
    let rtp_udp_sink = make_element("udpsink", None)?;
    let rtcp_udp_sink = make_element("udpsink", None)?;
    let rtcp_udp_src = make_element("udpsrc", None)?;

    bin.add_many(&[&rtpbin, &rtp_udp_sink, &rtcp_udp_sink, &rtcp_udp_src])?;

    payloader.link_pads(Some("src"), &rtpbin, Some("send_rtp_sink_0"))?;
    rtpbin.link_pads(Some("send_rtp_src_0"), &rtp_udp_sink, Some("sink"))?; // send media stream on 5004
    rtpbin.link_pads(Some("send_rtcp_src_0"), &rtcp_udp_sink, Some("sink"))?; //send rtcp contronls on port 5005
    rtcp_udp_src.link_pads(Some("src"), &rtpbin, Some("recv_rtcp_sink_0"))?;

    // set rtp ip and port
    rtp_udp_sink.set_property("host", server_address)?;
    rtp_udp_sink.set_property("port", rtp_send_port)?;

    // set rtcp ip and port (disable async and sync)
    rtcp_udp_sink.set_property("host", server_address)?;
    rtcp_udp_sink.set_property("port", rtcp_send_port)?;
    rtcp_udp_sink.set_property("async", &false)?; 
    rtcp_udp_sink.set_property("sync", &false)?;

    rtcp_udp_src.set_property("address", server_address)?;
    rtcp_udp_src.set_property("port", rtcp_receiver_port)?;

    rtpbin.set_property_from_str("ntp-time-source", "clock-time");

    let ghost_pad = gst::GhostPad::with_target(Some("sink"), &capsfilter.static_pad("sink").unwrap())?;
    bin.add_pad(&ghost_pad)?;

    Ok(bin)
}