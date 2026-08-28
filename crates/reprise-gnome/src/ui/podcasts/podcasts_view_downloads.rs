//! Download, unsubscribe and episode-removal actions for `PodcastsView`.
//! Split out of `podcasts_view.rs` to keep it under the file-size gate.

use super::*;

impl PodcastsView {
    pub(super) fn toggle_download(self: &Rc<Self>, episode_id: i64) {
        let allowed = {
            let states = self.download_states.borrow();
            download_request_allowed(states.get(&episode_id))
        };
        if !allowed {
            return;
        }
        let Ok(Some(row)) = podcasts::store::episode(&self.conn, episode_id) else {
            return;
        };
        if let Some(path) = row.downloaded_path.as_deref() {
            let file_exists = std::path::Path::new(path).is_file();
            if download_toggle_action(Some(path), file_exists) == DownloadToggleAction::Trash {
                let file = gio::File::for_path(path);
                if let Err(error) = file.trash(None::<&gio::Cancellable>) {
                    self.show_error(&error.to_string());
                    return;
                }
            }
            if let Err(error) = podcasts::store::set_downloaded_path(&self.conn, episode_id, None) {
                self.show_error(&error.to_string());
                return;
            }
            self.download_states
                .borrow_mut()
                .insert(episode_id, DownloadState::NotDownloaded);
            if file_exists {
                self.refresh();
                return;
            }
        }
        if connectivity::deferrable_action_outcome(
            self.connectivity.get(),
            DownloadState::NotDownloaded.local_availability(),
        ) == ActionOutcome::QueuedOffline
        {
            self.deferred_actions
                .borrow_mut()
                .push(DeferredAction::Download(episode_id));
            self.set_download_state(episode_id, &DownloadState::Queued);
            self.footer_status
                .set_text(&strings::text(strings::PODCAST_QUEUED_OFFLINE));
            return;
        }
        self.dispatch_download(episode_id);
    }

