// we try to inform all clients about our main ip addresses over a broadcast every 5 seconds
// 
// 

use std::net::{IpAddr, UdpSocket, Ipv4Addr};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

use log::info;

pub fn inform_clients() {

    use local_ip_address::list_afinet_netifas;

    let ifas = list_afinet_netifas().unwrap();

    for (name, ipaddr) in ifas {
        if matches!(ipaddr, IpAddr::V4(_)) && (!name.contains("lo") || ipaddr.is_loopback() == false ) && ipaddr.is_ipv4() {
            println!("This is your local IP address: {:?}, {}", ipaddr, name);
        }
    }

    thread::spawn(|| {

        let socket = UdpSocket::bind("0.0.0.0:5015").unwrap();
        socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        socket.set_broadcast(true).unwrap();
        let content = "micast-dj";

        loop {

            info!("send micast-dj info");
            socket.connect((IpAddr::V4(Ipv4Addr::BROADCAST), 5015));
            let res = socket.send(content.as_bytes());
            if res.is_err() {
                // try to reconnect...
                println!("think we got an error... {:?}", res);
            }
            thread::sleep(Duration::from_secs(5));

        }
    });

}

pub fn dedect_server_ip() -> Receiver<IpAddr> {

    let (sender, receiver) = std::sync::mpsc::channel();

    thread::spawn(|| {
        let socket = UdpSocket::bind((IpAddr::V4(Ipv4Addr::BROADCAST), 5015)).unwrap();
        socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        socket.set_broadcast(true).unwrap();

        loop {
            let mut buffer = [0u8; 250];
            let res = socket.recv_from(&mut buffer);
            match res {
                Ok((size, addr)) => {
                    let data = std::str::from_utf8(&buffer[..size]).unwrap();
                    info!("received datagramm from {} with {}", addr, data);

                },
                _ => {}
            }
        }
    });


    return receiver;
}

