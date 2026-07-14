//! Artist grid for the visual Library view.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::queries::{self, ArtistSummary};
use rusqlite::Connection;

use crate::ui::strings;

const GRID_COLUMNS: u32 = 3;
type OnActivate = Rc<dyn Fn(ArtistSummary)>;

#[derive(Clone)]
struct ArtistState {
    conn: Rc<RefCell<Connection>>,
    on_activate: Rc<RefCell<Option<OnActivate>>>,
}

pub(in crate::ui) struct ArtistView {
    root: gtk4::Stack,
    grid: gtk4::FlowBox,
    state: ArtistState,
}

impl ArtistView {
    pub(in crate::ui) fn new(conn: Rc<RefCell<Connection>>) -> Self {
        let grid = gtk4::FlowBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .column_spacing(16)
            .row_spacing(16)
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
            .icon_name("avatar-default-symbolic")
            .title(strings::text(strings::ARTISTS_EMPTY_TITLE))
            .description(strings::text(strings::ARTISTS_EMPTY_DESCRIPTION))
            .build();
        let root = gtk4::Stack::new();
        root.add_named(&scrolled, Some("grid"));
        root.add_named(&empty, Some("empty"));
        let view = Self {
            root,
            grid,
            state: ArtistState {
                conn,
                on_activate: Rc::new(RefCell::new(None)),
            },
        };
        view.refresh();
        view
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    pub(in crate::ui) fn set_on_activate(&self, callback: impl Fn(ArtistSummary) + 'static) {
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
    fn artist_count(&self) -> u32 {
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
            .expect("first artist card button");
        button.emit_clicked();
    }
}

fn refresh_widgets(root: &gtk4::Stack, grid: &gtk4::FlowBox, state: &ArtistState) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
    let artists = {
        let conn = state.conn.borrow();
        queries::query_artists(&conn)
    };
    let artists = match artists {
        Ok(artists) => artists,
        Err(error) => {
            tracing::warn!(%error, "could not load Artists view");
            root.set_visible_child_name("empty");
            return;
        }
    };
    if artists.is_empty() {
        root.set_visible_child_name("empty");
        return;
    }
    for artist in artists {
        grid.append(&build_card(state, artist));
    }
    root.set_visible_child_name("grid");
}

fn build_card(state: &ArtistState, artist: ArtistSummary) -> gtk4::Button {
    let avatar = gtk4::Box::builder()
        .width_request(52)
        .height_request(52)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .build();
    avatar.add_css_class("library-artist-avatar");
    avatar.append(
        &gtk4::Image::builder()
            .icon_name("avatar-default-symbolic")
            .pixel_size(28)
            .build(),
    );

    let title = card_label(&artist.artist, "library-card-title");
    let counts = strings::artist_counts(artist.album_count, artist.track_count);
    let subtitle = card_label(&counts, "library-card-subtitle");
    let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    labels.set_hexpand(true);
    labels.set_valign(gtk4::Align::Center);
    labels.append(&title);
    labels.append(&subtitle);
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    content.append(&avatar);
    content.append(&labels);

    let button = gtk4::Button::builder()
        .child(&content)
        .has_frame(false)
        .tooltip_text(&artist.artist)
        .build();
    button.add_css_class("reprise-surface");
    button.add_css_class("reprise-hover");
    button.add_css_class("library-artist-card");
    let on_activate = state.on_activate.clone();
    button.connect_clicked(move |_| {
        let callback = on_activate.borrow().clone();
        if let Some(callback) = callback {
            callback(artist.clone());
        }
    });
    button
}

fn card_label(text: &str, css_class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_max_width_chars(28);
    label.add_css_class(css_class);
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn artist_cards_are_built_from_the_query_and_emit_the_selected_identity() {
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
        let selected = Rc::new(RefCell::new(None));
        let view = ArtistView::new(conn);
        view.set_on_activate({
            let selected = selected.clone();
            move |artist| *selected.borrow_mut() = Some(artist)
        });

        assert_eq!(view.artist_count(), 2);
        view.activate_first();
        assert_eq!(selected.borrow().as_ref().unwrap().artist, "Artist A");
    }
}
