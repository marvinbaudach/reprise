//! The compact, read-only Up Next projection inside the Now Playing panel.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::models::Track;
use rusqlite::Connection;

use super::cover_loader::CoverLoader;
use crate::ui::track_list::queue_row_mapping::{classify, QueueRow};
use crate::ui::track_list::queue_sections::{
    section_ranges, QueueSection, QueueSectionKind, QueueViewModel,
};
use crate::ui::track_list::track_list_model::TrackListModel;

#[cfg(test)]
fn queue_rows(model: &QueueViewModel) -> Vec<QueueRow> {
    (0..u32::try_from(model.total_len()).unwrap_or(u32::MAX))
        .filter_map(|position| classify(position, &model.sections))
        .collect()
}

fn panel_section_headers(model: &QueueViewModel) -> Vec<(u32, String)> {
    model
        .sections
        .iter()
        .filter_map(|section| {
            let title = match &section.kind {
                QueueSectionKind::PlayNext => {
                    super::strings::text(super::strings::QUEUE_NEXT_IN_QUEUE)
                }
                QueueSectionKind::UpNext { source_label } => {
                    super::strings::queue_context_tail(source_label, section.len as usize)
                }
                QueueSectionKind::NowPlaying => return None,
            };
            Some((section.start, title))
        })
        .collect()
}

pub(super) fn format_up_next_footer(durations_ms: &[i64]) -> String {
    let total_duration_ms = durations_ms
        .iter()
        .copied()
        .fold(0_i64, i64::saturating_add);
    format_up_next_footer_total(durations_ms.len(), total_duration_ms)
}

fn format_up_next_footer_total(count: usize, total_duration_ms: i64) -> String {
    let duration = reprise_core::format::format_total_duration(total_duration_ms);
    super::strings::up_next_footer(count, &duration)
}

type OnJump = Rc<dyn Fn(QueueRow)>;
type OnRemove = Rc<dyn Fn(QueueRow)>;
type OnReorder = Rc<dyn Fn(QueueRow, QueueRow)>;

struct RowWidgets {
    cover: gtk4::Image,
    title: gtk4::Label,
    artist: gtk4::Label,
    generation: Rc<Cell<u64>>,
    row: Cell<Option<QueueRow>>,
    drop_target: gtk4::DropTarget,
}

pub(in crate::ui) struct UpNextPanel {
    root: gtk4::Stack,
    model: TrackListModel,
    queue_sections: Rc<RefCell<Vec<QueueSection>>>,
    section_headers: Rc<RefCell<Vec<(u32, String)>>>,
    on_jump: Rc<RefCell<Option<OnJump>>>,
    on_remove: Rc<RefCell<Option<OnRemove>>>,
    on_reorder: Rc<RefCell<Option<OnReorder>>>,
    conn: Rc<RefCell<Connection>>,
}

