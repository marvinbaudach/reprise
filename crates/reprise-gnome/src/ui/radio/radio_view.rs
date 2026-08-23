use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::connectivity::{self, Connectivity};
use reprise_core::db::Db;
use reprise_core::playback::PlaybackState;
use reprise_core::radio::{self, StationRow};
use reprise_core::source_error::{
    source_failure_presentation, SourceError, SourceErrorKind, SourceSurface,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::add_dialog::RadioAddDialog;
use super::radio_columns::{self, LiveState};
use super::radio_context_menu;
use super::radio_empty_state::{radio_empty_state_for, RadioEmptyState};
use super::radio_filter_bar::{filter_rows, RadioFilterBar};
use super::radio_model::{RadioModel, RadioObject};
use super::radio_presentation::{sort_rows, RadioLiveState};
use crate::ui::playback::external_media::{ExternalMedia, RadioPhase};
use crate::ui::playback::player_controller::PlayerController;
use crate::ui::sidebar::sidebar_presentation::NavIcon;
use crate::ui::source_empty_state::{SourceEmptyState, SourceEmptyStateCopy};
use crate::ui::source_error_banner::SourceErrorBanner;
use crate::ui::source_reveal::LoadedItemChange;
use crate::ui::strings;
use crate::ui::style::buttons;

#[path = "radio_failure_ui.rs"]
mod failure_ui;
#[path = "radio_view_reveal_request.rs"]
mod reveal_request;
use failure_ui::{
    radio_failure_action, reresolve_station_url, should_clear_radio_failure,
    should_show_offline_radio_notice, RadioFailureAction,
};

const LIST_PAGE: &str = "list";
const STATUS_PAGE: &str = "status";
/// `SRC-10`: the stack page holding the shared "nothing added yet" empty
/// state — distinct from `STATUS_PAGE`, which still carries `NoResults`
/// (Block B2, unchanged).
const EMPTY_PAGE: &str = "empty";
const ACTION_OPEN_ADD: &str = "open-add";

type IdCallback = Rc<dyn Fn(i64)>;
type Callback = Rc<dyn Fn()>;

pub(super) struct Shared {
    conn: Rc<Db>,
    controller: std::rc::Weak<PlayerController>,
    model: Rc<RadioModel>,
    column_model: Rc<dyn crate::ui::table_columns::EditorModel>,
    pub(super) filter_bar: Rc<RadioFilterBar>,
    end_of_results: Rc<crate::ui::end_of_results::EndOfResults>,
    rows: RefCell<Vec<StationRow>>,
    live: Rc<RefCell<RadioLiveState>>,
    /// `NET-3b`: explicit, injectable connectivity seam (see
    /// `reprise_core::connectivity`) — defaults to `Online`; the window
    /// composition root and tests change it only through
    /// [`RadioView::set_connectivity`].
    connectivity: Rc<Cell<Connectivity>>,
    failure_kind: RefCell<Option<SourceErrorKind>>,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    empty_state: Cell<RadioEmptyState>,
    empty_page: SourceEmptyState,
    error_banner: SourceErrorBanner,
    pub(super) root: gtk4::Widget,
    footer: gtk4::Box,
    footer_add: gtk4::Button,
    pub(super) add_dialog: RefCell<Option<Rc<RadioAddDialog>>>,
    toast_overlay: gtk4::glib::WeakRef<adw::ToastOverlay>,
    pending_toasts: Cell<u32>,
    on_mutated: RefCell<Option<Callback>>,
    on_activated: RefCell<Option<IdCallback>>,
    on_removed: RefCell<Option<IdCallback>>,
    /// `SRC-13`: kept so a station change arriving from outside this view
    /// reaches the same reveal policy that view entry uses.
    reveal: Rc<super::radio_reveal::RadioReveal>,
    /// The bound cells whose content depends on playback rather than on the
    /// station record. Re-applied on every snapshot so the playing marker and
    /// the "Now playing" title move without a model signal.
    cells: Rc<super::radio_live_cells::RadioLiveCells>,
    pub(super) artwork_cells: Rc<super::radio_live_cells::RadioLiveCells>,
    /// The station a row of *this* table was last activated for, until it
    /// actually connects. `SRC-13` says an activated row was visible by
    /// definition and must not move the viewport — but a stream connects
    /// asynchronously: `begin_radio` opens with `Reconnecting`, and the
    /// `Connected` snapshot that finally moves the reveal arrives seconds
    /// after the double-click has returned. Remembering the station, rather
    /// than a moment, is what carries the "activated here" fact that far.
    activated_here: Cell<Option<i64>>,
}

pub(in crate::ui) struct RadioView {
    pub(super) shared: Rc<Shared>,
}

impl RadioView {
    pub(in crate::ui) fn new(conn: Rc<Db>, controller: Option<&Rc<PlayerController>>) -> Self {
        let model = Rc::new(RadioModel::new());
        let filter_bar = RadioFilterBar::new(conn.clone());
        let live = Rc::new(RefCell::new(RadioLiveState::default()));
        let live_source = {
            let live = live.clone();
            Rc::new(move || live.borrow().clone()) as LiveState
        };
        let connectivity = Rc::new(Cell::new(Connectivity::default()));
        let connectivity_source = {
            let connectivity = connectivity.clone();
            Rc::new(move || connectivity.get()) as radio_columns::ConnectivitySource
        };

        let column_view = gtk4::ColumnView::builder()
            .model(model.selection())
            .show_row_separators(false)
            .show_column_separators(false)
            .build();
        column_view.add_css_class("reprise-radio-table");
        column_view.add_css_class(crate::ui::source_context_surface::TABLE_CSS_CLASS);

        let cells = Rc::new(super::radio_live_cells::RadioLiveCells::default());
        let artwork_cells = Rc::new(super::radio_live_cells::RadioLiveCells::default());
        if let Some(controller) = controller {
            let live_for_state = live.clone();
            let cells_for_state = cells.clone();
            controller.add_on_playback_state_changed(move |state| {
                live_for_state.borrow_mut().playing = state == PlaybackState::Playing;
                cells_for_state.reapply();
            });
        }
        let query_source: crate::ui::search_highlight::QuerySource = {
            let filter_bar = filter_bar.clone();
            Rc::new(move || filter_bar.filter().query)
        };
        radio_columns::append_columns(
            &column_view,
            &live_source,
            &connectivity_source,
            &radio_columns::images_allowed_source(&conn),
            &cells,
            &artwork_cells,
            &query_source,
        );
        let column_model = super::radio_column_layout::install(&column_view, conn.clone());
        {
            let live = live_source.clone();
            let connectivity = connectivity_source.clone();
            radio_context_menu::wire_keyboard(
                &column_view,
                model.selection(),
                move |id| super::radio_presentation::row_is_accented(id, &live()),
                move || connectivity(),
            );
        }
        let action_group = gio::SimpleActionGroup::new();
        column_view.insert_action_group("radio", Some(&action_group));

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&column_view)
            .vexpand(true)
            .hexpand(true)
            .build();
        let list_overlay = gtk4::Overlay::new();
        list_overlay.set_child(Some(&scrolled));
        let end_of_results = crate::ui::end_of_results::EndOfResults::install(
            &list_overlay,
            &scrolled,
            &column_view,
            crate::ui::end_of_results::ResultsUnit::Stations,
        );
        {
            let filter_bar = filter_bar.clone();
            end_of_results.connect_recover(move || filter_bar.clear_all());
        }
        let status = adw::StatusPage::builder().vexpand(true).build();
        let status_button = gtk4::Button::new();
        status_button.set_halign(gtk4::Align::Center);
        status.set_child(Some(&status_button));
        let empty_page = SourceEmptyState::new(&radio_empty_state_copy());
        let error_banner = SourceErrorBanner::new();
        let stack = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        stack.add_named(&list_overlay, Some(LIST_PAGE));
        stack.add_named(&status, Some(STATUS_PAGE));
        stack.add_named(empty_page.widget(), Some(EMPTY_PAGE));
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-radio-view");
        root.append(filter_bar.widget());
        root.append(error_banner.widget());
        root.append(&stack);
        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        footer.set_margin_top(6);
        footer.set_margin_bottom(6);
        footer.set_margin_start(12);
        footer.set_margin_end(12);
        let footer_add = radio_add_button();
        footer.append(&footer_add);
        root.append(&footer);
        root.insert_action_group("radio", Some(&action_group));
        let reveal = super::radio_reveal::install(
            root.upcast_ref(),
            &scrolled,
            &column_view,
            model.clone(),
            live.clone(),
        );

        let shared = Rc::new(Shared {
            conn: conn.clone(),
            controller: controller.map_or_else(std::rc::Weak::new, Rc::downgrade),
            model,
            column_model,
            filter_bar: filter_bar.clone(),
            end_of_results,
            rows: RefCell::new(Vec::new()),
            live,
            connectivity,
            failure_kind: RefCell::new(None),
            stack,
            status,
            status_button: status_button.clone(),
            empty_state: Cell::new(RadioEmptyState::Empty),
            empty_page,
            error_banner,
            root: root.upcast(),
            footer,
            footer_add,
            add_dialog: RefCell::new(None),
            toast_overlay: gtk4::glib::WeakRef::new(),
            pending_toasts: Cell::new(0),
            on_mutated: RefCell::new(None),
            on_activated: RefCell::new(None),
            on_removed: RefCell::new(None),
            reveal,
            cells,
            artwork_cells,
            activated_here: Cell::new(None),
        });
        let add_dialog = {
            let weak = Rc::downgrade(&shared);
            RadioAddDialog::new(conn, shared.connectivity.clone(), move || {
                if let Some(shared) = weak.upgrade() {
                    refresh_shared(&shared);
                    notify_mutated(&shared);
                }
            })
        };
        shared.add_dialog.replace(Some(add_dialog));

        wire_actions(&action_group, &shared);
        {
            let weak = Rc::downgrade(&shared);
            column_view.connect_activate(move |_, position| {
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                let Some(object) = shared
                    .model
                    .store()
                    .item(position)
                    .and_downcast::<RadioObject>()
                else {
                    return;
                };
                activate_station(&shared, &object.row());
            });
        }
        {
            let weak = Rc::downgrade(&shared);
            filter_bar.set_on_changed(move |_| {
                if let Some(shared) = weak.upgrade() {
                    render_rows(&shared);
                }
            });
        }
        {
            let weak = Rc::downgrade(&shared);
            shared.empty_page.connect_add(move || {
                if let Some(shared) = weak.upgrade() {
                    present_add_dialog(&shared);
                }
            });
        }
        {
            let weak = Rc::downgrade(&shared);
            status_button.connect_clicked(move |_| {
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                // `SRC-10` moved the "nothing added yet" empty state onto
                // its own page with its own button (wired above via
                // `empty_page.connect_add`); this button is reachable only
                // for `NoResults` now.
                if shared.empty_state.get() == RadioEmptyState::NoResults {
                    shared.filter_bar.clear_all();
                }
            });
        }
        if let Some(controller) = controller {
            let weak = Rc::downgrade(&shared);
            controller.add_on_external_changed(move |snapshot| {
                if let Some(shared) = weak.upgrade() {
                    on_external_snapshot(&shared, snapshot);
                }
            });
        }
        refresh_shared(&shared);
        Self { shared }
    }

    pub(in crate::ui) fn root(&self) -> &gtk4::Widget {
        &self.shared.root
    }

    pub(in crate::ui) fn column_model(&self) -> Rc<dyn crate::ui::table_columns::EditorModel> {
        self.shared.column_model.clone()
    }

    pub(in crate::ui) fn refresh(&self) {
        refresh_shared(&self.shared);
    }

    pub(in crate::ui) fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.shared.toast_overlay.set(Some(overlay));
    }

    /// `NET-3b`: sets the connectivity seam this view's Play affordance
    /// consults. The window composition root feeds it from the one shared
    /// `gio::NetworkMonitor`; tests can inject the same explicit value.
    pub(in crate::ui) fn set_connectivity(&self, value: Connectivity) {
        self.shared.connectivity.set(value);
        render_rows(&self.shared);
        let failure_kind = self.shared.failure_kind.borrow().clone();
        if should_show_offline_radio_notice(
            value,
            !self.shared.rows.borrow().is_empty(),
            failure_kind.as_ref(),
        ) {
            show_radio_failure(
                &self.shared,
                SourceErrorKind::Offline,
                "NetworkMonitor reports no available connection".to_owned(),
            );
        } else if should_clear_radio_failure(value, failure_kind.as_ref()) {
            self.shared.failure_kind.replace(None);
            self.shared.error_banner.hide();
        }
    }

    pub(in crate::ui) fn set_on_mutated(&self, callback: impl Fn() + 'static) {
        *self.shared.on_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_station_activated(&self, callback: impl Fn(i64) + 'static) {
        *self.shared.on_activated.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_station_removed(&self, callback: impl Fn(i64) + 'static) {
        *self.shared.on_removed.borrow_mut() = Some(Rc::new(callback));
    }

    /// `RAD-5`: forwards to the Add Station dialog's "Near you" hand-off —
    /// see `RadioAddDialog::set_on_location_settings`. Wired from
    /// `window_runtime_wiring.rs` once `PreferencesContext` exists, the same
    /// shape as the Online Lyrics settings button's `present_plugins` deep
    /// link.
    pub(in crate::ui) fn set_on_location_settings(&self, callback: impl Fn() + 'static) {
        if let Some(dialog) = self.shared.add_dialog.borrow().as_ref() {
            dialog.set_on_location_settings(callback);
        }
    }
}

/// Everything one external playback snapshot does to this view. Kept a free
/// function rather than an inline closure so the tests can drive the exact
/// path the controller drives.
fn on_external_snapshot(
    shared: &Rc<Shared>,
    snapshot: Option<crate::ui::playback::external_media::ExternalPlaybackSnapshot>,
) {
    let failure = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.radio.as_ref())
        .and_then(|radio| radio.inline_error())
        .map(str::to_owned);
    let was_connected = super::radio_reveal::connected_station(&shared.live.borrow());
    let playing = shared.live.borrow().playing;
    let mut next_live = live_state(snapshot);
    next_live.playing = playing;
    shared.live.replace(next_live);
    if let Some(failure) = failure {
        show_radio_failure(shared, SourceErrorKind::Unreachable, failure);
    } else {
        shared.failure_kind.replace(None);
        shared.error_banner.hide();
    }
    render_rows(shared);
    shared
        .reveal
        .on_external_change(was_connected, reveal_change(shared));
}

