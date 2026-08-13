//! One image chain for every band surface in My Stats (STATS-23): cached
//! portrait first, album covers while a missing portrait is fetched, and
//! initials when neither source resolves.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gtk4::{gio, glib};
use reprise_core::cover::ThumbnailSize;

use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;

type CachedPortraitResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

pub(super) struct ArtistImageRequest {
    pub artist: String,
    pub candidates: Vec<String>,
    pub size: ThumbnailSize,
    pub token: u64,
    pub generation: Rc<Cell<u64>>,
    pub on_loaded: Rc<dyn Fn(bool)>,
}

/// The next album to try after `tried` failures, or `None` when spent.
pub(super) fn next_candidate(candidates: &[String], tried: usize) -> Option<&str> {
    candidates.get(tried).map(String::as_str)
}

#[derive(Clone)]
pub(in crate::ui) struct StatsArtistImage {
    cover_loader: Rc<CoverLoader>,
    portrait: Rc<RefCell<Option<Rc<ArtistPortraitRuntime>>>>,
    cached_portrait: CachedPortraitResolver,
}

impl StatsArtistImage {
    pub(super) fn new(cover_loader: Rc<CoverLoader>) -> Rc<Self> {
        Rc::new(Self {
            cover_loader,
            portrait: Rc::new(RefCell::new(None)),
            cached_portrait: Arc::new(|artist| {
                match reprise_core::artist_portrait::load_cached(artist) {
                    reprise_core::artist_portrait::PortraitOutcome::Found(path) => Some(path),
                    reprise_core::artist_portrait::PortraitOutcome::NotFound => None,
                }
            }),
        })
    }

    #[cfg(test)]
    pub(super) fn for_test(
        cover_loader: Rc<CoverLoader>,
        cached_portrait: impl Fn(&str) -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            cover_loader,
            portrait: Rc::new(RefCell::new(None)),
            cached_portrait: Arc::new(cached_portrait),
        })
    }

    pub(super) fn set_portrait_runtime(&self, runtime: Rc<ArtistPortraitRuntime>) {
        *self.portrait.borrow_mut() = Some(runtime);
    }

    pub(super) fn load(self: &Rc<Self>, picture: &gtk4::Picture, request: ArtistImageRequest) {
        if !self.portraits_enabled() {
            self.walk_candidates(picture, &request, 0, &Rc::new(Cell::new(false)));
            return;
        }
        let this = self.clone();
        let picture = picture.clone();
        glib::spawn_future_local(async move {
            let name = request.artist.clone();
            let cached_portrait = this.cached_portrait.clone();
            let cached = gio::spawn_blocking(move || cached_portrait(&name))
                .await
                .ok()
                .flatten();
            if request.generation.get() != request.token {
                return;
            }
            if !this.portraits_enabled() {
                this.walk_candidates(&picture, &request, 0, &Rc::new(Cell::new(false)));
                return;
            }
            if let Some(path) = cached {
                this.show_cached_portrait(&picture, &path, &request);
                return;
            }

            // Local covers do not wait on the network. A later portrait wins.
            let portrait_shown = Rc::new(Cell::new(false));
            this.walk_candidates(&picture, &request, 0, &portrait_shown);
            this.fetch_portrait(&picture, &request, &portrait_shown);
        });
    }

    fn portraits_enabled(&self) -> bool {
        self.portrait
            .borrow()
            .as_ref()
            .is_some_and(|runtime| runtime.is_enabled())
    }

    fn show_cached_portrait(
        self: &Rc<Self>,
        picture: &gtk4::Picture,
        path: &Path,
        request: &ArtistImageRequest,
    ) {
        let sink = gtk4::Picture::new();
        let picture = picture.clone();
        let sink_for_result = sink.clone();
        let mirrored = mirror_request(request);
        let this = self.clone();
        self.cover_loader.load_image_into_picture(
            &sink,
            path,
            request.size,
            request.token,
            &request.generation,
            move |loaded| {
                if loaded {
                    picture.set_paintable(sink_for_result.paintable().as_ref());
                    (mirrored.on_loaded)(true);
                } else {
                    this.walk_candidates(&picture, &mirrored, 0, &Rc::new(Cell::new(false)));
                }
            },
        );
    }

    fn walk_candidates(
        self: &Rc<Self>,
        picture: &gtk4::Picture,
        request: &ArtistImageRequest,
        tried: usize,
        portrait_shown: &Rc<Cell<bool>>,
    ) {
        let Some(candidate) = next_candidate(&request.candidates, tried) else {
            if !portrait_shown.get() {
                (request.on_loaded)(false);
            }
            return;
        };
        let sink = gtk4::Picture::new();
        let this = self.clone();
        let picture = picture.clone();
        let sink_for_result = sink.clone();
        let mirrored = mirror_request(request);
        let portrait_shown = portrait_shown.clone();
        self.cover_loader.load_into_picture(
            &sink,
            candidate,
            request.size,
            request.token,
            &request.generation,
            move |loaded| {
                if loaded {
                    if !portrait_shown.get() {
                        picture.set_paintable(sink_for_result.paintable().as_ref());
                        (mirrored.on_loaded)(true);
                    }
                } else {
                    this.walk_candidates(&picture, &mirrored, tried + 1, &portrait_shown);
                }
            },
        );
    }

    fn fetch_portrait(
        self: &Rc<Self>,
        picture: &gtk4::Picture,
        request: &ArtistImageRequest,
        portrait_shown: &Rc<Cell<bool>>,
    ) {
        let runtime = self.portrait.borrow().clone();
        let Some(runtime) = runtime else {
            return;
        };
        if !runtime.request_would_run(&request.artist) {
            return;
        }
        let this = self.clone();
        let picture = picture.clone();
        let mirrored = mirror_request(request);
        let portrait_shown = portrait_shown.clone();
        let generation = request.generation.clone();
        let token = request.token;
        runtime.request_while(
            request.artist.clone(),
            move || generation.get() == token,
            move |found| {
                let Some(path) = found else {
                    return;
                };
                if mirrored.generation.get() != mirrored.token {
                    return;
                }
                this.show_fetched_portrait(&picture, &path, &mirrored, &portrait_shown);
            },
        );
    }

    fn show_fetched_portrait(
        self: &Rc<Self>,
        picture: &gtk4::Picture,
        path: &Path,
        request: &ArtistImageRequest,
        portrait_shown: &Rc<Cell<bool>>,
    ) {
        let sink = gtk4::Picture::new();
        let picture = picture.clone();
        let sink_for_result = sink.clone();
        let mirrored = mirror_request(request);
        let portrait_shown = portrait_shown.clone();
        self.cover_loader.load_image_into_picture(
            &sink,
            path,
            request.size,
            request.token,
            &request.generation,
            move |loaded| {
                if loaded {
                    portrait_shown.set(true);
                    picture.set_paintable(sink_for_result.paintable().as_ref());
                    (mirrored.on_loaded)(true);
                }
            },
        );
    }
}

fn mirror_request(request: &ArtistImageRequest) -> ArtistImageRequest {
    ArtistImageRequest {
        artist: request.artist.clone(),
        candidates: request.candidates.clone(),
        size: request.size,
        token: request.token,
        generation: request.generation.clone(),
        on_loaded: request.on_loaded.clone(),
    }
}

#[cfg(test)]
#[path = "stats_artist_image_tests.rs"]
mod tests;
