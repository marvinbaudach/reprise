//! Cache-first artist portraits for recycled placeholder tiles.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use reprise_core::artist_portrait::PortraitCredit;
use reprise_core::cover::ThumbnailSize;

use crate::ui::cover_loader::CoverLoader;
use crate::ui::now_playing::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::updates::release_cover::LazyReleaseCover;

pub(in crate::ui) type CachedPortraitResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
pub(in crate::ui) type PortraitCreditResolver =
    Arc<dyn Fn(&str) -> Option<PortraitCredit> + Send + Sync>;
pub(in crate::ui) type CacheOnlyPortraitResolver =
    fn(&str) -> reprise_core::artist_portrait::PortraitOutcome;

// This resolver has no NET-1a gate of its own. Pinning its function type and
// identity to Core's cache-only entry point keeps production construction from
// silently gaining network access.
pub(in crate::ui) const PRODUCTION_CACHE_ONLY_RESOLVER: CacheOnlyPortraitResolver =
    reprise_core::artist_portrait::load_cached;

pub(in crate::ui) struct ArtistPortraitTiles {
    pub(in crate::ui) portrait: RefCell<Option<Rc<ArtistPortraitRuntime>>>,
    pub(in crate::ui) loader: RefCell<Option<Rc<CoverLoader>>>,
    pub(in crate::ui) cached: CachedPortraitResolver,
    credit: PortraitCreditResolver,
    generation: Rc<Cell<u64>>,
}

impl ArtistPortraitTiles {
    #[cfg(not(test))]
    pub(in crate::ui) fn new() -> Rc<Self> {
        Self::with_resolvers(
            |artist| match PRODUCTION_CACHE_ONLY_RESOLVER(artist) {
                reprise_core::artist_portrait::PortraitOutcome::Found(path) => Some(path),
                reprise_core::artist_portrait::PortraitOutcome::NotFound => None,
            },
            reprise_core::artist_portrait::load_cached_credit,
        )
    }

