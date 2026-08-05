//! Shared construction points for transient notifications. Plain information
//! uses [`show`]; the common one-button shape uses [`show_with_action`].

use libadwaita as adw;

/// Auto-dismiss for plain informational toasts, in seconds — deliberately
/// shorter than libadwaita's 5 s default so status blips get out of the way.
const TOAST_TIMEOUT_S: u32 = 4;

pub(super) fn show(overlay: &adw::ToastOverlay, text: &str) {
    let toast = adw::Toast::new(text);
    toast.set_timeout(TOAST_TIMEOUT_S);
    overlay.add_toast(toast);
}

#[allow(dead_code)] // P7 wires the first Library Doctor action toast.
pub(super) fn show_with_action(
    overlay: &adw::ToastOverlay,
    text: &str,
    button: &str,
    on_click: impl Fn() + 'static,
) {
    let toast = adw::Toast::new(text);
    toast.set_button_label(Some(button));
    toast.connect_button_clicked(move |_| on_click());
    overlay.add_toast(toast);
}

#[cfg(test)]
pub(super) fn doctor_quiet_fix_toast(applied_changes: usize) -> Option<String> {
    (applied_changes > 0).then(|| crate::ui::strings::doctor_tags_fixed(applied_changes))
}

/// Redesign toast chrome: a fully-rounded dark pill with soft elevation and an
/// accent-coloured action label. Installed app-wide by [`super::style`]; the
/// action reads `@accent_color`, so it follows the theme accent.
pub(super) fn css() -> String {
    use crate::ui::style::tokens::SURFACE_SHADOW;
    format!(
        ".toast {{ border-radius: 9999px; box-shadow: {SURFACE_SHADOW}; }}\n\
         .toast button.text-button {{ color: @accent_color; font-weight: bold; }}"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn doc_8a_quiet_fixes_produce_one_undo_toast_and_review_findings_produce_none() {
        assert_eq!(super::doctor_quiet_fix_toast(0), None);
        assert_eq!(
            super::doctor_quiet_fix_toast(3).as_deref(),
            Some("3 tags fixed")
        );
    }
}
