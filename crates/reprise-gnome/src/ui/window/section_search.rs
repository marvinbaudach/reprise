//! SEARCH-8a: a query belongs only to the view where it is typed.
//!
//! The header bar owns a single `GtkSearchEntry`, but the query it holds
//! is deliberately transient: a sidebar destination switch clears the view
//! being left, starts the destination empty, and collapses the bar. Metadata
//! drills carry the query in their `BrowserPlace`, and Back restores the
//! complete saved place; this module does not keep parallel history.
//!
//! Three invariants make the rest of the shell simple:
//!
//! * The entry text is always the active scope's query. Every write pushed
//!   back by a view that cleared its own chip goes through here, so no
//!   participant has to guess which section a query belongs to.
//! * Query clearing calls only a section's `apply` handler. Its separately
//!   stored facet filters remain untouched unless the user invokes Clear all.
//! * A section without a list ([`SectionSearch::supports_search`] is false) can neither be
//!   searched nor reveal the bar: the lens is insensitive with a tooltip that
//!   names the section, Ctrl+F is a no-op, and typing cannot open the strip.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::view_source::ViewSource;
use reprise_view::search_scope::{self, SearchScope};

use crate::ui::browse_filter_strings as filter_strings;
use crate::ui::strings;

/// The content-stack page names this module has to recognise by hand: the
/// shared track page serves several sources, and the device page has no
/// `ViewSource` at all.
const LIBRARY_PAGE: &str = "library";
const DEVICE_SYNC_PAGE: &str = "device-sync";

/// The source behind a content-stack page, for the pages that have exactly
/// one. Pure, so the mapping is testable without a shell.
fn page_source(page: &str) -> Option<ViewSource> {
    match page {
        "stats" => Some(ViewSource::MyStats),
        "concerts" => Some(ViewSource::Concerts),
        "releases" => Some(ViewSource::Releases),
        "podcasts" => Some(ViewSource::Podcasts),
        "youtube" => Some(ViewSource::Youtube),
        "radio" => Some(ViewSource::Radio),
        _ => None,
    }
}

/// What a section does with its query. `apply` runs whenever the query for
/// that scope changes *and* whenever the section becomes visible again;
/// `clear_facets` is the section's own half of "Clear all".
struct SectionHandlers {
    apply: Rc<dyn Fn(&str)>,
    clear_facets: Rc<dyn Fn()>,
}

/// The shell state that answers "which section is visible right now".
struct ShellState {
    content_stack: gtk4::glib::WeakRef<gtk4::Stack>,
    window_title: gtk4::glib::WeakRef<libadwaita::WindowTitle>,
    current_source: Rc<dyn Fn() -> ViewSource>,
}

/// Every widget reference here is weak, deliberately. `SectionSearch` is
/// owned by the closures that consult it — a window action, the entry's own
/// signal handlers, the sidebar's `on_select` — and those closures are owned
/// by widgets under the window. A strong clone of the entry, the lens, or
/// (worst) the window itself as `key_capture` would close that loop and keep
/// the whole window alive past its own destruction: GTK's dispose normally
/// papers over it, but the test harness and any window replaced without
/// being closed would leak the pair. Weak here breaks the cycle at the end
/// that has no business owning anything.
pub(in crate::ui) struct SectionSearch {
    entry: gtk4::glib::WeakRef<gtk4::SearchEntry>,
    search_bar: gtk4::glib::WeakRef<gtk4::SearchBar>,
    toggle: gtk4::glib::WeakRef<gtk4::ToggleButton>,
    key_capture: gtk4::glib::WeakRef<gtk4::Widget>,
    active: Cell<SearchScope>,
    active_source: RefCell<Option<ViewSource>>,
    handlers: RefCell<BTreeMap<SearchScope, SectionHandlers>>,
    shell: RefCell<Option<ShellState>>,
}

