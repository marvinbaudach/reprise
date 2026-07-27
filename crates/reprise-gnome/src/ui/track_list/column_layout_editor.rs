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

pub(in crate::ui) const SMOKE_ENV: &str = "REPRISE_SMOKE_COLUMN_LAYOUT_EDITOR";
const DROP_BEFORE_CLASS: &str = "reprise-column-drop-before";
const DROP_AFTER_CLASS: &str = "reprise-column-drop-after";
/// Draggable reorder row (movable columns) — gets the accent hover surface.
const ROW_CLASS: &str = "reprise-column-row";
/// Drag handle icon — dim at rest, accentuated on hover/drag.
const HANDLE_CLASS: &str = "reprise-column-handle";
/// Resting opacity of the drag handle (quiet, not disabled-looking).
const HANDLE_REST_OPACITY: &str = "0.45";
/// Drag-handle opacity once the row is hovered.
const HANDLE_ACTIVE_OPACITY: &str = "0.85";
/// Opacity of the row itself while it is being dragged (a translucent ghost).
const DRAG_GHOST_OPACITY: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowCapabilities {
    toggleable: bool,
    draggable: bool,
}

/// Whether `id` gets a row in the column editor at all. Cover is a fixed
/// leading column — always first and always visible (see `column_layout`'s
/// `normalize`) — so it is deliberately absent from the editor; it cannot be
/// reordered or hidden. Every other column is listed.
fn editor_lists_column(id: ColumnId) -> bool {
    id != ColumnId::Cover
}

fn row_capabilities(_id: ColumnId) -> RowCapabilities {
    // Every listed column — Title included — is an ordinary, equal row:
    // toggleable and draggable. (Cover is not listed at all, see
    // `editor_lists_column`.)
    RowCapabilities {
        toggleable: true,
        draggable: true,
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
    ColumnId::parse(value)
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
    // input-parity: ACC-8 keyboard=alt-arrows
    let source = gtk4::DragSource::new();
    source.set_actions(gdk::DragAction::MOVE);
    // Observe pointer movement before ActionRow or one of its controls claims it.
    // Clicks still propagate normally when the gesture does not become a drag.
    source.set_propagation_phase(gtk4::PropagationPhase::Capture);
    source.connect_prepare(move |_, _, _| {
        Some(gdk::ContentProvider::for_value(&id.as_str().to_value()))
    });
    // Fade the row to a ghost while it is being dragged, restoring it on end.
    {
        let ghost = widget.upcast_ref::<gtk4::Widget>().clone();
        source.connect_drag_begin(move |_, _| ghost.set_opacity(DRAG_GHOST_OPACITY));
    }
    {
        let ghost = widget.upcast_ref::<gtk4::Widget>().clone();
        source.connect_drag_end(move |_, _, _| ghost.set_opacity(1.0));
    }
    widget.add_controller(source);

    // input-parity: ACC-8 keyboard=alt-arrows
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
        for id in layout
            .order
            .iter()
            .copied()
            .filter(|id| editor_lists_column(*id))
        {
            self.list.append(&build_row(self, &layout, id));
        }
    }
}

