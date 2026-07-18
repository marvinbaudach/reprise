//! GtkSignalListItemFactory for album grid cards. Builds the full card
//! widget tree in `setup`, populates in `bind`, cleans up in `unbind`.
//! Handles cover loading, placeholder gradients, hover overlay with play
//! button, now-playing EQ bars, tooltips, and artist deep-link.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::playback::PlaybackState;
use reprise_core::queries::AlbumSummary;

use crate::ui::album_card_css as css;
use crate::ui::album_card_state::{
    presentation, AlbumCardIdentityRegistry, AlbumCardPlayback, PendingAlbumReveal,
    RevealBindingRegistry,
};
use crate::ui::cover_loader::CoverLoader;
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
    pub generation: Rc<Cell<u64>>,
    pub identity_generation: Rc<Cell<u64>>,
    pub identities: Rc<RefCell<AlbumCardIdentityRegistry>>,
    pub playback_state: Rc<Cell<PlaybackState>>,
    pub reveal_generation: Rc<Cell<u64>>,
    pub pending_reveal: Rc<RefCell<Option<PendingAlbumReveal>>>,
    pub reveal_bindings: Rc<RefCell<RevealBindingRegistry>>,
    /// `(album_key, artist_key)` of the currently playing album, if any.
    pub now_playing_album: Rc<RefCell<Option<(String, String)>>>,
    /// Play button click → replace queue + play.
    pub on_play: AlbumActionSlot,
    /// Pointer-only primary button → toggle current album or rebuild.
    pub on_primary: AlbumActionSlot,
    /// Artist label click → navigate to Artists tab.
    pub on_artist_activate: ArtistActivateSlot,
}

