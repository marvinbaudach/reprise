use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::connectivity::{self, Connectivity};
use reprise_core::db::Db;
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
use crate::ui::source_reveal::LoadedItemChange;
use crate::ui::source_error_banner::SourceErrorBanner;
use crate::ui::strings;

#[path = "radio_failure_ui.rs"]
mod failure_ui;
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

type IdCallback = Rc<dyn Fn(i64)>;
type Callback = Rc<dyn Fn()>;

struct Shared {
    conn: Rc<Db>,
    controller: std::rc::Weak<PlayerController>,
    model: Rc<RadioModel>,
    filter_bar: Rc<RadioFilterBar>,
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
    root: gtk4::Widget,
    add_dialog: RefCell<Option<Rc<RadioAddDialog>>>,
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
    shared: Rc<Shared>,
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
        radio_columns::append_columns(&column_view, &live_source, &connectivity_source, &cells);
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
        stack.add_named(&scrolled, Some(LIST_PAGE));
        stack.add_named(&status, Some(STATUS_PAGE));
        stack.add_named(empty_page.widget(), Some(EMPTY_PAGE));
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-radio-view");
        root.append(filter_bar.widget());
        root.append(error_banner.widget());
        root.append(&stack);
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
            filter_bar: filter_bar.clone(),
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
            add_dialog: RefCell::new(None),
            toast_overlay: gtk4::glib::WeakRef::new(),
            pending_toasts: Cell::new(0),
            on_mutated: RefCell::new(None),
            on_activated: RefCell::new(None),
            on_removed: RefCell::new(None),
            reveal,
            cells,
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
            filter_bar.connect_add(move || {
                if let Some(shared) = weak.upgrade() {
                    present_add_dialog(&shared);
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
    shared.live.replace(live_state(snapshot));
    if let Some(failure) = failure {
        show_radio_failure(shared, SourceErrorKind::Unreachable, failure);
    } else {
        shared.failure_kind.replace(None);
        shared.error_banner.hide();
    }
    render_rows(shared);
    // The station list itself is unchanged for a pure playback snapshot, so
    // `render_rows` left the store alone; the live parts of the visible rows
    // are pushed straight into their cells instead.
    shared.cells.reapply();
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
    let rows = filter_rows(&shared.rows.borrow(), &shared.filter_bar.filter());
    let total = shared.rows.borrow().len();
    shared.filter_bar.set_counts(rows.len(), total);
    shared.model.replace(rows.clone());
    apply_empty_state(
        shared,
        radio_empty_state_for(rows.len(), shared.filter_bar.filter().is_active()),
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
    let toast = adw::Toast::new(&strings::radio_remove_named(&station.name));
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
mod tests {
    use super::*;
    use crate::ui::playback::external_media::{
        ExternalPlaybackSnapshot, RadioPresentation, StreamTags,
    };
    use crate::ui::playback::preview::PlaybackMode;
    use reprise_core::source_error::FailureAction;

    #[test]
    fn rad_1_table_projects_only_connected_radio_snapshots() {
        let connected = live_state(Some(ExternalPlaybackSnapshot {
            mode: PlaybackMode::Radio,
            media: ExternalMedia::Radio {
                station_id: 7,
                name: "Station".into(),
                stream_url: "https://radio.example/live".into(),
                uuid: None,
            },
            art_url: None,
            can_go_previous: false,
            can_go_next: false,
            stream_tags: StreamTags {
                title: Some("Artist — Song".into()),
                organization: None,
            },
            podcast_phase: None,
            restored: false,
            radio: Some(RadioPresentation::connected()),
            error: None,
        }));
        assert_eq!(connected.station_id, Some(7));
        assert!(connected.connected);
        assert_eq!(connected.title.as_deref(), Some("Artist — Song"));
    }

    #[test]
    fn rad_3_dead_stream_actions_distinguish_retry_from_directory_reresolution() {
        assert_eq!(
            radio_failure_action(FailureAction::TryAgain, Some("station-uuid")),
            RadioFailureAction::RetryPlayback
        );
        assert_eq!(
            radio_failure_action(FailureAction::FindNewUrl, Some("station-uuid")),
            RadioFailureAction::ReresolveDirectoryUrl
        );
        assert_eq!(
            radio_failure_action(FailureAction::FindNewUrl, None),
            RadioFailureAction::OpenAddDialog
        );
    }

    fn add_station(conn: &Rc<Db>, name: &str) -> i64 {
        radio::station::add_or_restore(
            conn,
            &radio::station::NewStation {
                uuid: None,
                name: name.into(),
                stream_url: format!("https://example.invalid/{name}"),
                homepage: None,
                favicon_url: None,
                genre: None,
                codec: None,
                bitrate_kbps: None,
                country_code: None,
                votes: None,
            },
            0,
        )
        .unwrap()
    }

    fn connected_snapshot(station_id: i64, title: &str) -> ExternalPlaybackSnapshot {
        ExternalPlaybackSnapshot {
            mode: PlaybackMode::Radio,
            media: ExternalMedia::Radio {
                station_id,
                name: "Station".into(),
                stream_url: "https://example.invalid/stream".into(),
                uuid: None,
            },
            art_url: None,
            can_go_previous: false,
            can_go_next: false,
            stream_tags: StreamTags {
                title: Some(title.into()),
                organization: None,
            },
            podcast_phase: None,
            restored: false,
            radio: Some(RadioPresentation::connected()),
            error: None,
        }
    }

    fn playing_cells(view: &RadioView) -> usize {
        fn count(widget: &gtk4::Widget) -> usize {
            let here = usize::from(widget.has_css_class("reprise-radio-playing"));
            let mut child = widget.first_child();
            let mut total = here;
            while let Some(current) = child {
                total += count(&current);
                child = current.next_sibling();
            }
            total
        }
        count(&view.shared.root)
    }

    /// The reported radio bug: double-clicking a station moved the highlight
    /// off it — every external snapshot (the play itself, the phase change,
    /// and later every new ICY title) rebuilt the whole store with
    /// `remove_all()`, and `GtkSingleSelection` answers that by autoselecting
    /// row 0. The same rebuild emptied the store for an instant, which reset
    /// the scroll offset — the "the rows keep switching around" half of the
    /// report. Nothing about the station list changed here, so nothing in the
    /// table may move.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn rad_1_a_live_state_update_never_moves_the_selection() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        add_station(&conn, "Alpha");
        let bravo = add_station(&conn, "Bravo");
        add_station(&conn, "Charlie");

        let view = RadioView::new(conn, None);
        let window = gtk4::Window::new();
        window.set_default_size(900, 400);
        window.set_child(Some(view.root()));
        window.present();
        crate::ui::source_context_surface::settle_layout();

        view.shared.model.selection().set_selected(1);
        assert_eq!(view.shared.model.selection().selected(), 1);

        on_external_snapshot(&view.shared, Some(connected_snapshot(bravo, "Artist — Song")));
        crate::ui::source_context_surface::settle_layout();

        assert_eq!(
            view.shared.model.selection().selected(),
            1,
            "a live-state snapshot must leave the selected station selected"
        );
        assert!(
            playing_cells(&view) > 0,
            "the connected station must still pick up its playing marker"
        );

        // A second snapshot carrying only a new title — the every-song case.
        on_external_snapshot(&view.shared, Some(connected_snapshot(bravo, "Next — Song")));
        crate::ui::source_context_surface::settle_layout();
        assert_eq!(view.shared.model.selection().selected(), 1);
    }

    fn list_vadjustment(view: &RadioView) -> gtk4::Adjustment {
        view.shared
            .stack
            .child_by_name(LIST_PAGE)
            .and_downcast::<gtk4::ScrolledWindow>()
            .expect("the list page is a ScrolledWindow")
            .vadjustment()
    }

    /// The other half of the report — "the rows keep switching around". A
    /// snapshot used to empty the store for an instant, which collapsed the
    /// scrolled window's content height and reset the offset to the top; and
    /// a station activated *here* was still classified as a change from
    /// elsewhere, so the reveal centred the row the user had just clicked.
    /// `SRC-13`: an activated row was visible by definition, so nothing moves.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_13_activating_a_station_here_leaves_the_viewport_where_the_user_put_it() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let ids: Vec<i64> = (0..40)
            .map(|index| add_station(&conn, &format!("Station {index:02}")))
            .collect();

        let view = RadioView::new(conn, None);
        let window = gtk4::Window::new();
        window.set_default_size(900, 300);
        window.set_child(Some(view.root()));
        window.present();
        crate::ui::source_context_surface::settle_layout();

        let adjustment = list_vadjustment(&view);
        adjustment.set_value(adjustment.upper() / 2.0);
        crate::ui::source_context_surface::settle_layout();
        let scrolled_to = adjustment.value();
        assert!(scrolled_to > 0.0, "the table must be scrollable for this");

        // A double-click on a station of this table. Scrolling the table above
        // counts as user activity, which would hold off *any* reveal for the
        // next 1.5 seconds and make this pass for the wrong reason.
        view.shared.reveal.forget_scroll_activity();
        let station = radio::station::get(&view.shared.conn, ids[35])
            .unwrap()
            .unwrap();
        activate_station(&view.shared, &station);
        // The stream connects asynchronously: the activation itself is long
        // over by the time the `Connected` snapshot — the one the reveal acts
        // on — arrives.
        on_external_snapshot(&view.shared, Some(connected_snapshot(ids[35], "Song")));
        crate::ui::source_context_surface::settle_layout();

        assert_eq!(
            adjustment.value(),
            scrolled_to,
            "activating a station here must not move the table"
        );

        // The same change arriving from elsewhere — the player bar, MPRIS — is
        // still revealed, which is what `SRC-13` promises. Without this the
        // assertion above would prove nothing: a reveal that never runs at all
        // also never moves the viewport.
        view.shared.reveal.forget_scroll_activity();
        on_external_snapshot(&view.shared, Some(connected_snapshot(ids[2], "Song")));
        crate::ui::source_context_surface::settle_layout();
        assert_ne!(
            adjustment.value(),
            scrolled_to,
            "a station connected elsewhere is still revealed"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_1_radio_empty_state_offers_add_station_without_playback() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let view = RadioView::new(conn, None);
        // `SRC-10` moved this action onto the shared empty-state page's own
        // button (`empty_page`) rather than the still-existing
        // `status_button`, which now serves only `NoResults`.
        assert_eq!(
            view.shared.empty_page.button_label_text().as_deref(),
            Some("Add station")
        );
        assert_eq!(view.shared.empty_state.get(), RadioEmptyState::Empty);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_10_radio_empty_state_hides_the_toolbar_and_the_first_station_restores_it() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let view = RadioView::new(conn.clone(), None);

        assert!(!view.shared.filter_bar.widget().is_visible());
        assert_eq!(
            view.shared.stack.visible_child_name().as_deref(),
            Some(EMPTY_PAGE)
        );

        radio::station::add_or_restore(
            &conn,
            &radio::station::NewStation {
                uuid: None,
                name: "Test Station".into(),
                stream_url: "https://example.invalid/stream".into(),
                homepage: None,
                favicon_url: None,
                genre: None,
                codec: None,
                bitrate_kbps: None,
                country_code: None,
                votes: None,
            },
            0,
        )
        .unwrap();
        view.refresh();

        assert!(view.shared.filter_bar.widget().is_visible());
        assert_eq!(
            view.shared.stack.visible_child_name().as_deref(),
            Some(LIST_PAGE)
        );
    }

    /// `SRC-10` addendum (Block B2): the filter-mismatch state is the
    /// opposite of the genuine empty state — the filter row stays visible,
    /// with a "Clear filters" action, because clearing the filter (not
    /// adding a station) is the way out. Would go red if `NoResults` hid
    /// the toolbar the same way `Empty` does.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_10_the_filter_mismatch_state_keeps_the_filter_row_visible_unlike_the_true_empty_state() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let view = RadioView::new(conn, None);

        apply_empty_state(&view.shared, RadioEmptyState::Empty);
        assert!(!view.shared.filter_bar.widget().is_visible());

        apply_empty_state(&view.shared, RadioEmptyState::NoResults);
        assert!(view.shared.filter_bar.widget().is_visible());
        assert_eq!(view.shared.status.title(), "Nothing matches these filters");
        assert_eq!(
            view.shared.status_button.label().as_deref(),
            Some("Clear filters")
        );

        // The button's click handler reads `empty_state` (set above) to
        // decide whether to clear filters — clicking it here must not
        // panic and must route through `clear_all` rather than a refresh.
        view.shared.status_button.emit_clicked();
    }
}
