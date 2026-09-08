//! Fetch and periodic-refresh lifecycle for the Concerts view.

use std::rc::Rc;

use reprise_core::concerts::{self, ConcertFailure};
use reprise_core::source_error::{SourceError, SourceErrorKind};

use super::concerts_view_render::{apply_footer, render_cache};
use super::concerts_view_state::Shared;
use super::concerts_worker::{request_allowed, ConcertsRequest, ConcertsResponse};
use crate::ui::feed_footer::FeedFooterState;

// How often the refresh *decision* is re-evaluated, not how often we fetch —
// `concerts::refresh_due` owns that and needs `FETCH_TTL_SECONDS` (1 h) plus a
// per-install jitter of up to 10 min to have passed. This tick must therefore
// stay well below the TTL: at an hourly tick, the one right after a refresh has
// elapsed exactly 1 h, loses to the jitter, and the next chance is an hour later —
// which turns the hourly refresh into a stable two-hour one for every install
// whose jitter is non-zero. A tick that finds nothing due costs one
// `SELECT MAX(last_attempt_at)`, so ticking often is cheap.
const REFRESH_TIMER_SECONDS: u32 = 10 * 60;

pub(super) fn maybe_background_refresh(shared: &Rc<Shared>) {
    let latest = concerts::latest_fetch_at(&shared.conn).ok().flatten();
    let due = concerts::refresh_due(
        latest,
        chrono::Utc::now().timestamp(),
        shared.runtime.jitter_seconds(),
    );
    if request_allowed(shared.runtime.enabled.get(), shared.fetching.get(), due) {
        request_fetch(shared, false);
    }
}

pub(super) fn request_fetch(shared: &Rc<Shared>, force: bool) {
    let has_credentials = {
        let conn = &shared.conn;
        concerts::config::credentials(conn).is_ok_and(|credentials| !credentials.is_empty())
    };
    if !has_credentials
        || !request_allowed(shared.runtime.enabled.get(), shared.fetching.get(), true)
    {
        return;
    }
    if shared.fetching.replace(true) {
        return;
    }
    apply_footer(
        shared,
        concerts::latest_fetch_at(&shared.conn).ok().flatten(),
    );

    let generation = shared.generation.get().wrapping_add(1);
    shared.generation.set(generation);
    let (sender, receiver) = async_channel::bounded(1);
    let (progress_sender, progress_receiver) = async_channel::unbounded();
    if !shared.runtime.request_with_progress(
        ConcertsRequest {
            generation,
            force,
            response: sender,
        },
        progress_sender,
    ) {
        finish_fetch(
            shared,
            Some(ConcertFailure::Source(SourceError::new(
                SourceErrorKind::Unreachable,
                "Queue Concerts refresh",
                "Concerts worker refused the refresh request",
            ))),
        );
        return;
    }
    let progress_weak = Rc::downgrade(shared);
    gtk4::glib::spawn_future_local(async move {
        while let Ok(progress) = progress_receiver.recv().await {
            let Some(shared) = progress_weak.upgrade() else {
                return;
            };
            if !shared.fetching.get() || shared.generation.get() != generation {
                return;
            }
            shared.footer.apply(FeedFooterState::Fetching {
                checked: progress.checked,
                total: progress.total,
            });
        }
    });
    let weak = Rc::downgrade(shared);
    gtk4::glib::spawn_future_local(async move {
        let response = receiver.recv().await;
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let failure = match response {
            Ok(ConcertsResponse {
                generation: response_generation,
                result,
            }) if response_generation == shared.generation.get() => match result {
                Ok(summary) => summary.failures.into_iter().next(),
                Err(error) => {
                    tracing::warn!(%error, "could not refresh Concerts");
                    Some(error.into_source_failure())
                }
            },
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(%error, "Concerts worker closed without a result");
                Some(ConcertFailure::Source(SourceError::new(
                    SourceErrorKind::Unreachable,
                    "Refresh Concerts",
                    error.to_string(),
                )))
            }
        };
        finish_fetch(&shared, failure);
    });
}

fn finish_fetch(shared: &Rc<Shared>, failure: Option<ConcertFailure>) {
    shared.fetching.set(false);
    if failure.is_none() {
        shared.loaded_this_visit.set(true);
    }
    shared.fetch_failure.replace(failure);
    if shared.fetch_failure.borrow().is_some() {
        *shared.failure_occurred_at.borrow_mut() = chrono::Utc::now().to_rfc3339();
    }
    if let Err(error) = render_cache(shared) {
        tracing::warn!(%error, "could not reload Concerts after fetch");
    }
    let callback = shared.on_refreshed.borrow().clone();
    if let Some(callback) = callback {
        callback();
    }
}

pub(super) fn enabled_changed(shared: &Rc<Shared>, enabled: bool) {
    if enabled {
        start_refresh_timer(shared);
    } else {
        stop_refresh_timer(shared);
    }
    if let Err(error) = render_cache(shared) {
        tracing::warn!(%error, "could not apply Concerts module state");
    }
}

fn start_refresh_timer(shared: &Rc<Shared>) {
    let existing = shared.refresh_timer.take();
    if existing.is_some() {
        shared.refresh_timer.set(existing);
        return;
    }
    let weak = Rc::downgrade(shared);
    let source = gtk4::glib::timeout_add_seconds_local(REFRESH_TIMER_SECONDS, move || {
        let Some(shared) = weak.upgrade() else {
            return gtk4::glib::ControlFlow::Break;
        };
        maybe_background_refresh(&shared);
        gtk4::glib::ControlFlow::Continue
    });
    shared.refresh_timer.set(Some(source));
}

fn stop_refresh_timer(shared: &Shared) {
    if let Some(source) = shared.refresh_timer.take() {
        source.remove();
    }
}
