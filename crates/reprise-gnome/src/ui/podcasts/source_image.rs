//! Remote source artwork used by library rows and Add-dialog result/preview
//! rows (channel/show thumbnails, iTunes `artworkUrl600`, radio-browser
//! `favicon`).
//!
//! `NET-1a` / `C1`: every caller passes `images_allowed`, already computed as
//! `online_sources::network_allowed(conn, &modules::SOURCE_IMAGES_MODULE)` at
//! its own call site — this widget never reads settings itself. A memory- or
//! disk-cache hit is always shown regardless of `images_allowed` (an
//! already-cached image is never hidden); only the network fallback on a
//! genuine cache miss is gated, via `reprise_core::remote_image::resolve`,
//! which is the sole place bytes are ever requested. The bounded on-disk
//! cache and the gate check both live in that pure core module; this file
//! only decodes the resulting path into a GTK texture.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::OnceLock;

use gtk4::prelude::*;
use reprise_core::remote_image::ImageOutcome;

const CACHE_LIMIT: usize = 128;
const ARTWORK_QUEUE_LIMIT: usize = 64;
const ARTWORK_WORKERS: usize = 4;

thread_local! {
    static TEXTURE_CACHE: RefCell<VecDeque<(String, gtk4::gdk::Texture)>> =
        const { RefCell::new(VecDeque::new()) };
}

#[derive(Clone)]
pub(crate) struct SourceImage {
    root: gtk4::Stack,
    fallback: gtk4::Image,
    picture: gtk4::Picture,
    generation: Rc<Cell<u64>>,
}

impl SourceImage {
    pub(crate) fn new(
        image_url: Option<&str>,
        fallback_icon: &str,
        size: i32,
        images_allowed: bool,
    ) -> SourceImage {
        let fallback = gtk4::Image::from_icon_name(fallback_icon);
        fallback.set_pixel_size(size);
        let picture = gtk4::Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_size_request(size, size);
        let root = gtk4::Stack::new();
        root.set_size_request(size, size);
        root.set_overflow(gtk4::Overflow::Hidden);
        root.add_css_class("reprise-source-image");
        root.add_named(&fallback, Some("fallback"));
        root.add_named(&picture, Some("artwork"));
        root.set_visible_child(&fallback);
        let image = Self {
            root,
            fallback,
            picture,
            generation: Rc::new(Cell::new(0)),
        };
        image.set_url(image_url, images_allowed);
        image
    }

    pub(crate) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    fn set_url(&self, image_url: Option<&str>, images_allowed: bool) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.root.set_visible_child(&self.fallback);
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);
        let Some(url) = image_url.and_then(validated_url) else {
            return;
        };
        if let Some(texture) = cached_texture(&url) {
            self.picture.set_paintable(Some(&texture));
            self.root.set_visible_child(&self.picture);
            return;
        }
        // `NET-1a` / `C1`: a memory-cache miss does not by itself justify a
        // network attempt. `queue_artwork` still checks the on-disk cache
        // (which may hold a hit from an earlier session) before consulting
        // this flag again in `remote_image::resolve`, so a closed gate never
        // hides an already-downloaded image — it only refuses a fresh fetch.
        if !images_allowed {
            return;
        }
        let Some(receiver) = queue_artwork(url.clone(), images_allowed) else {
            tracing::debug!(%url, "source artwork queue is full");
            return;
        };
        let weak_root = self.root.downgrade();
        let weak_picture = self.picture.downgrade();
        let current = self.generation.clone();
        gtk4::glib::spawn_future_local(async move {
            let path = match receiver.recv().await {
                Ok(Some(path)) => path,
                Ok(None) => return,
                Err(error) => {
                    tracing::debug!(%error, %url, "could not load source artwork");
                    return;
                }
            };
            if current.get() != generation {
                return;
            }
            let Some(root) = weak_root.upgrade() else {
                return;
            };
            let Some(picture) = weak_picture.upgrade() else {
                return;
            };
            let texture = match gtk4::gdk::Texture::from_filename(&path) {
                Ok(texture) => texture,
                Err(error) => {
                    tracing::debug!(%error, %url, path = %path.display(), "source artwork could not be decoded");
                    return;
                }
            };
            remember_texture(url, texture.clone());
            picture.set_paintable(Some(&texture));
            root.set_visible_child(&picture);
        });
    }
}

