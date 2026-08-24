//! The Preferences dialog's background-activity footer.
//!
//! `docs/plans/plugins-online-content-master-hierarchy.md`, third draft: the
//! dialog head belongs to the title, so nothing is hung into it or laid over
//! it any more. Every running job instead gets a fixed, non-scrolling place at
//! the foot of the dialog — **named**, one row each, all of them visible at the
//! same time. Before this the two online batches shared a single slot, which is
//! why the lyrics check only appeared once Artwork had been switched off.
//!
//! The main window's own scan card is untouched: it keeps its place there, in
//! its own z-layer under the dialog, and is never reparented into it.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::strings;

/// Set on the footer container; every rule in [`css`] is scoped to it.
pub(in crate::ui) const BAR_CLASS: &str = "reprise-background-bar";
const TITLE_CLASS: &str = "reprise-background-title";
const COUNT_CLASS: &str = "reprise-background-count";
const OWNER_CLASS: &str = "reprise-background-owner";
const DETAIL_CLASS: &str = "reprise-background-detail";
const TRACK_CLASS: &str = "reprise-background-track";
const PERCENT_CLASS: &str = "reprise-background-percent";
const EMPTY_CLASS: &str = "reprise-background-empty";

/// Column widths. The owner, track and percent columns are fixed so a row
/// cannot shift sideways while its counter runs; the description is the one
/// that gives way.
///
/// These are the draft's proportions, not its pixels. The draft's row was laid
/// out wider than this dialog is: ported literally (132/150/44) the fixed
/// columns left the description 101 px against the 197 px it asks for, and the
/// footer shipped reading "Album cover…" — a named job whose name is the only
/// part left. Measured on 2026-08-24, Adwaita defaults: "Online Lyrics" needs
/// 90 px, "100%" 39 px, the longest English description 197 px, and the pinned
/// sidebar takes 195 px of the dialog's 760.
const OWNER_WIDTH_PX: i32 = 100;
const TRACK_WIDTH_PX: i32 = 92;
const PERCENT_WIDTH_PX: i32 = 40;
/// Between two columns of one row.
const COLUMN_SPACING_PX: i32 = 12;

/// Which plugin a footer row belongs to. The order is the order the rows are
/// listed in, and it is fixed: a job never moves to another row's place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::ui) enum JobOwner {
    Artwork,
    OnlineLyrics,
}

impl JobOwner {
    /// The plugin's own name, exactly as the Plugins page prints it — a job may
    /// never show up unnamed or under a name of its own invention.
    fn title(self) -> String {
        strings::text(match self {
            Self::Artwork => strings::ARTWORK,
            Self::OnlineLyrics => strings::ONLINE_LYRICS,
        })
    }

    const ORDER: [Self; 2] = [Self::Artwork, Self::OnlineLyrics];
}

/// One row's worth of state: what is running, how far along, for whom.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct JobRowState {
    pub(in crate::ui) owner: JobOwner,
    pub(in crate::ui) detail: String,
    pub(in crate::ui) fraction: f64,
}

impl JobRowState {
    fn percent(&self) -> u32 {
        (self.fraction.clamp(0.0, 1.0) * 100.0).round() as u32
    }
}

/// What the footer shows for a given set of jobs and gate state — decided
/// without a widget in sight, so the rules are testable on their own.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct BarState {
    pub(in crate::ui) rows: Vec<JobRowState>,
    /// `None` while nothing runs: the draft asks for no badge at all then,
    /// rather than a badge reading zero.
    pub(in crate::ui) count_badge: Option<String>,
    /// The stand-in shown instead of rows while the gate is off.
    pub(in crate::ui) empty_notice: Option<String>,
}

pub(in crate::ui) fn bar_state(jobs: &[Option<JobRowState>], online_enabled: bool) -> BarState {
    // Off means off: no online job can be running, so none is listed and the
    // footer says why instead of standing empty.
    if !online_enabled {
        return BarState {
            rows: Vec::new(),
            count_badge: None,
            empty_notice: Some(strings::text(strings::BACKGROUND_NO_ONLINE_JOBS)),
        };
    }
    let rows = jobs.iter().flatten().cloned().collect::<Vec<_>>();
    BarState {
        count_badge: (!rows.is_empty()).then(|| rows.len().to_string()),
        empty_notice: None,
        rows,
    }
}

