//! The shared column editor surface used by every GTK table.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;

use super::editor_dnd;
use super::EditorModel;
use crate::ui::strings;

pub(in crate::ui) const SMOKE_ENV: &str = "REPRISE_SMOKE_COLUMN_LAYOUT_EDITOR";

struct EditorState {
    model: Rc<dyn EditorModel>,
    list: gtk4::ListBox,
}

pub(in crate::ui) struct EditorSurface {
    pub toolbar: adw::ToolbarView,
    pub list: gtk4::ListBox,
}

pub(in crate::ui) struct EditorDialogSurface {
    pub dialog: adw::Dialog,
    pub list: gtk4::ListBox,
}

struct SmokeExercise {
    hidden_id: String,
    moved_id: String,
    target_id: String,
}

fn apply_smoke_exercise(model: &dyn EditorModel) -> Option<SmokeExercise> {
    let columns = model.columns();
    let [hidden, moved, ..] = columns.as_slice() else {
        return None;
    };
    model.set_visible(&hidden.id, false);
    model.move_column(&moved.id, &hidden.id, false);
    Some(SmokeExercise {
        hidden_id: hidden.id.clone(),
        moved_id: moved.id.clone(),
        target_id: hidden.id.clone(),
    })
}

impl EditorState {
    fn move_by(self: &Rc<Self>, id: &str, offset: isize) {
        let columns = self.model.columns();
        let Some(index) = columns.iter().position(|column| column.id == id) else {
            return;
        };
        let Some(target_index) = index.checked_add_signed(offset) else {
            return;
        };
        let Some(target) = columns.get(target_index) else {
            return;
        };
        self.model.move_column(id, &target.id, offset.is_positive());
        self.rebuild();
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for descriptor in self.model.columns() {
            self.list.append(&build_row(self, descriptor));
        }
    }
}

fn build_row(state: &Rc<EditorState>, descriptor: super::ColumnDescriptor) -> adw::ActionRow {
    let id = descriptor.id;
    let row = adw::ActionRow::builder().title(descriptor.label).build();
    row.add_css_class(editor_dnd::row_class());
    let handle = gtk4::Image::from_icon_name("list-drag-handle-symbolic");
    handle.add_css_class(editor_dnd::handle_class());
    handle.set_tooltip_text(Some(&strings::text(strings::DRAG_TO_REORDER)));
    row.add_prefix(&handle);

    let toggle = gtk4::Switch::builder()
        .active(state.model.is_visible(&id))
        .valign(gtk4::Align::Center)
        .build();
    let state_weak = Rc::downgrade(state);
    let toggle_id = id.clone();
    toggle.connect_active_notify(move |toggle| {
        let Some(state) = state_weak.upgrade() else {
            return;
        };
        state.model.set_visible(&toggle_id, toggle.is_active());
        state.rebuild();
    });
    row.add_suffix(&toggle);
    row.set_activatable_widget(Some(&toggle));

    row.update_property(&[
        gtk4::accessible::Property::Description(&strings::text(strings::DRAG_TO_REORDER)),
        gtk4::accessible::Property::KeyShortcuts("Alt+ArrowUp Alt+ArrowDown"),
    ]);
    let keys = gtk4::EventControllerKey::new();
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let state_weak = Rc::downgrade(state);
    let keyboard_id = id.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let Some(offset) = editor_dnd::keyboard_reorder_offset(key, modifiers) else {
            return glib::Propagation::Proceed;
        };
        if let Some(state) = state_weak.upgrade() {
            state.move_by(&keyboard_id, offset);
        }
        glib::Propagation::Stop
    });
    row.add_controller(keys);

    let state_weak = Rc::downgrade(state);
    let target_id = id.clone();
    editor_dnd::wire_row_drag_and_drop(&row, id, move |source, after| {
        let Some(state) = state_weak.upgrade() else {
            return;
        };
        state.model.move_column(&source, &target_id, after);
        state.rebuild();
    });
    row
}

