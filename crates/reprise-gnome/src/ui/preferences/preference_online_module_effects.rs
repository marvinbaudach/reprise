use std::rc::Rc;

use reprise_core::connectivity::Connectivity;

use super::PreferencesContext;

pub(super) type ArtworkPermissionCallback = Rc<dyn Fn(bool)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermissionEffect {
    None,
    Start,
    Stop,
}

fn effect_for_transition(
    was_allowed: bool,
    is_allowed: bool,
    connectivity: Connectivity,
) -> PermissionEffect {
    match (was_allowed, is_allowed, connectivity) {
        (true, false, _) => PermissionEffect::Stop,
        (false, true, Connectivity::Online) => PermissionEffect::Start,
        _ => PermissionEffect::None,
    }
}

impl PreferencesContext {
    pub(in crate::ui) fn set_connectivity(&self, connectivity: Connectivity) {
        self.connectivity.set(connectivity);
    }

    pub(in crate::ui) fn set_on_artwork_permission_changed(
        &self,
        callback: impl Fn(bool) + 'static,
    ) {
        self.on_artwork_permission_changed
            .replace(Some(Rc::new(callback)));
    }

    /// `NET-1a`: persists the global online-sources gate and re-derives every
    /// cached module permission. Work starts only when the persisted change
    /// creates a fresh, currently-online off-to-on transition.
    pub(in crate::ui) fn set_online_sources_enabled(
        &self,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::online_sources::set_enabled(&self.conn, enabled)?;
        self.refresh_online_module_state("online sources gate toggled");
        Ok(())
    }

    pub(in crate::ui) fn refresh_online_module_state(&self, reason: &'static str) {
        let artwork_was_allowed = self.cover_download.enabled.get();
        let lyrics_was_allowed = self.lyrics_batch.permission_enabled();

        self.cover_download.recompute_enabled(&self.conn);
        self.artist_portrait.recompute_enabled(&self.conn);
        self.artist_news.recompute_enabled(&self.conn);
        self.concerts.recompute_enabled(&self.conn);
        self.podcasts.recompute_enabled(&self.conn);
        // `SRC-11`: source artwork keeps its gate in a process-wide atomic the
        // artwork workers read, so it has to be republished here too.
        crate::ui::podcasts::source_image::recompute_gate(&self.conn);
        if let Some(player) = &self.player {
            player.recompute_lyrics_enabled();
        }
        let lyrics_is_allowed = self.lyrics_batch.republish_enabled();

        self.apply_artwork_effect(effect_for_transition(
            artwork_was_allowed,
            self.cover_download.enabled.get(),
            self.connectivity.get(),
        ));
        match effect_for_transition(
            lyrics_was_allowed,
            lyrics_is_allowed,
            self.connectivity.get(),
        ) {
            PermissionEffect::Start => self.lyrics_batch.start(),
            PermissionEffect::Stop => self.lyrics_batch.cancel(),
            PermissionEffect::None => {}
        }
        self.sidebar.refresh(reason);
    }

    fn apply_artwork_effect(&self, effect: PermissionEffect) {
        let enabled = match effect {
            PermissionEffect::Start => true,
            PermissionEffect::Stop => false,
            PermissionEffect::None => return,
        };
        let callback = self.on_artwork_permission_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(enabled);
        }
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::connectivity::Connectivity;

    use super::{effect_for_transition, PermissionEffect};

    #[test]
    fn an_online_off_to_on_transition_starts_once() {
        assert_eq!(
            effect_for_transition(false, true, Connectivity::Online),
            PermissionEffect::Start
        );
        assert_eq!(
            effect_for_transition(true, true, Connectivity::Online),
            PermissionEffect::None
        );
    }

    #[test]
    fn an_offline_off_to_on_transition_waits_without_failure() {
        assert_eq!(
            effect_for_transition(false, true, Connectivity::Offline),
            PermissionEffect::None
        );
    }

    #[test]
    fn an_on_to_off_transition_stops_even_while_offline() {
        assert_eq!(
            effect_for_transition(true, false, Connectivity::Offline),
            PermissionEffect::Stop
        );
    }
}
