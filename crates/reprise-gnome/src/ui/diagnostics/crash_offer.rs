//! START-6: the running sentinel and the one next-start offer.
//!
//! The sentinel is armed once the process knows it is the first instance and
//! disarmed on every clean way out (`SessionSaver::save_once`, which both the
//! window close and a termination request reach, and the end of `run`). A
//! sentinel still present at the next start means the previous run was killed
//! or crashed, and the main window then offers "Copy Diagnostics" in exactly
//! one toast. The sentinel is not consumed by the offer: from the moment it is
//! armed it belongs to the run that is starting.

use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::crash_report::{self, PreviousExit};
use reprise_core::db::Db;

use crate::ui::{strings, toasts};

/// Seconds the offer stays up. An offer with an action outlasts an ordinary
/// status toast, as FB-1 asks.
const OFFER_TIMEOUT_S: u32 = 10;

/// Whether the offer is still to be made. One per process.
struct PendingOffer(AtomicBool);

impl PendingOffer {
    const fn none() -> Self {
        Self(AtomicBool::new(false))
    }

    fn after(&self, previous: PreviousExit) {
        self.0
            .store(previous == PreviousExit::Unclean, Ordering::SeqCst);
    }

    /// True exactly once after an unclean previous run.
    fn take(&self) -> bool {
        self.0.swap(false, Ordering::SeqCst)
    }
}

static PENDING: PendingOffer = PendingOffer::none();

/// Every clean exit disarms the one sentinel of the test process, so a test
/// that watches it must not overlap another test that saves a session.
#[cfg(test)]
pub(crate) fn marker_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Writes the sentinel for this run and remembers whether the previous one
/// left its own behind.
pub(crate) fn arm() {
    PENDING.after(crash_report::arm());
}

/// This run ended the way it meant to.
pub(crate) fn disarm() {
    crash_report::disarm();
}

/// Offers "Copy Diagnostics" if the previous run did not end cleanly, and
/// never again in this process.
pub(crate) fn offer(overlay: &adw::ToastOverlay, db: &Rc<Db>, db_path: &Path) {
    offer_once(&PENDING, overlay, db, db_path);
}

fn offer_once(
    pending: &PendingOffer,
    overlay: &adw::ToastOverlay,
    db: &Rc<Db>,
    db_path: &Path,
) -> Option<adw::Toast> {
    if !pending.take() {
        return None;
    }
    let (weak_overlay, db, db_path) = (overlay.downgrade(), db.clone(), db_path.to_path_buf());
    Some(toasts::show_with_action(
        overlay,
        &strings::text(strings::PREVIOUS_RUN_ENDED_UNEXPECTEDLY),
        &strings::text(strings::COPY_DIAGNOSTICS),
        OFFER_TIMEOUT_S,
        move || {
            if let Some(overlay) = weak_overlay.upgrade() {
                super::copy::copy_report(&overlay, &db, &db_path);
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_6_an_unclean_previous_run_is_offered_exactly_once() {
        let pending = PendingOffer::none();

        pending.after(PreviousExit::Unclean);

        assert!(pending.take());
        assert!(!pending.take());
    }

    #[test]
    fn start_6_a_clean_previous_run_offers_nothing() {
        let pending = PendingOffer::none();

        pending.after(PreviousExit::Clean);

        assert!(!pending.take());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn start_6_the_offer_is_one_toast_with_copy_diagnostics_and_not_repeated() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let overlay = adw::ToastOverlay::new();
        let db = Rc::new(crate::test_db::open().unwrap());
        let pending = PendingOffer::none();
        pending.after(PreviousExit::Unclean);
        let path = Path::new("reprise.db");

        let toast = offer_once(&pending, &overlay, &db, path).expect("the first start offers");

        assert_eq!(toast.button_label().as_deref(), Some("Copy Diagnostics"));
        assert!(
            offer_once(&pending, &overlay, &db, path).is_none(),
            "a second window must not repeat the offer"
        );
    }
}
