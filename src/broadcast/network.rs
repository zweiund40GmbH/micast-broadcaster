
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
#[allow(dead_code)]
pub fn create_bin( 
    rtcp_receiver_port: i32, 
    rtcp_send_port: i32, 
    rtp_send_port: i32, 
    server_address: &str,
    multicast_interface: Option<String>,
) -> Result<gst::Bin, anyhow::Error,> {

    debug!("setup gstbin for RTP Stream Networking");
    let bin = gst::Bin::new(Some("rtpbinbin"));

    // filter and payloader
    let capsfilter = make_element("capsfilter", None)?;
    let payloader = make_element("rtpL24pay", Some("pay0"))?;
    bin.add_many(&[&capsfilter, &payloader])?;
    let caps = gst::Caps::builder("audio/x-raw")
        //.field("rate", &48000i32)
        .field("rate", &44100i32)
        .build();
    capsfilter.try_set_property("caps", &caps).unwrap();  
    gst::Element::link_many(&[&capsfilter, &payloader])?;

    // network and transport
    let rtpbin        = make_element("rtpbin", None)?;
    let rtp_udp_sink  = make_element("udpsink",Some("network_rtp_sink"))?;
    let rtcp_udp_sink = make_element("udpsink",Some("network_rtcp_sink"))?;
    let rtcp_udp_src  = make_element("udpsrc", Some("network_rtcp_src"))?;

    bin.add_many(&[&rtpbin, &rtp_udp_sink, &rtcp_udp_sink, &rtcp_udp_src])?;


    payloader.link_pads(Some("src"), &rtpbin, Some("send_rtp_sink_0"))?;
    rtpbin.link_pads(Some("send_rtp_src_0"), &rtp_udp_sink, Some("sink"))?; // send media stream on 5004
    rtpbin.link_pads(Some("send_rtcp_src_0"), &rtcp_udp_sink, Some("sink"))?; //send rtcp contronls on port 5005
    rtcp_udp_src.link_pads(Some("src"), &rtpbin, Some("recv_rtcp_sink_0"))?;

    // set rtp ip and port
    rtp_udp_sink.try_set_property("host", server_address)?;
    rtp_udp_sink.try_set_property("port", rtp_send_port)?;

    // required?
    rtp_udp_sink.try_set_property("sync", &true)?;
    rtp_udp_sink.try_set_property("async", &false)?;
    

    debug!("RTCP SEND PORT: {}", rtcp_send_port);
    // set rtcp ip and port (disable async and sync)
    rtcp_udp_sink.try_set_property("host", server_address)?;
    rtcp_udp_sink.try_set_property("port", rtcp_send_port)?;
    rtcp_udp_sink.try_set_property("async", &false)?; 
    rtcp_udp_sink.try_set_property("sync", &false)?;


    
    
    //multicast-iface=enp5s0

    debug!("RTCP RECEIVE PORT: {}", rtcp_receiver_port);
    rtcp_udp_src.try_set_property("address", server_address)?;
    rtcp_udp_src.try_set_property("port", rtcp_receiver_port)?;
    //rtcp_udp_sink.try_set_property("async", &true)?; 

    rtpbin.try_set_property_from_str("ntp-time-source", "clock-time")?;
    //rtpbin.try_set_property("ntp-sync", &true)?;

    //rtpbin.try_set_property("rtcp-sync-send-time", &false)?;
    
    rtpbin.connect_pad_added(move |el, pad| {
        let name = pad.name().to_string();
        debug!("rtpbin pad_added: {}", name);

    });

    if let Some(multicast_interface) = multicast_interface {
        debug!("set multicast interface {}", multicast_interface);
        rtp_udp_sink.try_set_property("multicast-iface", &multicast_interface)?;
        rtcp_udp_sink.try_set_property("multicast-iface", &multicast_interface)?;
        rtcp_udp_src.try_set_property("multicast-iface", &multicast_interface)?;
    }

    let ghost_pad = gst::GhostPad::with_target(Some("sink"), &capsfilter.static_pad("sink").unwrap())?;
    bin.add_pad(&ghost_pad)?;

    Ok(bin)
}