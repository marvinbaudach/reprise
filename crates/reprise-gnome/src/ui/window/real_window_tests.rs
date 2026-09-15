//! Display tests against the exact main-window composition used by the app.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::NavigationPageExt;

use super::window_layout_test_hook::WindowLayoutTestHandles;

static NEXT_APPLICATION_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default)]
struct SidebarSeed {
    issue_rows: usize,
    running_cards: usize,
    device: bool,
    track_rows: usize,
}

fn diagnostic_job_card(index: usize) -> gtk4::Revealer {
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header.append(&gtk4::Spinner::new());
    header.append(
        &gtk4::Label::builder()
            .label(format!("Checking tracks {index}…"))
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .css_classes(["scan-card-title"])
            .build(),
    );
    header.append(&gtk4::Label::new(Some("45%")));
    header.append(&gtk4::Button::with_label("Cancel"));
    let progress = gtk4::ProgressBar::new();
    progress.set_height_request(3);
    let detail = gtk4::Label::builder()
        .label("742/1,648 tracks")
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .css_classes(["scan-card-detail"])
        .build();
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    body.add_css_class("scan-card");
    body.append(&header);
    body.append(&progress);
    body.append(&detail);
    gtk4::Revealer::builder()
        .transition_type(gtk4::RevealerTransitionType::None)
        .child(&body)
        .build()
}

fn seed_tracks(db: &reprise_core::db::Db, count: usize) {
    let connection = crate::test_db::connection(db);
    for index in 0..count {
        connection
            .execute(
                "INSERT INTO tracks (path, title, artist, album, year, duration_ms, added_at) \
                 VALUES (?1, ?2, 'The Artist', 'The Album', 2025, 754000, 0)",
                rusqlite::params![
                    format!("/test/real-window-{index}.flac"),
                    format!("Track {index}")
                ],
            )
            .expect("seed a real-window track");
    }
}

fn seed_sidebar(handles: &WindowLayoutTestHandles, seed: SidebarSeed) {
    for index in 0..seed.issue_rows {
        let row = crate::ui::sidebar_presentation::build_issue_nav_row(
            &format!("Issue {}", index + 1),
            crate::ui::sidebar_presentation::issue_row_presentation(
                1,
                crate::ui::sidebar_presentation::NavIcon::Missing,
            ),
            crate::ui::sidebar_presentation::NavIcon::Missing,
        );
        handles.issues_listbox.append(&row);
    }
    handles.issues_listbox.set_visible(seed.issue_rows > 0);

    for index in 0..seed.running_cards.min(3) {
        let card = diagnostic_job_card(index + 1);
        match index {
            0 => handles.sidebar.append_doctor_card(&card),
            1 => handles.sidebar.append_relink_card(&card),
            _ => handles.sidebar.append_scan_card(&card),
        }
        card.set_reveal_child(true);
    }
    if seed.device {
        handles.sidebar.present_device_for_layout_test();
    }
}

