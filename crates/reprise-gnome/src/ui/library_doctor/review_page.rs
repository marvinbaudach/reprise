use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::library_doctor::{
    DoctorApplyPlan, DoctorReviewFilter, DoctorReviewGroupId, DoctorReviewRowId,
    DoctorReviewRowState, DoctorReviewSession, DoctorScan, DoctorValue, DoctorWriteReport,
    DoctorWriteRowState,
};

use super::review_conflicts::{acknowledge_skipped_scan, ReviewConflicts, ReviewConflictsSlot};
use super::review_filter_bar::ReviewFilterBar;
use super::review_header::{
    album_header_factory, master_check_state, AlbumHeaderRegistry, OnSelect, ReviewColumnGroups,
    ReviewHeader,
};
use super::review_model::{
    available_categories, grouped_rows_for, layout_for_width, ReviewCategory, ReviewLayout,
    ReviewOutcome, ReviewRowModel, WIDE_BREAKPOINT,
};
use super::review_snapshot::{review_ready_count, splice_selection_rows, ReviewSnapshot};
use crate::ui::strings;

type OnEdit = Rc<dyn Fn(&[i64])>;

struct ReviewState {
    conn: Rc<Db>,
    scan: DoctorScan,
    session: Rc<RefCell<DoctorReviewSession>>,
    store: gio::ListStore,
    filter: gtk4::CustomFilter,
    sorted: gtk4::SortListModel,
    category: Rc<Cell<Option<ReviewCategory>>>,
    selection: gtk4::SingleSelection,
    content: gtk4::Stack,
    filter_bar: ReviewFilterBar,
    stale_notice: gtk4::Box,
    stale_notice_label: gtk4::Label,
    rescan: gtk4::Button,
    apply: gtk4::Button,
    change_summary: gtk4::Label,
    select_all: gtk4::CheckButton,
    select_all_handler: RefCell<Option<glib::SignalHandlerId>>,
    controls_locked: Cell<bool>,
    full_refresh_only: bool,
    layout: Rc<Cell<ReviewLayout>>,
    column_groups: ReviewColumnGroups,
    outcomes: RefCell<HashMap<DoctorReviewRowId, ReviewOutcome>>,
    ready_count: Cell<usize>,
    snapshot: Rc<RefCell<ReviewSnapshot>>,
    album_headers: AlbumHeaderRegistry,
    conflicts: ReviewConflictsSlot,
    #[cfg(test)]
    selection_requests: Cell<u32>,
    on_reviewed: Rc<dyn Fn()>,
}

impl ReviewState {
    fn refresh(self: &Rc<Self>) {
        let full_started = Instant::now();
        let selected = self.selection.selected();
        let mut session = self.session.borrow_mut();
        let categories = available_categories(&session);
        if self
            .category
            .get()
            .is_some_and(|active| !categories.contains(&active))
        {
            self.category.set(None);
            session.set_category_filter(None);
        }
        self.filter_bar.set_categories(&categories);
        let stale_notice = review_stale_notice(&session);
        let ready_count = review_ready_count(&session);
        self.ready_count.set(ready_count);
        let grouped_rows_started = Instant::now();
        let snapshot = ReviewSnapshot::from_rows(grouped_rows_for(
            &self.scan,
            &session,
            &self.outcomes.borrow(),
        ));
        let objects = snapshot
            .rows
            .iter()
            .cloned()
            .map(|row| glib::BoxedAnyObject::new(row).upcast::<glib::Object>())
            .collect::<Vec<_>>();
        tracing::debug!(
            stage = "grouped_rows_for",
            elapsed_us = grouped_rows_started.elapsed().as_micros(),
            "DOCTOR_REVIEW_REFRESH stage"
        );
        drop(session);
        self.stale_notice.set_visible(stale_notice.is_some());
        self.stale_notice_label
            .set_label(stale_notice.as_deref().unwrap_or_default());
        let old_row_count = self.snapshot.borrow().rows.len();
        *self.snapshot.borrow_mut() = snapshot;
        let splice_started = Instant::now();
        self.store.splice(
            0,
            u32::try_from(old_row_count).expect("review row count fits u32"),
            &objects,
        );
        tracing::debug!(
            stage = "store.splice",
            elapsed_us = splice_started.elapsed().as_micros(),
            "DOCTOR_REVIEW_REFRESH stage"
        );
        let conflicts_started = Instant::now();
        self.refresh_conflicts();
        tracing::debug!(
            stage = "refresh_conflicts",
            elapsed_us = conflicts_started.elapsed().as_micros(),
            "DOCTOR_REVIEW_REFRESH stage"
        );
        let count = self.sorted.n_items();
        self.content
            .set_visible_child_name(if count == 0 { "empty" } else { "rows" });
        if count > 0 && selected != gtk4::INVALID_LIST_POSITION {
            self.selection.set_selected(selected.min(count - 1));
        }
        self.refresh_action_summary(ready_count);
        let aggregate_started = Instant::now();
        self.refresh_filter_summary();
        self.refresh_master_check();
        tracing::debug!(
            stage = "aggregate",
            elapsed_us = aggregate_started.elapsed().as_micros(),
            "DOCTOR_REVIEW_REFRESH stage"
        );
        tracing::debug!(
            path = "full",
            rows = objects.len(),
            elapsed_us = full_started.elapsed().as_micros(),
            "DOCTOR_REVIEW_REFRESH path"
        );
    }

