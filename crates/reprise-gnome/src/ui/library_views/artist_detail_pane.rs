//! The Artists master/detail view's right (flex) pane: an artist hero with a
//! cover-accent glow, an albums row, and a top-tracks list. Rebuilt on each
//! `show_artist`; Task 8 wires it to the master list's selection.
//!
//! ## Generation guard
//!
//! `show_artist` bumps a single `generation` cell. Every async cover load
//! (`CoverLoader::load_into`) and the off-main accent extraction capture that
//! generation as a token and bail if the cell has moved on by the time they
//! resolve — so a fast artist switch can never apply a stale cover or glow to
//! the freshly-rebuilt pane. This mirrors `album_view.rs` / `cover_loader.rs`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use rusqlite::Connection;

use reprise_core::artist_portrait::PortraitOutcome;
use reprise_core::cover::ThumbnailSize;
use reprise_core::library::artist_detail::{artist_header, artist_top_tracks};
use reprise_core::queries::{query_artist_detail_albums, ArtistAlbum};

use crate::ui::artist_detail_hero::{self, Hero};
use crate::ui::artist_detail_row::{
    build_album_card, build_albums_section, build_top_section, build_top_track_row, TopTrackRow,
};
use crate::ui::artist_portrait_worker::{ArtistPortraitRequest, ArtistPortraitRuntime};
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;
use crate::ui::style::cover_accent::Rgb;

/// Number of top tracks shown before the "Show all …" affordance.
const TOP_TRACK_LIMIT: i64 = 5;
/// Columns in the expanded ("Show all") albums grid.
const ALBUMS_PER_ROW: u32 = 4;
/// Petrol teal fallback (#1CA98F) used for the glow before/without an accent.
const PETROL: Rgb = Rgb {
    r: 28,
    g: 169,
    b: 143,
};

/// A shared, settable callback taking the current artist name (Play all /
/// Shuffle / Show all tracks). Also used by the sibling hero/row modules.
pub(in crate::ui) type ArtistCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;
/// A shared, settable callback taking an activated album and its artist.
pub(in crate::ui) type AlbumCallback = Rc<RefCell<Option<Rc<dyn Fn(ArtistAlbum, String)>>>>;

/// The Play all / Shuffle / ⋮-menu / current-artist plumbing the hero's buttons
/// and menu items capture.
pub(in crate::ui) struct HeroCallbacks {
    pub(in crate::ui) on_play_all: ArtistCallback,
    pub(in crate::ui) on_shuffle: ArtistCallback,
    pub(in crate::ui) on_add_to_queue: ArtistCallback,
    pub(in crate::ui) on_go_to_folder: ArtistCallback,
    pub(in crate::ui) current_artist: Rc<RefCell<String>>,
}

/// Shared, reference-counted pane state. Held by the pane and by the
/// long-lived async accent closure.
struct Inner {
    conn: Rc<RefCell<Connection>>,
    cover_loader: Rc<CoverLoader>,
    portraits: Rc<ArtistPortraitRuntime>,
    generation: Rc<Cell<u64>>,
    current_artist: Rc<RefCell<String>>,
    on_play_all: ArtistCallback,
    on_shuffle: ArtistCallback,
    on_add_to_queue: ArtistCallback,
    on_go_to_folder: ArtistCallback,
    on_show_all_tracks: ArtistCallback,
    on_album_activate: AlbumCallback,
    hero: Hero,
    albums_flow: gtk4::FlowBox,
    albums_hint: gtk4::Label,
    albums_show_all: gtk4::Button,
    albums_expanded: Rc<Cell<bool>>,
    album_count: Rc<Cell<u32>>,
    top_tracks_box: gtk4::Box,
    top_show_all: gtk4::Button,
    top_rows: RefCell<Vec<TopTrackRow>>,
    now_playing_track: Cell<Option<i64>>,
}

pub(in crate::ui) struct ArtistDetailPane {
    root: gtk4::ScrolledWindow,
    inner: Rc<Inner>,
}

