//! The per-display-frame answer: one FFI crossing instead of a tick followed
//! by two queries.

use super::{AndroidBassPressure, AndroidVisualEngine};

/// What the phone reads after advancing the engine by one display frame.
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct AndroidVisualFrame {
    /// The same answer [`AndroidVisualEngine::tick`] gives.
    pub advanced: bool,
    /// [`AndroidVisualEngine::has_live_audio`], read right after the tick.
    pub has_live_audio: bool,
    /// [`AndroidVisualEngine::bass_pressure`], read right after the tick.
    pub bass_pressure: AndroidBassPressure,
}

#[uniffi::export]
impl AndroidVisualEngine {
    /// Advances the engine like [`Self::tick`] and answers the two per-frame
    /// queries from the state that tick left behind, so Kotlin crosses the
    /// FFI boundary once per frame for all three.
    pub fn frame(&self) -> AndroidVisualFrame {
        let advanced = self.tick();
        AndroidVisualFrame {
            advanced,
            has_live_audio: self.has_live_audio(),
            bass_pressure: self.bass_pressure(),
        }
    }
}
