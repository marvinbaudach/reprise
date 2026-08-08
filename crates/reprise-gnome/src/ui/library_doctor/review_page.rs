use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::library_doctor::{
    DoctorApplyPlan, DoctorReviewFilter, DoctorReviewGroupId, DoctorReviewRowId,
    DoctorReviewRowState, DoctorReviewSession, DoctorScan, DoctorValue, DoctorWriteReport,
    DoctorWriteRowState, LibraryDoctor,
};

use super::review_conflicts::ReviewConflicts;
use super::review_filter_bar::ReviewFilterBar;
use super::review_header::{album_header_factory, OnSelect, ReviewColumnGroups, ReviewHeader};
use super::review_model::{
    available_categories, grouped_rows_for, layout_for_width, ReviewCategory, ReviewLayout,
    ReviewOutcome, ReviewRowModel, WIDE_BREAKPOINT,
};
use crate::ui::strings;

type OnEdit = Rc<dyn Fn(&[i64])>;

struct ReviewState {
    conn: Rc<Db>,
    scan: DoctorScan,
    session: RefCell<DoctorReviewSession>,
    store: gio::ListStore,
    filter: gtk4::CustomFilter,
    sorted: gtk4::SortListModel,
    category: Rc<Cell<Option<ReviewCategory>>>,
    selection: gtk4::SingleSelection,
    content: gtk4::Stack,
    filter_bar: ReviewFilterBar,
    apply: gtk4::Button,
    change_summary: gtk4::Label,
    layout: Rc<Cell<ReviewLayout>>,
    column_groups: ReviewColumnGroups,
    outcomes: RefCell<HashMap<DoctorReviewRowId, ReviewOutcome>>,
    on_reviewed: Rc<dyn Fn()>,
}

impl ReviewState {
    fn refresh(self: &Rc<Self>) {
        let selected = self.selection.selected();
        let session = self.session.borrow();
        let categories = available_categories(&session);
        if self
            .category
            .get()
            .is_some_and(|active| !categories.contains(&active))
        {
            self.category.set(None);
        }
        self.filter_bar.set_categories(&categories);
        let objects = grouped_rows_for(&self.scan, &session, &self.outcomes.borrow())
            .into_iter()
            .map(|row| glib::BoxedAnyObject::new(row).upcast::<glib::Object>())
            .collect::<Vec<_>>();
        self.store.splice(0, self.store.n_items(), &objects);
        drop(session);
        self.refresh_conflicts();
        self.filter.changed(gtk4::FilterChange::Different);
        let count = self.sorted.n_items();
        self.content
            .set_visible_child_name(if count == 0 { "empty" } else { "rows" });
        if count > 0 && selected != gtk4::INVALID_LIST_POSITION {
            self.selection.set_selected(selected.min(count - 1));
        }
        let summary = self.session.borrow().summary();
        self.apply
            .set_label(&strings::doctor_apply_changes(summary.tag_change_count));
        self.apply.set_sensitive(summary.tag_change_count > 0);
        self.change_summary
            .set_label(&strings::doctor_apply_summary(
                summary.tag_change_count,
                summary.file_count,
            ));
        self.refresh_filter_summary();
    }

    fn visible_rows(&self) -> Vec<ReviewRowModel> {
        (0..self.sorted.n_items())
            .filter_map(|position| row_at(&self.sorted, position))
            .collect()
    }

    fn refresh_filter_summary(&self) {
        let rows = self.visible_rows();
        let changes = rows.iter().map(|row| row.selected_change_count).sum();
        let albums = rows
            .iter()
            .map(|row| row.album_key.as_str())
            .collect::<HashSet<_>>()
            .len();
        self.filter_bar.set_summary(changes, albums);
    }

    fn set_selected(self: &Rc<Self>, row_ids: &[DoctorReviewRowId], selected: bool) {
        let mut session = self.session.borrow_mut();
        for row_id in row_ids {
            if let Err(error) = session.set_selected(*row_id, selected) {
                tracing::warn!(%error, "could not update Library Doctor review selection");
            }
        }
        drop(session);
        self.refresh();
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
        self.set_selected(&model.row_ids, !model.row.selected);
    }

