//! Rendering of the Concerts view's current cached and failure state.

use std::rc::Rc;

use chrono::Local;
use gtk4::prelude::*;
use reprise_core::concerts::{self, ConcertFilter};
use reprise_core::connectivity::Connectivity;
use reprise_core::source_error::{FailureAction, FailureSurface};

use super::concerts_empty_state::{
    concerts_empty_state_for, concerts_empty_state_presentation, ConcertsEmptyState,
};
use super::concerts_failure_ui::{concerts_failure_presentation, failure_support, row_is_dimmed};
use super::concerts_search::concerts_matching;
use super::concerts_view_refresh::request_fetch;
use super::concerts_view_state::Shared;
use crate::ui::feed_footer::FeedFooterState;

pub(super) const LIST_PAGE: &str = "list";
pub(super) const STATUS_PAGE: &str = "status";
pub(super) const FAILURE_PAGE: &str = "failure";

pub(super) fn render_cache(shared: &Rc<Shared>) -> Result<(), rusqlite::Error> {
    let today = Local::now().date_naive();
    let conn = &shared.conn;
    let filter = shared.filter_bar.filter();
    let app_location = reprise_core::location::app_location(conn)?;
    let credentials = concerts::config::credentials(conn)?;
    let similar_enabled = concerts::config::similar_config(conn)?.enabled;
    let has_similar_rows = concerts::has_similar_events(conn)?;
    let query = shared.filter_bar.query();
    // FIL-1d: the query narrows what the facets returned, matching artist and
    // venue — the two fields the chip names.
    let rows = concerts_matching(
        concerts::query_events(conn, &filter, app_location.as_ref(), today)?,
        &query,
    );
    let facets_restrict = filter.country.is_some()
        || filter.horizon != reprise_core::concerts::DateHorizon::AllUpcoming
        || (app_location.is_some() && filter.radius_km.is_some());
    let restricted = filter != ConcertFilter::default() || !query.is_empty();
    let total = if restricted {
        concerts::count_upcoming(
            conn,
            &ConcertFilter::default(),
            app_location.as_ref(),
            today,
        )? as usize
    } else {
        rows.len()
    };
    let latest_fetch = concerts::latest_fetch_at(conn)?;
    let never_fetched = latest_fetch.is_none();
    shared
        .filter_bar
        .set_context(app_location.as_ref(), similar_enabled, has_similar_rows);
    shared.filter_bar.set_counts(rows.len(), total);
    shared.location_columns.apply(app_location.is_some());
    if app_location.is_some() {
        shared.location_banner.hide();
    } else {
        shared.location_banner.show(total);
    }
    shared
        .end_of_results
        .update(super::concerts_end_of_results::Input {
            shown: rows.len(),
            total,
            query,
            facets_restrict,
            radius_km: app_location.as_ref().and(filter.radius_km),
            city: app_location.map(|location| location.name),
        });
    shared.rows.replace(rows.clone());
    shared.model.replace(rows.clone());
    shared.cached_items.set(total);
    let state = concerts_empty_state_for(
        rows.len(),
        restricted,
        !credentials.is_empty(),
        never_fetched,
    );
    apply_empty_state(shared, state, total);
    apply_footer(shared, latest_fetch);
    render_current_failure(shared);
    Ok(())
}

fn apply_empty_state(shared: &Shared, state: ConcertsEmptyState, total: usize) {
    shared.empty_state.set(state);
    if state == ConcertsEmptyState::List {
        shared.stack.set_visible_child_name(LIST_PAGE);
        return;
    }

    let presentation = concerts_empty_state_presentation(state, total);
    shared.status.set_icon_name(Some(presentation.icon));
    shared.status.set_title(&presentation.title);
    shared
        .status
        .set_description(Some(&presentation.description));
    shared
        .status_button
        .set_visible(presentation.action.is_some());
    if let Some(action) = presentation.action {
        shared.status_button.set_label(&action);
    }
    shared.stack.set_visible_child_name(STATUS_PAGE);
}

fn render_current_failure(shared: &Rc<Shared>) {
    let Some(failure) = shared.fetch_failure.borrow().clone() else {
        shared.error_banner.hide();
        return;
    };
    let cached_items = shared.cached_items.get();
    let presentation = concerts_failure_presentation(&failure, cached_items);
    let support = failure_support(&failure, cached_items);
    let error = failure.source_error().clone();
    let occurred_at = shared.failure_occurred_at.borrow().clone();
    let weak = Rc::downgrade(shared);
    let dismiss_weak = weak.clone();
    let on_action = move |action| {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        match action {
            FailureAction::TryAgain => request_fetch(&shared, true),
            FailureAction::OpenPreferences => {
                let callback = shared.on_open_preferences.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            }
            FailureAction::CheckSubscription
            | FailureAction::Unsubscribe
            | FailureAction::FindNewUrl => {}
        }
    };
    let on_dismiss = move || {
        let Some(shared) = dismiss_weak.upgrade() else {
            return;
        };
        shared.fetch_failure.replace(None);
        shared.error_banner.hide();
    };
    match presentation.surface {
        FailureSurface::Banner => {
            shared.error_banner.show(
                &presentation,
                &support,
                &error,
                &occurred_at,
                on_action,
                on_dismiss,
            );
        }
        FailureSurface::FullArea => {
            shared.error_banner.hide();
            shared
                .failure_state
                .show(&presentation, &support, &error, &occurred_at, on_action);
            shared.stack.set_visible_child_name(FAILURE_PAGE);
        }
    }
}

pub(super) fn apply_row_connectivity(shared: &Shared) {
    shared
        .column_view
        .set_opacity(if row_is_dimmed(shared.connectivity.get()) {
            0.55
        } else {
            1.0
        });
}

pub(super) fn apply_footer(shared: &Shared, latest_fetch: Option<i64>) {
    let module_enabled =
        reprise_core::modules::is_enabled(&shared.conn, &reprise_core::modules::CONCERTS_MODULE)
            .unwrap_or(false);
    let network_enabled = reprise_core::online_sources::is_enabled(&shared.conn).unwrap_or(false);
    let has_credentials = concerts::config::credentials(&shared.conn)
        .is_ok_and(|credentials| !credentials.is_empty());
    let state = if !module_enabled {
        FeedFooterState::ModuleOff
    } else if !network_enabled {
        FeedFooterState::NetworkOff
    } else if !has_credentials {
        FeedFooterState::NoCredentials
    } else if shared.fetching.get() {
        FeedFooterState::Fetching {
            checked: 0,
            total: 0,
        }
    } else if shared.connectivity.get() == Connectivity::Offline {
        latest_fetch.map_or(FeedFooterState::NeverFetched, |latest| {
            FeedFooterState::Offline { latest }
        })
    } else if shared.fetch_failure.borrow().is_some() {
        latest_fetch.map_or(FeedFooterState::NeverFetched, |latest| {
            FeedFooterState::Failed { latest }
        })
    } else if let Some(at) = latest_fetch {
        if shared.loaded_this_visit.get() {
            FeedFooterState::Loaded { at }
        } else {
            FeedFooterState::Cached { at }
        }
    } else {
        FeedFooterState::NeverFetched
    };
    shared.footer.apply(state);
}