/// Where the station now connected came from, from this table's point of view.
/// The pending "activated here" station survives until something actually
/// connects — a stream that is still opening reports nothing connected, and
/// that is precisely the window this has to bridge.
fn reveal_change(shared: &Shared) -> LoadedItemChange {
    let Some(connected) = super::radio_reveal::connected_station(&shared.live.borrow()) else {
        return LoadedItemChange::ChangedElsewhere;
    };
    if shared.activated_here.replace(None) == Some(connected) {
        LoadedItemChange::ActivatedHere
    } else {
        LoadedItemChange::ChangedElsewhere
    }
}

fn refresh_shared(shared: &Rc<Shared>) {
    match radio::station::list(&shared.conn) {
        Ok(mut rows) => {
            sort_rows(&mut rows);
            shared.filter_bar.set_rows(&rows);
            shared.rows.replace(rows);
            render_rows(shared);
        }
        Err(error) => tracing::warn!(%error, "could not load radio stations"),
    }
}

fn render_rows(shared: &Rc<Shared>) {
    let filter = shared.filter_bar.filter();
    let rows = filter_rows(&shared.rows.borrow(), &filter);
    let total = shared.rows.borrow().len();
    shared.filter_bar.set_counts(rows.len(), total);
    shared
        .end_of_results
        .update(crate::ui::end_of_results::EndOfResultsInput {
            shown: rows.len(),
            total,
            query: filter.query.clone(),
            facets_restrict: filter.genre.is_some() || filter.country.is_some(),
        });
    shared.model.replace(rows.clone());
    // FIL-5a: a refined query can keep exactly the same station rows, in
    // which case the model deliberately emits no rebind signal. Reapply the
    // bound cells so their in-place search markup still follows the query.
    shared.cells.reapply();
    apply_empty_state(
        shared,
        radio_empty_state_for(rows.len(), filter.is_active()),
    );
}

