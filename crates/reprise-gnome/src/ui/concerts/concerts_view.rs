#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::concerts;
use reprise_core::connectivity::Connectivity;
use reprise_core::db::Db;

use super::concerts_columns::{self, OnOpenTarget};
use super::concerts_empty_state::ConcertsEmptyState;
use super::concerts_failure_ui::update_failure_for_connectivity;
use super::concerts_filter_bar::ConcertsFilterBar;
use super::concerts_location_banner::ConcertsLocationBanner;
use super::concerts_location_columns::LocationColumns;
use super::concerts_model::ConcertsModel;
use super::concerts_sorting::wire_sorting;
use super::concerts_view_refresh::{enabled_changed, maybe_background_refresh, request_fetch};
use super::concerts_view_render::{apply_row_connectivity, render_cache};
use super::concerts_view_state::Shared;
use super::concerts_worker::ConcertsRuntime;
use crate::ui::external_link::{self, LaunchErrorSlot};
use crate::ui::feed_footer::FeedFooter;
use crate::ui::location_broadcast::LocationBroadcast;
use crate::ui::source_empty_state::SourceFailureState;
use crate::ui::source_error_banner::SourceErrorBanner;

use super::concerts_view_render::{FAILURE_PAGE, LIST_PAGE, STATUS_PAGE};

#[cfg(test)]
use crate::ui::feed_footer::FeedFooterState;
#[cfg(test)]
use reprise_core::concerts::ConcertRow;
fn notify_filter_changed(runtime: &ConcertsRuntime) {
    runtime.notify_settings_changed();
}

pub(in crate::ui) struct ConcertsView {
    root: gtk4::Widget,
    shared: Rc<Shared>,
    artist_image: Rc<super::concerts_artist_cover::ConcertsArtistImage>,
}

impl ConcertsView {
    pub(in crate::ui) fn new(
        conn: Rc<Db>,
        runtime: &Rc<ConcertsRuntime>,
        location_broadcast: &Rc<LocationBroadcast>,
    ) -> Self {
        let model = Rc::new(ConcertsModel::new());
        let filter_bar = ConcertsFilterBar::new(conn.clone());
        let column_view = gtk4::ColumnView::builder()
            .model(model.selection())
            .show_row_separators(false)
            .show_column_separators(false)
            .build();
        column_view.add_css_class("reprise-concerts-table");
        crate::ui::table_columns::single_sort_indicator::mark(&column_view);

        let launch_error: LaunchErrorSlot = Rc::new(RefCell::new(None));
        let launch_error_for_open = launch_error.clone();
        let on_open: OnOpenTarget = Rc::new(move |target| {
            external_link::launch(&target, "concert", Some(&launch_error_for_open));
        });
        let query_source: crate::ui::search_highlight::QuerySource = {
            let filter_bar = filter_bar.clone();
            Rc::new(move || filter_bar.query())
        };
        let radius_source: super::concerts_status_cells::RadiusSource = {
            let filter_bar = filter_bar.clone();
            Rc::new(move || filter_bar.filter().radius_km)
        };
        #[cfg(not(test))]
        let artist_image = super::concerts_artist_cover::ConcertsArtistImage::new();
        #[cfg(test)]
        let artist_image = super::concerts_artist_cover::ConcertsArtistImage::for_test(|_| None);
        let columns = concerts_columns::append_columns(
            &column_view,
            &query_source,
            &radius_source,
            &artist_image,
        );
        let column_registry = super::concerts_column_layout::registry(&column_view, conn.clone());
        let (location_columns, column_model) =
            LocationColumns::new(column_registry, &column_view, columns);
        crate::ui::table_columns::header_popover::install_header_popover(
            &column_view,
            &column_model,
        );
        crate::ui::table_columns::header_dnd::install_header_drag(&column_view, &column_model);

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&column_view)
            .vexpand(true)
            .hexpand(true)
            .build();
        let list_overlay = gtk4::Overlay::new();
        list_overlay.set_child(Some(&scrolled));
        let end_of_results = super::concerts_end_of_results::ConcertsEndOfResults::install(
            &list_overlay,
            &scrolled,
            &column_view,
        );
        {
            let filter_bar = filter_bar.clone();
            end_of_results.connect_recover(move || filter_bar.clear_all());
        }
        let status = adw::StatusPage::builder().vexpand(true).build();
        let status_button = gtk4::Button::new();
        status_button.add_css_class("pill");
        status_button.set_halign(gtk4::Align::Center);
        status.set_child(Some(&status_button));
        let stack = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        stack.add_named(&list_overlay, Some(LIST_PAGE));
        stack.add_named(&status, Some(STATUS_PAGE));
        let failure_state = SourceFailureState::new("x-office-calendar-symbolic");
        stack.add_named(failure_state.widget(), Some(FAILURE_PAGE));

