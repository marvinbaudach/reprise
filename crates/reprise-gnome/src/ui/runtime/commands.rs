//! The write side of [`RuntimeSession`]: one thin method per
//! [`RuntimeCommand`] variant, every one of them forwarding to
//! [`RuntimeSession::send`] (`session.rs`) and therefore to
//! [`reprise_runtime_client::RuntimeClient::send`] — never `::call`. A bus
//! round trip on the GTK main thread is a visible stall, and every command a
//! user issues already has a visible consequence (a bar update, a queue
//! reorder) to wait for instead — see `RuntimeClient::send`'s own doc
//! comment, which this module does not repeat per call site.
//!
//! No logic lives here beyond building the right [`RuntimeCommand`] variant:
//! the mapping itself has no branches to get wrong, so there is nothing
//! `session_tests.rs` would learn from testing it separately from
//! `reprise-runtime-client`'s own `RuntimeCommand::wire` coverage. See
//! `session.rs`'s doc comment for the `pub(super) fn send` seam this file
//! relies on and why `client` itself stays private to that module.
#![allow(dead_code)]

use reprise_runtime_client::RuntimeCommand;
use reprise_runtime_protocol::jobs::JobCommand;
use reprise_runtime_protocol::playback::{ExternalMedia, PlaybackCommand};
use reprise_runtime_protocol::queue::QueueCommand;

use super::RuntimeSession;

impl RuntimeSession {
    pub(crate) fn play(&self) {
        self.send(RuntimeCommand::Playback(PlaybackCommand::Play));
    }

    pub(crate) fn pause(&self) {
        self.send(RuntimeCommand::Playback(PlaybackCommand::Pause));
    }

    pub(crate) fn stop(&self) {
        self.send(RuntimeCommand::Playback(PlaybackCommand::Stop));
    }

    pub(crate) fn next(&self) {
        self.send(RuntimeCommand::Playback(PlaybackCommand::Next));
    }

    pub(crate) fn previous(&self) {
        self.send(RuntimeCommand::Playback(PlaybackCommand::Previous));
    }

    /// Absolute volume in the inclusive `0.0..=1.0` range; the runtime
    /// clamps and reports the value it actually applied in the next
    /// snapshot.
    pub(crate) fn set_volume(&self, volume: f64) {
        self.send(RuntimeCommand::Playback(PlaybackCommand::SetVolume(volume)));
    }

    /// Relative seek in milliseconds; negative seeks backward — matches
    /// [`PlaybackCommand::Seek`]'s own contract.
    pub(crate) fn seek(&self, delta_ms: i64) {
        self.send(RuntimeCommand::Playback(PlaybackCommand::Seek(delta_ms)));
    }

    pub(crate) fn set_shuffle(&self, on: bool) {
        self.send(RuntimeCommand::Playback(PlaybackCommand::SetShuffle(on)));
    }

    /// `mode` is `off`, `all`, or `one` — the runtime's own vocabulary, not
    /// re-typed as an enum here: this frontend has no repeat type of its own
    /// yet to convert from (this is the first brick — see `mod.rs`'s doc
    /// comment), and inventing one purely to convert it back would be a
    /// third vocabulary for no reader.
    pub(crate) fn set_repeat(&self, mode: impl Into<String>) {
        self.send(RuntimeCommand::Playback(PlaybackCommand::SetRepeat(
            mode.into(),
        )));
    }

    /// Seeds the queue from `track_ids` and starts playback at
    /// `start_index`.
    pub(crate) fn play_tracks(&self, track_ids: Vec<i64>, start_index: usize) {
        self.send(RuntimeCommand::PlayTracks {
            track_ids,
            start_index,
        });
    }

    /// Plays something that is not a library track: a radio stream, a
    /// podcast episode, a preview render — see [`ExternalMedia`]'s own doc
    /// comment for why `location` travels inward only.
    pub(crate) fn play_external(&self, media: ExternalMedia) {
        self.send(RuntimeCommand::PlayExternal(media));
    }

    /// Inserts `track_ids` directly after the current item, in order.
    pub(crate) fn queue_add_next(&self, track_ids: Vec<i64>) {
        self.send(RuntimeCommand::Queue(QueueCommand::AddNext(track_ids)));
    }

    /// Appends `track_ids` to the end of the explicit queue, in order.
    pub(crate) fn queue_add_last(&self, track_ids: Vec<i64>) {
        self.send(RuntimeCommand::Queue(QueueCommand::AddLast(track_ids)));
    }

    /// Drops the explicit queue. The current item keeps playing — clearing a
    /// queue is not a stop command.
    pub(crate) fn queue_clear(&self) {
        self.send(RuntimeCommand::Queue(QueueCommand::Clear));
    }

    /// Moves one explicit-queue entry, by position.
    pub(crate) fn queue_move(&self, from: u64, to: u64) {
        self.send(RuntimeCommand::Queue(QueueCommand::Move { from, to }));
    }

    /// Drops explicit-queue entries by position.
    pub(crate) fn queue_remove_at(&self, positions: Vec<u64>) {
        self.send(RuntimeCommand::Queue(QueueCommand::RemoveAt(positions)));
    }

    /// Drops entries from the surrounding context, by play-order position.
    pub(crate) fn queue_remove_context_at(&self, positions: Vec<u64>) {
        self.send(RuntimeCommand::Queue(QueueCommand::RemoveContextAt(
            positions,
        )));
    }

    /// Plays the explicit-queue entry at `position` now, taking it out of
    /// the queue.
    pub(crate) fn queue_play_next_at(&self, position: u64) {
        self.send(RuntimeCommand::Queue(QueueCommand::PlayNextAt(position)));
    }

    /// Lets the context entry at `position` jump the line and play now —
    /// see [`QueueCommand::PlayContextAt`]'s own doc comment for why
    /// everything it passed stays queued rather than being dropped.
    pub(crate) fn queue_play_context_at(&self, position: u64) {
        self.send(RuntimeCommand::Queue(QueueCommand::PlayContextAt(position)));
    }

    /// Forgets `track_ids` wherever they appear in the queue — a library
    /// deletion reaching the queue, not a user editing it.
    pub(crate) fn queue_purge(&self, track_ids: Vec<i64>) {
        self.send(RuntimeCommand::Queue(QueueCommand::Purge(track_ids)));
    }

    /// Asks the runtime to stop job `job_id`. A request, not an assertion —
    /// see [`JobCommand::Cancel`]'s own doc comment.
    pub(crate) fn job_cancel(&self, job_id: i64) {
        self.send(RuntimeCommand::Job(JobCommand::Cancel(job_id)));
    }

    /// Promotes staged render `job_id` to a permanent library track.
    pub(crate) fn job_save(&self, job_id: i64) {
        self.send(RuntimeCommand::Job(JobCommand::Save(job_id)));
    }

    /// Drops staged render `job_id` without saving it.
    pub(crate) fn job_discard(&self, job_id: i64) {
        self.send(RuntimeCommand::Job(JobCommand::Discard(job_id)));
    }

    pub(crate) fn device_start(&self, device: impl Into<String>) {
        self.send(RuntimeCommand::DeviceStart {
            device: device.into(),
        });
    }

    pub(crate) fn device_cancel(&self, device: impl Into<String>) {
        self.send(RuntimeCommand::DeviceCancel {
            device: device.into(),
        });
    }
}
