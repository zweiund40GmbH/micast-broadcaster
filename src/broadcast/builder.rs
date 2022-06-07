


#[derive(Default)]
pub struct Builder {
    server_ip: Option<String>, 
    tcp_port: Option<i32>,
    rate: Option<i32>,
    /*rtp_sender_port:  Option<i32>, 
    rtcp_sender_port: Option<i32>, 
    rtcp_receive_port: Option<i32>, 
    clock_port: Option<i32>, 
    multicast_interface: Option<String>,*/
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
            ..Default::default()

            /*clock_port: Some(8555),
            rtp_sender_port: Some(5000),
            rtcp_sender_port: Some(5001),
            rtcp_receive_port: Some(5007),
            ..Default::default()*/
        }
    }

    /// set the server ip, can also be a broadcast ip
    pub fn set_server_ip(mut self, server_ip: &str) -> Self {
        self.server_ip = Some(server_ip.to_string());
        self
    }

    /// set the tcp_port where the tcpsink opens a server
    pub fn set_tcp_port(mut self, tcp_port: i32) -> Self {
        self.tcp_port = Some(tcp_port);
        self
    }

    pub fn set_audiorate(mut self, rate: i32) -> Self {
        self.rate = Some(rate);
        self
    }

    /*/// set the rtp send port (per default 5000)
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

    pub fn set_multicast_interface(mut self, interf: &str) -> Self {
        self.multicast_interface = Some(interf.to_string());
        self
    }*/

    /// # build the server
    pub fn build_server(&self) -> Result<super::Broadcast, anyhow::Error> {
        let ip = self.server_ip.clone();

        super::Broadcast::new(
            &ip.unwrap_or("127.0.0.1".to_string()),
            self.tcp_port.unwrap_or(3333),
            self.rate.unwrap_or(44100),
            /*self.rtp_sender_port.unwrap_or_default(),
            self.rtcp_sender_port.unwrap_or_default(),
            self.rtcp_receive_port.unwrap_or_default(),
            self.clock_port.unwrap_or_default(),
            self.multicast_interface.clone(),*/
        )
    }
}