    #[cfg(test)]
    fn visible_rows(&self) -> Vec<ReviewRowModel> {
        (0..self.sorted.n_items())
            .filter_map(|position| row_at(&self.sorted, position))
            .collect()
    }

    fn refresh_filter_summary(&self) {
        let snapshot = self.snapshot.borrow();
        let totals = snapshot.totals;
        debug_assert_eq!(totals.albums, snapshot.albums.len());
        self.filter_bar.set_summary(totals.changes, totals.albums);
    }

    fn refresh_master_check(&self) {
        let totals = self.snapshot.borrow().totals;
        let check = master_check_state(totals.selected, totals.selectable);
        let handler = self.select_all_handler.borrow_mut().take();
        if let Some(handler) = handler.as_ref() {
            self.select_all.block_signal(handler);
        }
        self.select_all.set_active(check.active);
        self.select_all.set_inconsistent(check.inconsistent);
        self.select_all
            .set_sensitive(check.sensitive && !self.controls_locked.get());
        if let Some(handler) = handler.as_ref() {
            self.select_all.unblock_signal(handler);
        }
        *self.select_all_handler.borrow_mut() = handler;
    }

    fn refresh_action_summary(&self, ready_count: usize) {
        let summary = self.session.borrow().summary();
        self.apply
            .set_label(&strings::doctor_apply_changes(summary.tag_change_count));
        self.apply.set_sensitive(summary.tag_change_count > 0);
        self.change_summary.set_label(&review_footer_summary(
            summary,
            self.category.get(),
            ready_count,
        ));
    }

    fn set_selected(self: &Rc<Self>, row_ids: &[DoctorReviewRowId], selected: bool) {
        #[cfg(test)]
        self.selection_requests
            .set(self.selection_requests.get() + 1);
        let mut session = self.session.borrow_mut();
        let mut session_changed = false;
        for row_id in row_ids {
            match session.set_selected(*row_id, selected) {
                Ok(()) => session_changed = true,
                Err(error) => {
                    tracing::warn!(%error, "could not update Library Doctor review selection");
                }
            }
        }
        drop(session);
        self.apply_selection(session_changed);
    }

    fn apply_selection(self: &Rc<Self>, session_changed: bool) {
        if !session_changed {
            return;
        }
        if self.full_refresh_only {
            self.refresh();
            return;
        }
        let started = Instant::now();
        let changed = {
            let snapshot = self.snapshot.borrow();
            let session = self.session.borrow();
            snapshot.selection_diff(&session)
        };
        if changed.is_empty() {
            tracing::debug!(
                path = "selection",
                touched = 0,
                elapsed_us = started.elapsed().as_micros(),
                "DOCTOR_REVIEW_REFRESH path"
            );
            return;
        }
        let row_count = self.snapshot.borrow().rows.len();
        let affected = changed
            .iter()
            .map(|(_, row)| row.album_key.clone())
            .collect::<HashSet<_>>();
        let snapshot = self.snapshot.borrow().clone().with_selection(&changed);
        let affected_albums = affected
            .iter()
            .filter_map(|album_key| {
                snapshot
                    .albums
                    .get(album_key)
                    .cloned()
                    .map(|counts| (album_key.clone(), counts))
            })
            .collect::<HashMap<_, _>>();
        *self.snapshot.borrow_mut() = snapshot;
        splice_selection_rows(&self.store, &changed, row_count);
        self.refresh_action_summary(self.ready_count.get());
        self.refresh_master_check();
        self.album_headers.push_selection(&affected_albums);
        tracing::debug!(
            path = "selection",
            touched = changed.len(),
            elapsed_us = started.elapsed().as_micros(),
            "DOCTOR_REVIEW_REFRESH path"
        );
    }

