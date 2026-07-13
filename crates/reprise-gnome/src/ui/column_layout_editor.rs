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
    can_move_up: bool,
    can_move_down: bool,
}

fn is_fixed(id: ColumnId) -> bool {
    matches!(id, ColumnId::Cover | ColumnId::Title)
}

fn row_capabilities(id: ColumnId, index: usize, len: usize) -> RowCapabilities {
    let movable = !is_fixed(id);
    RowCapabilities {
        toggleable: movable,
        draggable: movable,
        can_move_up: movable && index > 2,
        can_move_down: movable && index + 1 < len,
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

fn column_label(id: ColumnId) -> String {
    let message = match id {
        ColumnId::Cover => strings::COLUMN_COVER,
        ColumnId::Title => strings::COLUMN_TITLE,
        ColumnId::TrackNumber => strings::COLUMN_TRACK_NUMBER,
        ColumnId::Artist => strings::COLUMN_ARTIST,
        ColumnId::Album => strings::COLUMN_ALBUM,
        ColumnId::Genre => strings::COLUMN_GENRE,
        ColumnId::Year => strings::COLUMN_YEAR,
        ColumnId::Duration => strings::COLUMN_LENGTH,
        ColumnId::Rating => strings::RATING,
    };
    strings::text(message)
}

struct EditorState {
    layout: RefCell<ColumnLayout>,
    list: gtk4::ListBox,
    track_list: std::rc::Weak<TrackList>,
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
        for (index, id) in layout.order.iter().copied().enumerate() {
            self.list.append(&build_row(self, &layout, id, index));
        }
    }
}

fn build_row(
    state: &Rc<EditorState>,
    layout: &ColumnLayout,
    id: ColumnId,
    index: usize,
) -> adw::ActionRow {
    let capabilities = row_capabilities(id, index, layout.order.len());
    let row = adw::ActionRow::builder().title(column_label(id)).build();
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
        let up = gtk4::Button::builder()
            .icon_name("go-up-symbolic")
            .tooltip_text(strings::text(strings::MOVE_COLUMN_UP))
            .valign(gtk4::Align::Center)
            .sensitive(capabilities.can_move_up)
            .css_classes(["flat"])
            .build();
        let state_weak = Rc::downgrade(state);
        up.connect_clicked(move |_| {
            if let Some(state) = state_weak.upgrade() {
                state.move_by(id, -1);
            }
        });
        row.add_suffix(&up);

        let down = gtk4::Button::builder()
            .icon_name("go-down-symbolic")
            .tooltip_text(strings::text(strings::MOVE_COLUMN_DOWN))
            .valign(gtk4::Align::Center)
            .sensitive(capabilities.can_move_down)
            .css_classes(["flat"])
            .build();
        let state_weak = Rc::downgrade(state);
        down.connect_clicked(move |_| {
            if let Some(state) = state_weak.upgrade() {
                state.move_by(id, 1);
            }
        });
        row.add_suffix(&down);

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

pub(super) fn present(window: &adw::ApplicationWindow, track_list: &Rc<TrackList>) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&format!(
        ".{DROP_BEFORE_CLASS}:drop(active) {{ box-shadow: inset 0 2px @accent_color; }}\n\
         .{DROP_AFTER_CLASS}:drop(active) {{ box-shadow: inset 0 -2px @accent_color; }}"
    ));
    gtk4::style_context_add_provider_for_display(
        &gtk4::prelude::WidgetExt::display(window),
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
    let close = gtk4::Button::with_label(&strings::text(strings::CLOSE));
    let header = adw::HeaderBar::new();
    header.pack_start(&reset);
    header.pack_end(&close);
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::EDIT_COLUMN_LAYOUT),
        "",
    )));
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
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(520)
        .content_height(620)
        .build();
    {
        let dialog = dialog.clone();
        close.connect_clicked(move |_| {
            dialog.close();
        });
    }
    dialog.present(Some(window));
    let serialized = column_layout::serialize_layout(&state.layout.borrow());
    tracing::info!(
        layout = %serialized,
        "column layout editor presented"
    );
    if let Ok(smoke) = std::env::var(SMOKE_ENV) {
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

    #[test]
    fn fixed_and_movable_rows_expose_the_right_controls() {
        let fixed = row_capabilities(ColumnId::Title, 1, 9);
        assert!(!fixed.toggleable);
        assert!(!fixed.draggable);
        assert!(!fixed.can_move_up);
        assert!(!fixed.can_move_down);

        let first = row_capabilities(ColumnId::Artist, 2, 9);
        assert!(first.toggleable);
        assert!(first.draggable);
        assert!(!first.can_move_up);
        assert!(first.can_move_down);

        let last = row_capabilities(ColumnId::Genre, 8, 9);
        assert!(last.can_move_up);
        assert!(!last.can_move_down);
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
    fn movable_row_owns_drag_and_drop_controllers() {
        if gtk4::init().is_err() {
            return;
        }
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        wire_row_drag_and_drop(&row, ColumnId::Artist, |_, _| {});
        let controllers = row.observe_controllers();
        let mut has_drag = false;
        let mut has_drop = false;
        for index in 0..controllers.n_items() {
            let controller = controllers.item(index).unwrap();
            has_drag |= controller.is::<gtk4::DragSource>();
            has_drop |= controller.is::<gtk4::DropTarget>();
        }
        assert!(has_drag);
        assert!(has_drop);
    }
}
