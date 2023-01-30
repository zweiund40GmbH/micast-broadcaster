


#[derive(Default)]
pub struct Builder {
    server_ip: Option<String>, 
    tcp_port: Option<i32>,
    rate: Option<i32>,
    broadcast_ip: Option<String>,
    clock_port: Option<i32>,
    startup_output: super::OutputMode,
}


impl Builder {

    /// creates a Builder with default Values
    /// 
    pub fn new() -> Builder {
        Builder {
            ..Default::default()
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

    pub fn set_clock_port(mut self, port: i32) -> Self {
        self.clock_port = Some(port);
        self
    }

    pub fn set_broadcast_ip(mut self, bip: &str) -> Self {
        self.broadcast_ip = Some(bip.to_string());
        self
    }

    pub fn set_startup_output(mut self, output: super::OutputMode) -> Self {
        self.startup_output = output;
        self
    }

    /// # build the server
    pub fn build_server(&self) -> Result<super::Broadcast, anyhow::Error> {
        let ip = self.server_ip.clone();

        super::Broadcast::new(
            &ip.unwrap_or("127.0.0.1".to_string()),
            self.tcp_port.unwrap_or(3333),
            self.rate.unwrap_or(44100),
            self.clock_port.unwrap_or(8555),
            self.broadcast_ip.clone(),
            self.startup_output.clone(),
        )
    }
}

