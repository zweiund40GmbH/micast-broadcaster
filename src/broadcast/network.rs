
use gst::prelude::*;
use gst_rtp::prelude::*;
use crate::helpers::*;
use anyhow::{Context, Error};

use log::{debug, info};

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
///
#[allow(dead_code)]
pub fn create_bin( 
    rtcp_receiver_port: i32, 
    rtcp_send_port: i32, 
    rtp_send_port: i32, 
    server_address: &str,
    multicast_interface: Option<String>,
) -> Result<gst::Bin, anyhow::Error,> {

    debug!("setup gstbin for RTP Stream Networking");

    info!("rtcp_receiver_port: {} rtcp_send_port: {} rtp_send_port: {} server_address: {} multicast_interface: {}", 
        rtcp_receiver_port, 
        rtcp_send_port,
        rtp_send_port,
        server_address,
        multicast_interface.as_ref().unwrap_or(&"NONE".to_string()),
    );

    let bin = gst::Bin::new(Some("rtpbinbin"));

    // filter and payloader
    //let capsfilter = make_element("capsfilter", None)?;
    let payloader = make_element("rtpL24pay", Some("pay0"))?;

    payloader.set_property("pt", 96u32);

    //let hdr_ext = gst_rtp::RTPHeaderExtension::create_from_uri(
    //    "urn:ietf:params:rtp-hdrext:ntp-64",
    //)
    //.context("Creating NTP 64-bit RTP header extension")?;
    //hdr_ext.set_id(1);
    //payloader.emit_by_name::<()>("add-extension", &[&hdr_ext]);



    bin.add(&payloader)?;
    
    // network and transport
    let rtpbin = make_element("rtpbin", None)?;


    //if super::ENCRYPTION_ENABLED && std::env::var("BC_ENCRYPTION_DISABLED").ok().is_none() {
    //    crate::encryption::server_encryption(&rtpbin)?;
    //}

    let rtp_udp_sink  = make_element("udpsink",Some("network_rtp_sink"))?;
    let rtcp_udp_sink = make_element("udpsink",Some("network_rtcp_sink"))?;
    let rtcp_udp_src  = make_element("udpsrc", Some("network_rtcp_src"))?;

    bin.add_many(&[&rtpbin, &rtp_udp_sink, &rtcp_udp_sink, &rtcp_udp_src])?;


    payloader.link_pads(Some("src"), &rtpbin, Some("send_rtp_sink_0"))?;
    rtpbin.link_pads(Some("send_rtp_src_0"), &rtp_udp_sink, Some("sink"))?; // send media stream on 5004
    rtpbin.link_pads(Some("send_rtcp_src_0"), &rtcp_udp_sink, Some("sink"))?; //send rtcp contronls on port 5005
    rtcp_udp_src.link_pads(Some("src"), &rtpbin, Some("recv_rtcp_sink_0"))?;

    // set rtp ip and port
    rtp_udp_sink.set_property("host", server_address);
    rtp_udp_sink.set_property("port", rtp_send_port);

    // required?
    rtp_udp_sink.set_property("sync", true);
    rtp_udp_sink.set_property("async", false);

    // set rtcp ip and port (disable async and sync)
    rtcp_udp_sink.set_property("host", server_address);
    rtcp_udp_sink.set_property("port", rtcp_send_port);
    rtcp_udp_sink.set_property("async", &false); 
    rtcp_udp_sink.set_property("sync", &false);

    rtcp_udp_src.set_property("address", server_address);
    rtcp_udp_src.set_property("port", rtcp_receiver_port);
    //rtcp_udp_sink.try_set_property("async", &true)?; 

    rtpbin.set_property_from_str("ntp-time-source", "clock-time");

    //rtpbin.set_property("rtcp-sync-send-time", false);
    //rtpbin.try_set_property("ntp-sync", &true)?;

    //rtpbin.try_set_property("rtcp-sync-interval", &1000u32)?; // in ms
    //rtpbin.set_property("do-retransmission", true);
    //rtpbin.set_property("rtcp-sync-send-time", false);
    
    rtpbin.connect_pad_added(move |_el, pad| {
        let name = pad.name().to_string();
        debug!("rtpbin pad_added: {}", name);

    });


    //rtp_udp_sink.try_set_property("multicast-iface", &"enp5s0, wlp1s0, en7, eth0")?;
    //rtcp_udp_sink.try_set_property("multicast-iface", &"enp5s0, wlp1s0, en7, eth0")?;

    rtp_udp_sink.set_property("force-ipv4", true);
    rtcp_udp_sink.set_property("force-ipv4", true);

    let ghost_pad = gst::GhostPad::with_target(Some("sink"), &payloader.static_pad("sink").unwrap())?;
    bin.add_pad(&ghost_pad)?;

    Ok(bin)
}