//! Lazy release-group covers with an immediate, spinner-free fallback tile.

#![allow(dead_code)] // Shared by the Updates popover and recycled table cells.

use gtk4::prelude::*;

use crate::ui::{artist_avatar, one_shot_task};

const INITIALS_CLASS: &str = "reprise-release-cover-initials";
const TILE_CLASS: &str = "reprise-release-cover-tile";
const PICTURE_CLASS: &str = "reprise-release-cover-picture";
const MBID_CLASS: &str = "reprise-release-cover-mbid";
const ARTIST_CLASS: &str = "reprise-release-cover-artist";
const STARTED_CLASS: &str = "reprise-release-cover-started";

pub(in crate::ui) struct LazyReleaseCover {
    root: gtk4::Overlay,
    tile: gtk4::DrawingArea,
    initials: gtk4::Label,
    picture: gtk4::Picture,
    mbid: gtk4::Label,
    artist: gtk4::Label,
    started: gtk4::Label,
}

impl LazyReleaseCover {
    /// Compatibility constructor for the Updates row, which owns one cover
    /// for its whole lifetime. Recycled table cells use [`Self::new_unbound`]
    /// and bind their current row with [`Self::set_release`].
    pub(in crate::ui) fn new(release_group_mbid: &str, artist: &str, edge: i32) -> Self {
        let cover = Self::new_unbound(edge);
        cover.set_release(release_group_mbid, artist);
        cover
    }

    /// Builds an initials tile and, when allowed, replaces it only with an
    /// already-cached artist portrait. The empty release id guarantees the
    /// map handler can never start a cover or portrait network request.
    pub(in crate::ui) fn new_cached_artist(artist: &str, edge: i32, allowed: bool) -> Self {
        let cover = Self::new("", artist, edge);
        if allowed {
            if let reprise_core::artist_portrait::PortraitOutcome::Found(path) =
                reprise_core::artist_portrait::load_cached(artist)
            {
                cover.picture.set_filename(Some(path));
                cover.picture.set_visible(true);
            }
        }
        cover
    }

    /// Builds an empty cover suitable for a recycled `ColumnView` cell.
    pub(in crate::ui) fn new_unbound(edge: i32) -> Self {
        let root = gtk4::Overlay::new();
        root.set_size_request(edge, edge);
        root.add_css_class("new-release-cover");

        let initials = gtk4::Label::new(None);
        initials.set_visible(false);
        initials.set_can_target(false);
        initials.add_css_class(INITIALS_CLASS);

        let tile = gtk4::DrawingArea::new();
        tile.set_content_width(edge);
        tile.set_content_height(edge);
        tile.add_css_class(TILE_CLASS);
        let tile_initials = initials.clone();
        tile.set_draw_func(move |area, context, width, height| {
            let is_dark = crate::ui::style::accent::is_dark();
            let foreground = area.color();
            let surface = crate::ui::style::accent::window_background_rgb(area);
            super::release_cover_tile::draw(
                context,
                &area.pango_context(),
                f64::from(width),
                f64::from(height),
                &tile_initials.text(),
                &super::release_cover_tile::Appearance {
                    is_dark,
                    foreground,
                    surface,
                },
            );
        });
        root.set_child(Some(&tile));

        let picture = gtk4::Picture::new();
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_visible(false);
        picture.add_css_class(PICTURE_CLASS);
        root.add_overlay(&picture);

        // GTK owns these four invisible labels with the cell, so a wrapper
        // reconstructed during bind reaches the same per-cell async state
        // without unsafe qdata or a second GObject subclass.
        let mbid = state_label(MBID_CLASS);
        let artist = state_label(ARTIST_CLASS);
        let started = state_label(STARTED_CLASS);
        root.add_overlay(&initials);
        root.add_overlay(&mbid);
        root.add_overlay(&artist);
        root.add_overlay(&started);

        wire_lazy_fetch(&root, &picture, &mbid, &started);
        Self {
            root,
            tile,
            initials,
            picture,
            mbid,
            artist,
            started,
        }
    }

    pub(in crate::ui) fn from_widget(root: &gtk4::Overlay) -> Option<Self> {
        Some(Self {
            root: root.clone(),
            tile: child_with_class(root, TILE_CLASS)?,
            initials: child_with_class(root, INITIALS_CLASS)?,
            picture: child_with_class(root, PICTURE_CLASS)?,
            mbid: child_with_class(root, MBID_CLASS)?,
            artist: child_with_class(root, ARTIST_CLASS)?,
            started: child_with_class(root, STARTED_CLASS)?,
        })
    }

