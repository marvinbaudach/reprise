//! The Experimental preferences page (docs/ux-rules Section AB): the master
//! "Experimental features" switch that gates all instrumental UI (INST-11), and
//! the first-use model-download flow behind it (INST-12).
//!
//! ## INST-12 — real flow vs honest placeholder
//!
//! When this build links the stem-separation backend (`stem-backend` cargo
//! feature), the Download button runs the production provisioning path on a
//! background thread — `reprise_stems::provision::ensure_weights` (SHA-256
//! checksum + atomic write + a licence notice beside the file), streaming byte
//! progress back to the row, with clear failure text (offline, checksum). When
//! the build was compiled **without** that feature, the row is an honest,
//! disabled placeholder that says so — never a functionless enabled button.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use crate::ui::instrumental;
use crate::ui::strings;

/// What the model-download row can offer, given whether this build linked the
/// stem-separation backend and whether the weights are already on disk. Pure so
/// INST-12's "real flow vs honest placeholder" split is testable without GTK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum ModelAvailability {
    /// This build has no stem-separation backend: an honest, disabled row.
    Unavailable,
    /// Backend present, weights absent: the download button is live.
    Downloadable,
    /// Backend present, weights already provisioned: nothing to download.
    Ready,
}

/// The INST-12 decision: no backend is always an honest placeholder; with the
/// backend the row is downloadable until the weights are on disk.
pub(in crate::ui) fn model_availability(
    backend_compiled: bool,
    model_present: bool,
) -> ModelAvailability {
    match (backend_compiled, model_present) {
        (false, _) => ModelAvailability::Unavailable,
        (true, false) => ModelAvailability::Downloadable,
        (true, true) => ModelAvailability::Ready,
    }
}

/// Whether the pinned htdemucs weights already sit in the model directory. A
/// cheap presence + size check (the backend re-verifies the SHA-256 on load and
/// the download itself is atomic + checksummed, so a present, full-size file is
/// trustworthy for the row's initial hint).
#[cfg(feature = "stem-backend")]
fn model_present() -> bool {
    use reprise_stems::model::HTDEMUCS_FP32;
    use reprise_stems::provision::{default_model_dir, weights_path};
    default_model_dir()
        .map(|dir| {
            std::fs::metadata(weights_path(&dir, &HTDEMUCS_FP32))
                .map(|meta| meta.len() == HTDEMUCS_FP32.size_bytes)
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

#[cfg(not(feature = "stem-backend"))]
fn model_present() -> bool {
    false
}

/// Builds the Experimental page. The switch persists the
/// `experimental_features.enabled` key; the model group's visibility follows it
/// live (mirroring the Song Visuals gate). Toggling takes effect for already-
/// running surfaces on the next app start (the worker host reads the switch at
/// launch) — an accepted experimental rough edge; the settings key itself is
/// authoritative immediately.
pub(in crate::ui) fn build_page(conn: &Rc<RefCell<Connection>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::EXPERIMENTAL_PAGE_TITLE))
        .build();

    // INST-12: the model-download row — visible only while the switch is on.
    let model_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::MODEL_GROUP_TITLE))
        .build();
    let model_row = adw::ActionRow::builder()
        .title(strings::text(strings::MODEL_DOWNLOAD_TITLE))
        .build();
    let availability = model_availability(cfg!(feature = "stem-backend"), model_present());
    build_model_row(&model_row, availability);
    model_group.add(&model_row);

    let enabled = instrumental::experimental_enabled(&conn.borrow());
    model_group.set_visible(enabled);

    // INST-11: the master switch.
    let switch_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::EXPERIMENTAL_GROUP_TITLE))
        .description(strings::text(strings::EXPERIMENTAL_GROUP_DESCRIPTION))
        .build();
    let toggle = adw::SwitchRow::builder()
        .title(strings::text(strings::EXPERIMENTAL_TOGGLE_TITLE))
        .subtitle(strings::text(strings::EXPERIMENTAL_TOGGLE_SUBTITLE))
        .active(enabled)
        .build();
    {
        let conn = conn.clone();
        let model_group = model_group.clone();
        toggle.connect_active_notify(move |row| {
            let active = row.is_active();
            if let Err(error) = instrumental::set_experimental_enabled(&conn.borrow(), active) {
                tracing::warn!(%error, "could not save the experimental-features switch");
            }
            model_group.set_visible(active);
        });
    }
    switch_group.add(&toggle);

    page.add(&switch_group);
    page.add(&model_group);
    page
}

