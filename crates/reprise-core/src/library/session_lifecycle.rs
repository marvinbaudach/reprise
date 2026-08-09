//! Crash-conservative process lifecycle evidence for the persisted session.

use serde::{Deserialize, Serialize};

use crate::db::Db;

use super::session::{self, SessionState};

/// Evidence that the preceding process reached its normal close handler.
///
/// This is consumed at the next startup before any catch-up decision. A
/// missing marker therefore always means the previous process may have
/// crashed or been killed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanExit {
    pub completed_at: i64,
    pub library_root: String,
}

/// Loads the previous session and consumes its clean-exit evidence.
///
/// Returning the evidence only after the marker was durably cleared makes a
/// later kill conservative: the next process sees no clean exit and scans.
pub fn load_and_mark_running(db: &Db) -> SessionState {
    let mut running = session::load(db);
    let previous_clean_exit = running.clean_exit.take();
    if previous_clean_exit.is_none() {
        return running;
    }
    if let Err(error) = session::save(db, &running) {
        tracing::warn!(%error, "could not consume clean-exit marker; startup scan remains due");
        return running;
    }
    running.clean_exit = previous_clean_exit;
    running
}

pub fn mark_clean_exit(state: &mut SessionState, library_root: String, completed_at: i64) {
    state.clean_exit = Some(CleanExit {
        completed_at,
        library_root,
    });
}

pub fn mark_clean_exit_now(state: &mut SessionState, library_root: String) {
    let completed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64);
    mark_clean_exit(state, library_root, completed_at);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_consumes_the_clean_exit_marker_before_the_process_can_be_killed() {
        let db = Db::open_in_memory().unwrap();
        let mut state = SessionState::default();
        mark_clean_exit(&mut state, "/music".into(), 1_234);
        session::save(&db, &state).unwrap();

        let prior = load_and_mark_running(&db);

        assert_eq!(prior.clean_exit, state.clean_exit);
        assert_eq!(session::load(&db).clean_exit, None);
        assert_eq!(load_and_mark_running(&db).clean_exit, None);
    }
}
