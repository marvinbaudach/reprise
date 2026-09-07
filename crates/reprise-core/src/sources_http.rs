//! Shared mechanics for the podcasts, radio, and concerts HTTP boundaries.
//!
//! This module owns their identical user agent, unpoisoned mutex access, agent
//! construction, and fixture-directory scope. The fixture scope accepts a hook
//! so source-specific reset policy stays at its call site while restoration is
//! shared; radio uses it to clear its server cache on entry and exit.
//! A closure was chosen over a `FixtureScope` struct because only the reset
//! action varies; keep that policy source-owned unless it gains reusable state.
//!
//! Rate limiting, HTTP status mapping, error classification, and fixture route
//! matching deliberately do not belong here because their behavior and domain
//! types differ by source; in particular, sharing the limiter would break
//! concerts' cancellable 50 ms polling while a request waits for its slot.

#[cfg(test)]
use std::path::Path;
#[cfg(any(test, feature = "test-fixtures"))]
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

#[cfg(test)]
thread_local! {
    static TEST_FIXTURE_DIR: std::cell::RefCell<Option<(&'static str, PathBuf)>> = const {
        std::cell::RefCell::new(None)
    };
}

#[must_use]
pub(crate) fn user_agent() -> String {
    format!(
        "Reprise/{} ( {} )",
        env!("CARGO_PKG_VERSION"),
        crate::musicbrainz::CONTACT_URL
    )
}

pub(crate) fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn build_agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(user_agent())
        .http_status_as_error(false)
        .build()
        .new_agent()
}

#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn fixture_directory(environment_variable: &str) -> Option<PathBuf> {
    #[cfg(test)]
    if let Some((stored_name, directory)) = TEST_FIXTURE_DIR.with(|slot| slot.borrow().clone()) {
        if stored_name == environment_variable {
            return Some(directory);
        }
    }
    std::env::var(environment_variable).ok().map(PathBuf::from)
}

#[cfg(test)]
pub(crate) fn with_fixture_dir<T>(
    environment_variable: &'static str,
    directory: &Path,
    mut reset_source_state: impl FnMut(),
    operation: impl FnOnce() -> T,
) -> T {
    struct Reset<F: FnMut()> {
        previous: Option<(&'static str, PathBuf)>,
        reset_source_state: F,
    }

    impl<F: FnMut()> Drop for Reset<F> {
        fn drop(&mut self) {
            TEST_FIXTURE_DIR.with(|slot| *slot.borrow_mut() = self.previous.take());
            (self.reset_source_state)();
        }
    }

    reset_source_state();
    let previous = TEST_FIXTURE_DIR.with(|slot| {
        slot.borrow_mut()
            .replace((environment_variable, directory.to_path_buf()))
    });
    let _reset = Reset {
        previous,
        reset_source_state,
    };
    operation()
}