    fn set_category(&self, category: Option<ReviewCategory>) {
        self.category.set(category);
        self.filter.changed(gtk4::FilterChange::Different);
        self.content
            .set_visible_child_name(if self.sorted.n_items() == 0 {
                "empty"
            } else {
                "rows"
            });
        self.refresh_filter_summary();
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
        if self.session.borrow().groups().is_empty() {
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
        let panel = ReviewConflicts::new(
            self.session.borrow().groups(),
            &self.scan.unresolved_groups,
            &on_choose,
        );
        {
            let weak = Rc::downgrade(self);
            panel.skip.connect_clicked(move |_| {
                if let Some(state) = weak.upgrade() {
                    state.skip_all_conflicts();
                }
            });
        }
        self.store.append(&panel.root);
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

fn acknowledge_skipped_scan(db: &Db, scan_id: i64) -> Result<(), String> {
    LibraryDoctor::new(db)
        .set_reviewed_scan(scan_id)
        .map_err(|error| error.to_string())
}

fn row_at(model: &gtk4::SortListModel, position: u32) -> Option<ReviewRowModel> {
    let object = model
        .item(position)?
        .downcast::<glib::BoxedAnyObject>()
        .ok()?;
    let row = object.borrow::<ReviewRowModel>().clone();
    Some(row)
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
    all: gtk4::Button,
    none: gtk4::Button,
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
        let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
        let categories = available_categories(&session);
        let category = Rc::new(Cell::new(None::<ReviewCategory>));
        let active_category = category.clone();
        let filter = gtk4::CustomFilter::new(move |object| {
            let Some(boxed) = object.downcast_ref::<glib::BoxedAnyObject>() else {
                return object.is::<gtk4::Widget>();
            };
            let model = boxed.borrow::<ReviewRowModel>();
            active_category
                .get()
                .is_none_or(|category| category.matches(model.row.problem_class))
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
            .css_classes(["caption", "dim-label"])
            .build();
        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        footer.set_margin_top(12);
        footer.set_margin_bottom(12);
        footer.set_margin_start(18);
        footer.set_margin_end(18);
        change_summary.set_hexpand(true);
        footer.append(&change_summary);
        footer.append(&apply);
        let layout = Rc::new(Cell::new(ReviewLayout::Wide));
        let state = Rc::new(ReviewState {
            conn: conn.clone(),
            scan: scan.clone(),
            session: RefCell::new(session),
            store,
            filter,
            sorted,
            category,
            selection,
            content,
            filter_bar,
            apply,
            change_summary,
            layout,
            column_groups: header.groups.clone(),
            outcomes: RefCell::new(HashMap::new()),
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
        rows.set_header_factory(Some(&album_header_factory(&state.sorted, &on_select)));
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

        let all = gtk4::Button::with_label(&strings::text(strings::DOCTOR_ALL));
        let none = gtk4::Button::with_label(&strings::text(strings::DOCTOR_NONE));
        all.add_css_class("doctor-review-header-action");
        none.add_css_class("doctor-review-header-action");
        {
            let state = state.clone();
            all.connect_clicked(move |_| {
                state.session.borrow_mut().all();
                state.refresh();
            });
        }
        {
            let state = state.clone();
            none.connect_clicked(move |_| {
                state.session.borrow_mut().none();
                state.refresh();
            });
        }
        let presets = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        presets.append(&all);
        presets.append(&none);
        let top_bar = adw::HeaderBar::new();
        let title = adw::WindowTitle::new(&strings::text(strings::DOCTOR_REVIEW_TITLE), "");
        top_bar.set_title_widget(Some(&title));
        top_bar.pack_end(&presets);

        let page_content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        page_content.set_margin_top(12);
        page_content.append(&state.filter_bar.root);
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
        breakpoint.add_setter(&header.root, "visible", Some(&false.to_value()));
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
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&top_bar);
        toolbar.set_content(Some(&responsive));
        let navigation_page = adw::NavigationPage::builder()
            .title(strings::text(strings::DOCTOR_REVIEW_TITLE))
            .tag("library-doctor-review")
            .child(&toolbar)
            .build();
        let page = Rc::new(Self {
            navigation_page,
            state,
            rows,
            all,
            none,
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
        let state = self.state.clone();
        self.state.apply.connect_clicked(move |_| {
            let plan = state.session.borrow().freeze_plan();
            if plan.tag_change_count() > 0 {
                callback(plan);
            }
        });
    }

    pub(super) fn set_running(&self, running: bool) {
        self.rows.set_sensitive(!running);
        self.all.set_sensitive(!running);
        self.none.set_sensitive(!running);
        self.state.filter_bar.set_sensitive(!running);
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
#[path = "review_page_tests.rs"]
mod tests;
