//! Shared construction points for transient notifications. Plain information
//! uses [`show`]; the common one-button shape uses [`show_with_action`].

use libadwaita as adw;

/// Auto-dismiss for plain informational toasts, in seconds — deliberately
/// shorter than libadwaita's 5 s default so status blips get out of the way.
const TOAST_TIMEOUT_S: u32 = 4;

/// Builds an FB-11 plain-text toast so markup characters cannot discard its message.
pub(super) fn plain(text: &str) -> adw::Toast {
    let toast = adw::Toast::new(text);
    toast.set_use_markup(false);
    toast
}

pub(super) fn show(overlay: &adw::ToastOverlay, text: &str) {
    let toast = plain(text);
    toast.set_timeout(TOAST_TIMEOUT_S);
    overlay.add_toast(toast);
}

pub(super) fn show_with_action(
    overlay: &adw::ToastOverlay,
    text: &str,
    button: &str,
    timeout: u32,
    on_click: impl Fn() + 'static,
) -> adw::Toast {
    let toast = plain(text);
    toast.set_button_label(Some(button));
    toast.set_timeout(timeout);
    toast.connect_button_clicked(move |_| on_click());
    overlay.add_toast(toast.clone());
    toast
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
         .toast button.text-button {{ color: @reprise_accent_text_color; font-weight: bold; }}"
    )
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gtk4::prelude::*;

    #[derive(Clone, Copy)]
    enum LabelSettle {
        UntilText,
        ObserveFor(Duration),
    }

    fn rendered_label_texts(
        toast: libadwaita::Toast,
        settle: LabelSettle,
    ) -> (Vec<String>, Duration) {
        fn collect(widget: &gtk4::Widget, labels: &mut Vec<String>) {
            if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
                let text = label.text();
                if !text.is_empty() {
                    labels.push(text.to_string());
                }
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                collect(&current, labels);
                child = current.next_sibling();
            }
        }

        let overlay = libadwaita::ToastOverlay::new();
        overlay.set_child(Some(&gtk4::Box::new(gtk4::Orientation::Vertical, 0)));
        let window = libadwaita::Window::builder()
            .default_width(480)
            .default_height(160)
            .content(&overlay)
            .build();
        window.present();
        overlay.add_toast(toast);

        let started = Instant::now();
        match settle {
            LabelSettle::UntilText => {
                crate::ui::test_settle::settle_until(
                    crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
                    || {
                        let mut labels = Vec::new();
                        collect(overlay.upcast_ref(), &mut labels);
                        !labels.is_empty()
                    },
                );
            }
            LabelSettle::ObserveFor(duration) => {
                crate::ui::test_settle::settle_for(duration);
            }
        }
        let elapsed = started.elapsed();

        let mut labels = Vec::new();
        collect(overlay.upcast_ref(), &mut labels);
        window.close();
        (labels, elapsed)
    }

    #[test]
    fn doc_8a_quiet_fixes_produce_one_undo_toast_and_review_findings_produce_none() {
        assert_eq!(super::doctor_quiet_fix_toast(0), None);
        assert_eq!(
            super::doctor_quiet_fix_toast(3).as_deref(),
            Some("3 tags fixed")
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_11_toast_plain_text_survives_markup_characters() {
        libadwaita::init().expect("libadwaita must initialize under the display runner");
        let title = "Removed Library & Radio <Episode>";

        let (plain_labels, positive_wait) =
            rendered_label_texts(super::plain(title), LabelSettle::UntilText);
        assert_eq!(plain_labels, [title]);

        let markup_toast = super::plain(title);
        markup_toast.set_use_markup(true);
        // Presence has a success condition; absence does not, so the control gets a real
        // observation window at least as long as the positive arm consumed instead of
        // "passing" when a condition wait times out.
        let absence_wait = positive_wait.max(Duration::from_millis(100));
        let (markup_labels, _) =
            rendered_label_texts(markup_toast, LabelSettle::ObserveFor(absence_wait));
        assert!(markup_labels.is_empty());
    }
}