fn apply_empty_state(shared: &Shared, state: RadioEmptyState) {
    shared.empty_state.set(state);
    // `SRC-10`: the true "nothing added yet" empty state hides the toolbar
    // too — Add button, filter chips, and count all disappear, so the view
    // reads as unused rather than broken. `NoResults` keeps the toolbar,
    // since clearing filters is the way out of that state.
    shared
        .filter_bar
        .widget()
        .set_visible(state != RadioEmptyState::Empty);
    shared.footer.set_visible(state != RadioEmptyState::Empty);
    match state {
        RadioEmptyState::List => shared.stack.set_visible_child_name(LIST_PAGE),
        RadioEmptyState::Empty => shared.stack.set_visible_child_name(EMPTY_PAGE),
        RadioEmptyState::NoResults => {
            shared.status.set_icon_name(Some("system-search-symbolic"));
            shared
                .status
                .set_title(&strings::text(strings::SRC_NO_RESULTS_TITLE));
            shared.status.set_description(Some(""));
            shared
                .status_button
                .set_label(&strings::text(strings::SRC_CLEAR_FILTERS));
            shared.stack.set_visible_child_name(STATUS_PAGE);
        }
    }
}

fn radio_empty_state_copy() -> SourceEmptyStateCopy {
    SourceEmptyStateCopy {
        icon_name: NavIcon::Radio.icon_name(),
        title: strings::text(strings::RADIO_NO_STATIONS),
        body: strings::text(strings::RADIO_NO_STATIONS_DESCRIPTION),
        button_label: strings::text(strings::RADIO_ADD),
        button_icon_name: "list-add-symbolic",
        // Radio has no secondary line — the body already names the URL
        // path (a stream URL), so a second line would repeat it.
        secondary_line: None,
    }
}

