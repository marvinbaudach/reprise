//! Fetch orchestration for the Updates popover.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::updates::{Feed, FeedRefresh};

use super::{fetch_from_database, NewReleasesPopover};
use crate::ui::concerts::ConcertsRequest;
use crate::ui::one_shot_task;

impl NewReleasesPopover {
    pub(super) fn start_fetch(self: &Rc<Self>, include_concerts: bool) {
        if self.fetching.get() {
            return;
        }
        let news_enabled = reprise_core::modules::is_enabled(
            &self.conn,
            &reprise_core::modules::NEW_RELEASES_MODULE,
        )
        .unwrap_or(false);
        let concerts_enabled = include_concerts
            && reprise_core::modules::is_enabled(
                &self.conn,
                &reprise_core::modules::CONCERTS_MODULE,
            )
            .unwrap_or(false)
            && reprise_core::concerts::config::credentials(&self.conn)
                .is_ok_and(|credentials| !credentials.is_empty());
        let mut feeds = Vec::new();
        if news_enabled {
            feeds.push(Feed::NewReleases);
        }
        if concerts_enabled {
            feeds.push(Feed::Concerts);
        }
        let run = FeedRefresh::start(&feeds);
        if run.is_complete() {
            self.run.replace(run);
            self.render(false, false);
            return;
        }
        self.fetching.set(true);
        self.run.replace(run);
        self.fetch_stack.set_visible_child_name("spinner");
        self.spinner.start();
        self.fetch_button.set_sensitive(false);
        self.failure.set_visible(false);
        self.render(false, false);

        if news_enabled {
            self.start_news_fetch();
        }
        if concerts_enabled {
            self.start_concerts_fetch();
        }
    }

    fn start_news_fetch(self: &Rc<Self>) {
        let database_path = self.database_path.clone();
        let result = one_shot_task::spawn("reprise-new-releases", move || {
            fetch_from_database(&database_path)
        });
        let Ok(receiver) = result else {
            self.finish_feed(Feed::NewReleases, true);
            return;
        };
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let failed = match receiver.recv().await {
                Ok(Ok(report)) => report.failed > 0,
                Ok(Err(error)) => {
                    tracing::warn!(%error, "could not refresh New Releases");
                    true
                }
                Err(error) => {
                    tracing::warn!(%error, "New Releases worker closed without a result");
                    true
                }
            };
            if let Some(state) = weak.upgrade() {
                state.finish_feed(Feed::NewReleases, failed);
            }
        });
    }

    fn start_concerts_fetch(self: &Rc<Self>) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let (sender, receiver) = async_channel::bounded(1);
        if !self.concerts_runtime.request(ConcertsRequest {
            generation,
            force: true,
            response: sender,
        }) {
            self.finish_feed(Feed::Concerts, true);
            return;
        }
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let failed = match receiver.recv().await {
                Ok(response) if response.generation == generation => match response.result {
                    Ok(summary) => summary.failed > 0,
                    Err(error) => {
                        tracing::warn!(%error, "could not refresh Concerts from Updates");
                        true
                    }
                },
                Ok(_) => true,
                Err(error) => {
                    tracing::warn!(%error, "Concerts worker closed without an Updates result");
                    true
                }
            };
            if let Some(state) = weak.upgrade() {
                state.finish_feed(Feed::Concerts, failed);
            }
        });
    }

    fn finish_feed(self: &Rc<Self>, feed: Feed, failed: bool) {
        if matches!(feed, Feed::NewReleases) && !failed {
            let result = {
                let conn = &self.conn;
                reprise_core::library::settings::set_new_releases_fetch_completed(conn, true)
            };
            if let Err(error) = result {
                tracing::warn!(%error, "could not save New Releases fetch state");
            }
        }
        let (complete, news_failed) = {
            let mut run = self.run.borrow_mut();
            run.finish(feed, failed);
            (run.is_complete(), run.has_failed(Feed::NewReleases))
        };
        if !complete {
            return;
        }
        self.fetching.set(false);
        self.spinner.stop();
        self.fetch_stack.set_visible_child_name("icon");
        self.fetch_button.set_sensitive(true);
        self.render(false, news_failed);
    }
}