pub(in crate::ui) fn build_factory(shared: &Rc<AlbumCardShared>) -> gtk4::SignalListItemFactory {
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

            // Persistent now-playing layer: independent from hover/focus.
            let playing_layer = gtk4::Box::builder()
                .css_classes(vec![css::PLAYING_LAYER_CLASS.to_owned()])
                .halign(gtk4::Align::Start)
                .valign(gtk4::Align::Start)
                .visible(false)
                .build();
            playing_layer.append(&eq_bars::build(eq_bars::EqVariant::Animated));

            // Bottom interaction gradient: metadata + play button. It stays
            // independent from the persistent EQ/playing layer.
            let hover_overlay = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .css_classes(vec![css::HOVER_OVERLAY_CLASS.to_owned()])
                .halign(gtk4::Align::Fill)
                .valign(gtk4::Align::End)
                .build();
            let meta_label = gtk4::Label::builder()
                .css_classes(vec![css::META_CLASS.to_owned()])
                .xalign(0.0)
                .halign(gtk4::Align::Fill)
                .build();
            hover_overlay.append(&meta_label);

            // Bottom row: play button at the right edge.
            let bottom_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            let play_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            play_spacer.set_hexpand(true);
            bottom_row.append(&play_spacer);
            let play_button = gtk4::Button::builder()
                .icon_name("media-playback-start-symbolic")
                .css_classes(vec![css::PLAY_BUTTON_CLASS.to_owned()])
                .halign(gtk4::Align::End)
                .valign(gtk4::Align::End)
                .has_frame(false)
                .focusable(false)
                .build();
            let play_label = strings::text(strings::PLAY_ALBUM);
            play_button.set_tooltip_text(Some(&play_label));
            play_button.update_property(&[gtk4::accessible::Property::Label(&play_label)]);
            bottom_row.append(&play_button);
            hover_overlay.append(&bottom_row);

            // Square cover container via AspectFrame (ratio 1.0).
            let aspect = gtk4::AspectFrame::builder()
                .ratio(1.0)
                .obey_child(false)
                .build();
            // GtkAspectFrame's child is the cover image.
            aspect.set_child(Some(&cover));

            // The playing frame is an inner cover-only ring above both the
            // image and placeholder. It never intercepts pointer input.
            let playing_frame = gtk4::Box::builder()
                .halign(gtk4::Align::Fill)
                .valign(gtk4::Align::Fill)
                .can_target(false)
                .build();

            // The GridView's native `child` node owns keyboard focus. This
            // cover-only overlay renders its outer focus ring via a selector
            // rooted at that real focused node; the card itself is not a tab
            // stop.
            let focus_frame = gtk4::Box::builder()
                .css_classes(vec![css::FOCUS_FRAME_CLASS.to_owned()])
                .halign(gtk4::Align::Fill)
                .valign(gtk4::Align::Fill)
                .can_target(false)
                .build();

            // Reveal pulse is a third cover-only visual layer. It never
            // replaces the persistent playing ring or native focus ring.
            let reveal_frame = gtk4::Box::builder()
                .css_classes(vec![css::REVEAL_FRAME_CLASS.to_owned()])
                .halign(gtk4::Align::Fill)
                .valign(gtk4::Align::Fill)
                .can_target(false)
                .build();

            // Overlay: cover image is the base child, overlays are layered on top.
            let cover_overlay = gtk4::Overlay::builder()
                .css_classes(vec![css::COVER_CONTAINER_CLASS.to_owned()])
                .child(&aspect)
                .build();
            cover_overlay.add_overlay(&placeholder);
            cover_overlay.add_overlay(&playing_frame);
            cover_overlay.add_overlay(&focus_frame);
            cover_overlay.add_overlay(&reveal_frame);
            cover_overlay.add_overlay(&playing_layer);
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
            crate::ui::ellipsis_tooltip::arm(&title_label);
            crate::ui::ellipsis_tooltip::arm(&subtitle_label);

            // Artist deep-link: GestureClick on subtitle.
            // input-parity: ACC-8 keyboard=album-context-menu
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
            // input-parity: ACC-8 keyboard=album-context-menu
            subtitle_label.set_cursor_from_name(Some("pointer"));

            // Root card box.
            let card = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .css_classes(vec![css::CARD_CLASS.to_owned()])
                .build();
            card.append(&cover_overlay);
            card.append(&title_label);
            card.append(&subtitle_label);

            // Pointer-only primary action. Explicit menu/Ctrl+Enter Play
            // stays on the separate `on_play` rebuild path.
            {
                let on_primary = shared.on_primary.clone();
                let list_item_weak = list_item.downgrade();
                play_button.connect_clicked(move |_| {
                    let Some(li) = list_item_weak.upgrade() else {
                        return;
                    };
                    let Some(obj) = li.item() else { return };
                    let boxed = obj.downcast_ref::<glib::BoxedAnyObject>().unwrap();
                    let album: std::cell::Ref<AlbumSummary> = boxed.borrow();

                    let callback = on_primary.borrow().clone();
                    if let Some(cb) = callback {
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
            card.set_tooltip_text(None);
            card.update_property(&[gtk4::accessible::Property::Description(&format_tooltip(
                &album,
            ))]);

            let identity_generation = shared.identity_generation.get().wrapping_add(1);
            shared.identity_generation.set(identity_generation);
            shared.identities.borrow_mut().bind(
                card.as_ptr() as usize,
                identity_generation,
                album.clone(),
            );

            // cover_overlay children: aspect (base), placeholder, playing
            // frame, playing layer, hover overlay.
            let aspect = cover_overlay
                .first_child()
                .and_downcast::<gtk4::AspectFrame>()
                .expect("AspectFrame");
            let cover = aspect
                .child()
                .and_downcast::<gtk4::Image>()
                .expect("cover Image");

            // Placeholder: next overlay child after the aspect.
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
            shared.cover_loader.load_into_with_path(
                &cover,
                &album.representative_path,
                ThumbnailSize::Grid,
                generation,
                &shared.generation,
                move |_| {
                    if let Some(ph) = placeholder_weak.upgrade() {
                        ph.set_visible(false);
                    }
                },
            );

            let playing_frame = placeholder
                .next_sibling()
                .and_downcast::<gtk4::Box>()
                .expect("playing frame Box");

            let focus_frame = playing_frame
                .next_sibling()
                .and_downcast::<gtk4::Box>()
                .expect("focus frame Box");

            let reveal_frame = focus_frame
                .next_sibling()
                .and_downcast::<gtk4::Box>()
                .expect("reveal frame Box");

            let playing_layer = reveal_frame
                .next_sibling()
                .and_downcast::<gtk4::Box>()
                .expect("playing_layer Box");

            // Hover overlay: next sibling after the persistent layer.
            let hover_box = playing_layer
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

            let playback = if is_now_playing {
                match shared.playback_state.get() {
                    PlaybackState::Playing => AlbumCardPlayback::Playing,
                    PlaybackState::Paused => AlbumCardPlayback::Paused,
                    PlaybackState::Stopped => AlbumCardPlayback::LoadedStopped,
                }
            } else {
                AlbumCardPlayback::Normal
            };
            let card_presentation = presentation(playback);
            playing_layer.set_visible(card_presentation.show_playing_layer);
            if card_presentation.show_playing_layer {
                playing_frame.add_css_class(css::PLAYING_FRAME_CLASS);
            } else {
                playing_frame.remove_css_class(css::PLAYING_FRAME_CLASS);
            }

            let pending_reveal = shared.pending_reveal.borrow().clone();
            if let Some(pending) = pending_reveal.filter(|pending| pending.matches(&album)) {
                let pulse_class = if crate::ui::motion::animations_enabled() {
                    css::REVEAL_PULSE_CLASS
                } else {
                    css::REVEAL_PULSE_STATIC_CLASS
                };
                reveal_frame.remove_css_class(css::REVEAL_PULSE_CLASS);
                reveal_frame.remove_css_class(css::REVEAL_PULSE_STATIC_CLASS);
                reveal_frame.add_css_class(pulse_class);
                shared.pending_reveal.borrow_mut().take();

                let reveal_key = reveal_frame.as_ptr() as usize;
                shared
                    .reveal_bindings
                    .borrow_mut()
                    .bind(reveal_key, pending.generation);

                let reveal_frame = reveal_frame.downgrade();
                let reveal_bindings = shared.reveal_bindings.clone();
                gtk4::glib::timeout_add_local_once(
                    Duration::from_millis(css::REVEAL_DURATION_MS),
                    move || {
                        let may_clear = reveal_bindings
                            .borrow_mut()
                            .take_if_current(reveal_key, pending.generation);
                        if may_clear {
                            if let Some(reveal_frame) = reveal_frame.upgrade() {
                                reveal_frame.remove_css_class(css::REVEAL_PULSE_CLASS);
                                reveal_frame.remove_css_class(css::REVEAL_PULSE_STATIC_CLASS);
                            }
                        }
                    },
                );
            }

            if let Some(meta_label) = hover_box.first_child().and_downcast::<gtk4::Label>() {
                meta_label.set_text(&format_meta(&album));
            }

            // Bottom row: spacer, then play button.
            if let Some(bottom_row) = hover_box.last_child().and_downcast::<gtk4::Box>() {
                if let Some(play_btn) = bottom_row.last_child().and_downcast::<gtk4::Button>() {
                    let (icon, label) = match playback {
                        AlbumCardPlayback::Playing => (
                            "media-playback-pause-symbolic",
                            strings::text(strings::PAUSE_ALBUM),
                        ),
                        AlbumCardPlayback::Paused => (
                            "media-playback-start-symbolic",
                            strings::text(strings::RESUME_ALBUM),
                        ),
                        AlbumCardPlayback::Normal | AlbumCardPlayback::LoadedStopped => (
                            "media-playback-start-symbolic",
                            strings::text(strings::PLAY_ALBUM),
                        ),
                    };
                    play_btn.set_icon_name(icon);
                    play_btn.set_tooltip_text(Some(&label));
                    play_btn.update_property(&[gtk4::accessible::Property::Label(&label)]);
                }
            }
        });
    }

    // ── unbind ─────────────────────────────────────────────────────────────
    {
        let shared = shared.clone();
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
            if let Some(aspect) = &aspect {
                if let Some(cover) = aspect.child().and_downcast::<gtk4::Image>() {
                    cover.clear();
                }
            }
            let playing_frame = aspect
                .and_then(|aspect| aspect.next_sibling())
                .and_then(|placeholder| placeholder.next_sibling())
                .and_downcast::<gtk4::Box>();
            if let Some(playing_frame) = &playing_frame {
                playing_frame.remove_css_class(css::PLAYING_FRAME_CLASS);
                if let Some(reveal_frame) = playing_frame
                    .next_sibling()
                    .and_then(|focus_frame| focus_frame.next_sibling())
                {
                    shared
                        .reveal_bindings
                        .borrow_mut()
                        .unbind_current(reveal_frame.as_ptr() as usize);
                    reveal_frame.remove_css_class(css::REVEAL_PULSE_CLASS);
                    reveal_frame.remove_css_class(css::REVEAL_PULSE_STATIC_CLASS);
                }
            }

            shared
                .identities
                .borrow_mut()
                .unbind_current(card.as_ptr() as usize);

            // Reset now-playing state and hide placeholder to prevent stale flash.
            let placeholder = cover_overlay
                .first_child()
                .and_then(|w| w.next_sibling())
                .and_downcast::<gtk4::Box>();
            if let Some(placeholder) = placeholder {
                placeholder.set_visible(false);
                if let Some(playing_layer) = placeholder
                    .next_sibling()
                    .and_then(|playing_frame| playing_frame.next_sibling())
                    .and_then(|focus_frame| focus_frame.next_sibling())
                    .and_then(|reveal_frame| reveal_frame.next_sibling())
                    .and_downcast::<gtk4::Box>()
                {
                    playing_layer.set_visible(false);
                    if let Some(hover_box) =
                        playing_layer.next_sibling().and_downcast::<gtk4::Box>()
                    {
                        if let Some(bottom_row) = hover_box.last_child().and_downcast::<gtk4::Box>()
                        {
                            if let Some(play_btn) =
                                bottom_row.last_child().and_downcast::<gtk4::Button>()
                            {
                                play_btn.set_icon_name("media-playback-start-symbolic");
                            }
                        }
                    }
                }
            }
        });
    }

    factory
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

/// Full album summary retained as the card's accessible description.
pub(in crate::ui) fn format_tooltip(album: &AlbumSummary) -> String {
    let mut parts = vec![album.album.clone()];
    if !album.album_artist.is_empty() {
        parts.push(album.album_artist.clone());
    }
    if let Some(year) = album.year {
        parts.push(year.to_string());
    }
    parts.push(format_meta(album));
    parts.join(" · ")
}

pub(in crate::ui) fn format_meta(album: &AlbumSummary) -> String {
    strings::album_meta(album.track_count, album.total_duration_ms)
}
