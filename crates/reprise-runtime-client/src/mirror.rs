//! A surface's current view of the runtime, built by folding events.
//!
//! Every surface that talks to the runtime gets the same feed: one
//! [`ClientEvent::Connected`] snapshot on connect, then a stream of
//! per-facet deltas. Each of them needs something to render from between
//! events, and that something is the same shape everywhere — this is it,
//! written once instead of once per surface. It is pure: no bus, no
//! toolkit, nothing beyond folding [`ClientEvent`]s into a view.

use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::jobs::JobSnapshot;
use reprise_runtime_protocol::playback::PlaybackSnapshot;
use reprise_runtime_protocol::queue::QueueSnapshot;
use reprise_runtime_protocol::runtime::RuntimeSnapshot;

use crate::events::ClientEvent;

/// A surface's current view of the runtime.
///
/// Disconnected with nothing known until the first [`ClientEvent::Connected`]
/// ([`RuntimeMirror::new`]), and back to exactly that state on every
/// [`ClientEvent::Disconnected`]: a surface must render playback, the queue
/// and device/job state as unavailable, never a guess built from stale
/// values (RUN-2).
#[derive(Debug, Clone, Default)]
pub struct RuntimeMirror {
    connected: bool,
    playback: Option<PlaybackSnapshot>,
    queue: Option<QueueSnapshot>,
    /// Kept sorted by [`DeviceRunSnapshot::device`] so a list view has a
    /// stable order across updates instead of following arrival order,
    /// which would reshuffle rows on screen for no reason a user did.
    device_runs: Vec<DeviceRunSnapshot>,
    /// Kept sorted by [`JobSnapshot::job_id`], for the same reason.
    jobs: Vec<JobSnapshot>,
    /// The sequence of the newest thing applied.
    ///
    /// Zero is *not* a marker for "nothing yet": a runtime that has
    /// published no event hands out a snapshot at sequence zero, so a freshly
    /// connected mirror legitimately sits there. `connected` is what
    /// separates "nothing known" from "known, and nothing has happened
    /// since" — which is why [`Self::accepts`] checks it rather than
    /// treating zero as special.
    sequence: u64,
}

impl RuntimeMirror {
    /// A mirror with nothing known yet — the same shape a client is in
    /// before its first [`ClientEvent::Connected`] arrives.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Folds one event into the view.
    ///
    /// Returns whether anything a surface renders changed, so a caller can
    /// skip a redraw its own state doesn't need.
    pub fn apply(&mut self, event: &ClientEvent) -> bool {
        match event {
            ClientEvent::Connected(snapshot) => self.replace(snapshot),
            // A refusal ends the session as thoroughly as a disconnection
            // does; what differs is what the surface says about it, and that
            // is the surface's business, not the mirror's.
            ClientEvent::Disconnected | ClientEvent::Refused(_) => self.clear(),
            ClientEvent::PlaybackChanged {
                sequence, snapshot, ..
            } => self.apply_playback(*sequence, snapshot),
            ClientEvent::QueueChanged {
                sequence, snapshot, ..
            } => self.apply_queue(*sequence, snapshot),
            ClientEvent::DeviceRunChanged {
                sequence, snapshot, ..
            } => self.apply_device_run(*sequence, snapshot),
            ClientEvent::JobChanged {
                sequence, snapshot, ..
            } => self.apply_job(*sequence, snapshot),
            // A command's own result carries no runtime state — neither what
            // it did nor why it did not. The caller that sent it gets these
            // directly to react to, and whatever they changed arrives as its
            // own facet delta. There is nothing here for a mirror.
            ClientEvent::CommandFailed { .. } | ClientEvent::CommandCompleted { .. } => false,
        }
    }

    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// `None` while disconnected — a surface must render "unavailable",
    /// never a guess built from the last known state (RUN-2).
    #[must_use]
    pub fn playback(&self) -> Option<&PlaybackSnapshot> {
        self.playback.as_ref()
    }

    /// `None` while disconnected, for the same reason as [`Self::playback`].
    #[must_use]
    pub fn queue(&self) -> Option<&QueueSnapshot> {
        self.queue.as_ref()
    }

    /// Sorted by device name; empty while disconnected.
    #[must_use]
    pub fn device_runs(&self) -> &[DeviceRunSnapshot] {
        &self.device_runs
    }