impl UpNextPanel {
    pub(in crate::ui) fn new(
        conn: Rc<RefCell<Connection>>,
        cover_loader: &Rc<CoverLoader>,
    ) -> Rc<Self> {
        let model = TrackListModel::new(conn.clone());
        let queue_sections = Rc::new(RefCell::new(Vec::new()));
        let section_headers = Rc::new(RefCell::new(Vec::new()));
        let on_jump = Rc::new(RefCell::new(None));
        let on_remove = Rc::new(RefCell::new(None));
        let on_reorder = Rc::new(RefCell::new(None));
        let factory = build_factory(
            cover_loader,
            &queue_sections,
            &on_jump,
            &on_remove,
            &on_reorder,
        );
        let selection = gtk4::NoSelection::new(Some(model.clone()));
        let rows = gtk4::ListView::new(Some(selection), Some(factory));
        rows.set_header_factory(Some(&build_header_factory(&section_headers)));
        rows.add_css_class("reprise-up-next-list");
        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .child(&rows)
            .build();
        install_drag_autoscroll(&scrolled);
        let empty = gtk4::Label::new(Some(&super::strings::text(super::strings::QUEUE_EMPTY)));
        empty.add_css_class("reprise-up-next-empty");
        empty.set_vexpand(true);
        empty.set_valign(gtk4::Align::Center);

        let root = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .transition_duration(crate::ui::motion::MICRO_MS)
            .vexpand(true)
            .build();
        root.add_named(&scrolled, Some("tracks"));
        root.add_named(&empty, Some("empty"));
        root.set_visible_child_name("empty");

        Rc::new(Self {
            root,
            model,
            queue_sections,
            section_headers,
            on_jump,
            on_remove,
            on_reorder,
            conn,
        })
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    pub(in crate::ui) fn set_on_jump(&self, callback: impl Fn(QueueRow) + 'static) {
        *self.on_jump.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_remove(&self, callback: impl Fn(QueueRow) + 'static) {
        *self.on_remove.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_reorder(&self, callback: impl Fn(QueueRow, QueueRow) + 'static) {
        *self.on_reorder.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_queue_model(&self, model: &QueueViewModel) -> String {
        let upcoming = model.upcoming();
        *self.queue_sections.borrow_mut() = upcoming.sections.clone();
        *self.section_headers.borrow_mut() = panel_section_headers(&upcoming);
        self.model
            .set_queue_snapshot(&upcoming, section_ranges(&upcoming.sections));
        self.root
            .set_visible_child_name(if upcoming.total_len() == 0 {
                "empty"
            } else {
                "tracks"
            });
        let mut total_duration_ms = 0_i64;
        for offset in (0..upcoming.total_len()).step_by(200) {
            let ids = upcoming.ids_window(offset, 200);
            let duration =
                match reprise_core::queries::query_queue_duration_ms(&self.conn.borrow(), &ids) {
                    Ok(duration) => duration,
                    Err(error) => {
                        tracing::warn!(%error, "could not load up-next panel duration window");
                        0
                    }
                };
            total_duration_ms = total_duration_ms.saturating_add(duration);
            if ids.is_empty() {
                break;
            }
        }
        format_up_next_footer_total(upcoming.total_len(), total_duration_ms)
    }
}

fn build_header_factory(
    section_headers: &Rc<RefCell<Vec<(u32, String)>>>,
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    let section_headers = section_headers.clone();
    factory.connect_bind(move |_, object| {
        let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
            return;
        };
        let title = section_headers
            .borrow()
            .iter()
            .find(|(start, _)| *start == header.start())
            .map(|(_, title)| title.clone())
            .unwrap_or_default();
        let label = gtk4::Label::builder()
            .label(&title)
            .xalign(0.0)
            .css_classes(["heading", "reprise-up-next-section"])
            .build();
        header.set_child(Some(&label));
    });
    factory.connect_unbind(|_, object| {
        if let Some(header) = object.downcast_ref::<gtk4::ListHeader>() {
            header.set_child(gtk4::Widget::NONE);
        }
    });
    factory
}

fn list_item_key(item: &gtk4::ListItem) -> usize {
    item.as_ptr() as usize
}

fn build_factory(
    cover_loader: &Rc<CoverLoader>,
    queue_sections: &Rc<RefCell<Vec<QueueSection>>>,
    on_jump: &Rc<RefCell<Option<OnJump>>>,
    on_remove: &Rc<RefCell<Option<OnRemove>>>,
    on_reorder: &Rc<RefCell<Option<OnReorder>>>,
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    let states: Rc<RefCell<HashMap<usize, Rc<RowWidgets>>>> = Rc::new(RefCell::new(HashMap::new()));
    {
        let states = states.clone();
        let queue_sections = queue_sections.clone();
        let on_jump = on_jump.clone();
        let on_remove = on_remove.clone();
        let on_reorder = on_reorder.clone();
        factory.connect_setup(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let (row_widget, jump_button, remove_button, widgets) = build_row_widgets();
            let widgets = Rc::new(widgets);
            let widgets_on_click = widgets.clone();
            let on_jump = on_jump.clone();
            jump_button.connect_clicked(move |_| {
                let row = widgets_on_click.row.get();
                let callback = on_jump.borrow().clone();
                if let (Some(row), Some(callback)) = (row, callback) {
                    callback(row);
                }
            });
            let widgets_on_remove = widgets.clone();
            let on_remove = on_remove.clone();
            remove_button.connect_clicked(move |_| {
                let row = widgets_on_remove.row.get();
                let callback = on_remove.borrow().clone();
                if let (Some(row), Some(callback)) = (row, callback) {
                    callback(row);
                }
            });
            jump_button.update_property(&[gtk4::accessible::Property::KeyShortcuts(
                "Alt+ArrowUp Alt+ArrowDown",
            )]);
            let keys = gtk4::EventControllerKey::new();
            let widgets_for_keys = widgets.clone();
            let sections_for_keys = queue_sections.clone();
            let on_reorder_for_keys = on_reorder.clone();
            keys.connect_key_pressed(move |_, key, _, modifiers| {
                let Some(row) = widgets_for_keys.row.get() else {
                    return gtk4::glib::Propagation::Proceed;
                };
                let play_next_len = sections_for_keys
                    .borrow()
                    .iter()
                    .find_map(|section| {
                        matches!(section.kind, QueueSectionKind::PlayNext)
                            .then_some(section.len as usize)
                    })
                    .unwrap_or_default();
                let Some((from, to)) = keyboard_reorder_rows(row, play_next_len, key, modifiers)
                else {
                    return gtk4::glib::Propagation::Proceed;
                };
                let callback = on_reorder_for_keys.borrow().clone();
                if let Some(callback) = callback {
                    callback(from, to);
                    gtk4::glib::Propagation::Stop
                } else {
                    gtk4::glib::Propagation::Proceed
                }
            });
            jump_button.add_controller(keys);
            // input-parity: ACC-8 keyboard=up-next-alt-arrows
            let drag_source = gtk4::DragSource::new();
            drag_source.set_actions(gtk4::gdk::DragAction::MOVE);
            let widgets_for_drag = widgets.clone();
            drag_source.connect_prepare(move |_, _, _| {
                let row = widgets_for_drag.row.get()?;
                Some(gtk4::gdk::ContentProvider::for_value(
                    &encode_drag_row(row).to_value(),
                ))
            });
            row_widget.add_controller(drag_source);

            let widgets_for_enter = widgets.clone();
            widgets.drop_target.connect_enter(move |_, _, _| {
                if matches!(widgets_for_enter.row.get(), Some(QueueRow::PlayNext(_))) {
                    gtk4::gdk::DragAction::MOVE
                } else {
                    gtk4::gdk::DragAction::empty()
                }
            });
            let widgets_for_drop = widgets.clone();
            let on_reorder = on_reorder.clone();
            widgets.drop_target.connect_drop(move |_, value, _, _| {
                let Ok(payload) = value.get::<String>() else {
                    return false;
                };
                let (Some(from), Some(to)) =
                    (decode_drag_row(&payload), widgets_for_drop.row.get())
                else {
                    return false;
                };
                if !matches!(to, QueueRow::PlayNext(_)) {
                    return false;
                }
                if crate::ui::track_list::queue_row_mapping::reorder_rows(from, to).is_none() {
                    return false;
                }
                let callback = on_reorder.borrow().clone();
                if let Some(callback) = callback {
                    callback(from, to);
                    true
                } else {
                    false
                }
            });
            row_widget.add_controller(widgets.drop_target.clone());
            states.borrow_mut().insert(list_item_key(item), widgets);
            item.set_child(Some(&row_widget));
        });
    }
    {
        let states = states.clone();
        let queue_sections = queue_sections.clone();
        let cover_loader = cover_loader.clone();
        factory.connect_bind(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let Some(widgets) = states.borrow().get(&list_item_key(item)).cloned() else {
                return;
            };
            let Some(boxed) = item
                .item()
                .and_then(|object| object.downcast::<glib::BoxedAnyObject>().ok())
            else {
                return;
            };
            let track = boxed.borrow::<Track>();
            widgets.title.set_label(&track.title);
            widgets.artist.set_label(&track.artist);
            let row = classify(item.position(), &queue_sections.borrow());
            widgets.row.set(row);
            widgets
                .drop_target
                .set_actions(if matches!(row, Some(QueueRow::PlayNext(_))) {
                    gtk4::gdk::DragAction::MOVE
                } else {
                    gtk4::gdk::DragAction::empty()
                });
            let generation = widgets.generation.get().wrapping_add(1);
            widgets.generation.set(generation);
            CoverLoader::set_placeholder(&widgets.cover);
            cover_loader.load_into(
                &widgets.cover,
                &track.path,
                ThumbnailSize::List,
                generation,
                &widgets.generation,
            );
        });
    }
    {
        let states = states.clone();
        factory.connect_unbind(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let Some(widgets) = states.borrow().get(&list_item_key(item)).cloned() else {
                return;
            };
            widgets
                .generation
                .set(widgets.generation.get().wrapping_add(1));
            widgets.row.set(None);
            widgets
                .drop_target
                .set_actions(gtk4::gdk::DragAction::empty());
            widgets.title.set_label("");
            widgets.artist.set_label("");
            CoverLoader::set_placeholder(&widgets.cover);
        });
    }
    {
        let states = states.clone();
        factory.connect_teardown(move |_, object| {
            if let Some(item) = object.downcast_ref::<gtk4::ListItem>() {
                states.borrow_mut().remove(&list_item_key(item));
            }
        });
    }
    factory
}