fn present_add_dialog(shared: &Shared) {
    if let Some(dialog) = shared.add_dialog.borrow().clone() {
        dialog.present(&shared.root);
    }
}

fn radio_add_button() -> gtk4::Button {
    let add = gtk4::Button::with_label(&strings::text(strings::RADIO_ADD));
    buttons::arm(&add, buttons::ADD_ACTION_CLASS);
    add.set_action_name(Some("radio.open-add"));
    add
}

fn activate_station(shared: &Rc<Shared>, station: &StationRow) {
    // `SRC-13`: remembered until this station connects, so the reveal sees the
    // connection for what it is — a row of this very table, already visible,
    // started by the user. See `Shared::activated_here`.
    shared.activated_here.set(Some(station.id));
    if shared.live.borrow().station_id == Some(station.id) {
        if shared.live.borrow().connected {
            if let Some(controller) = shared.controller.upgrade() {
                controller.stop_external();
            }
        } else if let Some(controller) = shared.controller.upgrade() {
            if !controller.toggle_external_pause() {
                try_play_station(shared, &controller, station);
            }
        }
    } else if let Some(controller) = shared.controller.upgrade() {
        try_play_station(shared, &controller, station);
    } else {
        tracing::warn!("radio playback is unavailable without a playback backend");
    }
    if let Some(callback) = shared.on_activated.borrow().clone() {
        callback(station.id);
    }
}