fn build_row(state: &Rc<EditorState>, layout: &ColumnLayout, id: ColumnId) -> adw::ActionRow {
    let capabilities = row_capabilities(id);
    let row = adw::ActionRow::builder()
        .title(column_layout::column_label(id))
        .build();
    row.add_css_class(ROW_CLASS);
    let handle = gtk4::Image::from_icon_name("list-drag-handle-symbolic");
    handle.add_css_class(HANDLE_CLASS);
    handle.set_tooltip_text(Some(&strings::text(strings::DRAG_TO_REORDER)));
    row.add_prefix(&handle);

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

/// Redesign chrome for the column-layout editor; installed app-wide by
/// [`super::style`].
///
/// Deutsch: Reorder-Reihen bekommen ein sanftes Akzent-Hover-Feedback (wie die
/// app-weite `.reprise-hover`), damit sie als greifbare Flächen lesbar sind.
/// Der Griff ist im Ruhezustand gedimmt und wird beim Hover bzw. während eines
/// aktiven Drops akzentuiert. Die Vorher/Nachher-Drop-Indikatoren bleiben.
pub(in crate::ui) fn css() -> String {
    use super::style::tokens::{DROP_INDICATOR_THICKNESS, HOVER_BG_ALPHA, TRANSITION};
    format!(
        ".{ROW_CLASS} {{ transition: background-color {TRANSITION}; }}\n\
         .{ROW_CLASS}:hover {{ background-color: alpha(@accent_bg_color, {HOVER_BG_ALPHA}); }}\n\
         .{HANDLE_CLASS} {{ opacity: {HANDLE_REST_OPACITY}; \
           transition: opacity {TRANSITION}, color {TRANSITION}; }}\n\
         .{ROW_CLASS}:hover .{HANDLE_CLASS} {{ opacity: {HANDLE_ACTIVE_OPACITY}; color: @accent_color; }}\n\
         .{ROW_CLASS}:drop(active) .{HANDLE_CLASS} {{ opacity: 1; color: @accent_color; }}\n\
         .{DROP_BEFORE_CLASS}:drop(active) {{ box-shadow: inset 0 {DROP_INDICATOR_THICKNESS} @accent_color; }}\n\
         .{DROP_AFTER_CLASS}:drop(active) {{ box-shadow: inset 0 -{DROP_INDICATOR_THICKNESS} @accent_color; }}"
    )
}

/// `show_window_controls`: the dialog/preferences variants keep the header
/// bar's native decoration (adw::Dialog turns it into the dialog's close
/// button); the header POPOVER variant must not — a popover is no window,
/// and the default decoration renders stray minimize/maximize/close buttons
/// inside it.
fn build_surface(
    track_list: &Rc<TrackList>,
    title: &str,
    show_window_controls: bool,
) -> EditorSurface {
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
        if let Some(track_list) = state_guard.track_list.upgrade() {
            track_list.reset_column_widths();
        }
    });
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(show_window_controls);
    header.set_show_end_title_buttons(show_window_controls);
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

pub(in crate::ui) fn build_navigation_page(track_list: &Rc<TrackList>) -> adw::NavigationPage {
    let title = strings::text(strings::COLUMN_LAYOUT);
    let surface = build_surface(track_list, &title, true);
    let serialized = column_layout::serialize_layout(&surface.state.layout.borrow());
    tracing::info!(layout = %serialized, "column layout editor opened in preferences");
    adw::NavigationPage::with_tag(&surface.toolbar, &title, "column-layout")
}

