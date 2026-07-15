//! GtkSignalListItemFactory for album grid cards. Builds the full card
//! widget tree in `setup`, populates in `bind`, cleans up in `unbind`.
//! Handles cover loading, placeholder gradients, hover overlay with play
//! button, now-playing EQ bars, tooltips, and artist deep-link.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::queries::AlbumSummary;
use rusqlite::Connection;

use crate::ui::album_card_css as css;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::eq_bars;
use crate::ui::strings;

/// Leading articles stripped before deriving the placeholder initial.
const LEADING_ARTICLES: &[&str] = &[
    "The ", "A ", "An ", "Die ", "Der ", "Das ", "Les ", "La ", "Le ",
];

/// Shared state injected into every card's factory closures.
#[derive(Clone)]
pub(in crate::ui) struct AlbumCardShared {
    pub conn: Rc<RefCell<Connection>>,
    pub cover_loader: Rc<CoverLoader>,
    pub generation: Rc<Cell<u64>>,
    /// `(album_key, artist_key)` of the currently playing album, if any.
    pub now_playing_album: Rc<RefCell<Option<(String, String)>>>,
    /// True when playback is paused (EQ bars freeze).
    pub playback_paused: Rc<Cell<bool>>,
    /// Card click → navigate to Tracks tab with album filter.
    pub on_activate: Rc<RefCell<Option<Rc<dyn Fn(AlbumSummary)>>>>,
    /// Play button click → replace queue + play.
    pub on_play: Rc<RefCell<Option<Rc<dyn Fn(&AlbumSummary)>>>>,
    /// Shift+play button → append to queue.
    pub on_queue: Rc<RefCell<Option<Rc<dyn Fn(&AlbumSummary)>>>>,
    /// Artist label click → navigate to Artists tab.
    pub on_artist_activate: Rc<RefCell<Option<Rc<dyn Fn(String)>>>>,
}