/// `NET-3b`: a live stream cannot be deferred, so — unlike a download or a
/// device sync — offline never queues a fresh play attempt. Instead of
/// opening a connection that is known to fail, this simply does not start
/// one; the context menu's "No connection · Retry" label (`play_menu_label`)
/// is the only feedback, and clicking it again after connectivity returns
/// is the retry.
fn try_play_station(shared: &Shared, controller: &Rc<PlayerController>, station: &StationRow) {
    match connectivity::live_stream_action_outcome(shared.connectivity.get()) {
        connectivity::ActionOutcome::RunsNow => play_station(controller, station),
        connectivity::ActionOutcome::NoConnectionRetry => {
            tracing::debug!(
                station_id = station.id,
                "radio play skipped: no connection, showing retry instead of connecting"
            );
        }
        connectivity::ActionOutcome::QueuedOffline => unreachable!(
            "live_stream_action_outcome never returns QueuedOffline — a live stream cannot be deferred"
        ),
    }
}

fn play_station(controller: &Rc<PlayerController>, station: &StationRow) {
    if let Err(error) = controller.play_external(ExternalMedia::Radio {
        station_id: station.id,
        name: station.name.clone(),
        stream_url: station.stream_url.clone(),
        uuid: station.uuid.clone(),
    }) {
        tracing::warn!(%error, "could not start radio station");
    }
}

