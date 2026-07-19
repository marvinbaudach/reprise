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

struct RowWidgets {
    cover: gtk4::Image,
    title: gtk4::Label,
    artist: gtk4::Label,
    generation: Rc<Cell<u64>>,
    row: Cell<Option<QueueRow>>,
}

pub(in crate::ui) struct UpNextPanel {
    root: gtk4::Stack,
    model: TrackListModel,
    queue_sections: Rc<RefCell<Vec<QueueSection>>>,
    section_headers: Rc<RefCell<Vec<(u32, String)>>>,
    on_jump: Rc<RefCell<Option<OnJump>>>,
    on_remove: Rc<RefCell<Option<OnRemove>>>,
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
        let factory = build_factory(cover_loader, &queue_sections, &on_jump, &on_remove);
        let selection = gtk4::NoSelection::new(Some(model.clone()));
        let rows = gtk4::ListView::new(Some(selection), Some(factory));
        rows.set_header_factory(Some(&build_header_factory(&section_headers)));
        rows.add_css_class("reprise-up-next-list");
        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .child(&rows)
            .build();
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
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    let states: Rc<RefCell<HashMap<usize, Rc<RowWidgets>>>> = Rc::new(RefCell::new(HashMap::new()));
    {
        let states = states.clone();
        let on_jump = on_jump.clone();
        let on_remove = on_remove.clone();
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
            widgets
                .row
                .set(classify(item.position(), &queue_sections.borrow()));
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
        },
    )
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
mod tests {
    use super::*;

    fn collect_buttons_with_class(
        widget: &gtk4::Widget,
        class: &str,
        buttons: &mut Vec<gtk4::Button>,
    ) {
        if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
            if button.has_css_class(class) {
                buttons.push(button);
            }
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            collect_buttons_with_class(&widget, class, buttons);
            child = widget.next_sibling();
        }
    }

    #[test]
    fn upcoming_tracks_are_manual_entries_then_the_snapshot_after_current() {
        let model = crate::ui::track_list::queue_sections::compose(
            Some(10),
            &[90, 91],
            &[30, 40],
            Some("Music"),
        )
        .upcoming();
        assert_eq!(
            queue_rows(&model),
            vec![
                QueueRow::PlayNext(0),
                QueueRow::PlayNext(1),
                QueueRow::UpNext(0),
                QueueRow::UpNext(1),
            ]
        );
    }

    #[test]
    fn upcoming_tracks_handle_an_empty_queue_and_current_at_the_end() {
        let empty = crate::ui::track_list::queue_sections::compose(None, &[], &[], None);
        assert!(queue_rows(&empty.upcoming()).is_empty());
        let only_current = crate::ui::track_list::queue_sections::compose(Some(20), &[], &[], None);
        assert!(queue_rows(&only_current.upcoming()).is_empty());
        let manual =
            crate::ui::track_list::queue_sections::compose(Some(20), &[90], &[], None).upcoming();
        assert_eq!(queue_rows(&manual), vec![QueueRow::PlayNext(0)]);
    }

    #[test]
    fn que_2_two_sections_headers_conditional() {
        let both = crate::ui::track_list::queue_sections::compose(
            Some(10),
            &[20, 21],
            &[30],
            Some("Late Night"),
        )
        .upcoming();
        assert_eq!(
            panel_section_headers(&both),
            vec![
                (0, "Next in Queue".to_owned()),
                (2, "Playing from Late Night · 1 track".to_owned()),
            ]
        );

        let automatic_only =
            crate::ui::track_list::queue_sections::compose(Some(10), &[], &[30], Some("Album"))
                .upcoming();
        assert_eq!(
            panel_section_headers(&automatic_only),
            vec![(0, "Playing from Album · 1 track".to_owned())]
        );

        let manual_only =
            crate::ui::track_list::queue_sections::compose(Some(10), &[20], &[], None).upcoming();
        assert_eq!(
            panel_section_headers(&manual_only),
            vec![(0, "Next in Queue".to_owned())]
        );
        assert!(panel_section_headers(&QueueViewModel::default()).is_empty());
    }

    #[test]
    fn footer_formats_track_count_and_remaining_duration() {
        assert_eq!(format_up_next_footer(&[]), "0 tracks · 0 minutes");
        assert_eq!(format_up_next_footer(&[90_000]), "1 track · 1 minute");
        assert_eq!(
            format_up_next_footer(&[90_000, 330_000]),
            "2 tracks · 7 minutes"
        );
    }

    #[test]
    fn row_css_and_metrics_match_the_compact_21a_spec() {
        let css = css();
        assert_eq!(crate::ui::style::tokens::NOW_PLAYING_QUEUE_COVER_SIZE, 32);
        assert!(css.contains(".reprise-up-next-row"));
        assert!(css.contains("font-size: 13.5px"));
        assert!(!css.contains("reorder"));
        assert!(!css.contains("context-menu"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn up_next_row_click_jumps_to_the_exact_queue_entry() {
        gtk4::init().unwrap();
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
             (20, '/tmp/20.mp3', 'Track 20', 'Artist', 0),
             (40, '/tmp/40.mp3', 'Track 40', 'Artist', 0);",
        )
        .unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup());
        let panel = UpNextPanel::new(conn, &cover_loader);
        let jumped = Rc::new(RefCell::new(None));
        let jumped_on_click = jumped.clone();
        panel.set_on_jump(move |row| *jumped_on_click.borrow_mut() = Some(row));
        let model =
            crate::ui::track_list::queue_sections::compose(Some(10), &[20], &[40], Some("Music"));
        panel.set_queue_model(&model);
        let window = gtk4::Window::builder().child(panel.widget()).build();
        window.present();
        while glib::MainContext::default().iteration(false) {}

        let mut buttons = Vec::new();
        collect_buttons_with_class(
            panel.widget().upcast_ref(),
            "reprise-up-next-row",
            &mut buttons,
        );
        buttons[1].emit_clicked();

        assert_eq!(*jumped.borrow(), Some(QueueRow::UpNext(0)));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn panel_remove_targets_the_exact_queue_entry() {
        gtk4::init().unwrap();
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
             (20, '/tmp/20.mp3', 'Track 20', 'Artist', 0),
             (40, '/tmp/40.mp3', 'Track 40', 'Artist', 0);",
        )
        .unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup());
        let panel = UpNextPanel::new(conn, &cover_loader);
        let removed = Rc::new(RefCell::new(None));
        let removed_on_click = removed.clone();
        panel.set_on_remove(move |row| *removed_on_click.borrow_mut() = Some(row));
        let model =
            crate::ui::track_list::queue_sections::compose(Some(10), &[20], &[40], Some("Music"));
        panel.set_queue_model(&model);
        let window = gtk4::Window::builder().child(panel.widget()).build();
        window.present();
        while glib::MainContext::default().iteration(false) {}

        let mut buttons = Vec::new();
        collect_buttons_with_class(
            panel.widget().upcast_ref(),
            "reprise-up-next-remove",
            &mut buttons,
        );
        buttons[1].emit_clicked();

        assert_eq!(*removed.borrow(), Some(QueueRow::UpNext(0)));
    }
}
