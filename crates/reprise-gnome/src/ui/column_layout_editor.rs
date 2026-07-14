use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::column_layout::{self, ColumnId, ColumnLayout};
use crate::ui::strings;
use crate::ui::track_list::TrackList;

pub(super) const SMOKE_ENV: &str = "REPRISE_SMOKE_COLUMN_LAYOUT_EDITOR";
const DROP_BEFORE_CLASS: &str = "reprise-column-drop-before";
const DROP_AFTER_CLASS: &str = "reprise-column-drop-after";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowCapabilities {
    toggleable: bool,
    draggable: bool,
}

fn is_fixed(id: ColumnId) -> bool {
    matches!(id, ColumnId::Cover | ColumnId::Title)
}

fn row_capabilities(id: ColumnId) -> RowCapabilities {
    let movable = !is_fixed(id);
    RowCapabilities {
        toggleable: movable,
        draggable: movable,
    }
}

fn keyboard_reorder_offset(key: gdk::Key, modifiers: gdk::ModifierType) -> Option<isize> {
    if !modifiers.contains(gdk::ModifierType::ALT_MASK) {
        return None;
    }
    match key {
        gdk::Key::Up => Some(-1),
        gdk::Key::Down => Some(1),
        _ => None,
    }
}

fn parse_drag_payload(value: &str) -> Option<ColumnId> {
    ColumnId::parse(value).filter(|id| !is_fixed(*id))
}

fn set_drop_indicator(widget: &impl IsA<gtk4::Widget>, after: Option<bool>) {
    widget.remove_css_class(DROP_BEFORE_CLASS);
    widget.remove_css_class(DROP_AFTER_CLASS);
    match after {
        Some(true) => widget.add_css_class(DROP_AFTER_CLASS),
        Some(false) => widget.add_css_class(DROP_BEFORE_CLASS),
        None => {}
    }
}

fn is_after_half(widget: &impl IsA<gtk4::Widget>, y: f64) -> bool {
    y >= f64::from(widget.height()) / 2.0
}

fn wire_row_drag_and_drop(
    widget: &impl IsA<gtk4::Widget>,
    id: ColumnId,
    on_drop: impl Fn(ColumnId, bool) + 'static,
) {
    let source = gtk4::DragSource::new();
    source.set_actions(gdk::DragAction::MOVE);
    // Observe pointer movement before ActionRow or one of its controls claims it.
    // Clicks still propagate normally when the gesture does not become a drag.
    source.set_propagation_phase(gtk4::PropagationPhase::Capture);
    source.connect_prepare(move |_, _, _| {
        Some(gdk::ContentProvider::for_value(&id.as_str().to_value()))
    });
    widget.add_controller(source);

    let target = gtk4::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
    {
        let widget = widget.upcast_ref::<gtk4::Widget>().clone();
        target.connect_motion(move |_, _, y| {
            set_drop_indicator(&widget, Some(is_after_half(&widget, y)));
            gdk::DragAction::MOVE
        });
    }
    {
        let widget = widget.upcast_ref::<gtk4::Widget>().clone();
        target.connect_leave(move |_| set_drop_indicator(&widget, None));
    }
    let drop_widget = widget.upcast_ref::<gtk4::Widget>().clone();
    target.connect_drop(move |_, value, _, y| {
        set_drop_indicator(&drop_widget, None);
        let Ok(value) = value.get::<String>() else {
            return false;
        };
        let Some(source) = parse_drag_payload(&value) else {
            return false;
        };
        on_drop(source, is_after_half(&drop_widget, y));
        true
    });
    widget.add_controller(target);
}

struct EditorState {
    layout: RefCell<ColumnLayout>,
    list: gtk4::ListBox,
    track_list: std::rc::Weak<TrackList>,
}

struct EditorSurface {
    toolbar: adw::ToolbarView,
    state: Rc<EditorState>,
}

struct EditorDialogSurface {
    dialog: adw::Dialog,
    state: Rc<EditorState>,
}

impl EditorState {
    fn apply(self: &Rc<Self>, next: ColumnLayout) {
        let current = self.layout.borrow().clone();
        if next == current {
            return;
        }
        let Some(track_list) = self.track_list.upgrade() else {
            return;
        };
        match track_list.apply_column_layout(&next) {
            Ok(()) => {
                *self.layout.borrow_mut() = next;
            }
            Err(error) => {
                tracing::warn!(%error, "could not save edited column layout");
                track_list.toast(&strings::text(strings::COLUMN_LAYOUT_SAVE_FAILED));
            }
        }
        self.rebuild();
    }

