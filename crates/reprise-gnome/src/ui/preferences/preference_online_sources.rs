//! The "Online sources" preferences page (`SET-8`, owner design Turn 7b).
//!
//! One page, one global master ("Use online sources"), and three
//! equal-rank blocks — YouTube, Podcasts, Radio — each with its own master
//! switch and rows. Turning a block off hides its sidebar entry and stops
//! its requests; turning the global master off does that for all three at
//! once. Neither ever deletes subscriptions or favorites.
//!
//! The global master persists `online-sources-enabled` (`NET-1a`), the one
//! authority ANDed into every network entry point in core
//! (`reprise_core::online_sources::network_allowed`). This page's job is
//! presentation and persistence; the actual gate lives in core, next to the
//! module registry.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::modules;

use super::{strings, PreferencesContext};

pub(in crate::ui) fn build(context: &Rc<PreferencesContext>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::PREFERENCES_ONLINE_SOURCES))
        .icon_name("network-server-symbolic")
        .build();

    let global_group = adw::PreferencesGroup::new();
    let global_enabled =
        reprise_core::online_sources::is_enabled(&context.conn.borrow()).unwrap_or(true);
    let global_master = adw::SwitchRow::builder()
        .title(strings::text(strings::ONLINE_SOURCES_MASTER_TITLE))
        .subtitle(strings::text(strings::ONLINE_SOURCES_MASTER_BODY))
        .active(global_enabled)
        .build();
    global_group.add(&global_master);
    page.add(&global_group);

    let youtube_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::YOUTUBE))
        .description(strings::text(strings::ONLINE_SOURCES_YOUTUBE_SUBTITLE))
        .build();
    let youtube_enabled =
        modules::is_enabled(&context.conn.borrow(), &modules::YOUTUBE_MODULE).unwrap_or(false);
    let youtube_master = adw::SwitchRow::builder()
        .title(strings::text(strings::ONLINE_SOURCES_USE_YOUTUBE))
        .active(youtube_enabled)
        .build();
    youtube_group.add(&youtube_master);
    let youtube_rows = super::preference_youtube::build(&context.conn, youtube_enabled);
    youtube_rows.add_to(&youtube_group);
    youtube_group.set_sensitive(global_enabled);
    page.add(&youtube_group);
    {
        let context = context.clone();
        let rows = youtube_rows.clone();
        youtube_master.connect_active_notify(move |row| {
            let active = row.is_active();
            if let Err(error) = context
                .podcasts
                .set_youtube_enabled(&context.conn.borrow(), active)
            {
                tracing::warn!(%error, "could not save YouTube preference");
                return;
            }
            rows.set_sensitive(active);
            context.sidebar.refresh("YouTube module toggled");
        });
    }

    let podcasts_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::PODCASTS))
        .description(strings::text(strings::ONLINE_SOURCES_PODCASTS_SUBTITLE))
        .build();
    let podcasts_enabled =
        modules::is_enabled(&context.conn.borrow(), &modules::PODCASTS_MODULE).unwrap_or(false);
    let podcasts_master = adw::SwitchRow::builder()
        .title(strings::text(strings::ONLINE_SOURCES_USE_PODCASTS))
        .active(podcasts_enabled)
        .build();
    podcasts_group.add(&podcasts_master);
    let podcasts_rows = super::preference_podcasts::build(&context.conn, podcasts_enabled);
    podcasts_rows.add_to(&podcasts_group);
    podcasts_group.set_sensitive(global_enabled);
    page.add(&podcasts_group);
    {
        let context = context.clone();
        let rows = podcasts_rows.clone();
        podcasts_master.connect_active_notify(move |row| {
            let active = row.is_active();
            if let Err(error) = context
                .podcasts
                .set_podcasts_enabled(&context.conn.borrow(), active)
            {
                tracing::warn!(%error, "could not save Podcasts preference");
                return;
            }
            rows.set_sensitive(active);
            context.sidebar.refresh("Podcasts module toggled");
        });
    }

    let radio_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::RADIO))
        .description(strings::text(strings::ONLINE_SOURCES_RADIO_SUBTITLE))
        .build();
    let radio_enabled =
        modules::is_enabled(&context.conn.borrow(), &modules::RADIO_MODULE).unwrap_or(false);
    let radio_master = adw::SwitchRow::builder()
        .title(strings::text(strings::ONLINE_SOURCES_USE_RADIO))
        .active(radio_enabled)
        .build();
    radio_group.add(&radio_master);
    let radio_rows = super::preference_radio::build(&context.conn, radio_enabled);
    radio_rows.add_to(&radio_group);
    radio_group.set_sensitive(global_enabled);
    page.add(&radio_group);
    {
        let context = context.clone();
        let rows = radio_rows.clone();
        radio_master.connect_active_notify(move |row| {
            let active = row.is_active();
            if let Err(error) =
                modules::set_enabled(&context.conn.borrow(), &modules::RADIO_MODULE, active)
            {
                tracing::warn!(%error, "could not save Radio preference");
                return;
            }
            rows.set_sensitive(active);
            context.sidebar.refresh("Radio module toggled");
        });
    }

    let footer_group = adw::PreferencesGroup::builder()
        .description(strings::text(strings::ONLINE_SOURCES_FOOTER))
        .build();
    page.add(&footer_group);

    {
        let context = context.clone();
        let groups = [youtube_group, podcasts_group, radio_group];
        global_master.connect_active_notify(move |row| {
            let active = row.is_active();
            if let Err(error) = context.set_online_sources_enabled(active) {
                tracing::warn!(%error, "could not save the online-sources gate");
                return;
            }
            for group in &groups {
                group.set_sensitive(active);
            }
        });
    }

    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_1a_global_master_copy_matches_the_design() {
        assert_eq!(
            strings::text(strings::ONLINE_SOURCES_MASTER_TITLE),
            "Use online sources"
        );
        assert_eq!(
            strings::text(strings::ONLINE_SOURCES_MASTER_BODY),
            "Off makes this a local player only: no requests, no downloads, nothing hidden — the three entries disappear from the sidebar."
        );
    }

    #[test]
    fn set_9_footer_copy_matches_the_design() {
        assert_eq!(
            strings::text(strings::ONLINE_SOURCES_FOOTER),
            "Each block is self-contained: turning one off hides its sidebar entry and stops its requests; subscriptions and favorites are kept, not deleted."
        );
    }

    #[test]
    fn set_9_block_subtitles_match_the_design() {
        assert_eq!(
            strings::text(strings::ONLINE_SOURCES_YOUTUBE_SUBTITLE),
            "Channel feeds, audio via yt-dlp"
        );
        assert_eq!(
            strings::text(strings::ONLINE_SOURCES_PODCASTS_SUBTITLE),
            "RSS feeds, search via Apple Podcasts"
        );
        assert_eq!(
            strings::text(strings::ONLINE_SOURCES_RADIO_SUBTITLE),
            "Directory: radio-browser.info"
        );
    }
}
