//! Artists master list: the resizable left pane of the Artists master/detail view.
//!
//! A `GtkListView` (row recycling) over a `gio::ListStore` of
//! `BoxedAnyObject<ArtistSummary>`, wrapped in a `SortListModel` so alphabet
//! section headers can be toggled on/off, then a `SingleSelection` that emits
//! the selected artist's display name. The sort `DropDown` (A–Z / Most played
//! / Recently played) re-sorts the store in place; section headers show only
//! in A–Z.
//!
//! ## Per-row now-playing EQ and avatar gradient
//!
//! Row widgetry (the recycled row, its factory, and the now-playing EQ side
//! table) lives in the sibling `artist_master_row` module.
//! `set_now_playing_artist` walks that table to light up whichever realized
//! row matches, rather than forcing a full model rebind.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use crate::ui::artist_master_row::{self, Registry};
use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::discovery_hint::ArtistDiscovery;
use crate::ui::scroll_center;
use crate::ui::strings;
use reprise_core::queries::{self, ArtistSummary};

/// Minimum width of the master pane — the floor the `Paned` drag can shrink it
/// to (see `artist_view::INITIAL_MASTER_WIDTH` for its starting width). Below
/// this, artist names and the sort header would clip.
const PANE_MIN_WIDTH: i32 = 200;

type OnSelect = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;

/// The three sort orders offered by the header `DropDown`, in dropdown order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SortMode {
    Alphabetical,
    MostPlayed,
    RecentlyPlayed,
}

impl SortMode {
    fn from_index(index: u32) -> Self {
        match index {
            1 => Self::MostPlayed,
            2 => Self::RecentlyPlayed,
            _ => Self::Alphabetical,
        }
    }
}

/// Shared, reference-counted state. Held by `ArtistMaster` and by the pieces
/// that outlive a single call (the `DropDown`/selection handlers). GTK objects
/// that themselves store a handler (`selection`, the row factory) capture only
/// the standalone `Rc` fields below — never `Rc<Inner>` — to avoid a
/// widget→closure→`Inner`→widget reference cycle.
struct Inner {
    conn: Rc<RefCell<Connection>>,
    store: gio::ListStore,
    sort_model: gtk4::SortListModel,
    selection: gtk4::SingleSelection,
    list_view: gtk4::ListView,
    header_factory: gtk4::SignalListItemFactory,
    name_sorter: gtk4::CustomSorter,
    section_sorter: gtk4::CustomSorter,
    stack: gtk4::Stack,
    count_label: gtk4::Label,
    rows: Rc<RefCell<Vec<ArtistSummary>>>,
    mode: Rc<Cell<SortMode>>,
    registry: Registry,
    now_playing: Rc<RefCell<Option<String>>>,
    on_select: OnSelect,
}

pub(in crate::ui) struct ArtistMaster {
    root: gtk4::Box,
    inner: Rc<Inner>,
    discovery: ArtistDiscovery,
}

impl ArtistMaster {
    pub(in crate::ui) fn new(
        conn: Rc<RefCell<Connection>>,
        portraits: &Rc<ArtistPortraitRuntime>,
        cover_loader: &Rc<CoverLoader>,
        new_releases_enabled: bool,
    ) -> Self {
        let rows: Rc<RefCell<Vec<ArtistSummary>>> = Rc::new(RefCell::new(Vec::new()));
        let mode = Rc::new(Cell::new(SortMode::Alphabetical));
        let registry: Registry = Rc::new(RefCell::new(HashMap::new()));
        let now_playing: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let on_select: OnSelect = Rc::new(RefCell::new(None));

        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let sort_model = gtk4::SortListModel::new(Some(store.clone()), None::<gtk4::Sorter>);
        let name_sorter = gtk4::CustomSorter::new(name_compare);
        let section_sorter = gtk4::CustomSorter::new(section_compare);

        let selection = gtk4::SingleSelection::builder()
            .model(&sort_model)
            .autoselect(false)
            .can_unselect(true)
            .build();
        wire_selection(&selection, &on_select);

        let discovery = ArtistDiscovery::new(&conn, portraits.enabled.get(), new_releases_enabled);
        let factory = artist_master_row::build_row_factory(
            &registry,
            &now_playing,
            portraits,
            cover_loader,
            &discovery.portrait_evidence(),
        );
        let list_view = gtk4::ListView::new(Some(selection.clone()), Some(factory));
        list_view.add_css_class("artist-list");
        let header_factory = build_header_factory();

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&list_view)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();
        let empty = adw::StatusPage::builder()
            .icon_name("system-users-symbolic")
            .title(strings::text(strings::ARTISTS_EMPTY_TITLE))
            .description(strings::text(strings::ARTISTS_EMPTY_DESCRIPTION))
            .build();
        let stack = gtk4::Stack::new();
        stack.add_named(&scrolled, Some("list"));
        stack.add_named(&empty, Some("empty"));
        stack.set_vexpand(true);

