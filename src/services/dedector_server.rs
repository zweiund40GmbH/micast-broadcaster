
pub fn service(server_address: &str, tcp_port: u32) -> Result<(), anyhow::Error> {
    Ok(super::informip::inform_clients(server_address, tcp_port))
}