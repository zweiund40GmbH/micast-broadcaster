//#![feature(const_convert)]

pub(crate) const RTP_KEY: &str = "disou§fH398zJKMh&ufvgoih2u9hgsduihf!/&§$(§())2389dsiub;GT_Z2DFG9h=83F98h";
pub(crate) const RTP_MKI: &str = "fhiuh/289hsdHRDhsdiuh)nv2938hv9fhiuhsdgh28793h&/z7hf27893h87h87v7654rScx";

pub mod services;
mod helpers;
mod player;
mod encryption;
//pub mod rtspserver;
pub mod rtpserver;

pub mod broadcast;
pub mod output;


pub use player::PlaybackClient;
pub use player::local_player::LocalPlayer;
//pub use player::rtsp;
pub use broadcast::Broadcast;
//pub use scheduler::Scheduler;

pub use gst::glib;

