//! The conversion/staging view (plan §2.4/7; docs/ux-rules Section AB) — a
//! special view over `ai_jobs` + the staging store, **not** a real playlist
//! row-source. It renders exactly one aggregate progress bar (INST-2), one row
//! per active job with its state and affordances (INST-3), and enforces the
//! play/wait split (INST-4/INST-5) through the pure `conversion_model`.
//!
//! Playback, save, discard, save-all and clear are injected callbacks so the
//! widget stays testable without a player or a promotion; the window wires them
//! to the real facades. All progress figures come from the job rows.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::ai_jobs::{self, AiJob};
use reprise_core::ai_staging::StagingStore;
use rusqlite::Connection;

use super::conversion_model::{self, RowClickAction, RowState};
use crate::ui::strings;

type JobCallback = Rc<dyn Fn(i64)>;
type VoidCallback = Rc<dyn Fn()>;

/// Which per-row callback a job button fires.
#[derive(Clone, Copy)]
enum JobAction {
    Play,
    Save,
    Discard,
}

/// Per-row widgets kept for the headless display-test accessors (read under
/// `#[cfg(test)]`), so the tests can assert the *actual* widget state.
#[allow(dead_code)]
struct RowWidgets {
    play: gtk4::Button,
    save: gtk4::Button,
    discard: gtk4::Button,
    progress: gtk4::ProgressBar,
    state_label: gtk4::Label,
    state: RowState,
}

/// The conversion view. Cheap `Rc` handle; `refresh` reloads from the DB.
pub(in crate::ui) struct ConversionView {
    root: gtk4::Box,
    aggregate: gtk4::ProgressBar,
    /// INST-8 "Summe": the aggregate disk cost of all kept (undecided) renders,
    /// beside the per-row sizes. Hidden while no undecided render exists.
    disk_total: gtk4::Label,
    save_all: gtk4::Button,
    clear: gtk4::Button,
    list: gtk4::ListBox,
    empty: gtk4::Label,
    conn: Rc<RefCell<Connection>>,
    staging: StagingStore,
    rows: RefCell<HashMap<i64, RowWidgets>>,
    on_play: RefCell<Option<JobCallback>>,
    on_save: RefCell<Option<JobCallback>>,
    on_discard: RefCell<Option<JobCallback>>,
    on_save_all: RefCell<Option<VoidCallback>>,
    on_clear: RefCell<Option<VoidCallback>>,
}

impl ConversionView {
    pub(in crate::ui) fn new(conn: Rc<RefCell<Connection>>, staging: StagingStore) -> Rc<Self> {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let title = gtk4::Label::new(Some(&strings::text(strings::CONVERSION_TITLE)));
        title.add_css_class("title-4");
        title.set_halign(gtk4::Align::Start);
        title.set_hexpand(true);
        let save_all = gtk4::Button::with_label(&strings::text(strings::CONVERSION_SAVE_ALL));
        save_all.add_css_class("suggested-action");
        let clear = gtk4::Button::with_label(&strings::text(strings::CONVERSION_CLEAR));
        clear.add_css_class("flat");
        header.append(&title);
        header.append(&save_all);
        header.append(&clear);

        // INST-2: the single aggregate progress bar. No toast, no sidebar slot.
        let aggregate = gtk4::ProgressBar::new();
        aggregate.set_show_text(true);

        // INST-8: the "Summe" beside the per-row sizes — the total disk cost of
        // all kept renders. A separate caption from the INST-2 progress bar, so
        // job progress and disk cost stay distinct. Hidden until a render is kept.
        let disk_total = gtk4::Label::new(None);
        disk_total.add_css_class("dim-label");
        disk_total.add_css_class("caption");
        disk_total.set_halign(gtk4::Align::Start);
        disk_total.set_visible(false);

        let list = gtk4::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk4::SelectionMode::None);
        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_child(Some(&list));

        let empty = gtk4::Label::new(Some(&strings::text(strings::CONVERSION_EMPTY)));
        empty.add_css_class("dim-label");
        empty.set_vexpand(true);

        root.append(&header);
        root.append(&aggregate);
        root.append(&disk_total);
        root.append(&scrolled);
        root.append(&empty);

