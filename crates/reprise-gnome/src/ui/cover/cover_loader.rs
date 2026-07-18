//! Lazy, off-thread cover loading for GTK widgets. Decode/cache work runs on a
//! `gio` worker thread (never the main loop); the resulting `gdk::Texture` is
//! applied back on the main context, guarded by a per-widget generation token
//! so a recycled track-list row never shows a stale cover.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use reprise_core::cover::{resolve_source, thumbnail, CoverSource, ThumbnailSize};

use crate::ui::cover_download_worker::{CoverDownloadRuntime, DownloadOutcome, DownloadRequest};
use crate::ui::track_cover::TrackCover;

/// Symbolic placeholder shown when a track has no cover (or while loading /
/// on error). No decode — just an icon name GTK already ships.
const PLACEHOLDER_ICON: &str = "audio-x-generic-symbolic";

/// Cap on the in-memory texture cache. Thumbnails are tiny; this only spares
/// re-reading the on-disk PNG during scrolling. Evicts oldest-inserted first.
const MAX_CACHED_TEXTURES: usize = 256;

#[derive(Clone)]
struct CachedCover {
    texture: gdk::Texture,
    path: PathBuf,
}

pub struct CoverLoader {
    cache: RefCell<HashMap<String, CachedCover>>,
    order: RefCell<std::collections::VecDeque<String>>,
    download: CoverDownloadRuntime,
}

trait CoverTarget: Clone + 'static {
    fn show_placeholder(&self);
    fn show_texture(&self, texture: &gdk::Texture);
}

impl CoverTarget for gtk4::Image {
    fn show_placeholder(&self) {
        CoverLoader::set_placeholder(self);
    }

    fn show_texture(&self, texture: &gdk::Texture) {
        self.set_paintable(Some(texture));
    }
}

impl CoverTarget for TrackCover {
    fn show_placeholder(&self) {
        self.set_placeholder();
    }

    fn show_texture(&self, texture: &gdk::Texture) {
        self.set_paintable(Some(texture));
    }
}

impl CoverTarget for gtk4::Picture {
    fn show_placeholder(&self) {
        self.set_paintable(gdk::Paintable::NONE);
        self.set_visible(false);
    }

    fn show_texture(&self, texture: &gdk::Texture) {
        self.set_paintable(Some(texture));
        self.set_visible(true);
    }
}

impl CoverLoader {
    pub fn new(runtime: CoverDownloadRuntime) -> Rc<Self> {
        Rc::new(Self {
            cache: RefCell::new(HashMap::new()),
            order: RefCell::new(std::collections::VecDeque::new()),
            download: runtime,
        })
    }

    pub fn set_placeholder(image: &gtk4::Image) {
        image.set_icon_name(Some(PLACEHOLDER_ICON));
    }

    fn cache_get(&self, key: &str) -> Option<CachedCover> {
        self.cache.borrow().get(key).cloned()
    }

    fn cache_put(&self, key: String, cover: CachedCover) {
        let mut cache = self.cache.borrow_mut();
        if cache.contains_key(&key) {
            return;
        }
        let mut order = self.order.borrow_mut();
        if cache.len() >= MAX_CACHED_TEXTURES {
            if let Some(old) = order.pop_front() {
                cache.remove(&old);
            }
        }
        order.push_back(key.clone());
        cache.insert(key, cover);
    }

    pub fn invalidate_paths(&self, paths: &[std::path::PathBuf]) {
        let prefixes: Vec<String> = paths
            .iter()
            .map(|path| format!("{}|", path.to_string_lossy()))
            .collect();
        self.cache
            .borrow_mut()
            .retain(|key, _| !prefixes.iter().any(|prefix| key.starts_with(prefix)));
        self.order
            .borrow_mut()
            .retain(|key| !prefixes.iter().any(|prefix| key.starts_with(prefix)));
    }

    pub fn load_into(
        self: &Rc<Self>,
        image: &gtk4::Image,
        track_path: &str,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
    ) {
        self.load_target(image, track_path, size, token, current, |_| {});
    }

    pub fn load_into_track_cover(
        self: &Rc<Self>,
        cover: &TrackCover,
        track_path: &str,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
    ) {
        self.load_target(cover, track_path, size, token, current, |_| {});
    }