    pub(in crate::ui) fn set_release(&self, release_group_mbid: &str, artist: &str) {
        self.mbid.set_text(release_group_mbid);
        self.artist.set_text("");
        self.started.set_text("");
        self.initials.set_text(&artist_avatar::initials(artist));
        self.tile.queue_draw();
        self.picture.set_filename(None::<&std::path::Path>);
        self.picture.set_visible(false);
        if release_group_mbid.is_empty() {
            return;
        }
        if let Some(path) =
            reprise_core::cover_download::release_group_cover_path(release_group_mbid)
        {
            self.picture.set_filename(Some(path));
            self.picture.set_visible(true);
            self.started.set_text(release_group_mbid);
            return;
        }
        // Updates rows bind once before their first map, while ColumnView
        // cells can be rebound without an intervening unmap/map cycle.
        // Starting here only for an already-mapped widget serves the latter;
        // the map handler below remains the former's explicit trigger.
        if self.root.is_mapped() {
            start_fetch(&self.picture, &self.mbid, &self.started);
        }
    }

    /// Binds this cell to an artist instead of a release: initials tile,
    /// no image, and the artist as the cell's key. The MBID label stays
    /// empty, so neither `set_release` nor the map handler can ever start
    /// a release-cover fetch from a concert cell.
    pub(in crate::ui) fn set_artist_key(&self, artist: &str) {
        self.mbid.set_text("");
        self.artist.set_text(artist);
        self.started.set_text("");
        self.initials.set_text(&artist_avatar::initials(artist));
        self.tile.queue_draw();
        self.picture.set_paintable(None::<&gtk4::gdk::Paintable>);
        self.picture.set_visible(false);
    }

    pub(in crate::ui) fn artist_key(&self) -> String {
        self.artist.text().to_string()
    }

    pub(in crate::ui) fn show_paintable(&self, paintable: Option<&gtk4::gdk::Paintable>) {
        self.picture.set_paintable(paintable);
        self.picture.set_visible(paintable.is_some());
    }

    pub(in crate::ui) fn started(&self) -> String {
        self.started.text().to_string()
    }

    pub(in crate::ui) fn mark_started(&self, artist: &str) {
        self.started.set_text(artist);
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Overlay {
        &self.root
    }

    #[cfg(test)]
    pub(in crate::ui) fn initials_text(&self) -> String {
        self.initials.text().to_string()
    }

    #[cfg(test)]
    pub(in crate::ui) fn shows_image(&self) -> bool {
        self.picture.is_visible()
    }
}

fn state_label(class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.set_visible(false);
    label.set_can_target(false);
    label.add_css_class(class);
    label
}

fn child_with_class<T: IsA<gtk4::Widget> + Clone + 'static>(
    root: &gtk4::Overlay,
    class: &str,
) -> Option<T> {
    let mut child = root.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if widget.has_css_class(class) {
            if let Ok(widget) = widget.downcast::<T>() {
                return Some(widget);
            }
        }
        child = next;
    }
    None
}

fn wire_lazy_fetch(
    root: &gtk4::Overlay,
    picture: &gtk4::Picture,
    mbid: &gtk4::Label,
    started: &gtk4::Label,
) {
    let picture = picture.clone();
    let mbid = mbid.clone();
    let started = started.clone();
    root.connect_map(move |_| {
        start_fetch(&picture, &mbid, &started);
    });
}

fn start_fetch(picture: &gtk4::Picture, mbid: &gtk4::Label, started: &gtk4::Label) {
    let release_group_mbid = mbid.text().to_string();
    if release_group_mbid.is_empty() || started.text() == release_group_mbid {
        return;
    }
    started.set_text(&release_group_mbid);
    if notify_test_fetch(&release_group_mbid) {
        return;
    }
    let fetch_mbid = release_group_mbid.clone();
    let Ok(receiver) = one_shot_task::spawn("reprise-release-cover", move || {
        reprise_core::cover_download::fetch_release_group_cover(&fetch_mbid)
    }) else {
        return;
    };
    let picture = picture.clone();
    let mbid = mbid.clone();
    gtk4::glib::spawn_future_local(async move {
        if let Ok(reprise_core::cover_download::ReleaseGroupCover::Image(path)) =
            receiver.recv().await
        {
            // A recycled cell may have been rebound while this fetch was in
            // flight. Only the generation still naming this MBID may update
            // the picture.
            if mbid.text() == release_group_mbid {
                picture.set_filename(Some(path));
                picture.set_visible(true);
            }
        }
    });
}

#[cfg(not(test))]
fn notify_test_fetch(_release_group_mbid: &str) -> bool {
    false
}