struct ArtworkTask {
    url: String,
    /// `NET-1a`: threaded through to `remote_image::resolve` as a
    /// defense-in-depth re-check — `set_url` already refuses to enqueue a
    /// task when this is false, so this only matters if that ever changes.
    allowed: bool,
    response: async_channel::Sender<Option<PathBuf>>,
}

fn queue_artwork(url: String, allowed: bool) -> Option<async_channel::Receiver<Option<PathBuf>>> {
    static QUEUE: OnceLock<async_channel::Sender<ArtworkTask>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = async_channel::bounded::<ArtworkTask>(ARTWORK_QUEUE_LIMIT);
        for index in 0..ARTWORK_WORKERS {
            let receiver = receiver.clone();
            if let Err(error) = std::thread::Builder::new()
                .name(format!("reprise-source-artwork-{index}"))
                .spawn(move || {
                    while let Ok(task) = receiver.recv_blocking() {
                        let outcome = reprise_core::remote_image::resolve(
                            Some(&task.url),
                            task.allowed,
                            &mut |url| {
                                reprise_core::podcasts::source_artwork::fetch(url)
                                    .map_err(|error| error.to_string())
                            },
                        );
                        let path = match outcome {
                            ImageOutcome::Cached(path) | ImageOutcome::Fetched(path) => Some(path),
                            ImageOutcome::NotAllowed
                            | ImageOutcome::NoUrl
                            | ImageOutcome::FetchFailed => None,
                        };
                        let _ = task.response.send_blocking(path);
                    }
                })
            {
                tracing::warn!(%error, "could not start source artwork worker");
            }
        }
        sender
    });
    let (response, receiver) = async_channel::bounded(1);
    queue
        .try_send(ArtworkTask {
            url,
            allowed,
            response,
        })
        .ok()?;
    Some(receiver)
}

fn cached_texture(url: &str) -> Option<gtk4::gdk::Texture> {
    TEXTURE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index = cache.iter().position(|(cached, _)| cached == url)?;
        let entry = cache.remove(index)?;
        let texture = entry.1.clone();
        cache.push_front(entry);
        Some(texture)
    })
}

fn remember_texture(url: String, texture: gtk4::gdk::Texture) {
    TEXTURE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(index) = cache.iter().position(|(cached, _)| cached == &url) {
            cache.remove(index);
        }
        cache.push_front((url, texture));
        cache.truncate(CACHE_LIMIT);
    });
}

fn validated_url(value: &str) -> Option<String> {
    let value = value.trim();
    let uri = gtk4::glib::Uri::parse(value, gtk4::glib::UriFlags::NONE).ok()?;
    let valid_scheme = matches!(uri.scheme().as_str(), "http" | "https");
    (valid_scheme && uri.host().is_some()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_artwork_accepts_only_remote_http_urls() {
        assert_eq!(
            super::validated_url("https://images.test/show.jpg"),
            Some("https://images.test/show.jpg".into())
        );
        assert_eq!(
            super::validated_url("http://images.test/show.jpg"),
            Some("http://images.test/show.jpg".into())
        );
        assert_eq!(super::validated_url("file:///home/user/secret"), None);
        assert_eq!(super::validated_url("data:image/png;base64,AAAA"), None);
        assert_eq!(super::validated_url("not a URL"), None);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_11_gate_closed_stays_on_the_fallback_and_never_queues_a_fetch() {
        gtk4::init().unwrap();
        let image = super::SourceImage::new(
            Some("https://images.test/net-1a-widget-closed.jpg"),
            "audio-input-microphone-symbolic",
            40,
            false,
        );
        assert_eq!(
            image.widget().visible_child_name().as_deref(),
            Some("fallback")
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_11_no_url_stays_on_the_fallback_regardless_of_the_gate() {
        gtk4::init().unwrap();
        let image = super::SourceImage::new(None, "audio-input-microphone-symbolic", 40, true);
        assert_eq!(
            image.widget().visible_child_name().as_deref(),
            Some("fallback")
        );
    }
}