    fn toggle_position(self: &Rc<Self>, position: u32) {
        let Some(boxed) = self
            .selection
            .item(position)
            .and_then(|object| object.downcast::<glib::BoxedAnyObject>().ok())
        else {
            return;
        };
        let model = boxed.borrow::<ReviewRowModel>();
        if model.selectable_row_ids.is_empty() {
            return;
        }
        self.set_selected(&model.selectable_row_ids, !model.row.selected);
    }

    fn set_category(self: &Rc<Self>, category: Option<ReviewCategory>) {
        self.category.set(category);
        self.session
            .borrow_mut()
            .set_category_filter(category.map(ReviewCategory::problem_classes));
        self.filter.changed(gtk4::FilterChange::Different);
        self.refresh();
    }

    fn set_remote_visible(self: &Rc<Self>, visible: bool) {
        self.session.borrow_mut().set_remote_visible(visible);
        self.refresh();
    }

    fn set_layout(self: &Rc<Self>, layout: ReviewLayout) {
        if self.layout.replace(layout) != layout {
            self.column_groups
                .set_wide(matches!(layout, ReviewLayout::Wide));
            self.refresh();
        }
    }

    fn mark_paths_stale(self: &Rc<Self>, paths: &[PathBuf]) {
        let track_ids = self
            .scan
            .tracks
            .iter()
            .filter(|track| paths.contains(&track.reference.path))
            .map(|track| track.reference.track_id)
            .collect::<Vec<_>>();
        let ids = self
            .session
            .borrow()
            .rows()
            .iter()
            .filter(|row| track_ids.contains(&row.track_id))
            .map(|row| row.id)
            .collect::<Vec<_>>();
        let mut session = self.session.borrow_mut();
        for id in ids {
            let _ = session.mark_state(id, DoctorReviewRowState::Stale);
        }
        drop(session);
        self.refresh();
    }

    fn refresh_conflicts(self: &Rc<Self>) {
        let groups = {
            let session = self.session.borrow();
            session
                .groups()
                .iter()
                .filter(|group| session.group_matches_category_filter(group))
                .cloned()
                .collect::<Vec<_>>()
        };
        let fingerprint = ReviewConflictsSlot::fingerprint(&groups);
        let row_count =
            u32::try_from(self.snapshot.borrow().rows.len()).expect("review row count fits u32");
        let panel_present = self.store.n_items() == row_count + 1
            && self
                .store
                .item(row_count)
                .is_some_and(|item| item.is::<gtk4::Widget>());
        if panel_present {
            self.conflicts.relocate(row_count);
        }
        if groups.is_empty() {
            let tracked = self.conflicts.clear();
            if panel_present {
                debug_assert_eq!(tracked, Some(row_count));
                self.store.splice(row_count, 1, &[] as &[glib::Object]);
            }
            return;
        }
        if self.conflicts.is_current(&fingerprint) && panel_present {
            self.conflicts.remember(fingerprint, row_count);
            return;
        }
        let weak = Rc::downgrade(self);
        let on_choose = Rc::new(move |group_id, value: &DoctorValue| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if let Err(error) = state.session.borrow_mut().choose_candidate(group_id, value) {
                tracing::warn!(%error, "could not choose Library Doctor spelling");
                return;
            }
            state.refresh();
        }) as Rc<dyn Fn(DoctorReviewGroupId, &DoctorValue)>;
        let panel = ReviewConflicts::new(&groups, &self.scan.unresolved_groups, &on_choose);
        {
            let weak = Rc::downgrade(self);
            panel.skip.connect_clicked(move |_| {
                if let Some(state) = weak.upgrade() {
                    state.skip_all_conflicts();
                }
            });
        }
        if panel_present {
            debug_assert_eq!(self.conflicts.index(), Some(row_count));
            self.store.splice(row_count, 1, &[panel.root]);
        } else {
            self.conflicts.clear();
            debug_assert_eq!(self.store.n_items(), row_count);
            self.store.append(&panel.root);
        }
        self.conflicts.remember(fingerprint, row_count);
    }

    fn skip_all_conflicts(self: &Rc<Self>) {
        if let Err(error) = acknowledge_skipped_scan(&self.conn, self.scan.id) {
            tracing::warn!(%error, "could not acknowledge skipped Library Doctor conflicts");
            return;
        }
        self.session.borrow_mut().clear_group_choices();
        (self.on_reviewed)();
        self.refresh();
    }

    fn apply_report(self: &Rc<Self>, report: &DoctorWriteReport) {
        let mut outcomes = self.outcomes.borrow_mut();
        for row in &report.rows {
            let Some(row_id) = row.row_id else {
                continue;
            };
            outcomes.insert(
                row_id,
                ReviewOutcome {
                    state: row.state,
                    error: row.error.clone(),
                },
            );
            let transition = outcome_transition(row.state);
            if let Some(selected) = transition.selected {
                let _ = self.session.borrow_mut().set_selected(row_id, selected);
            }
            if let Some(state) = transition.review_state {
                let _ = self.session.borrow_mut().mark_state(row_id, state);
            }
        }
        drop(outcomes);
        self.refresh();
    }
}

