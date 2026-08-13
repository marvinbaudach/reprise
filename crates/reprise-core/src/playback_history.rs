//! PLAY-14: the actual playback history that Previous uses.
//!
//! The queue index is not history. With shuffle enabled,
//! [`crate::queue::Queue::set_tracks`] deliberately seeds the playhead at zero:
//! the activated track leads the shuffled order so no track silently falls out
//! of the queue. There is therefore nothing behind the playhead, and the old
//! `Queue::previous` stayed at index zero and returned the current track for
//! its caller to restart.
//!
//! This module records what actually played instead. It uses two stacks like a
//! browser's Back and Forward: `back` collects entries that were left,
//! `forward` is populated only by stepping back, and any ordinary transition
//! clears it.
//!
//! [`resolve_previous`] is the project's single Previous decision. All three
//! frontends call it; cases such as an Up Next track interrupting the context
//! are ordinary history entries rather than separate branches.
//!
//! This is runtime-only state: no database schema, no migration, and empty on
//! every application start. It is pure and GUI-free so every surface shares
//! the same semantics.

use std::collections::VecDeque;

use crate::up_next::QueueItem;

/// Above this position, Previous seeks to the start of the current item
/// instead of selecting an earlier one.
pub const PREVIOUS_RESTART_THRESHOLD_MS: i64 = 3_000;

/// Maximum number of entries retained on the backward stack.
pub const HISTORY_CAPACITY: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEntry {
    pub item: QueueItem,
    /// Surface-specific replay locator. Android stores its content URI here;
    /// surfaces that resolve stable IDs at playback time leave it empty.
    pub replay_uri: Option<String>,
    /// The context playhead when this entry played. `None` for anything that
    /// played beside the context, including Up Next and episodes.
    pub context_pos: Option<usize>,
    /// `Queue::sequence_identity()` when this entry was recorded.
    pub sequence: (u64, u64),
    /// Whether the entry came from Up Next or otherwise played beside context.
    pub from_up_next: bool,
}

impl HistoryEntry {
    /// Returns the recorded playhead only while the context is unchanged.
    pub fn playhead_in(&self, sequence: (u64, u64)) -> Option<usize> {
        (self.sequence == sequence)
            .then_some(self.context_pos)
            .flatten()
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlaybackHistory {
    back: VecDeque<HistoryEntry>,
    forward: VecDeque<HistoryEntry>,
    current: Option<HistoryEntry>,
}

impl PlaybackHistory {
    /// Records an ordinary transition and discards any forward branch.
    pub fn record(&mut self, entry: HistoryEntry) {
        if self.current.as_ref() == Some(&entry) {
            return;
        }
        self.forward.clear();
        if let Some(previous) = self.current.replace(entry) {
            self.back.push_back(previous);
            if self.back.len() > HISTORY_CAPACITY {
                self.back.pop_front();
            }
        }
    }

    pub fn step_back(&mut self) -> Option<HistoryEntry> {
        let target = self.back.pop_back()?;
        if let Some(leaving) = self.current.replace(target.clone()) {
            self.forward.push_back(leaving);
        }
        Some(target)
    }

    pub fn step_forward(&mut self) -> Option<HistoryEntry> {
        let target = self.forward.pop_back()?;
        if let Some(leaving) = self.current.replace(target.clone()) {
            self.back.push_back(leaving);
        }
        Some(target)
    }

    pub fn peek_back(&self) -> Option<HistoryEntry> {
        self.back.back().cloned()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    pub fn current(&self) -> Option<HistoryEntry> {
        self.current.clone()
    }

    pub fn back_len(&self) -> usize {
        self.back.len()
    }

    pub fn clear(&mut self) {
        self.back.clear();
        self.forward.clear();
        self.current = None;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviousAction {
    /// Seek to the beginning without restarting playback.
    RestartCurrent,
    GoTo(HistoryEntry),
}

/// Resolves the project's complete Previous behavior.
pub fn resolve_previous(position_ms: i64, history: &PlaybackHistory) -> PreviousAction {
    if position_ms > PREVIOUS_RESTART_THRESHOLD_MS {
        return PreviousAction::RestartCurrent;
    }
    match history.peek_back() {
        Some(entry) => PreviousAction::GoTo(entry),
        None => PreviousAction::RestartCurrent,
    }
}

#[cfg(test)]
#[path = "playback_history_tests.rs"]
mod tests;
