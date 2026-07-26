//! One rendering edge for every track, album, and artist metadata link.

use std::rc::Rc;

use libadwaita as adw;
use reprise_core::browser::navigation::NavigationIntent;

use super::library_shell::{self, ActiveContentFocus};
use crate::ui::nav_history::NavHistory;
use crate::ui::sidebar::Sidebar;
use crate::ui::track_list::TrackList;

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

#[derive(Clone)]
pub(in crate::ui) struct MetadataNavigator {
    history: Rc<NavHistory>,
    sidebar: std::rc::Weak<Sidebar>,
    track_list: std::rc::Weak<TrackList>,
    content_stack: gtk4::Stack,
    source_title: adw::WindowTitle,
    active_content_focus: ActiveContentFocus,
}

impl MetadataNavigator {
    pub(in crate::ui) fn new(
        history: Rc<NavHistory>,
        sidebar: &Rc<Sidebar>,
        track_list: &Rc<TrackList>,
        content_stack: gtk4::Stack,
        source_title: adw::WindowTitle,
        active_content_focus: ActiveContentFocus,
    ) -> Self {
        Self {
            history,
            sidebar: Rc::downgrade(sidebar),
            track_list: Rc::downgrade(track_list),
            content_stack,
            source_title,
            active_content_focus,
        }
    }

    pub(in crate::ui) fn navigate(&self, intent: NavigationIntent, reason: &'static str) {
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
            &self.content_stack,
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
    use std::cell::RefCell;

    use libadwaita::prelude::*;
    use reprise_core::browser::{AlbumKey, ArtistKey, BrowserPlace};
    use reprise_core::view_source::ViewSource;

    use super::*;

    #[test]
    fn browse_8_deleted_track_links_do_not_open_phantoms_but_scopes_survive() {
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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_title_follows_scope_navigation() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.ScopeTitleTest")
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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_1c_genre_scope_chip_x_returns_to_the_library_with_history() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.ScopeChipTest")
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
        let scope_chip = track_list.shared.browse_bar.scope_button().unwrap();
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
}
