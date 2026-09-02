//! Translates backend `StreamEvent`s into session-state transitions.
//!
//! The backend reports playback as a flat stream of events; this is the one
//! place that turns them into the follow-up actions the session must take
//! (start the next track, feed the backend a new "next" URI, or stop), while
//! keeping the play-recording and queue-persistence side effects outside the
//! state lock they read out of.

use reprise_core::playback::{
    playback_fault_policy, should_stop_skipping, PlaybackBackend, PlaybackFaultNotice,
    PlaybackState, PlayerEvent, StreamEvent,
};

use crate::listen_export_recorder::RecordedListen;
use crate::play_recorder::RecordedPlay;
use crate::playback::AndroidPlaybackState;

use super::SessionInner;

const TRACK_UNAVAILABLE_SKIPPED: &str = "Track unavailable — skipped";
const TOO_MANY_UNPLAYABLE_TRACKS: &str = "Playback stopped — too many unplayable tracks";

fn fault_notice_text(notice: PlaybackFaultNotice, title: Option<&str>) -> String {
    match notice {
        PlaybackFaultNotice::TrackUnavailableSkipped => TRACK_UNAVAILABLE_SKIPPED.to_owned(),
        PlaybackFaultNotice::CouldNotPlaySkipped => {
            format!("Could not play {} — skipping", title.unwrap_or("track"))
        }
    }
}

