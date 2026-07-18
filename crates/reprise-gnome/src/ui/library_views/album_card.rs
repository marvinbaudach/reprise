//! GtkSignalListItemFactory for album grid cards. Builds the full card
//! widget tree in `setup`, populates in `bind`, cleans up in `unbind`.
//! Handles cover loading, placeholder gradients, hover overlay with play
//! button, now-playing EQ bars, tooltips, and artist deep-link.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::queries::AlbumSummary;

use crate::ui::album_card_css as css;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::discovery_hint::{EvidenceTracker, VisibleEvidence};
use crate::ui::eq_bars;
use crate::ui::strings;

/// Leading articles stripped before deriving the placeholder initial.
const LEADING_ARTICLES: &[&str] = &[
    "The ", "A ", "An ", "Die ", "Der ", "Das ", "Les ", "La ", "Le ",
];

pub(in crate::ui) type AlbumActivate = Rc<dyn Fn(AlbumSummary)>;
pub(in crate::ui) type AlbumAction = Rc<dyn Fn(&AlbumSummary)>;
pub(in crate::ui) type ArtistActivate = Rc<dyn Fn(String)>;
pub(in crate::ui) type AlbumActivateSlot = Rc<RefCell<Option<AlbumActivate>>>;
pub(in crate::ui) type AlbumActionSlot = Rc<RefCell<Option<AlbumAction>>>;
pub(in crate::ui) type ArtistActivateSlot = Rc<RefCell<Option<ArtistActivate>>>;

/// Shared state injected into every card's factory closures.
#[derive(Clone)]
pub(in crate::ui) struct AlbumCardShared {
    pub cover_loader: Rc<CoverLoader>,
    pub fallback_evidence: EvidenceTracker,
    pub generation: Rc<Cell<u64>>,
    /// `(album_key, artist_key)` of the currently playing album, if any.
    pub now_playing_album: Rc<RefCell<Option<(String, String)>>>,
    /// Play button click → replace queue + play.
    pub on_play: AlbumActionSlot,
    /// Shift+play button → append to queue.
    pub on_queue: AlbumActionSlot,
    /// Artist label click → navigate to Artists tab.
    pub on_artist_activate: ArtistActivateSlot,
}

