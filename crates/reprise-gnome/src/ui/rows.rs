//! Shared construction points for libadwaita rows and banners. Their titles
//! and subtitles are plain text, so markup characters cannot discard them.

use gtk4::glib::GString;
use gtk4::prelude::IsA;
use libadwaita as adw;
use libadwaita::prelude::{ActionRowExt, ExpanderRowExt, PreferencesRowExt};

// A row never lets a title or subtitle reach the markup parser.
//
// libadwaita parses them as markup until `use-markup` is switched off, and a
// GObject builder does not let a caller win that race: whatever order its
// properties are given in, a subtitle (and, in some orders, a title) reaches
// the label before `use-markup` does, is parsed once, fails on an `&`, and
// logs "Failed to set text ... from markup" (measured: the published 0.1.346
// Flatpak logged it three times opening the Rhythmbox import dialog; the label
// then settled on plain text). So the builders below hold the title and
// subtitle back and give them to the finished row only after markup is off;
// `build()` is the one place that does so, and no caller can reorder it.
// Callers use the methods listed here as on the libadwaita builder; a new one
// is added to the list.
macro_rules! plain_row_builder {
    (
        $(#[$meta:meta])*
        $name:ident($builder:ty) -> $row:ty, subtitle = $set_subtitle:path {
            $( fn $method:ident($arg:ident: $arg_ty:ty); )*
        }
    ) => {
        $(#[$meta])*
        pub(in crate::ui) struct $name {
            inner: $builder,
            title: Option<GString>,
            subtitle: Option<GString>,
        }

        impl $name {
            fn new(inner: $builder) -> Self {
                Self { inner, title: None, subtitle: None }
            }

            pub(in crate::ui) fn title(mut self, title: impl Into<GString>) -> Self {
                self.title = Some(title.into());
                self
            }

            pub(in crate::ui) fn subtitle(mut self, subtitle: impl Into<GString>) -> Self {
                self.subtitle = Some(subtitle.into());
                self
            }

            $(
                pub(in crate::ui) fn $method(mut self, $arg: $arg_ty) -> Self {
                    self.inner = self.inner.$method($arg);
                    self
                }
            )*

            pub(in crate::ui) fn build(self) -> $row {
                let row = self.inner.build();
                row.set_use_markup(false);
                if let Some(title) = &self.title {
                    row.set_title(title);
                }
                if let Some(subtitle) = &self.subtitle {
                    $set_subtitle(&row, subtitle);
                }
                row
            }
        }
    };
}

/// [`plain_row_builder`] for the rows that have no subtitle.
macro_rules! plain_titled_row_builder {
    (
        $name:ident($builder:ty) -> $row:ty {
            $( fn $method:ident($arg:ident: $arg_ty:ty); )*
        }
    ) => {
        pub(in crate::ui) struct $name {
            inner: $builder,
            title: Option<GString>,
        }

        impl $name {
            fn new(inner: $builder) -> Self {
                Self { inner, title: None }
            }

            pub(in crate::ui) fn title(mut self, title: impl Into<GString>) -> Self {
                self.title = Some(title.into());
                self
            }

            $(
                pub(in crate::ui) fn $method(mut self, $arg: $arg_ty) -> Self {
                    self.inner = self.inner.$method($arg);
                    self
                }
            )*

            pub(in crate::ui) fn build(self) -> $row {
                let row = self.inner.build();
                row.set_use_markup(false);
                if let Some(title) = &self.title {
                    row.set_title(title);
                }
                row
            }
        }
    };
}

plain_row_builder! {
    ActionRowBuilder(adw::builders::ActionRowBuilder) -> adw::ActionRow,
    subtitle = ActionRowExt::set_subtitle {
        fn title_lines(title_lines: i32);
        fn subtitle_lines(subtitle_lines: i32);
        fn activatable(activatable: bool);
        fn activatable_widget(widget: &impl IsA<gtk4::Widget>);
    }
}

plain_row_builder! {
    SwitchRowBuilder(adw::builders::SwitchRowBuilder) -> adw::SwitchRow,
    subtitle = ActionRowExt::set_subtitle {
        fn active(active: bool);
    }
}

plain_row_builder! {
    ExpanderRowBuilder(adw::builders::ExpanderRowBuilder) -> adw::ExpanderRow,
    subtitle = ExpanderRowExt::set_subtitle {
        fn expanded(expanded: bool);
        fn show_enable_switch(show: bool);
        fn enable_expansion(enable: bool);
    }
}

plain_row_builder! {
    ComboRowBuilder(adw::builders::ComboRowBuilder) -> adw::ComboRow,
    subtitle = ActionRowExt::set_subtitle {
        fn model(model: &impl IsA<gtk4::gio::ListModel>);
        fn selected(selected: u32);
        fn sensitive(sensitive: bool);
    }
}

plain_titled_row_builder! {
    EntryRowBuilder(adw::builders::EntryRowBuilder) -> adw::EntryRow {
        fn text(text: impl Into<GString>);
        fn input_purpose(purpose: gtk4::InputPurpose);
    }
}

plain_titled_row_builder! {
    PasswordEntryRowBuilder(adw::builders::PasswordEntryRowBuilder) -> adw::PasswordEntryRow {
    }
}

pub(in crate::ui) fn action_row() -> ActionRowBuilder {
    ActionRowBuilder::new(adw::ActionRow::builder())
}

pub(in crate::ui) fn expander_row() -> ExpanderRowBuilder {
    ExpanderRowBuilder::new(adw::ExpanderRow::builder())
}

pub(in crate::ui) fn switch_row() -> SwitchRowBuilder {
    SwitchRowBuilder::new(adw::SwitchRow::builder())
}

pub(in crate::ui) fn combo_row() -> ComboRowBuilder {
    ComboRowBuilder::new(adw::ComboRow::builder())
}

pub(in crate::ui) fn entry_row() -> EntryRowBuilder {
    EntryRowBuilder::new(adw::EntryRow::builder())
}

pub(in crate::ui) fn password_entry_row() -> PasswordEntryRowBuilder {
    PasswordEntryRowBuilder::new(adw::PasswordEntryRow::builder())
}

/// The banner's own title is set after markup is off for the same reason the
/// builders above end in `use_markup(false)`: `Banner::new(title)` would parse
/// the title first.
pub(in crate::ui) fn banner(title: &str) -> adw::Banner {
    let banner = adw::Banner::new("");
    banner.set_use_markup(false);
    banner.set_title(title);
    banner
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gtk4::prelude::*;
    use libadwaita as adw;
    use libadwaita::prelude::*;

    use crate::ui::plain_text_display_tests::{rendered_label_texts, LabelSettle};

    fn rendered_row(row: &adw::ActionRow, settle: LabelSettle) -> (Vec<String>, Duration) {
        let group = adw::PreferencesGroup::new();
        group.add(row);
        rendered_label_texts(group.upcast_ref(), 480, 180, settle, || {})
    }

    // The control arm below expects the row's own title and subtitle to be
    // *absent* from the rendered labels — but an empty result also comes from
    // a harness that observed nothing at all (window never mapped, walk never
    // ran). A bare "absent" assertion cannot tell those apart, so a sentinel
    // label rides along in the same window: its presence proves the harness
    // was watching, keeping the negative assertions honest.
    const SENTINEL: &str = "rows harness sentinel";

    fn rendered_row_with_sentinel(
        row: &adw::ActionRow,
        settle: LabelSettle,
    ) -> (Vec<String>, Duration) {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let group = adw::PreferencesGroup::new();
        group.add(row);
        container.append(&group);
        container.append(&gtk4::Label::new(Some(SENTINEL)));
        rendered_label_texts(container.upcast_ref(), 480, 220, settle, || {})
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_13_row_plain_text_survives_markup_characters() {
        adw::init().expect("libadwaita must initialize under the display runner");
        let title = "Tom & Jerry <Live>";
        let subtitle = "AT&T > Radio";
        let banner_title = "Library & Radio <offline>";

        let plain_row = super::action_row().title(title).subtitle(subtitle).build();
        let (plain_labels, positive_wait) = rendered_row(&plain_row, LabelSettle::UntilText);
        assert!(plain_labels.iter().any(|label| label == title));
        assert!(plain_labels.iter().any(|label| label == subtitle));

        let banner = super::banner(banner_title);
        banner.set_revealed(true);
        let (banner_labels, _) =
            rendered_label_texts(banner.upcast_ref(), 480, 160, LabelSettle::UntilText, || {});
        assert!(banner_labels.iter().any(|label| label == banner_title));

        // The control arm is a row that parses markup from its first title,
        // which the builders in this module never produce, so it is built
        // directly (the idiom gate scans production code only).
        let markup_row = adw::ActionRow::builder()
            .use_markup(true)
            .title(title)
            .subtitle(subtitle)
            .build();
        let absence_wait = positive_wait.max(Duration::from_millis(100));
        let (markup_labels, _) =
            rendered_row_with_sentinel(&markup_row, LabelSettle::ObserveFor(absence_wait));
        assert!(
            markup_labels.iter().any(|label| label == SENTINEL),
            "the harness must still observe rendered labels while the markup arm swallows the title"
        );
        assert!(!markup_labels.iter().any(|label| label == title));
        assert!(!markup_labels.iter().any(|label| label == subtitle));
    }

    /// FB-13: no row or banner built here ever hands its title to the markup
    /// parser, not even for an instant. A builder applies its properties in
    /// reverse order, so a title given before `use-markup` was parsed as
    /// markup once and logged "Failed to set text ... from markup" (the
    /// published 0.1.346 Flatpak did, three times, opening the Rhythmbox
    /// import dialog) before the label settled on plain text.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_13_a_row_title_with_markup_characters_is_never_parsed_as_markup() {
        adw::init().expect("libadwaita must initialize under the display runner");
        let mark = crate::ui::gtk_log_capture::mark();
        let title = "Tom & Jerry <Live>";
        let subtitle = "AT&T > Radio";

        let _action = super::action_row().title(title).subtitle(subtitle).build();
        let _switch = super::switch_row().title(title).subtitle(subtitle).build();
        let _expander = super::expander_row()
            .title(title)
            .subtitle(subtitle)
            .build();
        let _combo = super::combo_row().title(title).subtitle(subtitle).build();
        let _entry = super::entry_row().title(title).build();
        let _password = super::password_entry_row().title(title).build();
        let _banner = super::banner(title);

        let logged = crate::ui::gtk_log_capture::messages_since(mark);
        assert!(
            logged
                .iter()
                .all(|message| !message.contains("from markup")),
            "a title reached the markup parser: {logged:?}"
        );
    }
}
