//! Command implementations. Each function opens the database through the core
//! facade, calls into `reprise-core`, and renders text or JSON.

pub mod events;
pub mod instrumental;
pub mod instrumental_wait;
pub mod jobs;
pub mod library;
#[cfg(feature = "mpris")]
pub mod playback;
pub mod playlist;
pub mod scan;
pub mod search;
#[cfg(feature = "worker")]
pub mod worker;