type SortChoice = (String, gtk4::CheckButton);

fn radio_choice(label: &str) -> gtk4::CheckButton {
    let choice = gtk4::CheckButton::builder()
        .label(label)
        .accessible_role(gtk4::AccessibleRole::Radio)
        .build();
    choice.update_property(&[gtk4::accessible::Property::Label(label)]);
    // a11y-semantics: role=radio name=explicit-label state=checked action=activate
    choice.set_focusable(true);
    choice
}

fn section_label(message: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(&strings::text(message)));
    label.set_halign(gtk4::Align::Start);
    label.add_css_class("heading");
    label
}

fn build_sort_section(model: &Rc<dyn EditorModel>) -> Option<gtk4::Box> {
    let descriptors = model.sortable_columns();
    if descriptors.is_empty() {
        return None;
    }

    let current = model.sort();
    let syncing = Rc::new(std::cell::Cell::new(true));
    let field_choices = Rc::new(std::cell::RefCell::new(Vec::<SortChoice>::new()));
    let field_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    let mut first = None::<gtk4::CheckButton>;
    for descriptor in descriptors {
        let choice = radio_choice(&descriptor.label);
        if let Some(first) = &first {
            choice.set_group(Some(first));
        } else {
            first = Some(choice.clone());
        }
        choice.set_active(current.as_ref().is_some_and(|(id, _)| id == &descriptor.id));
        field_box.append(&choice);
        field_choices.borrow_mut().push((descriptor.id, choice));
    }

    let ascending = radio_choice(&strings::text(strings::SORT_ASCENDING));
    let descending = radio_choice(&strings::text(strings::SORT_DESCENDING));
    descending.set_group(Some(&ascending));
    let descending_active = current
        .as_ref()
        .is_some_and(|(_, order)| *order == gtk4::SortType::Descending);
    ascending.set_active(!descending_active);
    descending.set_active(descending_active);
    let direction_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    direction_box.append(&ascending);
    direction_box.append(&descending);

    for (id, choice) in field_choices.borrow().clone() {
        let model = model.clone();
        let syncing = syncing.clone();
        let descending = descending.clone();
        choice.connect_toggled(move |choice| {
            if syncing.get() || !choice.is_active() {
                return;
            }
            let order = if descending.is_active() {
                gtk4::SortType::Descending
            } else {
                gtk4::SortType::Ascending
            };
            model.set_sort(&id, order);
        });
    }
    for (choice, order) in [
        (ascending.clone(), gtk4::SortType::Ascending),
        (descending.clone(), gtk4::SortType::Descending),
    ] {
        let model = model.clone();
        let syncing = syncing.clone();
        let field_choices = field_choices.clone();
        choice.connect_toggled(move |choice| {
            if syncing.get() || !choice.is_active() {
                return;
            }
            let selected = field_choices
                .borrow()
                .iter()
                .find(|(_, candidate)| candidate.is_active())
                .map(|(id, _)| id.clone());
            if let Some(id) = selected {
                model.set_sort(&id, order);
            }
        });
    }
    syncing.set(false);

    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    section.append(&section_label(strings::SORT_BY));
    section.append(&field_box);
    section.append(&section_label(strings::SORT_DIRECTION));
    section.append(&direction_box);
    Some(section)
}

fn handle_popover_key(
    key: gdk::Key,
    modifiers: gdk::ModifierType,
    popover: &glib::WeakRef<gtk4::Popover>,
) -> glib::Propagation {
    if key != gdk::Key::Escape || !modifiers.is_empty() {
        return glib::Propagation::Proceed;
    }
    if let Some(popover) = popover.upgrade() {
        popover.popdown();
    }
    glib::Propagation::Stop
}

pub(super) fn wire_popover_escape(popover: &gtk4::Popover) {
    let keys = gtk4::EventControllerKey::new();
    let popover_weak = popover.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        handle_popover_key(key, modifiers, &popover_weak)
    });
    popover.add_controller(keys);
}

