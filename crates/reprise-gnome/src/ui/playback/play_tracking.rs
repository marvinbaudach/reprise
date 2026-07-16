//! Local play-count and optional provider scrobbling paths extracted from
//! the edge-tight `player_controller` module.

use reprise_core::library::{stats, stats_screen};
use reprise_core::scrobbling::{self, TrackMetadata};

use crate::ui::player_controller::PlayerController;

fn active_providers(
    listenbrainz: bool,
    lastfm: bool,
) -> impl Iterator<Item = scrobbling::ScrobbleProvider> {
    [
        (listenbrainz, scrobbling::ScrobbleProvider::ListenBrainz),
        (lastfm, scrobbling::ScrobbleProvider::LastFm),
    ]
    .into_iter()
    .filter_map(|(active, provider)| active.then_some(provider))
}

impl PlayerController {
    pub(in crate::ui) fn begin_scrobble(&self, track: TrackMetadata) {
        if !self.listenbrainz.is_active() && !self.lastfm.is_active() {
            return;
        }
        if let Err(error) = track.validate() {
            tracing::debug!(%error, "track metadata cannot be scrobbled");
            return;
        }
        self.scrobble_session
            .borrow_mut()
            .begin(track.clone(), now_unix());
        self.listenbrainz.playing_now(track.clone());
        self.lastfm.playing_now(track);
    }

    /// Finishes both per-track accounting paths exactly once. The existing
    /// `current_track.take()` remains the idempotency guard for local counts;
    /// `ScrobbleSession::finish` independently takes its pending listen.
    pub(in crate::ui) fn evaluate_play_tracking(&self) {
        let Some((track_id, duration_ms)) = self.current_track.take() else {
            return;
        };
        let max_position_ms = self.max_position_ms.replace(0);

        let scrobble = self.scrobble_session.borrow_mut().finish(
            max_position_ms,
            self.listenbrainz.is_active() || self.lastfm.is_active(),
        );
        if let Some(listen) = scrobble {
            for provider in active_providers(self.listenbrainz.is_active(), self.lastfm.is_active())
            {
                let (service, runtime) = match provider {
                    scrobbling::ScrobbleProvider::ListenBrainz => {
                        ("ListenBrainz", self.listenbrainz.as_ref())
                    }
                    scrobbling::ScrobbleProvider::LastFm => ("Last.fm", self.lastfm.as_ref()),
                };
                self.enqueue_scrobble(provider, service, runtime, &listen);
            }
        }

        if !stats::should_count_play(max_position_ms, duration_ms) {
            return;
        }
        let result = {
            let conn = self.conn.borrow();
            stats::record_play(&conn, track_id, now_unix())
        };
        match result {
            Ok(()) => {
                tracing::debug!(track_id, max_position_ms, duration_ms, "play recorded");
            }
            Err(error) => {
                tracing::error!(%error, track_id, "failed to record play");
            }
        }

        self.record_local_listen_event(track_id, max_position_ms, duration_ms);
    }

    /// Records a per-play `listen_event` for the local "My Stats" screen. This
    /// runs for *every* completed play that crosses the listen threshold,
    /// independent of whether any scrobble provider is active — local stats
    /// count all qualifying plays. It deliberately reuses the same threshold
    /// predicate as scrobbling (`scrobbling::should_scrobble`, four minutes or
    /// half the track) rather than the looser play-count predicate, so an
    /// event only lands when the play would also have been scrobble-worthy.
    fn record_local_listen_event(&self, track_id: i64, max_position_ms: i64, duration_ms: i64) {
        if !scrobbling::should_scrobble(max_position_ms, duration_ms) {
            return;
        }
        let result = {
            let conn = self.conn.borrow();
            stats_screen::record_listen_event(&conn, track_id, now_unix(), max_position_ms)
        };
        if let Err(error) = result {
            tracing::error!(%error, track_id, "failed to record listen event");
        }
    }

    fn enqueue_scrobble(
        &self,
        provider: scrobbling::ScrobbleProvider,
        service: &str,
        runtime: &crate::ui::scrobble_runtime::ScrobbleRuntime,
        listen: &scrobbling::Listen,
    ) {
        if !runtime.is_active() {
            return;
        }
        let queued = scrobbling::enqueue_for(&self.conn.borrow(), provider, listen);
        match queued {
            Ok(queue_id) => {
                tracing::debug!(queue_id, service, "scrobble queued");
                runtime.flush();
            }
            Err(error) => tracing::warn!(%error, service, "could not queue scrobble"),
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_active_provider_produces_no_scrobble_destination() {
        assert_eq!(active_providers(false, false).count(), 0);
    }

    #[test]
    fn listenbrainz_can_be_the_only_scrobble_destination() {
        assert_eq!(
            active_providers(true, false).collect::<Vec<_>>(),
            vec![scrobbling::ScrobbleProvider::ListenBrainz]
        );
    }

    #[test]
    fn lastfm_can_be_the_only_scrobble_destination() {
        assert_eq!(
            active_providers(false, true).collect::<Vec<_>>(),
            vec![scrobbling::ScrobbleProvider::LastFm]
        );
    }

    #[test]
    fn one_completed_listen_targets_both_active_providers() {
        assert_eq!(
            active_providers(true, true).collect::<Vec<_>>(),
            vec![
                scrobbling::ScrobbleProvider::ListenBrainz,
                scrobbling::ScrobbleProvider::LastFm,
            ]
        );
    }
}
