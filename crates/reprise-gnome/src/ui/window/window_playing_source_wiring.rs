//! Player metadata links and explicit source-list reveal routing.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::browser::navigation::{NavigationIntent, SourceKind, SourceTarget};
use reprise_core::browser::{AlbumKey, ArtistKey, BrowserPlace};
use reprise_core::view_source::ViewSource;

use crate::ui::now_playing::NowPlayingPanel;
use crate::ui::playback::source_item_identity::source_reveal_intent;
use crate::ui::player_controller::PlayerController;
use crate::ui::playing_links::LinkSurface;
use crate::ui::podcasts::PodcastsView;
use crate::ui::radio::RadioView;

use super::super::metadata_navigation::MetadataNavigator;

#[allow(clippy::too_many_arguments)]
pub(super) fn install(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    player: Option<&Rc<PlayerController>>,
    info_panel: &Rc<NowPlayingPanel>,
    metadata_navigator: &MetadataNavigator,
    podcasts_view: &Rc<PodcastsView>,
    youtube_view: &Rc<PodcastsView>,
    radio_view: &Rc<RadioView>,
) {
    metadata_navigator.set_on_source_reveal({
        let podcasts_view = podcasts_view.clone();
        let youtube_view = youtube_view.clone();
        let radio_view = radio_view.clone();
        move |target| match target {
            SourceTarget::Episode {
                subscription_id,
                episode_id,
                kind: SourceKind::Podcasts,
            } => podcasts_view.request_reveal(subscription_id, episode_id),
            SourceTarget::Episode {
                subscription_id,
                episode_id,
                kind: SourceKind::Youtube,
            } => youtube_view.request_reveal(subscription_id, episode_id),
            SourceTarget::Station { .. } => radio_view.request_reveal_connected(),
        }
    });

    let Some(player) = player else { return };
    // BROWSE-4: every metadata surface emits the same semantic intents. The
    // router owns history and anchors; these callbacks only choose a target.
    let reveal_playing_track: Rc<dyn Fn()> = {
        let player = Rc::downgrade(player);
        let navigator = metadata_navigator.clone();
        Rc::new(move || {
            let Some(player) = player.upgrade() else {
                return;
            };
            if let Some(item) = player.current_source_item() {
                navigator.navigate(
                    source_reveal_intent(&item, LinkSurface::Title),
                    "playing episode link",
                );
                return;
            }
            let Some(track_id) = player.current_track_id() else {
                return;
            };
            let origin = player.current_play_origin().map_or_else(
                || BrowserPlace::from(ViewSource::Library),
                |origin| origin.place,
            );
            navigator.navigate(
                NavigationIntent::RevealTrack {
                    origin: Box::new(origin),
                    track_id,
                },
                "playing track link",
            );
        })
    };
    let reveal_playing_album: Rc<dyn Fn()> = {
        let player = Rc::downgrade(player);
        let navigator = metadata_navigator.clone();
        let reveal_playing_track = reveal_playing_track.clone();
        Rc::new(move || {
            let Some(player) = player.upgrade() else {
                return;
            };
            if let Some(item) = player.current_source_item() {
                navigator.navigate(
                    source_reveal_intent(&item, LinkSurface::Cover),
                    "playing source cover link",
                );
                return;
            }
            let Some((album, album_artist)) = player.current_album_identity() else {
                reveal_playing_track();
                return;
            };
            navigator.navigate(
                NavigationIntent::OpenAlbum {
                    album: AlbumKey::new(album, album_artist),
                    anchor_track_id: player.current_track_id(),
                },
                "playing album link",
            );
        })
    };
    let reveal_playing_artist: Rc<dyn Fn()> = {
        let player = Rc::downgrade(player);
        let navigator = metadata_navigator.clone();
        let reveal_playing_track = reveal_playing_track.clone();
        Rc::new(move || {
            let Some(player) = player.upgrade() else {
                return;
            };
            if let Some(item) = player.current_source_item() {
                navigator.navigate(
                    source_reveal_intent(&item, LinkSurface::Subtitle),
                    "playing source subtitle link",
                );
                return;
            }
            let Some(artist) = player.current_artist_identity() else {
                reveal_playing_track();
                return;
            };
            navigator.navigate(
                NavigationIntent::OpenArtist {
                    artist: ArtistKey::new(artist),
                    anchor_track_id: player.current_track_id(),
                },
                "playing artist link",
            );
        })
    };
    {
        let reveal = reveal_playing_album.clone();
        player.connect_cover_clicked(move || reveal());
    }
    {
        let reveal = reveal_playing_track.clone();
        player.set_on_title_click(move || reveal());
    }
    {
        let reveal = reveal_playing_artist.clone();
        player.connect_artist_clicked(move || reveal());
    }
    {
        let reveal = reveal_playing_album.clone();
        info_panel.set_on_album_reveal(move || reveal());
    }
    {
        let reveal = reveal_playing_track.clone();
        info_panel.set_on_track_reveal(move || reveal());
    }
    {
        let reveal = reveal_playing_artist.clone();
        info_panel.set_on_artist_reveal(move || reveal());
    }
    let jump_action = gtk4::gio::SimpleAction::new("jump-to-now-playing", None);
    jump_action.connect_activate(move |_, _| reveal_playing_track());
    window.add_action(&jump_action);
    app.set_accels_for_action("win.jump-to-now-playing", &["<Control>l"]);
}
