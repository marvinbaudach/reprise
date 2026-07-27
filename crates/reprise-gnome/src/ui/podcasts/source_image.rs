//! Remote source artwork used by subscription and search surfaces.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::OnceLock;

use gtk4::prelude::*;

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
    pub(crate) fn new(image_url: Option<&str>, fallback_icon: &str, size: i32) -> SourceImage {
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
        image.set_url(image_url);
        image
    }

    pub(crate) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    pub(crate) fn set_url(&self, image_url: Option<&str>) {
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
        let Some(receiver) = queue_artwork(url.clone()) else {
            tracing::debug!(%url, "source artwork queue is full");
            return;
        };
        let weak_root = self.root.downgrade();
        let weak_picture = self.picture.downgrade();
        let current = self.generation.clone();
        gtk4::glib::spawn_future_local(async move {
            let bytes = match receiver.recv().await {
                Ok(Ok(bytes)) => bytes,
                Ok(Err(error)) => {
                    tracing::debug!(%error, %url, "could not load source artwork");
                    return;
                }
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
            let bytes = gtk4::glib::Bytes::from_owned(bytes);
            let texture = match gtk4::gdk::Texture::from_bytes(&bytes) {
                Ok(texture) => texture,
                Err(error) => {
                    tracing::debug!(%error, %url, "source artwork could not be decoded");
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
    response: async_channel::Sender<Result<Vec<u8>, reprise_core::podcasts::PodcastError>>,
}

fn queue_artwork(
    url: String,
) -> Option<async_channel::Receiver<Result<Vec<u8>, reprise_core::podcasts::PodcastError>>> {
    static QUEUE: OnceLock<async_channel::Sender<ArtworkTask>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = async_channel::bounded::<ArtworkTask>(ARTWORK_QUEUE_LIMIT);
        for index in 0..ARTWORK_WORKERS {
            let receiver = receiver.clone();
            if let Err(error) = std::thread::Builder::new()
                .name(format!("reprise-source-artwork-{index}"))
                .spawn(move || {
                    while let Ok(task) = receiver.recv_blocking() {
                        let result = reprise_core::podcasts::source_artwork::fetch(&task.url);
                        let _ = task.response.send_blocking(result);
                    }
                })
            {
                tracing::warn!(%error, "could not start source artwork worker");
            }
        }
        sender
    });
    let (response, receiver) = async_channel::bounded(1);
    queue.try_send(ArtworkTask { url, response }).ok()?;
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
}