fn build_row_widgets() -> (gtk4::Box, gtk4::Button, gtk4::Button, RowWidgets) {
    let cover_size = crate::ui::style::tokens::NOW_PLAYING_QUEUE_COVER_SIZE;
    let cover = gtk4::Image::builder()
        .pixel_size(cover_size)
        .width_request(cover_size)
        .height_request(cover_size)
        .build();
    cover.add_css_class("reprise-up-next-cover");
    CoverLoader::set_placeholder(&cover);

    let title = gtk4::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    title.add_css_class("reprise-up-next-title");
    let artist = gtk4::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    artist.add_css_class("reprise-up-next-artist");
    let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
    labels.set_hexpand(true);
    labels.append(&title);
    labels.append(&artist);
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    content.append(&cover);
    content.append(&labels);

    let jump_button = gtk4::Button::builder()
        .child(&content)
        .css_classes(["flat", "reprise-up-next-row"])
        .hexpand(true)
        .build();
    let remove_button = gtk4::Button::builder()
        .icon_name("list-remove-symbolic")
        .tooltip_text(super::strings::remove_from_queue_label(1))
        .css_classes(["flat", "circular", "reprise-up-next-remove"])
        .valign(gtk4::Align::Center)
        .build();
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    row.add_css_class("reprise-up-next-row-container");
    row.append(&jump_button);
    row.append(&remove_button);
    (
        row,
        jump_button,
        remove_button,
        RowWidgets {
            cover,
            title,
            artist,
            generation: Rc::new(Cell::new(0)),
            row: Cell::new(None),
            // input-parity: ACC-8 keyboard=up-next-alt-arrows
            drop_target: gtk4::DropTarget::new(glib::Type::STRING, gtk4::gdk::DragAction::empty()),
        },
    )
}