    /// Loads a cover like [`Self::load_into`] and reports the exact cached
    /// image path after a successful decode. MPRIS uses that path for
    /// `mpris:artUrl`; reporting it from this existing pipeline avoids any
    /// synchronous tag/image work on the GTK main loop.
    pub fn load_into_with_path(
        self: &Rc<Self>,
        image: &gtk4::Image,
        track_path: &str,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
        on_loaded: impl Fn(PathBuf) + 'static,
    ) {
        self.load_target(image, track_path, size, token, current, on_loaded);
    }

    /// Loads an arbitrary image file into a `Picture` at a cached thumbnail
    /// size. Both thumbnail generation and `gdk::Texture` decode run on a
    /// worker thread; only the generation-guarded widget update runs on GTK's
    /// main context.
    pub fn load_file_into_picture(
        self: &Rc<Self>,
        picture: &gtk4::Picture,
        image_path: &std::path::Path,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
    ) {
        let key = format!("file:{}|{}", image_path.to_string_lossy(), size.pixels());
        if let Some(cached) = self.cache_get(&key) {
            picture.show_texture(&cached.texture);
            return;
        }
        picture.show_placeholder();

        let this = self.clone();
        let picture = picture.clone();
        let current = current.clone();
        let image_path = image_path.to_owned();
        glib::spawn_future_local(async move {
            let decoded = gio::spawn_blocking(move || {
                let cache_path = thumbnail(&CoverSource::FolderImage(image_path), size).ok()?;
                let texture = gdk::Texture::from_filename(&cache_path).ok()?;
                Some((texture, cache_path))
            })
            .await
            .ok()
            .flatten();

            if current.get() != token {
                return;
            }
            let Some((texture, cache_path)) = decoded else {
                return;
            };
            this.cache_put(
                key,
                CachedCover {
                    texture: texture.clone(),
                    path: cache_path,
                },
            );
            picture.show_texture(&texture);
        });
    }

    fn load_target<T: CoverTarget>(
        self: &Rc<Self>,
        target: &T,
        track_path: &str,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
        on_loaded: impl Fn(PathBuf) + 'static,
    ) {
        let key = format!("{track_path}|{}", size.pixels());
        if let Some(cached) = self.cache_get(&key) {
            target.show_texture(&cached.texture);
            on_loaded(cached.path);
            return;
        }
        target.show_placeholder();

        let this = self.clone();
        let target = target.clone();
        let current = current.clone();
        let path_owned = track_path.to_string();
        glib::spawn_future_local(async move {
            // Off the main loop: resolve source + build/hit the disk cache.
            let path_for_worker = path_owned.clone();
            let mut cache_path: Option<std::path::PathBuf> = gio::spawn_blocking(move || {
                let source = resolve_source(std::path::Path::new(&path_for_worker))?;
                thumbnail(&source, size).ok()
            })
            .await
            .ok()
            .flatten();

            // Back on the main loop: bail if this cell was recycled meanwhile.
            if current.get() != token {
                return;
            }
            if cache_path.is_none() {
                let (response, result) = async_channel::bounded(1);
                if !this.download.try_request(DownloadRequest {
                    track_path: path_owned.clone(),
                    skip_if_covered: false,
                    response,
                }) {
                    return;
                }
                let Ok(DownloadOutcome::Downloaded(downloaded_path)) = result.recv().await else {
                    return;
                };
                if current.get() != token {
                    return;
                }
                cache_path = gio::spawn_blocking(move || {
                    thumbnail(
                        &reprise_core::cover::CoverSource::FolderImage(downloaded_path),
                        size,
                    )
                    .ok()
                })
                .await
                .ok()
                .flatten();
            }

            // Re-check after a possible network request + thumbnail build.
            if current.get() != token {
                return;
            }
            let Some(cache_path) = cache_path else {
                return;
            };
            match gdk::Texture::from_filename(&cache_path) {
                Ok(texture) => {
                    this.cache_put(
                        key,
                        CachedCover {
                            texture: texture.clone(),
                            path: cache_path.clone(),
                        },
                    );
                    target.show_texture(&texture);
                    on_loaded(cache_path);
                }
                Err(error) => {
                    tracing::debug!(%error, path = %path_owned, "cover texture load failed");
                }
            }
        });
    }
}