fn build_dialog(track_list: &Rc<TrackList>) -> EditorDialogSurface {
    let title = strings::text(strings::EDIT_COLUMN_LAYOUT);
    let surface = build_surface(track_list, &title, true);
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

/// True when a click at vertical offset `y` (relative to the ColumnView) landed
/// on the header row. The header is always the ColumnView's first child and sits
/// flush at the top, so its height defines the band.
fn is_header_click(y: f64, header_height: i32) -> bool {
    header_height > 0 && y <= f64::from(header_height)
}

/// Builds the header popover: the same editor surface (toggle + drag list with a
/// Reset action) that the dialog uses, so a right-click on a column header edits
/// the layout inline instead of showing a plain visibility menu.
fn build_header_popover(track_list: &Rc<TrackList>) -> (gtk4::Popover, gtk4::ListBox) {
    let title = strings::text(strings::EDIT_COLUMN_LAYOUT);
    let surface = build_surface(track_list, &title, false);
    let content = gtk4::Frame::builder()
        .width_request(360)
        .height_request(440)
        .child(&surface.toolbar)
        .build();
    let popover = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .child(&content)
        .build();
    popover.add_css_class("menu");
    popover.add_css_class("reprise-column-header-popover");
    (popover, surface.state.list.clone())
}

/// Installs the right-click-on-header gesture that opens the editor popover.
/// Replaces the previous GMenu visibility list; row right-clicks are handled by
/// the per-cell context-menu gesture and never reach this controller.
pub(in crate::ui) fn install_header_popover(track_list: &Rc<TrackList>) {
    let column_view = track_list.column_view_widget().clone();
    // input-parity: ACC-8 keyboard=column-editor
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    // Capture phase, claiming on press: GtkColumnViewTitle's own click
    // gesture claims EVERY press (any button) at the target, so a
    // bubble-phase ancestor gesture loses the sequence before its handler
    // can run — the exact claim race that also breaks GTK's native column
    // drag (see `column_header_dnd`'s module doc). At capture this gesture
    // runs first, and its claim below keeps the title's own gesture (and
    // GTK's plain visibility menu) from ever seeing header right-clicks.
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let track_list_weak = Rc::downgrade(track_list);
    let view = column_view.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let header_height = view.first_child().map_or(0, |header| header.height());
        if !is_header_click(y, header_height) {
            return;
        }
        let Some(track_list) = track_list_weak.upgrade() else {
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let (popover, initial_focus) = build_header_popover(&track_list);
        popover.set_parent(&view);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(&view);
        focus_guard.bind_popover(&popover, &initial_focus);
        crate::ui::popover_lifecycle::unparent_after_actions(&popover);
        popover.popup();
        tracing::debug!("column header popover opened");
    });
    column_view.add_controller(gesture);
}

pub(in crate::ui) fn present(window: &adw::ApplicationWindow, track_list: &Rc<TrackList>) {
    let surface = build_dialog(track_list);
    let dialog = surface.dialog;
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(window);
    focus_guard.bind_closable_dialog(&dialog, &surface.state.list);
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
    fn cover_is_excluded_from_the_editor_but_other_columns_are_listed() {
        // Cover is a fixed leading column — never listed, so it can't be
        // reordered or hidden from the editor.
        assert!(!editor_lists_column(ColumnId::Cover));
        for id in [ColumnId::Title, ColumnId::Artist, ColumnId::Added] {
            assert!(editor_lists_column(id), "{id:?} should be listed");
        }
    }

    #[test]
    fn every_listed_column_is_draggable_and_toggleable() {
        for id in [ColumnId::Title, ColumnId::Artist, ColumnId::Added] {
            let caps = row_capabilities(id);
            assert!(caps.toggleable, "{id:?} should be toggleable");
            assert!(caps.draggable, "{id:?} should be draggable");
        }
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
    fn css_styles_reorder_rows_and_drag_handle() {
        let css = super::css();
        assert!(css.contains(".reprise-column-row:hover"));
        assert!(css.contains("@accent_bg_color"));
        assert!(css.contains(".reprise-column-handle"));
        assert!(css.contains(".reprise-column-drop-before"));
    }

    #[test]
    fn header_hit_test_matches_only_the_header_band() {
        assert!(is_header_click(0.0, 25));
        assert!(is_header_click(25.0, 25));
        assert!(!is_header_click(25.1, 25));
        assert!(!is_header_click(200.0, 25));
        // No measurable header (not yet realized) never counts as a hit.
        assert!(!is_header_click(0.0, 0));
    }

    #[test]
    fn drag_payload_accepts_any_known_column_including_cover_and_title() {
        assert_eq!(parse_drag_payload("artist"), Some(ColumnId::Artist));
        assert_eq!(parse_drag_payload("cover"), Some(ColumnId::Cover));
        assert_eq!(parse_drag_payload("title"), Some(ColumnId::Title));
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
        let runtime = crate::ui::cover_download_worker::setup_for_test();
        let track_list = Rc::new(TrackList::new(
            Rc::new(RefCell::new(conn)),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            runtime,
        ));

        let page = build_navigation_page(&track_list);

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
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let runtime = crate::ui::cover_download_worker::setup_for_test();
        let track_list = Rc::new(TrackList::new(
            Rc::new(RefCell::new(conn)),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
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
