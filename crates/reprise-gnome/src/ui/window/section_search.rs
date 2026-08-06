//! SEARCH-8: one query per section, not one query per window.
//!
//! The header bar owns a single `GtkSearchEntry`, but the query it holds
//! belongs to the section the user typed it in. This module is the only place
//! that knows that: it keeps a query per [`SearchScope`], swaps the entry text
//! when the visible section changes, and hands the current query to whichever
//! view registered itself for that scope.
//!
//! Two invariants make the rest of the shell simple:
//!
//! * The entry text is always the active scope's query. Every write — typed,
//!   restored on a section switch, or pushed back by a view that cleared its
//!   own chip — goes through here, so no participant has to guess which
//!   section a query belongs to.
//! * A section without a list ([`supports_search`] is false) can neither be
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
    queries: RefCell<BTreeMap<SearchScope, String>>,
    handlers: RefCell<BTreeMap<SearchScope, SectionHandlers>>,
    shell: RefCell<Option<ShellState>>,
    /// Set while this module writes the entry itself, so the resulting
    /// `changed` does not re-record a value it just restored.
    restoring: Cell<bool>,
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
            queries: RefCell::new(BTreeMap::new()),
            handlers: RefCell::new(BTreeMap::new()),
            shell: RefCell::new(None),
            restoring: Cell::new(false),
        });
        // `changed`, not `search-changed`: the stored query has to be exact
        // the instant the user types, because a section switch may read it
        // back before GTK's ~150 ms debounce would have fired.
        let weak = Rc::downgrade(&search);
        entry.connect_changed(move |entry| {
            let Some(search) = weak.upgrade() else {
                return;
            };
            if search.restoring.get() {
                return;
            }
            search.record(&entry.text());
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

    /// SEARCH-8: the section changed. Stash the query the user leaves behind,
    /// restore the one the section they enter already had, and re-apply it so
    /// the incoming list is filtered the way the user last left it.
    pub(in crate::ui) fn activate_source(self: &Rc<Self>, source: &ViewSource, section_name: &str) {
        self.activate(search_scope::scope_for(source), section_name);
    }

    pub(in crate::ui) fn activate(self: &Rc<Self>, scope: SearchScope, section_name: &str) {
        let previous = self.active.replace(scope);
        if previous == scope {
            // Still refresh the affordance: the section name can change
            // without the scope changing (playlist to playlist).
            self.sync_affordance(section_name);
            return;
        }
        self.record_active_from_entry(previous);
        let restored = self
            .queries
            .borrow()
            .get(&scope)
            .cloned()
            .unwrap_or_default();
        self.write_entry(&restored);
        self.sync_affordance(section_name);
        self.apply_to_active(&restored);
    }

    /// A view removed its own query (the chip's ×, or a jump that had to
    /// relax the search to reach its row). The entry follows so the two never
    /// disagree about what is filtered.
    pub(in crate::ui) fn set_query(self: &Rc<Self>, scope: SearchScope, query: &str) {
        self.queries
            .borrow_mut()
            .insert(scope, query.trim().to_owned());
        if self.active.get() != scope {
            return;
        }
        if self.entry_text().trim() == query.trim() {
            return;
        }
        self.write_entry(query.trim());
    }

    /// FIL-2: "Clear all" belongs to the section it was clicked in — it drops
    /// this section's query and this section's facets, and leaves every other
    /// section's alone.
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
        // Written through the same guard as every other programmatic write,
        // then recorded and applied explicitly — one path, rather than
        // relying on the entry's own handlers to do half of it.
        self.write_entry("");
        self.record("");
        self.apply_to_active("");
    }

    /// The one place this module writes the entry. The guard stops the
    /// resulting `changed` from re-recording a value we just restored.
    fn write_entry(&self, text: &str) {
        let Some(entry) = self.entry.upgrade() else {
            return;
        };
        self.restoring.set(true);
        entry.set_text(text);
        self.restoring.set(false);
    }

    fn entry_text(&self) -> String {
        self.entry
            .upgrade()
            .map(|entry| entry.text().to_string())
            .unwrap_or_default()
    }

    fn record(&self, query: &str) {
        self.queries
            .borrow_mut()
            .insert(self.active.get(), query.trim().to_owned());
    }

    fn record_active_from_entry(&self, scope: SearchScope) {
        let text = self.entry_text().trim().to_owned();
        self.queries.borrow_mut().insert(scope, text);
    }

    fn apply_to_active(&self, query: &str) {
        let scope = self.active.get();
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

    /// SEARCH-8: where there is no list, there is nothing to filter — the
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

    use super::*;

    struct Harness {
        search: Rc<SectionSearch>,
        entry: gtk4::SearchEntry,
        toggle: gtk4::ToggleButton,
        search_bar: gtk4::SearchBar,
        applied: Rc<StdRefCell<Vec<(SearchScope, String)>>>,
    }

    fn harness() -> Harness {
        let window = adw::ApplicationWindow::builder().build();
        let entry = gtk4::SearchEntry::new();
        let search_bar = gtk4::SearchBar::new();
        search_bar.connect_entry(&entry);
        let toggle = gtk4::ToggleButton::new();
        let search = SectionSearch::new(&entry, &search_bar, &toggle, &window);
        let applied = Rc::new(StdRefCell::new(Vec::new()));
        for scope in [
            SearchScope::Tracks,
            SearchScope::Podcasts,
            SearchScope::Radio,
        ] {
            let sink = applied.clone();
            search.register(
                scope,
                move |query| sink.borrow_mut().push((scope, query.to_owned())),
                || {},
            );
        }
        Harness {
            search,
            entry,
            toggle,
            search_bar,
            applied,
        }
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

    // UX SEARCH-8: a query typed in Podcasts leaves the Library query empty
    // and vice versa — the two never see each other's text.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8_a_query_belongs_to_the_section_it_was_typed_in() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();

        harness.search.activate(SearchScope::Tracks, "Music");
        harness.entry.set_text("falling");
        settle();

        harness.search.activate(SearchScope::Podcasts, "Podcasts");
        settle();
        assert_eq!(
            harness.entry.text(),
            "",
            "the Podcasts section starts without the Library query"
        );

        harness.entry.set_text("wer");
        settle();
        harness.search.activate(SearchScope::Tracks, "Music");
        settle();
        assert_eq!(
            harness.entry.text(),
            "falling",
            "Music gets its own query back, not the one typed in Podcasts"
        );

        harness.search.activate(SearchScope::Podcasts, "Podcasts");
        settle();
        assert_eq!(harness.entry.text(), "wer");
    }

    // UX SEARCH-8: a section switch that is immediately followed by the
    // incoming view restoring its OWN remembered text — which is exactly
    // what `track_list.set_source` does on its way in — must leave the
    // outgoing section's query alone. This is the contract
    // `library_shell::wire_source_routing` relies on when it activates the
    // scope BEFORE routing: reverse the two and the restored text is
    // recorded against the section the user just left.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8_a_view_restoring_its_own_text_cannot_overwrite_the_previous_section() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();

        harness.search.activate(SearchScope::Podcasts, "Podcasts");
        harness.entry.set_text("wer");
        settle();

        // The shell switches the scope first...
        harness.search.activate(SearchScope::Tracks, "Music");
        settle();
        // ...and only then does the track list push the source's own
        // remembered search into the shared entry, unguarded.
        harness.entry.set_text("acoustic");
        settle();

        harness.search.activate(SearchScope::Podcasts, "Podcasts");
        settle();
        assert_eq!(
            harness.entry.text(),
            "wer",
            "the restored track search was recorded against Podcasts"
        );

        harness.search.activate(SearchScope::Tracks, "Music");
        settle();
        assert_eq!(
            harness.entry.text(),
            "acoustic",
            "and Music kept the search its own source restored"
        );
    }

    // UX SEARCH-8: the query reaches the section it belongs to and no other.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8_a_query_is_only_applied_to_its_own_section() {
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
                .all(|(scope, _)| *scope == SearchScope::Podcasts),
            "a Podcasts query must never be handed to another section: {applied:?}"
        );
        assert!(applied.contains(&(SearchScope::Podcasts, "wer".to_owned())));
        assert!(harness.search.is_active(SearchScope::Podcasts));
        assert!(!harness.search.is_active(SearchScope::Tracks));
    }

    // UX SEARCH-8: where there is no list, the lens is insensitive, says why,
    // and the bar cannot be revealed.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8_sections_without_a_list_offer_no_search() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let harness = harness();

        harness.search.activate(SearchScope::Tracks, "Music");
        assert!(harness.toggle.is_sensitive());

        harness
            .search
            .activate(SearchScope::Unsupported, "My Stats");

        assert!(!harness.toggle.is_sensitive());
        assert_eq!(
            harness.toggle.tooltip_text().as_deref(),
            Some("Nothing to filter in My Stats")
        );
        assert!(!harness.search_bar.is_search_mode());
        assert!(!harness.search.supports_search());
        assert!(harness.search_bar.key_capture_widget().is_none());
    }

    // UX SEARCH-8: a view that clears its own chip pushes that back into the
    // entry instead of leaving a query on screen that nothing applies.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8_a_view_clearing_its_chip_clears_the_entry() {
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

    // UX FIL-2: "Clear all" clears the current section only.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_2_clear_all_only_touches_the_current_section() {
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
            "falling",
            "Clear all in Podcasts must not touch the Music query"
        );
    }
}
