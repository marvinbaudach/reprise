//! One rendering edge for every track, album, and artist metadata link.

use std::cell::RefCell;
use std::rc::Rc;

use libadwaita as adw;
use reprise_core::browser::navigation::{NavigationIntent, SourceTarget};

use super::library_shell::{self, ActiveContentFocus};
use crate::ui::nav_history::NavHistory;
use crate::ui::sidebar::Sidebar;
use crate::ui::track_list::TrackList;

type SourceRevealCallback = Rc<dyn Fn(SourceTarget)>;

fn normalize_catalog_intent(
    intent: NavigationIntent,
    mut is_present: impl FnMut(i64) -> bool,
) -> Option<NavigationIntent> {
    match intent {
        NavigationIntent::RevealTrack { origin, track_id } => {
            is_present(track_id).then_some(NavigationIntent::RevealTrack { origin, track_id })
        }
        NavigationIntent::OpenAlbum {
            album,
            anchor_track_id,
        } => Some(NavigationIntent::OpenAlbum {
            album,
            anchor_track_id: anchor_track_id.filter(|id| is_present(*id)),
        }),
        NavigationIntent::OpenArtist {
            artist,
            anchor_track_id,
        } => Some(NavigationIntent::OpenArtist {
            artist,
            anchor_track_id: anchor_track_id.filter(|id| is_present(*id)),
        }),
        other => Some(other),
    }
}

/// `BROWSE-4`: order is contractual. The target view must hold the reveal
/// request before routing maps it, and it must hold it *whichever* way routing
/// ends: an already-open source view yields no transition, and a torn-down
/// window has no widgets left to route with. Both leave through `route` doing
/// nothing, and neither may swallow the reveal the user asked for.
fn reveal_then_route(
    slot: &RefCell<Option<SourceRevealCallback>>,
    target: SourceTarget,
    route: impl FnOnce(),
) {
    let callback = slot.borrow().clone();
    if let Some(callback) = callback {
        callback(target);
    }
    route();
}

#[derive(Clone)]
pub(in crate::ui) struct MetadataNavigator {
    history: Rc<NavHistory>,
    sidebar: std::rc::Weak<Sidebar>,
    track_list: std::rc::Weak<TrackList>,
    content_navigation: adw::NavigationView,
    content_stack: gtk4::Stack,
    source_title: adw::WindowTitle,
    active_content_focus: ActiveContentFocus,
    on_source_reveal: Rc<RefCell<Option<SourceRevealCallback>>>,
}

impl MetadataNavigator {
    pub(in crate::ui) fn new(
        history: Rc<NavHistory>,
        sidebar: &Rc<Sidebar>,
        track_list: &Rc<TrackList>,
        content_navigation: adw::NavigationView,
        content_stack: gtk4::Stack,
        source_title: adw::WindowTitle,
        active_content_focus: ActiveContentFocus,
    ) -> Self {
        Self {
            history,
            sidebar: Rc::downgrade(sidebar),
            track_list: Rc::downgrade(track_list),
            content_navigation,
            content_stack,
            source_title,
            active_content_focus,
            on_source_reveal: Rc::new(RefCell::new(None)),
        }
    }

    pub(in crate::ui) fn set_on_source_reveal(&self, callback: impl Fn(SourceTarget) + 'static) {
        self.on_source_reveal.replace(Some(Rc::new(callback)));
    }

    pub(in crate::ui) fn navigate(&self, intent: NavigationIntent, reason: &'static str) {
        if let Some(target) = intent.source_target() {
            reveal_then_route(&self.on_source_reveal, target, || {
                let (Some(sidebar), Some(track_list)) =
                    (self.sidebar.upgrade(), self.track_list.upgrade())
                else {
                    return;
                };
                if let Some(place) = self
                    .history
                    .navigate_from(intent, track_list.browser_place())
                {
                    library_shell::route_to_place(
                        &place,
                        &sidebar,
                        &track_list,
                        library_shell::ContentPages::new(
                            &self.content_navigation,
                            &self.content_stack,
                        ),
                        &self.source_title,
                        &self.active_content_focus,
                        reason,
                    );
                }
            });
            return;
        }
        let (Some(sidebar), Some(track_list)) = (self.sidebar.upgrade(), self.track_list.upgrade())
        else {
            return;
        };
        let was_track_reveal = matches!(intent, NavigationIntent::RevealTrack { .. });
        let Some(intent) = normalize_catalog_intent(intent, |id| track_list.contains_track(id))
        else {
            if was_track_reveal {
                track_list.toast(&crate::ui::strings::text(
                    crate::ui::strings::TRACK_NOT_IN_LIBRARY,
                ));
            }
            return;
        };
        let Some(place) = self
            .history
            .navigate_from(intent, track_list.browser_place())
        else {
            return;
        };
        library_shell::route_to_place(
            &place,
            &sidebar,
            &track_list,
            library_shell::ContentPages::new(&self.content_navigation, &self.content_stack),
            &self.source_title,
            &self.active_content_focus,
            reason,
        );
    }

