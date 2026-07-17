//! Grouped Missing-files view with tombstone Undo and auto-clean arming.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::relink::RelinkTarget;
use reprise_core::library::settings::{self, AutoCleanSetting};
use reprise_core::models::Track;
use reprise_core::queries::{self, MissingGroup, MissingGroupKind};
use rusqlite::Connection;

use super::missing_dialogs::LocateContext;
use super::missing_progress::RelinkProgressView;
use super::{CollapsedList, IssueCard, IssuePill, IssueRow, RowSpec};
use crate::ui::strings;

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_REMOVE: &str = "remove";
const RESPONSE_REMOVE_NOW: &str = "remove-now";
const RESPONSE_START_TODAY: &str = "start-today";
const TRACK_ROW_PREFIX: &str = "missing-track-";

#[derive(Debug, Clone, PartialEq, Eq)]
struct GroupCopy {
    icon: String,
    title: String,
    meta: String,
    note: String,
    actionable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LocateActions {
    row: bool,
    folder: bool,
}

fn locate_actions(kind: &MissingGroupKind) -> LocateActions {
    match kind {
        MissingGroupKind::Deleted => LocateActions {
            row: true,
            folder: true,
        },
        MissingGroupKind::Unavailable { mount_point: None } => LocateActions {
            row: true,
            folder: false,
        },
        MissingGroupKind::Unavailable {
            mount_point: Some(_),
        } => LocateActions {
            row: false,
            folder: false,
        },
    }
}

fn group_copy(kind: &MissingGroupKind, count: u32) -> GroupCopy {
    let tracks = strings::missing_tracks(count);
    match kind {
        MissingGroupKind::Unavailable {
            mount_point: Some(mount),
        } => GroupCopy {
            icon: strings::issue_text(strings::MISSING_UNAVAILABLE_ICON),
            title: strings::issue_text(strings::MISSING_UNAVAILABLE_TITLE),
            meta: format!(
                "{mount} — {} · {tracks}",
                strings::issue_text(strings::MISSING_NOT_MOUNTED)
            ),
            note: strings::issue_text(strings::MISSING_RETURNS_WHEN_MOUNTED),
            actionable: false,
        },
        MissingGroupKind::Unavailable { mount_point: None } => GroupCopy {
            icon: strings::issue_text(strings::MISSING_UNAVAILABLE_ICON),
            title: strings::issue_text(strings::MISSING_UNAVAILABLE_TITLE),
            meta: format!(
                "{} — {tracks}",
                strings::issue_text(strings::MISSING_UNKNOWN_LOCATION)
            ),
            note: strings::issue_text(strings::MISSING_VERIFY_NEXT_SCAN),
            actionable: false,
        },
        MissingGroupKind::Deleted => GroupCopy {
            icon: strings::issue_text(strings::MISSING_DELETED_ICON),
            title: strings::issue_text(strings::MISSING_DELETED_TITLE),
            meta: format!(
                "{} · {tracks}",
                strings::issue_text(strings::MISSING_DELETED_META)
            ),
            note: String::new(),
            actionable: true,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoCleanActivation {
    Disabled,
    Armed,
    ConfirmBacklog { days: u32, eligible: usize },
}

fn activate_auto_clean(
    conn: &Connection,
    setting: AutoCleanSetting,
    now: i64,
) -> Result<AutoCleanActivation, rusqlite::Error> {
    let previous = settings::get_missing_auto_clean(conn);
    settings::set_missing_auto_clean(conn, setting)?;
    let AutoCleanSetting::Days(days) = setting else {
        return Ok(AutoCleanActivation::Disabled);
    };
    if !matches!(previous, AutoCleanSetting::Off) {
        if settings::get_auto_clean_armed_at(conn)?.is_none() {
            settings::set_auto_clean_armed_at(conn, now)?;
        }
        return Ok(AutoCleanActivation::Armed);
    }

    // Preview the pre-existing backlog through the frozen core facade. The
    // epoch arm is never observable outside this synchronous borrow: restore
    // the safe "today" arm before returning or presenting a dialog.
    settings::set_auto_clean_armed_at(conn, 0)?;
    let eligible = queries::auto_clean_eligible(conn, now)?.len();
    settings::set_auto_clean_armed_at(conn, now)?;
    Ok(if eligible == 0 {
        AutoCleanActivation::Armed
    } else {
        AutoCleanActivation::ConfirmBacklog { days, eligible }
    })
}

fn start_auto_clean_counting_today(conn: &Connection, now: i64) -> Result<(), rusqlite::Error> {
    settings::set_auto_clean_armed_at(conn, now)
}

fn auto_clean_confirmation_body(count: usize, days: u32) -> String {
    strings::missing_auto_clean_body(count, days)
}

fn remove_confirmation_body(count: usize) -> String {
    strings::missing_remove_body(count)
}

fn now_unix() -> i64 {
    gtk4::glib::real_time() / 1_000_000
}

fn missing_since_label(timestamp: i64) -> String {
    let date = chrono::DateTime::from_timestamp(timestamp.max(0), 0).map_or_else(
        || "Jan 1".to_string(),
        |value| value.format("%b %-d").to_string(),
    );
    strings::missing_since(&date)
}

type OnPurged = Rc<dyn Fn(&[i64])>;

pub(super) struct Shared {
    conn: Rc<RefCell<Connection>>,
    groups: gtk4::Box,
    auto_clean_button: gtk4::MenuButton,
    toast_overlay: gtk4::glib::WeakRef<adw::ToastOverlay>,
    window: gtk4::glib::WeakRef<adw::ApplicationWindow>,
    on_mutated: RefCell<Option<Rc<dyn Fn()>>>,
    on_purged: RefCell<Option<OnPurged>>,
    pending_undo_toasts: Cell<u32>,
    db_path: RefCell<Option<PathBuf>>,
    relink_progress: RelinkProgressView,
}

pub(in crate::ui) struct MissingFilesView {
    shared: Rc<Shared>,
    root: gtk4::ScrolledWindow,
}

impl MissingFilesView {
    pub(in crate::ui) fn new(conn: Rc<RefCell<Connection>>) -> Self {
        let auto_clean_button = gtk4::MenuButton::new();
        auto_clean_button.add_css_class("flat");
        auto_clean_button.add_css_class("pill");
        let groups = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.set_margin_top(12);
        content.set_margin_bottom(96);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        header.set_halign(gtk4::Align::End);
        header.append(&auto_clean_button);
        content.append(&header);
        content.append(&groups);

        let root = gtk4::ScrolledWindow::builder()
            .child(&content)
            .vexpand(true)
            .hexpand(true)
            .build();
        let shared = Rc::new(Shared {
            conn,
            groups,
            auto_clean_button,
            toast_overlay: gtk4::glib::WeakRef::new(),
            window: gtk4::glib::WeakRef::new(),
            on_mutated: RefCell::new(None),
            on_purged: RefCell::new(None),
            pending_undo_toasts: Cell::new(0),
            db_path: RefCell::new(None),
            relink_progress: RelinkProgressView::new(),
        });
        install_auto_clean_menu(&shared);
        Self { shared, root }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    pub(in crate::ui) fn refresh(&self) -> usize {
        refresh(&self.shared)
    }

    pub(in crate::ui) fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.shared.toast_overlay.set(Some(overlay));
    }

    pub(in crate::ui) fn set_window(&self, window: &adw::ApplicationWindow) {
        self.shared.window.set(Some(window));
    }

    pub(in crate::ui) fn set_on_mutated(&self, callback: impl Fn() + 'static) {
        *self.shared.on_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_purged(&self, callback: impl Fn(&[i64]) + 'static) {
        *self.shared.on_purged.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn remove_with_undo(&self, ids: &[i64]) {
        tombstone_with_undo(&self.shared, ids);
    }

    pub(in crate::ui) fn set_db_path(&self, db_path: PathBuf) {
        self.shared.db_path.borrow_mut().replace(db_path);
    }

    pub(in crate::ui) fn relink_progress_widget(&self) -> &gtk4::Revealer {
        self.shared.relink_progress.widget()
    }

    pub(in crate::ui) fn set_on_relink_progress_activate(&self, callback: impl Fn() + 'static) {
        self.shared.relink_progress.set_on_activate(callback);
    }
}

fn refresh(shared: &Rc<Shared>) -> usize {
    while let Some(child) = shared.groups.first_child() {
        shared.groups.remove(&child);
    }
    update_auto_clean_label(shared);
    let groups = {
        let conn = shared.conn.borrow();
        queries::query_missing_groups(&conn).unwrap_or_else(|error| {
            tracing::error!(%error, "missing view: failed to load groups");
            Vec::new()
        })
    };
    let count = groups.iter().map(|group| group.track_count as usize).sum();
    for group in groups {
        shared.groups.append(&build_group(shared, &group));
    }
    if count > 0 {
        shared.groups.append(&build_info_card(shared));
        let footnote = gtk4::Label::new(Some(&strings::issue_text(strings::MISSING_FOOTNOTE)));
        footnote.set_xalign(0.0);
        footnote.add_css_class("dim-label");
        shared.groups.append(&footnote);
    }
    count
}

fn build_group(shared: &Rc<Shared>, group: &MissingGroup) -> gtk4::Widget {
    let copy = group_copy(&group.kind, group.track_count);
    let locate = locate_actions(&group.kind);
    let action = if copy.actionable {
        let button =
            gtk4::Button::with_label(&strings::missing_remove_all(group.track_count as usize));
        button.add_css_class("flat");
        button.add_css_class("pill");
        button.add_css_class("issue-remove-pill");
        let shared = shared.clone();
        button.connect_clicked(move |_| {
            let ids = collect_group_ids(&shared, &MissingGroupKind::Deleted);
            confirm_remove(&shared, ids);
        });
        Some(button.upcast::<gtk4::Widget>())
    } else if copy.note.is_empty() {
        None
    } else {
        let note = gtk4::Label::new(Some(&copy.note));
        note.add_css_class("dim-label");
        Some(note.upcast::<gtk4::Widget>())
    };
    let card = IssueCard::new(&copy.icon, &copy.title, &copy.meta, action);
    if locate.folder {
        super::missing_menus::install_card_context_menu(shared, card.header_widget());
    }
    let kind = group.kind.clone();
    let row_shared = shared.clone();
    let listbox = card.body_listbox().clone();
    CollapsedList::attach_to(
        card.body_listbox(),
        group.track_count,
        Rc::new(move |index| {
            let track = {
                let conn = row_shared.conn.borrow();
                queries::query_missing_rows(&conn, &kind, index, 1)
                    .ok()
                    .and_then(|mut rows| rows.pop())
            };
            track.map_or_else(
                || gtk4::Label::new(Some("")).upcast(),
                |track| {
                    build_track_row(
                        &row_shared,
                        &listbox,
                        track,
                        matches!(kind, MissingGroupKind::Deleted),
                        locate_actions(&kind).row,
                    )
                },
            )
        }),
    );
    card.widget().clone().upcast()
}

fn build_track_row(
    shared: &Rc<Shared>,
    listbox: &gtk4::ListBox,
    track: Track,
    removable: bool,
    locatable: bool,
) -> gtk4::Widget {
    let id = track.id;
    let target = RelinkTarget {
        track_id: id,
        old_path: PathBuf::from(&track.path),
    };
    let mut pills = Vec::new();
    if removable {
        let shared = shared.clone();
        pills.push(
            IssuePill::new(strings::issue_text(strings::MISSING_REMOVE), move || {
                confirm_remove(&shared, vec![id]);
            })
            .with_css_class("issue-remove-pill"),
        );
    }
    if locatable {
        let shared = shared.clone();
        let target = target.clone();
        pills.push(IssuePill::new(
            strings::issue_text(strings::MISSING_LOCATE),
            move || super::missing_dialogs::locate_file(locate_context(&shared), target.clone()),
        ));
    }
    let title = if track.title.trim().is_empty() {
        std::path::Path::new(&track.path).file_name().map_or_else(
            || track.path.clone(),
            |name| name.to_string_lossy().into_owned(),
        )
    } else {
        track.title.clone()
    };
    let row = IssueRow::new(RowSpec {
        cover: None,
        primary: title,
        secondary: track.artist,
        tertiary: track.album,
        right_idle: missing_since_label(track.missing_since.unwrap_or(0)),
        pills,
    });
    row.widget()
        .set_widget_name(&format!("{TRACK_ROW_PREFIX}{id}"));
    if !removable {
        row.widget().set_opacity(0.65);
    }
    if removable || locatable {
        super::missing_menus::install_row_context_menu(
            shared,
            listbox,
            row.widget(),
            target,
            removable,
            locatable,
        );
    }
    row.widget().clone().upcast()
}

pub(super) fn collect_group_targets(shared: &Shared, kind: &MissingGroupKind) -> Vec<RelinkTarget> {
    let conn = shared.conn.borrow();
    queries::query_missing_rows(&conn, kind, 0, u32::MAX).map_or_else(
        |error| {
            tracing::error!(%error, "missing view: failed to load relink targets");
            Vec::new()
        },
        |rows| {
            rows.into_iter()
                .map(|track| RelinkTarget {
                    track_id: track.id,
                    old_path: track.path.into(),
                })
                .collect()
        },
    )
}

pub(super) fn locate_context(shared: &Shared) -> LocateContext {
    LocateContext {
        conn: shared.conn.clone(),
        db_path: shared.db_path.borrow().clone(),
        window: shared.window.clone(),
        toast_overlay: shared.toast_overlay.clone(),
        progress: shared.relink_progress.clone(),
        on_relinked: shared.on_mutated.borrow().clone(),
    }
}

pub(super) fn selected_ids(listbox: &gtk4::ListBox) -> Vec<i64> {
    listbox
        .selected_rows()
        .into_iter()
        .filter_map(|row| {
            row.widget_name()
                .strip_prefix(TRACK_ROW_PREFIX)?
                .parse()
                .ok()
        })
        .collect()
}

fn collect_group_ids(shared: &Shared, kind: &MissingGroupKind) -> Vec<i64> {
    let conn = shared.conn.borrow();
    queries::query_missing_rows(&conn, kind, 0, u32::MAX).map_or_else(
        |error| {
            tracing::error!(%error, "missing view: failed to load removal ids");
            Vec::new()
        },
        |rows| rows.into_iter().map(|track| track.id).collect(),
    )
}

pub(super) fn confirm_remove(shared: &Rc<Shared>, ids: Vec<i64>) {
    if ids.is_empty() {
        return;
    }
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("missing view: window is gone; cannot confirm removal");
        return;
    };
    let dialog = adw::AlertDialog::builder()
        .heading(strings::issue_text(strings::MISSING_REMOVE_HEADING))
        .body(remove_confirmation_body(ids.len()))
        .close_response(RESPONSE_CANCEL)
        .build();
    dialog.add_response(RESPONSE_CANCEL, &strings::issue_text(strings::CANCEL));
    dialog.add_response(
        RESPONSE_REMOVE,
        &strings::issue_text(strings::MISSING_REMOVE),
    );
    dialog.set_response_appearance(RESPONSE_REMOVE, adw::ResponseAppearance::Destructive);
    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        if response.as_str() == RESPONSE_REMOVE {
            tombstone_with_undo(&shared, &ids);
        }
    });
}

/// Revalidates a potentially stale dialog selection at the UI boundary.
/// `queries::tombstone_tracks` deliberately remains generic because FB-7
/// also removes present tracks outside this Missing-only surface. Here the
/// selection must still be proven `Deleted`; keeping the read and tombstone
/// in one transaction prevents a scanner resurrection from slipping between
/// them (a conflicting writer makes the transaction fail safely instead).
fn tombstone_still_deleted(
    conn: &mut Connection,
    requested_ids: &[i64],
    now: i64,
) -> Result<Vec<i64>, rusqlite::Error> {
    if requested_ids.is_empty() {
        return Ok(Vec::new());
    }
    let tx = conn.transaction()?;
    let currently_deleted: HashSet<i64> =
        queries::query_missing_rows(&tx, &MissingGroupKind::Deleted, 0, u32::MAX)?
            .into_iter()
            .map(|track| track.id)
            .collect();
    let mut seen = HashSet::new();
    let tombstoned: Vec<i64> = requested_ids
        .iter()
        .copied()
        .filter(|id| currently_deleted.contains(id) && seen.insert(*id))
        .collect();
    let changed = queries::tombstone_tracks(&tx, &tombstoned, now)?;
    debug_assert_eq!(changed, tombstoned.len());
    tx.commit()?;
    Ok(tombstoned)
}

fn tombstone_with_undo(shared: &Rc<Shared>, ids: &[i64]) {
    let result = {
        let mut conn = shared.conn.borrow_mut();
        tombstone_still_deleted(&mut conn, ids, now_unix())
    };
    let tombstoned = match result {
        Ok(ids) => ids,
        Err(error) => {
            tracing::error!(%error, "missing view: tombstone failed");
            return;
        }
    };
    if tombstoned.is_empty() {
        return;
    }
    let changed = tombstoned.len();
    notify_mutated(shared);
    let Some(overlay) = shared.toast_overlay.upgrade() else {
        purge_if_no_undo_left(shared);
        return;
    };
    shared
        .pending_undo_toasts
        .set(shared.pending_undo_toasts.get() + 1);
    let toast = adw::Toast::new(&strings::missing_removed(changed));
    toast.set_button_label(Some(&strings::issue_text(strings::MISSING_UNDO)));
    toast.set_timeout(10);
    toast.set_priority(adw::ToastPriority::High);
    {
        let shared = shared.clone();
        let ids = tombstoned;
        toast.connect_button_clicked(move |_| {
            let result = {
                let conn = shared.conn.borrow();
                queries::undo_tombstone(&conn, &ids)
            };
            match result {
                Ok(_) => notify_mutated(&shared),
                Err(error) => tracing::error!(%error, "missing view: undo failed"),
            }
        });
    }
    {
        let shared = shared.clone();
        toast.connect_dismissed(move |_| {
            shared
                .pending_undo_toasts
                .set(shared.pending_undo_toasts.get().saturating_sub(1));
            purge_if_no_undo_left(&shared);
        });
    }
    overlay.add_toast(toast);
}

fn purge_if_no_undo_left(shared: &Rc<Shared>) {
    if shared.pending_undo_toasts.get() != 0 {
        return;
    }
    let result = {
        let mut conn = shared.conn.borrow_mut();
        queries::purge_tombstones(&mut conn)
    };
    match result {
        Ok(ids) => {
            notify_purged(shared, &ids);
            notify_mutated(shared);
        }
        Err(error) => tracing::error!(%error, "missing view: tombstone purge failed"),
    }
}

fn notify_mutated(shared: &Shared) {
    let callback = shared.on_mutated.borrow().clone();
    if let Some(callback) = callback {
        callback();
    }
}

fn notify_purged(shared: &Shared, ids: &[i64]) {
    let callback = shared.on_purged.borrow().clone();
    if let Some(callback) = callback {
        callback(ids);
    }
}

fn install_auto_clean_menu(shared: &Rc<Shared>) {
    let popover = gtk4::Popover::new();
    let choices = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    for (label, setting) in [
        (
            strings::MISSING_AUTO_CLEAN_OPTION_OFF,
            AutoCleanSetting::Off,
        ),
        (
            strings::MISSING_AUTO_CLEAN_OPTION_30,
            AutoCleanSetting::Days(30),
        ),
        (
            strings::MISSING_AUTO_CLEAN_OPTION_90,
            AutoCleanSetting::Days(90),
        ),
    ] {
        let button = gtk4::Button::with_label(&strings::issue_text(label));
        button.add_css_class("flat");
        let shared = shared.clone();
        let popover = popover.clone();
        button.connect_clicked(move |_| {
            handle_auto_clean_choice(&shared, setting);
            popover.popdown();
        });
        choices.append(&button);
    }
    popover.set_child(Some(&choices));
    shared.auto_clean_button.set_popover(Some(&popover));
}

fn handle_auto_clean_choice(shared: &Rc<Shared>, setting: AutoCleanSetting) {
    let now = now_unix();
    let plan = {
        let conn = shared.conn.borrow();
        activate_auto_clean(&conn, setting, now)
    };
    match plan {
        Ok(AutoCleanActivation::ConfirmBacklog { days, eligible }) => {
            show_auto_clean_confirmation(shared, days, eligible, now);
        }
        Ok(_) => notify_mutated(shared),
        Err(error) => tracing::error!(%error, "missing view: failed to set auto-clean"),
    }
    update_auto_clean_label(shared);
}

fn show_auto_clean_confirmation(shared: &Rc<Shared>, days: u32, eligible: usize, now: i64) {
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("missing view: window is gone; auto-clean stays armed from today");
        return;
    };
    let dialog = adw::AlertDialog::builder()
        .heading(strings::issue_text(strings::MISSING_AUTO_CLEAN_HEADING))
        .body(auto_clean_confirmation_body(eligible, days))
        .close_response(RESPONSE_START_TODAY)
        .build();
    dialog.add_response(
        RESPONSE_START_TODAY,
        &strings::issue_text(strings::MISSING_AUTO_CLEAN_START_TODAY),
    );
    dialog.add_response(
        RESPONSE_REMOVE_NOW,
        &strings::issue_text(strings::MISSING_AUTO_CLEAN_REMOVE_NOW),
    );
    dialog.set_response_appearance(RESPONSE_REMOVE_NOW, adw::ResponseAppearance::Destructive);
    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        if response.as_str() == RESPONSE_REMOVE_NOW {
            run_auto_clean_now(&shared, now);
        } else {
            let result = {
                let conn = shared.conn.borrow();
                start_auto_clean_counting_today(&conn, now)
            };
            if let Err(error) = result {
                tracing::error!(%error, "missing view: failed to arm auto-clean from today");
            }
            notify_mutated(&shared);
        }
    });
}