    fn move_by(self: &Rc<Self>, id: ColumnId, offset: isize) {
        let layout = self.layout.borrow().clone();
        let Some(index) = layout.order.iter().position(|candidate| *candidate == id) else {
            return;
        };
        let Some(target_index) = index.checked_add_signed(offset) else {
            return;
        };
        let Some(target) = layout.order.get(target_index).copied() else {
            return;
        };
        let next = if offset > 0 {
            column_layout::move_column_after(&layout, id, target)
        } else {
            column_layout::move_column(&layout, id, target)
        };
        self.apply(next);
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let layout = self.layout.borrow().clone();
        for id in layout.order.iter().copied() {
            self.list.append(&build_row(self, &layout, id));
        }
    }
}

fn build_row(state: &Rc<EditorState>, layout: &ColumnLayout, id: ColumnId) -> adw::ActionRow {
    let capabilities = row_capabilities(id);
    let row = adw::ActionRow::builder()
        .title(column_layout::column_label(id))
        .build();
    if is_fixed(id) {
        row.set_subtitle(&strings::text(strings::COLUMN_ALWAYS_VISIBLE));
    } else {
        let handle = gtk4::Image::from_icon_name("list-drag-handle-symbolic");
        handle.set_tooltip_text(Some(&strings::text(strings::DRAG_TO_REORDER)));
        row.add_prefix(&handle);
    }

    if capabilities.toggleable {
        let toggle = gtk4::Switch::builder()
            .active(layout.visible.contains(&id))
            .valign(gtk4::Align::Center)
            .build();
        let state_weak = Rc::downgrade(state);
        toggle.connect_active_notify(move |toggle| {
            let Some(state) = state_weak.upgrade() else {
                return;
            };
            let layout = state.layout.borrow().clone();
            state.apply(column_layout::set_column_visible(
                &layout,
                id,
                toggle.is_active(),
            ));
        });
        row.add_suffix(&toggle);
        row.set_activatable_widget(Some(&toggle));
    }

    if capabilities.draggable {
        row.update_property(&[
            gtk4::accessible::Property::Description(&strings::text(strings::DRAG_TO_REORDER)),
            gtk4::accessible::Property::KeyShortcuts("Alt+ArrowUp Alt+ArrowDown"),
        ]);
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let state_weak = Rc::downgrade(state);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(offset) = keyboard_reorder_offset(key, modifiers) else {
                return glib::Propagation::Proceed;
            };
            if let Some(state) = state_weak.upgrade() {
                state.move_by(id, offset);
            }
            glib::Propagation::Stop
        });
        row.add_controller(keys);

        let state_weak = Rc::downgrade(state);
        wire_row_drag_and_drop(&row, id, move |source, after| {
            let Some(state) = state_weak.upgrade() else {
                return;
            };
            let layout = state.layout.borrow().clone();
            let next = if after {
                column_layout::move_column_after(&layout, source, id)
            } else {
                column_layout::move_column(&layout, source, id)
            };
            state.apply(next);
        });
    }
    row
}

