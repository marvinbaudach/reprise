//! SEARCH-8a: a query belongs only to the view where it is typed.
//!
//! The header bar owns a single `GtkSearchEntry`, but the query it holds
//! is deliberately transient: a sidebar destination switch clears the view
//! being left, starts the destination empty, and closes the popover. Metadata
//! drills carry the query in their `BrowserPlace`, and Back restores the
//! complete saved place; this module does not keep parallel history.
//!
//! Three invariants make the rest of the shell simple:
//!
//! * The entry text is always the active scope's query. Every write pushed
//!   back by a view that cleared its own chip goes through here, so no
//!   participant has to guess which section a query belongs to.
//! * Query clearing calls only a section's `apply` and `commit` handlers. Its separately
//!   stored facet filters remain untouched unless the user invokes Clear all.
//! * A section without a list ([`SectionSearch::supports_search`] is false) can neither be
//!   searched nor open the popover: the lens is insensitive with a tooltip that
//!   names the section, and Ctrl+F is a no-op.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::view_source::ViewSource;
use reprise_view::search_chip::{self, SearchSurface};
use reprise_view::search_scope::{self, SearchScope};

use crate::ui::filter_bar_strings as filter_strings;
use crate::ui::strings;

use super::search_popover::{SearchPopover, WeakSearchPopover};

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
/// `commit` receives only the query that should appear as a filter chip;
/// `clear_facets` is the section's own half of "Clear all".
struct SectionHandlers {
    apply: Rc<dyn Fn(&str)>,
    commit: Rc<dyn Fn(&str)>,
    clear_facets: Rc<dyn Fn()>,
}

/// The shell state that answers "which section is visible right now".
struct ShellState {
    content_stack: gtk4::glib::WeakRef<gtk4::Stack>,
    window_title: gtk4::glib::WeakRef<libadwaita::WindowTitle>,
    current_source: Rc<dyn Fn() -> ViewSource>,
}

/// Every widget handle here is weak, deliberately. `SectionSearch` is
/// owned by the closures that consult it — a window action, the entry's own
/// signal handlers, the sidebar's `on_select` — and those closures are owned
/// by widgets under the window. A strong clone of the entry, lens, or popover
/// would close that loop and keep the whole window alive past destruction.
/// Weak handles break the cycle at the end that has no business owning it.
pub(in crate::ui) struct SectionSearch {
    entry: gtk4::glib::WeakRef<gtk4::SearchEntry>,
    search: WeakSearchPopover,
    toggle: gtk4::glib::WeakRef<gtk4::ToggleButton>,
    surface: Cell<SearchSurface>,
    active: Cell<SearchScope>,
    active_source: RefCell<Option<ViewSource>>,
    handlers: RefCell<BTreeMap<SearchScope, SectionHandlers>>,
    shell: RefCell<Option<ShellState>>,
}

impl SectionSearch {
    pub(in crate::ui) fn new(
        entry: &gtk4::SearchEntry,
        popover: &SearchPopover,
        toggle: &gtk4::ToggleButton,
    ) -> Rc<Self> {
        let search = Rc::new(Self {
            entry: entry.downgrade(),
            search: popover.downgrade(),
            toggle: toggle.downgrade(),
            surface: Cell::new(SearchSurface::Closed),
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
        let weak = Rc::downgrade(&search);
        popover.connect_open_changed(move |open| {
            let Some(search) = weak.upgrade() else {
                return;
            };
            search.surface.set(if open {
                SearchSurface::Open
            } else {
                SearchSurface::Closed
            });
            search.apply_to_active(&search.entry_text());
        });
        popover.set_abort_on_escape({
            let weak = Rc::downgrade(&search);
            Rc::new(move || {
                if let Some(search) = weak.upgrade() {
                    search.clear_active_query();
                }
            })
        });
        search
    }

    /// Registers a section's query sinks. `apply` receives the live query,
    /// `commit` receives the closed-surface query or an empty string;
    /// `clear_facets` drops that section's facet filters for "Clear all".
    pub(in crate::ui) fn register(
        &self,
        scope: SearchScope,
        apply: impl Fn(&str) + 'static,
        commit: impl Fn(&str) + 'static,
        clear_facets: impl Fn() + 'static,
    ) {
        self.handlers.borrow_mut().insert(
            scope,
            SectionHandlers {
                apply: Rc::new(apply),
                commit: Rc::new(commit),
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
        self.close_popover();
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

    /// FIL-2a: "Clear all" belongs to the view it was clicked in — it drops
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

    /// SEARCH-4a: the query half of the active section's existing clear path.
    /// This is shared by the chip's × round-trip, open-popover Escape and the
    /// window-level Escape used while only the committed chip remains.
    pub(in crate::ui) fn clear_active_query(&self) -> bool {
        if self.entry_text().trim().is_empty() {
            return false;
        }
        self.write_entry("");
        // SearchEntry may debounce `search-changed`; Escape must remove both
        // the filter and committed chip in this key press, not a later turn.
        self.apply_to_active("");
        true
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
        let handlers = self
            .handlers
            .borrow()
            .get(&scope)
            .map(|handlers| (handlers.apply.clone(), handlers.commit.clone()));
        let Some((apply, commit)) = handlers else {
            tracing::debug!(?scope, "no search sink registered for this section");
            return;
        };
        let query = query.trim();
        apply(query);
        commit(search_chip::committed_query(query, self.surface.get()).unwrap_or_default());
    }

    fn close_popover(&self) {
        self.search.close();
    }

    /// SEARCH-8a: where there is no list, there is nothing to filter — the
    /// lens says so and stops responding.
    fn sync_affordance(&self, section_name: &str) {
        let Some(toggle) = self.toggle.upgrade() else {
            return;
        };
        self.search.set_scope(self.active.get());
        let supported = self.supports_search();
        toggle.set_sensitive(supported);
        if supported {
            toggle.set_tooltip_text(Some(&strings::shortcut_tooltip(
                strings::SEARCH_PLACEHOLDER,
                strings::SHORTCUT_SEARCH,
            )));
            return;
        }
        toggle.set_tooltip_text(Some(&filter_strings::nothing_to_filter(section_name)));
        toggle.set_active(false);
        self.close_popover();
    }
}

#[cfg(test)]
#[path = "section_search/tests.rs"]
mod tests;