/// `show_window_controls`: the dialog/preferences variants keep the header
/// bar's native decoration (adw::Dialog turns it into the dialog's close
/// button); the header popover variant must not — a popover is no window,
/// and the default decoration renders stray minimize/maximize/close buttons
/// inside it.
pub(in crate::ui) fn build_surface(
    model: &Rc<dyn EditorModel>,
    show_window_controls: bool,
) -> EditorSurface {
    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    let state = Rc::new(EditorState {
        model: model.clone(),
        list: list.clone(),
    });
    state.rebuild();

    let reset = gtk4::Button::with_label(&strings::text(strings::RESET_TO_DEFAULT));
    let state_guard = state.clone();
    reset.connect_clicked(move |_| {
        state_guard.model.reset();
        state_guard.rebuild();
    });
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(show_window_controls);
    header.set_show_end_title_buttons(show_window_controls);
    header.pack_start(&reset);
    header.set_title_widget(Some(&adw::WindowTitle::new(&model.title(), "")));
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
    if let Some(sort) = build_sort_section(model) {
        content.append(&sort);
    }
    content.append(&list);
    let scroll = gtk4::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroll));

    EditorSurface { toolbar, list }
}

pub(in crate::ui) fn build_navigation_page(model: &Rc<dyn EditorModel>) -> adw::NavigationPage {
    let title = strings::text(strings::COLUMN_LAYOUT);
    let surface = build_surface(model, true);
    tracing::info!(table = %model.title(), "column layout editor opened in preferences");
    adw::NavigationPage::with_tag(&surface.toolbar, &title, "column-layout")
}

pub(in crate::ui) fn build_dialog(model: &Rc<dyn EditorModel>) -> EditorDialogSurface {
    let surface = build_surface(model, true);
    let dialog = adw::Dialog::builder()
        .child(&surface.toolbar)
        .content_width(520)
        .content_height(620)
        .build();
    EditorDialogSurface {
        dialog,
        list: surface.list,
    }
}

