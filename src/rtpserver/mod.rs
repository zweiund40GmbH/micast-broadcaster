
use gst::prelude::*;
use gst_rtp::prelude::*;
use log::{warn, debug};

#[derive(Debug,Clone)]
pub struct RTPServer {
    bin: gst::Bin,
    rtcp_receiver: Option<gst::Element>,
}

unsafe impl Send for RTPServer {}
unsafe impl Sync for RTPServer {}


impl RTPServer {
    pub fn new(with_rtcp: bool) -> Result<RTPServer, anyhow::Error> {

        let bin = RTPServer::_prepare_bin(with_rtcp)?;

        let rtcp_receiver = if with_rtcp {
            bin.by_name("udprtscpsrc0")
        } else { None };

        Ok(RTPServer { bin, rtcp_receiver })

    }

    pub fn get_element(&self) -> gst::Element {
        let el: gst::Element = self.bin.clone().upcast();
        el.clone()
    }

    pub fn get_sink(&self) -> Option<gst::Pad> {
        self.bin.static_pad("sink")
    }

    pub fn set_listen_for_rtcp_packets(&self, port: i32) -> Result<(), anyhow::Error> {
        debug!("enable listen for rtcp packets");
        if let Some(rtcp_receiver) = &self.rtcp_receiver {
            rtcp_receiver.set_property("port", &port);
            rtcp_receiver.set_property("address", "0.0.0.0");
        } else {
            return Err(anyhow::anyhow!("rtcp receiver not found"));
        }

        Ok(())

    }

    /// # add a client to the server
    /// 
    pub fn add_client(&self, address: (&str, u32)) -> Result<(), anyhow::Error> {

        let port = address.1 as i32;
        if let Some(rtp_udp_sink) = self.bin.by_name("rtpsink0") {
            debug!("add client: {} {}", address.0, port);
            debug!("add local client: {} {}", "127.0.0.1", port);
            rtp_udp_sink.emit_by_name::<()>("add", &[&address.0, &port]);
            rtp_udp_sink.emit_by_name::<()>("add", &[&"127.0.0.1", &port]);

        } else {
            return Err(anyhow::anyhow!("rtpsink0 not found"));
        }

        if let Some(rtcp_udp_sink) = self.bin.by_name("rtcpsink0") {
            let rtcp_port = port + 1;
            //rtcp_udp_sink.emit_by_name::<()>("add", &[&address.0, &rtcp_port]);
            rtcp_udp_sink.emit_by_name::<()>("add", &[&"127.0.0.1", &rtcp_port]);
        }


        Ok(())
    }

    ///
    /// Remove a client from the RTP Server
    /// 
    pub fn remove_client(&self, address: (&str, u32)) -> Result<(), anyhow::Error> {

        let port = address.1 as i32;
        if let Some(rtp_udp_sink) = self.bin.by_name("rtpsink0") {
            rtp_udp_sink.emit_by_name::<()>("remove", &[&address.0, &port]);

        } else {
            return Err(anyhow::anyhow!("rtpsink0 not found"));
        }

        if let Some(rtcp_udp_sink) = self.bin.by_name("rtcpsink0") {
            let rtcp_port = port + 1;
            rtcp_udp_sink.emit_by_name::<()>("remove", &[&address.0, &rtcp_port]);
        }

        Ok(())
    }

    /// Create a new `gst::Element` of type `multiudpsink` for RTP or RTCP and configure its properties.
    ///
    /// # Arguments
    ///
    /// * `mcast` - A boolean value to enable multicast for the `multiudpsink` element.
    /// * `is_rtp` - A boolean value to indicate if the `multiudpsink` element is for RTP or RTCP.
    ///
    /// # Returns
    ///
    /// The configured `gst::Element` or an `anyhow::Error` if an error occurred during creation or configuration.
    ///
    fn _set_udpsink(mcast: bool, is_rtp: bool) -> Result<gst::Element, anyhow::Error> {
        let prop_name = if is_rtp { "rtpsink0" } else { "rtcpsink0" };
        let udpsink = gst::ElementFactory::make_with_name("multiudpsink", Some(prop_name))
            .map_err(|_| anyhow::anyhow!("Failed to create multiudpsink element"))?;

        udpsink.set_property("close-socket", false);
        udpsink.set_property("send-duplicates", false);

        if mcast {
            udpsink.set_property("auto-multicast", true);
            udpsink.set_property("multicast-iface", &"enp5s0, wlp1s0, eth0, en10");
            //udpsink.set_property("loop", false);
            //udpsink.set_property("ttl-mc", 10i32);
        }

        if is_rtp {
            udpsink.set_property("sync", true);
        } else {
            udpsink.set_property("sync", false);
            udpsink.set_property("buffer-size", 0x8000i32);
        }

        udpsink.set_property("async", false);

        Ok(udpsink)
    }