        let count_label = gtk4::Label::new(None);
        let (header, dropdown) = build_header(&count_label);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("artist-master");
        root.set_width_request(PANE_MIN_WIDTH);
        root.append(&header);
        root.append(discovery.widget());
        root.append(&stack);

        let inner = Rc::new(Inner {
            conn,
            store,
            sort_model,
            selection,
            list_view,
            header_factory,
            name_sorter,
            section_sorter,
            stack,
            count_label,
            rows,
            mode,
            registry,
            now_playing,
            on_select,
        });

        wire_dropdown(&dropdown, &inner);

        let master = Self {
            root,
            inner,
            discovery,
        };
        master.reload();
        master
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_on_select(&self, callback: impl Fn(String) + 'static) {
        *self.inner.on_select.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_hint_settings(
        &self,
        callback: impl Fn(&'static [&'static str]) + 'static,
    ) {
        self.discovery.set_on_open_plugins(callback);
    }

    /// Re-runs the query, replaces the cached rows, and re-applies the current
    /// sort. Degrades to an empty list (logged) on a query error — never
    /// panics on a bad connection.
    pub(in crate::ui) fn reload(&self) {
        let loaded = {
            let conn = self.inner.conn.borrow();
            queries::query_artists(&conn).unwrap_or_else(|error| {
                tracing::error!(%error, "artist master: failed to load artists");
                Vec::new()
            })
        };
        *self.inner.rows.borrow_mut() = loaded;
        apply_rows(&self.inner);
    }

    /// Selects the row whose artist matches `artist` (Unicode case-folded, used
    /// by the detail-pane deep link) and scrolls it into the centre of the
    /// viewport. No-op if none match.
    pub(in crate::ui) fn select_artist(&self, artist: &str) {
        if let Some(position) = select_artist_by_name(&self.inner.selection, artist) {
            reveal_centered(&self.inner.list_view, position);
        }
    }

    /// A self-contained `select_artist` callable for the player-bar artist
    /// deep-link (Task 9b): selects the artist *and* scrolls its row to the
    /// centre of the viewport (so the click lands centred on every press, not
    /// just the first realize). Captures clones of the `SingleSelection` and
    /// `ListView` GObjects only — never `self.inner` — so the returned closure
    /// holds no strong reference back to `PlayerController`/the widget tree and
    /// can be stored on the player bar without forming a reference cycle. Both
    /// GObjects are created once (in `new`), stay live via the widget tree, and
    /// `reload` splices the model in place rather than replacing them, so the
    /// captured handles stay valid across refreshes.
    pub(in crate::ui) fn select_callback(&self) -> Rc<dyn Fn(&str)> {
        let selection = self.inner.selection.clone();
        let list_view = self.inner.list_view.clone();
        Rc::new(move |artist: &str| {
            if let Some(position) = select_artist_by_name(&selection, artist) {
                reveal_centered(&list_view, position);
            }
        })
    }

