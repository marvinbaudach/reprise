//! The compact, read-only Up Next projection inside the Now Playing panel.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;

use super::cover_loader::CoverLoader;
use super::player_controller::PlayerController;
use crate::ui::track_list::queue_row_mapping::QueueRow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct UpNextEntry {
    row: QueueRow,
    title: String,
    artist: String,
    path: String,
    duration_ms: i64,
}

fn upcoming_queue_rows(
    play_next: &[i64],
    context: &[i64],
    current_index: Option<usize>,
) -> Vec<(QueueRow, i64)> {
    let mut rows = play_next
        .iter()
        .copied()
        .enumerate()
        .map(|(index, id)| (QueueRow::PlayNext(index), id))
        .collect::<Vec<_>>();
    if let Some(current_index) = current_index {
        rows.extend(
            context
                .iter()
                .copied()
                .skip(current_index.saturating_add(1))
                .enumerate()
                .map(|(offset, id)| (QueueRow::UpNext(offset), id)),
        );
    }
    rows
}

pub(super) fn format_up_next_footer(durations_ms: &[i64]) -> String {
    let total_duration_ms = durations_ms
        .iter()
        .copied()
        .fold(0_i64, i64::saturating_add);
    let duration = reprise_core::format::format_total_duration(total_duration_ms);
    super::strings::up_next_footer(durations_ms.len(), &duration)
}

type OnJump = Rc<dyn Fn(QueueRow)>;

pub(in crate::ui) struct UpNextPanel {
    root: gtk4::Stack,
    rows: gtk4::Box,
    on_jump: Rc<RefCell<Option<OnJump>>>,
}

impl UpNextPanel {
    pub(in crate::ui) fn new() -> Rc<Self> {
        let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
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
            rows,
            on_jump: Rc::new(RefCell::new(None)),
        })
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    pub(in crate::ui) fn set_on_jump(&self, callback: impl Fn(QueueRow) + 'static) {
        *self.on_jump.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_entries(
        &self,
        entries: &[UpNextEntry],
        cover_loader: &Rc<CoverLoader>,
    ) -> String {
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }

        for entry in entries {
            self.rows
                .append(&build_row(entry, cover_loader, self.on_jump.clone()));
        }
        self.root.set_visible_child_name(if entries.is_empty() {
            "empty"
        } else {
            "tracks"
        });
        let durations = entries
            .iter()
            .map(|entry| entry.duration_ms)
            .collect::<Vec<_>>();
        format_up_next_footer(&durations)
    }
}

fn build_row(
    entry: &UpNextEntry,
    cover_loader: &Rc<CoverLoader>,
    on_jump: Rc<RefCell<Option<OnJump>>>,
) -> gtk4::Button {
    let cover_size = crate::ui::style::tokens::NOW_PLAYING_QUEUE_COVER_SIZE;
    let cover = gtk4::Image::builder()
        .pixel_size(cover_size)
        .width_request(cover_size)
        .height_request(cover_size)
        .build();
    cover.add_css_class("reprise-up-next-cover");
    CoverLoader::set_placeholder(&cover);
    let generation = Rc::new(Cell::new(1));
    cover_loader.load_into(
        &cover,
        &entry.path,
        ThumbnailSize::List,
        generation.get(),
        &generation,
    );

    let title = gtk4::Label::builder()
        .label(&entry.title)
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    title.add_css_class("reprise-up-next-title");
    let artist = gtk4::Label::builder()
        .label(&entry.artist)
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

    let button = gtk4::Button::builder()
        .child(&content)
        .css_classes(["flat", "reprise-up-next-row"])
        .build();
    let row = entry.row;
    button.connect_clicked(move |_| {
        let callback = on_jump.borrow().clone();
        if let Some(callback) = callback {
            callback(row);
        }
    });
    button
}

impl PlayerController {
    pub(in crate::ui) fn now_playing_panel_up_next_entries(&self) -> Vec<UpNextEntry> {
        let play_next = self.up_next.borrow().ids().to_vec();
        let (context, current_index) = {
            let queue = self.queue.borrow();
            (queue.ids_in_order(), queue.current_order_position())
        };
        let rows = upcoming_queue_rows(&play_next, &context, current_index);
        let conn = self.conn.borrow();
        rows.into_iter()
            .filter_map(
                |(row, id)| match reprise_core::queries::query_track_summary(&conn, id) {
                    Ok(Some(track)) => Some(UpNextEntry {
                        row,
                        title: track.title,
                        artist: track.artist,
                        path: track.path,
                        duration_ms: track.duration_ms,
                    }),
                    Ok(None) => {
                        tracing::warn!(id, "up-next panel track no longer exists; skipping row");
                        None
                    }
                    Err(error) => {
                        tracing::warn!(%error, id, "could not load up-next panel track");
                        None
                    }
                },
            )
            .collect()
    }
}

pub(in crate::ui) fn css() -> String {
    use crate::ui::style::tokens::{
        MUTED_TEXT_ALPHA, NOW_PLAYING_QUEUE_TITLE_SIZE, RADIUS_SURFACE,
    };
    format!(
        ".reprise-up-next-list {{ padding: 0 12px; }}\n\
         .reprise-up-next-row {{ \
           background: transparent; border: none; box-shadow: none; \
           padding: 5px 6px; }}\n\
         .reprise-up-next-row:hover {{ background: alpha(#ffffff, 0.06); }}\n\
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

    #[test]
    fn upcoming_tracks_are_manual_entries_then_the_snapshot_after_current() {
        assert_eq!(
            upcoming_queue_rows(&[90, 91], &[10, 20, 30, 40], Some(1)),
            vec![
                (QueueRow::PlayNext(0), 90),
                (QueueRow::PlayNext(1), 91),
                (QueueRow::UpNext(0), 30),
                (QueueRow::UpNext(1), 40),
            ]
        );
    }

    #[test]
    fn upcoming_tracks_handle_an_empty_queue_and_current_at_the_end() {
        assert!(upcoming_queue_rows(&[], &[], None).is_empty());
        assert!(upcoming_queue_rows(&[], &[10, 20], Some(1)).is_empty());
        assert_eq!(
            upcoming_queue_rows(&[90], &[10, 20], Some(1)),
            vec![(QueueRow::PlayNext(0), 90)]
        );
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
        let panel = UpNextPanel::new();
        let jumped = Rc::new(RefCell::new(None));
        let jumped_on_click = jumped.clone();
        panel.set_on_jump(move |row| *jumped_on_click.borrow_mut() = Some(row));
        let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup());
        let entries = [
            test_entry(QueueRow::PlayNext(2), 20),
            test_entry(QueueRow::UpNext(4), 40),
        ];
        panel.set_entries(&entries, &cover_loader);

        let second = panel
            .rows
            .first_child()
            .and_then(|first| first.next_sibling())
            .unwrap()
            .downcast::<gtk4::Button>()
            .unwrap();
        second.emit_clicked();

        assert_eq!(*jumped.borrow(), Some(QueueRow::UpNext(4)));
    }

    fn test_entry(row: QueueRow, id: i64) -> UpNextEntry {
        UpNextEntry {
            row,
            title: format!("Track {id}"),
            artist: "Artist".into(),
            path: format!("/tmp/{id}.mp3"),
            duration_ms: 60_000,
        }
    }
}
