
/// #service sends the ip address of the server to the clients
/// 
/// This function is called by the main thread and is used to send the ip address of the server to the clients.
/// # Arguments
/// * `tcp_port` - the rtp port of the server
pub fn service(rtp_port: u32) -> Result<(), anyhow::Error> {
    Ok(super::informip::inform_clients(rtp_port))
}