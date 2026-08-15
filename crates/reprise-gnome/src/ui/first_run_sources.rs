//! Online-source choices shown inside the first-run wizard.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::online_sources::WizardSourceSelection;

use super::strings;

pub(super) struct SourceWidgets {
    pub(super) group: adw::PreferencesGroup,
    pub(super) footer: gtk4::Label,
    radio: adw::SwitchRow,
    podcasts: adw::SwitchRow,
    youtube: adw::SwitchRow,
}

impl SourceWidgets {
    pub(super) fn selection(&self) -> WizardSourceSelection {
        WizardSourceSelection {
            radio: self.radio.is_active(),
            podcasts: self.podcasts.is_active(),
            youtube: self.youtube.is_active(),
        }
    }
}

pub(super) fn build_source_group(selection: WizardSourceSelection) -> SourceWidgets {
    let group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::PREFERENCES_ONLINE_SOURCES))
        .description(strings::text(strings::ONBOARDING_ONLINE_SOURCES_BODY))
        .build();
    let radio = source_row(
        strings::ONLINE_SOURCES_USE_RADIO,
        strings::ONLINE_SOURCES_RADIO_SUBTITLE,
        selection.radio,
    );
    let podcasts = source_row(
        strings::ONLINE_SOURCES_USE_PODCASTS,
        strings::ONLINE_SOURCES_PODCASTS_SUBTITLE,
        selection.podcasts,
    );
    let youtube = source_row(
        strings::ONLINE_SOURCES_USE_YOUTUBE,
        strings::ONLINE_SOURCES_YOUTUBE_SUBTITLE,
        selection.youtube,
    );
    group.add(&radio);
    group.add(&podcasts);
    group.add(&youtube);

    let footer = gtk4::Label::builder()
        .label(strings::text(strings::ONBOARDING_ONLINE_SOURCES_FOOTER))
        .wrap(true)
        .xalign(0.0)
        .accessible_role(gtk4::AccessibleRole::Presentation)
        .build();
    footer.add_css_class("dim-label");
    footer.add_css_class("caption");

    SourceWidgets {
        group,
        footer,
        radio,
        podcasts,
        youtube,
    }
}

fn source_row(title: &str, subtitle: &str, active: bool) -> adw::SwitchRow {
    adw::SwitchRow::builder()
        .title(strings::text(title))
        .subtitle(strings::text(subtitle))
        .use_markup(false)
        .active(active)
        .build()
}
