//! Off-main-thread track-change notifications with stale-cover rejection.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use libadwaita as adw;
use reprise_core::db::Db;

use super::player_controller::PlayerController;

#[path = "notifications_updates.rs"]
pub(super) mod updates;

const UPDATE_DUE_CHECK_SECONDS: u32 = 60 * 60;

pub(crate) fn install_update_actions(
    application: &gio::Application,
    open_view: impl Fn(&str) + 'static,
) {
    let open_link = gio::SimpleAction::new("open-updates-link", Some(glib::VariantTy::STRING));
    open_link.connect_activate(|_, parameter| {
        let Some(url) = parameter.and_then(glib::Variant::str) else {
            tracing::warn!("update notification link action received no string target");
            return;
        };
        super::external_link::launch(url, "update notification", None);
    });
    application.add_action(&open_link);

    let open_view = Rc::new(open_view);
    let view_action = gio::SimpleAction::new("open-updates-view", Some(glib::VariantTy::STRING));
    view_action.connect_activate(move |_, parameter| {
        let Some(target) = parameter.and_then(glib::Variant::str) else {
            tracing::warn!("update notification view action received no string target");
            return;
        };
        open_view(target);
    });
    application.add_action(&view_action);
}

pub(crate) fn arm_update_notifications(application: &adw::Application, db: &Rc<Db>) {
    let application = application.clone().upcast::<gio::Application>();
    let db = db.clone();
    let cover_generation = Rc::new(Cell::new(0));
    let run = Rc::new(move || {
        let now = chrono::Utc::now().timestamp();
        let today = chrono::Local::now().date_naive();
        if let Err(error) =
            updates::send_due_releases(&application, &db, now, now, today, &cover_generation)
        {
            tracing::warn!(%error, "could not run update notification due check");
        }
    });
    {
        let run = run.clone();
        glib::idle_add_local_once(move || run());
    }
    glib::timeout_add_seconds_local(UPDATE_DUE_CHECK_SECONDS, move || {
        run();
        glib::ControlFlow::Continue
    });
}

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
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk4::glib::variant::ToVariant;

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

    #[test]
    fn update_notification_actions_accept_only_string_targets() {
        let application = gio::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
        let opened = Rc::new(RefCell::new(Vec::new()));
        let opened_for_action = opened.clone();
        install_update_actions(&application, move |target| {
            opened_for_action.borrow_mut().push(target.to_owned());
        });

        let link = application.lookup_action("open-updates-link").unwrap();
        let view = application.lookup_action("open-updates-view").unwrap();
        assert_eq!(
            link.parameter_type().as_deref(),
            Some(glib::VariantTy::STRING)
        );
        assert_eq!(
            view.parameter_type().as_deref(),
            Some(glib::VariantTy::STRING)
        );

        view.activate(Some(&"concerts".to_variant()));
        assert_eq!(opened.borrow().as_slice(), ["concerts"]);
    }
}