    #[cfg(test)]
    pub(in crate::ui) fn for_test(
        cached: impl Fn(&str) -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Rc<Self> {
        Self::with_resolvers(cached, |_| None)
    }

    #[cfg(test)]
    pub(in crate::ui) fn for_test_with_credit(
        cached: impl Fn(&str) -> Option<PathBuf> + Send + Sync + 'static,
        credit: impl Fn(&str) -> Option<PortraitCredit> + Send + Sync + 'static,
    ) -> Rc<Self> {
        Self::with_resolvers(cached, credit)
    }

    fn with_resolvers(
        cached: impl Fn(&str) -> Option<PathBuf> + Send + Sync + 'static,
        credit: impl Fn(&str) -> Option<PortraitCredit> + Send + Sync + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            portrait: RefCell::new(None),
            loader: RefCell::new(None),
            cached: Arc::new(cached),
            credit: Arc::new(credit),
            generation: Rc::new(Cell::new(0)),
        })
    }

    pub(in crate::ui) fn set_sources(
        &self,
        loader: Rc<CoverLoader>,
        portrait: Rc<ArtistPortraitRuntime>,
    ) {
        *self.loader.borrow_mut() = Some(loader);
        *self.portrait.borrow_mut() = Some(portrait);
    }

    pub(in crate::ui) fn show(self: &Rc<Self>, tile: &LazyReleaseCover) {
        let artist = tile.artist_key();
        let portrait_key = tile.portrait_key();
        apply_credit(tile.widget(), None);
        if artist.trim().is_empty() || tile.portrait_started() == portrait_key {
            return;
        }
        tile.mark_portrait_started(&portrait_key);
        let root = tile.widget().clone();
        let this = self.clone();
        glib::spawn_future_local(async move {
            let lookup_artist = artist.clone();
            let cached = this.cached.clone();
            let credit = this.credit.clone();
            let found = gio::spawn_blocking(move || {
                let path = cached(&lookup_artist);
                let credit = path.as_ref().and_then(|_| credit(&lookup_artist));
                (path, credit)
            })
            .await
            .ok();
            let Some(tile) = LazyReleaseCover::from_widget(&root) else {
                return;
            };
            if tile.portrait_key() != portrait_key || !tile.portrait_is_requested() {
                return;
            }
            if let Some((Some(path), credit)) = found {
                this.show_path(&tile, &portrait_key, &path, credit);
            } else if root.is_mapped() {
                this.fetch_after_cache_miss(&tile);
            } else {
                tile.mark_portrait_started("");
            }
        });
    }

    fn fetch_after_cache_miss(self: &Rc<Self>, tile: &LazyReleaseCover) {
        let artist = tile.artist_key();
        let portrait_key = tile.portrait_key();
        if artist.trim().is_empty() {
            return;
        }
        let runtime = self.portrait.borrow().clone();
        let Some(runtime) = runtime else {
            tile.mark_portrait_started("");
            return;
        };
        if !runtime.request_would_run(&artist) {
            tile.mark_portrait_started("");
            return;
        }

        let guard_root = tile.widget().clone();
        let guard_key = portrait_key.clone();
        let result_root = tile.widget().clone();
        let result_key = portrait_key.clone();
        let this = self.clone();
        runtime.request_while(
            artist,
            move || {
                LazyReleaseCover::from_widget(&guard_root).is_some_and(|tile| {
                    tile.portrait_key() == guard_key && tile.portrait_is_requested()
                })
            },
            move |found| {
                let Some(path) = found else {
                    return;
                };
                let Some(tile) = LazyReleaseCover::from_widget(&result_root) else {
                    return;
                };
                if tile.portrait_key() == result_key && tile.portrait_is_requested() {
                    this.show_path_after_credit_lookup(&tile, &result_key, path);
                }
            },
        );
    }

    fn show_path_after_credit_lookup(
        self: &Rc<Self>,
        tile: &LazyReleaseCover,
        portrait_key: &str,
        path: PathBuf,
    ) {
        let artist = tile.artist_key();
        let root = tile.widget().clone();
        let portrait_key = portrait_key.to_owned();
        let credit = self.credit.clone();
        let this = self.clone();
        glib::spawn_future_local(async move {
            let resolved = gio::spawn_blocking(move || credit(&artist))
                .await
                .ok()
                .flatten();
            let Some(tile) = LazyReleaseCover::from_widget(&root) else {
                return;
            };
            if tile.portrait_key() == portrait_key && tile.portrait_is_requested() {
                this.show_path(&tile, &portrait_key, &path, resolved);
            }
        });
    }

    fn show_path(
        self: &Rc<Self>,
        tile: &LazyReleaseCover,
        portrait_key: &str,
        path: &Path,
        credit: Option<PortraitCredit>,
    ) {
        let loader = self.loader.borrow().clone();
        let Some(loader) = loader else {
            return;
        };
        let sink = gtk4::Picture::new();
        let sink_for_result = sink.clone();
        let root = tile.widget().clone();
        let portrait_key = portrait_key.to_owned();

        // CoverLoader requires a generation pair, but recycled-cell
        // correctness comes from the portrait_key/artist_key comparisons
        // around every async boundary. This shared counter is deliberately
        // never invalidated; it only satisfies the loader's signature.
        let token = self.generation.get();
        loader.load_image_into_picture(
            &sink,
            path,
            ThumbnailSize::Portrait,
            token,
            &self.generation,
            move |loaded| {
                if !loaded {
                    return;
                }
                let Some(tile) = LazyReleaseCover::from_widget(&root) else {
                    return;
                };
                if tile.portrait_key() == portrait_key
                    && tile.portrait_is_requested()
                    && !tile.has_image()
                {
                    tile.show_paintable(sink_for_result.paintable().as_ref());
                    apply_credit(tile.widget(), credit.as_ref());
                }
            },
        );
    }
}

const CREDIT_URL_CLASS: &str = "reprise-artist-portrait-credit-url";
const CREDIT_ACTION_GROUP: &str = "artist-portrait-credit";
const CREDIT_ACTION_OPEN: &str = "open-source";

