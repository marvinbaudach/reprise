//! The dialog chrome every "add a source" dialog builds the same way: the content box with the
//! search entry first, the footnote and the Cancel/primary footer last, inside a header bar, a
//! toolbar view and an `adw::Dialog`. Each source appends its own middle through the `body`
//! closure, so child order — which is focus order — stays exactly what the source had before.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

/// Spacing of the content box and of the footer, shared by every add dialog.
const CONTENT_SPACING: i32 = 12;
const FOOTER_SPACING: i32 = 8;

/// Everything that differs between the add dialogs' chrome. Already translated text only.
pub(in crate::ui) struct ChromeSpec {
    /// Header `adw::WindowTitle`.
    pub title: String,
    /// Also set `.title(..)` on the `adw::Dialog`, which gives it an accessible name.
    pub dialog_title: bool,
    /// `SearchEntry` placeholder.
    pub hint: String,
    /// May contain `'\n'`.
    pub footnote: String,
    pub cancel_label: String,
    pub primary_label: String,
    pub content_width: i32,
    pub content_height: i32,
    /// All four margins of the content box.
    pub margin: i32,
    pub status_wraps: bool,
}

/// The handles a source keeps after the chrome is built. The content box, the footnote and the
/// footer are only reachable through the widget tree: nothing reads them afterwards.
pub(in crate::ui) struct SourceAddChrome {
    pub dialog: adw::Dialog,
    pub entry: gtk4::SearchEntry,
    pub status: gtk4::Label,
    pub primary: gtk4::Button,
    /// Cancel closes the dialog by itself; only the display tests read the button.
    #[cfg(test)]
    pub cancel: gtk4::Button,
}

impl SourceAddChrome {
    /// Builds the chrome. `body` appends the source-specific middle: it receives the content box and
    /// the status label, which it must append itself, where the source wants it.
    pub(in crate::ui) fn build(
        spec: ChromeSpec,
        body: impl FnOnce(&gtk4::Box, &gtk4::Label),
    ) -> Self {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, CONTENT_SPACING);
        content.set_margin_top(spec.margin);
        content.set_margin_bottom(spec.margin);
        content.set_margin_start(spec.margin);
        content.set_margin_end(spec.margin);

        let entry = gtk4::SearchEntry::builder()
            .placeholder_text(spec.hint)
            .build();

        let status = gtk4::Label::new(None);
        status.add_css_class("reprise-text-secondary");
        status.set_xalign(0.0);
        if spec.status_wraps {
            status.set_wrap(true);
        }

        // SRC-7: say once why an added source stops appearing, instead of letting
        // it vanish unexplained on the next search.
        let footnote = gtk4::Label::new(Some(&spec.footnote));
        footnote.add_css_class("caption");
        footnote.add_css_class("reprise-text-secondary");
        footnote.set_xalign(0.0);
        footnote.set_wrap(true);

        let cancel = gtk4::Button::with_label(&spec.cancel_label);
        let primary = gtk4::Button::with_label(&spec.primary_label);
        primary.add_css_class("suggested-action");
        primary.set_sensitive(false);
        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, FOOTER_SPACING);
        footer.set_halign(gtk4::Align::End);
        footer.append(&cancel);
        footer.append(&primary);

        content.append(&entry);
        body(&content, &status);
        content.append(&footnote);
        content.append(&footer);

        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(&spec.title, "")));
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&content));
        let mut dialog = adw::Dialog::builder()
            .content_width(spec.content_width)
            .content_height(spec.content_height)
            .child(&toolbar);
        if spec.dialog_title {
            dialog = dialog.title(spec.title);
        }
        let dialog = dialog.build();

        let dialog_for_cancel = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(dialog) = dialog_for_cancel.upgrade() {
                dialog.close();
            }
        });

        Self {
            dialog,
            entry,
            status,
            primary,
            #[cfg(test)]
            cancel,
        }
    }

    /// `dialog.present(Some(parent))` then `entry.grab_focus()` — the two statements every add
    /// dialog ends `present` with.
    pub(in crate::ui) fn present(&self, parent: &impl IsA<gtk4::Widget>) {
        self.dialog.present(Some(parent));
        self.entry.grab_focus();
    }
}