fn build_real_window(width: i32, height: i32, seed: SidebarSeed) -> WindowLayoutTestHandles {
    gtk4::init().expect("GTK test display");
    let sequence = NEXT_APPLICATION_ID.fetch_add(1, Ordering::Relaxed);
    let app = adw::Application::builder()
        .application_id(format!("de.reprise.Reprise.RealWindowTest{sequence}"))
        .build();
    app.register(None::<&gio::Cancellable>)
        .expect("register test application");
    let db = std::rc::Rc::new(crate::test_db::open().expect("open test database"));
    seed_tracks(&db, seed.track_rows);
    let state = reprise_core::library::session::SessionState {
        window_width: width,
        window_height: height,
        maximized: false,
        ..Default::default()
    };
    reprise_core::library::session::save(&db, &state).expect("save test window geometry");
    let db_path = db.path().expect("file-backed test database").to_path_buf();
    let _handler = super::surface::build(
        &app,
        &db,
        &db_path,
        crate::ui::file_open::StartupOpenIntent::Library,
    );
    let handles = super::window_layout_test_hook::take()
        .expect("window composition publishes layout test handles");
    seed_sidebar(&handles, seed);
    handles.split_view.set_show_sidebar(true);
    // The isolated display runner deliberately has no window manager. GTK's
    // client-side shadow consumes five pixels on every edge there, so pin the
    // surface ten pixels larger and assert against the window widget's actual
    // allocation, as the SET-19 production-geometry test does.
    handles.window.set_size_request(width + 10, height + 10);

    let deadline = Instant::now() + Duration::from_secs(5);
    while (!handles.window.is_mapped()
        || handles.window.width() != width
        || handles.window.height() != height)
        && Instant::now() < deadline
    {
        gtk4::glib::MainContext::default().iteration(true);
    }
    assert!(handles.window.is_mapped(), "the production window must map");
    assert_eq!(
        (handles.window.width(), handles.window.height()),
        (width, height)
    );
    gtk4::glib::MainContext::default()
        .block_on(gtk4::glib::timeout_future(Duration::from_millis(50)));
    if seed.track_rows > 0 {
        let deadline = Instant::now() + Duration::from_secs(5);
        while handles
            .column_view
            .model()
            .is_none_or(|model| model.n_items() < seed.track_rows as u32)
            && Instant::now() < deadline
        {
            gtk4::glib::MainContext::default().iteration(true);
        }
        assert_eq!(
            handles.column_view.model().map(|model| model.n_items()),
            Some(seed.track_rows as u32),
            "the production table must load every seeded track"
        );
        gtk4::glib::MainContext::default()
            .block_on(gtk4::glib::timeout_future(Duration::from_millis(50)));
        while gtk4::glib::MainContext::default().iteration(false) {}
    }
    handles.window.set_size_request(
        super::window_bootstrap::MIN_WIDTH,
        super::window_bootstrap::MIN_HEIGHT,
    );
    handles
}

fn chain_report(handles: &WindowLayoutTestHandles) -> String {
    let widgets: [(&str, gtk4::Widget); 10] = [
        ("window", handles.window.clone().upcast()),
        (
            "player shell",
            handles.player_bar_shell.widget().clone().upcast(),
        ),
        ("split", handles.split_view.clone().upcast()),
        ("sidebar page", handles.sidebar_page.clone().upcast()),
        (
            "sidebar scroller",
            handles.navigation_scroller.clone().upcast(),
        ),
        ("pinned block", handles.pinned_block.clone()),
        (
            "player bar",
            handles.player_bar.clone().expect("player bar"),
        ),
        ("content navigation", handles.content_nav.clone().upcast()),
        ("track scroller", handles.track_scrolled.clone().upcast()),
        ("column view", handles.column_view.clone().upcast()),
    ];
    let mut report = String::new();
    for (name, widget) in widgets {
        let bounds = widget.compute_bounds(&handles.window);
        let vertical = widget.measure(gtk4::Orientation::Vertical, widget.width());
        let horizontal = widget.measure(gtk4::Orientation::Horizontal, widget.height());
        report.push_str(&format!(
            "{name}: {} {} bounds={bounds:?} vmeasure={vertical:?} hmeasure={horizontal:?} vexpand={} valign={:?}\n",
            widget.css_name(),
            widget.type_().name(),
            widget.vexpands(),
            widget.valign(),
        ));
    }
    report
}

fn bottom_in(widget: &gtk4::Widget, ancestor: &impl IsA<gtk4::Widget>) -> f32 {
    let bounds = widget
        .compute_bounds(ancestor)
        .expect("the widget is allocated in its ancestor");
    bounds.y() + bounds.height()
}

fn assert_within_one(left: f32, right: f32, message: &str, report: &str) {
    assert!(
        (left - right).abs() <= 1.0,
        "{message}: left={left}, right={right}\n{report}"
    );
}

fn descendants(root: &impl IsA<gtk4::Widget>) -> Vec<gtk4::Widget> {
    let mut found = Vec::new();
    let mut pending = Vec::new();
    let mut root_child = root.first_child();
    while let Some(current) = root_child {
        pending.push(current.clone());
        root_child = current.next_sibling();
    }
    while let Some(widget) = pending.pop() {
        let mut child = widget.first_child();
        while let Some(current) = child {
            pending.push(current.clone());
            child = current.next_sibling();
        }
        found.push(widget);
    }
    found
}