pub(in crate::ui) fn apply_credit(root: &gtk4::Overlay, credit: Option<&PortraitCredit>) {
    let state = credit_state(root);
    match credit {
        Some(credit) => {
            let text = crate::ui::strings::artist_portrait_credit(&credit.author, &credit.license);
            root.set_tooltip_text(Some(&text));
            root.update_property(&[gtk4::accessible::Property::Description(&text)]);
            state.set_text(&credit.file_page_url);
        }
        None => {
            root.set_tooltip_text(None);
            root.reset_property(gtk4::AccessibleProperty::Description);
            state.set_text("");
        }
    }
}

pub(in crate::ui) fn load_credit(
    artist: String,
    resolver: PortraitCreditResolver,
    complete: impl FnOnce(Option<PortraitCredit>) + 'static,
) {
    glib::spawn_future_local(async move {
        let credit = gio::spawn_blocking(move || resolver(&artist))
            .await
            .ok()
            .flatten();
        complete(credit);
    });
}

fn credit_state(root: &gtk4::Overlay) -> gtk4::Label {
    let mut child = root.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.has_css_class(CREDIT_URL_CLASS) {
            if let Ok(label) = widget.downcast::<gtk4::Label>() {
                return label;
            }
        }
    }

    let label = gtk4::Label::new(None);
    label.set_visible(false);
    label.set_can_target(false);
    label.add_css_class(CREDIT_URL_CLASS);
    root.add_overlay(&label);
    wire_credit_menu(root);
    label
}

fn wire_credit_menu(root: &gtk4::Overlay) {
    root.set_accessible_role(gtk4::AccessibleRole::Group);
    root.update_property(&[gtk4::accessible::Property::KeyShortcuts("Menu Shift+F10")]);

    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    let pointer_root = root.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        show_credit_menu(&pointer_root, x as i32, y as i32);
    });
    root.add_controller(gesture);

    let keys = crate::ui::source_context_surface::context_keys();
    let key_root = root.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !crate::ui::source_context_surface::is_context_menu_shortcut(key, modifiers) {
            return glib::Propagation::Proceed;
        }
        show_credit_menu(&key_root, key_root.width() / 2, key_root.height() / 2);
        glib::Propagation::Stop
    });
    root.add_controller(keys);
}

fn credit_menu(root: &gtk4::Overlay) -> Option<gio::Menu> {
    validated_https_url(&credit_state(root).text()).map(|_| {
        let menu = gio::Menu::new();
        menu.append(
            Some(&crate::ui::strings::text(
                crate::ui::strings::ARTIST_PORTRAIT_OPEN_SOURCE,
            )),
            Some(&format!("{CREDIT_ACTION_GROUP}.{CREDIT_ACTION_OPEN}")),
        );
        menu
    })
}

#[cfg(test)]
fn credit_description(root: &gtk4::Overlay) -> Option<String> {
    validated_https_url(&credit_state(root).text())
        .and_then(|_| root.tooltip_text().map(Into::into))
}

fn show_credit_menu(root: &gtk4::Overlay, x: i32, y: i32) {
    let Some(url) = validated_https_url(&credit_state(root).text()) else {
        return;
    };
    let Some(menu) = credit_menu(root) else {
        return;
    };
    let actions = gio::SimpleActionGroup::new();
    let open = gio::SimpleAction::new(CREDIT_ACTION_OPEN, None);
    open.connect_activate(move |_, _| open_credit_url(&url));
    actions.add_action(&open);
    root.insert_action_group(CREDIT_ACTION_GROUP, Some(&actions));

    let popover = crate::ui::menu_a11y::popover_menu_from_model(&menu);
    popover.set_parent(root);
    popover.set_has_arrow(false);
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x, y, 1, 1)));
    crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    popover.popup();
}

fn validated_https_url(value: &str) -> Option<String> {
    let value = value.trim();
    let uri = glib::Uri::parse(value, glib::UriFlags::NONE).ok()?;
    (uri.scheme().as_str() == "https" && uri.host().is_some_and(|host| !host.is_empty()))
        .then(|| value.to_owned())
}

fn open_credit_url(value: &str) {
    let Some(url) = validated_https_url(value) else {
        tracing::warn!(
            url = value,
            "refused to open a non-HTTPS portrait credit URL"
        );
        return;
    };
    let url_for_log = url.clone();
    gtk4::UriLauncher::new(&url).launch(
        None::<&gtk4::Window>,
        gio::Cancellable::NONE,
        move |result| {
            if let Err(error) = result {
                tracing::warn!(%error, url = url_for_log, "could not open portrait credit URL");
            }
        },
    );
}