        let footer = FeedFooter::new();
        let error_banner = SourceErrorBanner::new();
        let location_banner = ConcertsLocationBanner::new();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-concerts-view");
        root.append(filter_bar.widget());
        root.append(location_banner.widget());
        root.append(error_banner.widget());
        root.append(&stack);
        root.append(footer.widget());

        let shared = Rc::new(Shared {
            conn,
            runtime: runtime.clone(),
            model,
            filter_bar: filter_bar.clone(),
            end_of_results,
            rows: RefCell::new(Vec::new()),
            cached_items: Cell::new(0),
            column_view: column_view.clone(),
            column_model,
            location_columns,
            stack,
            status,
            status_button: status_button.clone(),
            footer,
            error_banner,
            location_banner,
            failure_state,
            fetch_failure: RefCell::new(None),
            failure_occurred_at: RefCell::new(String::new()),
            connectivity: Cell::new(Connectivity::Online),
            fetching: Cell::new(false),
            loaded_this_visit: Cell::new(false),
            generation: Cell::new(0),
            refresh_timer: Cell::new(None),
            empty_state: Cell::new(ConcertsEmptyState::NeverFetched),
            on_clear_filters: RefCell::new(None),
            on_refreshed: RefCell::new(None),
            on_open_preferences: RefCell::new(None),
            on_launch_error: launch_error,
        });
        {
            let shared = Rc::downgrade(&shared);
            filter_bar.set_on_changed(move |_| {
                if let Some(shared) = shared.upgrade() {
                    notify_filter_changed(&shared.runtime);
                }
            });
        }
        {
            let shared = Rc::downgrade(&shared);
            filter_bar.set_on_open_location(move || {
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                let callback = shared.on_open_preferences.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
        {
            let weak = Rc::downgrade(&shared);
            shared.location_banner.set_on_open_location(move || {
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                let callback = shared.on_open_preferences.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
        {
            let filter_bar = filter_bar.clone();
            *shared.on_clear_filters.borrow_mut() = Some(Rc::new(move || {
                filter_bar.clear_all();
            }));
        }
        {
            let shared_weak = Rc::downgrade(&shared);
            shared.footer.connect_reload(move || {
                if let Some(shared) = shared_weak.upgrade() {
                    request_fetch(&shared, true);
                }
            });
        }

        {
            let shared = shared.clone();
            status_button.connect_clicked(move |_| {
                let callback = match shared.empty_state.get() {
                    ConcertsEmptyState::NoCredentials => None,
                    ConcertsEmptyState::NoResults => shared.on_clear_filters.borrow().clone(),
                    ConcertsEmptyState::NeverFetched
                    | ConcertsEmptyState::Empty
                    | ConcertsEmptyState::List => None,
                };
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
        super::concerts_activation::wire(&column_view, &shared.model, on_open);
        {
            let root = root.downgrade();
            let shared = Rc::downgrade(&shared);
            runtime.subscribe_enabled(
                move || root.upgrade().is_some(),
                move |enabled| {
                    if let Some(shared) = shared.upgrade() {
                        enabled_changed(&shared, enabled);
                    }
                },
            );
        }
        {
            let root = root.downgrade();
            let shared = Rc::downgrade(&shared);
            location_broadcast.subscribe(
                move || root.upgrade().is_some(),
                move || {
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    if let Err(error) = shared.filter_bar.reload_persisted() {
                        tracing::warn!(%error, "could not reload app location settings");
                        return;
                    }
                    if let Err(error) = render_cache(&shared) {
                        tracing::warn!(%error, "could not apply app location settings");
                        return;
                    }
                    let callback = shared.on_refreshed.borrow().clone();
                    if let Some(callback) = callback {
                        callback();
                    }
                },
            );
        }
        {
            let root = root.downgrade();
            let shared = Rc::downgrade(&shared);
            runtime.subscribe_settings(
                move || root.upgrade().is_some(),
                move || {
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    if let Err(error) = shared.filter_bar.reload_persisted() {
                        tracing::warn!(%error, "could not reload Concerts settings");
                        return;
                    }
                    if let Err(error) = render_cache(&shared) {
                        tracing::warn!(%error, "could not apply Concerts settings");
                        return;
                    }
                    let callback = shared.on_refreshed.borrow().clone();
                    if let Some(callback) = callback {
                        callback();
                    }
                },
            );
        }
        wire_sorting(&column_view, &shared);
        shared.location_columns.sort_by_date();

        Self {
            root: root.upcast(),
            shared,
            artist_image,
        }
    }

    pub(in crate::ui) fn set_artist_image(
        &self,
        loader: Rc<crate::ui::cover_loader::CoverLoader>,
        runtime: Rc<crate::ui::artist_portrait_worker::ArtistPortraitRuntime>,
    ) {
        self.artist_image.set_sources(loader, runtime);
    }

    pub(in crate::ui) fn root(&self) -> &gtk4::Widget {
        &self.root
    }

    pub(in crate::ui) fn column_model(&self) -> Rc<dyn crate::ui::table_columns::EditorModel> {
        self.shared.column_model.clone()
    }

    /// SEARCH-8a: applies this view's query (FIL-1d: artist and venue).
    pub(in crate::ui) fn set_search_query(&self, query: &str) {
        self.shared.filter_bar.set_query(query);
    }

    pub(in crate::ui) fn set_committed_search_query(&self, query: &str) {
        self.shared.filter_bar.set_committed_query(query);
    }

    /// SEARCH-8a: the bar requests or reports a query transition to the shell.
    pub(in crate::ui) fn set_on_search_query_changed(&self, callback: impl Fn(&str) + 'static) {
        self.shared.filter_bar.set_on_query_changed(callback);
    }

    /// FIL-2a: "Clear all" for this view — its query and its facets.
    pub(in crate::ui) fn clear_all_filters(&self) {
        self.shared.filter_bar.clear_all();
    }

    pub(in crate::ui) fn refresh(&self) {
        self.shared.loaded_this_visit.set(false);
        if let Err(error) = render_cache(&self.shared) {
            tracing::warn!(%error, "could not load concerts view");
        }
        maybe_background_refresh(&self.shared);
    }

    pub(in crate::ui) fn set_on_clear_filters(&self, callback: impl Fn() + 'static) {
        *self.shared.on_clear_filters.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_launch_error(&self, callback: impl Fn(String) + 'static) {
        *self.shared.on_launch_error.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_refreshed(&self, callback: impl Fn() + 'static) {
        *self.shared.on_refreshed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_open_preferences(&self, callback: impl Fn() + 'static) {
        *self.shared.on_open_preferences.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_connectivity(&self, value: Connectivity) {
        self.shared.connectivity.set(value);
        let can_fetch = concerts::config::credentials(&self.shared.conn)
            .is_ok_and(|credentials| !credentials.is_empty());
        let previous = self.shared.fetch_failure.borrow().clone();
        update_failure_for_connectivity(
            &mut self.shared.fetch_failure.borrow_mut(),
            value,
            can_fetch,
        );
        if self.shared.fetch_failure.borrow().as_ref() != previous.as_ref() {
            *self.shared.failure_occurred_at.borrow_mut() = chrono::Utc::now().to_rfc3339();
        }
        apply_row_connectivity(&self.shared);
        if let Err(error) = render_cache(&self.shared) {
            tracing::warn!(%error, "could not apply Concerts connectivity");
        }
    }
}

#[cfg(test)]
#[path = "concerts_view_tests.rs"]
mod tests;