    /// Sorted by job id; empty while disconnected.
    #[must_use]
    pub fn jobs(&self) -> &[JobSnapshot] {
        &self.jobs
    }

    /// The sequence of the newest thing applied.
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// A snapshot is the truth: everything runtime-bound is replaced
    /// wholesale, never merged with what the mirror already held.
    /// Reconciling a snapshot with a stale mirror is exactly how two views
    /// of one player start to disagree (RUN-3).
    fn replace(&mut self, snapshot: &RuntimeSnapshot) -> bool {
        self.connected = true;
        self.playback = Some(snapshot.playback.clone());
        self.queue = Some(snapshot.queue.clone());
        self.device_runs = sorted_device_runs(snapshot.device_runs.clone());
        self.jobs = sorted_jobs(snapshot.jobs.clone());
        self.sequence = snapshot.sequence;
        // Unconditionally true: connecting is itself the change a surface
        // renders — "unavailable" becomes "known" — even on the rare
        // reconnect whose payload happens to match what was already held.
        true
    }

    /// Returns to exactly the state [`Self::new`] starts in.
    ///
    /// A no-op, reported as such, if the mirror was already disconnected:
    /// nothing a surface renders changes by being told twice.
    fn clear(&mut self) -> bool {
        let changed = self.connected;
        *self = Self::default();
        changed
    }

    fn apply_playback(&mut self, sequence: u64, snapshot: &PlaybackSnapshot) -> bool {
        if !self.accepts(sequence) {
            return false;
        }
        self.sequence = sequence;
        if self.playback.as_ref() == Some(snapshot) {
            return false;
        }
        self.playback = Some(snapshot.clone());
        true
    }

    fn apply_queue(&mut self, sequence: u64, snapshot: &QueueSnapshot) -> bool {
        if !self.accepts(sequence) {
            return false;
        }
        self.sequence = sequence;
        if self.queue.as_ref() == Some(snapshot) {
            return false;
        }
        self.queue = Some(snapshot.clone());
        true
    }

    fn apply_device_run(&mut self, sequence: u64, snapshot: &DeviceRunSnapshot) -> bool {
        if !self.accepts(sequence) {
            return false;
        }
        self.sequence = sequence;
        upsert_by(
            &mut self.device_runs,
            snapshot.clone(),
            |entry| &entry.device,
            &snapshot.device,
        )
    }

    fn apply_job(&mut self, sequence: u64, snapshot: &JobSnapshot) -> bool {
        if !self.accepts(sequence) {
            return false;
        }
        self.sequence = sequence;
        upsert_by(
            &mut self.jobs,
            snapshot.clone(),
            |entry| &entry.job_id,
            &snapshot.job_id,
        )
    }

    /// A delta only lands on a connected mirror (there is no base to apply
    /// it to otherwise), and only if it is strictly newer than the last
    /// thing applied — out-of-order or duplicate delivery must not move the
    /// view backwards.
    fn accepts(&self, sequence: u64) -> bool {
        self.connected && sequence > self.sequence
    }
}

fn sorted_device_runs(mut runs: Vec<DeviceRunSnapshot>) -> Vec<DeviceRunSnapshot> {
    runs.sort_by(|left, right| left.device.cmp(&right.device));
    runs
}

fn sorted_jobs(mut jobs: Vec<JobSnapshot>) -> Vec<JobSnapshot> {
    jobs.sort_by_key(|job| job.job_id);
    jobs
}

/// Replaces the entry `key` picks out, or appends when there isn't one yet.
/// Returns whether the list actually changed, so an identical replay of an
/// already-applied value does not get reported as a render-worthy change.
fn upsert_by<T, K>(list: &mut Vec<T>, value: T, key: impl Fn(&T) -> &K, target: &K) -> bool
where
    T: PartialEq,
    K: Ord,
{
    match list.binary_search_by(|entry| key(entry).cmp(target)) {
        Ok(index) => {
            if list[index] == value {
                false
            } else {
                list[index] = value;
                true
            }
        }
        Err(index) => {
            list.insert(index, value);
            true
        }
    }
}

#[cfg(test)]
#[path = "mirror_tests.rs"]
mod mirror_tests;
