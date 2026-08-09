//! The shared column editor surface used by every GTK table.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::editor_dnd;
use super::EditorModel;
use crate::ui::strings;

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
    let scroll = gtk4::ScrolledWindow::builder()
        .child(&list)
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
    if let Ok(smoke) = std::env::var(crate::ui::column_layout_editor::SMOKE_ENV) {
        let model = model.clone();
        glib::timeout_add_seconds_local_once(1, move || {
            if smoke == "exercise" {
                model.set_visible("artist", false);
                model.move_column("rating", "artist", false);
                tracing::info!("column layout editor smoke applied");
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
    }

    impl FakeModel {
        fn new() -> Self {
            Self {
                order: RefCell::new(vec!["artist".to_owned(), "album".to_owned()]),
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

        fn is_visible(&self, _id: &str) -> bool {
            true
        }

        fn set_visible(&self, _id: &str, _visible: bool) {}

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
