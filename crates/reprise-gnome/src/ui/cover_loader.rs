//! Lazy, off-thread cover loading for GTK widgets. Decode/cache work runs on a
//! `gio` worker thread (never the main loop); the resulting `gdk::Texture` is
//! applied back on the main context, guarded by a per-widget generation token
//! so a recycled track-list row never shows a stale cover.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;

use reprise_core::cover::{resolve_source, thumbnail, ThumbnailSize};

/// Symbolic placeholder shown when a track has no cover (or while loading /
/// on error). No decode — just an icon name GTK already ships.
const PLACEHOLDER_ICON: &str = "audio-x-generic-symbolic";

/// Cap on the in-memory texture cache. Thumbnails are tiny; this only spares
/// re-reading the on-disk PNG during scrolling. Evicts oldest-inserted first.
const MAX_CACHED_TEXTURES: usize = 256;

pub struct CoverLoader {
    cache: RefCell<HashMap<String, gdk::Texture>>,
    order: RefCell<std::collections::VecDeque<String>>,
}

impl CoverLoader {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            cache: RefCell::new(HashMap::new()),
            order: RefCell::new(std::collections::VecDeque::new()),
        })
    }

    pub fn set_placeholder(image: &gtk4::Image) {
        image.set_icon_name(Some(PLACEHOLDER_ICON));
    }

    fn cache_get(&self, key: &str) -> Option<gdk::Texture> {
        self.cache.borrow().get(key).cloned()
    }

    fn cache_put(&self, key: String, texture: gdk::Texture) {
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
        cache.insert(key, texture);
    }

    pub fn load_into(
        self: &Rc<Self>,
        image: &gtk4::Image,
        track_path: &str,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
    ) {
        let key = format!("{track_path}|{}", size.pixels());
        if let Some(texture) = self.cache_get(&key) {
            image.set_paintable(Some(&texture));
            return;
        }
        Self::set_placeholder(image);

        let this = self.clone();
        let image = image.clone();
        let current = current.clone();
        let path_owned = track_path.to_string();
        glib::spawn_future_local(async move {
            // Off the main loop: resolve source + build/hit the disk cache.
            let path_for_worker = path_owned.clone();
            let cache_path: Option<std::path::PathBuf> = gio::spawn_blocking(move || {
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
            let Some(cache_path) = cache_path else {
                return;
            };
            match gdk::Texture::from_filename(&cache_path) {
                Ok(texture) => {
                    this.cache_put(key, texture.clone());
                    image.set_paintable(Some(&texture));
                }
                Err(error) => {
                    tracing::debug!(%error, path = %path_owned, "cover texture load failed");
                }
            }
        });
    }
}
