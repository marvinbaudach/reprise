//! Capturing the place the user is leaving (`BROWSE-4a`).
//!
//! A frontend can show a page the router never heard about — the Library
//! Doctor is shown without a transition. Before an intent leaves such a page,
//! the frontend declares it here so Back can return to it.

use super::{
    same_destination, BrowserNavigation, BrowserPlace, NavigationIntent, NavigationTransition,
};

impl BrowserNavigation {
    /// Declares the *section* the user is looking at before an intent leaves it.
    ///
    /// A visible place the router already holds is refreshed like
    /// [`Self::replace_current`]. A section the router never heard about is
    /// entered as a new place, so the place it replaced stays one step behind
    /// it and Back out of the next jump returns here. A diverging *track*
    /// place is ignored: a track list can lag behind the section page that
    /// replaced it, and trusting it would overwrite the section the user
    /// actually left.
    pub fn observe_visible(&mut self, visible: BrowserPlace) -> bool {
        if same_destination(&self.current, &visible) {
            return self.replace_current(visible);
        }
        if visible.track_state().is_some() {
            return false;
        }
        self.go_new(visible).is_some()
    }

    /// Whether `place` names the destination the router is at.
    #[must_use]
    pub fn holds(&self, place: &BrowserPlace) -> bool {
        same_destination(&self.current, place)
    }

    /// The place one step back, which Back would restore.
    #[must_use]
    pub fn previous(&self) -> Option<&BrowserPlace> {
        self.back.last()
    }

    /// Runs `intent` from the visible *section* `visible`, recording the
    /// section only when the intent actually goes somewhere. An intent that
    /// yields no transition — an empty album, a track id that addresses
    /// nothing, Forward with nothing ahead — leaves the router untouched, so
    /// it neither enters the section into history nor clears Forward.
    pub fn navigate_observing(
        &mut self,
        visible: BrowserPlace,
        intent: NavigationIntent,
    ) -> Option<NavigationTransition> {
        if same_destination(&self.current, &visible) {
            self.replace_current(visible);
            return self.navigate(intent);
        }
        let mut trial = self.clone();
        trial.observe_visible(visible);
        let transition = trial.navigate(intent)?;
        *self = trial;
        Some(transition)
    }
}
