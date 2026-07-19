//! NAV-10 first-entry, remembered-entry, and explicit-reveal policy.

use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::ui) enum LibraryMode {
    Tracks,
    Albums,
    Artists,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum EntryAction {
    RevealPlaying,
    RestoreRemembered,
    StayPut,
}

#[derive(Default)]
pub(in crate::ui) struct ContextAnchorPolicy {
    visited: HashSet<LibraryMode>,
}

impl ContextAnchorPolicy {
    pub(in crate::ui) fn enter(
        &mut self,
        mode: LibraryMode,
        playing_context_available: bool,
    ) -> EntryAction {
        if self.visited.contains(&mode) {
            return EntryAction::RestoreRemembered;
        }
        self.visited.insert(mode);
        if !playing_context_available {
            return EntryAction::StayPut;
        }
        EntryAction::RevealPlaying
    }

    pub(in crate::ui) fn has_visited(&self, mode: LibraryMode) -> bool {
        self.visited.contains(&mode)
    }

    #[cfg(test)]
    fn explicit_reveal(&mut self, mode: LibraryMode) -> EntryAction {
        self.visited.insert(mode);
        EntryAction::RevealPlaying
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_10_first_entry_lands_on_playing_context() {
        let mut policy = ContextAnchorPolicy::default();
        assert_eq!(
            policy.enter(LibraryMode::Tracks, true),
            EntryAction::RevealPlaying
        );
    }

    #[test]
    fn nav_10_subsequent_switch_restores_remembered_position() {
        let mut policy = ContextAnchorPolicy::default();
        assert_eq!(
            policy.enter(LibraryMode::Albums, true),
            EntryAction::RevealPlaying
        );
        assert_eq!(
            policy.enter(LibraryMode::Albums, true),
            EntryAction::RestoreRemembered
        );
    }

    #[test]
    fn nav_10_reveal_always_jumps() {
        let mut policy = ContextAnchorPolicy::default();
        assert_eq!(
            policy.enter(LibraryMode::Artists, true),
            EntryAction::RevealPlaying
        );
        assert_eq!(
            policy.explicit_reveal(LibraryMode::Artists),
            EntryAction::RevealPlaying
        );
    }
}