/// Projects a cover-download batch onto its footer row. Only a running batch is
/// activity; a finished one is reported by the main window's scan card, which
/// this footer does not replace.
pub(in crate::ui) fn artwork_job(
    progress: crate::ui::cover_download_batch::BatchProgress,
) -> Option<JobRowState> {
    (progress.state == crate::ui::cover_download_batch::BatchState::Running).then(|| JobRowState {
        owner: JobOwner::Artwork,
        detail: strings::background_job_album_covers(progress.checked, progress.total),
        fraction: progress.fraction().clamp(0.0, 1.0),
    })
}

pub(in crate::ui) fn lyrics_job(
    progress: crate::ui::lyrics_batch::LyricsBatchProgress,
) -> Option<JobRowState> {
    (progress.state == crate::ui::lyrics_batch::LyricsBatchState::Running).then(|| JobRowState {
        owner: JobOwner::OnlineLyrics,
        detail: strings::background_job_missing_lyrics(progress.checked, progress.total),
        fraction: progress.fraction().clamp(0.0, 1.0),
    })
}

type CancelJob = Rc<dyn Fn(JobOwner)>;

struct BackgroundBarInner {
    root: gtk4::Box,
    count: gtk4::Label,
    rows_box: gtk4::Box,
    empty: gtk4::Label,
    scan_slot: gtk4::Box,
    jobs: RefCell<Vec<Option<JobRowState>>>,
    online_enabled: std::cell::Cell<bool>,
    on_cancel: RefCell<Option<CancelJob>>,
}

/// The footer itself. Cheap to clone — every handle points at one widget tree.
#[derive(Clone)]
pub(in crate::ui) struct BackgroundBar {
    inner: Rc<BackgroundBarInner>,
}

impl BackgroundBar {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        root.add_css_class(BAR_CLASS);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        let title = gtk4::Label::new(Some(&strings::text(strings::BACKGROUND_ACTIVITY)));
        title.set_xalign(0.0);
        title.add_css_class(TITLE_CLASS);
        let count = gtk4::Label::new(None);
        count.add_css_class(COUNT_CLASS);
        count.set_visible(false);
        header.append(&title);
        header.append(&count);

        // Hidden while empty, not merely childless: a visible empty box still
        // takes the parent's spacing on both sides, which reads as a gap
        // nobody put there.
        let rows_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        rows_box.set_visible(false);
        let scan_slot = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        scan_slot.set_visible(false);
        let empty = gtk4::Label::new(None);
        empty.set_xalign(0.0);
        empty.add_css_class(EMPTY_CLASS);
        empty.set_visible(false);

        root.append(&header);
        root.append(&rows_box);
        root.append(&scan_slot);
        root.append(&empty);

        let inner = Rc::new(BackgroundBarInner {
            root,
            count,
            rows_box,
            empty,
            scan_slot,
            jobs: RefCell::new(vec![None; JobOwner::ORDER.len()]),
            online_enabled: std::cell::Cell::new(true),
            on_cancel: RefCell::new(None),
        });
        let bar = Self { inner };
        bar.render();
        bar
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Widget {
        self.inner.root.upcast_ref()
    }

    /// The library scan keeps its own presentation; it is given a place in the
    /// footer rather than an overlay over the dialog head.
    pub(in crate::ui) fn adopt_scan_chrome(&self, line: &gtk4::Widget, chip: &gtk4::Widget) {
        line.set_halign(gtk4::Align::Fill);
        line.set_hexpand(true);
        chip.set_halign(gtk4::Align::Start);
        chip.set_margin_top(0);
        chip.set_margin_end(0);
        self.inner.scan_slot.append(line);
        self.inner.scan_slot.append(chip);
        self.inner.scan_slot.set_visible(true);
    }

