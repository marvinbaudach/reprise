//! Off-main-thread track-change notifications with stale-cover rejection.

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;

use super::player_controller::PlayerController;

fn notification_body(artist: &str, album: &str) -> String {
    format!("{artist} — {album}")
}

fn generation_is_current(expected: u64, current: u64) -> bool {
    expected == current
}

impl PlayerController {
    pub(super) fn notify_now_playing(&self, title: &str, artist: &str, album: &str, path: &str) {
        let Some(application) = self.application.upgrade() else {
            tracing::debug!("no application handle; skipping track-change notification");
            return;
        };
        let title = title.to_string();
        let body = notification_body(artist, album);
        let path = path.to_string();
        let expected_generation = self.bar_cover_generation.get();
        let current_generation = self.bar_cover_generation.clone();

        let notification = gio::Notification::new(&title);
        notification.set_body(Some(&body));
        application.send_notification(Some("now-playing"), &notification);

        glib::spawn_future_local(async move {
            let thumbnail = gio::spawn_blocking(move || {
                let source = reprise_core::cover::resolve_source(std::path::Path::new(&path))?;
                reprise_core::cover::thumbnail(&source, reprise_core::cover::ThumbnailSize::Bar)
                    .ok()
            })
            .await
            .ok()
            .flatten();
            if !generation_is_current(expected_generation, current_generation.get()) {
                return;
            }

            let Some(thumbnail) = thumbnail else {
                return;
            };
            let notification = gio::Notification::new(&title);
            notification.set_body(Some(&body));
            let icon = gio::FileIcon::new(&gio::File::for_path(thumbnail));
            notification.set_icon(&icon);
            application.send_notification(Some("now-playing"), &notification);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_body_keeps_artist_and_album_readable() {
        assert_eq!(notification_body("Artist", "Album"), "Artist — Album");
    }

    #[test]
    fn only_the_current_cover_generation_may_send() {
        assert!(generation_is_current(7, 7));
        assert!(!generation_is_current(7, 8));
    }
}