        let view = Rc::new(Self {
            root,
            aggregate,
            disk_total,
            save_all,
            clear,
            list,
            empty,
            conn,
            staging,
            rows: RefCell::new(HashMap::new()),
            on_play: RefCell::new(None),
            on_save: RefCell::new(None),
            on_discard: RefCell::new(None),
            on_save_all: RefCell::new(None),
            on_clear: RefCell::new(None),
        });
        view.wire_header();
        view.refresh();
        view
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    fn wire_header(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.save_all.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                let callback = view.on_save_all.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            }
        });
        let weak = Rc::downgrade(self);
        // INST-7: "Clear playlist" warns on undecided renders. The warning
        // itself is the callback's responsibility (it owns the window); the view
        // hands it the fact via `has_undecided_now`.
        self.clear.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                let callback = view.on_clear.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            }
        });
    }

    /// Reloads the rows and the aggregate bar from the job queue + staging. Cheap
    /// to call on every worker tick — the active set is a handful of rows.
    pub(in crate::ui) fn refresh(self: &Rc<Self>) {
        let jobs = {
            let conn = self.conn.borrow();
            ai_jobs::list_active_jobs(&conn).unwrap_or_else(|error| {
                tracing::error!(%error, "conversion view: could not list active jobs");
                Vec::new()
            })
        };

        let aggregate = conversion_model::aggregate(&jobs);
        self.aggregate.set_fraction(aggregate.fraction());
        self.aggregate.set_text(Some(&strings::conversion_aggregate(
            aggregate.done,
            aggregate.total,
            aggregate.percent(),
        )));

        // INST-8: the disk cost of each staged render, keyed by job id so the
        // row can show it (undecided renders are kept, so their size is real).
        let sizes: HashMap<i64, u64> = self
            .staging
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|entry| (entry.job_id, entry.size_bytes))
            .collect();

        self.list.remove_all();
        let mut rows = HashMap::with_capacity(jobs.len());
        let mut kept_bytes: u64 = 0;
        for job in &jobs {
            let staged = self.staging.exists(job.id);
            let state = conversion_model::row_state(job, staged);
            let size = sizes.get(&job.id).copied();
            // INST-8 "Summe": the kept-render total is the sum of exactly the
            // per-row sizes shown for undecided (kept, unsaved) renders.
            if matches!(state, RowState::DoneUnsaved { .. }) {
                kept_bytes = kept_bytes.saturating_add(size.unwrap_or(0));
            }
            let widgets = self.build_row(job, state, size);
            rows.insert(job.id, widgets);
        }
        *self.rows.borrow_mut() = rows;

        // INST-8: show the aggregate disk cost of all kept renders, hidden when
        // none are kept.
        if kept_bytes > 0 {
            self.disk_total
                .set_text(&strings::conversion_disk_total(&format_render_size(
                    kept_bytes,
                )));
            self.disk_total.set_visible(true);
        } else {
            self.disk_total.set_visible(false);
        }

        let empty = jobs.is_empty();
        self.empty.set_visible(empty);
        self.list.set_visible(!empty);
        self.save_all
            .set_sensitive(conversion_model::has_undecided(&jobs));
    }

    fn build_row(self: &Rc<Self>, job: &AiJob, state: RowState, size: Option<u64>) -> RowWidgets {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row.set_margin_top(6);
        row.set_margin_bottom(6);
        row.set_margin_start(8);
        row.set_margin_end(8);

        let info = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        info.set_hexpand(true);
        info.set_halign(gtk4::Align::Start);
        let title = gtk4::Label::new(Some(&row_title(job)));
        title.set_halign(gtk4::Align::Start);
        let state_label = gtk4::Label::new(Some(&row_state_caption(state, size)));
        state_label.add_css_class("dim-label");
        state_label.add_css_class("caption");
        state_label.set_halign(gtk4::Align::Start);
        info.append(&title);
        info.append(&state_label);

        // INST-3: processing rows carry their own permille bar.
        let progress = gtk4::ProgressBar::new();
        progress.set_valign(gtk4::Align::Center);
        progress.set_width_request(120);
        match state {
            RowState::Processing { permille } => {
                progress.set_fraction(f64::from(permille) / 1000.0);
                progress.set_visible(true);
            }
            _ => progress.set_visible(false),
        }

        // INST-4/INST-5: Play is enabled only when the click resolves to Play; a
        // processing row waits-with-progress (not playable) and shows its bar.
        let play = flat_button(strings::CONVERSION_PLAY);
        play.set_sensitive(matches!(
            conversion_model::click_action(state),
            RowClickAction::Play
        ));
        self.wire_job_button(&play, job.id, JobAction::Play);

        // INST-6: per-row Save/Discard on an undecided render only.
        let undecided = matches!(state, RowState::DoneUnsaved { .. });
        let save = flat_button(strings::CONVERSION_SAVE);
        save.set_visible(undecided);
        self.wire_job_button(&save, job.id, JobAction::Save);
        let discard = flat_button(strings::CONVERSION_DISCARD);
        discard.set_visible(undecided || matches!(state, RowState::Failed));
        discard.add_css_class("destructive-action");
        self.wire_job_button(&discard, job.id, JobAction::Discard);

        row.append(&info);
        row.append(&progress);
        row.append(&play);
        row.append(&save);
        row.append(&discard);
        self.list.append(&row);

        RowWidgets {
            play,
            save,
            discard,
            progress,
            state_label,
            state,
        }
    }

    fn wire_job_button(self: &Rc<Self>, button: &gtk4::Button, job_id: i64, action: JobAction) {
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            let Some(view) = weak.upgrade() else {
                return;
            };
            let slot = match action {
                JobAction::Play => &view.on_play,
                JobAction::Save => &view.on_save,
                JobAction::Discard => &view.on_discard,
            };
            let callback = slot.borrow().clone();
            if let Some(callback) = callback {
                callback(job_id);
            }
        });
    }

    // --- Callback setters (wired by the window) ---

    pub(in crate::ui) fn set_on_play(&self, callback: impl Fn(i64) + 'static) {
        *self.on_play.borrow_mut() = Some(Rc::new(callback));
    }
    pub(in crate::ui) fn set_on_save(&self, callback: impl Fn(i64) + 'static) {
        *self.on_save.borrow_mut() = Some(Rc::new(callback));
    }
    pub(in crate::ui) fn set_on_discard(&self, callback: impl Fn(i64) + 'static) {
        *self.on_discard.borrow_mut() = Some(Rc::new(callback));
    }
    pub(in crate::ui) fn set_on_save_all(&self, callback: impl Fn() + 'static) {
        *self.on_save_all.borrow_mut() = Some(Rc::new(callback));
    }
    pub(in crate::ui) fn set_on_clear(&self, callback: impl Fn() + 'static) {
        *self.on_clear.borrow_mut() = Some(Rc::new(callback));
    }

    /// Whether any row is a finished, undecided render — the fact the window's
    /// "clear playlist" confirmation keys on (INST-7).
    pub(in crate::ui) fn has_undecided_now(&self) -> bool {
        let conn = self.conn.borrow();
        ai_jobs::list_active_jobs(&conn).is_ok_and(|jobs| conversion_model::has_undecided(&jobs))
    }
}

