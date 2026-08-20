//! Worker-backed refresh, YouTube window, and queued-download requests.

use super::super::podcasts_footer::{REFRESH_LABEL_PAGE, REFRESH_SPINNER_PAGE};
use super::super::podcasts_refresh_decision::{
    refresh_button_state, scope_status, tab_open_refresh_allowed, RefreshButtonState,
    RefreshWindow, TAB_OPEN_STALE_SECONDS,
};
use super::*;

struct RefreshFeedbackGuard(std::rc::Weak<PodcastsView>);

impl Drop for RefreshFeedbackGuard {
    fn drop(&mut self) {
        if let Some(view) = self.0.upgrade() {
            view.end_refresh_feedback();
        }
    }
}

impl PodcastsView {
    pub(in crate::ui) fn request_refresh(self: &Rc<Self>, force: bool) -> bool {
        let policy = if force {
            podcasts::refresh::RefreshPolicy::Force
        } else {
            podcasts::refresh::RefreshPolicy::Due
        };
        self.request_refresh_with(podcasts::refresh::RefreshRequest { policy, kind: None })
    }

    fn request_refresh_with(self: &Rc<Self>, request: podcasts::refresh::RefreshRequest) -> bool {
        let operation = PodcastsOperation::Refresh {
            policy: request.policy,
            kind: request.kind,
        };
        let generation = request_generation(self.generation.get(), operation);
        self.generation.set(generation);
        let (response, receiver) = podcasts_response_channel();
        let queued = self.runtime.request(PodcastsRequest {
            generation,
            operation,
            response,
        });
        if !queued {
            return false;
        }
        self.begin_refresh_feedback();
        self.footer_spinner.start();
        self.footer_status
            .set_text(&strings::text(strings::PODCAST_REFRESHING));
        let weak = Rc::downgrade(self);
        let feedback_guard = RefreshFeedbackGuard(weak.clone());
        glib::spawn_future_local(async move {
            let _feedback_guard = feedback_guard;
            while let Ok(response) = receiver.recv().await {
                let Some(view) = weak.upgrade() else {
                    return;
                };
                if view.generation.get() != response.generation {
                    return;
                }
                match response.result {
                    Ok(PodcastsWorkerResult::DownloadState { episode_id, state }) => {
                        let known = {
                            let rows = view.rows.borrow();
                            rows.iter().any(|row| row.id == episode_id)
                        };
                        if !known {
                            view.refresh();
                            view.footer_status
                                .set_text(&strings::text(strings::PODCAST_REFRESHING));
                        }
                        view.set_download_state(episode_id, &state);
                    }
                    Ok(PodcastsWorkerResult::Refreshed(summary)) => {
                        view.footer_spinner.stop();
                        view.refresh();
                        if summary.failures.is_empty() {
                            view.clear_fetch_failure();
                        } else {
                            view.show_refresh_failures(&summary.failures);
                        }
                        (view.callbacks.on_sidebar_refresh)();
                        view.request_fill_downloads();
                        break;
                    }
                    Ok(PodcastsWorkerResult::LoadedMore {
                        subscription_id,
                        end,
                    }) => {
                        view.footer_spinner.stop();
                        view.youtube_detail.set_loaded_limit(subscription_id, end);
                        view.refresh();
                        break;
                    }
                    Ok(PodcastsWorkerResult::Filled(_)) => {}
                    Err(error) => {
                        view.footer_spinner.stop();
                        view.refresh();
                        tracing::warn!(%error, "podcast refresh failed");
                        view.show_unclassified_refresh_failure(error);
                        break;
                    }
                }
            }
        });
        true
    }