    /// set UDPSRC for receiving rtcp packets
    /// 
    fn _set_udpsrc() -> Result<gst::Element, anyhow::Error> {
        let udpsrc = gst::ElementFactory::make_with_name("udpsrc", Some("udprtscpsrc0"))
            .map_err(|_| anyhow::anyhow!("Failed to create udpsrc for rtcp receiving element"))?;

        let cap = gst::Caps::new_empty_simple("application/x-rtcp");
        udpsrc.set_property("caps", &cap);

        Ok(udpsrc)
    }


    

    fn _prepare_bin(with_rtcp: bool) -> Result<gst::Bin, anyhow::Error> {
        
        // prepare by creating an empty bin
        let bin = gst::Bin::new(Some("RTPServer0"));

        let queue = gst::ElementFactory::make_with_name("queue", Some("queue0"))?;
        // create a payloader to handle the audio stream
        let payloader = gst::ElementFactory::make_with_name("rtpL24pay", Some("pay0"))?;
        // 97 is for audio streams
        payloader.set_property("pt", 96u32);

        // try it out
        if let Some(hdr_ext) = gst_rtp::RTPHeaderExtension::create_from_uri(
            "urn:ietf:params:rtp-hdrext:ntp-64",
        ) {
            hdr_ext.set_id(1);
            payloader.emit_by_name::<()>("add-extension", &[&hdr_ext]);
        } else {
            warn!("could not extend rtp header extension");
        }

        // send stream to a multicast group
        let rtp_udp_sink  = Self::_set_udpsink(true, true)?;

        let rtpbin = gst::ElementFactory::make_with_name("rtpbin", Some("RTPBin0"))?;

        rtpbin.connect("on-ssrc-active", true, |data| {

            let rtpbin = data[0].get::<gst::Element>().unwrap();
            let sid = data[1].get::<u32>().unwrap();

            let session: gst::Element = rtpbin.emit_by_name("get-session", &[&sid]);
            let sdes: gst::Structure = session.property("sdes");
            debug!("SSRC active: {:?}", sdes);
            None
        });
        rtpbin.connect("on-ssrc-sdes", true, |data| {

            let rtpbin = data[0].get::<gst::Element>().unwrap();
            let sid = data[1].get::<u32>().unwrap();

            let session: gst::Element = rtpbin.emit_by_name("get-session", &[&sid]);
            let sdes: gst::Structure = session.property("sdes");
            debug!("on-ssrc-sdes received {:?}", sdes);
            None
        });

        rtpbin.connect("on-timeout", true, |data| {

            let rtpbin = data[0].get::<gst::Element>().unwrap();
            let sid = data[1].get::<u32>().unwrap();

            let session: gst::Element = rtpbin.emit_by_name("get-session", &[&sid]);
            let sdes: gst::Structure = session.property("sdes");
            debug!("Client timedout: {:?}", sdes);
            None
        });
        

        // some options and properties
        rtpbin.set_property_from_str("ntp-time-source", "clock-time");
        //rtpbin.set_property("use-pipeline-clock", &true);
        rtpbin.set_property("rtcp-sync-send-time", true);
        //rtpbin.set_property("do-retransmission", &false);

        // add rtpbin and udpsink to bin
        bin.add_many(&[&queue, &payloader, &rtpbin, &rtp_udp_sink])?;

        // link elements
        queue.link(&payloader)?;

        payloader.link_pads(Some("src"), &rtpbin, Some("send_rtp_sink_0"))?;
        rtpbin.link_pads(Some("send_rtp_src_0"), &rtp_udp_sink, Some("sink"))?; // send media stream on 5004

        if with_rtcp {
            let rtcp_udp_sink = Self::_set_udpsink(true, false)?;
            bin.add(&rtcp_udp_sink)?;
            rtpbin.link_pads(Some("send_rtcp_src_0"), &rtcp_udp_sink, Some("sink"))?; // send media stream on 5004

            // also link receiving part
            let rtcp_udp_src = Self::_set_udpsrc()?;
            bin.add(&rtcp_udp_src)?;
            rtcp_udp_src.link_pads(Some("src"), &rtpbin, Some("recv_rtcp_sink_0"))?;
        }

        // create a ghost pad as an internal entrypoint for this bin (here comes the audio stream)
        let ghost_pad = gst::GhostPad::with_target(Some("sink"), &queue.static_pad("sink").unwrap())?;
        bin.add_pad(&ghost_pad)?;

        Ok(bin)
    }
}