impl ArtistDetailPane {
    pub(in crate::ui) fn new(
        conn: Rc<RefCell<Connection>>,
        cover_loader: Rc<CoverLoader>,
        portraits: Rc<ArtistPortraitRuntime>,
    ) -> Self {
        let current_artist = Rc::new(RefCell::new(String::new()));
        let on_play_all: ArtistCallback = Rc::new(RefCell::new(None));
        let on_shuffle: ArtistCallback = Rc::new(RefCell::new(None));
        let on_add_to_queue: ArtistCallback = Rc::new(RefCell::new(None));
        let on_go_to_folder: ArtistCallback = Rc::new(RefCell::new(None));
        let on_show_all_tracks: ArtistCallback = Rc::new(RefCell::new(None));
        let on_album_activate: AlbumCallback = Rc::new(RefCell::new(None));

        let hero = artist_detail_hero::build_hero(&HeroCallbacks {
            on_play_all: on_play_all.clone(),
            on_shuffle: on_shuffle.clone(),
            on_add_to_queue: on_add_to_queue.clone(),
            on_go_to_folder: on_go_to_folder.clone(),
            current_artist: current_artist.clone(),
        });

        let (albums_section, albums_flow, albums_hint, albums_show_all) =
            build_albums_section(ALBUMS_PER_ROW);
        let (top_section, top_tracks_box, top_show_all) = build_top_section();

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 24);
        column.add_css_class("artist-detail");
        column.append(hero.widget());
        column.append(&albums_section);
        column.append(&top_section);

        let root = gtk4::ScrolledWindow::builder()
            .child(&column)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();

        let inner = Rc::new(Inner {
            conn,
            cover_loader,
            portraits,
            generation: Rc::new(Cell::new(0)),
            current_artist,
            on_play_all,
            on_shuffle,
            on_add_to_queue,
            on_go_to_folder,
            on_show_all_tracks,
            on_album_activate,
            hero,
            albums_flow,
            albums_hint,
            albums_show_all,
            albums_expanded: Rc::new(Cell::new(false)),
            album_count: Rc::new(Cell::new(0)),
            top_tracks_box,
            top_show_all,
            top_rows: RefCell::new(Vec::new()),
            now_playing_track: Cell::new(None),
        });

        wire_albums_show_all(&inner);
        wire_top_show_all(&inner);
        subscribe_portrait_enabled(&inner.portraits, &inner);

