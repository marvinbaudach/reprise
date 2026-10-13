//! Cache-first artist portraits for recycled placeholder tiles.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use reprise_core::artist_portrait::PortraitCredit;
use reprise_core::cover::ThumbnailSize;

use crate::ui::artist_portrait_credit::{apply_credit, CreditLinks};
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
    links: CreditLinks,
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
            links: CreditLinks::default(),
            generation: Rc::new(Cell::new(0)),
        })
    }

    /// The file-page links of the credited portraits this set of tiles has
    /// shown, for menus that live outside the tile.
    pub(in crate::ui) fn credit_links(&self) -> CreditLinks {
        self.links.clone()
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
        if artist.trim().is_empty() || tile.portrait_started() == portrait_key {
            return;
        }
        // A new portrait key means a rebound cell: drop the previous credit
        // before anything async can land. A remap with the same key returned
        // above, so it keeps what the tile already shows.
        apply_credit(tile.widget(), None);
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
        let links = self.links.clone();
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
                    links.remember(&tile.artist_key(), credit.as_ref());
                }
            },
        );
    }
}
