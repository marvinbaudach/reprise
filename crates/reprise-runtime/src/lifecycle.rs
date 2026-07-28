//! When the runtime process may end, as a reducer.
//!
//! §9.2 gives the runtime four states and §9.6 the single rule that decides
//! the last transition. Both are pure logic — no bus, no timer, no signal —
//! so they live here rather than in the Linux service, where a mistake would
//! only ever be observable as "the daemon exited during a sync" on someone
//! else's machine.
//!
//! `Absent` is not represented: it is the state of *no process*, and a value
//! of this type only exists inside one. The reachable states are therefore
//! `Starting`, `Serving`, `Draining` and the two terminal ones.

use std::time::Duration;

/// How long the runtime stays up after the last reason to exist went away.
///
/// Two minutes, not two seconds: closing a window and reopening it is an
/// ordinary thing to do, and paying activation latency for it is worse than
/// an idle process holding one database connection. Nothing is lost either
/// way — the grace only ever starts when there is nothing to lose.
pub const IDLE_GRACE: Duration = Duration::from_secs(120);

/// Why a runtime process gave up before it ever served anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalCause {
    /// Another process already holds the single-owner lease. A second
    /// runtime is a bug, not a case to handle: this one exits (§9.3).
    LeaseHeld,
    /// The peer that activated this process speaks a protocol major version
    /// this build cannot serve.
    ProtocolMajor,
}

/// The runtime process's own state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// Running, lease claimed, handshake not yet complete. Clients wait with
    /// a timeout; they do not poll.
    Starting,
    /// Lease held, commands accepted, snapshots published.
    Serving,
    /// Nothing left to do and nobody watching; the grace period is running.
    /// Reversible, and reversed by anything at all happening.
    Draining {
        /// Monotonic milliseconds at which the grace started.
        since_ms: u64,
    },
    /// The grace expired. The process is on its way out.
    Stopping,
    /// Never served, and never will.
    Refused(RefusalCause),
}

/// What an [`LifecycleMachine::observe`] call decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleChange {
    /// The grace period just started.
    EnteredDraining,
    /// Something happened; the grace period was abandoned.
    LeftDraining,
    /// The grace expired. The owner should shut down.
    Shutdown,
}

/// The lifecycle, advanced by explicit calls rather than by a clock it reads
/// itself — so every transition is reachable in a test without waiting.
#[derive(Debug, Clone, Copy)]
pub struct LifecycleMachine {
    state: Lifecycle,
    grace: Duration,
}

impl Default for LifecycleMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleMachine {
    #[must_use]
    pub fn new() -> Self {
        Self::with_grace(IDLE_GRACE)
    }

    /// A machine with a different grace, for tests and for a service that
    /// was told to exit promptly.
    #[must_use]
    pub fn with_grace(grace: Duration) -> Self {
        Self {
            state: Lifecycle::Starting,
            grace,
        }
    }

    #[must_use]
    pub fn state(&self) -> Lifecycle {
        self.state
    }

    /// Whether the process should still be here.
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(
            self.state,
            Lifecycle::Starting | Lifecycle::Serving | Lifecycle::Draining { .. }
        )
    }

    /// The handshake succeeded and the lease is held.
    ///
    /// Also the way out of `Draining`, because a client connecting is one of
    /// the things that must abort it — and it must not have to wait for the
    /// next observation to do so.
    pub fn serve(&mut self) {
        if matches!(self.state, Lifecycle::Starting | Lifecycle::Draining { .. }) {
            self.state = Lifecycle::Serving;
        }
    }

    /// The start is over before it began. Terminal: the process exits with a
    /// structured cause and starts nothing (§9.2).
    pub fn refuse(&mut self, cause: RefusalCause) {
        if matches!(self.state, Lifecycle::Starting) {
            self.state = Lifecycle::Refused(cause);
        }
    }

    /// Reconciles the state with the world.
    ///
    /// `idle` is the conjunction from §9.6 — no client, no playback, no
    /// device run, no job — which [`crate::Runtime::is_idle`] computes. This
    /// method deliberately takes the answer rather than the four inputs: the
    /// rule that *all* of them must hold belongs to the runtime that owns
    /// them, and duplicating it here would let the two drift apart.
    pub fn observe(&mut self, idle: bool, now_ms: u64) -> Option<LifecycleChange> {
        match self.state {
            Lifecycle::Serving if idle => {
                self.state = Lifecycle::Draining { since_ms: now_ms };
                Some(LifecycleChange::EnteredDraining)
            }
            Lifecycle::Draining { .. } if !idle => {
                self.state = Lifecycle::Serving;
                Some(LifecycleChange::LeftDraining)
            }
            Lifecycle::Draining { since_ms } => {
                let elapsed = Duration::from_millis(now_ms.saturating_sub(since_ms));
                if elapsed >= self.grace {
                    self.state = Lifecycle::Stopping;
                    Some(LifecycleChange::Shutdown)
                } else {
                    None
                }
            }
            // `Starting` deliberately does not drain: a runtime that exited
            // between activation and its first command would make the client
            // that woke it look like it failed.
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;