    fn request_fill_downloads(self: &Rc<Self>) -> bool {
        if self.fill_in_flight.replace(true) {
            return false;
        }
        let operation = PodcastsOperation::FillDownloads;
        let generation = request_generation(self.generation.get(), operation);
        let (response, receiver) = podcasts_response_channel();
        if !self.runtime.request(PodcastsRequest {
            generation,
            operation,
            response,
        }) {
            self.fill_in_flight.set(false);
            return false;
        }
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(response) = receiver.recv().await {
                let Some(view) = weak.upgrade() else {
                    return;
                };
                match response.result {
                    Ok(PodcastsWorkerResult::DownloadState { episode_id, state }) => {
                        view.set_download_state(episode_id, &state);
                    }
                    Ok(PodcastsWorkerResult::Filled(summary)) => {
                        view.fill_in_flight.set(false);
                        view.refresh();
                        (view.callbacks.on_sidebar_refresh)();
                        tracing::debug!(
                            downloaded = summary.downloaded,
                            failed = summary.failed,
                            "podcast download fill-up finished"
                        );
                        break;
                    }
                    Ok(
                        PodcastsWorkerResult::Refreshed(_)
                        | PodcastsWorkerResult::LoadedMore { .. },
                    ) => {}
                    Err(error) => {
                        view.fill_in_flight.set(false);
                        tracing::warn!(%error, "podcast download fill-up failed");
                        break;
                    }
                }
            }
        });
        true
    }

    pub(in crate::ui) fn request_tab_open_refresh(self: &Rc<Self>) -> bool {
        let network_allowed = match podcasts::config::source_network_allowed(&self.conn, self.kind)
        {
            Ok(network_allowed) => network_allowed,
            Err(error) => {
                tracing::warn!(%error, "could not inspect podcast network permission for a tab-open refresh");
                return false;
            }
        };
        let metered = gio::NetworkMonitor::default().is_network_metered();
        let subscriptions = match podcasts::store::active_subscriptions(&self.conn) {
            Ok(subscriptions) => subscriptions,
            Err(error) => {
                tracing::warn!(%error, "could not inspect podcast subscriptions for a tab-open refresh");
                return false;
            }
        };
        let status = scope_status(
            &subscriptions,
            Some(self.kind),
            RefreshWindow::Seconds(TAB_OPEN_STALE_SECONDS),
            chrono::Utc::now().timestamp(),
        );
        if !tab_open_refresh_allowed(
            network_allowed,
            self.connectivity.get(),
            metered,
            self.refresh_in_flight.get() > 0,
            status,
        ) {
            return false;
        }
        self.request_refresh_with(podcasts::refresh::RefreshRequest {
            policy: podcasts::refresh::RefreshPolicy::StaleFor {
                seconds: TAB_OPEN_STALE_SECONDS,
            },
            kind: Some(self.kind),
        })
    }

    pub(super) fn begin_refresh_feedback(&self) {
        self.refresh_in_flight
            .set(self.refresh_in_flight.get().saturating_add(1));
        self.apply_refresh_button_state();
    }

    pub(super) fn end_refresh_feedback(&self) {
        self.refresh_in_flight
            .set(self.refresh_in_flight.get().saturating_sub(1));
        self.apply_refresh_button_state();
    }

    fn apply_refresh_button_state(&self) {
        match refresh_button_state(self.refresh_in_flight.get()) {
            RefreshButtonState::Busy => {
                self.refresh_spinner.start();
                self.refresh_stack
                    .set_visible_child_name(REFRESH_SPINNER_PAGE);
                self.refresh_button.set_sensitive(false);
            }
            RefreshButtonState::Idle => {
                self.refresh_stack
                    .set_visible_child_name(REFRESH_LABEL_PAGE);
                self.refresh_spinner.stop();
                self.refresh_button.set_sensitive(true);
            }
        }
    }

    pub(super) fn request_load_more(self: &Rc<Self>, subscription_id: i64, end: usize) -> bool {
        if self.connectivity.get() == Connectivity::Offline {
            self.deferred_actions
                .borrow_mut()
                .push(DeferredAction::LoadMore {
                    subscription_id,
                    end,
                });
            self.footer_status
                .set_text(&strings::text(strings::PODCAST_QUEUED_OFFLINE));
            return true;
        }
        let operation = PodcastsOperation::LoadMore {
            subscription_id,
            end,
        };
        let generation = request_generation(self.generation.get(), operation);
        self.generation.set(generation);
        let (response, receiver) = podcasts_response_channel();
        if !self.runtime.request(PodcastsRequest {
            generation,
            operation,
            response,
        }) {
            return false;
        }
        self.footer_spinner.start();
        self.footer_status
            .set_text(&strings::text(strings::YOUTUBE_LOADING_MORE));
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(response) = receiver.recv().await {
                let Some(view) = weak.upgrade() else {
                    return;
                };
                if view.generation.get() != response.generation {
                    return;
                }
                match response.result {
                    Ok(PodcastsWorkerResult::LoadedMore {
                        subscription_id,
                        end,
                    }) => {
                        view.footer_spinner.stop();
                        view.youtube_detail.set_loaded_limit(subscription_id, end);
                        view.refresh();
                        break;
                    }
                    Ok(PodcastsWorkerResult::DownloadState { episode_id, state }) => {
                        view.set_download_state(episode_id, &state);
                    }
                    Ok(PodcastsWorkerResult::Refreshed(_)) => {}
                    Ok(PodcastsWorkerResult::Filled(_)) => {}
                    Err(error) => {
                        view.footer_spinner.stop();
                        view.refresh();
                        view.show_error(&error);
                        break;
                    }
                }
            }
        });
        true
    }
}
