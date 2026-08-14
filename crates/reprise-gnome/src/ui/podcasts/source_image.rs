//! Remote source artwork used by library rows and Add-dialog result/preview
//! rows (channel/show thumbnails, iTunes `artworkUrl600`, radio-browser
//! `favicon`) with either an icon or initials fallback.
//!
//! `NET-1a` / `C1`: every caller passes `images_allowed`, already computed as
//! `online_sources::network_allowed(conn, &modules::ARTWORK_MODULE)` at
//! its own call site — this widget never reads settings itself. A memory- or
//! disk-cache hit is always shown regardless of `images_allowed` (an
//! already-cached image is never hidden); only the network fallback on a
//! genuine cache miss is gated, via `reprise_core::remote_image::resolve`,
//! which is the sole place bytes are ever requested. The bounded on-disk
//! cache and the gate check both live in that pure core module. The same
//! worker that resolves the path also decodes and scales it; the main thread
//! only wraps the returned pixels in a GTK memory texture.
//!
//! The background artwork workers do not simply reuse the `images_allowed`
//! value a caller passed when a task was queued: that value can go stale if
//! the gate is switched off while the task is still sitting in the queue.
//! Instead [`load_texture`] and Preferences publish fresh values to
//! [`GATE_OPEN`], and only the worker reads it immediately before calling
//! `resolve`. This atomic is exclusively a fetch-time channel to worker
//! threads, never a source of UI state — see its doc comment for why this
//! shape was chosen over a per-task snapshot or a DB read from a worker.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::remote_image::CacheScope;

#[path = "source_artwork_queue.rs"]
mod source_artwork_queue;
#[path = "source_image_fallback.rs"]
mod source_image_fallback;
#[path = "source_image_texture.rs"]
mod source_image_texture;

pub(super) use source_image_texture::remember_texture;
use source_image_texture::{
    cached_texture, cached_texture_at_any_size, decode_pixels, memory_texture, DecodedPixels,
};

/// The fetch-time `images_allowed` gate shared exclusively with the worker
/// threads below; no UI path reads this atomic as its state source.
///
/// `NET-1a` requires that switching the gate off takes effect immediately —
/// including for artwork tasks that were already queued while it was on. The
/// worker threads have no `Db` and must not open one per
/// task (that would mean a DB hit on every dequeue, on a thread with no
/// natural connection lifetime); polling settings from a background thread is
/// the wrong shape here. Instead, every caller of `SourceImage::new`/
/// `set_url` already recomputes `images_allowed` fresh from its own live
/// connection on every render pass, as required by `SRC-11` — that is already
/// the freshest signal the app has. `load_texture` publishes each such value
/// into this atomic, Preferences republishes it when the setting changes, and
/// only the worker reads it again immediately before calling
/// `remote_image::resolve`, instead of trusting whatever value a task happened
/// to capture when it was built. A task queued while the gate was open
/// therefore still gets refused if the gate has since closed.
///
/// The worker queue is unbounded and coalesces matching in-flight URLs, so
/// rendering a large source cannot discard a visible row's only attempt.
///
/// Starts `false` so a failed/unknown gate state (nothing has published a
/// value yet) counts as not-allowed, per `NET-1a`.
static GATE_OPEN: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
static GATE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Copy)]
pub(crate) enum StartupTiming {
    Immediate,
    AfterQuiet,
}

#[derive(Clone, Copy)]
pub(crate) struct ArtworkRequest<'a> {
    primary_url: Option<&'a str>,
    fallback_url: Option<&'a str>,
    dimensions: (i32, i32),
    images_allowed: bool,
    cache_scope: CacheScope,
    startup_timing: StartupTiming,
}

