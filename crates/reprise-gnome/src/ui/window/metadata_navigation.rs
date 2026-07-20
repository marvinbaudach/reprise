//! One rendering edge for every track, album, and artist metadata link.

use std::rc::Rc;

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
    sidebar: Rc<Sidebar>,
    track_list: Rc<TrackList>,
    content_stack: gtk4::Stack,
    active_content_focus: ActiveContentFocus,
}

impl MetadataNavigator {
    pub(in crate::ui) fn new(
        history: Rc<NavHistory>,
        sidebar: Rc<Sidebar>,
        track_list: Rc<TrackList>,
        content_stack: gtk4::Stack,
        active_content_focus: ActiveContentFocus,
    ) -> Self {
        Self {
            history,
            sidebar,
            track_list,
            content_stack,
            active_content_focus,
        }
    }

    pub(in crate::ui) fn navigate(&self, intent: NavigationIntent, reason: &'static str) {
        let was_track_reveal = matches!(intent, NavigationIntent::RevealTrack { .. });
        let Some(intent) =
            normalize_catalog_intent(intent, |id| self.track_list.contains_track(id))
        else {
            if was_track_reveal {
                self.track_list.toast(&crate::ui::strings::text(
                    crate::ui::strings::TRACK_NOT_IN_LIBRARY,
                ));
            }
            return;
        };
        let Some(place) = self
            .history
            .navigate_from(intent, self.track_list.browser_place())
        else {
            return;
        };
        library_shell::route_to_place(
            &place,
            &self.sidebar,
            &self.track_list,
            &self.content_stack,
            &self.active_content_focus,
            reason,
        );
    }
}

#[cfg(test)]
mod tests {
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
}