pub(in crate::ui) fn build_factory(shared: &Rc<AlbumCardShared>) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    let evidence_items: Rc<RefCell<HashMap<usize, VisibleEvidence>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // ── setup ──────────────────────────────────────────────────────────────
    {
        let shared = shared.clone();
        let evidence_items = evidence_items.clone();
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
            let evidence = shared.fallback_evidence.visible_item();
            wire_fallback_evidence(&placeholder, &evidence);
            evidence_items
                .borrow_mut()
                .insert(list_item.as_ptr() as usize, evidence);
            let placeholder_initial = gtk4::Label::builder()
                .css_classes(vec![css::PLACEHOLDER_INITIAL_CLASS.to_owned()])
                .halign(gtk4::Align::Center)
                .valign(gtk4::Align::Center)
                .hexpand(true)
                .vexpand(true)
                .build();
            placeholder.append(&placeholder_initial);

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
            play_button.set_tooltip_text(Some(&strings::text(strings::PLAY_ALBUM)));
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
                        .is_some_and(|kb| {
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

            // Card click → activate: handled by the GridView itself via
            // `single_click_activate` (see `album_view`) — the cell
            // machinery emits `activate`, which `album_view` routes to
            // `on_activate`. A per-card `GestureClick` on this plain `Box`
            // was unreliable (the cell machinery claims the press sequence
            // — the "GestureClick on a plain Box inside a cell" trap), so
            // the card deliberately has NO click gesture of its own. The
            // hover play button and the artist subtitle keep their own
            // gestures; their claims stop the cell activation.

            list_item.set_child(Some(&card));
        });
    }

    // ── bind ───────────────────────────────────────────────────────────────
    {
        let shared = shared.clone();
        let evidence_items = evidence_items.clone();
        factory.connect_bind(move |_factory, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            let obj = list_item.item().unwrap();
            let boxed = obj.downcast_ref::<glib::BoxedAnyObject>().unwrap();
            let album: std::cell::Ref<AlbumSummary> = boxed.borrow();
            let evidence = evidence_items
                .borrow()
                .get(&(list_item.as_ptr() as usize))
                .cloned();
            if let Some(evidence) = &evidence {
                evidence.set_fallback(false);
            }

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

            // Placeholder: next overlay child after aspect.
            let placeholder = aspect
                .next_sibling()
                .and_downcast::<gtk4::Box>()
                .expect("placeholder Box");

            for index in 0..css::PLACEHOLDER_GRADIENT_COUNT {
                placeholder.remove_css_class(&format!("album-placeholder-gradient-{index}"));
            }
            placeholder.add_css_class(&placeholder_class_for_album(
                &album.album,
                &album.album_artist,
            ));
            if let Some(initial) = placeholder.first_child().and_downcast::<gtk4::Label>() {
                initial.set_text(&derive_initial(&album.album));
            }
            placeholder.set_visible(true);

            // Load cover art; hide placeholder once cover is available.
            let generation = shared.generation.get();
            let placeholder_weak = placeholder.downgrade();
            shared.cover_loader.load_into_with_resolution(
                &cover,
                &album.representative_path,
                ThumbnailSize::Grid,
                generation,
                &shared.generation,
                move |resolved| {
                    if let Some(ph) = placeholder_weak.upgrade() {
                        ph.set_visible(resolved.is_none());
                    }
                    if let Some(evidence) = &evidence {
                        evidence.set_fallback(resolved.is_none());
                    }
                },
            );

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
                    .is_some_and(|(a, ar)| {
                        a.eq_ignore_ascii_case(&album.album)
                            && ar.eq_ignore_ascii_case(&album.album_artist)
                    });

            // Bottom row: spacer (first child), then bottom_row (last child).
            if let Some(bottom_row) = hover_box.last_child().and_downcast::<gtk4::Box>() {
                if let Some(eq_container) = bottom_row.first_child().and_downcast::<gtk4::Box>() {
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
    let evidence_for_unbind = evidence_items.clone();
    factory.connect_unbind(move |_factory, list_item| {
        let list_item = list_item
            .downcast_ref::<gtk4::ListItem>()
            .expect("ListItem");
        if let Some(evidence) = evidence_for_unbind
            .borrow()
            .get(&(list_item.as_ptr() as usize))
        {
            evidence.set_fallback(false);
        }
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

        // Reset now-playing state and hide placeholder to prevent stale flash.
        let placeholder = cover_overlay
            .first_child()
            .and_then(|w| w.next_sibling())
            .and_downcast::<gtk4::Box>();
        if let Some(placeholder) = placeholder {
            placeholder.set_visible(false);
            if let Some(hover_box) = placeholder.next_sibling().and_downcast::<gtk4::Box>() {
                hover_box.remove_css_class("album-now-playing");
                if let Some(bottom_row) = hover_box.last_child().and_downcast::<gtk4::Box>() {
                    if let Some(play_btn) = bottom_row.last_child().and_downcast::<gtk4::Button>() {
                        play_btn.set_icon_name("media-playback-start-symbolic");
                    }
                    if let Some(eq_container) = bottom_row.first_child().and_downcast::<gtk4::Box>()
                    {
                        eq_container.set_visible(false);
                    }
                }
            }
        }
    });

    factory.connect_teardown(move |_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<gtk4::ListItem>() {
            evidence_items
                .borrow_mut()
                .remove(&(list_item.as_ptr() as usize));
        }
    });

    factory
}

fn wire_fallback_evidence(placeholder: &gtk4::Box, evidence: &VisibleEvidence) {
    let visible = evidence.clone();
    placeholder.connect_map(move |_| visible.set_mapped(true));
    let hidden = evidence.clone();
    placeholder.connect_unmap(move |_| hidden.set_mapped(false));
}

/// Simple deterministic hasher (multiply-and-xor) for stable color generation.
fn simple_hash(input: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in input.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

/// Maps an album identity onto the centrally registered placeholder palette.
pub(in crate::ui) fn placeholder_class_for_album(album: &str, album_artist: &str) -> String {
    let input = format!("{}{}", album.to_lowercase(), album_artist.to_lowercase());
    let index = simple_hash(&input) as usize % css::PLACEHOLDER_GRADIENT_COUNT;
    format!("album-placeholder-gradient-{index}")
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
        .map_or_else(|| "♪".to_string(), |c| c.to_uppercase().to_string())
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
    use std::time::Duration;

    use super::*;

    fn wait_for_layout() {
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        gtk4::glib::timeout_add_local_once(Duration::from_millis(50), move || quit.quit());
        main_loop.run();
    }

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
    fn placeholder_gradient_class_is_consistent() {
        let class1 = placeholder_class_for_album("Album", "Artist");
        let class2 = placeholder_class_for_album("Album", "Artist");
        assert_eq!(class1, class2);
        assert!(class1.starts_with("album-placeholder-gradient-"));
    }

    #[test]
    fn placeholder_gradient_class_stays_in_palette() {
        for artist in ["Artist A", "Artist B", "Artist C", "Artist D"] {
            let class = placeholder_class_for_album("Greatest Hits", artist);
            let index = class
                .strip_prefix("album-placeholder-gradient-")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            assert!(index < css::PLACEHOLDER_GRADIENT_COUNT);
        }
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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn tip_1a_album_card_play_overlay_has_tooltip() {
        if gtk4::init().is_err() {
            return;
        }
        let (worker, _receiver) = async_channel::unbounded();
        let cover_loader =
            CoverLoader::new(crate::ui::cover_download_worker::CoverDownloadRuntime {
                enabled: Rc::new(Cell::new(false)),
                worker,
            });
        let shared = Rc::new(AlbumCardShared {
            cover_loader,
            fallback_evidence: EvidenceTracker::new(true),
            generation: Rc::new(Cell::new(0)),
            now_playing_album: Rc::new(RefCell::new(None)),
            on_play: Rc::new(RefCell::new(None)),
            on_queue: Rc::new(RefCell::new(None)),
            on_artist_activate: Rc::new(RefCell::new(None)),
        });
        let store = gtk4::gio::ListStore::new::<glib::BoxedAnyObject>();
        store.append(&glib::BoxedAnyObject::new(AlbumSummary {
            album: "Test Album".into(),
            album_artist: "Test Artist".into(),
            representative_path: "/nonexistent/test.flac".into(),
            track_count: 1,
            year: Some(2026),
            total_duration_ms: 60_000,
            max_added_at: 0,
            total_play_count: 0,
        }));
        let selection = gtk4::NoSelection::new(Some(store));
        let grid = gtk4::GridView::new(Some(selection), Some(build_factory(&shared)));
        let window = gtk4::Window::builder().child(&grid).build();
        window.present();
        wait_for_layout();

        let violations = crate::ui::tooltip_discipline::tooltip_violations(grid.upcast_ref());
        assert!(violations.is_empty(), "{violations:?}");
        window.close();
    }
}
