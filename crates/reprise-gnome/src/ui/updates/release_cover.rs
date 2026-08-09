//! Lazy release-group covers with an immediate, spinner-free fallback tile.

#![allow(dead_code)] // Shared by the Updates popover and recycled table cells.

use gtk4::prelude::*;

use crate::ui::{artist_avatar, one_shot_task};

const INITIALS_CLASS: &str = "reprise-release-cover-initials";
const PICTURE_CLASS: &str = "reprise-release-cover-picture";
const MBID_CLASS: &str = "reprise-release-cover-mbid";
const STARTED_CLASS: &str = "reprise-release-cover-started";

pub(in crate::ui) struct LazyReleaseCover {
    root: gtk4::Overlay,
    initials: gtk4::Label,
    picture: gtk4::Picture,
    mbid: gtk4::Label,
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

    /// Builds an empty cover suitable for a recycled `ColumnView` cell.
    pub(in crate::ui) fn new_unbound(edge: i32) -> Self {
        let root = gtk4::Overlay::new();
        root.set_size_request(edge, edge);
        root.add_css_class("new-release-cover");

        let background = gtk4::DrawingArea::new();
        background.set_content_width(edge);
        background.set_content_height(edge);
        background.set_draw_func(move |_, context, width, height| {
            let accent = crate::ui::style::accent::accent_rgba();
            context.set_source_rgb(
                f64::from(accent.red()),
                f64::from(accent.green()),
                f64::from(accent.blue()),
            );
            context.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
            let _ = context.fill();
        });
        root.set_child(Some(&background));

        let initials = gtk4::Label::new(None);
        initials.set_halign(gtk4::Align::Center);
        initials.set_valign(gtk4::Align::Center);
        initials.add_css_class("title-3");
        initials.add_css_class(INITIALS_CLASS);
        root.add_overlay(&initials);

        let picture = gtk4::Picture::new();
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_visible(false);
        picture.add_css_class(PICTURE_CLASS);
        root.add_overlay(&picture);

        let hairline = gtk4::DrawingArea::new();
        hairline.set_can_target(false);
        hairline.set_draw_func(|_, context, width, height| {
            context.set_source_rgba(1.0, 1.0, 1.0, 0.22);
            context.set_line_width(1.0);
            context.rectangle(
                0.5,
                0.5,
                f64::from(width).max(1.0) - 1.0,
                f64::from(height).max(1.0) - 1.0,
            );
            let _ = context.stroke();
        });
        root.add_overlay(&hairline);

        // GTK owns these two invisible labels with the cell, so a wrapper
        // reconstructed during bind reaches the same per-cell async state
        // without unsafe qdata or a second GObject subclass.
        let mbid = state_label(MBID_CLASS);
        let started = state_label(STARTED_CLASS);
        root.add_overlay(&mbid);
        root.add_overlay(&started);

        wire_lazy_fetch(&root, &picture, &mbid, &started);
        Self {
            root,
            initials,
            picture,
            mbid,
            started,
        }
    }

    pub(in crate::ui) fn from_widget(root: &gtk4::Overlay) -> Option<Self> {
        Some(Self {
            root: root.clone(),
            initials: child_with_class(root, INITIALS_CLASS)?,
            picture: child_with_class(root, PICTURE_CLASS)?,
            mbid: child_with_class(root, MBID_CLASS)?,
            started: child_with_class(root, STARTED_CLASS)?,
        })
    }

    pub(in crate::ui) fn set_release(&self, release_group_mbid: &str, artist: &str) {
        self.mbid.set_text(release_group_mbid);
        self.started.set_text("");
        self.initials.set_text(&artist_avatar::initials(artist));
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

    pub(in crate::ui) fn widget(&self) -> &gtk4::Overlay {
        &self.root
    }

    #[cfg(test)]
    fn initials_text(&self) -> String {
        self.initials.text().to_string()
    }

    #[cfg(test)]
    fn shows_image(&self) -> bool {
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
    fn style_10_releases_cover_rebinds_when_the_row_changes() {
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

    /// STYLE-10: `unbind`/`bind` can recycle one mapped cell for a different
    /// release without an intervening `unmap`/`map`. Bind time must therefore
    /// start the second release's fetch instead of waiting for a map that will
    /// never happen.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_10_releases_cover_fetches_again_when_rebound_without_unmap() {
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