fn table_report(handles: &WindowLayoutTestHandles) -> String {
    let mut report = chain_report(handles);
    let adjustment = handles.track_scrolled.hadjustment();
    report.push_str(&format!(
        "table adjustment: value={} upper={} page_size={} scroller_width={}\n",
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size(),
        handles.track_scrolled.width(),
    ));
    let column_view = handles.column_view.clone().upcast::<gtk4::Widget>();
    for widget in descendants(&handles.column_view) {
        if widget.is_visible()
            && (widget.type_().name() == "GtkColumnViewRowWidget"
                || widget.has_css_class("reprise-track-cell")
                || (widget.css_name() == "button"
                    && has_css_ancestor(&widget, "header", &column_view)))
        {
            report.push_str(&format!(
                "table child: {} {} classes={:?} bounds={:?}\n",
                widget.css_name(),
                widget.type_().name(),
                widget.css_classes(),
                widget.compute_bounds(&handles.track_scrolled),
            ));
        }
    }
    report
}

fn has_css_ancestor(widget: &gtk4::Widget, css_name: &str, stop: &gtk4::Widget) -> bool {
    let mut parent = widget.parent();
    while let Some(current) = parent {
        if current.css_name() == css_name {
            return true;
        }
        if current == *stop {
            break;
        }
        parent = current.parent();
    }
    false
}

fn assert_inside_table_viewport(
    widget: &gtk4::Widget,
    handles: &WindowLayoutTestHandles,
    report: &str,
) {
    let bounds = widget
        .compute_bounds(&handles.track_scrolled)
        .expect("visible table content shares the viewport coordinate space");
    let right = handles.track_scrolled.hadjustment().page_size() as f32;
    assert!(
        bounds.x() >= -1.0 && bounds.x() + bounds.width() <= right + 1.0,
        "{} {} lies outside the table viewport: bounds={bounds:?}, right={right}\n{report}",
        widget.css_name(),
        widget.type_().name(),
    );
}

