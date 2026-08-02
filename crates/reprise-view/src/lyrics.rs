//! Pure current-track and active-line state for the Lyrics surface.

use std::path::PathBuf;

use reprise_core::lyrics::{active_line_index, LyricsBody, LyricsHit, LyricsQuery};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsTrack {
    pub query: LyricsQuery,
    pub track_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestIntent {
    pub generation: u64,
    pub track: LyricsTrack,
    pub force: bool,
}

#[derive(Debug, Default)]
pub struct LyricsState {
    generation: u64,
    track: Option<LyricsTrack>,
    hit: Option<LyricsHit>,
    active_line: Option<usize>,
}

/// P1a's binding rule: no shared view state may hold a closure, because
/// UniFFI cannot carry one across an FFI boundary. `Rc<dyn Fn>` is neither
/// `Send` nor `Sync`, so this permanent guard rejects such a regression.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LyricsState>();
};

impl LyricsState {
    pub fn set_track(&mut self, track: Option<LyricsTrack>) -> Option<RequestIntent> {
        if self.track == track {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.track = track.clone();
        self.hit = None;
        self.active_line = None;
        track.map(|track| RequestIntent {
            generation: self.generation,
            track,
            force: false,
        })
    }

    pub fn retry(&mut self) -> Option<RequestIntent> {
        let track = self.track.clone()?;
        self.generation = self.generation.wrapping_add(1);
        self.hit = None;
        self.active_line = None;
        Some(RequestIntent {
            generation: self.generation,
            track,
            force: true,
        })
    }

    pub fn request_missing(&mut self) -> Option<RequestIntent> {
        if self.hit.is_some() {
            return None;
        }
        let track = self.track.clone()?;
        self.generation = self.generation.wrapping_add(1);
        Some(RequestIntent {
            generation: self.generation,
            track,
            force: false,
        })
    }

    pub fn request_upgrade(&mut self, force: bool) -> Option<RequestIntent> {
        let track = self.track.clone()?;
        self.generation = self.generation.wrapping_add(1);
        Some(RequestIntent {
            generation: self.generation,
            track,
            force,
        })
    }

    pub fn accepts(&self, generation: u64) -> bool {
        self.track.is_some() && self.generation == generation
    }

    pub fn set_hit(&mut self, hit: LyricsHit) {
        self.hit = Some(hit);
        self.active_line = None;
    }

    pub fn update_position(&mut self, position_ms: i64) -> Option<Option<usize>> {
        let next = match self.body() {
            Some(LyricsBody::Synced(lines)) => active_line_index(lines, position_ms),
            Some(LyricsBody::Plain(_) | LyricsBody::Instrumental) | None => None,
        };
        if next == self.active_line {
            return None;
        }
        self.active_line = next;
        Some(next)
    }

    pub fn query(&self) -> Option<&LyricsQuery> {
        self.track.as_ref().map(|track| &track.query)
    }

    pub fn body(&self) -> Option<&LyricsBody> {
        self.hit.as_ref().map(|hit| &hit.body)
    }

    pub fn hit(&self) -> Option<&LyricsHit> {
        self.hit.as_ref()
    }

    pub fn active_line(&self) -> Option<usize> {
        self.active_line
    }

    pub fn active_line_timestamp_ms(&self) -> Option<i64> {
        let LyricsBody::Synced(lines) = self.body()? else {
            return None;
        };
        self.active_line
            .and_then(|index| lines.get(index))
            .map(|line| line.start_ms)
    }

    pub fn next_line_timestamp_ms(&self, position_ms: i64) -> Option<i64> {
        let LyricsBody::Synced(lines) = self.body()? else {
            return None;
        };
        lines
            .iter()
            .find(|line| line.start_ms > position_ms)
            .map(|line| line.start_ms)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use reprise_core::lyrics::{LyricsBody, LyricsHit, LyricsQuery, LyricsSource, TimedLine};

    use super::*;

    fn query(title: &str) -> LyricsQuery {
        LyricsQuery {
            title: title.into(),
            artist: "Synthetic Artist".into(),
            album: "Synthetic Album".into(),
            duration_ms: 10_000,
        }
    }

    fn track(title: &str) -> LyricsTrack {
        LyricsTrack {
            query: query(title),
            track_path: Some(PathBuf::from(format!("/music/{title}.flac"))),
        }
    }

    fn hit(body: LyricsBody) -> LyricsHit {
        LyricsHit {
            body,
            source: LyricsSource::Lrclib,
        }
    }

    fn synced() -> LyricsHit {
        hit(LyricsBody::Synced(vec![
            TimedLine::new(1_000, "first synthetic line"),
            TimedLine::new(2_000, "second synthetic line"),
        ]))
    }

    #[test]
    fn track_changes_advance_generation_while_same_identity_is_idempotent() {
        let mut state = LyricsState::default();
        let first = state.set_track(Some(track("One"))).unwrap();
        assert_eq!(first.generation, 1);
        assert_eq!(first.track, track("One"));
        assert!(state.accepts(1));

        state.set_hit(synced());
        assert!(state.set_track(Some(track("One"))).is_none());
        assert!(state.body().is_some());

        let second = state.set_track(Some(track("Two"))).unwrap();
        assert_eq!(second.generation, 2);
        assert!(state.body().is_none());
        assert!(!state.accepts(first.generation));
        assert!(state.accepts(second.generation));
    }

    #[test]
    fn clear_and_retry_invalidate_old_responses_without_losing_identity() {
        let mut state = LyricsState::default();
        let first = state.set_track(Some(track("One"))).unwrap();
        let retry = state.retry().unwrap();
        assert_eq!(retry.track, first.track);
        assert!(retry.force);
        assert_eq!(retry.generation, first.generation + 1);
        assert!(!state.accepts(first.generation));

        assert!(state.set_track(None).is_none());
        assert!(state.query().is_none());
        assert!(state.body().is_none());
        assert!(!state.accepts(retry.generation));
        assert!(state.retry().is_none());
    }

    #[test]
    fn online_upgrade_keeps_a_local_plain_hit_visible_while_generation_advances() {
        let mut state = LyricsState::default();
        let initial = state.set_track(Some(track("One"))).unwrap();
        state.set_hit(hit(LyricsBody::Plain("local text".into())));

        let upgrade = state.request_upgrade(false).unwrap();

        assert_eq!(upgrade.track, initial.track);
        assert!(!upgrade.force);
        assert_eq!(state.body(), Some(&LyricsBody::Plain("local text".into())));
        assert!(state.accepts(upgrade.generation));
        assert!(!state.accepts(initial.generation));
    }

    #[test]
    fn synchronized_position_reports_only_real_active_line_changes() {
        let mut state = LyricsState::default();
        state.set_track(Some(track("One")));
        state.set_hit(synced());

        assert_eq!(state.update_position(999), None);
        assert_eq!(state.update_position(1_000), Some(Some(0)));
        assert_eq!(state.update_position(1_999), None);
        assert_eq!(state.update_position(2_000), Some(Some(1)));
        assert_eq!(state.update_position(500), Some(None));
        assert_eq!(state.update_position(500), None);
    }

    #[test]
    fn synchronized_body_reports_the_next_future_line_boundary() {
        let mut state = LyricsState::default();
        state.set_track(Some(track("One")));
        state.set_hit(synced());

        assert_eq!(state.next_line_timestamp_ms(0), Some(1_000));
        assert_eq!(state.next_line_timestamp_ms(1_000), Some(2_000));
        assert_eq!(state.next_line_timestamp_ms(1_999), Some(2_000));
        assert_eq!(state.next_line_timestamp_ms(2_000), None);
        assert_eq!(state.next_line_timestamp_ms(20_000), None);
    }

    #[test]
    fn unsynchronized_bodies_never_produce_an_active_line() {
        let mut state = LyricsState::default();
        state.set_track(Some(track("One")));
        for body in [
            LyricsBody::Plain("synthetic plain text".into()),
            LyricsBody::Instrumental,
        ] {
            state.set_hit(hit(body));
            assert_eq!(state.update_position(5_000), None);
            assert_eq!(state.active_line(), None);
        }
    }
}