impl<'a> ArtworkRequest<'a> {
    pub(crate) fn new(
        primary_url: Option<&'a str>,
        fallback_url: Option<&'a str>,
        dimensions: (i32, i32),
        images_allowed: bool,
        cache_scope: CacheScope,
        startup_timing: StartupTiming,
    ) -> Self {
        Self {
            primary_url,
            fallback_url,
            dimensions,
            images_allowed,
            cache_scope,
            startup_timing,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtworkStage {
    Fallback,
    Primary,
}

fn artwork_chain(
    primary_url: Option<&str>,
    fallback_url: Option<&str>,
) -> Vec<(ArtworkStage, String)> {
    let primary = primary_url.and_then(validated_url);
    let fallback = fallback_url
        .and_then(validated_url)
        .filter(|fallback| Some(fallback) != primary.as_ref());
    let mut chain = Vec::with_capacity(2);
    if let Some(fallback) = fallback {
        chain.push((ArtworkStage::Fallback, fallback));
    }
    if let Some(primary) = primary {
        chain.push((ArtworkStage::Primary, primary));
    }
    chain
}

fn may_publish_artwork(
    stage: ArtworkStage,
    generation: u64,
    current: &Cell<u64>,
    primary_visible: &Cell<bool>,
) -> bool {
    if current.get() != generation {
        return false;
    }
    match stage {
        ArtworkStage::Fallback => !primary_visible.get(),
        ArtworkStage::Primary => {
            primary_visible.set(true);
            true
        }
    }
}

/// `NET-1a` / `SET-4`: re-publishes the gate from settings when the setting
/// itself changes, rather than waiting for the next queued image.
///
/// Publishing only from the artwork queue is not enough on its own: it makes the
/// flag depend on somebody rendering another uncached image. Switch the gate
/// off from Preferences — a page that shows no source artwork at all — while a
/// queue is still draining, and nothing would submit more artwork, so the
/// stale `true` would survive and the queued tasks would keep fetching. That is
/// exactly the leak the atomic exists to close, one step removed.
///
/// A failed lookup counts as not allowed: refusing when unsure is the safe
/// direction for a privacy promise (`SRC-11`).
pub(in crate::ui) fn recompute_gate(conn: &Db) {
    let allowed =
        reprise_core::online_sources::network_allowed(conn, &reprise_core::modules::ARTWORK_MODULE)
            .unwrap_or(false);
    GATE_OPEN.store(allowed, Ordering::Relaxed);
}

#[derive(Clone)]
pub(crate) struct SourceImage {
    root: gtk4::Stack,
    fallback: gtk4::Widget,
    artwork: gtk4::Image,
    generation: Rc<Cell<u64>>,
}

impl SourceImage {
    pub(crate) fn new(
        image_url: Option<&str>,
        fallback_icon: &str,
        size: i32,
        images_allowed: bool,
        cache_scope: CacheScope,
    ) -> SourceImage {
        Self::new_with_dimensions(
            ArtworkRequest::new(
                image_url,
                None,
                (size, size),
                images_allowed,
                cache_scope,
                StartupTiming::Immediate,
            ),
            fallback_icon,
        )
    }

    /// Same as [`Self::new`], but also hands the decoded texture to
    /// `on_texture`.
    ///
    /// A second surface that wants the same artwork — the Now Playing bloom
    /// and shimmer want exactly the cover this widget shows — must not start
    /// its own load. Both loads would begin in the same main-loop turn, before
    /// either could populate the texture cache, so both would miss it, both
    /// would take a queue slot and a worker, and both would ask the same
    /// third-party host for the same image. One load, two consumers.
    pub(crate) fn new_observed(
        request: ArtworkRequest<'_>,
        fallback_icon: &str,
        on_texture: impl Fn(&gtk4::gdk::Texture) + 'static,
    ) -> SourceImage {
        let (width, height) = request.dimensions;
        let image = Self::build(
            source_image_fallback::Fallback::Icon(fallback_icon),
            width,
            height,
        );
        image.set_urls(request, on_texture);
        image
    }

    pub(crate) fn new_with_dimensions(
        request: ArtworkRequest<'_>,
        fallback_icon: &str,
    ) -> SourceImage {
        let (width, height) = request.dimensions;
        let image = Self::build(
            source_image_fallback::Fallback::Icon(fallback_icon),
            width,
            height,
        );
        image.set_urls(request, |_| {});
        image
    }

    /// The widget tree alone, without a load — both constructors share it so
    /// the artwork is only ever requested once, by whichever `set_url` follows.
    fn build(
        fallback_kind: source_image_fallback::Fallback<'_>,
        width: i32,
        height: i32,
    ) -> SourceImage {
        let fallback = source_image_fallback::widget(fallback_kind, width, height);
        // The artwork is a `Gtk::Image`, not a `Gtk::Picture`, and that is the
        // whole point of this widget: a `Picture` measures its natural size
        // from the texture, and neither `set_size_request` nor an `AspectFrame`
        // caps that — both only ever raise the *minimum*. A single 600 px cover
        // therefore grew its row to 600 px. An `Image` with `pixel_size` asks
        // for exactly that many pixels no matter how large the texture is, and
        // paints the texture scaled into whatever it is given, so the row's
        // height comes from its text alone. The texture itself is still cached
        // at 2x this size, so the downscale stays sharp on HiDPI.
        let artwork = gtk4::Image::new();
        artwork.set_pixel_size(width.min(height));
        artwork.set_halign(gtk4::Align::Center);
        artwork.set_valign(gtk4::Align::Center);
        artwork.set_hexpand(false);
        artwork.set_vexpand(false);
        let root = gtk4::Stack::new();
        root.set_size_request(width, height);
        root.set_halign(gtk4::Align::Center);
        root.set_valign(gtk4::Align::Center);
        root.set_hexpand(false);
        root.set_vexpand(false);
        // A `Stack` is homogeneous by default, i.e. it measures every page and
        // sizes itself to the largest — so a hidden artwork page would still
        // inflate the row. Both pages are bounded to the same size here anyway;
        // switching homogeneity off keeps that true even if one page changes.
        root.set_hhomogeneous(false);
        root.set_vhomogeneous(false);
        root.set_overflow(gtk4::Overflow::Hidden);
        root.add_css_class("reprise-source-image");
        root.add_named(&fallback, Some("fallback"));
        root.add_named(&artwork, Some("artwork"));
        root.set_visible_child(&fallback);
        Self {
            root,
            fallback,
            artwork,
            generation: Rc::new(Cell::new(0)),
        }
    }

    pub(crate) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    fn set_urls(
        &self,
        request: ArtworkRequest<'_>,
        on_texture: impl Fn(&gtk4::gdk::Texture) + 'static,
    ) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.root.set_visible_child(&self.fallback);
        self.artwork.set_paintable(gtk4::gdk::Paintable::NONE);
        let weak_root = self.root.downgrade();
        let weak_artwork = self.artwork.downgrade();
        load_texture_chain(request, generation, &self.generation, move |texture| {
            // The observer runs even if the widget itself is already gone:
            // it feeds a different surface, whose own generation check
            // decides whether the texture is still wanted.
            on_texture(&texture);
            let Some(root) = weak_root.upgrade() else {
                return;
            };
            let Some(artwork) = weak_artwork.upgrade() else {
                return;
            };
            artwork.set_paintable(Some(&texture));
            root.set_visible_child(&artwork);
        });
    }
}

fn load_texture_chain(
    request: ArtworkRequest<'_>,
    generation: u64,
    current: &Rc<Cell<u64>>,
    on_ready: impl Fn(gtk4::gdk::Texture) + 'static,
) {
    let primary_visible = Rc::new(Cell::new(false));
    let on_ready: Rc<dyn Fn(gtk4::gdk::Texture)> = Rc::new(on_ready);
    for (stage, url) in artwork_chain(request.primary_url, request.fallback_url) {
        let primary_visible = primary_visible.clone();
        let current_for_callback = current.clone();
        let on_ready = on_ready.clone();
        if stage == ArtworkStage::Fallback {
            if let Some(texture) = cached_texture_at_any_size(&url, request.cache_scope) {
                if may_publish_artwork(stage, generation, &current_for_callback, &primary_visible) {
                    on_ready(texture);
                }
            }
        }
        load_texture(Some(&url), request, generation, current, move |texture| {
            if may_publish_artwork(stage, generation, &current_for_callback, &primary_visible) {
                on_ready(texture);
            }
        });
    }
}

/// Loads source artwork through the one gated cache/queue/decode path and
/// hands the finished texture to a generation-safe caller.
fn load_texture(
    image_url: Option<&str>,
    request: ArtworkRequest<'_>,
    generation: u64,
    current: &Rc<Cell<u64>>,
    on_ready: impl Fn(gtk4::gdk::Texture) + 'static,
) {
    let (width, height) = request.dimensions;
    if current.get() != generation {
        return;
    }
    let Some(url) = image_url.and_then(validated_url) else {
        return;
    };
    if let Some(texture) = cached_texture(&url, width, height, request.cache_scope) {
        on_ready(texture);
        return;
    }
    // `NET-1a` / `SRC-11`: resolve checks the disk cache before consulting
    // the network gate, so an already-downloaded image remains visible while
    // a closed gate still refuses every fresh request.
    // Publish at registration time, before the startup gate can delay this
    // task. A later Preferences change can therefore close `GATE_OPEN` while
    // the task waits, and the worker's fetch-time read below remains final.
    GATE_OPEN.store(request.images_allowed, Ordering::Relaxed);
    let current = current.clone();
    let start = move || {
        if current.get() != generation {
            return;
        }
        let receiver = source_artwork_queue::queue(url.clone(), width, height, request.cache_scope);
        gtk4::glib::spawn_future_local(async move {
            let pixels = match receiver.recv().await {
                Ok(Some(pixels)) => pixels,
                Ok(None) => return,
                Err(error) => {
                    tracing::debug!(%error, %url, "could not load source artwork");
                    return;
                }
            };
            // This generation check deliberately remains on the GTK thread,
            // immediately before the ready-made pixels can be published.
            if current.get() != generation {
                return;
            }
            let texture = memory_texture(pixels);
            remember_texture(url, width, height, request.cache_scope, texture.clone());
            on_ready(texture);
        });
    };
    match request.startup_timing {
        StartupTiming::Immediate => start(),
        StartupTiming::AfterQuiet => crate::ui::startup_quiet::run_after_quiet(start),
    }
}

/// Loads the same gated and cached source artwork into an image owned by a
/// different surface. The caller owns the generation so changing playback
/// invalidates any older decode before it can repaint the player bar.
pub(crate) fn load_into_image(
    image: &gtk4::Image,
    request: ArtworkRequest<'_>,
    generation: u64,
    current: &Rc<Cell<u64>>,
) {
    if current.get() != generation {
        return;
    }
    crate::ui::cover_loader::CoverLoader::set_placeholder(image);
    let weak_image = image.downgrade();
    load_texture_chain(request, generation, current, move |texture| {
        let Some(image) = weak_image.upgrade() else {
            return;
        };
        image.set_paintable(Some(&texture));
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
    fn large_source_pixels_are_decoded_to_twice_the_requested_cache_size() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large.png");
        let pixbuf =
            gtk4::gdk_pixbuf::Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, true, 8, 600, 600)
                .unwrap();
        pixbuf.fill(0x336699ff);
        pixbuf.savev(&path, "png", &[]).unwrap();

        let pixels = super::decode_pixels(&path, 40, 40).unwrap();

        assert_eq!((pixels.width, pixels.height), (80, 80));
    }

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
    fn src_11_transient_memory_texture_does_not_bypass_persistent_storage() {
        let url = "https://images.test/src-11-memory-scope-boundary.png".to_owned();
        let texture = super::memory_texture(super::DecodedPixels {
            bytes: vec![0, 0, 0],
            width: 1,
            height: 1,
            rowstride: 3,
            has_alpha: false,
        });
        super::remember_texture(
            url.clone(),
            40,
            40,
            reprise_core::remote_image::CacheScope::Transient,
            texture,
        );

        assert!(super::cached_texture(
            &url,
            40,
            40,
            reprise_core::remote_image::CacheScope::Persistent,
        )
        .is_none());
        assert!(super::cached_texture(
            &url,
            40,
            40,
            reprise_core::remote_image::CacheScope::Transient,
        )
        .is_some());
        assert!(super::cached_texture_at_any_size(
            &url,
            reprise_core::remote_image::CacheScope::Transient,
        )
        .is_some());
        assert!(super::cached_texture_at_any_size(
            &url,
            reprise_core::remote_image::CacheScope::Persistent,
        )
        .is_none());
    }

    #[test]
    fn src_11_failed_episode_artwork_keeps_the_show_fallback() {
        let current = std::cell::Cell::new(7);
        let primary_visible = std::cell::Cell::new(false);

        assert!(super::may_publish_artwork(
            super::ArtworkStage::Fallback,
            7,
            &current,
            &primary_visible,
        ));
        assert!(!primary_visible.get());
    }

    #[test]
    fn src_11_missing_artwork_chain_keeps_the_source_glyph() {
        assert!(super::artwork_chain(None, None).is_empty());
    }

    #[test]
    fn src_11_episode_artwork_replaces_the_show_fallback() {
        let current = std::cell::Cell::new(9);
        let primary_visible = std::cell::Cell::new(false);

        assert!(super::may_publish_artwork(
            super::ArtworkStage::Fallback,
            9,
            &current,
            &primary_visible,
        ));
        assert!(super::may_publish_artwork(
            super::ArtworkStage::Primary,
            9,
            &current,
            &primary_visible,
        ));
        assert!(primary_visible.get());
        assert!(!super::may_publish_artwork(
            super::ArtworkStage::Fallback,
            9,
            &current,
            &primary_visible,
        ));
    }

    #[test]
    fn src_11_recycled_row_rejects_both_artwork_stages() {
        let current = std::cell::Cell::new(12);
        let primary_visible = std::cell::Cell::new(false);

        assert!(!super::may_publish_artwork(
            super::ArtworkStage::Fallback,
            11,
            &current,
            &primary_visible,
        ));
        assert!(!super::may_publish_artwork(
            super::ArtworkStage::Primary,
            11,
            &current,
            &primary_visible,
        ));
        assert!(!primary_visible.get());
    }

    /// A real 1x1 truecolor PNG, small enough to inline and valid enough for
    /// GDK to decode — the cache only ever holds bytes a decoder accepted.
    const TINY_PNG: [u8; 69] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xA8,
        0xAF, 0xAF, 0x07, 0x00, 0x02, 0xFE, 0x01, 0x7E, 0xBA, 0x25, 0x70, 0x25, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    /// Preferences shows no source artwork, so switching the setting off must
    /// publish the closed gate without waiting for a further enqueue.
    #[test]
    fn src_11_turning_the_setting_off_closes_the_gate_without_a_further_enqueue() {
        use std::sync::atomic::Ordering;

        let _gate = super::GATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let conn = crate::test_db::open().unwrap();
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, true)
            .unwrap();
        super::recompute_gate(&conn);
        assert!(
            super::GATE_OPEN.load(Ordering::SeqCst),
            "precondition: module on and global on means the gate is open"
        );

        // The user switches the global master off from Preferences. No image
        // is rendered there, so nothing submits more artwork.
        reprise_core::online_sources::set_enabled(&conn, false).unwrap();
        super::recompute_gate(&conn);

        assert!(
            !super::GATE_OPEN.load(Ordering::SeqCst),
            "Preferences must publish the closed gate immediately"
        );
    }

    /// `SRC-11`: a cache hit is always shown, independently of the gate.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_11_a_cached_image_is_shown_even_with_the_gate_closed() {
        gtk4::init().unwrap();
        let url = "https://images.test/src-11-cached.png";
        // Populate the cache through the public core path, with the gate open.
        let outcome = reprise_core::remote_image::resolve(
            Some(url),
            reprise_core::remote_image::CacheScope::Persistent,
            true,
            &mut |_| Ok(TINY_PNG.to_vec()),
        );
        assert!(
            matches!(
                outcome,
                reprise_core::remote_image::ImageOutcome::Fetched(_)
                    | reprise_core::remote_image::ImageOutcome::Cached(_)
            ),
            "precondition: the image must be in the cache, got {outcome:?}"
        );

        // Now the gate is closed. No request may happen — and no image may be
        // hidden either.
        let image = super::SourceImage::new(
            Some(url),
            "audio-input-microphone-symbolic",
            40,
            false,
            reprise_core::remote_image::CacheScope::Persistent,
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while image.widget().visible_child_name().as_deref() != Some("artwork") {
            while gtk4::glib::MainContext::default().iteration(false) {}
            assert!(
                std::time::Instant::now() < deadline,
                "a cached image must be shown with the gate closed, but the widget stayed on the fallback"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_11_gate_closed_stays_on_the_fallback_and_never_fetches() {
        gtk4::init().unwrap();
        let image = super::SourceImage::new(
            Some("https://images.test/net-1a-widget-closed.jpg"),
            "audio-input-microphone-symbolic",
            40,
            false,
            reprise_core::remote_image::CacheScope::Persistent,
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
        let image = super::SourceImage::new(
            None,
            "audio-input-microphone-symbolic",
            40,
            true,
            reprise_core::remote_image::CacheScope::Persistent,
        );
        assert_eq!(
            image.widget().visible_child_name().as_deref(),
            Some("fallback")
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn play_10_cached_external_artwork_loads_into_an_existing_player_image() {
        use std::cell::Cell;
        use std::rc::Rc;

        gtk4::init().unwrap();
        let url = "https://images.test/play-10-player-bar.png";
        reprise_core::remote_image::resolve(
            Some(url),
            reprise_core::remote_image::CacheScope::Persistent,
            true,
            &mut |_| Ok(TINY_PNG.to_vec()),
        );
        let image = gtk4::Image::new();
        let current = Rc::new(Cell::new(1));

        super::load_into_image(
            &image,
            super::ArtworkRequest::new(
                Some(url),
                None,
                (56, 56),
                false,
                reprise_core::remote_image::CacheScope::Persistent,
                super::StartupTiming::Immediate,
            ),
            1,
            &current,
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while image.paintable().is_none() {
            while gtk4::glib::MainContext::default().iteration(false) {}
            assert!(
                std::time::Instant::now() < deadline,
                "cached episode artwork did not reach the existing player image"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
