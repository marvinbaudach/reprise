//! What production code does when a state that cannot happen in a correct
//! program happens anyway.
//!
//! A panic inside a GTK signal handler unwinds into C and aborts the whole
//! application, so the frontend denies `unwrap`, `expect`, `panic!` and their
//! relatives from the crate root. A genuinely impossible state still needs an
//! answer, and `invariant_broken!` is it: log the broken invariant, trip a
//! debug assertion, and let the caller take its neutral path.
//!
//! - Release builds (the Flatpak) never crash. They log at `error` level and
//!   the caller degrades, usually with `let … else { invariant_broken!(…); return … }`.
//! - Debug builds and tests still trip on a broken invariant, so a regression
//!   is found where it is introduced instead of being absorbed by the fallback.
//!
//! Use plain `tracing` for states that are not invariant breaks, such as the
//! X11-only path running on Wayland.

/// Logs a broken invariant, asserts in debug builds, and returns so the caller
/// can degrade. Takes `format!`-style arguments naming the invariant.
macro_rules! invariant_broken {
    ($($message:tt)+) => {{
        let message = ::std::format!($($message)+);
        ::tracing::error!(
            "invariant broken: {message} ({}:{})",
            ::std::file!(),
            ::std::line!(),
        );
        ::std::debug_assert!(false, "invariant broken: {message}");
    }};
}

#[cfg(test)]
mod tests {
    use crate::ui::test_log_capture::CapturedLogs;

    #[test]
    #[should_panic(expected = "invariant broken: row 7 has no model")]
    fn a_broken_invariant_trips_the_debug_assertion() {
        invariant_broken!("row {} has no model", 7);
    }

    #[test]
    fn a_broken_invariant_reaches_the_log_before_it_asserts() {
        let logs = CapturedLogs::default();

        let outcome = logs
            .capture(|| std::panic::catch_unwind(|| invariant_broken!("row {} has no model", 7)));

        let logged = logs.text();
        assert!(
            logged.contains("ERROR") && logged.contains("invariant broken: row 7 has no model"),
            "the broken invariant must be logged at error level, got: {logged:?}"
        );
        // The debug assertion fires too; release-mode test runs skip it.
        assert_eq!(outcome.is_err(), cfg!(debug_assertions));
    }
}
