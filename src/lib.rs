//#![feature(const_convert)]

pub(crate) const CROSSFADE_TIME_MS: u64 = 1000;
pub(crate) const MAX_VOLUME_SPOT: f64 = 0.6;
pub(crate) const MIN_VOLUME_BROADCAST: f64 = 0.02;

pub(crate) const RTP_KEY: &str = "disou§fH398zJKMh&ufvgoih2u9hgsduihf!/&§$(§())2389dsiub;GT_Z2DFG9h=83F98h";
pub(crate) const RTP_MKI: &str = "fhiuh/289hsdHRDhsdiuh)nv2938hv9fhiuhsdgh28793h&/z7hf27893h87h87v7654rScx";

mod helpers;
mod player;
mod encryption;

pub mod scheduler;
pub mod broadcast;


pub use player::PlaybackClient;
pub use player::local_player::LocalPlayer;
pub use broadcast::Broadcast;
pub use scheduler::Scheduler;

pub use gst::glib;