impl SectionSearch {
    pub(in crate::ui) fn new(
        entry: &gtk4::SearchEntry,
        search_bar: &gtk4::SearchBar,
        toggle: &gtk4::ToggleButton,
        key_capture: &impl IsA<gtk4::Widget>,
    ) -> Rc<Self> {
        let search = Rc::new(Self {
            entry: entry.downgrade(),
            search_bar: search_bar.downgrade(),
            toggle: toggle.downgrade(),
            key_capture: key_capture.clone().upcast::<gtk4::Widget>().downgrade(),
            active: Cell::new(SearchScope::Tracks),
            active_source: RefCell::new(None),
            handlers: RefCell::new(BTreeMap::new()),
            shell: RefCell::new(None),
        });
        // The debounced signal is what actually re-filters a list.
        let weak = Rc::downgrade(&search);
        entry.connect_search_changed(move |entry| {
            let Some(search) = weak.upgrade() else {
                return;
            };
            search.apply_to_active(&entry.text());
        });
        search
    }

    /// Registers a section's query sink. `apply` receives the raw query;
    /// `clear_facets` drops that section's facet filters for "Clear all".
    pub(in crate::ui) fn register(
        &self,
        scope: SearchScope,
        apply: impl Fn(&str) + 'static,
        clear_facets: impl Fn() + 'static,
    ) {
        self.handlers.borrow_mut().insert(
            scope,
            SectionHandlers {
                apply: Rc::new(apply),
                clear_facets: Rc::new(clear_facets),
            },
        );
    }

    /// Whether the *active* section can be searched at all. `view_session`
    /// and the other per-scope sinks consult this before acting on a query,
    /// so a query typed in Podcasts never reaches the track list.
    pub(in crate::ui) fn is_active(&self, scope: SearchScope) -> bool {
        self.active.get() == scope
    }

    pub(in crate::ui) fn supports_search(&self) -> bool {
        search_scope::supports_search(self.active.get())
    }

    /// Binds the shell's own state as the authority for "which section is
    /// visible": the content stack's page, the track list's source for the
    /// shared track page, and the window title for the section's name. Every
    /// route — sidebar, Back/Forward, the now-playing jump, the device page —
    /// already moves those, so none of them has to know this module exists.
    pub(in crate::ui) fn observe(
        self: &Rc<Self>,
        content_stack: &gtk4::Stack,
        window_title: &libadwaita::WindowTitle,
        current_source: impl Fn() -> ViewSource + 'static,
    ) {
        *self.shell.borrow_mut() = Some(ShellState {
            content_stack: content_stack.downgrade(),
            window_title: window_title.downgrade(),
            current_source: Rc::new(current_source),
        });
        let weak = Rc::downgrade(self);
        content_stack.connect_visible_child_name_notify(move |_| {
            if let Some(search) = weak.upgrade() {
                search.refresh_later();
            }
        });
        self.refresh_later();
    }

    /// Re-resolves the visible section in an idle. Deferred on purpose: a
    /// route sets the stack page before it sets the window title, so reading
    /// both in the same turn would name the section the user just left.
    pub(in crate::ui) fn refresh_later(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        gtk4::glib::idle_add_local_once(move || {
            if let Some(search) = weak.upgrade() {
                search.refresh();
            }
        });
    }

    fn refresh(self: &Rc<Self>) {
        let Some((scope, name)) = self.visible_section() else {
            return;
        };
        self.activate(scope, &name);
    }

    fn visible_section(&self) -> Option<(SearchScope, String)> {
        let shell = self.shell.borrow();
        let shell = shell.as_ref()?;
        let content_stack = shell.content_stack.upgrade()?;
        let page = content_stack.visible_child_name()?;
        let name = shell
            .window_title
            .upgrade()
            .map(|title| title.title().to_string())
            .unwrap_or_default();
        let scope = match page.as_str() {
            DEVICE_SYNC_PAGE => SearchScope::Unsupported,
            LIBRARY_PAGE => search_scope::scope_for(&(shell.current_source)()),
            other => page_source(other).map_or(SearchScope::Unsupported, |source| {
                search_scope::scope_for(&source)
            }),
        };
        Some((scope, name))
    }