/// Sets the model row's subtitle, button and (hidden) progress bar for the
/// INST-12 state. The download is only wired when the weights are actually
/// fetchable; the placeholder and ready states leave nothing clickable.
fn build_model_row(row: &adw::ActionRow, availability: ModelAvailability) {
    let progress = gtk4::ProgressBar::new();
    progress.set_valign(gtk4::Align::Center);
    progress.set_width_request(140);
    progress.set_visible(false);
    row.add_suffix(&progress);

    let button = gtk4::Button::with_label(&strings::text(strings::MODEL_DOWNLOAD_BUTTON));
    button.set_valign(gtk4::Align::Center);
    button.add_css_class("flat");
    row.add_suffix(&button);

    match availability {
        ModelAvailability::Unavailable => {
            row.set_subtitle(&strings::text(strings::MODEL_UNAVAILABLE_SUBTITLE));
            button.set_sensitive(false);
        }
        ModelAvailability::Ready => {
            row.set_subtitle(&strings::text(strings::MODEL_READY_SUBTITLE));
            button.set_visible(false);
        }
        ModelAvailability::Downloadable => {
            row.set_subtitle(&strings::text(strings::MODEL_DOWNLOAD_SUBTITLE));
            wire_download(row, &button, &progress);
        }
    }
}

/// Without the backend the download can never be wired — the row stays the
/// honest placeholder. A no-op that keeps `build_model_row` compiling on both
/// feature configurations (the `Downloadable` arm is unreachable here).
#[cfg(not(feature = "stem-backend"))]
fn wire_download(_row: &adw::ActionRow, _button: &gtk4::Button, _progress: &gtk4::ProgressBar) {}

/// One progress/terminal event the worker thread streams to the UI thread.
#[cfg(feature = "stem-backend")]
enum ProvisionEvent {
    Progress { read: u64, total: Option<u64> },
    Done(Result<(), String>),
}

/// Wires the Download button to the background provisioning flow (INST-12).
#[cfg(feature = "stem-backend")]
fn wire_download(row: &adw::ActionRow, button: &gtk4::Button, progress: &gtk4::ProgressBar) {
    let row = row.downgrade();
    let button_weak = button.downgrade();
    let progress_weak = progress.downgrade();
    button.connect_clicked(move |button| {
        // A single click starts one download; block re-entry until it settles.
        button.set_sensitive(false);
        if let Some(progress) = progress_weak.upgrade() {
            progress.set_visible(true);
            progress.set_fraction(0.0);
        }
        if let Some(row) = row.upgrade() {
            row.set_subtitle(&strings::model_downloading_indeterminate());
        }
        start_download(row.clone(), button_weak.clone(), progress_weak.clone());
    });
}

/// Spawns the provisioning worker and the UI-thread event drain. The worker runs
/// the production `ensure_weights` path; the drain reflects byte progress,
/// verification and the terminal Ready/failure state on the row.
#[cfg(feature = "stem-backend")]
fn start_download(
    row: gtk4::glib::WeakRef<adw::ActionRow>,
    button: gtk4::glib::WeakRef<gtk4::Button>,
    progress: gtk4::glib::WeakRef<gtk4::ProgressBar>,
) {
    let (tx, rx) = async_channel::unbounded::<ProvisionEvent>();

    // Worker thread: the production provisioning path (checksum + atomic write +
    // licence notice), streaming byte progress out. Never touches a widget.
    std::thread::spawn(move || {
        let outcome = provision_blocking_streaming(&tx);
        let _ = tx.send_blocking(ProvisionEvent::Done(outcome));
    });

    // UI thread: drain progress + terminal events onto the row.
    gtk4::glib::spawn_future_local(async move {
        while let Ok(event) = rx.recv().await {
            match event {
                ProvisionEvent::Progress { read, total } => {
                    let Some(progress) = progress.upgrade() else {
                        continue;
                    };
                    match total {
                        Some(total) if total > 0 => {
                            let fraction = (read as f64 / total as f64).clamp(0.0, 1.0);
                            progress.set_fraction(fraction);
                            if let Some(row) = row.upgrade() {
                                if read >= total {
                                    // Bytes are in; verification + write happen now.
                                    row.set_subtitle(&strings::text(strings::MODEL_FINISHING));
                                } else {
                                    let percent = (fraction * 100.0).round() as u16;
                                    row.set_subtitle(&strings::model_downloading(percent));
                                }
                            }
                        }
                        _ => {
                            progress.pulse();
                            if let Some(row) = row.upgrade() {
                                row.set_subtitle(&strings::model_downloading_indeterminate());
                            }
                        }
                    }
                }
                ProvisionEvent::Done(result) => {
                    if let Some(progress) = progress.upgrade() {
                        progress.set_visible(false);
                    }
                    match result {
                        Ok(()) => {
                            if let Some(row) = row.upgrade() {
                                row.set_subtitle(&strings::text(strings::MODEL_READY_SUBTITLE));
                            }
                            if let Some(button) = button.upgrade() {
                                button.set_visible(false);
                            }
                        }
                        Err(detail) => {
                            if let Some(row) = row.upgrade() {
                                row.set_subtitle(&strings::model_download_failed(&detail));
                            }
                            // Re-enable so the user can retry (e.g. after coming
                            // back online).
                            if let Some(button) = button.upgrade() {
                                button.set_sensitive(true);
                            }
                        }
                    }
                    break;
                }
            }
        }
    });
}

