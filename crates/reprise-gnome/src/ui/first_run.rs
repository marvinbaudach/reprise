//! First-run decision and onboarding persistence.

use reprise_core::library::settings;
use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FirstRunDecision {
    ShowWizard,
    ExistingLibrary,
    AlreadyCompleted,
}

pub(super) fn decide(completed: bool, library_root: Option<&str>) -> FirstRunDecision {
    if completed {
        return FirstRunDecision::AlreadyCompleted;
    }
    if library_root.is_some_and(|root| !root.trim().is_empty()) {
        return FirstRunDecision::ExistingLibrary;
    }
    FirstRunDecision::ShowWizard
}

#[allow(dead_code)] // Wired into window construction in Task 7.
pub(super) fn initial_decision(conn: &Connection) -> FirstRunDecision {
    let completed = match settings::get_onboarding_completed(conn) {
        Ok(completed) => completed,
        Err(error) => {
            tracing::warn!(%error, "could not read onboarding state; showing setup");
            return FirstRunDecision::ShowWizard;
        }
    };
    let library_root = match settings::get_library_root(conn) {
        Ok(root) => root,
        Err(error) => {
            tracing::warn!(%error, "could not read library root for onboarding; showing setup");
            return FirstRunDecision::ShowWizard;
        }
    };
    let decision = decide(completed, library_root.as_deref());
    if decision == FirstRunDecision::ExistingLibrary {
        if let Err(error) = settings::set_onboarding_completed(conn, true) {
            tracing::warn!(%error, "could not mark existing-library onboarding complete");
        }
    }
    decision
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_fresh_install_shows_the_wizard() {
        assert_eq!(decide(false, None), FirstRunDecision::ShowWizard);
        assert_eq!(decide(false, Some("  ")), FirstRunDecision::ShowWizard);
    }

    #[test]
    fn existing_library_is_a_silent_upgrade() {
        assert_eq!(
            decide(false, Some("/music")),
            FirstRunDecision::ExistingLibrary
        );
    }

    #[test]
    fn completed_onboarding_never_reopens_the_wizard() {
        assert_eq!(decide(true, None), FirstRunDecision::AlreadyCompleted);
    }
}
