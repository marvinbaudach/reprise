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
use reprise_core::cover::{thumbnail, ThumbnailSize};

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

/// The recycling guard a load runs under: the token this request was issued
/// with, plus the counter saying which token is still the current one. A row
/// that scrolled away — or a bar whose track moved on — bumps the counter, and
/// every older request in flight goes quiet instead of writing a stale cover.
///
/// The two always travel together, so they travel as one.
#[derive(Clone, Copy)]
struct RequestGuard<'a> {
    token: u64,
    current: &'a Rc<Cell<u64>>,
}

/// What a target shows between the request and its answer.
///
/// The two callers want opposite things, and neither is a safe default for
/// the other.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WhileResolving {
    /// A recycled list row starts out carrying the *previous row's* artwork —
    /// a different track's cover, not a stale version of this one. It must go
    /// at once, before anything is known.
    ShowPlaceholder,
    /// A now-playing surface shows one track at a time, and the next track is
    /// usually from the same album: the cache is keyed by track path, so the
    /// second track of an album misses even though it resolves to the very
    /// same file. Blanking up front turns that into a placeholder flash
    /// between two identical covers. The placeholder still goes up the moment
    /// the *local* answer comes back empty — before the network is asked,
    /// which may take seconds or never answer — so a track without artwork
    /// never keeps its predecessor's cover on screen.
    KeepPreviousCover,
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
        self.load_target(
            image,
            track_path,
            size,
            RequestGuard { token, current },
            WhileResolving::ShowPlaceholder,
            |_| {},
        );
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
        self.load_target(
            picture,
            track_path,
            size,
            RequestGuard { token, current },
            WhileResolving::ShowPlaceholder,
            move |path| on_loaded(path.is_some()),
        );
    }

    pub fn load_into_track_cover(
        self: &Rc<Self>,
        cover: &TrackCover,
        track_path: &str,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
    ) {
        self.load_target(
            cover,
            track_path,
            size,
            RequestGuard { token, current },
            WhileResolving::ShowPlaceholder,
            |_| {},
        );
    }

    /// Loads the cover of the track a now-playing surface currently shows —
    /// the player bar, the mini-player, the Now Playing panel — and reports
    /// both successful and empty resolutions while `token` is current.
    ///
    /// These surfaces keep the cover they have until the next one is known;
    /// see `WhileResolving::KeepPreviousCover` for why they, and only they,
    /// get that treatment.
    pub fn load_into_now_playing(
        self: &Rc<Self>,
        image: &gtk4::Image,
        track_path: &str,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
        on_resolved: impl Fn(Option<PathBuf>) + 'static,
    ) {
        self.load_target(
            image,
            track_path,
            size,
            RequestGuard { token, current },
            WhileResolving::KeepPreviousCover,
            on_resolved,
        );
    }

    fn load_target<T: CoverTarget>(
        self: &Rc<Self>,
        target: &T,
        track_path: &str,
        size: ThumbnailSize,
        guard: RequestGuard<'_>,
        while_resolving: WhileResolving,
        on_resolved: impl Fn(Option<PathBuf>) + 'static,
    ) {
        let RequestGuard { token, current } = guard;
        if current.get() != token {
            return;
        }
        let key = format!("{track_path}|{}", size.pixels());
        if let Some(cached) = self.cache_get(&key) {
            target.show_texture(&cached.texture);
            on_resolved(Some(cached.path));
            return;
        }
        if while_resolving == WhileResolving::ShowPlaceholder {
            target.show_placeholder();
        }

        let this = self.clone();
        let target = target.clone();
        let current = current.clone();
        let path_owned = track_path.to_string();
        glib::spawn_future_local(async move {
            // Off the main loop: resolve source + build/hit the disk cache.
            let path_for_worker = path_owned.clone();
            let mut cache_path: Option<std::path::PathBuf> = gio::spawn_blocking(move || {
                // Asks what this track resolved to last time before opening
                // it. The thumbnail cache alone cannot save the read: its key
                // is a hash of the cover bytes, so it can only be consulted
                // once the file has already been read.
                reprise_core::cover::thumbnail_for_track(
                    std::path::Path::new(&path_for_worker),
                    size,
                )
            })
            .await
            .ok()
            .flatten();

            // Back on the main loop: bail if this cell was recycled meanwhile.
            if current.get() != token {
                return;
            }
            if cache_path.is_none() {
                // The local answer is in, and it is empty. Whatever a
                // now-playing surface still shows belongs to the previous
                // track and is wrong from here on — so the placeholder goes up
                // now, ahead of the network question below, which may take
                // seconds or never answer. A list row is already showing it.
                if while_resolving == WhileResolving::KeepPreviousCover {
                    target.show_placeholder();
                }
                // Asking the download worker means it reads the file's tags to
                // work out which album to ask about. If it already came back
                // empty for this track, and nothing has changed since, that
                // read would only arrive at the same answer.
                let known_empty = {
                    let path = path_owned.clone();
                    gio::spawn_blocking(move || {
                        reprise_core::cover::download_marked_unavailable(
                            std::path::Path::new(&path),
                            size,
                        )
                    })
                    .await
                    .unwrap_or(false)
                };
                if known_empty {
                    if current.get() == token {
                        on_resolved(None);
                    }
                    return;
                }
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
                    if matches!(result, Ok(DownloadOutcome::Unavailable)) {
                        let path = path_owned.clone();
                        gio::spawn_blocking(move || {
                            reprise_core::cover::remember_download_unavailable(
                                std::path::Path::new(&path),
                                size,
                            );
                        })
                        .await
                        .ok();
                    }
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
                    // A resolved path that will not decode leaves the same
                    // hole as no path at all, and this is the one route to it
                    // that never passed the empty-resolution branch above.
                    if while_resolving == WhileResolving::KeepPreviousCover {
                        target.show_placeholder();
                    }
                    on_resolved(None);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controlled_loader() -> (Rc<CoverLoader>, async_channel::Receiver<DownloadRequest>) {
        let (worker, requests) = async_channel::unbounded();
        let loader = CoverLoader::new(CoverDownloadRuntime {
            enabled: Rc::new(Cell::new(true)),
            worker,
        });
        (loader, requests)
    }

    fn pump_until(condition: impl Fn() -> bool) {
        let context = glib::MainContext::default();
        for _ in 0..10_000 {
            if condition() {
                return;
            }
            while context.pending() {
                context.iteration(false);
            }
            std::thread::yield_now();
        }
    }

    /// A now-playing surface shows one track at a time, so its cover may only
    /// be taken down once something is known about the next one. Blanking it
    /// up front makes every track change inside one album flash the
    /// placeholder on the way to the same artwork — the in-memory cache is
    /// keyed by track path, so the second track of an album is a miss even
    /// though it resolves to the identical file.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn a_now_playing_cover_survives_until_the_next_track_is_resolved() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let (loader, _requests) = controlled_loader();
        let image = gtk4::Image::new();
        let current = Rc::new(Cell::new(1));

        loader.load_into_now_playing(
            &image,
            "/missing/now-playing-cover-test.flac",
            ThumbnailSize::Bar,
            1,
            &current,
            |_| {},
        );
        assert!(
            image.icon_name().is_none(),
            "the cover was blanked before the new track had resolved to anything"
        );

        // The local answer is what decides. Once it comes back empty, whatever
        // the surface still shows is wrong and the placeholder goes up — before
        // the network is asked, which may take seconds or never answer.
        pump_until(|| image.icon_name().is_some());
        assert_eq!(image.icon_name().as_deref(), Some(PLACEHOLDER_ICON));
    }

    /// The opposite discipline, and the reason the policy above cannot simply
    /// be the loader's default: a recycled list row starts out carrying the
    /// previous row's cover, which is a different track's artwork, not a
    /// slightly stale version of the same one.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn a_recycled_list_row_drops_the_previous_cover_immediately() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let (loader, _requests) = controlled_loader();
        let image = gtk4::Image::new();
        let current = Rc::new(Cell::new(1));

        loader.load_into(
            &image,
            "/missing/list-row-cover-test.flac",
            ThumbnailSize::List,
            1,
            &current,
        );
        assert_eq!(image.icon_name().as_deref(), Some(PLACEHOLDER_ICON));
    }

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
