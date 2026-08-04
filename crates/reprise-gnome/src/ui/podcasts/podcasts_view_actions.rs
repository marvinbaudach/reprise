//! Action-group installation for `PodcastsView`. Split out of
//! `podcasts_view.rs` to keep it under the file-size gate.

use super::super::podcasts_batch_actions::{self, BatchResult};
use super::*;
use crate::ui::podcasts::podcasts_playback::{activation_for_episode, EpisodeActivation};

impl PodcastsView {
    pub(super) fn install_actions(self: &Rc<Self>) {
        let group = gio::SimpleActionGroup::new();
        self.add_target_action(&group, podcasts_context_menu::ACTION_PLAY, |view, id| {
            match activation_for_episode(view.playing_episode.get().map(|mark| mark.id), id) {
                EpisodeActivation::TogglePlayback => {
                    (view.callbacks.on_play_pause)();
                }
                EpisodeActivation::StartEpisode => {
                    if let Ok(Some(row)) = podcasts::store::episode(&view.conn, id) {
                        let episode_ids = view.neighbour_ids_for_episode(id);
                        view.activating_here.set(true);
                        (view.callbacks.on_episode_activated)(row, episode_ids);
                        view.activating_here.set(false);
                    }
                }
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
            podcasts_context_menu::ACTION_OPEN_IN_BROWSER,
            |view, id| {
                if let Ok(Some(row)) = podcasts::store::episode(&view.conn, id) {
                    if let Some(url) = podcasts_context_menu::browser_url(&row) {
                        crate::ui::external_link::launch(url, "podcast episode page", None);
                    }
                }
            },
        );
        self.add_selected_action(
            &group,
            podcasts_context_menu::ACTION_PLAY_NEXT,
            |view, ids| view.queue_selected_episodes(&ids, QueuePlacement::PlayNext),
        );
        self.add_selected_action(
            &group,
            podcasts_context_menu::ACTION_ADD_TO_QUEUE,
            |view, ids| view.queue_selected_episodes(&ids, QueuePlacement::End),
        );
        podcasts_context_menu::install_disabled_queue_actions(&group);
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
        self.add_selected_action(
            &group,
            podcasts_context_menu::ACTION_MARK_PLAYED_SELECTED,
            |view, ids| view.mark_selected(&ids, true),
        );
        self.add_selected_action(
            &group,
            podcasts_context_menu::ACTION_MARK_UNPLAYED_SELECTED,
            |view, ids| view.mark_selected(&ids, false),
        );
        self.add_selected_action(
            &group,
            podcasts_context_menu::ACTION_DOWNLOAD_SELECTED,
            |view, ids| view.download_selected(&ids),
        );
        self.add_selected_action(
            &group,
            podcasts_context_menu::ACTION_DELETE_DOWNLOADS_SELECTED,
            |view, ids| view.delete_downloads_selected(&ids),
        );
        self.add_selected_action(
            &group,
            podcasts_context_menu::ACTION_REMOVE_SELECTED,
            |view, ids| view.remove_episodes(&ids),
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
        let select_row =
            gio::SimpleAction::new("select-row", Some(&<(i64, u8)>::static_variant_type()));
        let weak = Rc::downgrade(self);
        select_row.connect_activate(move |_, target| {
            let Some(view) = weak.upgrade() else { return };
            let Some((episode_id, mode)) = target.and_then(glib::Variant::get::<(i64, u8)>) else {
                return;
            };
            let Some(mode) = SelectMode::from_u8(mode) else {
                tracing::debug!(mode, "unknown podcast selection mode");
                return;
            };
            view.select_row(episode_id, mode);
        });
        group.add_action(&select_row);
        let clear_selection = gio::SimpleAction::new("clear-selection", None);
        let weak = Rc::downgrade(self);
        clear_selection.connect_activate(move |_, _| {
            if let Some(view) = weak.upgrade() {
                view.clear_visible_selection();
            }
        });
        group.add_action(&clear_selection);
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

    fn neighbour_ids_for_episode(&self, episode_id: i64) -> Vec<i64> {
        if let Some(ids) = self.youtube_detail.neighbour_ids_for_episode(episode_id) {
            return ids;
        }
        episode_ids_in_rendered_order(&self.groups.borrow())
    }

    /// Whether this view currently renders the episode at all. Only the
    /// headless `REPRISE_SMOKE_EPISODE_PLAY` hook needs this: both source views
    /// arm it, and without the check the one that does *not* show the episode
    /// would still start it — neighbourless, since it is absent from that
    /// view's own rendered order — and overwrite the correct session.
    pub(in crate::ui) fn renders_episode(&self, episode_id: i64) -> bool {
        self.neighbour_ids_for_episode(episode_id)
            .contains(&episode_id)
    }

    fn add_selected_action(
        self: &Rc<Self>,
        group: &gio::SimpleActionGroup,
        name: &str,
        callback: impl Fn(&Rc<Self>, Vec<i64>) + 'static,
    ) {
        let action = gio::SimpleAction::new(name, Some(&Vec::<i64>::static_variant_type()));
        let weak = Rc::downgrade(self);
        action.connect_activate(move |_, target| {
            let Some(view) = weak.upgrade() else {
                return;
            };
            let Some(ids) = target.and_then(glib::Variant::get::<Vec<i64>>) else {
                return;
            };
            callback(&view, ids);
        });
        group.add_action(&action);
    }

    fn mark_selected(&self, episode_ids: &[i64], played: bool) {
        let now = chrono::Utc::now().timestamp();
        let result = podcasts_batch_actions::run_batch(episode_ids, |episode_id| {
            let outcome = if played {
                podcasts::store::mark_played(&self.conn, episode_id, now)
            } else {
                podcasts::store::mark_unplayed(&self.conn, episode_id)
            };
            if let Err(error) = outcome {
                tracing::warn!(%error, episode_id, "could not update podcast episode status");
                return false;
            }
            true
        });
        if result.succeeded() > 0 {
            self.refresh();
            (self.callbacks.on_sidebar_refresh)();
        }
    }

    fn queue_selected_episodes(&self, episode_ids: &[i64], placement: QueuePlacement) {
        let Some(items) = available_episode_items(&self.conn, episode_ids) else {
            tracing::warn!("refused stale or unavailable podcast queue selection");
            return;
        };
        let queued = match placement {
            QueuePlacement::PlayNext => (self.callbacks.on_play_next)(&items),
            QueuePlacement::End => (self.callbacks.on_add_to_queue)(&items),
        };
        if queued {
            self.show_batch_toast(&strings::episodes_added_to_queue_toast(items.len()));
        }
    }

    fn download_selected(self: &Rc<Self>, episode_ids: &[i64]) {
        let states = self.download_states.borrow().clone();
        let targets = podcasts_batch_actions::downloadable_ids(episode_ids, &states);
        for episode_id in targets {
            self.toggle_download(episode_id);
        }
    }

    fn delete_downloads_selected(self: &Rc<Self>, episode_ids: &[i64]) {
        let downloads = episode_ids
            .iter()
            .filter_map(|episode_id| {
                podcasts::store::episode(&self.conn, *episode_id)
                    .ok()
                    .flatten()
                    .and_then(|episode| {
                        episode
                            .downloaded_path
                            .map(|path| (episode.id, std::path::PathBuf::from(path)))
                    })
            })
            .collect::<Vec<_>>();
        let result = podcasts_batch_actions::trash_downloads(&downloads, |path| {
            match gio::File::for_path(path).trash(None::<&gio::Cancellable>) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(%error, path = %path.display(), "could not trash podcast download");
                    false
                }
            }
        });
        for episode_id in &result.succeeded_ids {
            if let Err(error) = podcasts::store::set_downloaded_path(&self.conn, *episode_id, None)
            {
                tracing::warn!(%error, episode_id, "could not clear podcast download path");
            }
        }
        if result.requested == 0 {
            // Nothing in the selection had a downloaded file. Silence would
            // read as a dead button, so say so.
            self.show_batch_toast(&strings::text(strings::PODCAST_BATCH_NOTHING_TO_DELETE));
            return;
        }
        self.refresh();
        self.show_batch_result(&result);
    }

    fn remove_episodes(self: &Rc<Self>, episode_ids: &[i64]) {
        // SRC-12a requires a one-episode selection to behave exactly as it did
        // before multi-selection existed. The batch path would report the
        // generic "1 removed" where the single path names the episode, so a
        // lone selection is handed straight back to it. The context menu makes
        // the same choice in `build_for_selection`; the toolbar buttons route
        // here, so the fallback has to live at this end too.
        if let [episode_id] = episode_ids {
            self.remove_episode(*episode_id);
            return;
        }
        let episodes = episode_ids
            .iter()
            .filter_map(|episode_id| {
                podcasts::store::episode(&self.conn, *episode_id)
                    .ok()
                    .flatten()
                    .map(|episode| (episode.id, episode))
            })
            .collect::<BTreeMap<_, _>>();
        let now = chrono::Utc::now().timestamp();
        let result = podcasts_batch_actions::run_batch(episode_ids, |episode_id| {
            if !episodes.contains_key(&episode_id) {
                return false;
            }
            match podcasts::store::tombstone_episode(&self.conn, episode_id, now) {
                Ok(changed) => changed,
                Err(error) => {
                    tracing::warn!(%error, episode_id, "could not remove podcast episode");
                    false
                }
            }
        });
        self.selection
            .borrow_mut()
            .remove_all(&result.succeeded_ids);
        if result.succeeded() == 0 {
            self.show_batch_result(&result);
            return;
        }
        self.refresh();
        (self.callbacks.on_sidebar_refresh)();
        let Some(overlay) = self.toast_overlay.upgrade() else {
            self.commit_removed_episodes(&result.succeeded_ids, &episodes);
            return;
        };
        let toast = adw::Toast::new(&batch_result_text(&result));
        toast.set_button_label(Some(&strings::text(strings::PODCAST_UNDO)));
        toast.set_timeout(10);
        toast.set_priority(adw::ToastPriority::High);
        let undone = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(self);
        let undo_flag = undone.clone();
        let succeeded_ids = result.succeeded_ids.clone();
        toast.connect_button_clicked(move |_| {
            undo_flag.set(true);
            let Some(view) = weak.upgrade() else { return };
            let undo_result = podcasts_batch_actions::undo_batch(&succeeded_ids, |episode_id| {
                match podcasts::store::undo_remove_episode(&view.conn, episode_id) {
                    Ok(changed) => changed,
                    Err(error) => {
                        tracing::warn!(%error, episode_id, "could not undo podcast episode removal");
                        false
                    }
                }
            });
            view.refresh();
            (view.callbacks.on_sidebar_refresh)();
            // An undo can be partial — `commit_remove_episode` may already have
            // run for some of the batch. Staying silent about that would leave
            // episodes gone while the user believes the undo restored all of
            // them, so the same honest count every other batch reports is
            // reported here too.
            if undo_result.failed > 0 {
                view.show_batch_result(&undo_result);
            }
        });
        let weak = Rc::downgrade(self);
        let succeeded_ids = result.succeeded_ids;
        toast.connect_dismissed(move |_| {
            if undone.get() {
                return;
            }
            if let Some(view) = weak.upgrade() {
                view.commit_removed_episodes(&succeeded_ids, &episodes);
            }
        });
        overlay.add_toast(toast);
    }

    fn commit_removed_episodes(
        self: &Rc<Self>,
        episode_ids: &[i64],
        episodes: &BTreeMap<i64, EpisodeRow>,
    ) {
        for episode_id in episode_ids {
            match podcasts::store::commit_remove_episode(&self.conn, *episode_id) {
                Ok(Some(path)) => {
                    if let Some(episode) = episodes.get(episode_id) {
                        self.kept_downloads
                            .borrow_mut()
                            .add(episode.subscription_id, vec![path]);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, episode_id, "could not commit podcast episode removal");
                }
            }
        }
        self.schedule_download_toast();
    }

    fn show_batch_result(&self, result: &BatchResult) {
        self.show_batch_toast(&batch_result_text(result));
    }

    fn show_batch_toast(&self, message: &str) {
        let Some(overlay) = self.toast_overlay.upgrade() else {
            return;
        };
        overlay.add_toast(adw::Toast::new(message));
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

#[derive(Clone, Copy)]
enum QueuePlacement {
    PlayNext,
    End,
}

fn available_episode_items(db: &Db, episode_ids: &[i64]) -> Option<Vec<QueueItem>> {
    if episode_ids.is_empty() {
        return None;
    }
    episode_ids
        .iter()
        .map(|episode_id| {
            podcasts::store::episode(db, *episode_id)
                .ok()
                .flatten()
                .map(|_| QueueItem::Episode(*episode_id))
        })
        .collect()
}

fn batch_result_text(result: &BatchResult) -> String {
    strings::podcast_batch_result(result.succeeded(), result.failed)
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use reprise_core::podcasts::feed::ParsedEpisode;
    use reprise_core::podcasts::store::NewSubscription;

    #[test]
    fn src_12a_partial_batch_feedback_reports_every_success_and_failure_once() {
        let result = BatchResult {
            requested: 7,
            succeeded_ids: vec![1, 2, 3, 4],
            failed: 3,
        };

        // One message carrying both numbers, not two translated fragments
        // glued together — word order around "N done, M failed" is not the
        // same in every language.
        assert_eq!(batch_result_text(&result), "4 episodes updated; 3 failed");
        assert_eq!(result.succeeded() + result.failed, result.requested);
    }

    #[test]
    fn src_12a_a_fully_successful_batch_says_nothing_about_failures() {
        let result = BatchResult {
            requested: 3,
            succeeded_ids: vec![1, 2, 3],
            failed: 0,
        };

        assert_eq!(batch_result_text(&result), "3 episodes updated");
    }

    #[test]
    fn ctx_12_queue_activation_revalidates_every_selected_episode() {
        let db = Db::open_in_memory().unwrap();
        let subscription_id = podcasts::store::add_or_restore(
            &db,
            &NewSubscription {
                kind: PodcastKind::Rss,
                feed_url: "https://example.test/feed".into(),
                title: "Show".into(),
                author: None,
                image_url: None,
                auto_download: false,
            },
            1,
        )
        .unwrap();
        let episode_id = podcasts::store::upsert_episode(
            &db,
            subscription_id,
            &ParsedEpisode {
                guid: "episode".into(),
                title: "Episode".into(),
                image_url: None,
                audio_url: "https://example.test/episode.mp3".into(),
                page_url: None,
                published_at: None,
                duration_secs: None,
            },
            2,
        )
        .unwrap()
        .unwrap()
        .episode_id;

        assert_eq!(
            available_episode_items(&db, &[episode_id]),
            Some(vec![QueueItem::Episode(episode_id)])
        );
        assert_eq!(available_episode_items(&db, &[episode_id, i64::MAX]), None);
    }
}
