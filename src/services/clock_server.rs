
use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::sync::mpsc::{channel, Sender};
use log::info;


pub fn service() -> Result<(), anyhow::Error> {
    Ok(super::informip::inform_clients())
}