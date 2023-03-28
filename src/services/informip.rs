// we try to inform all clients about our main ip addresses over a broadcast every 5 seconds
// 
// 
use crate::sleep_ms;
use std::net::{IpAddr, UdpSocket, Ipv4Addr};
use crossbeam_channel::{Sender, Receiver};
use crossbeam_channel::unbounded;
use std::thread;
use std::time::Duration;

use local_ip_address::list_afinet_netifas;
use log::{info, trace, warn, debug};



const BROADCAST_PORT:u16 = 5889;
const CONFIRMATION_PORT:u16 = 5887;

pub fn inform_clients(broadcast_ip: &str, broadcast_port: u32) {


    let content = format!("micast-dj|{}|{}|\n", broadcast_ip, broadcast_port);

    thread::spawn(move || {

        let content = &content;

        loop {

            let ifas = list_afinet_netifas().unwrap();

            for (name, ipaddr) in ifas {
                if matches!(ipaddr, IpAddr::V4(_)) && (!name.contains("lo") || ipaddr.is_loopback() == false ) && ipaddr.is_ipv4() {
                    //println!("This is your local IP address: {:?}, {}", ipaddr, name);

                    // make broadcast ip
                    let broadcast_ip = { 
                        let ip =  match ipaddr {
                            IpAddr::V4(ip) => {
                                ip
                            }, 
                            _ => break
                        };
                        let mut temp = ip.octets();
                        temp[3] = 0xff;
                        Ipv4Addr::from(temp)
                    };
                    
                    debug!("send micast-dj info for ip {}", broadcast_ip);
                    //let socket = UdpSocket::bind(format!("0.0.0.0:{}", BROADCAST_PORT)).unwrap();
                    let try_socket = UdpSocket::bind("0.0.0.0:0");
                    if let Ok(socket) = try_socket {
                        socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                        socket.set_broadcast(true).unwrap();
                        let _ = socket.connect((broadcast_ip, BROADCAST_PORT));
                        let res = socket.send(content.as_bytes());
                        if res.is_err() {
                            // try to reconnect...
                            println!("think we got an error... {:?}", res);
                        }

                    } else {
                        warn!("error on create socket for server inform clients: {:?}", try_socket.err());
                        sleep_ms!(500);
                    }
                }
            }

            thread::sleep(Duration::from_secs(5));

        }
    });

}


/// Wait a specific Duration for a broadcast message
/// 
/// # Returns
/// Option<(IpAddr, String)> - the ip address where the broadcast comes from and the data where RTP Streams are send to
pub fn wait_for_broadcast(timeout: std::time::Duration) -> Option<(IpAddr, String)> {
    let start_instant = std::time::Instant::now();
    while start_instant.elapsed() < timeout {

        let ifas = list_afinet_netifas().unwrap();
        for (name, ipaddr) in ifas {
            if matches!(ipaddr, IpAddr::V4(_)) && (!name.contains("lo") || ipaddr.is_loopback() == false ) && ipaddr.is_ipv4() {
                // make broadcast ip
                let broadcast_ip = { 
                    let ip =  match ipaddr {
                        IpAddr::V4(ip) => {
                            ip
                        }, 
                        _ => break
                    };
                    let mut temp = ip.octets();
                    temp[3] = 0xff;
                    let broadcast_ip = Ipv4Addr::from(temp);
                    broadcast_ip
                };
                
                let try_socket = UdpSocket::bind((broadcast_ip, BROADCAST_PORT));
                if let Ok(socket) = try_socket {
                    trace!("listen on socket {:?} for ip {}", socket.local_addr().unwrap(), broadcast_ip);
                    socket.set_read_timeout(Some(std::time::Duration::from_millis(500))).unwrap();
                    socket.set_broadcast(true).unwrap();

                    let mut buffer = [0u8; 256];
                    let res = socket.recv_from(&mut buffer);
                    match res {
                        Ok((size, addr)) => {
                            let data = std::str::from_utf8(&buffer[..size]).unwrap();
                            let d: Vec<&str> = data.split("|").collect();
                            if d.len() < 1 {
                                warn!("received datagramm from {} with wrong data {}", addr, data);
                                break
                            }
                            info!("received datagramm from {} with {}", addr, data);
                            return Some((addr.clone().ip(), d[1].to_string()))
                        },
                        Err(e) => {
                            trace!("error on recv from broadcast: {:?}", e);
                        }
                    }
                } else {
                    trace!("error on create socket for broadcast: {:?}", try_socket.err().unwrap());
                    sleep_ms!(200);
                }

            }
        }
    }
    
    None
}


pub fn confirm(server_ip: &str) {


    let content = format!("mirror|\n");
    let addr = format!("{}:{}", server_ip, CONFIRMATION_PORT);

    thread::spawn(move || {

        let content = &content;
                    
        let try_socket = UdpSocket::bind(format!("0.0.0.0:0"));
        if let Ok(socket) = try_socket {
            socket.set_read_timeout(Some(std::time::Duration::from_millis(500))).unwrap();
            let _ = socket.connect(addr);
            let res = socket.send(content.as_bytes());
            if res.is_err() {
                // try to reconnect...
                println!("think we got an error... {:?}", res);
            }

        } else {
            warn!("error on create socket for server inform clients: {:?}", try_socket.err());
            sleep_ms!(500);
        }

        thread::sleep(Duration::from_secs(2));

    });

}


pub fn thread_for_confirm() -> Result<(Receiver<(IpAddr, String)>, Sender<bool>), Box<dyn std::error::Error>> {
    let (send_client, receive_client) = unbounded::<(IpAddr, String)>();
    let (send_stop, recevie_stop) = unbounded::<bool>();
    
    thread::spawn(move || {
        let mut keep_runnin = true;

        while keep_runnin {
            debug!("wait for confirmations...");

            let try_socket = UdpSocket::bind(("0.0.0.0", CONFIRMATION_PORT));
            if let Ok(socket) = try_socket {
                info!("create socket for confirmation on port {}", CONFIRMATION_PORT);
                while keep_runnin {
                    // if we receive stop, stop!
                    if let Ok(stop) = recevie_stop.try_recv() {
                        keep_runnin = stop;
                    }
                    let mut buffer = [0u8; 256];
                    let res = socket.recv_from(&mut buffer);
                    match res {
                        Ok((size, addr)) => {
                            let data = std::str::from_utf8(&buffer[..size]).unwrap();
                            let d: Vec<&str> = data.split("|").collect();
                            if d.len() < 1 {
                                trace!("received confirmation from {} with wrong data {}", addr, data);
                                break
                            }
                            trace!("received confirmation from {} with {}", addr, data);
                            send_client.try_send((addr.clone().ip(), d[1].to_string()));
                        },
                        Err(e) => {
                            trace!("error on recv from confirmation: {:?}", e);
                        }
                    }
                    sleep_ms!(300);
                }

            } else {
                warn!("error on create socket for confirmation: {:?}", try_socket.err().unwrap());
                sleep_ms!(200);
            }

            // if we receive stop, stop!
            if let Ok(stop) = recevie_stop.try_recv() {
                keep_runnin = stop;
            }

        }

        debug!("stop thread for confirm");

    });

    Ok((receive_client, send_stop))
}