
pub(crate) const CROSSFADE_TIME_MS: u64 = 1000;

mod helpers;

mod player;


pub mod broadcast;


pub use player::PlaybackClient;
pub use broadcast::Broadcast;
pub use helpers::make_element;