pub(in crate::ui) fn build_factory(
    shared: &Rc<AlbumCardShared>,
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();

    // ── setup ──────────────────────────────────────────────────────────────
    {
        let shared = shared.clone();
        factory.connect_setup(move |_factory, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");

            // Cover image fills the aspect frame.
            let cover = gtk4::Image::builder()
                .css_classes(vec![css::COVER_CLASS.to_owned()])
                .pixel_size(256)
                .build();

            // Placeholder: shown until a cover is available.
            let placeholder = gtk4::Box::builder()
                .css_classes(vec![css::PLACEHOLDER_CLASS.to_owned()])
                .halign(gtk4::Align::Fill)
                .valign(gtk4::Align::Fill)
                .build();
            let placeholder_initial = gtk4::Label::builder()
                .css_classes(vec![css::PLACEHOLDER_INITIAL_CLASS.to_owned()])
                .halign(gtk4::Align::Center)
                .valign(gtk4::Align::Center)
                .hexpand(true)
                .vexpand(true)
                .build();
            placeholder.append(&placeholder_initial);

            // Attach a CssProvider to the placeholder for per-album gradients.
            // We store it in a RefCell embedded in the widget hierarchy via a
            // key on the placeholder's qdata so it survives reuse.
            let gradient_provider = gtk4::CssProvider::new();
            placeholder
                .style_context()
                .add_provider(&gradient_provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
            // SAFETY: The provider is kept alive by the RefCell and the widget
            // tree. The key is a stable static string. We only read/write on
            // the GTK main thread.
            unsafe {
                placeholder.set_data("gradient-provider", gradient_provider);
            }

            // Hover overlay: gradient scrim + bottom row with EQ + play button.
            let hover_overlay = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .css_classes(vec![css::HOVER_OVERLAY_CLASS.to_owned()])
                .halign(gtk4::Align::Fill)
                .valign(gtk4::Align::Fill)
                .build();
            let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            spacer.set_vexpand(true);
            hover_overlay.append(&spacer);

            // Bottom row: EQ bars (left) and play button (right).
            let bottom_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            let eq_bars_widget = eq_bars::build(eq_bars::EqVariant::Animated);
            let eq_container = gtk4::Box::builder()
                .css_classes(vec![css::EQ_CONTAINER_CLASS.to_owned()])
                .halign(gtk4::Align::Start)
                .valign(gtk4::Align::End)
                .visible(false)
                .build();
            eq_container.append(&eq_bars_widget);
            bottom_row.append(&eq_container);
            let play_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            play_spacer.set_hexpand(true);
            bottom_row.append(&play_spacer);
            let play_button = gtk4::Button::builder()
                .icon_name("media-playback-start-symbolic")
                .css_classes(vec![css::PLAY_BUTTON_CLASS.to_owned()])
                .halign(gtk4::Align::End)
                .valign(gtk4::Align::End)
                .has_frame(false)
                .build();
            bottom_row.append(&play_button);
            hover_overlay.append(&bottom_row);

            // Square cover container via AspectFrame (ratio 1.0).
            let aspect = gtk4::AspectFrame::builder()
                .ratio(1.0)
                .obey_child(false)
                .build();
            // GtkAspectFrame's child is the cover image.
            aspect.set_child(Some(&cover));

            // Overlay: cover image is the base child, overlays are layered on top.
            let cover_overlay = gtk4::Overlay::builder()
                .css_classes(vec![css::COVER_CONTAINER_CLASS.to_owned()])
                .child(&aspect)
                .build();
            cover_overlay.add_overlay(&placeholder);
            cover_overlay.add_overlay(&hover_overlay);

            // Hover detection toggles a CSS class; the CSS does the opacity
            // transition so we don't fight the GTK state machine.
            let motion = gtk4::EventControllerMotion::new();
            {
                let card_ref = cover_overlay.downgrade();
                motion.connect_enter(move |_ctrl, _x, _y| {
                    if let Some(card) = card_ref.upgrade() {
                        card.add_css_class("album-hovered");
                    }
                });
            }
            {
                let card_ref = cover_overlay.downgrade();
                motion.connect_leave(move |_ctrl| {
                    if let Some(card) = card_ref.upgrade() {
                        card.remove_css_class("album-hovered");
                    }
                });
            }
            cover_overlay.add_controller(motion);

            // Text labels below cover.
            let title_label = gtk4::Label::builder()
                .css_classes(vec![css::TITLE_CLASS.to_owned()])
                .xalign(0.0)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .max_width_chars(24)
                .build();
            let subtitle_label = gtk4::Label::builder()
                .css_classes(vec![css::SUBTITLE_CLASS.to_owned()])
                .xalign(0.0)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .max_width_chars(24)
                .build();

            // Artist deep-link: GestureClick on subtitle.
            let artist_click = gtk4::GestureClick::new();
            artist_click.set_propagation_phase(gtk4::PropagationPhase::Capture);
            {
                let on_artist = shared.on_artist_activate.clone();
                let subtitle_weak = subtitle_label.downgrade();
                artist_click.connect_released(move |gesture, _n, _x, _y| {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    let callback = on_artist.borrow().clone();
                    if let (Some(cb), Some(label)) = (callback, subtitle_weak.upgrade()) {
                        let text = label.text().to_string();
                        if !text.is_empty() {
                            cb(text);
                        }
                    }
                });
            }
            subtitle_label.add_controller(artist_click);
            subtitle_label.set_cursor_from_name(Some("pointer"));

            // Root card box.
            let card = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .css_classes(vec![css::CARD_CLASS.to_owned()])
                .build();
            card.append(&cover_overlay);
            card.append(&title_label);
            card.append(&subtitle_label);

            // Play button click: Shift = enqueue, plain = play.
            {
                let on_play = shared.on_play.clone();
                let on_queue = shared.on_queue.clone();
                let list_item_weak = list_item.downgrade();
                play_button.connect_clicked(move |btn| {
                    let Some(li) = list_item_weak.upgrade() else {
                        return;
                    };
                    let Some(obj) = li.item() else { return };
                    let boxed = obj.downcast_ref::<glib::BoxedAnyObject>().unwrap();
                    let album: std::cell::Ref<AlbumSummary> = boxed.borrow();

                    let shift = btn
                        .display()
                        .default_seat()
                        .and_then(|seat| seat.keyboard())
                        .map_or(false, |kb| {
                            kb.modifier_state()
                                .contains(gtk4::gdk::ModifierType::SHIFT_MASK)
                        });
                    if shift {
                        if let Some(cb) = on_queue.borrow().clone() {
                            cb(&album);
                        }
                    } else if let Some(cb) = on_play.borrow().clone() {
                        cb(&album);
                    }
                });
            }

            // Card click → activate.
            let card_click = gtk4::GestureClick::new();
            {
                let on_activate = shared.on_activate.clone();
                let list_item_weak = list_item.downgrade();
                card_click.connect_released(move |gesture, _n, _x, _y| {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    let Some(li) = list_item_weak.upgrade() else {
                        return;
                    };
                    let Some(obj) = li.item() else { return };
                    let boxed = obj.downcast_ref::<glib::BoxedAnyObject>().unwrap();
                    let album: std::cell::Ref<AlbumSummary> = boxed.borrow();
                    if let Some(cb) = on_activate.borrow().clone() {
                        cb(album.clone());
                    }
                });
            }
            card.add_controller(card_click);

            list_item.set_child(Some(&card));
        });
    }

    // ── bind ───────────────────────────────────────────────────────────────
    {
        let shared = shared.clone();
        factory.connect_bind(move |_factory, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            let obj = list_item.item().unwrap();
            let boxed = obj.downcast_ref::<glib::BoxedAnyObject>().unwrap();
            let album: std::cell::Ref<AlbumSummary> = boxed.borrow();

            // Navigate: card > cover_overlay > (aspect+placeholder+hover), title, subtitle
            let card = list_item
                .child()
                .and_downcast::<gtk4::Box>()
                .expect("card Box");
            let cover_overlay = card
                .first_child()
                .and_downcast::<gtk4::Overlay>()
                .expect("cover Overlay");
            let title_label = cover_overlay
                .next_sibling()
                .and_downcast::<gtk4::Label>()
                .expect("title Label");
            let subtitle_label = title_label
                .next_sibling()
                .and_downcast::<gtk4::Label>()
                .expect("subtitle Label");

            title_label.set_text(&album.album);
            let artist_display = if album.album_artist.is_empty() {
                strings::text(strings::UNKNOWN_ARTIST)
            } else {
                album.album_artist.clone()
            };
            subtitle_label.set_text(&artist_display);
            card.set_tooltip_text(Some(&format_tooltip(&album)));

            // cover_overlay children: aspect (base child), placeholder, hover_overlay.
            // first_child() = aspect frame (the overlay's base child).
            let aspect = cover_overlay
                .first_child()
                .and_downcast::<gtk4::AspectFrame>()
                .expect("AspectFrame");
            let cover = aspect
                .child()
                .and_downcast::<gtk4::Image>()
                .expect("cover Image");

            // Load cover art.
            let generation = shared.generation.get();
            shared.cover_loader.load_into(
                &cover,
                &album.representative_path,
                ThumbnailSize::Grid,
                generation,
                &shared.generation,
            );

            // Placeholder: next overlay child after aspect.
            let placeholder = aspect
                .next_sibling()
                .and_downcast::<gtk4::Box>()
                .expect("placeholder Box");

            // Update gradient via the stored CssProvider.
            let gradient_css =
                placeholder_css_for_album(&album.album, &album.album_artist);
            // SAFETY: same thread, same key used in setup.
            let provider: Option<gtk4::CssProvider> = unsafe {
                placeholder
                    .data::<gtk4::CssProvider>("gradient-provider")
                    .map(|ptr| ptr.as_ref().clone())
            };
            if let Some(provider) = provider {
                provider.load_from_data(&gradient_css);
            }
            if let Some(initial) = placeholder
                .first_child()
                .and_downcast::<gtk4::Label>()
            {
                initial.set_text(&derive_initial(&album.album));
            }
            placeholder.set_visible(true);

            // Hover overlay: next sibling after placeholder.
            let hover_box = placeholder
                .next_sibling()
                .and_downcast::<gtk4::Box>()
                .expect("hover_overlay Box");

            let is_now_playing =
                shared
                    .now_playing_album
                    .borrow()
                    .as_ref()
                    .map_or(false, |(a, ar)| {
                        a.eq_ignore_ascii_case(&album.album)
                            && ar.eq_ignore_ascii_case(&album.album_artist)
                    });

            // Bottom row: spacer (first child), then bottom_row (last child).
            if let Some(bottom_row) = hover_box.last_child().and_downcast::<gtk4::Box>() {
                if let Some(eq_container) =
                    bottom_row.first_child().and_downcast::<gtk4::Box>()
                {
                    eq_container.set_visible(is_now_playing);
                }
                if let Some(play_btn) = bottom_row.last_child().and_downcast::<gtk4::Button>() {
                    play_btn.set_icon_name(if is_now_playing {
                        "media-playback-pause-symbolic"
                    } else {
                        "media-playback-start-symbolic"
                    });
                }
            }
            if is_now_playing {
                hover_box.add_css_class("album-now-playing");
            }
        });
    }

    // ── unbind ─────────────────────────────────────────────────────────────
    factory.connect_unbind(move |_factory, list_item| {
        let list_item = list_item
            .downcast_ref::<gtk4::ListItem>()
            .expect("ListItem");
        let card = list_item
            .child()
            .and_downcast::<gtk4::Box>()
            .expect("card Box");
        let cover_overlay = card
            .first_child()
            .and_downcast::<gtk4::Overlay>()
            .expect("cover Overlay");

        // Clear cover texture to free memory.
        let aspect = cover_overlay
            .first_child()
            .and_downcast::<gtk4::AspectFrame>();
        if let Some(aspect) = aspect {
            if let Some(cover) = aspect.child().and_downcast::<gtk4::Image>() {
                cover.clear();
            }
        }

        // Reset now-playing state.
        let placeholder = cover_overlay
            .first_child()
            .and_then(|w| w.next_sibling())
            .and_downcast::<gtk4::Box>();
        if let Some(placeholder) = placeholder {
            if let Some(hover_box) = placeholder
                .next_sibling()
                .and_downcast::<gtk4::Box>()
            {
                hover_box.remove_css_class("album-now-playing");
                if let Some(bottom_row) = hover_box.last_child().and_downcast::<gtk4::Box>() {
                    if let Some(play_btn) = bottom_row.last_child().and_downcast::<gtk4::Button>() {
                        play_btn.set_icon_name("media-playback-start-symbolic");
                    }
                    if let Some(eq_container) =
                        bottom_row.first_child().and_downcast::<gtk4::Box>()
                    {
                        eq_container.set_visible(false);
                    }
                }
            }
        }
    });

    factory
}