#[cfg(test)]
fn row_at(model: &gtk4::SortListModel, position: u32) -> Option<ReviewRowModel> {
    let object = model
        .item(position)?
        .downcast::<glib::BoxedAnyObject>()
        .ok()?;
    let row = object.borrow::<ReviewRowModel>().clone();
    Some(row)
}

/// What the header states: the changes on screen and the albums they sit in.
///
/// This is the inventory, not the selection — the (possibly filtered) rows the
/// page is showing. Unchecking a row leaves it standing, which is the whole
/// point of the split: the header answers "what is here", the footer and the
/// Apply button answer "what will be written". Mixing the two produced
/// "1 changes · 2 albums", where the first number followed the checkbox and the
/// second did not.
#[cfg(test)]
fn review_header_counts(rows: &[ReviewRowModel]) -> (usize, usize) {
    let totals = ReviewSnapshot::from_rows(rows.to_vec()).totals;
    (totals.changes, totals.albums)
}

fn review_stale_notice(session: &DoctorReviewSession) -> Option<String> {
    let count = session
        .rows()
        .iter()
        .filter(|row| session.category_filter_matches(row.problem_class))
        .filter(|row| row.state == DoctorReviewRowState::Stale)
        .count();
    (count > 0).then(|| strings::doctor_stale_notice(count))
}

fn review_footer_summary(
    summary: reprise_core::library_doctor::DoctorReviewSummary,
    category: Option<ReviewCategory>,
    ready_count: usize,
) -> String {
    category.map_or_else(
        || strings::doctor_apply_summary(summary.tag_change_count, ready_count, summary.file_count),
        |category| {
            strings::doctor_filter_scope(
                summary.tag_change_count,
                summary.total_tag_change_count,
                &strings::text(category.label()),
            )
        },
    )
}