    /// Lights the mini-EQ on whichever realized row matches `artist`
    /// (case-insensitively), clearing every other row. Newly recycled rows
    /// pick the state up from `connect_bind`.
    pub(in crate::ui) fn set_now_playing_artist(&self, artist: Option<String>) {
        *self.inner.now_playing.borrow_mut() = artist;
        let now_playing = self.inner.now_playing.borrow();
        for handles in self.inner.registry.borrow().values() {
            handles.set_now_playing(now_playing.as_deref());
        }
    }

    pub(in crate::ui) fn count(&self) -> u32 {
        self.inner.store.n_items()
    }

    #[cfg(test)]
    pub(in crate::ui) fn select_index_for_test(&self, index: u32) {
        self.inner.selection.set_selected(index);
    }
}

/// The alphabet section key for a name: its uppercased first letter, or `#`
/// for anything non-alphabetic (digits, symbols) or empty.
fn section_key(name: &str) -> String {
    name.chars().next().map_or_else(
        || "#".to_string(),
        |ch| {
            let upper = ch.to_uppercase().next().unwrap_or(ch);
            if upper.is_alphabetic() {
                upper.to_string()
            } else {
                "#".to_string()
            }
        },
    )
}

/// `CustomSorter` comparator grouping consecutive rows into alphabet sections.
/// The store is already alphabetically ordered in A–Z mode, so comparing
/// section keys is enough to mark the section boundaries.
fn section_compare(a: &glib::Object, b: &glib::Object) -> gtk4::Ordering {
    let key_a = boxed_section_key(a);
    let key_b = boxed_section_key(b);
    key_a.cmp(&key_b).into()
}

fn boxed_section_key(obj: &glib::Object) -> String {
    obj.downcast_ref::<glib::BoxedAnyObject>()
        .map(|boxed| section_key(&boxed.borrow::<ArtistSummary>().artist))
        .unwrap_or_default()
}

/// `CustomSorter` comparator sorting rows by case-insensitive artist name.
/// Set as the `SortListModel`'s main sorter in A–Z mode (in addition to the
/// `section_sorter`) so GTK is guaranteed to partition the list into alphabet
/// sections, rather than relying on the pre-sorted splice order alone. The
/// store is already in this order, so this never actually reorders anything.
fn name_compare(a: &glib::Object, b: &glib::Object) -> gtk4::Ordering {
    let name_a = boxed_artist_name(a);
    let name_b = boxed_artist_name(b);
    name_a.to_lowercase().cmp(&name_b.to_lowercase()).into()
}

fn boxed_artist_name(obj: &glib::Object) -> String {
    obj.downcast_ref::<glib::BoxedAnyObject>()
        .map(|boxed| boxed.borrow::<ArtistSummary>().artist.clone())
        .unwrap_or_default()
}

/// The display name of the currently-selected item, if any (used to restore
/// selection across a sort-change splice, which invalidates it).
fn current_selected_artist_name(selection: &gtk4::SingleSelection) -> Option<String> {
    selection
        .selected_item()
        .and_then(|obj| obj.downcast::<glib::BoxedAnyObject>().ok())
        .map(|boxed| boxed.borrow::<ArtistSummary>().artist.clone())
}

/// Selects the row whose artist matches `artist` (Unicode case-folded) and
/// returns its position, or `None` if none match. The position lets a deep-link
/// caller reveal the row (see [`reveal_centered`]); the plain selection restore
/// after a re-sort ignores it, keeping the viewport put.
fn select_artist_by_name(selection: &gtk4::SingleSelection, artist: &str) -> Option<u32> {
    let target = artist.to_lowercase();
    let count = selection.n_items();
    for index in 0..count {
        let matches = selection
            .item(index)
            .and_then(|obj| obj.downcast::<glib::BoxedAnyObject>().ok())
            .is_some_and(|boxed| boxed.borrow::<ArtistSummary>().artist.to_lowercase() == target);
        if matches {
            selection.set_selected(index);
            return Some(index);
        }
    }
    None
}

/// Row count of the master list's current model — the divisor for
/// [`scroll_center::centered_scroll_target`]'s uniform-height row math. The A–Z
/// section headers are not counted here, only the artist rows.
fn master_row_count(list_view: &gtk4::ListView) -> u32 {
    list_view.model().map_or(0, |model| model.n_items())
}