#[cfg(test)]
mod credit_tests {
    use super::*;
    use reprise_core::artist_portrait::PortraitCredit;

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xA8,
        0xAF, 0xAF, 0x07, 0x00, 0x02, 0xFE, 0x01, 0x7E, 0xBA, 0x25, 0x70, 0x25, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn commons_credit() -> PortraitCredit {
        PortraitCredit {
            author: "<Photographer & editor>".into(),
            license: "CC BY-SA 4.0".into(),
            license_url: "https://creativecommons.org/licenses/by-sa/4.0/".into(),
            file_page_url: "https://commons.wikimedia.org/wiki/File:Portrait.jpg".into(),
        }
    }

    fn menu_label(menu: &gio::Menu) -> Option<String> {
        menu.item_attribute_value(0, "label", Some(glib::VariantTy::STRING))
            .and_then(|value| value.get::<String>())
    }

    #[test]
    fn portrait_credit_links_require_https_at_the_ui_boundary() {
        assert_eq!(
            validated_https_url("https://commons.wikimedia.org/wiki/File:Portrait.jpg").as_deref(),
            Some("https://commons.wikimedia.org/wiki/File:Portrait.jpg")
        );
        assert!(
            validated_https_url("http://commons.wikimedia.org/wiki/File:Portrait.jpg").is_none()
        );
        assert!(validated_https_url("javascript:alert(1)").is_none());
        assert!(validated_https_url("https://").is_none());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn art_5_a_commons_portrait_credits_the_releases_tile() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let portrait = cache.path().join("portrait.png");
        std::fs::write(&portrait, TINY_PNG).unwrap();
        let expected_credit = commons_credit();
        let tiles = ArtistPortraitTiles::for_test_with_credit(
            {
                let portrait = portrait.clone();
                move |_| Some(portrait.clone())
            },
            {
                let expected_credit = expected_credit.clone();
                move |_| Some(expected_credit.clone())
            },
        );
        tiles.set_sources(
            crate::ui::cover_loader::CoverLoader::new(
                crate::ui::cover_download_worker::setup_for_test(),
            ),
            ArtistPortraitRuntime::for_test(false, |_| None),
        );
        let tile = LazyReleaseCover::new("", "Lorna Shore", 160);

        tiles.show(&tile);

        assert!(crate::ui::test_settle::settle_until(
            crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
            || tile.shows_image() && tile.widget().tooltip_text().is_some(),
        ));

        let expected = "Photo: <Photographer & editor> · CC BY-SA 4.0 · Wikimedia Commons";
        assert_eq!(tile.widget().tooltip_text().as_deref(), Some(expected));
        assert_eq!(credit_description(tile.widget()).as_deref(), Some(expected));
        assert_eq!(
            credit_menu(tile.widget()).as_ref().and_then(menu_label),
            Some("Open image source".into())
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn art_5_a_commons_portrait_credits_the_stats_card() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let card = crate::ui::stats::stats_band_card::StatsBandCard::new();

        apply_credit(card.widget(), Some(&commons_credit()));

        assert!(card
            .widget()
            .tooltip_text()
            .is_some_and(|text| text.contains("<Photographer & editor>")));
        assert!(credit_description(card.widget()).is_some());
        assert_eq!(
            credit_menu(card.widget()).as_ref().and_then(menu_label),
            Some("Open image source".into())
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn art_5_a_deezer_portrait_has_no_credit_on_either_surface() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let tile = LazyReleaseCover::new("", "Lorna Shore", 160);
        let card = crate::ui::stats::stats_band_card::StatsBandCard::new();
        apply_credit(tile.widget(), Some(&commons_credit()));
        apply_credit(card.widget(), Some(&commons_credit()));

        apply_credit(tile.widget(), None);
        apply_credit(card.widget(), None);

        for surface in [tile.widget(), card.widget()] {
            assert!(surface.tooltip_text().is_none());
            assert!(credit_description(surface).is_none());
            assert!(credit_menu(surface).is_none());
        }
    }
}