        Self { root, inner }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(in crate::ui) fn set_on_play_all(&self, callback: impl Fn(String) + 'static) {
        *self.inner.on_play_all.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_shuffle(&self, callback: impl Fn(String) + 'static) {
        *self.inner.on_shuffle.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_add_to_queue(&self, callback: impl Fn(String) + 'static) {
        *self.inner.on_add_to_queue.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_go_to_folder(&self, callback: impl Fn(String) + 'static) {
        *self.inner.on_go_to_folder.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_show_all_tracks(&self, callback: impl Fn(String) + 'static) {
        *self.inner.on_show_all_tracks.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_album_activate(
        &self,
        callback: impl Fn(ArtistAlbum, String) + 'static,
    ) {
        *self.inner.on_album_activate.borrow_mut() = Some(Rc::new(callback));
    }

    /// Rebuilds the pane for `artist`. Bumps the generation to cancel any
    /// still-pending cover/accent callbacks from the previous artist.
    pub(in crate::ui) fn show_artist(&self, artist: &str, now_unix: i64) {
        *self.inner.current_artist.borrow_mut() = artist.to_string();
        let token = self.inner.generation.get().wrapping_add(1);
        self.inner.generation.set(token);

        let (header, top_tracks, albums) = {
            let conn = self.inner.conn.borrow();
            let header = artist_header(&conn, artist, now_unix).unwrap_or_else(|error| {
                tracing::warn!(%error, artist, "artist detail: header query failed");
                reprise_core::library::artist_detail::ArtistHeader {
                    album_count: 0,
                    track_count: 0,
                    catalog_ms: 0,
                    plays_this_year: 0,
                }
            });
            let top = artist_top_tracks(&conn, artist, TOP_TRACK_LIMIT).unwrap_or_default();
            let albums = query_artist_detail_albums(&conn, artist).unwrap_or_default();
            (header, top, albums)
        };

        self.inner.hero.update(artist, &header);
        request_portrait(&self.inner, artist.to_string());
        rebuild_albums(&self.inner, artist, albums, token);
        rebuild_top_tracks(&self.inner, &top_tracks, header.track_count, token);
    }

    /// Lights the matching top-track row's `EqBars`, clearing every other row.
    pub(in crate::ui) fn set_now_playing_track(&self, track_id: Option<i64>) {
        self.inner.now_playing_track.set(track_id);
        for row in self.inner.top_rows.borrow().iter() {
            row.set_now_playing(track_id);
        }
    }

    #[cfg(test)]
    pub(in crate::ui) fn hero_name(&self) -> String {
        self.inner.hero.name_text()
    }

    #[cfg(test)]
    fn top_track_row_count(&self) -> usize {
        self.inner.top_rows.borrow().len()
    }
}

fn request_portrait(inner: &Rc<Inner>, artist: String) {
    if !inner.portraits.enabled.get() || artist.trim().is_empty() {
        return;
    }
    let generation = inner.generation.get();
    let (sender, receiver) = async_channel::bounded(1);
    inner.portraits.request(ArtistPortraitRequest {
        generation,
        artist,
        force: false,
        response: sender,
    });
    let inner = inner.clone();
    glib::spawn_future_local(async move {
        let Ok(response) = receiver.recv().await else {
            return;
        };
        if response.generation != inner.generation.get() || !inner.portraits.enabled.get() {
            return;
        }
        if let Ok(PortraitOutcome::Found(path)) = response.result {
            if let Ok(texture) = gtk4::gdk::Texture::from_filename(&path) {
                inner.hero.set_portrait(&texture);
            }
        }
    });
}

fn subscribe_portrait_enabled(portraits: &Rc<ArtistPortraitRuntime>, inner: &Rc<Inner>) {
    let alive = Rc::downgrade(inner);
    let target = alive.clone();
    portraits.subscribe_enabled(
        move || alive.upgrade().is_some(),
        move |enabled| {
            let Some(inner) = target.upgrade() else {
                return;
            };
            if !enabled {
                inner.hero.clear_portrait();
                return;
            }
            let artist = inner.current_artist.borrow().clone();
            request_portrait(&inner, artist);
        },
    );
}

/// Repopulates the albums FlowBox. Empty albums clear the glow and show the
/// hint; otherwise the glow starts at the Petrol fallback and upgrades once the
/// first album's cover accent resolves off-main.
fn rebuild_albums(inner: &Rc<Inner>, artist: &str, albums: Vec<ArtistAlbum>, token: u64) {
    clear_children(inner.albums_flow.upcast_ref());
    inner.albums_expanded.set(false);
    inner.album_count.set(albums.len() as u32);

    if albums.is_empty() {
        inner.albums_flow.set_visible(false);
        inner.albums_show_all.set_visible(false);
        inner.albums_hint.set_visible(true);
        inner.hero.clear_glow();
        return;
    }

    inner.albums_flow.set_visible(true);
    inner.albums_hint.set_visible(false);
    inner
        .albums_show_all
        .set_visible(albums.len() as u32 > ALBUMS_PER_ROW);
    inner
        .albums_show_all
        .set_label(&strings::text(strings::ARTIST_DETAIL_SHOW_ALL));

    let accent_path = albums[0].representative_path.clone();
    for album in albums {
        let card = build_album_card(
            &inner.cover_loader,
            &inner.generation,
            token,
            album,
            artist.to_string(),
            inner.on_album_activate.clone(),
        );
        inner.albums_flow.append(&card);
    }
    apply_albums_layout(
        &inner.albums_flow,
        inner.albums_expanded.get(),
        inner.album_count.get(),
    );

    // Immediate Petrol fallback; upgraded when the cover accent resolves.
    inner.hero.set_glow_accent(PETROL);
    spawn_accent(inner, token, accent_path);
}

/// Repopulates the top-tracks list and its Show-all button.
fn rebuild_top_tracks(
    inner: &Rc<Inner>,
    tracks: &[reprise_core::library::artist_detail::ArtistTopTrack],
    total_tracks: i64,
    token: u64,
) {
    clear_children(inner.top_tracks_box.upcast_ref());
    let mut handles = Vec::with_capacity(tracks.len());
    for (index, track) in tracks.iter().enumerate() {
        let (row, handle) = build_top_track_row(
            &inner.cover_loader,
            &inner.generation,
            token,
            index + 1,
            track,
        );
        inner.top_tracks_box.append(&row);
        handles.push(handle);
    }
    *inner.top_rows.borrow_mut() = handles;

    // Re-apply the now-playing indicator to the freshly built rows.
    let now_playing = inner.now_playing_track.get();
    for row in inner.top_rows.borrow().iter() {
        row.set_now_playing(now_playing);
    }

    let has_more = total_tracks > TOP_TRACK_LIMIT;
    inner.top_show_all.set_visible(has_more);
    if has_more {
        inner
            .top_show_all
            .set_label(&strings::artist_detail_show_all_tracks(total_tracks));
    }
}

/// Applies the collapsed (single-row) vs. expanded (grid) FlowBox layout.
fn apply_albums_layout(flow: &gtk4::FlowBox, expanded: bool, count: u32) {
    let count = count.max(1);
    if expanded {
        flow.set_min_children_per_line(1);
        flow.set_max_children_per_line(ALBUMS_PER_ROW);
        flow.remove_css_class("collapsed");
    } else {
        // Force a single line; the pane's `hscrollbar_policy(Never)` clips the
        // overflow, so only the first row is visible until "Show all".
        flow.set_min_children_per_line(count);
        flow.set_max_children_per_line(count);
        flow.add_css_class("collapsed");
    }
}

/// Wires the albums Show-all toggle. Captures only the standalone flow/state
/// clones — never `Rc<Inner>` — so the closure stored on this button (which
/// `Inner` owns) can't form a widget → closure → `Inner` → widget cycle.
fn wire_albums_show_all(inner: &Rc<Inner>) {
    let flow = inner.albums_flow.clone();
    let expanded = inner.albums_expanded.clone();
    let album_count = inner.album_count.clone();
    inner.albums_show_all.connect_clicked(move |button| {
        let now = !expanded.get();
        expanded.set(now);
        apply_albums_layout(&flow, now, album_count.get());
        let label = if now {
            strings::text(strings::ARTIST_DETAIL_SHOW_LESS)
        } else {
            strings::text(strings::ARTIST_DETAIL_SHOW_ALL)
        };
        button.set_label(&label);
    });
}

fn wire_top_show_all(inner: &Rc<Inner>) {
    let on_show_all_tracks = inner.on_show_all_tracks.clone();
    let current_artist = inner.current_artist.clone();
    inner.top_show_all.connect_clicked(move |_| {
        let cb = on_show_all_tracks.borrow().clone();
        if let Some(cb) = cb {
            let artist = current_artist.borrow().clone();
            cb(artist);
        }
    });
}

/// Off-main cover-accent extraction for the hero glow, generation-guarded.
fn spawn_accent(inner: &Rc<Inner>, token: u64, track_path: String) {
    let inner = inner.clone();
    glib::spawn_future_local(async move {
        let accent = gio::spawn_blocking(move || {
            let source = reprise_core::cover::resolve_source(std::path::Path::new(&track_path))?;
            let thumb = reprise_core::cover::thumbnail(&source, ThumbnailSize::Grid).ok()?;
            crate::ui::style::cover_accent::accent_from_cover_file(&thumb)
        })
        .await
        .ok()
        .flatten();

        // Bail if a newer artist has been shown while we were decoding.
        if inner.generation.get() != token {
            return;
        }
        inner.hero.set_glow_accent(accent.unwrap_or(PETROL));
    });
}

/// Removes every child of a container widget (FlowBox children come wrapped in
/// their own `FlowBoxChild`, so `first_child`/`remove` handles both).
fn clear_children(container: &gtk4::Widget) {
    while let Some(child) = container.first_child() {
        if let Some(flow) = container.downcast_ref::<gtk4::FlowBox>() {
            if let Some(flow_child) = child.downcast_ref::<gtk4::FlowBoxChild>() {
                flow.remove(flow_child);
                continue;
            }
        }
        if let Some(boxed) = container.downcast_ref::<gtk4::Box>() {
            boxed.remove(&child);
            continue;
        }
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn show_artist_renders_hero_name_and_top_tracks() {
        if gtk4::init().is_err() {
            return;
        }
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id,path,title,artist,album,album_artist,year,\
               duration_ms,play_count,last_played_at,added_at) VALUES
             (1,'/a.flac','A','Solo','One','Solo',2020,180000,5,100,0),
             (2,'/b.flac','B','Solo','One','Solo',2020,120000,2,50,0),
             (3,'/c.flac','C','Solo','Two','Solo',2022,200000,9,200,0);",
        )
        .unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let loader =
            crate::ui::cover_loader::CoverLoader::new(crate::ui::cover_download_worker::setup());

        // 2026-07-15T00:00:00Z — a fixed reference "now" so the year window is
        // clock-independent.
        const NOW: i64 = 1_784_073_600;
        let portraits =
            crate::ui::artist_portrait_worker::ArtistPortraitRuntime::setup(&conn.borrow());
        let pane = ArtistDetailPane::new(conn, loader, portraits);
        pane.show_artist("Solo", NOW);

        assert_eq!(pane.hero_name(), "Solo");
        assert!(pane.top_track_row_count() >= 1);
    }
}