fn compare_rows(left: &glib::Object, right: &glib::Object, section_only: bool) -> gtk4::Ordering {
    let left = left.downcast_ref::<glib::BoxedAnyObject>();
    let right = right.downcast_ref::<glib::BoxedAnyObject>();
    let (Some(left), Some(right)) = (left, right) else {
        return match (left.is_some(), right.is_some()) {
            (true, false) => gtk4::Ordering::Smaller,
            (false, true) => gtk4::Ordering::Larger,
            _ => gtk4::Ordering::Equal,
        };
    };
    let left = left.borrow::<ReviewRowModel>();
    let right = right.borrow::<ReviewRowModel>();
    let ordering = if section_only {
        left.album_position.cmp(&right.album_position)
    } else {
        (left.album_position, left.row_position).cmp(&(right.album_position, right.row_position))
    };
    match ordering {
        std::cmp::Ordering::Less => gtk4::Ordering::Smaller,
        std::cmp::Ordering::Equal => gtk4::Ordering::Equal,
        std::cmp::Ordering::Greater => gtk4::Ordering::Larger,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OutcomeTransition {
    selected: Option<bool>,
    review_state: Option<DoctorReviewRowState>,
}

const fn outcome_transition(state: DoctorWriteRowState) -> OutcomeTransition {
    match state {
        DoctorWriteRowState::Applied | DoctorWriteRowState::Reverted => OutcomeTransition {
            selected: Some(false),
            review_state: None,
        },
        DoctorWriteRowState::Conflict => OutcomeTransition {
            selected: None,
            review_state: Some(DoctorReviewRowState::Conflict),
        },
        DoctorWriteRowState::Unavailable => OutcomeTransition {
            selected: None,
            review_state: Some(DoctorReviewRowState::Stale),
        },
        DoctorWriteRowState::Cancelled | DoctorWriteRowState::Failed => OutcomeTransition {
            selected: None,
            review_state: None,
        },
    }
}

pub(super) struct LibraryDoctorReviewPage {
    navigation_page: adw::NavigationPage,
    state: Rc<ReviewState>,
    rows: gtk4::ListView,
}

impl LibraryDoctorReviewPage {
    pub(super) fn new(
        conn: &Rc<Db>,
        _parent: &adw::ApplicationWindow,
        scan: &DoctorScan,
        _on_remote_changed: Rc<dyn Fn(bool)>,
        on_reviewed: Rc<dyn Fn()>,
        on_edit: &OnEdit,
    ) -> Rc<Self> {
        let session = Rc::new(RefCell::new(DoctorReviewSession::from_scan(
            scan.clone(),
            DoctorReviewFilter::NeedsReview,
        )));
        let categories = available_categories(&session.borrow());
        let category = Rc::new(Cell::new(None::<ReviewCategory>));
        let filter_session = session.clone();
        let filter = gtk4::CustomFilter::new(move |object| {
            let Some(boxed) = object.downcast_ref::<glib::BoxedAnyObject>() else {
                return object.is::<gtk4::Widget>();
            };
            let model = boxed.borrow::<ReviewRowModel>();
            filter_session
                .borrow()
                .category_filter_matches(model.row.problem_class)
        });
        let store = gio::ListStore::new::<glib::Object>();
        let filtered = gtk4::FilterListModel::new(Some(store.clone()), Some(filter.clone()));
        let sorter = gtk4::CustomSorter::new(|left, right| compare_rows(left, right, false));
        let sorted = gtk4::SortListModel::new(Some(filtered.clone()), Some(sorter));
        let section_sorter = gtk4::CustomSorter::new(|left, right| compare_rows(left, right, true));
        sorted.set_section_sorter(Some(&section_sorter));
        let selection = gtk4::SingleSelection::builder()
            .model(&sorted)
            .autoselect(false)
            .can_unselect(true)
            .build();
        let rows = gtk4::ListView::builder()
            .model(&selection)
            .single_click_activate(false)
            .build();
        let header = ReviewHeader::new();
        let filter_bar = ReviewFilterBar::new(&categories);
        let stale_notice_label = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .wrap(true)
            .build();
        let rescan = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_SCAN_AGAIN))
            .css_classes(["flat"])
            .build();
        let stale_notice = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        stale_notice.add_css_class("doctor-review-stale");
        stale_notice.append(&stale_notice_label);
        stale_notice.append(&rescan);
        let empty = adw::StatusPage::builder()
            .icon_name(crate::ui::icons::DONE)
            .title(strings::text(strings::DOCTOR_NO_CHANGES))
            .description(strings::text(strings::DOCTOR_NO_CHANGES_DESCRIPTION))
            .build();
        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .child(&rows)
            .build();
        let content = gtk4::Stack::new();
        content.set_vexpand(true);
        content.add_named(&scrolled, Some("rows"));
        content.add_named(&empty, Some("empty"));
        let apply = gtk4::Button::builder()
            .css_classes(["suggested-action", "pill"])
            .build();
        let change_summary = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["doctor-review-footer-summary"])
            .build();
        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        footer.add_css_class("doctor-review-footer");
        apply.add_css_class("doctor-review-apply");
        change_summary.set_hexpand(true);
        footer.append(&change_summary);
        footer.append(&apply);
        let layout = Rc::new(Cell::new(ReviewLayout::Wide));
        let album_headers = AlbumHeaderRegistry::default();
        let full_refresh_only =
            std::env::var("REPRISE_DOCTOR_FULL_REFRESH").is_ok_and(|value| value == "1");
        let state = Rc::new(ReviewState {
            conn: conn.clone(),
            scan: scan.clone(),
            session,
            store,
            filter,
            sorted,
            category,
            selection,
            content,
            filter_bar,
            stale_notice,
            stale_notice_label,
            rescan,
            apply,
            change_summary,
            select_all: header.select_all.clone(),
            select_all_handler: RefCell::new(None),
            controls_locked: Cell::new(false),
            full_refresh_only,
            layout,
            column_groups: header.groups.clone(),
            outcomes: RefCell::new(HashMap::new()),
            ready_count: Cell::new(0),
            snapshot: Rc::new(RefCell::new(ReviewSnapshot::default())),
            album_headers,
            conflicts: ReviewConflictsSlot::default(),
            #[cfg(test)]
            selection_requests: Cell::new(0),
            on_reviewed,
        });
        let on_select = {
            let state = state.clone();
            Rc::new(move |row_ids: &[DoctorReviewRowId], selected| {
                state.set_selected(row_ids, selected);
            }) as OnSelect
        };
        rows.set_factory(Some(&super::review_row::factory(
            &on_select,
            on_edit,
            &header.groups,
            &state.layout,
        )));
        rows.set_header_factory(Some(&album_header_factory(
            &state.sorted,
            &on_select,
            &state.snapshot,
            &state.album_headers,
        )));
        {
            let state = state.clone();
            rows.connect_activate(move |_, position| state.toggle_position(position));
        }
        {
            let callback_state = state.clone();
            state.filter_bar.connect_changed(Rc::new(move |category| {
                callback_state.set_category(category);
            }));
        }

        {
            let handler = header.select_all.connect_toggled(glib::clone!(
                #[weak]
                state,
                move |button| {
                    if button.is_active() {
                        state.session.borrow_mut().all();
                    } else {
                        state.session.borrow_mut().none();
                    }
                    state.apply_selection(true);
                }
            ));
            *state.select_all_handler.borrow_mut() = Some(handler);
        }
        let page_content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        page_content.append(&state.filter_bar.root);
        page_content.append(&state.stale_notice);
        page_content.append(&header.root);
        page_content.append(&state.content);
        page_content.append(&footer);
        let responsive = adw::BreakpointBin::new();
        responsive.set_child(Some(&page_content));
        let condition = adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            f64::from(WIDE_BREAKPOINT),
            adw::LengthUnit::Px,
        );
        let breakpoint = adw::Breakpoint::new(condition);
        breakpoint.add_setter(&header.labels, "visible", Some(&false.to_value()));
        breakpoint.add_setter(&header.select_all_label, "visible", Some(&true.to_value()));
        {
            let state = state.clone();
            breakpoint.connect_apply(move |_| {
                state.set_layout(layout_for_width(WIDE_BREAKPOINT - 1));
            });
        }
        {
            let state = state.clone();
            breakpoint.connect_unapply(move |_| {
                state.set_layout(layout_for_width(WIDE_BREAKPOINT));
            });
        }
        responsive.add_breakpoint(breakpoint);
        let navigation_page = adw::NavigationPage::builder()
            .title(strings::text(strings::DOCTOR_REVIEW_TITLE))
            .tag("library-doctor-review")
            .child(&responsive)
            .build();
        let page = Rc::new(Self {
            navigation_page,
            state,
            rows,
        });
        page.state.refresh();
        page
    }

    pub(super) fn navigation_page(&self) -> &adw::NavigationPage {
        &self.navigation_page
    }

    pub(super) fn mark_paths_stale(&self, paths: &[PathBuf]) {
        self.state.mark_paths_stale(paths);
    }

    pub(super) fn set_remote_active(&self, active: bool) {
        self.state.set_remote_visible(active);
    }

    pub(super) fn connect_apply(&self, callback: impl Fn(DoctorApplyPlan) + 'static) {
        self.state.apply.connect_clicked(glib::clone!(
            #[weak(rename_to = state)]
            self.state,
            move |_| {
                let plan = state.session.borrow().freeze_plan();
                if plan.tag_change_count() > 0 {
                    callback(plan);
                }
            }
        ));
    }

    pub(super) fn connect_rescan(&self, callback: impl Fn() + 'static) {
        self.state.rescan.connect_clicked(move |_| callback());
    }

    pub(super) fn set_running(&self, running: bool) {
        self.state.controls_locked.set(running);
        self.rows.set_sensitive(!running);
        self.state.select_all.set_sensitive(!running);
        self.state.filter_bar.set_sensitive(!running);
        self.state.rescan.set_sensitive(!running);
        if running {
            self.state.apply.set_sensitive(false);
        } else {
            self.state.refresh();
        }
    }

    pub(super) fn set_write_report(&self, report: &DoctorWriteReport) {
        self.state.apply_report(report);
    }
}

#[cfg(test)]
#[path = "review_page_perf_tests.rs"]
mod review_page_perf_tests;

#[cfg(test)]
#[path = "review_refresh_tests.rs"]
mod review_refresh_tests;

#[cfg(test)]
#[path = "review_page_tests.rs"]
mod tests;
