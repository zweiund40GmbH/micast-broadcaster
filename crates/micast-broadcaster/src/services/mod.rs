pub mod dedector_server;
mod informip;
pub use informip::wait_for_broadcast;
pub use informip::confirm;
pub use informip::thread_for_confirm;

pub const RECONFIRMATIONTIME_IN_MS: u64 = 1200;
pub const TIMEOUT_CONFIRM_IN_MS: u64 = 5000;