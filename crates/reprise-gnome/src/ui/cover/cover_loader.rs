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
use reprise_core::cover::{resolve_source, thumbnail, ThumbnailSize};

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

impl CoverTarget for gtk4::Picture {
    fn show_placeholder(&self) {
        self.set_paintable(gtk4::gdk::Paintable::NONE);
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

    /// Loads into a picture and calls `on_loaded` only while `token` is still
    /// current. Once stale, a request neither changes the target nor runs the
    /// callback.
    pub fn load_into_picture(
        self: &Rc<Self>,
        picture: &gtk4::Picture,
        track_path: &str,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
        on_loaded: impl Fn(bool) + 'static,
    ) {
        self.load_target(picture, track_path, size, token, current, move |path| {
            on_loaded(path.is_some());
        });
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
        self.load_target(image, track_path, size, token, current, move |path| {
            if let Some(path) = path {
                on_loaded(path);
            }
        });
    }

    fn load_target<T: CoverTarget>(
        self: &Rc<Self>,
        target: &T,
        track_path: &str,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
        on_resolved: impl Fn(Option<PathBuf>) + 'static,
    ) {
        if current.get() != token {
            return;
        }
        let key = format!("{track_path}|{}", size.pixels());
        if let Some(cached) = self.cache_get(&key) {
            target.show_texture(&cached.texture);
            on_resolved(Some(cached.path));
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
                    if current.get() == token {
                        on_resolved(None);
                    }
                    return;
                }
                let result = result.recv().await;
                if current.get() != token {
                    return;
                }
                let Ok(DownloadOutcome::Downloaded(downloaded_path)) = result else {
                    on_resolved(None);
                    return;
                };
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
                on_resolved(None);
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
                    on_resolved(Some(cache_path));
                }
                Err(error) => {
                    tracing::debug!(%error, path = %path_owned, "cover texture load failed");
                    on_resolved(None);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stale_picture_load_does_not_run_completion_callback() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let (worker, requests) = async_channel::unbounded();
        let loader = CoverLoader::new(CoverDownloadRuntime {
            enabled: Rc::new(Cell::new(true)),
            worker,
        });
        let picture = gtk4::Picture::new();
        let current = Rc::new(Cell::new(1));
        let completed = Rc::new(Cell::new(false));
        loader.load_into_picture(
            &picture,
            "/missing/stale-cover-test.flac",
            ThumbnailSize::Portrait,
            1,
            &current,
            {
                let completed = completed.clone();
                move |_| completed.set(true)
            },
        );

        let context = glib::MainContext::default();
        let request = (0..10_000).find_map(|_| {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::yield_now();
            requests.try_recv().ok()
        });
        let request = request.expect("cover request should reach the controlled worker");
        current.set(2);
        request
            .response
            .try_send(DownloadOutcome::Unavailable)
            .unwrap();
        for _ in 0..100 {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::yield_now();
        }

        assert!(!completed.get());
    }
}