pub(in crate::ui) fn present_dialog(window: &adw::ApplicationWindow, model: &Rc<dyn EditorModel>) {
    let surface = build_dialog(model);
    let dialog = surface.dialog;
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(window);
    focus_guard.bind_closable_dialog(&dialog, &surface.list);
    dialog.present(Some(window));
    tracing::info!(table = %model.title(), "column layout editor presented");
    if let Ok(smoke) = std::env::var(SMOKE_ENV) {
        let model = model.clone();
        glib::timeout_add_seconds_local_once(1, move || {
            if smoke == "exercise" {
                let table = model.title();
                if let Some(applied) = apply_smoke_exercise(model.as_ref()) {
                    tracing::info!(
                        %table,
                        hidden_column = %applied.hidden_id,
                        moved_column = %applied.moved_id,
                        before_column = %applied.target_id,
                        "column layout editor smoke applied"
                    );
                } else {
                    tracing::warn!(
                        %table,
                        editable_columns = model.columns().len(),
                        "column layout editor smoke skipped; at least two editable columns required"
                    );
                }
            }
            dialog.close();
            tracing::info!("column layout editor smoke closed");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::ui::table_columns::ColumnDescriptor;

    struct FakeModel {
        order: RefCell<Vec<String>>,
        hidden: RefCell<Vec<String>>,
        sort: RefCell<Option<(String, gtk4::SortType)>>,
    }

    impl FakeModel {
        fn new() -> Self {
            Self {
                order: RefCell::new(vec!["date".to_owned(), "title".to_owned()]),
                hidden: RefCell::new(Vec::new()),
                sort: RefCell::new(Some(("date".to_owned(), gtk4::SortType::Ascending))),
            }
        }
    }

    impl EditorModel for FakeModel {
        fn title(&self) -> String {
            "Edit column layout".to_owned()
        }

        fn columns(&self) -> Vec<ColumnDescriptor> {
            self.order
                .borrow()
                .iter()
                .map(|id| ColumnDescriptor {
                    id: id.clone(),
                    label: id.clone(),
                })
                .collect()
        }

        fn sortable_columns(&self) -> Vec<ColumnDescriptor> {
            self.columns()
        }

        fn sort(&self) -> Option<(String, gtk4::SortType)> {
            self.sort.borrow().clone()
        }

        fn set_sort(&self, id: &str, order: gtk4::SortType) {
            self.sort.replace(Some((id.to_owned(), order)));
        }

        fn is_visible(&self, id: &str) -> bool {
            !self.hidden.borrow().iter().any(|hidden| hidden == id)
        }

        fn set_visible(&self, id: &str, visible: bool) {
            if visible {
                self.hidden.borrow_mut().retain(|hidden| hidden != id);
            } else {
                self.hidden.borrow_mut().push(id.to_owned());
            }
        }

        fn move_column(&self, id: &str, target: &str, after: bool) {
            let mut order = self.order.borrow_mut();
            let Some(source) = order.iter().position(|candidate| candidate == id) else {
                return;
            };
            let id = order.remove(source);
            let Some(target) = order.iter().position(|candidate| candidate == target) else {
                return;
            };
            order.insert(target + usize::from(after), id);
        }

        fn reset(&self) {}
    }

    fn model() -> Rc<dyn EditorModel> {
        Rc::new(FakeModel::new())
    }

    struct UnsortableModel(FakeModel);

    impl EditorModel for UnsortableModel {
        fn title(&self) -> String {
            self.0.title()
        }

        fn columns(&self) -> Vec<ColumnDescriptor> {
            self.0.columns()
        }

        fn is_visible(&self, id: &str) -> bool {
            self.0.is_visible(id)
        }

        fn set_visible(&self, id: &str, visible: bool) {
            self.0.set_visible(id, visible);
        }

        fn move_column(&self, id: &str, target: &str, after: bool) {
            self.0.move_column(id, target, after);
        }

        fn reset(&self) {
            self.0.reset();
        }
    }

    #[test]
    fn smoke_exercise_uses_the_models_own_column_descriptors() {
        let model = FakeModel::new();

        let applied = apply_smoke_exercise(&model).expect("two editable columns");

        assert_eq!(applied.hidden_id, "date");
        assert_eq!(applied.moved_id, "title");
        assert_eq!(applied.target_id, "date");
        assert!(!model.is_visible("date"));
        assert_eq!(model.order.borrow().as_slice(), ["title", "date"]);
    }

    fn descendant_button_count(widget: &gtk4::Widget) -> usize {
        let mut count = usize::from(widget.is::<gtk4::Button>());
        let mut child = widget.first_child();
        while let Some(current) = child {
            count += descendant_button_count(&current);
            child = current.next_sibling();
        }
        count
    }

    fn contains_button_label(widget: &gtk4::Widget, label: &str) -> bool {
        if widget
            .clone()
            .downcast::<gtk4::Button>()
            .is_ok_and(|button| button.label().as_deref() == Some(label))
        {
            return true;
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            let next = current.next_sibling();
            if contains_button_label(&current, label) {
                return true;
            }
            child = next;
        }
        false
    }

    fn descendants<T: glib::object::IsA<gtk4::Widget> + glib::object::ObjectType>(
        widget: &gtk4::Widget,
    ) -> Vec<T> {
        let mut matches = widget
            .clone()
            .downcast::<T>()
            .ok()
            .into_iter()
            .collect::<Vec<_>>();
        let mut child = widget.first_child();
        while let Some(current) = child {
            let next = current.next_sibling();
            matches.extend(descendants::<T>(&current));
            child = next;
        }
        matches
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_13_sort_choices_are_keyboard_radio_actions() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let surface = build_surface(&model(), true);
        let choices = descendants::<gtk4::CheckButton>(surface.toolbar.upcast_ref());

        assert_eq!(choices.len(), 4, "two fields and two directions");
        for choice in choices {
            assert_eq!(choice.accessible_role(), gtk4::AccessibleRole::Radio);
            assert!(gtk4::test_accessible_has_property(
                &choice,
                gtk4::AccessibleProperty::Label
            ));
            assert!(choice.is_focusable());
            assert!(choice.activate());
            assert!(choice.is_active());
            assert!(gtk4::test_accessible_has_state(
                &choice,
                gtk4::AccessibleState::Checked
            ));
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_13_sort_choices_match_every_accepted_table_sort_field() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let track_list = Rc::new(crate::ui::track_list::TrackList::new(
            Rc::new(crate::test_db::open().unwrap()),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        ));
        let model = crate::ui::column_layout::model(&track_list);
        let actual = model
            .sortable_columns()
            .into_iter()
            .map(|column| column.id)
            .collect::<Vec<_>>();
        let layout = track_list.column_registry.layout();
        let columns = track_list.shared.column_view.columns();
        let expected = (0..columns.n_items())
            .filter_map(|index| {
                let column = columns
                    .item(index)
                    .and_downcast::<gtk4::ColumnViewColumn>()?;
                let field = column.id()?.to_string();
                let key = crate::ui::column_layout::ColumnId::from_sort_field(&field)?;
                (layout.visible.contains(&key) && column.sorter().is_some()).then_some(field)
            })
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_13_sort_popover_closes_on_escape() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let popover = gtk4::Popover::new();

        assert_eq!(
            handle_popover_key(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                &popover.downgrade()
            ),
            glib::Propagation::Stop
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn sort_section_is_shared_by_the_dialog_and_header_popover() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let model = model();
        let dialog = build_dialog(&model);
        let (popover, _) = super::super::header_popover::build_header_popover(&model);

        for root in [dialog.dialog.child().unwrap(), popover.child().unwrap()] {
            let radios = descendants::<gtk4::CheckButton>(&root);
            assert_eq!(radios.len(), 4);
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn unsortable_models_do_not_show_a_sort_section() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let model: Rc<dyn EditorModel> = Rc::new(UnsortableModel(FakeModel::new()));
        let surface = build_surface(&model, true);

        assert!(descendants::<gtk4::CheckButton>(surface.toolbar.upcast_ref()).is_empty());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn movable_row_reorders_without_visible_arrow_buttons() {
        if gtk4::init().is_err() {
            return;
        }
        let model = model();
        let state = Rc::new(EditorState {
            model,
            list: gtk4::ListBox::new(),
        });
        let descriptor = state.model.columns().remove(0);
        let row = build_row(&state, descriptor);
        let controllers = row.observe_controllers();
        let keyboard_phase = (0..controllers.n_items()).find_map(|index| {
            controllers
                .item(index)?
                .downcast::<gtk4::EventControllerKey>()
                .ok()
                .map(|controller| controller.propagation_phase())
        });

        assert_eq!(descendant_button_count(row.upcast_ref()), 0);
        assert_eq!(keyboard_phase, Some(gtk4::PropagationPhase::Capture));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn navigation_editor_builds_a_poppable_preferences_detail_page() {
        gtk4::init().unwrap();
        let page = build_navigation_page(&model());

        assert_eq!(page.title(), strings::text(strings::COLUMN_LAYOUT));
        assert!(page.can_pop());
        assert!(page
            .child()
            .is_some_and(|child| child.is::<adw::ToolbarView>()));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn standalone_editor_uses_native_close_without_a_labeled_close_button() {
        gtk4::init().unwrap();
        let surface = build_dialog(&model());
        let content = surface.dialog.child().unwrap();

        assert!(contains_button_label(
            &content,
            &strings::text(strings::RESET_TO_DEFAULT)
        ));
        assert!(!contains_button_label(
            &content,
            &strings::text(strings::CLOSE)
        ));
    }
}
