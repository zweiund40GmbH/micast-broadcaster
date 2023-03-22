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

pub fn inform_clients(broadcast_ip: &str, broadcast_port: u32) {


    let content = format!("micast-dj|{}|{}", broadcast_ip, broadcast_port);

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

pub fn dedect_server_ip() -> (Sender<bool>, Receiver<IpAddr>) {

    let (sender, receiver) = unbounded();
    let (stopsender, stopreceiver) = unbounded();

    thread::spawn(move || {
        let mut hold_receiving = true;
        while hold_receiving {

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
                        trace!("set {} to broadcast ip  {}", ip, broadcast_ip);
                        broadcast_ip
                    };
                    
                    let try_socket = UdpSocket::bind((broadcast_ip, BROADCAST_PORT));
                    if let Ok(socket) = try_socket {
                        trace!("listen on socket {:?} for ip {}", socket.local_addr().unwrap(), broadcast_ip);
                        socket.set_read_timeout(Some(std::time::Duration::from_millis(500))).unwrap();
                        socket.set_broadcast(true).unwrap();

                        trace!("check if we get some data from mainsystems.....");
                        let mut buffer = [0u8; 256];
                        let res = socket.recv_from(&mut buffer);
                        match res {
                            Ok((size, addr)) => {
                                let data = std::str::from_utf8(&buffer[..size]).unwrap();
                                info!("received datagramm from {} with {}", addr, data);
                                let _ = sender.send(addr.clone().ip());
                                break;
                            },
                            Err(e) => {
                                trace!("error on recv from broadcast: {:?}", e);
                            }
                        }
                    } else {
                        trace!("error on create socket for broadcast: {:?}", try_socket.err().unwrap());
                        sleep_ms!(500);
                    }

                }
            }

            if let Ok(stop) = stopreceiver.try_recv() {
                hold_receiving = stop;
            }
            sleep_ms!(542);
            trace!("next round...");
        }
    });


    return (stopsender, receiver);
}