impl SessionInner {
    pub(super) fn handle_event(&self, event: StreamEvent, missing: Option<bool>) {
        enum FollowUp {
            None,
            Start,
            Feed(Option<String>),
            Stop,
        }

        let fault_title = if matches!(&event.event, PlayerEvent::Error(_)) && missing == Some(false)
        {
            let track_id = self
                .state
                .lock()
                .ok()
                .and_then(|state| state.queue.current());
            track_id.and_then(|track_id| match self.library.track_by_id(track_id) {
                Ok(track) => track.map(|track| (track_id, track.title)),
                Err(error) => {
                    tracing::warn!(%error, "could not read faulted Android track title");
                    None
                }
            })
        } else {
            None
        };

        let (follow_up, play_to_record, queue_to_save) = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if !state.accepts(event.generation) {
                return;
            }
            match event.event {
                PlayerEvent::StateChanged(playback) => {
                    if playback == PlaybackState::Playing {
                        state.consecutive_faults = 0;
                        state.fault_skip_limit = None;
                        state.snapshot.error = None;
                    }
                    state.snapshot.state = playback.into();
                    (FollowUp::None, None, None)
                }
                PlayerEvent::Position {
                    position_ms,
                    duration_ms,
                } => {
                    state.snapshot.position_ms = position_ms.max(0);
                    if duration_ms > 0 {
                        state.snapshot.duration_ms = duration_ms;
                    }
                    state.max_position_ms = state.max_position_ms.max(position_ms.max(0));
                    let play = state.play_to_record(false);
                    (FollowUp::None, play, None)
                }
                PlayerEvent::TrackFinished => {
                    let play = state.play_to_record(true);
                    state.snapshot.automatic_advance_count =
                        state.snapshot.automatic_advance_count.saturating_add(1);
                    if state.queue.advance_auto().is_some() {
                        state.adopt_current();
                        (FollowUp::Start, play, Some(state.queue.clone()))
                    } else {
                        state.stop();
                        (FollowUp::Stop, play, Some(state.queue.clone()))
                    }
                }
                PlayerEvent::AdvancedToNext => {
                    let play = state.play_to_record(true);
                    state.snapshot.automatic_advance_count =
                        state.snapshot.automatic_advance_count.saturating_add(1);
                    if state.queue.advance_auto().is_some() {
                        state.adopt_current();
                        state.current_loaded = true;
                        let history_entry = state
                            .current_track_id()
                            .zip(state.current_uri())
                            .map(|(track_id, uri)| state.history_entry_for_started(track_id, uri));
                        if let Some(history_entry) = history_entry {
                            state.note_playback_started(history_entry);
                        }
                        (
                            FollowUp::Feed(state.next_uri()),
                            play,
                            Some(state.queue.clone()),
                        )
                    } else {
                        state.stop();
                        (FollowUp::Stop, play, Some(state.queue.clone()))
                    }
                }
                PlayerEvent::Error(message) => {
                    tracing::warn!(%message, "Android playback backend reported an error");
                    let policy = playback_fault_policy(!missing.unwrap_or(false));
                    let title = fault_title
                        .as_ref()
                        .filter(|(track_id, _)| Some(*track_id) == state.queue.current())
                        .map(|(_, title)| title.as_str());
                    state.current_loaded = false;
                    let queue_len = state.queue.len();
                    let skip_limit = *state.fault_skip_limit.get_or_insert(queue_len);
                    state.consecutive_faults = state.consecutive_faults.saturating_add(1);
                    let repeated_fault_bound_reached = state.consecutive_faults > 1
                        && should_stop_skipping(state.consecutive_faults, skip_limit);
                    if repeated_fault_bound_reached {
                        state.snapshot.fault_notice = None;
                        state.snapshot.error = Some(TOO_MANY_UNPLAYABLE_TRACKS.to_owned());
                        state.stop();
                        (FollowUp::Stop, None, None)
                    } else if policy.skip {
                        state.snapshot.fault_notice =
                            Some(fault_notice_text(policy.notices[0], title));
                        state.snapshot.fault_notice_count =
                            state.snapshot.fault_notice_count.saturating_add(1);
                        let faulted_track_id = state.queue.current();
                        let next_track = state
                            .queue
                            .next_manual()
                            .filter(|track_id| Some(*track_id) != faulted_track_id);
                        if next_track.is_some() {
                            state.adopt_current();
                            (FollowUp::Start, None, Some(state.queue.clone()))
                        } else {
                            state.stop();
                            (FollowUp::Stop, None, Some(state.queue.clone()))
                        }
                    } else {
                        state.stop();
                        (FollowUp::Stop, None, Some(state.queue.clone()))
                    }
                }
                PlayerEvent::Buffering { .. } => {
                    // Buffering describes only a stream that is still loaded.
                    // Queue exhaustion and errors both clear `current_loaded`,
                    // so a callback already in flight cannot revive their
                    // terminal Stopped snapshot.
                    if state.current_loaded {
                        state.snapshot.state = AndroidPlaybackState::Buffering;
                    }
                    (FollowUp::None, None, None)
                }
                PlayerEvent::StreamTags { .. } | PlayerEvent::Spectrum(_) => {
                    (FollowUp::None, None, None)
                }
            }
        };

        if let Some(queue) = queue_to_save {
            if let Err(error) = self.persist_queue(&queue) {
                tracing::warn!(%error, "could not persist automatic Android queue advance");
            }
        }

        // Queued, not written: this runs on Media3's application thread, and
        // `FollowUp::Start` below is the gapless transition into the next
        // track. See `play_recorder`.
        if let Some((track_id, ms_played)) = play_to_record {
            let play = RecordedPlay::now(track_id);
            self.plays.record(play);
            self.listen_exports.record(RecordedListen {
                track_id,
                at_unix: play.at_unix,
                ms_played,
            });
        }

        match follow_up {
            FollowUp::None => self.notify(),
            FollowUp::Start => {
                if let Err(error) = self.start_current() {
                    tracing::warn!(%error, "could not start the next Android queue item");
                    // `start_current` can return before its own notification if
                    // the queue changed between this event and the follow-up.
                    self.notify();
                }
            }
            FollowUp::Feed(next_uri) => {
                if let Ok(backend) = self.backend() {
                    backend.set_next(next_uri.as_deref());
                }
                self.notify();
            }
            FollowUp::Stop => {
                if let Ok(backend) = self.backend() {
                    let _ = backend.stop();
                }
                self.notify();
            }
        }
    }
}
