//! Worker-backed refresh, YouTube window, and queued-download requests.

use super::*;

impl PodcastsView {
    /// `NET-3c`: dispatches the queued-download runner. Only ever called
    /// from `set_connectivity`'s `Offline` → `Online` transition — this
    /// does not itself decide connectivity, it only wires the request and
    /// applies whatever the worker reports back, the same shape as
    /// `request_refresh` and `request_load_more` below.
    pub(super) fn request_run_queued(self: &Rc<Self>) -> bool {
        let operation = PodcastsOperation::RunQueued;
        let generation = request_generation(self.generation.get(), operation);
        let (response, receiver) = podcasts_response_channel();
        if !self.runtime.request(PodcastsRequest {
            generation,
            operation,
            priority: PodcastsPriority::Normal,
            response,
        }) {
            return false;
        }
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
                    Ok(PodcastsWorkerResult::DownloadState { episode_id, state }) => {
                        view.set_download_state(episode_id, &state);
                        if matches!(state, DownloadState::Downloaded { .. }) {
                            view.refresh();
                        }
                    }
                    Ok(PodcastsWorkerResult::QueueRunComplete { .. }) => break,
                    Ok(
                        PodcastsWorkerResult::Refreshed(_)
                        | PodcastsWorkerResult::LoadedMore { .. },
                    ) => {}
                    Err(error) => {
                        view.show_error(&error);
                        break;
                    }
                }
            }
        });
        true
    }

    pub(in crate::ui) fn request_refresh(self: &Rc<Self>, force: bool) -> bool {
        let operation = PodcastsOperation::Refresh { force };
        let generation = request_generation(self.generation.get(), operation);
        self.generation.set(generation);
        let (response, receiver) = podcasts_response_channel();
        let queued = self.runtime.request(PodcastsRequest {
            generation,
            operation,
            priority: PodcastsPriority::Normal,
            response,
        });
        if !queued {
            return false;
        }
        self.footer_spinner.start();
        self.footer_status
            .set_text(&strings::text(strings::PODCAST_REFRESHING));
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
                    Ok(PodcastsWorkerResult::Refreshed(_)) => {
                        view.footer_spinner.stop();
                        view.refresh();
                        (view.callbacks.on_sidebar_refresh)();
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
                    // `NET-3c`: this request's own response channel never
                    // actually carries this variant — only a `RunQueued`
                    // request's dedicated channel does — but the match must
                    // stay exhaustive over the one shared result enum.
                    Ok(PodcastsWorkerResult::QueueRunComplete { .. }) => {}
                    Err(error) => {
                        view.footer_spinner.stop();
                        view.refresh();
                        // `POD-16`: the same footer, the same rule. The worker
                        // hands back a plain string that may be a `DbError`'s
                        // whole failing statement, so it is logged rather than
                        // appended — the sentence already says what happened
                        // and that the saved episodes are still shown.
                        tracing::warn!(%error, "podcast refresh failed");
                        view.footer_status
                            .set_text(&strings::text(strings::PODCAST_REFRESH_FAILED));
                        break;
                    }
                }
            }
        });
        true
    }

    pub(super) fn request_load_more(self: &Rc<Self>, subscription_id: i64, end: usize) -> bool {
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
            priority: PodcastsPriority::Normal,
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
                    Ok(
                        PodcastsWorkerResult::Refreshed(_)
                        | PodcastsWorkerResult::QueueRunComplete { .. },
                    ) => {}
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
