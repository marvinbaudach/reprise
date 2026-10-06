//! The fixture-directory seam shared by the podcasts, radio and concerts boundaries.
//!
//! The fixture scope accepts a hook so source-specific reset policy stays at its call site while
//! restoration is shared; radio uses it to clear its server cache on entry and exit.
//! A closure was chosen over a `FixtureScope` struct because only the reset
//! action varies; keep that policy source-owned unless it gains reusable state.

#[cfg(test)]
use std::path::Path;
#[cfg(any(test, feature = "test-fixtures"))]
use std::path::PathBuf;

#[cfg(test)]
thread_local! {
    static TEST_FIXTURE_DIR: std::cell::RefCell<Option<(&'static str, PathBuf)>> = const {
        std::cell::RefCell::new(None)
    };
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
