// we try to inform all clients about our main ip addresses over a broadcast every 5 seconds
// 
// 

use std::net::{IpAddr, UdpSocket, Ipv4Addr};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

use local_ip_address::list_afinet_netifas;
use log::info;

pub fn inform_clients() {



    thread::spawn(|| {

        let content = "micast-dj";

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
                    
                    info!("send micast-dj info for ip {}", broadcast_ip);
                    let socket = UdpSocket::bind("0.0.0.0:5015").unwrap();
                    socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                    socket.set_broadcast(true).unwrap();
                    socket.connect((broadcast_ip, 5015));
                    let res = socket.send(content.as_bytes());
                    if res.is_err() {
                        // try to reconnect...
                        println!("think we got an error... {:?}", res);
                    }
                }
            }

            thread::sleep(Duration::from_secs(5));

        }
    });

}

pub fn dedect_server_ip() -> Receiver<IpAddr> {

    let (sender, receiver) = std::sync::mpsc::channel();

    thread::spawn(move || {

        loop {
            let mut receiver_sockets = Vec::new();
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
                        Ipv4Addr::from(temp)
                    };
                    
                    let socket = UdpSocket::bind((broadcast_ip, 5015)).unwrap();
                    socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                    socket.set_broadcast(true).unwrap();
                    receiver_sockets.push(socket);

                }
            }

            std::thread::sleep(std::time::Duration::from_millis(10000));
            for s in receiver_sockets {
                println!("recv data...");
                let mut buffer = [0u8; 250];
                let res = s.recv_from(&mut buffer);
                match res {
                    Ok((size, addr)) => {
                        let data = std::str::from_utf8(&buffer[..size]).unwrap();
                        info!("received datagramm from {} with {}", addr, data);
                        let _ = sender.send(addr.clone().ip());
                        break;
                    },
                    _ => {}
                }
            }
        }
    });


    return receiver;
}

