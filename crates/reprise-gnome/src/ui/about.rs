//! Native application identity, credits, and licensing dialog.

use libadwaita as adw;
use libadwaita::prelude::*;

use super::strings;

const DEVELOPER: &str = "Marvin Baudach";
const COPYRIGHT: &str = "© 2026 Marvin Baudach";

/// The version shown in About: the crate version, plus the short git commit of
/// this build when one was embedded at build time (nightly dev builds set
/// `REPRISE_GIT_SHA`). Lets a tester read off exactly which dev revision runs.
fn version_string() -> String {
    match option_env!("REPRISE_GIT_SHA") {
        Some(sha) if !sha.is_empty() => format!("{} ({sha})", env!("CARGO_PKG_VERSION")),
        _ => env!("CARGO_PKG_VERSION").to_string(),
    }
}

fn build_dialog() -> adw::AboutDialog {
    let dialog = adw::AboutDialog::builder()
        .application_icon(crate::APP_ID)
        .application_name(strings::text(strings::APP_NAME))
        .version(&version_string())
        .developer_name(DEVELOPER)
        .developers(vec![DEVELOPER])
        .copyright(COPYRIGHT)
        .license_type(gtk4::License::Gpl30)
        .build();
    dialog.add_legal_section(
        &strings::text(strings::REPRISE_ENGINE_AND_LINUX_PLATFORM),
        Some(COPYRIGHT),
        gtk4::License::MitX11,
        None,
    );
    dialog
}

pub(super) fn present(parent: &adw::ApplicationWindow) {
    let dialog = build_dialog();
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(parent);
    focus_guard.restore_on_dialog_close(dialog.upcast_ref());
    focus_guard.close_on_control_w(dialog.upcast_ref());
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn dialog_exposes_application_identity_author_and_license() {
        gtk4::init().expect("GTK display must be available");
        let dialog = build_dialog();

        assert_eq!(dialog.application_icon(), crate::APP_ID);
        assert_eq!(dialog.application_name(), "Reprise");
        assert_eq!(dialog.version(), version_string());
        assert!(dialog.version().starts_with(env!("CARGO_PKG_VERSION")));
        assert_eq!(dialog.developer_name(), DEVELOPER);
        assert_eq!(dialog.developers(), [DEVELOPER]);
        assert_eq!(dialog.copyright(), COPYRIGHT);
        assert_eq!(dialog.license_type(), gtk4::License::Gpl30);
    }
}
