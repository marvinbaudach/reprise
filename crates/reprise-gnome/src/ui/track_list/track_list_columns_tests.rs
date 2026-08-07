use reprise_core::models::{MissingReason, Track};
use reprise_core::queries::QueueItemMetadata;
use std::cell::RefCell;
use std::rc::Rc;

use super::*;

fn track_metadata(id: i64, title: &str, album: &str, album_artist: &str) -> QueueItemMetadata {
    QueueItemMetadata::Track(Track {
        id,
        path: format!("/music/{id}.flac"),
        title: title.into(),
        artist: album_artist.into(),
        album: album.into(),
        album_artist: album_artist.into(),
        year: None,
        track_no: None,
        genre: String::new(),
        duration_ms: 0,
        bitrate_kbps: None,
        rating: 0,
        play_count: 0,
        last_played_at: None,
        added_at: 0,
        file_mtime: 0,
        missing_since: None,
        missing_reason: None,
        untagged: false,
        file_size: 0,
        device: None,
        inode: None,
        playlist_position: None,
        is_ai: false,
    })
}

#[test]
fn cover_link_presentation_requires_an_album_and_names_the_target() {
    let album = cover_link_presentation(&track_metadata(7, "Blue", "Blue Album", "The Artist"));
    assert_eq!(album.accessible_label, "Go to album Blue Album");
    assert_eq!(
        album.target,
        Some(CoverAlbumTarget {
            track_id: 7,
            album: "Blue Album".into(),
            album_artist: "The Artist".into(),
        })
    );

    let untitled = cover_link_presentation(&track_metadata(8, "Loose Track", "  ", "Solo"));
    assert_eq!(untitled.accessible_label, "Loose Track");
    assert_eq!(untitled.target, None);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_13_track_cover_is_an_album_link_only_with_an_unambiguous_target() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let cover = TrackCover::new();
    let slot = arm_cover_album_link(&cover);
    let activate: Rc<dyn Fn(CoverAlbumTarget)> = Rc::new(|_| {});

    bind_cover_album_link(
        &cover,
        &slot,
        cover_link_presentation(&track_metadata(7, "Blue", "Blue Album", "The Artist")),
        activate.clone(),
    );
    assert!(cover.is_focusable());
    assert_eq!(cover.accessible_role(), gtk4::AccessibleRole::Link);
    assert!(cover.has_css_class(crate::ui::link_activation::LINK_CLASS));
    assert_eq!(
        cover.cursor().and_then(|cursor| cursor.name()).as_deref(),
        Some("pointer")
    );
    assert!(gtk4::test_accessible_has_property(
        &cover,
        gtk4::AccessibleProperty::Label
    ));

    bind_cover_album_link(
        &cover,
        &slot,
        cover_link_presentation(&track_metadata(8, "Loose Track", "", "Solo")),
        activate,
    );
    assert!(!cover.is_focusable());
    assert_eq!(cover.accessible_role(), gtk4::AccessibleRole::Img);
    assert!(!cover.has_css_class(crate::ui::link_activation::LINK_CLASS));
    assert!(cover.cursor().is_none());
    assert!(slot.borrow().is_none());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn recycled_cover_activation_uses_the_newly_bound_album_target() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let cover = TrackCover::new();
    let slot = arm_cover_album_link(&cover);
    let targets = Rc::new(RefCell::new(Vec::new()));
    let activate: Rc<dyn Fn(CoverAlbumTarget)> = {
        let targets = targets.clone();
        Rc::new(move |target| targets.borrow_mut().push(target))
    };

    bind_cover_album_link(
        &cover,
        &slot,
        cover_link_presentation(&track_metadata(1, "First", "First Album", "First Artist")),
        activate.clone(),
    );
    bind_cover_album_link(
        &cover,
        &slot,
        cover_link_presentation(&track_metadata(
            2,
            "Second",
            "Second Album",
            "Second Artist",
        )),
        activate,
    );

    let callback = slot.borrow().clone().expect("the rebound cover is armed");
    callback();
    assert_eq!(
        *targets.borrow(),
        vec![CoverAlbumTarget {
            track_id: 2,
            album: "Second Album".into(),
            album_artist: "Second Artist".into(),
        }]
    );

    clear_cover_album_link(&cover, &slot, "");
    assert!(
        slot.borrow().is_none(),
        "an unbound cell keeps no old target"
    );
    assert_eq!(cover.accessible_role(), gtk4::AccessibleRole::Img);
    assert!(!cover.has_css_class(crate::ui::link_activation::LINK_CLASS));
}

#[test]
fn ac_24_the_running_row_reads_neither_bass_signal() {
    // The EQ bars and the row colour are the whole marker (MOT-5, NAV-10b).
    for source in [
        include_str!("track_list_columns.rs"),
        include_str!("track_list_title_column.rs"),
    ] {
        assert!(!source.contains("row_wash"));
        assert!(!source.contains("kick"));
    }
}

#[test]
fn rating_sort_requires_query_reload_but_other_sorts_need_one_row() {
    assert_eq!(rating_refresh_for_sort("rating"), RatingRefresh::Query);
    assert_eq!(rating_refresh_for_sort("title"), RatingRefresh::Row);
}

#[test]
fn missing_track_explanation_distinguishes_unavailable_drive_from_missing_file() {
    assert_eq!(
        missing_track_explanation(Some(1_000_000_000), Some(MissingReason::Unmounted)),
        Some("On unavailable drive — returns when mounted".into())
    );
    for reason in [MissingReason::Deleted, MissingReason::Unknown] {
        assert_eq!(
            missing_track_explanation(Some(1_000_000_000), Some(reason)),
            Some("File missing since 2001-09-09 01:46".into())
        );
    }
    assert_eq!(missing_track_explanation(None, None), None);
}

#[test]
fn missing_title_css_uses_half_opacity() {
    let css = crate::ui::track_list_row_interaction::css();
    assert!(css.contains(".missing-track-title"));
    assert!(css.contains("opacity: 0.5"));
}

// UX INST-10: the AI badge renders for AI-manipulated tracks and never on a
// plain one. The provenance flag is the only input — no gate sits in front
// of it, so a track the CLI/MCP frontends produced is always marked.
#[test]
fn inst_10_ai_badge_shows_only_for_ai_tracks() {
    assert!(ai_badge_visible(true), "an AI track shows the badge");
    assert!(!ai_badge_visible(false), "a plain track shows no badge");
}