    pub(in crate::ui) fn set_on_cancel(&self, callback: impl Fn(JobOwner) + 'static) {
        self.inner.on_cancel.replace(Some(Rc::new(callback)));
    }

    pub(in crate::ui) fn set_online_enabled(&self, enabled: bool) {
        if self.inner.online_enabled.replace(enabled) == enabled {
            return;
        }
        self.render();
    }

    /// Publishes one owner's state. Each owner writes only its own slot, so two
    /// jobs can never take each other's place.
    pub(in crate::ui) fn publish(&self, owner: JobOwner, job: Option<JobRowState>) {
        let index = JobOwner::ORDER
            .iter()
            .position(|candidate| *candidate == owner)
            .expect("every job owner is listed in the row order");
        {
            let mut jobs = self.inner.jobs.borrow_mut();
            if jobs[index] == job {
                return;
            }
            jobs[index] = job;
        }
        self.render();
    }

    fn state(&self) -> BarState {
        bar_state(&self.inner.jobs.borrow(), self.inner.online_enabled.get())
    }

    fn render(&self) {
        let state = self.state();
        while let Some(child) = self.inner.rows_box.first_child() {
            self.inner.rows_box.remove(&child);
        }
        for row in &state.rows {
            self.inner.rows_box.append(&self.job_row(row));
        }
        self.inner.rows_box.set_visible(!state.rows.is_empty());
        match &state.count_badge {
            Some(text) => {
                self.inner.count.set_label(text);
                self.inner.count.set_visible(true);
            }
            None => self.inner.count.set_visible(false),
        }
        match &state.empty_notice {
            Some(text) => {
                self.inner.empty.set_label(text);
                self.inner.empty.set_visible(true);
            }
            None => self.inner.empty.set_visible(false),
        }
    }

    fn job_row(&self, state: &JobRowState) -> gtk4::Box {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, COLUMN_SPACING_PX);

        let owner_title = state.owner.title();
        let owner = gtk4::Label::new(Some(&owner_title));
        owner.set_xalign(0.0);
        owner.set_width_request(OWNER_WIDTH_PX);
        owner.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        owner.add_css_class(OWNER_CLASS);

        let detail = gtk4::Label::new(Some(&state.detail));
        detail.set_xalign(0.0);
        detail.set_hexpand(true);
        // The one column that gives way. Everything else in the row is a fixed
        // width, so the description is what absorbs a narrow dialog — and it
        // ellipsizes rather than pushing the dialog wider. An ellipsizing label
        // asks for the ellipsis as its minimum (13 px measured), so this column
        // costs the footer almost nothing at its floor; a `width-chars` floor
        // here would only raise that minimum and eat the dialog's headroom.
        //
        // Ellipsized from the middle, not the end: what overflows here is a
        // translation, and the count it closes with is the half worth keeping.
        detail.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        detail.add_css_class(DETAIL_CLASS);

        let track = gtk4::ProgressBar::new();
        track.set_fraction(state.fraction.clamp(0.0, 1.0));
        track.set_valign(gtk4::Align::Center);
        // `TRACK_CLASS` carries the width. Adwaita gives a progress bar's
        // trough a 150 px `min-width`, and a smaller `width-request` loses to
        // it silently — the stylesheet is the only place that can lower it.
        track.add_css_class(TRACK_CLASS);

        // Fixed width, right-aligned: the row must not jump while it counts.
        let percent = gtk4::Label::new(Some(&format!("{}%", state.percent())));
        percent.set_xalign(1.0);
        percent.set_width_request(PERCENT_WIDTH_PX);
        percent.add_css_class(PERCENT_CLASS);

        let cancel = gtk4::Button::from_icon_name("window-close-symbolic");
        cancel.add_css_class("flat");
        cancel.add_css_class("circular");
        let cancel_label = strings::background_job_cancel(&owner_title);
        cancel.set_tooltip_text(Some(&cancel_label));
        cancel.update_property(&[gtk4::accessible::Property::Label(&cancel_label)]);
        {
            let owner = state.owner;
            // Through the shared inner: rows are built while progress replays,
            // which happens before the cancel handler is registered.
            let inner = Rc::downgrade(&self.inner);
            cancel.connect_clicked(move |_| {
                let Some(inner) = inner.upgrade() else {
                    return;
                };
                let callback = inner.on_cancel.borrow().clone();
                if let Some(callback) = callback {
                    callback(owner);
                }
            });
        }

