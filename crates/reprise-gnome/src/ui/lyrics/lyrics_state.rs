//! Pure current-track and active-line state for the Lyrics surface.

use reprise_core::lyrics::{active_line_index, LyricsBody, LyricsQuery};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct RequestIntent {
    pub(in crate::ui) generation: u64,
    pub(in crate::ui) query: LyricsQuery,
}

#[derive(Debug, Default)]
pub(in crate::ui) struct LyricsState {
    generation: u64,
    query: Option<LyricsQuery>,
    body: Option<LyricsBody>,
    active_line: Option<usize>,
}

impl LyricsState {
    pub(in crate::ui) fn set_track(&mut self, query: Option<LyricsQuery>) -> Option<RequestIntent> {
        if self.query == query {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.query = query.clone();
        self.body = None;
        self.active_line = None;
        query.map(|query| RequestIntent {
            generation: self.generation,
            query,
        })
    }

    pub(in crate::ui) fn retry(&mut self) -> Option<RequestIntent> {
        let query = self.query.clone()?;
        self.generation = self.generation.wrapping_add(1);
        self.body = None;
        self.active_line = None;
        Some(RequestIntent {
            generation: self.generation,
            query,
        })
    }

    pub(in crate::ui) fn request_missing(&mut self) -> Option<RequestIntent> {
        if self.body.is_some() {
            return None;
        }
        self.retry()
    }

    pub(in crate::ui) fn accepts(&self, generation: u64) -> bool {
        self.query.is_some() && self.generation == generation
    }

    pub(in crate::ui) fn set_body(&mut self, body: LyricsBody) {
        self.body = Some(body);
        self.active_line = None;
    }

    pub(in crate::ui) fn update_position(&mut self, position_ms: i64) -> Option<Option<usize>> {
        let next = match self.body.as_ref() {
            Some(LyricsBody::Synced(lines)) => active_line_index(lines, position_ms),
            Some(LyricsBody::Plain(_) | LyricsBody::Instrumental) | None => None,
        };
        if next == self.active_line {
            return None;
        }
        self.active_line = next;
        Some(next)
    }

    pub(in crate::ui) fn query(&self) -> Option<&LyricsQuery> {
        self.query.as_ref()
    }

    pub(in crate::ui) fn body(&self) -> Option<&LyricsBody> {
        self.body.as_ref()
    }

    pub(in crate::ui) fn active_line(&self) -> Option<usize> {
        self.active_line
    }

    pub(in crate::ui) fn active_line_timestamp_ms(&self) -> Option<i64> {
        let LyricsBody::Synced(lines) = self.body.as_ref()? else {
            return None;
        };
        self.active_line
            .and_then(|index| lines.get(index))
            .map(|line| line.start_ms)
    }

    pub(in crate::ui) fn next_line_timestamp_ms(&self, position_ms: i64) -> Option<i64> {
        let LyricsBody::Synced(lines) = self.body.as_ref()? else {
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
    use reprise_core::lyrics::{LyricsBody, LyricsQuery, TimedLine};

    use super::*;

    fn query(title: &str) -> LyricsQuery {
        LyricsQuery {
            title: title.into(),
            artist: "Synthetic Artist".into(),
            album: "Synthetic Album".into(),
            duration_ms: 10_000,
        }
    }

    fn synced() -> LyricsBody {
        LyricsBody::Synced(vec![
            TimedLine::new(1_000, "first synthetic line"),
            TimedLine::new(2_000, "second synthetic line"),
        ])
    }

    #[test]
    fn track_changes_advance_generation_while_same_identity_is_idempotent() {
        let mut state = LyricsState::default();
        let first = state.set_track(Some(query("One"))).unwrap();
        assert_eq!(first.generation, 1);
        assert_eq!(first.query, query("One"));
        assert!(state.accepts(1));

        state.set_body(synced());
        assert!(state.set_track(Some(query("One"))).is_none());
        assert!(state.body().is_some());

        let second = state.set_track(Some(query("Two"))).unwrap();
        assert_eq!(second.generation, 2);
        assert!(state.body().is_none());
        assert!(!state.accepts(first.generation));
        assert!(state.accepts(second.generation));
    }

    #[test]
    fn clear_and_retry_invalidate_old_responses_without_losing_identity() {
        let mut state = LyricsState::default();
        let first = state.set_track(Some(query("One"))).unwrap();
        let retry = state.retry().unwrap();
        assert_eq!(retry.query, first.query);
        assert_eq!(retry.generation, first.generation + 1);
        assert!(!state.accepts(first.generation));

        assert!(state.set_track(None).is_none());
        assert!(state.query().is_none());
        assert!(state.body().is_none());
        assert!(!state.accepts(retry.generation));
        assert!(state.retry().is_none());
    }

    #[test]
    fn synchronized_position_reports_only_real_active_line_changes() {
        let mut state = LyricsState::default();
        state.set_track(Some(query("One")));
        state.set_body(synced());

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
        state.set_track(Some(query("One")));
        state.set_body(synced());

        assert_eq!(state.next_line_timestamp_ms(0), Some(1_000));
        assert_eq!(state.next_line_timestamp_ms(1_000), Some(2_000));
        assert_eq!(state.next_line_timestamp_ms(1_999), Some(2_000));
        assert_eq!(state.next_line_timestamp_ms(2_000), None);
        assert_eq!(state.next_line_timestamp_ms(20_000), None);
    }

    #[test]
    fn unsynchronized_bodies_never_produce_an_active_line() {
        let mut state = LyricsState::default();
        state.set_track(Some(query("One")));
        for body in [
            LyricsBody::Plain("synthetic plain text".into()),
            LyricsBody::Instrumental,
        ] {
            state.set_body(body);
            assert_eq!(state.update_position(5_000), None);
            assert_eq!(state.active_line(), None);
        }
    }
}