fn run_auto_clean_now(shared: &Rc<Shared>, now: i64) {
    let result = {
        let mut conn = shared.conn.borrow_mut();
        remove_auto_clean_backlog_now(&mut conn, now)
    };
    match result {
        Ok(ids) => {
            notify_purged(shared, &ids);
            notify_mutated(shared);
        }
        Err(error) => tracing::error!(%error, "missing view: auto-clean failed"),
    }
}

fn remove_auto_clean_backlog_now(
    conn: &mut Connection,
    now: i64,
) -> Result<Vec<i64>, rusqlite::Error> {
    let run = settings::set_auto_clean_armed_at(conn, 0)
        .and_then(|()| queries::run_auto_clean(conn, now));
    let rearm = settings::set_auto_clean_armed_at(conn, now);
    match (run, rearm) {
        (Ok(ids), Ok(())) => Ok(ids),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

fn update_auto_clean_label(shared: &Shared) {
    let setting = settings::get_missing_auto_clean(&shared.conn.borrow());
    let label = match setting {
        AutoCleanSetting::Off => strings::issue_text(strings::MISSING_AUTO_CLEAN_OFF),
        AutoCleanSetting::Days(days) => strings::missing_auto_clean_label(days),
    };
    shared.auto_clean_button.set_label(&label);
}

fn build_info_card(shared: &Shared) -> gtk4::Widget {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    card.add_css_class("missing-info-card");
    let explanation = gtk4::Label::new(Some(&strings::issue_text(strings::MISSING_RELINK_INFO)));
    explanation.set_xalign(0.0);
    explanation.set_wrap(true);
    card.append(&explanation);
    let last_relinked = settings::get_last_scan_relinked(&shared.conn.borrow())
        .ok()
        .flatten();
    if let Some(count) = last_relinked {
        let label = gtk4::Label::new(Some(&strings::missing_last_relinked(&count.to_string())));
        label.set_xalign(0.0);
        card.append(&label);
    }
    let auto_clean_off = matches!(
        settings::get_missing_auto_clean(&shared.conn.borrow()),
        AutoCleanSetting::Off
    );
    let has_deleted = {
        let conn = shared.conn.borrow();
        queries::query_missing_groups(&conn).is_ok_and(|groups| {
            groups
                .iter()
                .any(|group| matches!(group.kind, MissingGroupKind::Deleted))
        })
    };
    if auto_clean_off && has_deleted {
        let hint = gtk4::Label::new(Some(&strings::issue_text(strings::MISSING_AUTO_CLEAN_HINT)));
        hint.set_xalign(0.0);
        hint.set_wrap(true);
        card.append(&hint);
    }
    card.upcast()
}

pub(in crate::ui) fn purge_startup_tombstones(
    conn: &Rc<RefCell<Connection>>,
) -> Result<Vec<i64>, rusqlite::Error> {
    queries::purge_tombstones(&mut conn.borrow_mut())
}

#[cfg(test)]
#[path = "missing_view_tests.rs"]
mod tests;
