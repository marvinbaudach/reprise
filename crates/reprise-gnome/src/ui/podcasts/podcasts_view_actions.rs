//! Action-group installation for `PodcastsView`. Split out of
//! `podcasts_view.rs` to keep it under the file-size gate.

use super::*;

impl PodcastsView {
    pub(super) fn install_actions(self: &Rc<Self>) {
        let group = gio::SimpleActionGroup::new();
        self.add_target_action(&group, podcasts_context_menu::ACTION_PLAY, |view, id| {
            if let Ok(Some(row)) = podcasts::store::episode(&view.conn, id) {
                (view.callbacks.on_episode_activated)(row);
            }
        });
        self.add_target_action(
            &group,
            podcasts_context_menu::ACTION_COPY_URL,
            |view, id| {
                if let Ok(Some(row)) = podcasts::store::episode(&view.conn, id) {
                    view.root.clipboard().set_text(&row.audio_url);
                }
            },
        );
        self.add_target_action(
            &group,
            podcasts_context_menu::ACTION_TOGGLE_PLAYED,
            |view, id| {
                if let Ok(Some(row)) = podcasts::store::episode(&view.conn, id) {
                    let result = if row.played_at.is_some() {
                        podcasts::store::mark_unplayed(&view.conn, id)
                    } else {
                        podcasts::store::mark_played(&view.conn, id, chrono::Utc::now().timestamp())
                    };
                    if let Err(error) = result {
                        tracing::warn!(%error, "could not update podcast episode status");
                    }
                    view.refresh();
                    (view.callbacks.on_sidebar_refresh)();
                }
            },
        );
        self.add_target_action(
            &group,
            podcasts_context_menu::ACTION_TOGGLE_DOWNLOAD,
            PodcastsView::toggle_download,
        );
        self.add_target_action(
            &group,
            podcasts_context_menu::ACTION_REMOVE_EPISODE,
            PodcastsView::remove_episode,
        );
        self.add_target_action(
            &group,
            podcasts_context_menu::ACTION_UNSUBSCRIBE,
            PodcastsView::unsubscribe,
        );
        super::super::podcasts_device_sync::install_action(self, &group);
        let load_more =
            gio::SimpleAction::new("load-more", Some(&<(i64, u32)>::static_variant_type()));
        let weak = Rc::downgrade(self);
        load_more.connect_activate(move |_, target| {
            let Some(view) = weak.upgrade() else { return };
            let Some((subscription_id, end)) =
                target.and_then(gtk4::glib::Variant::get::<(i64, u32)>)
            else {
                return;
            };
            view.request_load_more(subscription_id, end as usize);
        });
        group.add_action(&load_more);
        self.add_target_action(&group, "show-all-episodes", |view, subscription_id| {
            view.expanded_episode_sources
                .borrow_mut()
                .insert(subscription_id);
            view.render();
        });
        self.youtube_detail.install_actions(&group);
        let add = gio::SimpleAction::new("open-add", None);
        let weak = Rc::downgrade(self);
        add.connect_activate(move |_, _| {
            if let Some(view) = weak.upgrade() {
                view.open_add_dialog();
            }
        });
        group.add_action(&add);
        self.root.insert_action_group("podcasts", Some(&group));
    }

    fn add_target_action(
        self: &Rc<Self>,
        group: &gio::SimpleActionGroup,
        name: &str,
        callback: impl Fn(&Rc<Self>, i64) + 'static,
    ) {
        let action = gio::SimpleAction::new(name, Some(&i64::static_variant_type()));
        let weak = Rc::downgrade(self);
        action.connect_activate(move |_, target| {
            let Some(view) = weak.upgrade() else {
                return;
            };
            let Some(id) = target.and_then(glib::Variant::get::<i64>) else {
                return;
            };
            callback(&view, id);
        });
        group.add_action(&action);
    }

    pub(super) fn open_add_dialog(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        add_dialog::present(
            &self.root,
            &self.conn,
            self.kind,
            self.connectivity(),
            move |import_latest| {
                if let Some(view) = weak.upgrade() {
                    view.refresh();
                    if import_latest {
                        view.request_refresh(true);
                    }
                    (view.callbacks.on_sidebar_refresh)();
                }
            },
        );
    }
}