fn encode_drag_row(row: QueueRow) -> String {
    match row {
        QueueRow::NowPlaying => "playing".to_owned(),
        QueueRow::PlayNext(index) => format!("manual:{index}"),
        QueueRow::UpNext(index) => format!("context:{index}"),
    }
}

fn decode_drag_row(payload: &str) -> Option<QueueRow> {
    let (section, index) = payload.split_once(':')?;
    let index = index.parse().ok()?;
    match section {
        "manual" => Some(QueueRow::PlayNext(index)),
        "context" => Some(QueueRow::UpNext(index)),
        _ => None,
    }
}

fn keyboard_reorder_rows(
    row: QueueRow,
    play_next_len: usize,
    key: gtk4::gdk::Key,
    modifiers: gtk4::gdk::ModifierType,
) -> Option<(QueueRow, QueueRow)> {
    if modifiers != gtk4::gdk::ModifierType::ALT_MASK {
        return None;
    }
    let QueueRow::PlayNext(from) = row else {
        return None;
    };
    let to = match key {
        gtk4::gdk::Key::Up => from.checked_sub(1)?,
        gtk4::gdk::Key::Down => from.checked_add(1).filter(|to| *to < play_next_len)?,
        _ => return None,
    };
    let target = QueueRow::PlayNext(to);
    crate::ui::track_list::queue_row_mapping::reorder_rows(row, target)?;
    Some((row, target))
}