fn flat_button(label: &str) -> gtk4::Button {
    let button = gtk4::Button::with_label(&strings::text(label));
    button.add_css_class("flat");
    button.set_valign(gtk4::Align::Center);
    button
}

fn row_title(job: &AiJob) -> String {
    match job.source_track_id {
        Some(id) => format!("Conversion · source #{id}"),
        None => format!("Conversion · job #{}", job.id),
    }
}

fn state_text(state: RowState) -> String {
    let key = match state {
        RowState::Queued => strings::STATE_QUEUED,
        RowState::Processing { .. } => strings::STATE_PROCESSING,
        RowState::DoneUnsaved { .. } => strings::STATE_READY_UNSAVED,
        RowState::Saved => strings::STATE_SAVED,
        RowState::Failed => strings::STATE_FAILED,
    };
    strings::text(key)
}

/// The row's caption: its state, plus the render's disk cost for an undecided
/// (kept, unsaved) render — INST-8's "disk cost is visible in the view".
fn row_state_caption(state: RowState, size: Option<u64>) -> String {
    let base = state_text(state);
    match (state, size) {
        (RowState::DoneUnsaved { .. }, Some(bytes)) => {
            format!("{base} · {}", format_render_size(bytes))
        }
        _ => base,
    }
}

/// A compact human size for a staging render (INST-8). Uses the app's standard
/// binary units — KiB/MiB, powers of 1024, one decimal — matching
/// `device_sync_strings`' `format_bytes`. The math already divided by
/// 1024; only the labels were wrong (they read KB/MB for KiB/MiB values).
fn format_render_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= MIB {
        format!("{:.1} MiB", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.1} KiB", bytes_f / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
#[path = "conversion_view_tests.rs"]
mod tests;

#[cfg(test)]
impl ConversionView {
    pub(in crate::ui) fn aggregate_fraction(&self) -> f64 {
        self.aggregate.fraction()
    }
    pub(in crate::ui) fn aggregate_text(&self) -> String {
        self.aggregate
            .text()
            .map(|t| t.to_string())
            .unwrap_or_default()
    }
    pub(in crate::ui) fn disk_total_text(&self) -> String {
        self.disk_total.text().to_string()
    }
    pub(in crate::ui) fn disk_total_visible(&self) -> bool {
        self.disk_total.is_visible()
    }
    pub(in crate::ui) fn row_play_enabled(&self, job_id: i64) -> Option<bool> {
        self.rows
            .borrow()
            .get(&job_id)
            .map(|row| row.play.is_sensitive())
    }
    pub(in crate::ui) fn row_is_processing(&self, job_id: i64) -> Option<bool> {
        self.rows.borrow().get(&job_id).map(|row| {
            matches!(row.state, RowState::Processing { .. }) && row.progress.is_visible()
        })
    }
    pub(in crate::ui) fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }
    pub(in crate::ui) fn save_all_sensitive(&self) -> bool {
        self.save_all.is_sensitive()
    }
    pub(in crate::ui) fn row_state_text(&self, job_id: i64) -> Option<String> {
        self.rows
            .borrow()
            .get(&job_id)
            .map(|row| row.state_label.text().to_string())
    }
    pub(in crate::ui) fn row_save_visible(&self, job_id: i64) -> Option<bool> {
        self.rows
            .borrow()
            .get(&job_id)
            .map(|row| row.save.is_visible())
    }
    pub(in crate::ui) fn row_discard_visible(&self, job_id: i64) -> Option<bool> {
        self.rows
            .borrow()
            .get(&job_id)
            .map(|row| row.discard.is_visible())
    }
}
