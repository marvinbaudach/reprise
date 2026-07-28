use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::modules::ModuleDescriptor;

use super::{strings, PreferencesContext};

const TARGET_CLASS: &str = "reprise-plugin-target";
pub(in crate::ui) const ONLINE_LYRICS_TARGETS: &[&str] = &["online_lyrics"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PluginGroup {
    Local,
    Online,
    Connected,
}

fn plugin_group(descriptor: &ModuleDescriptor) -> PluginGroup {
    match descriptor.id {
        "song_visuals" | "library_doctor" => PluginGroup::Local,
        "listenbrainz" | "lastfm" => PluginGroup::Connected,
        _ => PluginGroup::Online,
    }
}

fn creates_scrobbling_entry(descriptor: &ModuleDescriptor) -> bool {
    descriptor.id == "listenbrainz"
}

/// YouTube, Podcasts and Radio have moved to the dedicated Online sources
/// page (`SET-8`) — their master switches and rows no longer render here.
fn moved_to_online_sources_page(descriptor: &ModuleDescriptor) -> bool {
    matches!(descriptor.id, "podcasts" | "youtube" | "radio")
}

pub(in crate::ui) fn network_descriptors() -> [&'static ModuleDescriptor; 6] {
    [
        &reprise_core::modules::LIBRARY_DOCTOR_MODULE,
        &reprise_core::modules::NEW_RELEASES_MODULE,
        &reprise_core::modules::CONCERTS_MODULE,
        &reprise_core::modules::COVER_DOWNLOAD_MODULE,
        &reprise_core::modules::ARTIST_PORTRAITS_MODULE,
        &reprise_core::modules::ONLINE_LYRICS_MODULE,
    ]
}

pub(in crate::ui) fn highlight_duration() -> std::time::Duration {
    std::time::Duration::from_millis(u64::from(crate::ui::motion::AMBIENT_MS))
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{TARGET_CLASS} {{ \
           background-color: alpha(@accent_bg_color, 0.22); \
           box-shadow: inset 3px 0 @accent_color; \
           transition: background-color {}ms {}, box-shadow {}ms {}; }}",
        crate::ui::motion::MICRO_MS,
        crate::ui::motion::MICRO_CSS_EASING,
        crate::ui::motion::MICRO_MS,
        crate::ui::motion::MICRO_CSS_EASING,
    )
}

pub(in crate::ui) fn plugin_applies_live(descriptor: &ModuleDescriptor) -> bool {
    descriptor.applies_live
}

pub(in crate::ui) fn plugin_title(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::LISTENBRAINZ,
        "lastfm" => strings::LASTFM,
        "new_releases" => strings::NEW_RELEASES,
        "concerts" => strings::CONCERTS,
        "podcasts" => strings::PODCASTS,
        "radio" => strings::RADIO,
        "library_doctor" => strings::LIBRARY_DOCTOR,
        "cover_download" => strings::COVER_DOWNLOAD,
        "artist_portraits" => strings::ARTIST_PORTRAITS,
        "online_lyrics" => strings::ONLINE_LYRICS,
        "song_visuals" => strings::SONG_VISUALS,
        _ => return descriptor.name.to_string(),
    };
    strings::text(message)
}

pub(in crate::ui) fn plugin_description(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::PLUGIN_LISTENBRAINZ_DESCRIPTION,
        "lastfm" => strings::PLUGIN_LASTFM_DESCRIPTION,
        "new_releases" => strings::NEW_RELEASES_DESCRIPTION,
        "concerts" => strings::CONCERTS_DESCRIPTION,
        "podcasts" => strings::PODCASTS_DESCRIPTION,
        "radio" => strings::RADIO_DESCRIPTION,
        "library_doctor" => strings::LIBRARY_DOCTOR_DESCRIPTION,
        "cover_download" => strings::COVER_DOWNLOAD_DESCRIPTION,
        "artist_portraits" => strings::ARTIST_PORTRAITS_DESCRIPTION,
        "online_lyrics" => strings::ONLINE_LYRICS_DESCRIPTION,
        "song_visuals" => strings::SONG_VISUALS_DESCRIPTION,
        _ => return descriptor.description.to_string(),
    };
    strings::text(message)
}

