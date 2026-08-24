//! Cache-first artist portraits for recycled placeholder tiles.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use reprise_core::cover::ThumbnailSize;

use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::updates::release_cover::LazyReleaseCover;

pub(in crate::ui) type CachedPortraitResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
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
    generation: Rc<Cell<u64>>,
}

impl ArtistPortraitTiles {
    #[cfg(not(test))]
    pub(in crate::ui) fn new() -> Rc<Self> {
        Self::with_resolver(|artist| match PRODUCTION_CACHE_ONLY_RESOLVER(artist) {
            reprise_core::artist_portrait::PortraitOutcome::Found(path) => Some(path),
            reprise_core::artist_portrait::PortraitOutcome::NotFound => None,
        })
    }

    #[cfg(test)]
    pub(in crate::ui) fn for_test(
        cached: impl Fn(&str) -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Rc<Self> {
        Self::with_resolver(cached)
    }

    fn with_resolver(cached: impl Fn(&str) -> Option<PathBuf> + Send + Sync + 'static) -> Rc<Self> {
        Rc::new(Self {
            portrait: RefCell::new(None),
            loader: RefCell::new(None),
            cached: Arc::new(cached),
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
        if artist.trim().is_empty() || tile.portrait_started() == portrait_key {
            return;
        }
        tile.mark_portrait_started(&portrait_key);
        let root = tile.widget().clone();
        let this = self.clone();
        glib::spawn_future_local(async move {
            let lookup_artist = artist.clone();
            let cached = this.cached.clone();
            let found = gio::spawn_blocking(move || cached(&lookup_artist))
                .await
                .ok()
                .flatten();
            let Some(tile) = LazyReleaseCover::from_widget(&root) else {
                return;
            };
            if tile.portrait_key() != portrait_key || !tile.portrait_is_requested() {
                return;
            }
            if let Some(path) = found {
                this.show_path(&tile, &portrait_key, &path);
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
                    this.show_path(&tile, &result_key, &path);
                }
            },
        );
    }

    fn show_path(self: &Rc<Self>, tile: &LazyReleaseCover, portrait_key: &str, path: &Path) {
        let loader = self.loader.borrow().clone();
        let Some(loader) = loader else {
            return;
        };
        let sink = gtk4::Picture::new();
        let sink_for_result = sink.clone();
        let root = tile.widget().clone();
        let portrait_key = portrait_key.to_owned();

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
                }
            },
        );
    }
}
