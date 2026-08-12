//! Native application identity, credits, and licensing dialog.

use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use std::path::Path;

use super::strings;

const DEVELOPER: &str = "Marvin Baudach";
const CREDIT: &str = "Marvin Baudach <tomateq@pm.me>";
const COPYRIGHT: &str = "© 2026 Marvin Baudach";
const APPDATA_RESOURCE: &str =
    "/io/github/marvinbaudach/Reprise/io.github.marvinbaudach.Reprise.metainfo.xml";
const ISSUE_URL: &str = "https://github.com/marvinbaudach/reprise/issues";
const DEBUG_INFO_FILENAME: &str = "reprise-debug-info.txt";

/// The version shown in About: the crate version, plus the short git commit of
/// this build when one was embedded at build time (nightly dev builds set
/// `REPRISE_GIT_SHA`). Lets a tester read off exactly which dev revision runs.
fn version_string() -> String {
    version_string_from(env!("CARGO_PKG_VERSION"), option_env!("REPRISE_GIT_SHA"))
}

fn version_string_from(version: &str, git_sha: Option<&str>) -> String {
    git_sha
        .filter(|sha| !sha.is_empty())
        .map_or_else(|| version.to_string(), |sha| format!("{version} ({sha})"))
}

fn build_dialog(db: &Db, db_path: &Path) -> adw::AboutDialog {
    build_dialog_for_version(db, db_path, env!("CARGO_PKG_VERSION"))
}

fn build_dialog_for_version(
    db: &Db,
    db_path: &Path,
    release_notes_version: &str,
) -> adw::AboutDialog {
    let dialog = adw::AboutDialog::from_appdata(APPDATA_RESOURCE, Some(release_notes_version));
    dialog.set_application_icon(crate::APP_ID);
    dialog.set_application_name(&strings::text(strings::APP_NAME));
    dialog.set_version(&version_string());
    dialog.set_developer_name(DEVELOPER);
    dialog.set_developers(&[CREDIT]);
    dialog.set_designers(&[CREDIT]);
    dialog.set_artists(&[CREDIT]);
    dialog.set_documenters(&[CREDIT]);
    dialog.set_translator_credits(&strings::text(strings::TRANSLATOR_CREDITS));
    dialog.set_copyright(COPYRIGHT);
    dialog.set_license_type(gtk4::License::Gpl30);
    dialog.set_website("");
    dialog.set_issue_url(ISSUE_URL);
    dialog.set_debug_info(&super::diagnostics::build_report(db, db_path));
    dialog.set_debug_info_filename(DEBUG_INFO_FILENAME);
    add_legal_sections(&dialog);
    dialog
}

fn add_legal_sections(dialog: &adw::AboutDialog) {
    for name in ["GTK", "libadwaita", "GStreamer", "GVfs"] {
        dialog.add_legal_section(name, None, gtk4::License::Lgpl21, None);
    }
    dialog.add_legal_section("Lofty", None, gtk4::License::MitX11, None);
    dialog.add_legal_section("CAVA", None, gtk4::License::MitX11, None);
    dialog.add_legal_section("SQLite", None, gtk4::License::Custom, Some("Public Domain"));
}

pub(super) fn present(parent: &adw::ApplicationWindow, db: &Db, db_path: &Path) {
    let dialog = build_dialog(db, db_path);
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(parent);
    focus_guard.restore_on_dialog_close(dialog.upcast_ref());
    focus_guard.close_on_control_w(dialog.upcast_ref());
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> reprise_core::db::Db {
        reprise_core::db::Db::open_in_memory().unwrap()
    }

    #[test]
    fn version_uses_commit_when_present_and_plain_version_otherwise() {
        assert_eq!(
            version_string_from("0.1.1", Some("8d062859de")),
            "0.1.1 (8d062859de)"
        );
        assert_eq!(version_string_from("0.1.1", None), "0.1.1");
        assert_eq!(version_string_from("0.1.1", Some("")), "0.1.1");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn dialog_exposes_application_identity_author_and_license() {
        gtk4::init().expect("GTK display must be available");
        crate::register_app_resources();
        let db = test_db();
        let dialog = build_dialog(&db, std::path::Path::new("/missing/reprise.db"));

        assert_eq!(dialog.application_icon(), crate::APP_ID);
        assert_eq!(dialog.application_name(), "Reprise");
        assert_eq!(dialog.version(), version_string());
        assert!(dialog.version().starts_with(env!("CARGO_PKG_VERSION")));
        assert_eq!(dialog.developer_name(), DEVELOPER);
        assert_eq!(dialog.developers(), [CREDIT]);
        assert_eq!(dialog.designers(), [CREDIT]);
        assert_eq!(dialog.artists(), [CREDIT]);
        assert_eq!(dialog.documenters(), [CREDIT]);
        assert_eq!(dialog.copyright(), COPYRIGHT);
        assert_eq!(dialog.license_type(), gtk4::License::Gpl30);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn dialog_without_a_matching_release_opens_without_release_notes() {
        gtk4::init().expect("GTK display must be available");
        crate::register_app_resources();
        let db = test_db();
        let dialog =
            build_dialog_for_version(&db, std::path::Path::new("/missing/reprise.db"), "999.0.0");

        assert!(dialog.release_notes().is_empty());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn dialog_omits_the_dead_website_and_links_the_issue_tracker() {
        gtk4::init().expect("GTK display must be available");
        crate::register_app_resources();
        let db = test_db();
        let dialog = build_dialog(&db, std::path::Path::new("/missing/reprise.db"));

        assert!(dialog.website().is_empty());
        assert_eq!(dialog.issue_url(), ISSUE_URL);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn dialog_carries_a_fresh_nonempty_debug_report() {
        gtk4::init().expect("GTK display must be available");
        crate::register_app_resources();
        let db = test_db();
        let dialog = build_dialog(&db, std::path::Path::new("/missing/reprise.db"));

        assert!(!dialog.debug_info().is_empty());
        assert!(dialog
            .debug_info()
            .starts_with(&format!("reprise {} (", env!("CARGO_PKG_VERSION"))));
        assert_eq!(dialog.debug_info_filename(), "reprise-debug-info.txt");
    }
}
