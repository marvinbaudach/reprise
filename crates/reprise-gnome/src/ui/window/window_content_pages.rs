//! Construction of the content stack's pages.
//!
//! B1 turned the expensive ones into `DeferredPage`s and gave every
//! construction site a measurement span, which is a lot of shape for something
//! `window.rs` only needs to name. The composition root asks for the pages
//! here; how each one is built, and which of them is deferred, lives in this
//! file.
//!
//! The two entry points are deliberately separate rather than one call: the
//! library pages are installed before `library_shell::build`, the event pages
//! after it, and `startup_report` records that order.

use std::path::Path;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;

use super::super::artist_portrait_worker::ArtistPortraitRuntime;
use super::super::concerts::{self, ConcertsRuntime, ConcertsView};
use super::super::cover_loader::CoverLoader;
use super::super::location_broadcast::LocationBroadcast;
use super::super::releases::ReleasesView;
use super::super::stats_view::StatsView;
use super::content_stack::DeferredPage;

/// The pages that exist before the library shell is built: the library itself,
/// the deferred statistics view and the Library Doctor's navigation host.
///
/// The library page is added first and made visible immediately — it is the
/// opening page, so deferring it would only move its cost behind first paint
/// without removing it.
pub(super) fn install_library_pages(
    content_stack: &gtk4::Stack,
    track_content: &impl IsA<gtk4::Widget>,
    cover_loader: Rc<CoverLoader>,
    artist_portrait: Rc<ArtistPortraitRuntime>,
    conn: &Rc<Db>,
) -> (DeferredPage<StatsView>, adw::NavigationView) {
    {
        let _measurement = super::startup_report::measure("view.library.add-named");
        content_stack.add_named(track_content, Some("library"));
    }
    content_stack.set_visible_child_name("library");
    let stats_view = DeferredPage::install(content_stack, "stats", {
        let conn = conn.clone();
        move || {
            let _measurement = super::startup_report::measure("view.stats.construct");
            let view = Rc::new(StatsView::new(cover_loader));
            view.set_portrait_runtime(artist_portrait);
            view.wire_year_selector(&conn);
            let root = view.widget().clone();
            (view, root.upcast())
        }
    });
    super::startup_report::mark("stats");
    // Size to the visible page in both axes: dedicated content pages must not
    // inherit the library's minimum size, nor vice versa.
    let library_doctor_navigation = {
        let _measurement = super::startup_report::measure("view.library-doctor.construct");
        adw::NavigationView::new()
    };
    {
        let _measurement = super::startup_report::measure("view.library-doctor.add-named");
        content_stack.add_named(&library_doctor_navigation, Some("library-doctor"));
    }
    (stats_view, library_doctor_navigation)
}

/// Concerts and Releases, installed after the library shell exists.
///
/// Concerts is deferred; Releases is not. The startup table in
/// `docs/measurements/content-stack-startup.md` is what decided that — Releases
/// costs about 3.8 ms to build and is cheap enough to keep eager.
pub(super) fn install_event_pages(
    content_stack: &gtk4::Stack,
    conn: &Rc<Db>,
    db_path: &Path,
    concerts_runtime: &Rc<ConcertsRuntime>,
    location_broadcast: &Rc<LocationBroadcast>,
    cover_loader: Rc<CoverLoader>,
    artist_portrait: Rc<ArtistPortraitRuntime>,
) -> (DeferredPage<ConcertsView>, Rc<ReleasesView>) {
    let releases_cover_loader = cover_loader.clone();
    let releases_artist_portrait = artist_portrait.clone();
    let concerts_view = DeferredPage::install(content_stack, "concerts", {
        let conn = conn.clone();
        let concerts_runtime = concerts_runtime.clone();
        let location_broadcast = location_broadcast.clone();
        move || {
            let _measurement = super::startup_report::measure("view.concerts.construct");
            let view = Rc::new(concerts::install(
                conn,
                &concerts_runtime,
                &location_broadcast,
            ));
            view.set_artist_image(cover_loader, artist_portrait);
            let root = view.root().clone();
            (view, root)
        }
    });
    super::startup_report::mark("concerts");
    let releases_view = {
        let _measurement = super::startup_report::measure("view.releases.construct");
        Rc::new(super::super::releases::install(
            conn.clone(),
            db_path.to_path_buf(),
        ))
    };
    releases_view.set_artist_image(releases_cover_loader, releases_artist_portrait);
    super::startup_report::mark("releases");
    {
        let _measurement = super::startup_report::measure("view.releases.add-named");
        content_stack.add_named(releases_view.root(), Some("releases"));
    }
    (concerts_view, releases_view)
}

/// Routes both event pages' launch failures to the window's toast overlay.
///
/// Concerts is deferred, so its callback is registered with the page rather
/// than the view: `on_materialized` runs it before a synchronous navigation
/// returns, which is the only way the callback can be in place by the time the
/// page is on screen. Releases is eager and takes it directly.
pub(super) fn wire_launch_errors(
    toast_overlay: &adw::ToastOverlay,
    concerts_view: &DeferredPage<ConcertsView>,
    releases_view: &Rc<ReleasesView>,
) {
    {
        let overlay = toast_overlay.downgrade();
        concerts_view.on_materialized(move |concerts| {
            concerts.set_on_launch_error(move |error| {
                if let Some(overlay) = overlay.upgrade() {
                    super::super::toasts::show(&overlay, &error);
                }
            });
        });
    }
    let overlay = toast_overlay.downgrade();
    releases_view.set_on_launch_error(move |error| {
        if let Some(overlay) = overlay.upgrade() {
            super::super::toasts::show(&overlay, &error);
        }
    });
}
