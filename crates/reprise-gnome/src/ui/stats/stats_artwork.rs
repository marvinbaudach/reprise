//! Portrait-first artwork loading shared by the My Stats leader and tiles.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;

use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StatsArtworkSource {
    Loading,
    Portrait,
    Cover,
    Initials,
}

pub(super) struct StatsArtworkRequest<'a> {
    pub picture: &'a gtk4::Picture,
    pub fallback: &'a gtk4::Label,
    pub artist: &'a str,
    pub track_path: &'a str,
    pub token: u64,
    pub current: &'a Rc<Cell<u64>>,
    pub portrait: Option<Rc<ArtistPortraitRuntime>>,
    pub cover: Option<Rc<CoverLoader>>,
    pub source: Rc<Cell<StatsArtworkSource>>,
}

/// Loads portrait -> album cover -> initials without letting an older request
/// update a recycled card or tile.
pub(super) fn load(request: StatsArtworkRequest<'_>) {
    let StatsArtworkRequest {
        picture,
        fallback,
        artist,
        track_path,
        token,
        current,
        portrait,
        cover,
        source,
    } = request;
    picture.set_paintable(gtk4::gdk::Paintable::NONE);
    picture.set_visible(false);
    fallback.set_visible(true);
    source.set(StatsArtworkSource::Loading);

    let Some(portrait) = portrait else {
        load_cover(picture, fallback, track_path, token, current, cover, source);
        return;
    };
    let picture_for_result = picture.clone();
    let fallback_for_result = fallback.clone();
    let track_path = track_path.to_string();
    let current_for_result = current.clone();
    portrait.load_into_picture(picture, artist, token, current, move |loaded| {
        if current_for_result.get() != token {
            return;
        }
        if loaded {
            source.set(StatsArtworkSource::Portrait);
            picture_for_result.set_visible(true);
            fallback_for_result.set_visible(false);
            return;
        }
        load_cover(
            &picture_for_result,
            &fallback_for_result,
            &track_path,
            token,
            &current_for_result,
            cover,
            source,
        );
    });
}

fn load_cover(
    picture: &gtk4::Picture,
    fallback: &gtk4::Label,
    track_path: &str,
    token: u64,
    current: &Rc<Cell<u64>>,
    cover: Option<Rc<CoverLoader>>,
    source: Rc<Cell<StatsArtworkSource>>,
) {
    let Some(cover) = cover else {
        source.set(StatsArtworkSource::Initials);
        picture.set_visible(false);
        fallback.set_visible(true);
        return;
    };
    let picture_target = picture.clone();
    let picture_for_result = picture.clone();
    let fallback_for_result = fallback.clone();
    let current_for_result = current.clone();
    cover.load_into_picture(
        &picture_target,
        track_path,
        ThumbnailSize::Portrait,
        token,
        current,
        move |loaded| {
            if current_for_result.get() != token {
                return;
            }
            source.set(if loaded {
                StatsArtworkSource::Cover
            } else {
                StatsArtworkSource::Initials
            });
            picture_for_result.set_visible(loaded);
            fallback_for_result.set_visible(!loaded);
        },
    );
}
