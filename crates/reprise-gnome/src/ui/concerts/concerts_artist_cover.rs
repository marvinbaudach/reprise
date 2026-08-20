//! Lazy artist portraits for recycled Concerts table cells.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use reprise_core::cover::ThumbnailSize;

use super::concerts_model::ConcertObject;
use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::table_column_widths as widths;
use crate::ui::updates::release_cover::LazyReleaseCover;

type CachedPortraitResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

pub(super) struct ConcertsArtistImage {
    portrait: RefCell<Option<Rc<ArtistPortraitRuntime>>>,
    loader: RefCell<Option<Rc<CoverLoader>>>,
    cached: CachedPortraitResolver,
    generation: Rc<Cell<u64>>,
}

impl ConcertsArtistImage {
    #[cfg(not(test))]
    pub(super) fn new() -> Rc<Self> {
        Self::with_resolver(
            |artist| match reprise_core::artist_portrait::load_cached(artist) {
                reprise_core::artist_portrait::PortraitOutcome::Found(path) => Some(path),
                reprise_core::artist_portrait::PortraitOutcome::NotFound => None,
            },
        )
    }

    #[cfg(test)]
    pub(super) fn for_test(
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

    pub(super) fn set_sources(&self, loader: Rc<CoverLoader>, portrait: Rc<ArtistPortraitRuntime>) {
        *self.loader.borrow_mut() = Some(loader);
        *self.portrait.borrow_mut() = Some(portrait);
    }

    pub(super) fn show(self: &Rc<Self>, tile: &LazyReleaseCover) {
        let artist = tile.artist_key();
        if artist.trim().is_empty() {
            return;
        }
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
            if tile.artist_key() != artist {
                return;
            }
            if let Some(path) = found {
                this.show_path(&tile, &artist, &path);
            } else if root.is_mapped() {
                this.start(&tile);
            }
        });
    }

    pub(super) fn start(self: &Rc<Self>, tile: &LazyReleaseCover) {
        let artist = tile.artist_key();
        if artist.trim().is_empty() || tile.started() == artist {
            return;
        }
        let runtime = self.portrait.borrow().clone();
        let Some(runtime) = runtime else {
            return;
        };
        if !runtime.request_would_run(&artist) {
            return;
        }
        tile.mark_started(&artist);

        let guard_root = tile.widget().clone();
        let guard_artist = artist.clone();
        let result_root = tile.widget().clone();
        let result_artist = artist.clone();
        let this = self.clone();
        runtime.request_while(
            artist,
            move || {
                LazyReleaseCover::from_widget(&guard_root)
                    .is_some_and(|tile| tile.artist_key() == guard_artist)
            },
            move |found| {
                let Some(path) = found else {
                    return;
                };
                let Some(tile) = LazyReleaseCover::from_widget(&result_root) else {
                    return;
                };
                if tile.artist_key() == result_artist {
                    this.show_path(&tile, &result_artist, &path);
                }
            },
        );
    }

    fn show_path(self: &Rc<Self>, tile: &LazyReleaseCover, artist: &str, path: &Path) {
        let loader = self.loader.borrow().clone();
        let Some(loader) = loader else {
            return;
        };
        let sink = gtk4::Picture::new();
        let sink_for_result = sink.clone();
        let root = tile.widget().clone();
        let artist = artist.to_owned();

        // CoverLoader requires a generation pair, but concert-cell correctness
        // comes from the reconstructible artist-key checks around every async
        // boundary. This column-wide counter is therefore intentionally never
        // invalidated; it only satisfies the loader's decode/cache signature.
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
                if tile.artist_key() == artist {
                    tile.show_paintable(sink_for_result.paintable().as_ref());
                }
            },
        );
    }
}

pub(super) fn cover_column(view: &gtk4::ColumnView, image: &Rc<ConcertsArtistImage>) {
    let factory = gtk4::SignalListItemFactory::new();
    let setup_image = image.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let tile = LazyReleaseCover::new_unbound(widths::COVER);
        let image = setup_image.clone();
        tile.widget().connect_map(move |root| {
            if let Some(tile) = LazyReleaseCover::from_widget(root) {
                image.start(&tile);
            }
        });
        item.set_child(Some(tile.widget()));
    });
    let bind_image = image.clone();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(root) = item.child().and_downcast::<gtk4::Overlay>() else {
            return;
        };
        let Some(tile) = LazyReleaseCover::from_widget(&root) else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ConcertObject>() else {
            return;
        };
        tile.set_artist_key(&object.row().artist_name);
        bind_image.show(&tile);
        bind_image.start(&tile);
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(root) = item.child().and_downcast::<gtk4::Overlay>() else {
            return;
        };
        if let Some(tile) = LazyReleaseCover::from_widget(&root) {
            tile.set_artist_key("");
        }
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(crate::ui::strings::text(crate::ui::strings::COLUMN_COVER))
        .factory(&factory)
        .resizable(false)
        .build();
    widths::pin(&column, widths::COVER);
    view.append_column(&column);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_construction_uses_only_the_injected_cache_resolver() {
        let image = ConcertsArtistImage::for_test(|artist| {
            assert_eq!(artist, "Falling Leaves");
            Some(PathBuf::from("/isolated/portrait.png"))
        });

        assert_eq!(
            (image.cached)("Falling Leaves"),
            Some(PathBuf::from("/isolated/portrait.png"))
        );
        assert!(image.loader.borrow().is_none());
        assert!(image.portrait.borrow().is_none());
    }
}