/// Scrolls the master list so row `position` sits vertically centered in the
/// viewport. Mirrors the track table's centering (see
/// `current_track_selection`, both built on [`scroll_center`]): a direct
/// vadjustment write when the list already has geometry, falling back to an idle
/// re-try (then a plain `scroll_to`) when it does not — e.g. the first time the
/// Artists view is shown, before its `ListView` has been allocated. This is what
/// makes a player-bar artist deep-link land centered on *every* click, not just
/// the first realize.
fn reveal_centered(list_view: &gtk4::ListView, position: u32) {
    let n_rows = master_row_count(list_view);
    match scroll_center::centered_scroll_target(list_view, n_rows, position) {
        Some((adjustment, value)) => adjustment.set_value(value),
        None => {
            let list_view = list_view.clone();
            gtk4::glib::idle_add_local_once(move || {
                let n_rows = master_row_count(&list_view);
                match scroll_center::centered_scroll_target(&list_view, n_rows, position) {
                    Some((adjustment, value)) => adjustment.set_value(value),
                    // Still no geometry (or the list fits the viewport): just
                    // make the row visible; there is nothing to center against.
                    None => list_view.scroll_to(position, gtk4::ListScrollFlags::NONE, None),
                }
            });
        }
    }
}

/// Sorts `rows` in place for `mode`. Ties (and the whole A–Z order) fall back
/// to case-insensitive artist name so the order is always deterministic.
fn sort_rows(rows: &mut [ArtistSummary], mode: SortMode) {
    let by_name = |a: &ArtistSummary, b: &ArtistSummary| {
        a.artist.to_lowercase().cmp(&b.artist.to_lowercase())
    };
    match mode {
        SortMode::Alphabetical => rows.sort_by(by_name),
        SortMode::MostPlayed => {
            rows.sort_by(|a, b| {
                b.total_plays
                    .cmp(&a.total_plays)
                    .then_with(|| by_name(a, b))
            });
        }
        SortMode::RecentlyPlayed => {
            rows.sort_by(|a, b| {
                b.last_played_at
                    .cmp(&a.last_played_at)
                    .then_with(|| by_name(a, b))
            });
        }
    }
}

/// Re-sorts the cached rows for the active mode, splices them into the store,
/// toggles the section headers (A–Z only), restores the prior selection, and
/// refreshes the header count and empty-state page.
fn apply_rows(inner: &Inner) {
    let mode = inner.mode.get();
    let mut rows = inner.rows.borrow().clone();
    sort_rows(&mut rows, mode);

    // The splice below invalidates `SingleSelection`, so capture the
    // currently-selected artist first and restore it afterwards.
    let selected_artist = current_selected_artist_name(&inner.selection);

    let objects: Vec<glib::Object> = rows
        .iter()
        .map(|row| glib::BoxedAnyObject::new(row.clone()).upcast())
        .collect();
    inner.store.splice(0, inner.store.n_items(), &objects);

    if mode == SortMode::Alphabetical {
        // Setting the alphabetical order as the *main* sorter too (on top of
        // the already-alphabetically-sorted splice above) costs no
        // reordering, but guarantees GTK actually partitions the list into
        // sections — relying on section-sorter-without-main-sorter to do
        // that from pre-sorted store order alone is unverified.
        inner.sort_model.set_sorter(Some(&inner.name_sorter));
        inner
            .sort_model
            .set_section_sorter(Some(&inner.section_sorter));
        inner
            .list_view
            .set_header_factory(Some(&inner.header_factory));
    } else {
        inner.sort_model.set_sorter(gtk4::Sorter::NONE);
        inner.sort_model.set_section_sorter(gtk4::Sorter::NONE);
        inner
            .list_view
            .set_header_factory(gtk4::ListItemFactory::NONE);
    }

    if let Some(artist) = selected_artist {
        select_artist_by_name(&inner.selection, &artist);
    }

    inner
        .count_label
        .set_text(&strings::artist_master_count(rows.len()));
    inner
        .stack
        .set_visible_child_name(if rows.is_empty() { "empty" } else { "list" });
}

