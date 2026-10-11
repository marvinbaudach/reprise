//! Dropping player events that belong to a stream already left behind.

use reprise_core::playback::{PlayerEvent, StreamEvent, StreamGeneration};

use super::player_event_handling::coalesce_tagged;

/// Whether `event`, produced under `generation`, belongs to a stream the
/// player has already left, given the generation `current` now.
///
/// A fault (the start watchdog's give-up included) is reported on the player's
/// thread, then waits in the channel until the drain gets to it. If the user
/// started another track in between, `apply_event` would blame the new track
/// for it and skip it. Every start bumps the stream generation, so a fault
/// tagged older than the generation now current belongs to a stream already
/// left behind.
///
/// A position tick is the evidence that playback progressed, and one left over
/// from the stream before would credit a start that is stuck; it is no
/// position of the current stream either, so it goes too.
///
/// Compared with `<`, never `!=`: the player moves the generation on its own
/// too (a gapless or crossfade hand-off), and an event from after that is newer
/// than what the controller last saw. Other events are never dropped; their own handlers
/// already tolerate a stale arrival.
fn is_left_behind(
    generation: StreamGeneration,
    event: &PlayerEvent,
    current: StreamGeneration,
) -> bool {
    matches!(event, PlayerEvent::Error(_) | PlayerEvent::Position { .. }) && generation < current
}

/// Applies a burst of tagged events in order, dropping those left behind.
///
/// The current generation is read again before every event, not once per
/// burst: applying an event can itself start a stream (a `TrackFinished`
/// advances the queue), and the events behind it that were produced under the
/// stream it ended are then stale although they were not when the burst began.
pub(super) fn deliver_burst(
    burst: Vec<StreamEvent>,
    mut current_generation: impl FnMut() -> StreamGeneration,
    mut apply: impl FnMut(PlayerEvent),
) {
    let tagged = burst
        .into_iter()
        .map(|tagged| (tagged.generation, tagged.event));
    for (generation, event) in coalesce_tagged(tagged) {
        let current = current_generation();
        if is_left_behind(generation, &event, current) {
            if matches!(event, PlayerEvent::Error(_)) {
                tracing::warn!(
                    ?generation,
                    ?current,
                    "dropping a player fault from a stream that was already left"
                );
            }
            continue;
        }
        apply(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::playback::{
        PlaybackFailure, PlaybackFailureKind, PlaybackSessionId, PlaybackState,
    };

    fn fault_at(generation: u64) -> StreamEvent {
        StreamEvent {
            generation: StreamGeneration::from(generation),
            event: PlayerEvent::Error(PlaybackFailure::new(
                "playback did not start",
                PlaybackFailureKind::Other,
                PlaybackSessionId::UNSCOPED,
            )),
        }
    }

    fn event_at(generation: u64, event: PlayerEvent) -> StreamEvent {
        StreamEvent {
            generation: StreamGeneration::from(generation),
            event,
        }
    }

    fn is_fault(event: &PlayerEvent) -> bool {
        matches!(event, PlayerEvent::Error(_))
    }

    /// What reaches the controller when the generation stays at `current`.
    fn without_stale_events(
        events: Vec<StreamEvent>,
        current: StreamGeneration,
    ) -> Vec<PlayerEvent> {
        let mut applied = Vec::new();
        deliver_burst(events, || current, |event| applied.push(event));
        applied
    }

    #[test]
    fn fb_6_a_fault_from_before_the_latest_start_is_dropped() {
        let kept = without_stale_events(vec![fault_at(4)], StreamGeneration::from(5));
        assert!(kept.is_empty());
    }

    #[test]
    fn fb_6_a_fault_of_the_current_stream_is_kept() {
        let kept = without_stale_events(vec![fault_at(5)], StreamGeneration::from(5));
        assert_eq!(kept.len(), 1);
        assert!(is_fault(&kept[0]));
    }

    #[test]
    fn fb_6_a_fault_newer_than_the_controller_has_seen_is_kept() {
        let kept = without_stale_events(vec![fault_at(7)], StreamGeneration::from(5));
        assert_eq!(kept.len(), 1, "a hand-off the player made is not stale");
    }

    fn tick_at(generation: u64, position_ms: i64) -> StreamEvent {
        event_at(
            generation,
            PlayerEvent::Position {
                position_ms,
                duration_ms: 100_000,
            },
        )
    }

    #[test]
    fn fb_6_a_position_tick_from_a_stream_already_left_is_dropped() {
        let kept = without_stale_events(
            vec![tick_at(4, 9_000), tick_at(5, 100)],
            StreamGeneration::from(5),
        );
        assert_eq!(kept.len(), 1);
        assert!(
            matches!(
                kept[0],
                PlayerEvent::Position {
                    position_ms: 100,
                    ..
                }
            ),
            "only the current stream's tick shows its playhead moving"
        );
    }

    #[test]
    fn fb_6_only_faults_and_ticks_are_dropped_and_order_is_kept() {
        let burst = vec![
            tick_at(5, 10),
            tick_at(3, 20),
            fault_at(3),
            event_at(3, PlayerEvent::StateChanged(PlaybackState::Playing)),
            fault_at(5),
            event_at(3, PlayerEvent::TrackFinished),
        ];

        let kept = without_stale_events(burst, StreamGeneration::from(5));

        assert_eq!(kept.len(), 4);
        assert!(matches!(kept[0], PlayerEvent::Position { .. }));
        assert!(matches!(kept[1], PlayerEvent::StateChanged(_)));
        assert!(is_fault(&kept[2]));
        assert!(matches!(kept[3], PlayerEvent::TrackFinished));
    }

    #[test]
    fn fb_6_an_event_behind_one_that_starts_a_stream_is_judged_by_the_new_generation() {
        let generation = std::cell::Cell::new(5);
        let mut applied = Vec::new();
        let burst = vec![
            event_at(5, PlayerEvent::TrackFinished),
            tick_at(5, 90_000),
            fault_at(5),
            event_at(5, PlayerEvent::StateChanged(PlaybackState::Playing)),
        ];

        deliver_burst(
            burst,
            || StreamGeneration::from(generation.get()),
            |event| {
                if matches!(event, PlayerEvent::TrackFinished) {
                    // The advance that TrackFinished causes starts the next track.
                    generation.set(6);
                }
                applied.push(event);
            },
        );

        assert_eq!(
            applied.len(),
            2,
            "the tick and the fault of the track that finished are stale"
        );
        assert!(matches!(applied[0], PlayerEvent::TrackFinished));
        assert!(matches!(applied[1], PlayerEvent::StateChanged(_)));
    }
}
