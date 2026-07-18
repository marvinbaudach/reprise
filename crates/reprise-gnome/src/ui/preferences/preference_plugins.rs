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

pub(in crate::ui) fn network_descriptors() -> [&'static ModuleDescriptor; 4] {
    [
        &reprise_core::modules::NEW_RELEASES_MODULE,
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
        "cover_download" => strings::COVER_DOWNLOAD,
        "artist_portraits" => strings::ARTIST_PORTRAITS,
        "online_lyrics" => strings::ONLINE_LYRICS,
        _ => return descriptor.name.to_string(),
    };
    strings::text(message)
}

pub(in crate::ui) fn plugin_description(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::PLUGIN_LISTENBRAINZ_DESCRIPTION,
        "lastfm" => strings::PLUGIN_LASTFM_DESCRIPTION,
        "new_releases" => strings::NEW_RELEASES_DESCRIPTION,
        "cover_download" => strings::COVER_DOWNLOAD_DESCRIPTION,
        "artist_portraits" => strings::ARTIST_PORTRAITS_DESCRIPTION,
        "online_lyrics" => strings::ONLINE_LYRICS_DESCRIPTION,
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
        let group = adw::PreferencesGroup::new();
        for descriptor in reprise_core::modules::ALL_MODULES {
            // Scrobbling services use inline ExpanderRows instead of SwitchRows.
            if descriptor.id == "listenbrainz" {
                group.add(&self.build_listenbrainz_row());
                continue;
            }
            if descriptor.id == "lastfm" {
                group.add(&self.build_lastfm_row());
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
                row_ref.set(Some(&row));
                self.plugin_rows.borrow_mut().insert(descriptor.id, row_ref);
            }
            let scope_row = (descriptor.id == "new_releases")
                .then(|| super::preference_new_releases::scope_row(&self.conn, active));
            let syncing = Rc::new(Cell::new(false));
            let weak = Rc::downgrade(self);
            let descriptor = *descriptor;
            let syncing_notify = syncing.clone();
            let scope_notify = scope_row.clone();
            row.connect_active_notify(move |row| {
                let Some(context) = weak.upgrade() else {
                    return;
                };
                if syncing_notify.get() {
                    return;
                }
                let active = row.is_active();
                if let Some(scope) = &scope_notify {
                    scope.set_sensitive(active);
                }
                let result = match descriptor.id {
                    "new_releases" => context
                        .artist_news
                        .set_enabled(&context.conn.borrow(), active),
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
                }
            });
            if descriptor.id == "new_releases" {
                let alive = glib::WeakRef::new();
                alive.set(Some(&row));
                let target = alive.clone();
                let scope_target = scope_row.as_ref().map(|scope| {
                    let target = glib::WeakRef::new();
                    target.set(Some(scope));
                    target
                });
                let syncing = syncing.clone();
                self.artist_news.subscribe_enabled(
                    move || alive.upgrade().is_some(),
                    move |enabled| {
                        let Some(row) = target.upgrade() else { return };
                        syncing.set(true);
                        row.set_active(enabled);
                        syncing.set(false);
                        if let Some(scope) = scope_target.as_ref().and_then(glib::WeakRef::upgrade)
                        {
                            scope.set_sensitive(enabled);
                        }
                    },
                );
            }
            group.add(&row);
            if let Some(scope) = scope_row {
                group.add(&scope);
            }
        }
        page.add(&group);
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
            row.add_css_class(TARGET_CLASS);
            let row = row.downgrade();
            glib::timeout_add_local_once(highlight_duration(), move || {
                if let Some(row) = row.upgrade() {
                    row.remove_css_class(TARGET_CLASS);
                }
            });
        }
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
        assert_eq!(descriptors.len(), 4);
        for descriptor in descriptors {
            assert!(plugin_applies_live(descriptor));
            assert!(plugin_description(descriptor).contains("contacts"));
        }
    }
}
