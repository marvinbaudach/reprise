//! Where the current Android song stood, so a cold start can put it back.
//!
//! The queue snapshot knows which song is current but not how far in it was.
//! This single-slot file holds that: the track and its playhead in
//! milliseconds. It is written whenever the song starts, pauses or is
//! seeked while paused, and read once when the session is restored. It is
//! keyed by track id, so a stale record for a song that is no longer current
//! is ignored rather than applied to the wrong one.
//!
//! The write is an atomic rename without an fsync: the process being killed
//! (the case this exists for) cannot lose a completed rename, and the
//! callers sit on Media3's looper thread, where a disk flush would stall
//! the player.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::playback::{AndroidPlaybackError, AndroidPlaybackState};

use super::{SessionInner, SessionState};

const FILE_NAME: &str = "android-resume-position.v1";
const TEMP_FILE_NAME: &str = ".android-resume-position.v1.tmp";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct ResumePoint {
    pub(super) track_id: i64,
    pub(super) position_ms: i64,
}

pub(super) struct ResumePositionFile {
    path: PathBuf,
    temporary: PathBuf,
    /// Writers arrive from Media3's looper and from the service's own thread;
    /// they share one temporary file, so they take turns.
    writing: Mutex<()>,
}

impl ResumePositionFile {
    pub(super) fn new(database_path: &Path) -> io::Result<Self> {
        let directory = database_path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "database path has no parent")
        })?;
        Ok(Self {
            path: directory.join(FILE_NAME),
            temporary: directory.join(TEMP_FILE_NAME),
            writing: Mutex::new(()),
        })
    }

    pub(super) fn read(&self) -> Option<ResumePoint> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => {
                tracing::warn!(%error, "could not read the Android resume position");
                return None;
            }
        };
        serde_json::from_slice(&bytes)
            .map_err(|error| tracing::warn!(%error, "ignored a damaged Android resume position"))
            .ok()
    }

    fn write(&self, point: ResumePoint) -> io::Result<()> {
        let _turn = self
            .writing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let bytes = serde_json::to_vec(&point).map_err(io::Error::other)?;
        let result = fs::write(&self.temporary, bytes)
            .and_then(|()| fs::rename(&self.temporary, &self.path));
        if result.is_err() {
            let _ = fs::remove_file(&self.temporary);
        }
        result
    }
}

impl SessionState {
    /// The playhead worth remembering, or `None` while playback history is
    /// presenting a song the queue cursor is not on.
    pub(super) fn resume_point(&self) -> Option<ResumePoint> {
        if self.history.presented().is_some() {
            return None;
        }
        Some(ResumePoint {
            track_id: self.queue.current()?,
            position_ms: self.snapshot.position_ms.max(0),
        })
    }
}

impl SessionInner {
    /// Remembers `point`. A failed write costs the position on the next cold
    /// start, never the playback, so it is logged and not returned.
    pub(super) fn remember_position(&self, point: Option<ResumePoint>) {
        let Some(point) = point else {
            return;
        };
        if let Err(error) = self.resume.write(point) {
            tracing::warn!(%error, "could not save the Android resume position");
        }
    }

    /// Moves the playhead of a paused song before the backend is asked to.
    ///
    /// Media3 reports no position while paused, so Core's own snapshot would
    /// otherwise keep showing the pre-seek time. Returns `true` when nothing
    /// is loaded in the backend yet (a song restored on a cold start): the
    /// position is then held for the first play and the backend must not be
    /// asked, because its empty player would drop the seek.
    pub(super) fn seek_paused(&self, position_ms: i64) -> Result<bool, AndroidPlaybackError> {
        let (unloaded, point) = {
            let mut state = self.lock()?;
            if state.snapshot.state != AndroidPlaybackState::Paused
                || state.queue.current().is_none()
            {
                return Ok(false);
            }
            let unloaded = !state.current_loaded;
            state.snapshot.position_ms = position_ms;
            if unloaded {
                state.pending_start_ms = position_ms;
            }
            (unloaded, state.resume_point())
        };
        self.remember_position(point);
        self.notify();
        Ok(unloaded)
    }
}