    /// SEARCH-8a: a distinct sidebar source starts a new search context even
    /// when both sources share the track-list scope. Re-routing the exact
    /// active source is a no-op; source identity is tracked separately because
    /// scope equality alone cannot distinguish Library from Recently Added.
    /// Metadata drills bypass this clearing path, while Back restoration is
    /// applied later from the history-owned `BrowserPlace`, not remembered
    /// here.
    pub(in crate::ui) fn activate_source(self: &Rc<Self>, source: &ViewSource, section_name: &str) {
        let scope = search_scope::scope_for(source);
        let already_active = self.active.get() == scope
            && self
                .active_source
                .borrow()
                .as_ref()
                .is_some_and(|active| active == source);
        if already_active {
            self.sync_affordance(section_name);
            return;
        }
        *self.active_source.borrow_mut() = Some(source.clone());
        self.switch_view(scope, section_name);
    }

    pub(in crate::ui) fn activate(self: &Rc<Self>, scope: SearchScope, section_name: &str) {
        if self.active.get() == scope {
            // Still refresh the affordance: an observer can repeat the same
            // scope after the route already changed the visible title.
            self.sync_affordance(section_name);
            return;
        }
        self.switch_view(scope, section_name);
    }

    fn switch_view(&self, scope: SearchScope, section_name: &str) {
        let previous = self.active.replace(scope);
        self.apply_to_scope(previous, "");
        let entry_changed = self.write_entry("");
        self.collapse_bar();
        self.sync_affordance(section_name);
        if !entry_changed {
            // Identical text emits no signal, so reset explicitly.
            self.apply_to_scope(scope, "");
        }
    }

    /// A view removed its own query (the chip's ×, or a jump that had to
    /// relax the search to reach its row). The entry follows so the two never
    /// disagree about what is filtered.
    pub(in crate::ui) fn set_query(self: &Rc<Self>, scope: SearchScope, query: &str) {
        if self.active.get() != scope {
            return;
        }
        if self.entry_text().trim() == query.trim() {
            return;
        }
        self.write_entry(query.trim());
    }

    /// FIL-2: "Clear all" belongs to the view it was clicked in — it drops
    /// that view's query and facets, and leaves every other view's facets
    /// alone.
    pub(in crate::ui) fn clear_all(self: &Rc<Self>) {
        let scope = self.active.get();
        let clear_facets = self
            .handlers
            .borrow()
            .get(&scope)
            .map(|handlers| handlers.clear_facets.clone());
        if let Some(clear_facets) = clear_facets {
            clear_facets();
        }
        // Apply explicitly rather than relying on the entry's signal handler
        // to do half of an action that also clears facets.
        self.write_entry("");
        self.apply_to_active("");
    }

    /// The one place this module writes the entry.
    fn write_entry(&self, text: &str) -> bool {
        let Some(entry) = self.entry.upgrade() else {
            return false;
        };
        let changed = entry.text() != text;
        if changed {
            entry.set_text(text);
        }
        changed
    }

    fn entry_text(&self) -> String {
        self.entry
            .upgrade()
            .map_or_else(String::new, |entry| entry.text().to_string())
    }

    fn apply_to_active(&self, query: &str) {
        self.apply_to_scope(self.active.get(), query);
    }

    fn apply_to_scope(&self, scope: SearchScope, query: &str) {
        let apply = self
            .handlers
            .borrow()
            .get(&scope)
            .map(|handlers| handlers.apply.clone());
        match apply {
            Some(apply) => apply(query.trim()),
            None => tracing::debug!(?scope, "no search sink registered for this section"),
        }
    }

    fn collapse_bar(&self) {
        if let Some(toggle) = self.toggle.upgrade() {
            toggle.set_active(false);
        }
        if let Some(search_bar) = self.search_bar.upgrade() {
            search_bar.set_search_mode(false);
        }
    }

