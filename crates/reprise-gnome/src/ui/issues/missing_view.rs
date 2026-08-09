//! Grouped Missing-files view with tombstone Undo and auto-clean arming.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use chrono::Datelike;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::library::relink::RelinkTarget;
use reprise_core::library::settings::{self, AutoCleanSetting};
use reprise_core::models::Track;
use reprise_core::queries::{self, MissingGroup, MissingGroupKind};

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
        MissingGroupKind::Unlocatable => LocateActions {
            row: true,
            folder: false,
        },
        MissingGroupKind::Unavailable { .. } => LocateActions {
            row: false,
            folder: false,
        },
    }
}

fn group_copy(kind: &MissingGroupKind, count: u32) -> GroupCopy {
    let tracks = strings::missing_tracks(count);
    match kind {
        MissingGroupKind::Unavailable { mount_point: mount } => GroupCopy {
            icon: strings::issue_text(strings::MISSING_UNAVAILABLE_ICON),
            title: strings::issue_text(strings::MISSING_UNAVAILABLE_TITLE),
            meta: format!(
                "{mount} — {} · {tracks}",
                strings::issue_text(strings::MISSING_NOT_MOUNTED)
            ),
            note: strings::issue_text(strings::MISSING_RETURNS_WHEN_MOUNTED),
            actionable: false,
        },
        MissingGroupKind::Unlocatable => GroupCopy {
            icon: strings::issue_text(strings::MISSING_UNLOCATABLE_ICON),
            title: strings::issue_text(strings::MISSING_UNLOCATABLE_TITLE),
            meta: format!(
                "{} · {tracks}",
                strings::issue_text(strings::MISSING_UNLOCATABLE_META)
            ),
            note: String::new(),
            actionable: true,
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
    db: &Db,
    setting: AutoCleanSetting,
    now: i64,
) -> Result<AutoCleanActivation, rusqlite::Error> {
    let conn = &db;
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

fn start_auto_clean_counting_today(db: &Db, now: i64) -> Result<(), rusqlite::Error> {
    settings::set_auto_clean_armed_at(db, now)
}

fn auto_clean_confirmation_body(count: usize, days: u32) -> String {
    strings::missing_auto_clean_body(count, days)
}

fn remove_confirmation_body(kind: &MissingGroupKind, count: usize) -> String {
    match kind {
        MissingGroupKind::Unlocatable => strings::missing_unlocatable_remove_body(count),
        MissingGroupKind::Deleted | MissingGroupKind::Unavailable { .. } => {
            strings::missing_remove_body(count)
        }
    }
}

fn now_unix() -> i64 {
    gtk4::glib::real_time() / 1_000_000
}

fn missing_since_label(timestamp: i64) -> String {
    let pattern = &crate::ui::date_format::current().date;
    let date = chrono::DateTime::from_timestamp(timestamp.max(0), 0).map_or_else(
        || pattern.render(Some(1970), Some(1), Some(1)),
        |value| pattern.render(Some(value.year()), Some(value.month()), Some(value.day())),
    );
    strings::missing_since(&date)
}

type OnPurged = Rc<dyn Fn(&[i64])>;

pub(super) struct Shared {
    conn: Rc<Db>,
    groups: gtk4::Box,
    auto_clean_button: gtk4::MenuButton,
    toast_overlay: gtk4::glib::WeakRef<adw::ToastOverlay>,
    window: gtk4::glib::WeakRef<adw::ApplicationWindow>,
    on_mutated: RefCell<Option<Rc<dyn Fn()>>>,
    on_purged: RefCell<Option<OnPurged>>,
    pending_undo_toasts: Cell<u32>,
    db_path: RefCell<Option<PathBuf>>,
    relink_progress: RelinkProgressView,
    /// `FIL-1d`: this section's query, matched against file paths. Transient
    /// — it lives in the widget for as long as the visit does and is never
    /// written to settings.
    query: RefCell<String>,
}

pub(in crate::ui) struct MissingFilesView {
    shared: Rc<Shared>,
    root: gtk4::ScrolledWindow,
}

impl MissingFilesView {
    pub(in crate::ui) fn new(conn: Rc<Db>) -> Self {
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
            query: RefCell::new(String::new()),
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

    /// SEARCH-8a / FIL-1d: applies this view's query, matched against file
    /// paths. Returns the number of gaps that survived it, which is what the
    /// filter row counts.
    pub(in crate::ui) fn set_search_query(&self, query: &str) -> usize {
        self.shared.query.replace(query.trim().to_owned());
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
        tombstone_with_undo(&self.shared, &bulk_cleanup_kind(), ids);
    }

    pub(in crate::ui) fn set_db_path(&self, db_path: PathBuf) {
        self.shared.db_path.borrow_mut().replace(db_path);
    }

    pub(in crate::ui) fn relink_progress_widget(&self) -> &gtk4::Revealer {
        self.shared.relink_progress.widget()
    }

    pub(in crate::ui) fn set_on_relink_progress_activate(
        &self,
        callback: impl Fn(reprise_core::view_source::ViewSource) + 'static,
    ) {
        self.shared.relink_progress.set_on_activate(callback);
    }
}

fn refresh(shared: &Rc<Shared>) -> usize {
    while let Some(child) = shared.groups.first_child() {
        shared.groups.remove(&child);
    }
    update_auto_clean_label(shared);
    let query = shared.query.borrow().clone();
    let groups = {
        let conn = &shared.conn;
        queries::query_missing_groups_matching(conn, &query).unwrap_or_else(|error| {
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

/// The one category the sidebar's bulk "remove all missing" acts on.
///
/// Two places have to agree on it: `sidebar_issue_cleanup::missing_ids_for_
/// cleanup` selects the ids, `MissingFilesView::remove_with_undo` tombstones
/// them under a state guard keyed by kind. When those two were written out
/// separately, widening the selection without touching the guard would have
/// let the extra ids fall silently out of the tombstone — the same shape of
/// bug the per-card Remove-all button already had. Both now read this.
pub(in crate::ui) fn bulk_cleanup_kind() -> MissingGroupKind {
    MissingGroupKind::Deleted
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
        let kind = group.kind.clone();
        button.connect_clicked(move |_| {
            let ids = collect_group_ids(&shared, &kind);
            confirm_remove(&shared, kind.clone(), ids);
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
        super::missing_menus::install_card_context_menu(
            shared,
            card.header_widget(),
            &copy.title,
            group.kind.clone(),
        );
    }
    let kind = group.kind.clone();
    let actionable = copy.actionable;
    let query = shared.query.borrow().clone();
    let row_shared = shared.clone();
    let listbox = card.body_listbox().clone();
    CollapsedList::attach_to(
        card.body_listbox(),
        group.track_count,
        Rc::new(move |index| {
            let track = {
                let conn = &row_shared.conn;
                queries::query_missing_rows_matching(conn, &kind, &query, index, 1)
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
                        kind.clone(),
                        actionable,
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
    kind: MissingGroupKind,
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
        let remove_kind = kind.clone();
        pills.push(
            IssuePill::new(strings::issue_text(strings::MISSING_REMOVE), move || {
                confirm_remove(&shared, remove_kind.clone(), vec![id]);
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
            kind,
            removable,
            locatable,
        );
    }
    row.widget().clone().upcast()
}

pub(super) fn collect_group_targets(shared: &Shared, kind: &MissingGroupKind) -> Vec<RelinkTarget> {
    let conn = &shared.conn;
    queries::query_missing_rows(conn, kind, 0, u32::MAX).map_or_else(
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
    let conn = &shared.conn;
    queries::query_missing_rows(conn, kind, 0, u32::MAX).map_or_else(
        |error| {
            tracing::error!(%error, "missing view: failed to load removal ids");
            Vec::new()
        },
        |rows| rows.into_iter().map(|track| track.id).collect(),
    )
}

pub(super) fn confirm_remove(shared: &Rc<Shared>, kind: MissingGroupKind, ids: Vec<i64>) {
    if ids.is_empty() {
        return;
    }
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("missing view: window is gone; cannot confirm removal");
        return;
    };
    let dialog = adw::AlertDialog::builder()
        .heading(strings::issue_text(strings::MISSING_REMOVE_HEADING))
        .body(remove_confirmation_body(&kind, ids.len()))
        .close_response(RESPONSE_CANCEL)
        .build();
    dialog.add_response(RESPONSE_CANCEL, &strings::issue_text(strings::CANCEL));
    dialog.add_response(
        RESPONSE_REMOVE,
        &strings::issue_text(strings::MISSING_REMOVE),
    );
    dialog.set_response_appearance(RESPONSE_REMOVE, adw::ResponseAppearance::Destructive);
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(&window);
    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        focus_guard.restore();
        if response.as_str() == RESPONSE_REMOVE {
            tombstone_with_undo(&shared, &kind, &ids);
        }
    });
}

fn tombstone_with_undo(shared: &Rc<Shared>, kind: &MissingGroupKind, ids: &[i64]) {
    let result = queries::tombstone_still_missing(&shared.conn, kind, ids, now_unix());
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
                let conn = &shared.conn;
                queries::undo_tombstone(conn, &ids)
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
        let conn = &shared.conn;
        queries::purge_tombstones(conn)
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
    let plan = activate_auto_clean(&shared.conn, setting, now);
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
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(&window);
    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        focus_guard.restore();
        if response.as_str() == RESPONSE_REMOVE_NOW {
            run_auto_clean_now(&shared, now);
        } else {
            let result = start_auto_clean_counting_today(&shared.conn, now);
            if let Err(error) = result {
                tracing::error!(%error, "missing view: failed to arm auto-clean from today");
            }
            notify_mutated(&shared);
        }
    });
}

fn run_auto_clean_now(shared: &Rc<Shared>, now: i64) {
    let result = remove_auto_clean_backlog_now(&shared.conn, now);
    match result {
        Ok(ids) => {
            notify_purged(shared, &ids);
            notify_mutated(shared);
        }
        Err(error) => tracing::error!(%error, "missing view: auto-clean failed"),
    }
}

fn remove_auto_clean_backlog_now(db: &Db, now: i64) -> Result<Vec<i64>, rusqlite::Error> {
    let conn = &db;
    let run = settings::set_auto_clean_armed_at(conn, 0)
        .and_then(|()| queries::run_auto_clean(conn, now));
    let rearm = settings::set_auto_clean_armed_at(conn, now);
    match (run, rearm) {
        (Ok(ids), Ok(())) => Ok(ids),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

fn update_auto_clean_label(shared: &Shared) {
    let setting = settings::get_missing_auto_clean(&shared.conn);
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
    let last_relinked = settings::get_last_scan_relinked(&shared.conn)
        .ok()
        .flatten();
    if let Some(count) = last_relinked {
        let label = gtk4::Label::new(Some(&strings::missing_last_relinked(&count.to_string())));
        label.set_xalign(0.0);
        card.append(&label);
    }
    let auto_clean_off = matches!(
        settings::get_missing_auto_clean(&shared.conn),
        AutoCleanSetting::Off
    );
    // Auto-clean remains deleted-only; Unlocatable cleanup is always an
    // explicit user action despite sharing the card's remove flow.
    let has_deleted = {
        let conn = &shared.conn;
        queries::query_missing_groups(conn).is_ok_and(|groups| {
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

pub(in crate::ui) fn purge_startup_tombstones(conn: &Rc<Db>) -> Result<Vec<i64>, rusqlite::Error> {
    queries::purge_tombstones(conn)
}

#[cfg(test)]
#[path = "missing_view_tests.rs"]
mod tests;
