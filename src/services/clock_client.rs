use std::any::Any;
use std::sync::Arc;
use std::time::Duration;
use zeroconf::prelude::*;
use zeroconf::{MdnsBrowser, ServiceDiscovery, ServiceType};
use std::sync::mpsc::{channel, Receiver};
use log::info;

pub fn service() -> Result<Receiver<ServiceDiscovery>, anyhow::Error>{

    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        loop {
            {
                info!("create clock browser (zeroconf)");

                let mut browser = MdnsBrowser::new(ServiceType::new("micast-ntp", "udp").unwrap());

                browser.set_context(Box::new(sender));
                browser.set_service_discovered_callback(Box::new(on_service_discovered));
            
                let event_loop = browser.browse_services().unwrap();

                loop {
                    // calling `poll()` will keep this service alive
                    info!("test...");
                    event_loop.poll(Duration::from_millis(0)).unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            } 
        }

    });

    Ok(receiver)
}

fn on_service_discovered(
    result: zeroconf::Result<ServiceDiscovery>,
    context: Option<Arc<dyn Any>>,
) {
    info!("clock Service discovered: {:?}", result.as_ref().unwrap());

    if let Ok(se) = result.as_ref() {
        if let Some(context) = context {
            if let Some(sender) = context.downcast_ref::<std::sync::mpsc::Sender<ServiceDiscovery>>() {
                let _ = sender.send(se.clone());
            }
        }

    }
    // ...
}