/// Builds the header bar: title, live count, and the sort `DropDown`. Returns
/// the row and the dropdown (wired separately, once `Inner` exists).
fn build_header(count_label: &gtk4::Label) -> (gtk4::Box, gtk4::DropDown) {
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    header.add_css_class("artist-master-header");

    let title = gtk4::Label::new(Some(&strings::text(strings::LIBRARY_VIEW_ARTISTS)));
    title.add_css_class("artist-master-title");
    title.set_xalign(0.0);

    count_label.add_css_class("artist-master-count");
    count_label.add_css_class("dim-label");
    count_label.set_xalign(0.0);
    count_label.set_hexpand(true);

    let dropdown = gtk4::DropDown::from_strings(&[
        &strings::text(strings::ARTIST_SORT_ALPHABETICAL),
        &strings::text(strings::ARTIST_SORT_MOST_PLAYED),
        &strings::text(strings::ARTIST_SORT_RECENTLY_PLAYED),
    ]);
    dropdown.add_css_class("artist-master-sort");

    header.append(&title);
    header.append(count_label);
    header.append(&dropdown);
    (header, dropdown)
}

fn wire_dropdown(dropdown: &gtk4::DropDown, inner: &Rc<Inner>) {
    let inner = inner.clone();
    dropdown.connect_selected_notify(move |dropdown| {
        inner.mode.set(SortMode::from_index(dropdown.selected()));
        apply_rows(&inner);
    });
}

/// On any selection change, emit the selected artist's display name. Skips the
/// no-selection state (nothing selected / just unselected). Captures only the
/// `on_select` cell — never `Inner` — since the closure is stored inside the
/// selection object it observes.
fn wire_selection(selection: &gtk4::SingleSelection, on_select: &OnSelect) {
    let on_select = on_select.clone();
    selection.connect_selected_notify(move |selection| {
        let Some(boxed) = selection
            .selected_item()
            .and_then(|obj| obj.downcast::<glib::BoxedAnyObject>().ok())
        else {
            return;
        };
        let artist = boxed.borrow::<ArtistSummary>().artist.clone();
        let callback = on_select.borrow().clone();
        if let Some(callback) = callback {
            callback(artist);
        }
    });
}

/// The alphabet section-header factory (A–Z mode only): a single left-aligned
/// letter label per section.
fn build_header_factory() -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, obj| {
        let Some(header) = obj.downcast_ref::<gtk4::ListHeader>() else {
            return;
        };
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.add_css_class("artist-list-section");
        header.set_child(Some(&label));
    });
    factory.connect_bind(|_, obj| {
        let Some(header) = obj.downcast_ref::<gtk4::ListHeader>() else {
            return;
        };
        let Some(label) = header
            .child()
            .and_then(|w| w.downcast::<gtk4::Label>().ok())
        else {
            return;
        };
        let key = header
            .item()
            .map(|obj| boxed_section_key(&obj))
            .unwrap_or_default();
        label.set_text(&key);
    });
    factory
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn master_lists_artists_and_emits_selection() {
        if gtk4::init().is_err() {
            return;
        }
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
             ('/1','A','Alpha','X',0),('/2','B','Beta','Y',0);",
        )
        .unwrap();
        let conn = std::rc::Rc::new(std::cell::RefCell::new(conn));
        let selected = std::rc::Rc::new(std::cell::RefCell::new(None));
        let portraits = crate::ui::artist_portrait_worker::ArtistPortraitRuntime::setup_for_test();
        let cover_loader = crate::ui::cover_loader::CoverLoader::new(
            crate::ui::cover_download_worker::setup_for_test(),
        );
        let master = ArtistMaster::new(conn, &portraits, &cover_loader, true);
        master.set_on_select({
            let selected = selected.clone();
            move |artist| *selected.borrow_mut() = Some(artist)
        });
        assert_eq!(master.count(), 2);
        master.select_index_for_test(0);
        assert_eq!(selected.borrow().as_deref(), Some("Alpha"));
    }
}
