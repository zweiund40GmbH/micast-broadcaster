
pub(crate) const CROSSFADE_TIME_MS: u64 = 1000;
pub(crate) const MAX_VOLUME_SPOT: f64 = 0.8;
pub(crate) const MIN_VOLUME_BROADCAST: f64 = 0.0;

mod helpers;
mod player;

pub mod scheduler;
pub mod broadcast;

pub use player::PlaybackClient;
pub use broadcast::Broadcast;
pub use scheduler::Scheduler;