    pub(super) fn dispatch_download(self: &Rc<Self>, episode_id: i64) -> bool {
        let operation = PodcastsOperation::Download { episode_id };
        let generation = request_generation(self.generation.get(), &operation);
        let (response, receiver) = podcasts_response_channel();
        if !self.runtime.request(PodcastsRequest {
            generation,
            operation,
            response,
        }) {
            return false;
        }
        self.set_download_state(episode_id, &DownloadState::Queued);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(response) = receiver.recv().await {
                let Some(view) = weak.upgrade() else {
                    return;
                };
                match response.result {
                    Ok(PodcastsWorkerResult::DownloadState { episode_id, state }) => {
                        let terminal = matches!(
                            state,
                            DownloadState::Downloaded { .. } | DownloadState::Failed { .. }
                        );
                        view.set_download_state(episode_id, &state);
                        if matches!(state, DownloadState::Downloaded { .. }) {
                            view.refresh();
                        }
                        if terminal {
                            break;
                        }
                    }
                    Ok(PodcastsWorkerResult::Refreshed(_)) => {}
                    Ok(PodcastsWorkerResult::LoadedMore { .. }) => {}
                    Ok(
                        PodcastsWorkerResult::Filled(_) | PodcastsWorkerResult::SyncProgress { .. },
                    ) => {}
                    Err(error) => {
                        tracing::warn!(%error, episode_id, "podcast download failed");
                        view.set_download_state(
                            episode_id,
                            &DownloadState::Failed {
                                message: strings::text(strings::PODCAST_DOWNLOAD_FAILED),
                            },
                        );
                        view.show_error(&strings::text(strings::PODCAST_DOWNLOAD_FAILED));
                        break;
                    }
                }
            }
        });
        true
    }

    pub(in crate::ui) fn set_download_state(&self, episode_id: i64, state: &DownloadState) {
        self.download_states
            .borrow_mut()
            .insert(episode_id, state.clone());
        let widgets = self.download_widgets.borrow().get(&episode_id).cloned();
        if let Some(widgets) = widgets {
            podcasts_groups::update_download_state(&widgets, state);
        }
        self.youtube_detail.update_download_state(episode_id, state);
    }

    pub(super) fn unsubscribe(self: &Rc<Self>, subscription_id: i64) {
        let Ok(Some(subscription)) = podcasts::store::subscription(&self.conn, subscription_id)
        else {
            return;
        };
        self.cancel_subscription_sync(subscription_id);
        let paths = podcasts::store::downloaded_paths_for_subscription(&self.conn, subscription_id)
            .unwrap_or_default();
        if let Err(error) = podcasts::store::tombstone_subscription(
            &self.conn,
            subscription_id,
            chrono::Utc::now().timestamp(),
        ) {
            self.show_error(&error.to_string());
            return;
        }
        (self.callbacks.on_subscription_removed)(subscription_id);
        (self.callbacks.on_sidebar_refresh)();
        self.refresh();

        let Some(overlay) = self.toast_overlay.upgrade() else {
            self.kept_downloads.borrow_mut().add(subscription_id, paths);
            if let Err(error) =
                podcasts::store::commit_remove_subscription(&self.conn, subscription_id)
            {
                self.show_error(&error.to_string());
            }
            return;
        };
        let toast =
            crate::ui::toasts::plain(&strings::podcast_unsubscribe_from(&subscription.title));
        toast.set_button_label(Some(&strings::text(strings::PODCAST_UNDO)));
        toast.set_timeout(10);
        toast.set_priority(adw::ToastPriority::High);
        let undone = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(self);
        let undo_flag = undone.clone();
        toast.connect_button_clicked(move |_| {
            undo_flag.set(true);
            if let Some(view) = weak.upgrade() {
                if let Err(error) =
                    podcasts::store::undo_remove_subscription(&view.conn, subscription_id)
                {
                    view.show_error(&error.to_string());
                }
                view.refresh();
                (view.callbacks.on_sidebar_refresh)();
            }
        });
        let weak = Rc::downgrade(self);
        toast.connect_dismissed(move |_| {
            if undone.get() {
                return;
            }
            let Some(view) = weak.upgrade() else {
                return;
            };
            view.kept_downloads
                .borrow_mut()
                .add(subscription_id, paths.clone());
            if let Err(error) =
                podcasts::store::commit_remove_subscription(&view.conn, subscription_id)
            {
                view.show_error(&error.to_string());
                return;
            }
            view.schedule_download_toast();
        });
        overlay.add_toast(toast);
    }

    pub(super) fn remove_episode(self: &Rc<Self>, episode_id: i64) {
        let Ok(Some(episode)) = podcasts::store::episode(&self.conn, episode_id) else {
            return;
        };
        if let Err(error) = podcasts::store::tombstone_episode(
            &self.conn,
            episode_id,
            chrono::Utc::now().timestamp(),
        ) {
            self.show_error(&error.to_string());
            return;
        }
        self.refresh();
        (self.callbacks.on_sidebar_refresh)();

        let Some(overlay) = self.toast_overlay.upgrade() else {
            match podcasts::store::commit_remove_episode(&self.conn, episode_id) {
                Ok(Some(path)) => self
                    .kept_downloads
                    .borrow_mut()
                    .add(episode.subscription_id, vec![path]),
                Ok(None) => {}
                Err(error) => self.show_error(&error.to_string()),
            }
            self.schedule_download_toast();
            return;
        };

        let toast = crate::ui::toasts::plain(&strings::podcast_removed_episode(&episode.title));
        toast.set_button_label(Some(&strings::text(strings::PODCAST_UNDO)));
        toast.set_timeout(10);
        toast.set_priority(adw::ToastPriority::High);
        let undone = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(self);
        let undo_flag = undone.clone();
        toast.connect_button_clicked(move |_| {
            undo_flag.set(true);
            if let Some(view) = weak.upgrade() {
                if let Err(error) = podcasts::store::undo_remove_episode(&view.conn, episode_id) {
                    view.show_error(&error.to_string());
                }
                view.refresh();
                (view.callbacks.on_sidebar_refresh)();
            }
        });
        let weak = Rc::downgrade(self);
        toast.connect_dismissed(move |_| {
            if undone.get() {
                return;
            }
            let Some(view) = weak.upgrade() else {
                return;
            };
            match podcasts::store::commit_remove_episode(&view.conn, episode_id) {
                Ok(Some(path)) => view
                    .kept_downloads
                    .borrow_mut()
                    .add(episode.subscription_id, vec![path]),
                Ok(None) => {}
                Err(error) => {
                    view.show_error(&error.to_string());
                    return;
                }
            }
            view.schedule_download_toast();
        });
        overlay.add_toast(toast);
    }

    pub(super) fn schedule_download_toast(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(view) = weak.upgrade() {
                view.flush_download_toast();
            }
        });
    }

    fn flush_download_toast(&self) {
        let (shows, paths) = self.kept_downloads.borrow_mut().take();
        if paths.is_empty() {
            return;
        }
        let Some(overlay) = self.toast_overlay.upgrade() else {
            return;
        };
        let toast = crate::ui::toasts::plain(&strings::podcast_downloads_kept(shows, paths.len()));
        toast.set_button_label(Some(&strings::text(strings::PODCAST_DELETE_FILES)));
        toast.set_priority(adw::ToastPriority::High);
        toast.connect_button_clicked(move |_| {
            if download_commit_action(true) != DownloadCommitAction::Trash {
                return;
            }
            for path in &paths {
                if let Err(error) =
                    gio::File::for_path(path).trash(None::<&gio::Cancellable>)
                {
                    tracing::warn!(%error, path = %path.display(), "could not trash podcast download");
                }
            }
        });
        overlay.add_toast(toast);
    }
}