fn live_state(
    snapshot: Option<crate::ui::playback::external_media::ExternalPlaybackSnapshot>,
) -> RadioLiveState {
    let Some(snapshot) = snapshot else {
        return RadioLiveState::default();
    };
    let ExternalMedia::Radio { station_id, .. } = snapshot.media else {
        return RadioLiveState::default();
    };
    RadioLiveState {
        station_id: Some(station_id),
        connected: snapshot
            .radio
            .as_ref()
            .is_some_and(|radio| radio.phase() == RadioPhase::Connected),
        playing: false,
        title: snapshot.stream_tags.title,
        failed: snapshot
            .radio
            .as_ref()
            .and_then(|radio| radio.inline_error())
            .is_some(),
    }
}

fn show_radio_failure(shared: &Rc<Shared>, kind: SourceErrorKind, technical_cause: String) {
    let error = SourceError::new(kind, "Play radio station", technical_cause);
    shared.failure_kind.replace(Some(error.kind().clone()));
    let presentation = source_failure_presentation(
        SourceSurface::Radio,
        error.kind(),
        shared.rows.borrow().len(),
        1,
    );
    let weak = Rc::downgrade(shared);
    let dismiss_weak = weak.clone();
    shared.error_banner.show(
        &presentation,
        "",
        &error,
        &chrono::Utc::now().to_rfc3339(),
        move |action| {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            let station_id = shared.live.borrow().station_id;
            let Some(station) =
                station_id.and_then(|id| radio::station::get(&shared.conn, id).ok().flatten())
            else {
                return;
            };
            match radio_failure_action(action, station.uuid.as_deref()) {
                RadioFailureAction::RetryPlayback => activate_station(&shared, &station),
                RadioFailureAction::ReresolveDirectoryUrl => {
                    reresolve_station_url(&shared, &station);
                }
                RadioFailureAction::OpenAddDialog => present_add_dialog(&shared),
                RadioFailureAction::None => {}
            }
        },
        move || {
            let Some(shared) = dismiss_weak.upgrade() else {
                return;
            };
            shared.failure_kind.replace(None);
            shared.error_banner.hide();
        },
    );
}

