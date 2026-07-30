use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::connectivity::{self, Connectivity};
use reprise_core::db::Db;
use reprise_core::radio::{self, StationRow};

use super::add_dialog::RadioAddDialog;
use super::radio_columns::{self, LiveState, OnRemove};
use super::radio_context_menu;
use super::radio_empty_state::{radio_empty_state_for, RadioEmptyState};
use super::radio_filter_bar::{filter_rows, RadioFilterBar};
use super::radio_model::{RadioModel, RadioObject};
use super::radio_presentation::{sort_rows, RadioLiveState};
use crate::ui::playback::external_media::{ExternalMedia, RadioPhase};
use crate::ui::playback::player_controller::PlayerController;
use crate::ui::sidebar::sidebar_presentation::NavIcon;
use crate::ui::source_empty_state::{SourceEmptyState, SourceEmptyStateCopy};
use crate::ui::strings;

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
    /// `reprise_core::connectivity`) — defaults to `Online` and is not
    /// wired to any real OS signal yet; only [`RadioView::set_connectivity`]
    /// (and tests) change it.
    connectivity: Rc<Cell<Connectivity>>,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    empty_state: Cell<RadioEmptyState>,
    empty_page: SourceEmptyState,
    root: gtk4::Widget,
    add_dialog: RefCell<Option<Rc<RadioAddDialog>>>,
    toast_overlay: gtk4::glib::WeakRef<adw::ToastOverlay>,
    pending_toasts: Cell<u32>,
    on_mutated: RefCell<Option<Callback>>,
    on_activated: RefCell<Option<IdCallback>>,
    on_removed: RefCell<Option<IdCallback>>,
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

        let remove_target = Rc::new(RefCell::new(None::<std::rc::Weak<Shared>>));
        let remove_shared = remove_target.clone();
        let on_remove: OnRemove = Rc::new(move |id| {
            if let Some(shared) = remove_shared
                .borrow()
                .as_ref()
                .and_then(std::rc::Weak::upgrade)
            {
                remove_station(&shared, id);
            }
        });
        radio_columns::append_columns(&column_view, &on_remove, &live_source, &connectivity_source);
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
        root.append(&stack);

        let shared = Rc::new(Shared {
            conn: conn.clone(),
            controller: controller.map_or_else(std::rc::Weak::new, Rc::downgrade),
            model,
            filter_bar: filter_bar.clone(),
            rows: RefCell::new(Vec::new()),
            live,
            connectivity,
            stack,
            status,
            status_button: status_button.clone(),
            empty_state: Cell::new(RadioEmptyState::Empty),
            empty_page,
            root: root.upcast(),
            add_dialog: RefCell::new(None),
            toast_overlay: gtk4::glib::WeakRef::new(),
            pending_toasts: Cell::new(0),
            on_mutated: RefCell::new(None),
            on_activated: RefCell::new(None),
            on_removed: RefCell::new(None),
        });
        remove_target.replace(Some(Rc::downgrade(&shared)));

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
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                shared.live.replace(live_state(snapshot));
                render_rows(&shared);
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
    /// consults. Not wired to any real OS signal yet — see
    /// `reprise_core::connectivity` for what such a signal could and could
    /// not know; this is the injection point a future binding would call.
    pub(in crate::ui) fn set_connectivity(&self, value: Connectivity) {
        self.shared.connectivity.set(value);
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
    }
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
            stream_tags: StreamTags {
                title: Some("Artist — Song".into()),
                organization: None,
            },
            podcast_phase: None,
            radio: Some(RadioPresentation::connected()),
            error: None,
        }));
        assert_eq!(connected.station_id, Some(7));
        assert!(connected.connected);
        assert_eq!(connected.title.as_deref(), Some("Artist — Song"));
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
