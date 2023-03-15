use std::sync::mpsc::Receiver;
pub use std::net::IpAddr;

pub fn service() -> Result<Receiver<IpAddr>, anyhow::Error>{
    Ok(super::informip::dedect_server_ip())
   // ...
}