impl PreferencesContext {
    pub(in crate::ui) fn plugins_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_PLUGINS))
            .icon_name("application-x-addon-symbolic")
            .build();
        let local_group = adw::PreferencesGroup::builder()
            .title(strings::text(strings::LOCAL_FEATURES))
            .build();
        let online_group = adw::PreferencesGroup::builder()
            .title(strings::text(strings::ONLINE_CONTENT))
            .build();
        let connected_group = adw::PreferencesGroup::builder()
            .title(strings::text(strings::CONNECTED_SERVICES))
            .build();
        for descriptor in reprise_core::modules::ALL_MODULES {
            if moved_to_online_sources_page(descriptor) {
                continue;
            }
            // Scrobbling has one entry; provider controls live on its detail page.
            if plugin_group(descriptor) == PluginGroup::Connected {
                if creates_scrobbling_entry(descriptor) {
                    connected_group.add(&super::preference_scrobbling::build(self));
                }
                continue;
            }
            let group = match plugin_group(descriptor) {
                PluginGroup::Local => &local_group,
                PluginGroup::Online => &online_group,
                PluginGroup::Connected => &connected_group,
            };
            if descriptor.id == "library_doctor" {
                group.add(&super::preference_library_doctor::plugin_row(self));
                continue;
            }

            let description = plugin_description(descriptor);
            let subtitle = if plugin_applies_live(descriptor) {
                description
            } else {
                format!(
                    "{} · {}",
                    description,
                    strings::text(strings::RESTART_REQUIRED)
                )
            };
            let active = reprise_core::modules::is_enabled(&self.conn.borrow(), descriptor)
                .unwrap_or(descriptor.default_enabled);
            let row = adw::SwitchRow::builder()
                .title(plugin_title(descriptor))
                .subtitle(subtitle)
                .use_markup(false)
                .active(active)
                .build();
            if network_descriptors()
                .iter()
                .any(|network| network.id == descriptor.id)
            {
                let row_ref = glib::WeakRef::new();
                row_ref.set(Some(row.upcast_ref::<gtk4::Widget>()));
                self.plugin_rows.borrow_mut().insert(descriptor.id, row_ref);
            }
            let syncing = Rc::new(Cell::new(false));
            let weak = Rc::downgrade(self);
            let descriptor = *descriptor;
            let syncing_notify = syncing.clone();
            row.connect_active_notify(move |row| {
                let Some(context) = weak.upgrade() else {
                    return;
                };
                if syncing_notify.get() {
                    return;
                }
                let active = row.is_active();
                let result = match descriptor.id {
                    "new_releases" => context
                        .artist_news
                        .set_enabled(&context.conn.borrow(), active),
                    "concerts" => context.concerts.set_enabled(&context.conn.borrow(), active),
                    "cover_download" => context
                        .cover_download
                        .set_enabled(&context.conn.borrow(), active),
                    "artist_portraits" => context
                        .artist_portrait
                        .set_enabled(&context.conn.borrow(), active),
                    "online_lyrics" => match &context.player {
                        Some(player) => player.set_online_lyrics_enabled(active),
                        None => reprise_core::modules::set_enabled(
                            &context.conn.borrow(),
                            descriptor,
                            active,
                        ),
                    },
                    "song_visuals" => {
                        if let Some(player) = &context.player {
                            if let Err(error) = player.set_song_visuals_enabled(active) {
                                tracing::warn!(%error, "could not apply live song visuals");
                                syncing_notify.set(true);
                                row.set_active(!active);
                                syncing_notify.set(false);
                                return;
                            }
                        }
                        match reprise_core::modules::set_enabled(
                            &context.conn.borrow(),
                            descriptor,
                            active,
                        ) {
                            Ok(()) => {
                                context.info_panel.set_song_visuals_enabled(active);
                                Ok(())
                            }
                            Err(error) => {
                                if let Some(player) = &context.player {
                                    let _ = player.set_song_visuals_enabled(!active);
                                }
                                Err(error)
                            }
                        }
                    }
                    _ => reprise_core::modules::set_enabled(
                        &context.conn.borrow(),
                        descriptor,
                        active,
                    ),
                };
                if let Err(error) = result {
                    tracing::warn!(%error, module = descriptor.id, "could not save plugin state");
                    syncing_notify.set(true);
                    row.set_active(!active);
                    syncing_notify.set(false);
                    return;
                }
                if descriptor.id == "concerts" {
                    context.sidebar.refresh("source module toggled");
                }
            });
            if descriptor.id == "new_releases" {
                let alive = glib::WeakRef::new();
                alive.set(Some(&row));
                let target = alive.clone();
                let syncing = syncing.clone();
                self.artist_news.subscribe_enabled(
                    move || alive.upgrade().is_some(),
                    move |enabled| {
                        let Some(row) = target.upgrade() else { return };
                        syncing.set(true);
                        row.set_active(enabled);
                        syncing.set(false);
                    },
                );
            }
            if descriptor.id == "concerts" {
                let alive = glib::WeakRef::new();
                alive.set(Some(&row));
                let target = alive.clone();
                let syncing = syncing.clone();
                self.concerts.subscribe_enabled(
                    move || alive.upgrade().is_some(),
                    move |enabled| {
                        let Some(row) = target.upgrade() else { return };
                        syncing.set(true);
                        row.set_active(enabled);
                        syncing.set(false);
                    },
                );
            }
            group.add(&row);
        }
        page.add(&local_group);
        page.add(&online_group);
        page.add(&connected_group);
        page
    }

    pub(in crate::ui) fn highlight_pending_plugin_rows(&self) {
        let targets = std::mem::take(&mut *self.pending_plugin_targets.borrow_mut());
        let rows = targets
            .iter()
            .filter_map(|target| {
                self.plugin_rows
                    .borrow()
                    .get(target)
                    .and_then(glib::WeakRef::upgrade)
            })
            .collect::<Vec<_>>();
        if let Some(first) = rows.first() {
            first.grab_focus();
        }
        for row in rows {
            if let Ok(expander) = row.clone().downcast::<adw::ExpanderRow>() {
                expander.set_expanded(true);
            }
            row.add_css_class(TARGET_CLASS);
            let row = row.downgrade();
            glib::timeout_add_local_once(highlight_duration(), move || {
                if let Some(row) = row.upgrade() {
                    row.remove_css_class(TARGET_CLASS);
                }
            });
        }
    }

    pub(in crate::ui) fn set_library_doctor_job_running(&self, running: bool) {
        self.library_doctor_job_running.set(running);
        self.doctor_controls.set_job_running(running);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nr_7_new_releases_plugin_uses_privacy_copy_and_live_toggle_id() {
        let descriptor = &reprise_core::modules::NEW_RELEASES_MODULE;

        assert_eq!(plugin_title(descriptor), "New Releases");
        assert!(plugin_description(descriptor).contains("contacts MusicBrainz"));
        assert!(plugin_applies_live(descriptor));
    }

    #[test]
    fn concerts_plugin_uses_event_provider_privacy_copy_and_live_toggle_id() {
        let descriptor = &reprise_core::modules::CONCERTS_MODULE;

        assert_eq!(plugin_title(descriptor), "Concerts");
        assert!(plugin_description(descriptor).contains("contacts"));
        assert!(plugin_applies_live(descriptor));
    }

    #[test]
    fn podcast_and_radio_plugins_expose_source_privacy_copy() {
        for descriptor in [
            &reprise_core::modules::PODCASTS_MODULE,
            &reprise_core::modules::RADIO_MODULE,
        ] {
            assert!(plugin_description(descriptor)
                .to_ascii_lowercase()
                .contains("contacts"));
            assert!(plugin_applies_live(descriptor));
        }
    }

    /// `SET-8`: the three online-sources modules render on their own page,
    /// not on Plugins — no duplicated master switches.
    #[test]
    fn set_8_podcasts_youtube_and_radio_moved_off_the_plugins_page() {
        for descriptor in [
            &reprise_core::modules::PODCASTS_MODULE,
            &reprise_core::modules::YOUTUBE_MODULE,
            &reprise_core::modules::RADIO_MODULE,
        ] {
            assert!(moved_to_online_sources_page(descriptor));
        }
        assert!(!moved_to_online_sources_page(
            &reprise_core::modules::CONCERTS_MODULE
        ));
    }

    #[test]
    fn network_plugin_deep_link_highlight_is_transient() {
        assert_eq!(
            highlight_duration(),
            std::time::Duration::from_millis(u64::from(crate::ui::motion::AMBIENT_MS))
        );
        let css = css();
        assert!(css.contains(".reprise-plugin-target"));
        assert!(css.contains(&format!(
            "{}ms {}",
            crate::ui::motion::MICRO_MS,
            crate::ui::motion::MICRO_CSS_EASING
        )));
    }

    #[test]
    fn all_network_plugin_rows_expose_privacy_copy() {
        let descriptors = network_descriptors();
        assert_eq!(descriptors.len(), 6);
        for descriptor in descriptors {
            assert!(plugin_applies_live(descriptor));
            assert!(plugin_description(descriptor)
                .to_ascii_lowercase()
                .contains("contacts"));
        }
    }

    #[test]
    fn set_6a_plugins_are_grouped_by_user_intent_with_one_scrobbling_entry() {
        assert_eq!(
            plugin_group(&reprise_core::modules::SONG_VISUALS_MODULE),
            PluginGroup::Local
        );
        assert_eq!(
            plugin_group(&reprise_core::modules::LIBRARY_DOCTOR_MODULE),
            PluginGroup::Local
        );
        for descriptor in [
            &reprise_core::modules::NEW_RELEASES_MODULE,
            &reprise_core::modules::CONCERTS_MODULE,
            &reprise_core::modules::PODCASTS_MODULE,
            &reprise_core::modules::RADIO_MODULE,
            &reprise_core::modules::COVER_DOWNLOAD_MODULE,
            &reprise_core::modules::ARTIST_PORTRAITS_MODULE,
            &reprise_core::modules::ONLINE_LYRICS_MODULE,
        ] {
            assert_eq!(plugin_group(descriptor), PluginGroup::Online);
        }
        for descriptor in [
            &reprise_core::modules::LISTENBRAINZ_MODULE,
            &reprise_core::modules::LASTFM_MODULE,
        ] {
            assert_eq!(plugin_group(descriptor), PluginGroup::Connected);
        }
        assert_eq!(
            reprise_core::modules::ALL_MODULES
                .iter()
                .filter(|descriptor| creates_scrobbling_entry(descriptor))
                .count(),
            1
        );
    }

    #[test]
    fn doc_6b_library_doctor_controls_explain_job_locking() {
        let idle = super::super::preference_library_doctor::control_state(false);
        assert!(idle.remote_sensitive);
        assert!(!idle.subtitle.contains("running"));

        let running = super::super::preference_library_doctor::control_state(true);
        assert!(!running.remote_sensitive);
        assert!(running.subtitle.contains("running"));
    }

    #[test]
    fn doc_7b_library_doctor_is_available_without_an_activation_state() {
        let idle = super::super::preference_library_doctor::control_state(false);
        assert!(idle.remote_sensitive);
        assert!(idle.revert_sensitive);
    }
}
