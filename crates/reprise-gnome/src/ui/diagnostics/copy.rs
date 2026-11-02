//! Copying the diagnostics report to the clipboard.
//!
//! One function serves every route into it: the next-start crash offer
//! (`crash_offer`) and, later, the primary-menu entry.

use std::path::Path;

use gtk4::gdk;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;

use crate::ui::{strings, toasts};

/// Puts the redacted diagnostics report on the clipboard and says so in a
/// toast. Without a display there is no clipboard, and the toast says that
/// instead of claiming a copy that did not happen.
pub(crate) fn copy_report(overlay: &adw::ToastOverlay, db: &Db, db_path: &Path) {
    let report = super::build_report(db, db_path);
    let message = if copy_text(&report) {
        strings::DIAGNOSTICS_COPIED
    } else {
        strings::DIAGNOSTICS_COPY_FAILED
    };
    toasts::show(overlay, &strings::text(message));
}

fn copy_text(text: &str) -> bool {
    let Some(display) = gdk::Display::default() else {
        return false;
    };
    display.clipboard().set_text(text);
    true
}