    pub(in crate::ui) fn leave_scope(&self) {
        self.navigate(
            NavigationIntent::Sidebar(reprise_core::browser::navigation::SidebarTarget::Music),
            "scope chip cleared",
        );
    }
}

#[cfg(test)]
mod tests {
    use libadwaita::prelude::*;
    use reprise_core::browser::{AlbumKey, ArtistKey, BrowserPlace};
    use reprise_core::view_source::ViewSource;

    use super::*;

    #[test]
    fn browse_11_deleted_track_links_do_not_open_phantoms_but_scopes_survive() {
        let track = NavigationIntent::RevealTrack {
            origin: Box::new(BrowserPlace::from(ViewSource::Library)),
            track_id: 42,
        };
        assert!(normalize_catalog_intent(track, |_| false).is_none());

        let album = normalize_catalog_intent(
            NavigationIntent::OpenAlbum {
                album: AlbumKey::new("Album", "Artist"),
                anchor_track_id: Some(42),
            },
            |_| false,
        )
        .unwrap();
        assert_eq!(
            album,
            NavigationIntent::OpenAlbum {
                album: AlbumKey::new("Album", "Artist"),
                anchor_track_id: None,
            }
        );

        let artist = normalize_catalog_intent(
            NavigationIntent::OpenArtist {
                artist: ArtistKey::new("Artist"),
                anchor_track_id: Some(42),
            },
            |_| false,
        )
        .unwrap();
        assert_eq!(
            artist,
            NavigationIntent::OpenArtist {
                artist: ArtistKey::new("Artist"),
                anchor_track_id: None,
            }
        );
    }

    /// `BROWSE-4`: the reveal reaches the source view before routing, and
    /// survives a routing step that does nothing — dead widget refs after a
    /// window teardown, or `navigate_from` returning `None` because the view
    /// is already open. That second case is the everyday one: jumping to the
    /// station while the Radio view is open routes nowhere, and the reveal is
    /// the entire visible effect.
    #[test]
    fn browse_4_the_source_reveal_fires_first_and_survives_a_routing_no_op() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let slot: Rc<RefCell<Option<SourceRevealCallback>>> =
            Rc::new(RefCell::new(Some(Rc::new({
                let events = events.clone();
                move |target| events.borrow_mut().push(format!("reveal {target:?}"))
            }))));

        reveal_then_route(&slot, SourceTarget::Station { station_id: 5 }, {
            let events = events.clone();
            move || events.borrow_mut().push("route".to_owned())
        });
        // The routing half found nothing to do — the reveal still happened.
        reveal_then_route(&slot, SourceTarget::Station { station_id: 5 }, || {});

        assert_eq!(
            *events.borrow(),
            [
                "reveal Station { station_id: 5 }",
                "route",
                "reveal Station { station_id: 5 }",
            ]
        );

        // No callback registered yet: routing must still run.
        slot.replace(None);
        reveal_then_route(&slot, SourceTarget::Station { station_id: 5 }, {
            let events = events.clone();
            move || events.borrow_mut().push("route".to_owned())
        });
        assert_eq!(events.borrow().len(), 4);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_title_follows_scope_navigation() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.ScopeTitleTest")
            .build();
        app.register(None::<&gtk4::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        let sidebar = Rc::new(Sidebar::new(conn.clone(), &window, || 0));
        let track_list = Rc::new(TrackList::new(
            conn,
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        ));
        let content_stack = gtk4::Stack::new();
        content_stack.add_named(&gtk4::Box::default(), Some("library"));
        let source_title = adw::WindowTitle::new("My Stats", "");
        let history = Rc::new(NavHistory::default());
        let library = BrowserPlace::from(ViewSource::Library);
        history.restore(library.clone(), library);
        let navigator = MetadataNavigator::new(
            history,
            &sidebar,
            &track_list,
            adw::NavigationView::new(),
            content_stack.clone(),
            source_title.clone(),
            ActiveContentFocus::new(&content_stack, &track_list),
        );

        navigator.navigate(
            NavigationIntent::OpenArtist {
                artist: ArtistKey::new("Lorna Shore"),
                anchor_track_id: None,
            },
            "test artist navigation",
        );

        assert_eq!(source_title.title(), "Lorna Shore");
    }

