//! Album cover grid for the visual Library view.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::cover::ThumbnailSize;
use reprise_core::queries::{self, AlbumSummary};
use rusqlite::Connection;

use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

const GRID_COLUMNS: u32 = 4;
const COVER_SIZE: i32 = 184;
type OnActivate = Rc<dyn Fn(AlbumSummary)>;

#[derive(Clone)]
struct AlbumState {
    conn: Rc<RefCell<Connection>>,
    cover_loader: Rc<CoverLoader>,
    generation: Rc<Cell<u64>>,
    on_activate: Rc<RefCell<Option<OnActivate>>>,
}

pub(in crate::ui) struct AlbumView {
    root: gtk4::Stack,
    grid: gtk4::FlowBox,
    state: AlbumState,
}

impl AlbumView {
    pub(in crate::ui) fn new(conn: Rc<RefCell<Connection>>, cover_loader: Rc<CoverLoader>) -> Self {
        let grid = gtk4::FlowBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .column_spacing(16)
            .row_spacing(18)
            .min_children_per_line(1)
            .max_children_per_line(GRID_COLUMNS)
            .homogeneous(true)
            .valign(gtk4::Align::Start)
            .build();
        grid.add_css_class("library-grid");
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&grid)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();
        let empty = adw::StatusPage::builder()
            .icon_name("folder-music-symbolic")
            .title(strings::text(strings::ALBUMS_EMPTY_TITLE))
            .description(strings::text(strings::ALBUMS_EMPTY_DESCRIPTION))
            .build();
        let root = gtk4::Stack::new();
        root.add_named(&scrolled, Some("grid"));
        root.add_named(&empty, Some("empty"));
        let view = Self {
            root,
            grid,
            state: AlbumState {
                conn,
                cover_loader,
                generation: Rc::new(Cell::new(0)),
                on_activate: Rc::new(RefCell::new(None)),
            },
        };
        view.refresh();
        view
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    pub(in crate::ui) fn set_on_activate(&self, callback: impl Fn(AlbumSummary) + 'static) {
        *self.state.on_activate.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn refresh(&self) {
        refresh_widgets(&self.root, &self.grid, &self.state);
    }

    pub(in crate::ui) fn refresh_callback(&self) -> Rc<dyn Fn()> {
        let root = self.root.downgrade();
        let grid = self.grid.downgrade();
        let state = self.state.clone();
        Rc::new(move || {
            let (Some(root), Some(grid)) = (root.upgrade(), grid.upgrade()) else {
                return;
            };
            refresh_widgets(&root, &grid, &state);
        })
    }

    #[cfg(test)]
    fn album_count(&self) -> u32 {
        let mut count = 0;
        while self.grid.child_at_index(count as i32).is_some() {
            count += 1;
        }
        count
    }

    #[cfg(test)]
    fn activate_first(&self) {
        let button = self
            .grid
            .child_at_index(0)
            .and_then(|child| child.child())
            .and_then(|child| child.downcast::<gtk4::Button>().ok())
            .expect("first album card button");
        button.emit_clicked();
    }
}

fn refresh_widgets(root: &gtk4::Stack, grid: &gtk4::FlowBox, state: &AlbumState) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
    let albums = {
        let conn = state.conn.borrow();
        queries::query_albums(&conn)
    };
    let albums = match albums {
        Ok(albums) => albums,
        Err(error) => {
            tracing::warn!(%error, "could not load Albums view");
            root.set_visible_child_name("empty");
            return;
        }
    };
    if albums.is_empty() {
        root.set_visible_child_name("empty");
        return;
    }
    let generation = state.generation.get().wrapping_add(1);
    state.generation.set(generation);
    for album in albums {
        grid.append(&build_card(state, album, generation));
    }
    root.set_visible_child_name("grid");
}

fn build_card(state: &AlbumState, album: AlbumSummary, generation: u64) -> gtk4::Button {
    let image = gtk4::Image::builder()
        .pixel_size(COVER_SIZE)
        .width_request(COVER_SIZE)
        .height_request(COVER_SIZE)
        .build();
    image.add_css_class("library-album-cover");
    state.cover_loader.load_into(
        &image,
        &album.representative_path,
        ThumbnailSize::Grid,
        generation,
        &state.generation,
    );

    let title = card_label(&album.album, "library-card-title");
    let artist = if album.album_artist.is_empty() {
        strings::text(strings::UNKNOWN_ARTIST)
    } else {
        album.album_artist.clone()
    };
    let subtitle = card_label(&artist, "library-card-subtitle");
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
    content.append(&image);
    content.append(&title);
    content.append(&subtitle);

    let button = gtk4::Button::builder()
        .child(&content)
        .has_frame(false)
        .tooltip_text(&album.album)
        .build();
    button.add_css_class("reprise-surface");
    button.add_css_class("reprise-hover");
    button.add_css_class("library-album-card");
    let on_activate = state.on_activate.clone();
    button.connect_clicked(move |_| {
        let callback = on_activate.borrow().clone();
        if let Some(callback) = callback {
            callback(album.clone());
        }
    });
    button
}

fn card_label(text: &str, css_class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_max_width_chars(24);
    label.add_css_class(css_class);
    label
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn album_cards_are_built_from_the_query_and_emit_the_selected_identity() {
        gtk4::init().unwrap();
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
             ('/one.flac','One','Artist A','First',0),
             ('/two.flac','Two','Artist B','Second',0);",
        )
        .unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let loader =
            crate::ui::cover_loader::CoverLoader::new(crate::ui::cover_download_worker::setup());
        let selected = Rc::new(RefCell::new(None));
        let view = AlbumView::new(conn, loader);
        view.set_on_activate({
            let selected = selected.clone();
            move |album| *selected.borrow_mut() = Some(album)
        });

        assert_eq!(view.album_count(), 2);
        view.activate_first();
        assert_eq!(selected.borrow().as_ref().unwrap().album, "First");
    }
}