#[cfg(test)]
type TestFetchObserver = std::rc::Rc<dyn Fn(&str)>;

#[cfg(test)]
thread_local! {
    static TEST_FETCH_OBSERVER: std::cell::RefCell<Option<TestFetchObserver>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
fn notify_test_fetch(release_group_mbid: &str) -> bool {
    TEST_FETCH_OBSERVER.with(|slot| {
        let observer = slot.borrow().clone();
        if let Some(observer) = observer {
            observer(release_group_mbid);
            true
        } else {
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descendant_with_class(widget: &gtk4::Widget, class: &str) -> Option<gtk4::Widget> {
        if widget.has_css_class(class) {
            return Some(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(found) = descendant_with_class(&current, class) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }

    fn release_cover_in_column_view(artist: &str) -> (gtk4::Window, LazyReleaseCover) {
        let model = gtk4::gio::ListStore::new::<gtk4::StringObject>();
        model.append(&gtk4::StringObject::new(artist));
        let selection = gtk4::NoSelection::new(Some(model));
        let view = gtk4::ColumnView::new(Some(selection));
        view.add_css_class(crate::ui::source_context_surface::TABLE_CSS_CLASS);
        let factory = gtk4::SignalListItemFactory::new();
        factory.connect_setup(|_, object| {
            let item = object.downcast_ref::<gtk4::ListItem>().unwrap();
            let cover = LazyReleaseCover::new_unbound(56);
            item.set_child(Some(&crate::ui::source_context_surface::wrap(
                cover.widget(),
            )));
        });
        let artist = artist.to_owned();
        factory.connect_bind(move |_, object| {
            let item = object.downcast_ref::<gtk4::ListItem>().unwrap();
            let child = item.child().unwrap();
            let root = descendant_with_class(&child, "new-release-cover")
                .unwrap()
                .downcast::<gtk4::Overlay>()
                .unwrap();
            LazyReleaseCover::from_widget(&root)
                .unwrap()
                .set_release("11111111-1111-1111-1111-111111111111", &artist);
        });
        let column = gtk4::ColumnViewColumn::new(Some("Cover"), Some(factory));
        column.set_fixed_width(68);
        view.append_column(&column);
        let window = gtk4::Window::builder()
            .default_width(180)
            .default_height(120)
            .child(&view)
            .build();
        window.present();
        crate::ui::source_context_surface::settle_layout();
        let root = descendant_with_class(view.upcast_ref(), "new-release-cover")
            .expect("the ColumnView realized its release-cover cell")
            .downcast::<gtk4::Overlay>()
            .unwrap();
        (window, LazyReleaseCover::from_widget(&root).unwrap())
    }

    struct RenderedTile {
        width: i32,
        height: i32,
        stride: usize,
        pixels: Vec<u8>,
    }

    impl RenderedTile {
        fn rgba_at(&self, x: i32, y: i32) -> [u8; 4] {
            let offset = y as usize * self.stride + x as usize * 4;
            self.pixels[offset..offset + 4]
                .try_into()
                .expect("one RGBA pixel")
        }

        fn ink_bounds(&self) -> (i32, i32, i32, i32) {
            let mut bounds = (self.width, self.height, -1, -1);
            for y in 3..self.height - 3 {
                let left_ground = self.rgba_at(3, y);
                let right_ground = self.rgba_at(self.width - 4, y);
                for x in 4..self.width - 4 {
                    let fraction = f64::from(x - 3) / f64::from(self.width - 7);
                    let expected = std::array::from_fn::<_, 4, _>(|channel| {
                        (f64::from(left_ground[channel]) * (1.0 - fraction)
                            + f64::from(right_ground[channel]) * fraction)
                            .round() as u8
                    });
                    let pixel = self.rgba_at(x, y);
                    let distance: u16 = (0..3)
                        .map(|channel| u16::from(pixel[channel].abs_diff(expected[channel])))
                        .sum();
                    if distance > 36 {
                        bounds.0 = bounds.0.min(x);
                        bounds.1 = bounds.1.min(y);
                        bounds.2 = bounds.2.max(x);
                        bounds.3 = bounds.3.max(y);
                    }
                }
            }
            assert!(
                bounds.2 >= bounds.0,
                "the rendered tile contains no glyph ink"
            );
            bounds
        }
    }

    fn render_tile(window: &gtk4::Window, cover: &LazyReleaseCover) -> RenderedTile {
        let width = cover.root.width();
        let height = cover.root.height();
        let paintable = gtk4::WidgetPaintable::new(Some(&cover.root));
        let snapshot = gtk4::Snapshot::new();
        paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
        let node = snapshot.to_node().expect("the release tile paints a node");
        let renderer = window
            .native()
            .and_then(|native| native.renderer())
            .expect("the presented window has a renderer");
        let texture = renderer.render_texture(&node, None);
        let stride = texture.width() as usize * 4;
        let mut pixels = vec![0; stride * texture.height() as usize];
        texture.download(&mut pixels, stride);
        RenderedTile {
            width: texture.width(),
            height: texture.height(),
            stride,
            pixels,
        }
    }

    fn assert_rendered_ink_is_centered(artist: &str) -> RenderedTile {
        let (window, cover) = release_cover_in_column_view(artist);
        let tile = render_tile(&window, &cover);
        let (min_x, min_y, max_x, max_y) = tile.ink_bounds();
        let left = min_x;
        let right = tile.width - 1 - max_x;
        let top = min_y;
        let bottom = tile.height - 1 - max_y;
        assert!(
            (left - right).abs() <= 1 && (top - bottom).abs() <= 1,
            "{artist:?} ink is not optically centered: left={left}, right={right}, \
             top={top}, bottom={bottom}"
        );
        window.close();
        tile
    }

    struct FetchObserverGuard;

    impl Drop for FetchObserverGuard {
        fn drop(&mut self) {
            TEST_FETCH_OBSERVER.with(|slot| slot.replace(None));
        }
    }

    fn observe_fetches(observer: impl Fn(&str) + 'static) -> FetchObserverGuard {
        TEST_FETCH_OBSERVER.with(|slot| slot.replace(Some(std::rc::Rc::new(observer))));
        FetchObserverGuard
    }

    /// STYLE-10: `ColumnView` recycles row widgets, so a cover cell is bound
    /// to a second release without ever being constructed again.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_13_releases_cover_rebinds_when_the_row_changes() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cover = LazyReleaseCover::new_unbound(40);
        cover.set_release("11111111-1111-1111-1111-111111111111", "Falling Leaves");
        let first = cover.initials_text();
        cover.set_release("22222222-2222-2222-2222-222222222222", "Air");
        assert_ne!(cover.initials_text(), first, "the cell kept the old row");
        assert!(
            !cover.shows_image(),
            "a rebound cell must clear its picture"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn release_placeholder_initials_are_centered_with_the_app_css() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        crate::ui::style::install_css_string_for_test(&format!(
            "{}\n{}",
            crate::ui::style::theme::theme_css(
                crate::ui::style::theme::Theme::DEFAULT,
                false,
                crate::ui::style::accent::AccentSource::App,
            ),
            crate::ui::style::app_css_for_test(),
        ));
        let tile = assert_rendered_ink_is_centered("Mental Cruelty");
        assert_rendered_ink_is_centered("W");
        assert_ne!(
            tile.rgba_at(4, 4),
            tile.rgba_at(4, tile.height - 5),
            "the muted placeholder must visibly carry its vertical gradient"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn concert_artist_binding_uses_a_separate_key_without_a_release_fetch() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cover = LazyReleaseCover::new_unbound(56);

        cover.set_artist_key("Falling Leaves");

        assert_eq!(cover.artist_key(), "Falling Leaves");
        assert_eq!(cover.initials_text(), "FL");
        assert!(cover.mbid.text().is_empty());
        assert!(cover.started.text().is_empty());
        assert!(!cover.shows_image());
    }

    /// STYLE-10: `unbind`/`bind` can recycle one mapped cell for a different
    /// release without an intervening `unmap`/`map`. Bind time must therefore
    /// start the second release's fetch instead of waiting for a map that will
    /// never happen.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_13_releases_cover_fetches_again_when_rebound_without_unmap() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let fetches = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let recorded = fetches.clone();
        let _observer = observe_fetches(move |mbid| {
            recorded.borrow_mut().push(mbid.to_owned());
        });
        let cover = LazyReleaseCover::new_unbound(40);
        let first = "11111111-1111-1111-1111-111111111111";
        let second = "22222222-2222-2222-2222-222222222222";
        cover.set_release(first, "Falling Leaves");
        let window = gtk4::Window::new();
        window.set_child(Some(cover.widget()));
        window.present();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while fetches.borrow().is_empty() {
            while gtk4::glib::MainContext::default().iteration(false) {}
            assert!(
                std::time::Instant::now() < deadline,
                "the first map never started its fetch"
            );
        }
        cover.set_release(second, "Air");

        assert_eq!(fetches.borrow().as_slice(), [first, second]);
        assert!(cover.widget().is_mapped(), "the cell was never unmapped");
        window.close();
    }
}