fn build_surface(track_list: &Rc<TrackList>, title: &str) -> EditorSurface {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&format!(
        ".{DROP_BEFORE_CLASS}:drop(active) {{ box-shadow: inset 0 2px @accent_color; }}\n\
         .{DROP_AFTER_CLASS}:drop(active) {{ box-shadow: inset 0 -2px @accent_color; }}"
    ));
    gtk4::style_context_add_provider_for_display(
        &track_list.root_widget().display(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    let state = Rc::new(EditorState {
        layout: RefCell::new(track_list.current_column_layout()),
        list: list.clone(),
        track_list: Rc::downgrade(track_list),
    });
    state.rebuild();

    let reset = gtk4::Button::with_label(&strings::text(strings::RESET_TO_DEFAULT));
    let state_guard = state.clone();
    reset.connect_clicked(move |_| {
        state_guard.apply(ColumnLayout::default());
    });
    let header = adw::HeaderBar::new();
    header.pack_start(&reset);
    header.set_title_widget(Some(&adw::WindowTitle::new(title, "")));
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

    EditorSurface { toolbar, state }
}

pub(super) fn build_navigation_page(track_list: &Rc<TrackList>) -> adw::NavigationPage {
    let title = strings::text(strings::ONBOARDING_RHYTHMBOX_COLUMN_LAYOUT);
    let surface = build_surface(track_list, &title);
    let serialized = column_layout::serialize_layout(&surface.state.layout.borrow());
    tracing::info!(layout = %serialized, "column layout editor opened in preferences");
    adw::NavigationPage::with_tag(&surface.toolbar, &title, "column-layout")
}

fn build_dialog(track_list: &Rc<TrackList>) -> EditorDialogSurface {
    let title = strings::text(strings::EDIT_COLUMN_LAYOUT);
    let surface = build_surface(track_list, &title);
    let dialog = adw::Dialog::builder()
        .child(&surface.toolbar)
        .content_width(520)
        .content_height(620)
        .build();
    EditorDialogSurface {
        dialog,
        state: surface.state,
    }
}

pub(super) fn present(window: &adw::ApplicationWindow, track_list: &Rc<TrackList>) {
    let surface = build_dialog(track_list);
    let dialog = surface.dialog;
    dialog.present(Some(window));
    let serialized = column_layout::serialize_layout(&surface.state.layout.borrow());
    tracing::info!(
        layout = %serialized,
        "column layout editor presented"
    );
    if let Ok(smoke) = std::env::var(SMOKE_ENV) {
        let state = surface.state;
        glib::timeout_add_seconds_local_once(1, move || {
            if smoke == "exercise" {
                let layout = state.layout.borrow().clone();
                let layout = column_layout::set_column_visible(&layout, ColumnId::Artist, false);
                let layout =
                    column_layout::move_column(&layout, ColumnId::Rating, ColumnId::Artist);
                state.apply(layout);
                let serialized = column_layout::serialize_layout(&state.layout.borrow());
                tracing::info!(layout = %serialized, "column layout editor smoke applied");
            }
            dialog.close();
            tracing::info!("column layout editor smoke closed");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn fixed_and_movable_rows_expose_the_right_controls() {
        let fixed = row_capabilities(ColumnId::Title);
        assert!(!fixed.toggleable);
        assert!(!fixed.draggable);

        let movable = row_capabilities(ColumnId::Artist);
        assert!(movable.toggleable);
        assert!(movable.draggable);
    }

    #[test]
    fn alt_arrow_keys_reorder_without_stealing_plain_navigation() {
        let alt = gdk::ModifierType::ALT_MASK;
        assert_eq!(keyboard_reorder_offset(gdk::Key::Up, alt), Some(-1));
        assert_eq!(keyboard_reorder_offset(gdk::Key::Down, alt), Some(1));
        assert_eq!(
            keyboard_reorder_offset(gdk::Key::Up, gdk::ModifierType::empty()),
            None
        );
        assert_eq!(keyboard_reorder_offset(gdk::Key::Return, alt), None);
    }

    #[test]
    fn drag_payload_accepts_only_movable_column_ids() {
        assert_eq!(parse_drag_payload("artist"), Some(ColumnId::Artist));
        assert_eq!(parse_drag_payload("cover"), None);
        assert_eq!(parse_drag_payload("title"), None);
        assert_eq!(parse_drag_payload("foreign"), None);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn movable_row_captures_drag_before_child_controls_and_accepts_drops() {
        if gtk4::init().is_err() {
            return;
        }
        let row = adw::ActionRow::builder().title("Artist").build();
        let toggle = gtk4::Switch::new();
        row.add_suffix(&toggle);
        row.set_activatable_widget(Some(&toggle));
        wire_row_drag_and_drop(&row, ColumnId::Artist, |_, _| {});
        let controllers = row.observe_controllers();
        let mut drag_phase = None;
        let mut has_drop = false;
        for index in 0..controllers.n_items() {
            let controller = controllers.item(index).unwrap();
            if let Ok(source) = controller.clone().downcast::<gtk4::DragSource>() {
                drag_phase = Some(source.propagation_phase());
            }
            has_drop |= controller.is::<gtk4::DropTarget>();
        }
        assert_eq!(drag_phase, Some(gtk4::PropagationPhase::Capture));
        assert!(has_drop);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn movable_row_reorders_without_visible_arrow_buttons() {
        if gtk4::init().is_err() {
            return;
        }
        let layout = ColumnLayout::default();
        let state = Rc::new(EditorState {
            layout: RefCell::new(layout.clone()),
            list: gtk4::ListBox::new(),
            track_list: std::rc::Weak::new(),
        });

        let row = build_row(&state, &layout, ColumnId::Artist);
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
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let runtime = crate::ui::cover_download_worker::setup();
        let track_list = Rc::new(TrackList::new(
            Rc::new(RefCell::new(conn)),
            Box::new(|_, _, _| {}),
            |_, _, _, _| {},
            Vec::new,
            runtime,
        ));

        let page = build_navigation_page(&track_list);

        assert_eq!(
            page.title(),
            strings::text(strings::ONBOARDING_RHYTHMBOX_COLUMN_LAYOUT)
        );
        assert!(page.can_pop());
        assert!(page
            .child()
            .is_some_and(|child| child.is::<adw::ToolbarView>()));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn standalone_editor_uses_native_close_without_a_labeled_close_button() {
        gtk4::init().unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let runtime = crate::ui::cover_download_worker::setup();
        let track_list = Rc::new(TrackList::new(
            Rc::new(RefCell::new(conn)),
            Box::new(|_, _, _| {}),
            |_, _, _, _| {},
            Vec::new,
            runtime,
        ));

        let surface = build_dialog(&track_list);
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
