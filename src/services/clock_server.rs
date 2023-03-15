
use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zeroconf::prelude::*;
use zeroconf::{MdnsService, ServiceRegistration, ServiceType, EventLoop, TxtRecord};
use std::sync::mpsc::{channel, Sender};
use log::info;

#[derive(Default, Debug)]
pub struct Context {
    service_name: String,
}

pub fn service() -> Result<Sender<bool>, anyhow::Error> {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let mut run = true;
        loop {
            if run {
                info!("create clock distribution (zeroconf) service");
                let mut service = MdnsService::new(ServiceType::new("micast-ntp", "udp").unwrap(), 8555);
                let mut txt_record = TxtRecord::new();
                let context: Arc<Mutex<Context>> = Arc::default();

                txt_record.insert("micast-wall", "ntpclock").unwrap();

                service.set_registered_callback(Box::new(on_service_registered));
                service.set_context(Box::new(context));
                service.set_txt_record(txt_record);

                let event_loop = service.register().unwrap();

                while run {
                    // calling `poll()` will keep this service alive
                    event_loop.poll(Duration::from_millis(500)).unwrap();
                    if let Ok(r) = receiver.try_recv() {
                        run = r;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            } else {
                if let Ok(r) = receiver.recv() {
                    info!("receive information about stopping or starting clock zeroconf service {:?}", r);
                    run = r;
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }

        }

    });

    Ok(sender)
}

#[allow(unused)]
fn on_service_registered(
    result: zeroconf::Result<ServiceRegistration>,
    context: Option<Arc<dyn Any>>,
) {
    let service = result.unwrap();

    println!("Clock service registered: {:?}", service);

    let context = context
        .as_ref()
        .unwrap()
        .downcast_ref::<Arc<Mutex<Context>>>()
        .unwrap()
        .clone();

    context.lock().unwrap().service_name = service.name().clone();

    info!("zeroconf service Context: {:?}", context);

    // ...
}