fn remove_station(shared: &Rc<Shared>, id: i64) {
    let Some(station) = radio::station::get(&shared.conn, id).ok().flatten() else {
        return;
    };
    if shared.live.borrow().station_id == Some(id) {
        if let Some(controller) = shared.controller.upgrade() {
            controller.stop_external();
        }
    }
    match radio::station::tombstone(&shared.conn, id, now_unix()) {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            tracing::warn!(%error, "could not remove radio station");
            return;
        }
    }
    refresh_shared(shared);
    notify_mutated(shared);
    if let Some(callback) = shared.on_removed.borrow().clone() {
        callback(id);
    }
    let Some(overlay) = shared.toast_overlay.upgrade() else {
        commit_remove(shared, id);
        return;
    };
    shared
        .pending_toasts
        .set(shared.pending_toasts.get().saturating_add(1));
    let toast = crate::ui::toasts::plain(&strings::radio_remove_named(&station.name));
    toast.set_button_label(Some(&strings::text(strings::RADIO_UNDO)));
    toast.set_timeout(10);
    toast.set_priority(adw::ToastPriority::High);
    {
        let weak = Rc::downgrade(shared);
        toast.connect_button_clicked(move |_| {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            if let Err(error) = radio::station::undo_remove(&shared.conn, id) {
                tracing::warn!(%error, "could not undo radio removal");
            }
            refresh_shared(&shared);
            notify_mutated(&shared);
        });
    }
    {
        let weak = Rc::downgrade(shared);
        toast.connect_dismissed(move |_| {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            shared
                .pending_toasts
                .set(shared.pending_toasts.get().saturating_sub(1));
            commit_remove(&shared, id);
        });
    }
    overlay.add_toast(toast);
}

fn commit_remove(shared: &Rc<Shared>, id: i64) {
    if let Err(error) = radio::station::commit_remove(&shared.conn, id) {
        tracing::warn!(%error, "could not commit radio removal");
    }
}

fn wire_actions(actions: &gio::SimpleActionGroup, shared: &Rc<Shared>) {
    let open_add = gio::SimpleAction::new(ACTION_OPEN_ADD, None);
    let weak = Rc::downgrade(shared);
    open_add.connect_activate(move |_, _| {
        if let Some(shared) = weak.upgrade() {
            present_add_dialog(&shared);
        }
    });
    actions.add_action(&open_add);
    add_id_action(
        actions,
        radio_context_menu::ACTION_PLAY,
        shared,
        |shared, id| {
            if let Some(station) = radio::station::get(&shared.conn, id).ok().flatten() {
                activate_station(shared, &station);
            }
        },
    );
    add_id_action(
        actions,
        radio_context_menu::ACTION_COPY_URL,
        shared,
        |shared, id| {
            let Some(station) = radio::station::get(&shared.conn, id).ok().flatten() else {
                return;
            };
            if let Some(display) = gtk4::gdk::Display::default() {
                display.clipboard().set_text(&station.stream_url);
            }
        },
    );
    add_id_action(
        actions,
        radio_context_menu::ACTION_EDIT,
        shared,
        |shared, id| {
            let Some(station) = radio::station::get(&shared.conn, id).ok().flatten() else {
                return;
            };
            let conn = shared.conn.clone();
            let weak = Rc::downgrade(shared);
            super::edit_dialog::present(&shared.root, conn, &station, move || {
                if let Some(shared) = weak.upgrade() {
                    refresh_shared(&shared);
                    notify_mutated(&shared);
                }
            });
        },
    );
    add_id_action(
        actions,
        radio_context_menu::ACTION_REMOVE,
        shared,
        remove_station,
    );
}

fn add_id_action(
    actions: &gio::SimpleActionGroup,
    name: &str,
    shared: &Rc<Shared>,
    callback: impl Fn(&Rc<Shared>, i64) + 'static,
) {
    let action = gio::SimpleAction::new(name, Some(&i64::static_variant_type()));
    let weak = Rc::downgrade(shared);
    action.connect_activate(move |_, value| {
        let Some(id) = value.and_then(gtk4::glib::Variant::get::<i64>) else {
            return;
        };
        if let Some(shared) = weak.upgrade() {
            callback(&shared, id);
        }
    });
    actions.add_action(&action);
}

fn notify_mutated(shared: &Shared) {
    if let Some(callback) = shared.on_mutated.borrow().clone() {
        callback();
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

#[cfg(test)]
#[path = "radio_view_tests.rs"]
mod tests;