    /// UX FIL-1c, end to end: starting playback from inside an artist scope
    /// mutates the queue, and `window.rs`'s `on_queue_changed` hook updates
    /// only the retained Queue badge. It must not route the view anywhere —
    /// the scope, its chip, and its filtered rows survive the play.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_1c_playing_inside_a_scope_keeps_the_scope_and_its_chip() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.ScopeSurvivesRefreshTest")
            .build();
        app.register(None::<&gtk4::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        let sidebar = Rc::new(Sidebar::new(conn.clone(), &window, || 0));
        let track_list = Rc::new(TrackList::new(
            conn,
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        ));
        let content_stack = gtk4::Stack::new();
        content_stack.add_named(&gtk4::Box::default(), Some("library"));
        let source_title = adw::WindowTitle::new("My Stats", "");
        let history = Rc::new(NavHistory::default());
        let library = BrowserPlace::from(ViewSource::Library);
        history.restore(library.clone(), library);
        let navigator = MetadataNavigator::new(
            history,
            &sidebar,
            &track_list,
            adw::NavigationView::new(),
            content_stack.clone(),
            source_title.clone(),
            ActiveContentFocus::new(&content_stack, &track_list),
        );
        // The routing half of `library_shell::wire_source_routing` that a
        // sidebar selection drives — the seam this bug travelled through.
        sidebar.set_on_select({
            let track_list = track_list.clone();
            move |source, _title| track_list.set_source(source)
        });

        navigator.navigate(
            NavigationIntent::OpenArtist {
                artist: ArtistKey::new("Lorna Shore"),
                anchor_track_id: None,
            },
            "test artist navigation",
        );
        sidebar.refresh_queue_count();

        assert_eq!(
            track_list.current_source(),
            ViewSource::Artist("Lorna Shore".into()),
            "the queue badge update a play triggers must not drop the artist scope"
        );
        assert_eq!(source_title.title(), "Lorna Shore");
        let scope_chip = track_list
            .shared
            .browse_bar
            .place_button()
            .expect("the scope chip must survive the refresh");
        assert!(scope_chip
            .label()
            .is_some_and(|label| label.contains("Lorna Shore")));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_1c_genre_scope_chip_x_returns_to_the_library_with_history() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.ScopeChipTest")
            .build();
        app.register(None::<&gtk4::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        let sidebar = Rc::new(Sidebar::new(conn.clone(), &window, || 0));
        let track_list = Rc::new(TrackList::new(
            conn,
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        ));
        let content_stack = gtk4::Stack::new();
        content_stack.add_named(&gtk4::Box::default(), Some("library"));
        let source_title = adw::WindowTitle::new("Music", "");
        let history = Rc::new(NavHistory::default());
        let library = BrowserPlace::from(ViewSource::Library);
        history.restore(library.clone(), library);
        let navigator = MetadataNavigator::new(
            history.clone(),
            &sidebar,
            &track_list,
            adw::NavigationView::new(),
            content_stack.clone(),
            source_title,
            ActiveContentFocus::new(&content_stack, &track_list),
        );
        track_list.set_on_scope_cleared({
            let navigator = navigator.clone();
            move || navigator.leave_scope()
        });
        navigator.navigate(
            NavigationIntent::OpenGenre {
                genre: "Metalcore".into(),
            },
            "test genre navigation",
        );
        let scope_chip = track_list.shared.browse_bar.place_button().unwrap();
        assert!(scope_chip
            .label()
            .is_some_and(|label| label.contains("Metalcore")));
        assert!(scope_chip
            .tooltip_text()
            .is_some_and(|tooltip| tooltip.contains("Metalcore")));
        assert!(scope_chip.width_request() >= 20);

        scope_chip.emit_clicked();

        assert_eq!(track_list.current_source(), ViewSource::Library);
        let previous = history
            .go_back_from(track_list.browser_place())
            .expect("leaving the scope must push it onto Back history");
        assert_eq!(
            previous.view_source(),
            ViewSource::Genre("Metalcore".into())
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_8_recently_added_is_a_sidebar_place_without_a_pill_widget() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.RecentScopeChipTest")
            .build();
        app.register(None::<&gtk4::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        let sidebar = Rc::new(Sidebar::new(conn.clone(), &window, || 0));
        let track_list = Rc::new(TrackList::new(
            conn,
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        ));
        let content_stack = gtk4::Stack::new();
        content_stack.add_named(&gtk4::Box::default(), Some("library"));
        let source_title = adw::WindowTitle::new("Music", "");
        let history = Rc::new(NavHistory::default());
        let library = BrowserPlace::from(ViewSource::Library);
        history.restore(library.clone(), library);
        let navigator = MetadataNavigator::new(
            history.clone(),
            &sidebar,
            &track_list,
            adw::NavigationView::new(),
            content_stack.clone(),
            source_title,
            ActiveContentFocus::new(&content_stack, &track_list),
        );
        track_list.set_on_scope_cleared({
            let navigator = navigator.clone();
            move || navigator.leave_scope()
        });
        navigator.navigate(
            NavigationIntent::Sidebar(
                reprise_core::browser::navigation::SidebarTarget::RecentlyAdded,
            ),
            "test recently added navigation",
        );
        // FIL-8 (revised 2026-07-31): Recently added is a sidebar place — the
        // sidebar row names it, so it carries no place pill. Leaving happens by
        // selecting another sidebar row, not by dismissing a pill.
        assert!(track_list.shared.browse_bar.place_button().is_none());
        assert_eq!(track_list.current_source(), ViewSource::RecentlyAdded);
    }
}
