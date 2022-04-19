


#[derive(Default)]
pub struct Builder {
    server_ip: Option<String>, 
    rtp_sender_port:  Option<i32>, 
    rtcp_sender_port: Option<i32>, 
    rtcp_receive_port: Option<i32>, 
    clock_port: Option<i32>, 
}


impl Builder {

    /// creates a Builder with default Values
    /// 
    /// - `clock_port` 8555
    /// - `rtp_sender_port` 5000
    /// - `rtcp_sender_port` 5001
    /// - `rtcp_receiver_port` 5007
    pub fn new() -> Builder {
        Builder {
            clock_port: Some(8555),
            rtp_sender_port: Some(5000),
            rtcp_sender_port: Some(5001),
            rtcp_receive_port: Some(5007),
            ..Default::default()
        }
    }

    /// set the server ip, can also be a broadcast ip
    pub fn set_server_ip(mut self, server_ip: &str) -> Self {
        self.server_ip = Some(server_ip.to_string());
        self
    }

    /// set the rtp send port (per default 5000)
    pub fn set_rtp_sender_port(mut self, port: i32) -> Self {
        self.rtp_sender_port = Some(port);
        self
    }

    /// set the rtcp send port (per default 5001) 
    /// 
    /// which is the control port for RTP which is send to all clients
    pub fn set_rtcp_sender_port(mut self, port: i32) -> Self {
        self.rtcp_sender_port = Some(port);
        self
    }

    /// set the rtcp receive port (per default 5007) 
    /// 
    /// which is the control port for RTP which received from clients
    pub fn set_rtcp_receive_port(mut self, port: i32) -> Self {
        self.rtcp_receive_port = Some(port);
        self
    }

    pub fn set_clock_port(mut self, port: i32) -> Self {
        self.clock_port = Some(port);
        self
    }

    /// # build the server
    pub fn build_server(&self) -> Result<super::Broadcast, anyhow::Error> {
        let ip = self.server_ip.clone();

        super::Broadcast::new(
            &ip.unwrap_or_default(),
            self.rtp_sender_port.unwrap_or_default(),
            self.rtcp_sender_port.unwrap_or_default(),
            self.rtcp_receive_port.unwrap_or_default(),
            self.clock_port.unwrap_or_default(),
        )
    }
}



/* .set_server_ip("224.1.1.1")
        .set_clock_port(8555)
        .set_rtp_port(5000)
        .set_rtcp_port(5001)
        .set_rtcp_receive_port(5007)
        */