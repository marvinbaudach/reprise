//! Shared module-state mutation across Preferences and contributed surfaces.
//!
//! A module switch can own live runtime state in addition to its persisted
//! Core flag. This coordinator keeps those per-module adapters and the common
//! post-write refresh in one place, so Plugins and sidebar actions cannot
//! drift into different behavior.

use std::rc::Rc;

use reprise_core::modules::ModuleDescriptor;

use super::PreferencesContext;

fn persist_module_state(
    context: &PreferencesContext,
    descriptor: &'static ModuleDescriptor,
    enabled: bool,
) -> Result<(), String> {
    if descriptor.id == "song_visuals" {
        if let Some(player) = &context.player {
            player
                .set_song_visuals_enabled(enabled)
                .map_err(|error| error.to_string())?;
        }
        return match reprise_core::modules::set_enabled(&context.conn, descriptor, enabled) {
            Ok(()) => {
                context.info_panel.set_song_visuals_enabled(enabled);
                Ok(())
            }
            Err(error) => {
                if let Some(player) = &context.player {
                    let _ = player.set_song_visuals_enabled(!enabled);
                }
                Err(error.to_string())
            }
        };
    }
    let result = match descriptor.id {
        "youtube" => context.podcasts.set_youtube_enabled(&context.conn, enabled),
        "podcasts" => context
            .podcasts
            .set_podcasts_enabled(&context.conn, enabled),
        "new_releases" => context.artist_news.set_enabled(&context.conn, enabled),
        "concerts" => context.concerts.set_enabled(&context.conn, enabled),
        "cover_download" => context.cover_download.set_enabled(&context.conn, enabled),
        "artist_portraits" => context.artist_portrait.set_enabled(&context.conn, enabled),
        "online_lyrics" => match &context.player {
            Some(player) => player.set_online_lyrics_enabled(enabled),
            None => reprise_core::modules::set_enabled(&context.conn, descriptor, enabled),
        },
        _ => reprise_core::modules::set_enabled(&context.conn, descriptor, enabled),
    };
    result.map_err(|error| error.to_string())
}

impl PreferencesContext {
    pub(in crate::ui) fn wire_sidebar_module_menu(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.sidebar.set_on_disable_module(move |module| {
            let context = weak
                .upgrade()
                .ok_or_else(|| "preferences context is no longer available".to_string())?;
            context.set_module_enabled(module, false, "sidebar module disabled")
        });
    }

    pub(super) fn set_module_enabled(
        &self,
        descriptor: &'static ModuleDescriptor,
        enabled: bool,
        reason: &'static str,
    ) -> Result<(), String> {
        persist_module_state(self, descriptor, enabled)?;
        self.refresh_online_module_state(reason);
        Ok(())
    }
}