/// Generates CSS for the placeholder gradient based on album+artist hash.
/// OKLCH: Start L≈0.45/C≈0.08, End L≈0.18/C≈0.05, angle 135°, hue from hash.
pub(in crate::ui) fn placeholder_css_for_album(album: &str, album_artist: &str) -> String {
    let mut hasher = DefaultHasher::new();
    album.to_lowercase().hash(&mut hasher);
    album_artist.to_lowercase().hash(&mut hasher);
    let hash = hasher.finish();
    let hue1 = (hash % 360) as f64;
    let hue2 = ((hash >> 16) % 360) as f64;
    let h2 = if (hue2 - hue1).abs() < 30.0 {
        hue1 + 40.0
    } else {
        hue2
    };
    format!(
        ".{} {{ background: linear-gradient(135deg, \
           oklch(0.45 0.08 {hue1:.0}), oklch(0.18 0.05 {h2:.0})); }}",
        css::PLACEHOLDER_CLASS,
    )
}

/// Derives a single initial from the album title: strips leading articles,
/// takes the first alphanumeric character (uppercase), fallback "♪".
pub(in crate::ui) fn derive_initial(album: &str) -> String {
    let mut title = album.trim();
    for article in LEADING_ARTICLES {
        if let Some(rest) = title.strip_prefix(article) {
            title = rest;
            break;
        }
    }
    title
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "♪".to_string())
}