    /// SEARCH-8a: where there is no list, there is nothing to filter — the
    /// lens says so and stops responding, and the strip cannot be revealed by
    /// typing either.
    fn sync_affordance(&self, section_name: &str) {
        let (Some(toggle), Some(search_bar)) = (self.toggle.upgrade(), self.search_bar.upgrade())
        else {
            return;
        };
        let supported = self.supports_search();
        toggle.set_sensitive(supported);
        if supported {
            toggle.set_tooltip_text(Some(&strings::shortcut_tooltip(
                strings::SEARCH_PLACEHOLDER,
                strings::SHORTCUT_SEARCH,
            )));
            if let Some(key_capture) = self.key_capture.upgrade() {
                search_bar.set_key_capture_widget(Some(&key_capture));
            }
            return;
        }
        toggle.set_tooltip_text(Some(&filter_strings::nothing_to_filter(section_name)));
        toggle.set_active(false);
        search_bar.set_search_mode(false);
        search_bar.set_key_capture_widget(None::<&gtk4::Widget>);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell as StdRefCell;

    use libadwaita as adw;
    use libadwaita::prelude::*;
    use reprise_core::browser::navigation::{NavigationIntent, SidebarTarget};
    use reprise_core::browser::{AlbumKey, ArtistKey, BrowserPlace};

    use super::*;
    use crate::ui::nav_history::{NavHistory, NavPlace};

    #[path = "section_search_unsupported_tests.rs"]
    mod unsupported_tests;

    struct Harness {
        search: Rc<SectionSearch>,
        entry: gtk4::SearchEntry,
        toggle: gtk4::ToggleButton,
        search_bar: gtk4::SearchBar,
        track_chip_host: gtk4::Box,
        applied: Rc<StdRefCell<Vec<(SearchScope, String)>>>,
        facets_cleared: Rc<StdRefCell<Vec<SearchScope>>>,
    }

    fn harness() -> Harness {
        let window = adw::ApplicationWindow::builder().build();
        let entry = gtk4::SearchEntry::new();
        let search_bar = gtk4::SearchBar::new();
        search_bar.connect_entry(&entry);
        let toggle = gtk4::ToggleButton::new();
        let search = SectionSearch::new(&entry, &search_bar, &toggle, &window);
        let track_chip_host = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let applied = Rc::new(StdRefCell::new(Vec::new()));
        let facets_cleared = Rc::new(StdRefCell::new(Vec::new()));
        for scope in [
            SearchScope::Tracks,
            SearchScope::Podcasts,
            SearchScope::Radio,
        ] {
            let sink = applied.clone();
            let cleared = facets_cleared.clone();
            let track_chip_host = track_chip_host.clone();
            search.register(
                scope,
                move |query| {
                    sink.borrow_mut().push((scope, query.to_owned()));
                    if scope != SearchScope::Tracks {
                        return;
                    }
                    while let Some(child) = track_chip_host.first_child() {
                        track_chip_host.remove(&child);
                    }
                    if !query.trim().is_empty() {
                        track_chip_host.append(&crate::ui::browse::search_chip::build(
                            SearchScope::Tracks,
                            query,
                            || {},
                        ));
                    }
                },
                move || cleared.borrow_mut().push(scope),
            );
        }
        Harness {
            search,
            entry,
            toggle,
            search_bar,
            track_chip_host,
            applied,
            facets_cleared,
        }
    }

    fn track_chip_label(harness: &Harness) -> Option<String> {
        harness
            .track_chip_host
            .first_child()?
            .downcast::<gtk4::Button>()
            .ok()?
            .label()
            .map(|label| label.to_string())
    }

    fn settle() {
        while gtk4::glib::MainContext::default().iteration(false) {}
    }

    /// GTK debounces `search-changed` by ~150 ms, so a typed query reaches
    /// its section a moment after the keystroke. Pump until it does rather
    /// than asserting into that window.
    fn settle_until(label: &str, condition: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !condition() {
            settle();
            assert!(std::time::Instant::now() < deadline, "timed out: {label}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    // UX SEARCH-8a: switching views drops the query and collapses the field,
    // because the destination is a new search context.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8a_switching_views_drops_the_query_and_collapses_the_bar() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();

        harness.search.activate(SearchScope::Tracks, "Music");
        harness.toggle.set_active(true);
        harness.search_bar.set_search_mode(true);
        harness.entry.set_text("falling");
        settle();

        harness.search.activate(SearchScope::Podcasts, "Podcasts");
        settle();
        assert_eq!(
            harness.entry.text(),
            "",
            "the Podcasts section starts without the Library query"
        );
        assert!(!harness.toggle.is_active());
        assert!(!harness.search_bar.is_search_mode());

        harness.search.activate(SearchScope::Tracks, "Music");
        settle();
        assert_eq!(
            harness.entry.text(),
            "",
            "returning through a new view switch must not resurrect Music's old query"
        );
    }

    // UX SEARCH-8a: track sources share one SearchScope, but choosing another
    // sidebar destination is still a view switch and starts empty.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8a_switching_track_views_drops_the_query_despite_the_shared_scope() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();

        harness
            .search
            .activate_source(&ViewSource::Library, "Music");
        harness.toggle.set_active(true);
        harness.search_bar.set_search_mode(true);
        harness.entry.set_text("falling");
        settle();

        let history = NavHistory::default();
        let mut library = BrowserPlace::from(ViewSource::Library);
        library.track_state_mut().unwrap().search = "falling".into();
        history.record_route(&NavPlace::browser(library.clone()));
        let destination = history
            .navigate_from(
                NavigationIntent::Sidebar(SidebarTarget::RecentlyAdded),
                library,
            )
            .expect("Recently Added must be a different sidebar destination");
        harness
            .search
            .activate_source(&destination.view_source(), "Recently Added");
        let destination_query = &destination
            .browser_place()
            .track_state()
            .expect("Recently Added is a track view")
            .search;
        harness
            .search
            .set_query(SearchScope::Tracks, destination_query);
        settle();

        assert_eq!(harness.entry.text(), "");
        assert!(!harness.toggle.is_active());
        assert!(!harness.search_bar.is_search_mode());
    }

    // UX SEARCH-8a/FIL-1c: a metadata intent drills into the Library's filter
    // context rather than choosing a new sidebar destination. Its history
    // place carries the query into the Artist page, and Back restores the
    // complete remembered Library state without search owning parallel
    // origin state.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8a_drilling_into_an_artist_place_keeps_query_and_chip_then_back_restores_them() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();
        let history = NavHistory::default();

        harness
            .search
            .activate_source(&ViewSource::Library, "Music");
        harness.entry.set_text("falling");
        let chip_probe = harness.track_chip_host.clone();
        settle_until("the Library search chip appears", move || {
            chip_probe.first_child().is_some()
        });

        let mut library = BrowserPlace::from(ViewSource::Library);
        let library_state = library.track_state_mut().unwrap();
        library_state.search = "falling".into();
        library_state.browse.genre = Some("Metalcore".into());
        history.record_route(&NavPlace::browser(library.clone()));
        let artist = history
            .navigate_from(
                NavigationIntent::OpenArtist {
                    artist: ArtistKey::new("Lorna Shore"),
                    anchor_track_id: None,
                },
                library,
            )
            .expect("the Artist page must be a new history place");

        let artist_query = &artist
            .browser_place()
            .track_state()
            .expect("the Artist page is a track place")
            .search;
        harness.search.set_query(SearchScope::Tracks, artist_query);
        settle();

        assert_eq!(harness.entry.text(), "falling");
        assert_eq!(
            track_chip_label(&harness).as_deref(),
            Some("⌕ “falling” in track, artist and album  ×")
        );

        let restored = history
            .go_back_from(artist.browser_place().clone())
            .expect("Back must restore the filtered Library place");
        harness
            .search
            .activate_source(&restored.view_source(), "Music");
        let restored_state = restored
            .browser_place()
            .track_state()
            .expect("the restored Library is a track place");
        harness
            .search
            .set_query(SearchScope::Tracks, &restored_state.search);
        let chip_probe = harness.track_chip_host.clone();
        settle_until("Back restores the Library search chip", move || {
            chip_probe.first_child().is_some()
        });

        assert_eq!(harness.entry.text(), "falling");
        assert_eq!(restored_state.browse.genre.as_deref(), Some("Metalcore"));
        assert_eq!(
            track_chip_label(&harness).as_deref(),
            Some("⌕ “falling” in track, artist and album  ×")
        );
    }

    // UX SEARCH-8a: while a view stays active, its query reaches that view and
    // no other.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8a_a_query_is_only_applied_to_the_active_view() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();

        harness.search.activate(SearchScope::Podcasts, "Podcasts");
        harness.entry.set_text("wer");
        let applied_probe = harness.applied.clone();
        settle_until("the typed query reaches its section", move || {
            applied_probe
                .borrow()
                .contains(&(SearchScope::Podcasts, "wer".to_owned()))
        });

        let applied = harness.applied.borrow().clone();
        assert!(
            applied
                .iter()
                .all(|(scope, query)| query.is_empty() || *scope == SearchScope::Podcasts),
            "a non-empty Podcasts query must never be handed to another view: {applied:?}"
        );
        assert!(applied.contains(&(SearchScope::Tracks, String::new())));
        assert!(applied.contains(&(SearchScope::Podcasts, "wer".to_owned())));
        assert!(harness.search.is_active(SearchScope::Podcasts));
        assert!(!harness.search.is_active(SearchScope::Tracks));
    }