fn assert_pinned_essentials_visible(
    handles: &WindowLayoutTestHandles,
    pinned: &gtk4::ScrolledWindow,
    report: &str,
) {
    let region = pinned.child().expect("the pinned region exists");
    let issues = region.first_child().expect("the issues block exists");
    let heading = issues.first_child().expect("the ISSUES heading exists");
    let first_issue = handles
        .issues_listbox
        .row_at_index(0)
        .expect("the first issue row exists");
    let first_card = std::iter::successors(
        handles.activity_slot.first_child(),
        gtk4::prelude::WidgetExt::next_sibling,
    )
    .find(|child| child.is_visible() && child.is::<gtk4::Revealer>())
    .expect("the first running card is visible");
    for (name, widget) in [
        ("ISSUES heading", &heading),
        ("first issue row", first_issue.upcast_ref::<gtk4::Widget>()),
        ("first running card", &first_card),
    ] {
        let bounds = widget.compute_bounds(pinned).unwrap();
        assert!(
            bounds.y() >= 0.0 && bounds.y() + bounds.height() <= pinned.height() as f32,
            "the {name} must be visible at the enforced minimum\n{report}"
        );
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_8_the_real_sidebar_leaves_no_band_under_the_pinned_block() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_real_window(
        1280,
        720,
        SidebarSeed {
            issue_rows: 1,
            running_cards: 1,
            device: true,
            ..Default::default()
        },
    );
    let report = chain_report(&handles);
    let sidebar = handles.sidebar_page.upcast_ref::<gtk4::Widget>();
    let split = handles.split_view.upcast_ref::<gtk4::Widget>();
    let shell = handles
        .player_bar_shell
        .widget()
        .upcast_ref::<gtk4::Widget>();
    let scrolled = handles.navigation_scroller.upcast_ref::<gtk4::Widget>();
    let pinned = &handles.pinned_block;
    let bar = handles
        .player_bar
        .as_ref()
        .expect("the player bar is available");

    let scrolled_bounds = scrolled.compute_bounds(sidebar).unwrap();
    let pinned_bounds = pinned.compute_bounds(sidebar).unwrap();
    assert_within_one(
        scrolled_bounds.y() + scrolled_bounds.height(),
        pinned_bounds.y(),
        "the navigation scroller must meet the pinned block",
        &report,
    );
    assert_within_one(
        pinned_bounds.y() + pinned_bounds.height(),
        sidebar.height() as f32,
        "the pinned block must meet the sidebar-page bottom",
        &report,
    );
    assert_within_one(
        bottom_in(sidebar, shell),
        bottom_in(split, shell),
        "the sidebar page and split view must share a bottom edge",
        &report,
    );
    assert_within_one(
        bottom_in(split, shell),
        bar.compute_bounds(shell).unwrap().y(),
        "the split view must meet the player bar",
        &report,
    );
    assert_within_one(
        scrolled_bounds.height(),
        sidebar.height() as f32 - pinned_bounds.height(),
        "the scroller must receive all sidebar height not painted by the pinned block",
        &report,
    );

    handles.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_15_three_running_cards_never_raise_the_window_minimum() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_real_window(
        super::window_bootstrap::MIN_WIDTH,
        super::window_bootstrap::MIN_HEIGHT,
        SidebarSeed {
            issue_rows: 3,
            running_cards: 3,
            device: true,
            ..Default::default()
        },
    );
    let report = chain_report(&handles);
    let pinned = handles
        .pinned_block
        .clone()
        .downcast::<gtk4::ScrolledWindow>()
        .expect("the pinned block yields through its own scroller");
    let pinned_bounds = pinned.compute_bounds(&handles.sidebar_page).unwrap();
    let library_floor = handles
        .sidebar
        .shared
        .listbox
        .row_at_index(5)
        .expect("the Library block has a heading and five rows");
    let library_bounds = library_floor
        .compute_bounds(&handles.sidebar_page)
        .expect("the Library floor is allocated");
    let vertical_minimum = handles
        .window
        .measure(
            gtk4::Orientation::Vertical,
            super::window_bootstrap::MIN_WIDTH,
        )
        .0;
    assert!(
        vertical_minimum <= super::window_bootstrap::MIN_HEIGHT,
        "running cards must not raise the real-window minimum: {vertical_minimum}\n{report}"
    );
    assert!(
        library_bounds.y() + library_bounds.height() <= pinned_bounds.y() + 1.0,
        "the complete Library floor must remain above the yielding block: row={library_bounds:?}, pinned={pinned_bounds:?}\n{report}"
    );
    let bar = handles.player_bar.as_ref().unwrap();
    let bar_bounds = bar.compute_bounds(&handles.window).unwrap();
    assert!(
        bar_bounds.y() >= 0.0
            && bar_bounds.y() + bar_bounds.height() <= handles.window.height() as f32,
        "the player bar must remain whole\n{report}"
    );
    assert!(
        pinned.vadjustment().upper() > pinned.vadjustment().page_size(),
        "three running cards must scroll inside the pinned block\n{report}"
    );
    handles.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_5_the_real_window_holds_its_player_bar_at_the_minimum() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_real_window(
        super::window_bootstrap::MIN_WIDTH,
        super::window_bootstrap::MIN_HEIGHT,
        SidebarSeed {
            issue_rows: 1,
            running_cards: 1,
            ..Default::default()
        },
    );
    let report = chain_report(&handles);
    assert_eq!(
        (handles.window.width(), handles.window.height()),
        (
            super::window_bootstrap::MIN_WIDTH,
            super::window_bootstrap::MIN_HEIGHT
        ),
        "the production window must receive the enforced minimum\n{report}"
    );
    let vertical_minimum = handles
        .window
        .measure(
            gtk4::Orientation::Vertical,
            super::window_bootstrap::MIN_WIDTH,
        )
        .0;
    let horizontal_minimum = handles
        .window
        .measure(
            gtk4::Orientation::Horizontal,
            super::window_bootstrap::MIN_HEIGHT,
        )
        .0;
    assert!(
        vertical_minimum > super::window_bootstrap::MIN_HEIGHT - 10
            && vertical_minimum <= super::window_bootstrap::MIN_HEIGHT,
        "the measured minimum {vertical_minimum} must round up to the next ten\n{report}"
    );
    assert!(
        horizontal_minimum <= super::window_bootstrap::MIN_WIDTH,
        "the real horizontal minimum is {horizontal_minimum}\n{report}"
    );
    let bar = handles.player_bar.as_ref().unwrap();
    let bar_bounds = bar.compute_bounds(&handles.window).unwrap();
    let bar_natural = bar.measure(gtk4::Orientation::Vertical, bar.width()).1;
    assert!(
        bar_bounds.y() >= 0.0
            && bar_bounds.y() + bar_bounds.height() <= handles.window.height() as f32
            && bar_bounds.height() >= bar_natural as f32,
        "the structural player bar must keep its natural height\n{report}"
    );
    for (name, column) in [
        ("sidebar", handles.sidebar_page.upcast_ref::<gtk4::Widget>()),
        ("content", handles.content_nav.upcast_ref::<gtk4::Widget>()),
    ] {
        let bounds = column.compute_bounds(&handles.split_view).unwrap();
        let minimum = column
            .measure(gtk4::Orientation::Horizontal, column.height())
            .0;
        assert!(
            minimum <= bounds.width() as i32,
            "the {name} column minimum {minimum} exceeds its {} px allocation\n{report}",
            bounds.width()
        );
    }
    let pinned = handles
        .pinned_block
        .clone()
        .downcast::<gtk4::ScrolledWindow>()
        .unwrap();
    assert_pinned_essentials_visible(&handles, &pinned, &report);
    handles.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_6_the_real_table_never_overflows_at_1280() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_real_window(
        1280,
        720,
        SidebarSeed {
            track_rows: 3,
            ..Default::default()
        },
    );
    let report = table_report(&handles);
    let adjustment = handles.track_scrolled.hadjustment();
    assert_within_one(
        adjustment.upper() as f32,
        adjustment.page_size() as f32,
        "the table must not acquire horizontal overflow",
        &report,
    );

    let column_view = handles.column_view.clone().upcast::<gtk4::Widget>();
    let visible = descendants(&handles.column_view)
        .into_iter()
        .filter(gtk4::prelude::WidgetExt::is_visible)
        .collect::<Vec<_>>();
    let headers = visible
        .iter()
        .filter(|widget| has_css_ancestor(widget, "header", &column_view))
        .filter(|widget| widget.css_name() == "button")
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        !headers.is_empty(),
        "the real table exposes its headers\n{report}"
    );
    for header in headers {
        assert_inside_table_viewport(&header, &handles, &report);
    }

    let mut rows = visible
        .iter()
        .filter(|widget| {
            widget.type_().name() == "GtkColumnViewRowWidget" && widget.css_name() != "header"
        })
        .filter_map(|widget| {
            widget
                .compute_bounds(&handles.track_scrolled)
                .map(|bounds| (bounds.y(), widget.clone()))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.0.total_cmp(&right.0));
    let last_row = rows.last().map_or_else(
        || panic!("the real table paints its seeded rows\n{report}"),
        |(_, row)| row,
    );
    let last_row_cells = visible
        .iter()
        .filter(|widget| widget.has_css_class("reprise-track-cell"))
        .filter(|widget| widget.is_ancestor(last_row))
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        !last_row_cells.is_empty(),
        "the last row exposes app-owned cells\n{report}"
    );
    for cell in last_row_cells {
        assert_inside_table_viewport(&cell, &handles, &report);
    }
    for text in ["12:34", "2025"] {
        let label = visible
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk4::Label>())
            .find(|label| label.text() == text)
            .unwrap_or_else(|| panic!("the real table paints {text}\n{report}"));
        assert_inside_table_viewport(label.upcast_ref(), &handles, &report);
    }
    // GtkColumnView owns private row widgets. Release the strong references
    // collected for geometry before closing the production window so GTK can
    // tear its accessibility relations down in ownership order.
    drop(rows);
    drop(visible);
    handles.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_real_window_test_instrument_publishes_the_production_surface() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_real_window(800, 600, SidebarSeed::default());

    assert_eq!(
        handles.split_view.parent(),
        Some(handles.player_bar_shell.widget().clone().upcast())
    );
    assert!(handles.player_bar.is_some());
    assert_eq!(
        handles.navigation_scroller.parent(),
        Some(handles.sidebar.widget().clone().upcast())
    );
    assert_eq!(
        handles.pinned_block.parent(),
        Some(handles.sidebar.widget().clone().upcast())
    );
    assert!(handles.activity_slot.is_ancestor(&handles.pinned_block));
    assert_eq!(
        handles.content_nav.parent(),
        Some(handles.split_view.clone().upcast())
    );
    assert_eq!(
        handles.column_view.parent(),
        Some(handles.track_scrolled.clone().upcast())
    );
    assert_eq!(
        handles.sidebar_page.child(),
        Some(handles.sidebar.widget().clone().upcast())
    );

    handles.window.close();
}