/// Tooltip: "Title · Artist · Year · N tracks · Duration".
pub(in crate::ui) fn format_tooltip(album: &AlbumSummary) -> String {
    let mut parts = vec![album.album.clone()];
    if !album.album_artist.is_empty() {
        parts.push(album.album_artist.clone());
    }
    if let Some(year) = album.year {
        parts.push(year.to_string());
    }
    parts.push(format!("{} tracks", album.track_count));
    parts.push(strings::album_duration(album.total_duration_ms));
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_initial_strips_leading_articles() {
        assert_eq!(derive_initial("The Wall"), "W");
        assert_eq!(derive_initial("A Rush of Blood"), "R");
        assert_eq!(derive_initial("Die Ärzte"), "Ä");
    }

    #[test]
    fn derive_initial_uses_first_alphanumeric() {
        assert_eq!(derive_initial("  123 Album"), "1");
        assert_eq!(derive_initial("...Trails"), "T");
    }

    #[test]
    fn derive_initial_fallback_is_music_note() {
        assert_eq!(derive_initial(""), "♪");
        assert_eq!(derive_initial("---"), "♪");
    }

    #[test]
    fn placeholder_gradient_is_consistent() {
        let css1 = placeholder_css_for_album("Album", "Artist");
        let css2 = placeholder_css_for_album("Album", "Artist");
        assert_eq!(css1, css2);
        assert!(css1.contains("oklch(0.45"));
        assert!(css1.contains("oklch(0.18"));
        assert!(css1.contains("135deg"));
    }

    #[test]
    fn placeholder_gradient_differs_for_same_album_different_artist() {
        let css1 = placeholder_css_for_album("Greatest Hits", "Artist A");
        let css2 = placeholder_css_for_album("Greatest Hits", "Artist B");
        assert_ne!(css1, css2);
    }

    #[test]
    fn format_tooltip_includes_all_fields() {
        let album = AlbumSummary {
            album: "OK Computer".into(),
            album_artist: "Radiohead".into(),
            representative_path: "/a.flac".into(),
            track_count: 12,
            year: Some(1997),
            total_duration_ms: 3_180_000,
            max_added_at: 0,
            total_play_count: 0,
        };
        let tip = format_tooltip(&album);
        assert!(tip.contains("OK Computer"));
        assert!(tip.contains("Radiohead"));
        assert!(tip.contains("1997"));
        assert!(tip.contains("12 tracks"));
        assert!(tip.contains("53 min"));
    }

    #[test]
    fn format_tooltip_omits_year_when_none() {
        let album = AlbumSummary {
            album: "Untitled".into(),
            album_artist: "".into(),
            representative_path: "/a.flac".into(),
            track_count: 1,
            year: None,
            total_duration_ms: 60_000,
            max_added_at: 0,
            total_play_count: 0,
        };
        let tip = format_tooltip(&album);
        // No empty segment from missing year or artist.
        assert!(!tip.contains(" ·  · "));
    }
}