fn install_drag_autoscroll(scrolled: &gtk4::ScrolledWindow) {
    const EDGE_PX: f64 = 48.0;
    const STEP_PX: f64 = 24.0;

    let motion = gtk4::DropControllerMotion::new();
    let scrolled_for_motion = scrolled.clone();
    motion.connect_motion(move |_, _, y| {
        let adjustment = scrolled_for_motion.vadjustment();
        adjustment.set_value(autoscroll_value(
            adjustment.value(),
            adjustment.lower(),
            adjustment.upper(),
            adjustment.page_size(),
            f64::from(scrolled_for_motion.height()),
            y,
            EDGE_PX,
            STEP_PX,
        ));
    });
    scrolled.add_controller(motion);
}

#[allow(clippy::too_many_arguments)]
fn autoscroll_value(
    current: f64,
    lower: f64,
    upper: f64,
    page_size: f64,
    height: f64,
    pointer_y: f64,
    edge: f64,
    step: f64,
) -> f64 {
    let max = (upper - page_size).max(lower);
    let next = if pointer_y < edge {
        current - step
    } else if pointer_y > height - edge {
        current + step
    } else {
        current
    };
    next.clamp(lower, max)
}

pub(in crate::ui) fn css() -> String {
    use crate::ui::style::tokens::{
        MUTED_TEXT_ALPHA, NOW_PLAYING_QUEUE_TITLE_SIZE, RADIUS_SURFACE,
    };
    format!(
        ".reprise-up-next-list {{ padding: 0 12px; }}\n\
         .reprise-up-next-section {{ \
           color: alpha(#ffffff, {MUTED_TEXT_ALPHA}); padding: 12px 6px 5px; }}\n\
         .reprise-up-next-row {{ \
           background: transparent; border: none; box-shadow: none; \
           padding: 5px 6px; }}\n\
         .reprise-up-next-row:hover {{ background: alpha(#ffffff, 0.06); }}\n\
         .reprise-up-next-remove {{ color: alpha(#ffffff, {MUTED_TEXT_ALPHA}); }}\n\
         .reprise-up-next-remove:hover {{ color: #ffffff; }}\n\
         .reprise-up-next-cover {{ border-radius: {RADIUS_SURFACE}; }}\n\
         .reprise-up-next-title {{ \
           color: #ffffff; font-size: {NOW_PLAYING_QUEUE_TITLE_SIZE}; }}\n\
         .reprise-up-next-artist {{ \
           color: alpha(#ffffff, {MUTED_TEXT_ALPHA}); font-size: 11.5px; }}\n\
         .reprise-up-next-empty {{ color: alpha(#ffffff, {MUTED_TEXT_ALPHA}); }}"
    )
}

#[cfg(test)]
#[path = "up_next_panel_tests.rs"]
mod tests;