    // UX SEARCH-8a: a view that clears its own chip pushes that back into the
    // entry instead of leaving a query on screen that nothing applies.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8a_a_view_clearing_its_chip_clears_the_entry() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();

        harness.search.activate(SearchScope::Radio, "Radio");
        harness.entry.set_text("nova");
        settle();

        harness.search.set_query(SearchScope::Radio, "");
        settle();

        assert_eq!(harness.entry.text(), "");
    }

    // UX SEARCH-8a: only a query is discarded on a view switch. The facet
    // callback is reserved for the user's explicit Clear all action.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8a_switching_views_leaves_facet_filters_untouched() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();

        harness.search.activate(SearchScope::Podcasts, "Podcasts");
        harness.entry.set_text("wer");
        settle();
        harness.search.activate(SearchScope::Radio, "Radio");
        settle();

        assert!(
            harness.facets_cleared.borrow().is_empty(),
            "switching views must not invoke either view's facet reset"
        );
        let applied = harness.applied.borrow();
        assert!(applied.contains(&(SearchScope::Podcasts, String::new())));
        assert!(applied.contains(&(SearchScope::Radio, String::new())));
    }

    // UX SEARCH-8a: Back is the deliberate exception. The complete query is
    // recovered from the existing browser history's TrackViewState; the
    // search coordinator owns no second origin or history flag.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8a_back_from_a_detail_restores_the_same_lists_query_from_history() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();
        let history = NavHistory::default();

        harness
            .search
            .activate_source(&ViewSource::Library, "Music");
        harness.entry.set_text("falling");
        settle();

        let mut list = BrowserPlace::from(ViewSource::Library);
        list.track_state_mut().unwrap().search = "falling".into();
        history.record_route(&NavPlace::browser(list.clone()));
        let detail = history
            .navigate_from(
                NavigationIntent::OpenAlbum {
                    album: AlbumKey::new("Pain Remains", "Lorna Shore"),
                    anchor_track_id: None,
                },
                list,
            )
            .expect("the album detail must be a new history place");
        harness.search.set_query(SearchScope::Tracks, "");

        let restored = history
            .go_back_from(detail.browser_place().clone())
            .expect("Back must restore the list place");
        harness
            .search
            .activate_source(&restored.view_source(), "Music");
        let restored_query = &restored
            .browser_place()
            .track_state()
            .expect("the restored place is the same track list")
            .search;
        harness
            .search
            .set_query(SearchScope::Tracks, restored_query);
        settle();

        assert_eq!(harness.entry.text(), "falling");
        assert!(!harness.search_bar.is_search_mode());
    }

    // UX FIL-2a: "Clear all" clears the current section only.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_2a_clear_all_only_touches_the_current_section() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();
        let cleared = Rc::new(Cell::new(0_u32));
        let counter = cleared.clone();
        harness.search.register(
            SearchScope::Podcasts,
            |_| {},
            move || {
                counter.set(counter.get() + 1);
            },
        );

        harness.search.activate(SearchScope::Tracks, "Music");
        harness.entry.set_text("falling");
        settle();
        harness.search.activate(SearchScope::Podcasts, "Podcasts");
        harness.entry.set_text("wer");
        settle();

        assert_eq!(cleared.get(), 0, "a view switch does not clear facets");
        harness.search.clear_all();
        settle();

        assert_eq!(harness.entry.text(), "");
        assert_eq!(
            cleared.get(),
            1,
            "only the visible section clears its facets"
        );
        harness.search.activate(SearchScope::Tracks, "Music");
        settle();
        assert_eq!(
            harness.entry.text(),
            "",
            "a new view switch must not resurrect Music's old query"
        );
    }
}