/// Runs the production provisioning path into the default model directory,
/// streaming byte progress through `tx`. Returns a user-facing error string on
/// failure (offline, checksum mismatch, no data dir).
#[cfg(feature = "stem-backend")]
fn provision_blocking_streaming(tx: &async_channel::Sender<ProvisionEvent>) -> Result<(), String> {
    let model_dir = reprise_stems::provision::default_model_dir().map_err(|e| e.to_string())?;
    let fetch = |url: &str| -> Result<Vec<u8>, String> {
        reprise_stems::provision::http_fetcher_with_progress(url, &mut |read, total| {
            let _ = tx.send_blocking(ProvisionEvent::Progress { read, total });
        })
    };
    provision_blocking(&model_dir, &fetch)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The provisioning core: `ensure_weights` for the pinned htdemucs spec into
/// `model_dir`, with the fetcher injected. Factored out so the checksum contract
/// is testable with a temp dir and a fake fetcher (no 316 MB download).
#[cfg(feature = "stem-backend")]
fn provision_blocking(
    model_dir: &std::path::Path,
    fetch: &reprise_stems::provision::Fetcher<'_>,
) -> Result<std::path::PathBuf, reprise_stems::provision::ProvisionError> {
    reprise_stems::provision::ensure_weights(model_dir, &reprise_stems::model::HTDEMUCS_FP32, fetch)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{model_availability, ModelAvailability};

    // UX INST-11: the master switch persists, defaults off, and reads back — the
    // gate every instrumental surface consults.
    #[test]
    fn inst_11_experimental_switch_persists_and_defaults_off() {
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        assert!(
            !crate::ui::instrumental::experimental_enabled(&conn),
            "experimental features are off by default"
        );
        crate::ui::instrumental::set_experimental_enabled(&conn, true).unwrap();
        assert!(
            crate::ui::instrumental::experimental_enabled(&conn),
            "the switch reads back as on after persisting"
        );
        crate::ui::instrumental::set_experimental_enabled(&conn, false).unwrap();
        assert!(!crate::ui::instrumental::experimental_enabled(&conn));
    }

    // UX INST-12: the model-download row is a real flow behind the backend and an
    // honest placeholder without it — never a functionless enabled button.
    #[test]
    fn inst_12_model_flow_is_real_behind_the_backend_and_a_placeholder_without_it() {
        // No backend compiled → honest placeholder regardless of any on-disk file.
        assert_eq!(
            model_availability(false, false),
            ModelAvailability::Unavailable
        );
        assert_eq!(
            model_availability(false, true),
            ModelAvailability::Unavailable
        );
        // Backend compiled → the real flow: downloadable when absent, ready when
        // the weights are already provisioned.
        assert_eq!(
            model_availability(true, false),
            ModelAvailability::Downloadable
        );
        assert_eq!(model_availability(true, true), ModelAvailability::Ready);
    }

    // UX INST-12: the download entrypoint verifies the pinned SHA-256 before
    // trusting the weights, and never writes a tampered file. Runs only in the
    // stem-backend build (which the --all-features gate exercises).
    #[cfg(feature = "stem-backend")]
    #[test]
    fn inst_12_download_flow_verifies_checksum_before_trusting_the_model() {
        let dir = tempfile::tempdir().unwrap();
        // A fake fetcher returns bytes that do NOT match the pinned htdemucs hash.
        let fetch = |_url: &str| Ok(b"not the real weights".to_vec());
        let err = super::provision_blocking(dir.path(), &fetch).unwrap_err();
        assert!(
            matches!(
                err,
                reprise_stems::provision::ProvisionError::ChecksumMismatch { .. }
            ),
            "a mismatching download is refused: {err:?}"
        );
        assert!(
            !reprise_stems::provision::weights_path(
                dir.path(),
                &reprise_stems::model::HTDEMUCS_FP32
            )
            .exists(),
            "a tampered download is never written to disk"
        );
    }
}
