//! Leaving the current track on purpose, as opposed to the Next gesture.

use super::{AndroidPlaybackError, AndroidPlaybackSession};
use reprise_core::queue::Queue;

#[uniffi::export]
impl AndroidPlaybackSession {
    /// Moves on from the current track, or stops when nothing follows it.
    ///
    /// [`Self::next`] is the user's gesture and does nothing on the last
    /// track with Repeat off (PLAY-8b). A caller that is taking the playing
    /// track away, such as a deletion, still has to leave it, so it asks for
    /// this instead.
    pub fn skip_current_or_stop(&self) -> Result<(), AndroidPlaybackError> {
        if self.inner.forward_from_history()? {
            return Ok(());
        }
        self.move_playhead(Queue::next_manual)
    }
}