        row.append(&owner);
        row.append(&detail);
        row.append(&track);
        row.append(&percent);
        row.append(&cancel);
        row
    }
}

pub(in crate::ui) fn css() -> String {
    format!(
        "/* --- Preferences footer: every background job named, in one place --- */ \
         .{BAR_CLASS} {{ \
           background-color: @sidebar_bg_color; \
           border-top: 1px solid alpha(@window_fg_color, 0.09); \
           padding: 12px 20px; }} \
         .{TITLE_CLASS} {{ \
           font-size: 0.8em; \
           font-weight: bold; \
           letter-spacing: 0.1em; \
           text-transform: uppercase; \
           color: alpha(@window_fg_color, 0.55); }} \
         .{COUNT_CLASS} {{ \
           font-size: 0.8em; \
           padding: 1px 7px; \
           border-radius: 9px; \
           background-color: alpha(@accent_color, 0.16); \
           color: @reprise_accent_text_color; }} \
         .{OWNER_CLASS} {{ color: alpha(@window_fg_color, 0.85); }} \
         .{DETAIL_CLASS} {{ color: alpha(@window_fg_color, 0.6); }} \
         .{PERCENT_CLASS} {{ font-size: 0.9em; color: alpha(@window_fg_color, 0.6); }} \
         .{EMPTY_CLASS} {{ color: alpha(@window_fg_color, 0.45); }} \
         .{TRACK_CLASS} {{ min-height: 4px; }} \
         .{TRACK_CLASS} trough {{ \
           min-height: 4px; \
           min-width: {TRACK_WIDTH_PX}px; \
           border-radius: 2px; \
           background-color: alpha(@window_fg_color, 0.12); }} \
         .{TRACK_CLASS} progress {{ \
           min-height: 4px; \
           border-radius: 2px; \
           background-image: none; \
           background-color: @accent_color; }}"
    )
}

impl super::PreferencesContext {
    /// Subscribes the footer to every job that may run behind this dialog.
    ///
    /// Each batch writes its own row and nothing else, so both are named and
    /// both are visible at once — the lyrics check no longer has to wait for
    /// Artwork to be switched off before it can be seen.
    pub(super) fn wire_background_bar(self: &Rc<Self>, bar: &BackgroundBar) {
        self.background_bar.replace(Some(bar.clone()));
        let alive = {
            let root = gtk4::glib::WeakRef::new();
            root.set(Some(bar.widget()));
            move || root.upgrade().is_some()
        };

        self.cover_batch.subscribe_progress(alive.clone(), {
            let bar = bar.clone();
            move |progress| bar.publish(JobOwner::Artwork, artwork_job(progress))
        });
        self.lyrics_batch.subscribe_progress(alive, {
            let bar = bar.clone();
            move |progress| bar.publish(JobOwner::OnlineLyrics, lyrics_job(progress))
        });

        let weak = Rc::downgrade(self);
        bar.set_on_cancel(move |owner| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            // A row's cancel belongs to that row's job and to nothing else.
            match owner {
                JobOwner::Artwork => context.cover_batch.cancel(),
                JobOwner::OnlineLyrics => context.lyrics_batch.cancel(),
            }
        });
        self.refresh_background_bar_gate();
    }

    /// Republishes the gate onto the footer. Called from the one funnel every
    /// module write already goes through, so the footer cannot drift from the
    /// switch that governs it.
    pub(in crate::ui) fn refresh_background_bar_gate(&self) {
        let Some(bar) = self.background_bar.borrow().clone() else {
            return;
        };
        bar.set_online_enabled(
            reprise_core::online_sources::is_enabled(&self.conn).unwrap_or(false),
        );
    }
}

#[cfg(test)]
#[path = "preference_background_bar_tests.rs"]
mod tests;
