//! Isolated display evidence for where background work is allowed to appear in
//! the Preferences dialog.
//!
//! `SET-18`: the dialog head carries the title and the search, and nothing —
//! toast, banner or progress — is hung into it or laid over it. Running jobs
//! get a fixed bottom bar instead. Four things have to be proven, not asserted:
//! that the footer really is *below* the head and leaves the page where it was;
//! the counterprobe, that the in-flow arrangement this replaced really did
//! shove the page down, so the footer is solving a real problem; that the
//! footer costs the dialog height and never width; and that its fixed columns
//! leave the description room to state its count instead of truncating it.
//!
//! The last two are computed from what each widget asks for, not read off an
//! allocation: `xvfb-run` comes up 640x480 here, narrower than the dialog, so
//! no display test ever sees the authored width allocated. The measurement tool
//! at the end prints the same balance sheet when a column width is in question.

use std::path::PathBuf;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::scanner::ScanProgress;

use super::tests::{settle_for, settle_layout, test_pages};
use super::*;
use crate::ui::preference_background_bar::{BackgroundBar, JobOwner, JobRowState};
use crate::ui::scan_progress::ScanProgressView;

/// Floor for the counterprobe's measured displacement. The scan card really
/// occupies about 62 px under the app stylesheet at Adwaita's default font
/// metrics. The Library Doctor card-family restyle removed its 8 px top margin
/// and compacted the shared scan-card style; the floor keeps the same 8 px
/// allowance for font metrics. What it has to prove is a whole card's worth of
/// displacement, not one exact pixel count.
const RETIRED_TOP_BAR_MIN_JUMP_PX: f32 = 54.0;

fn artwork_job_row() -> JobRowState {
    JobRowState {
        owner: JobOwner::Artwork,
        detail: "Album covers · 1942 of 2132".to_owned(),
        fraction: 0.91,
    }
}

fn lyrics_job_row() -> JobRowState {
    JobRowState {
        owner: JobOwner::OnlineLyrics,
        detail: "Missing lyrics · 261 of 2132".to_owned(),
        fraction: 0.12,
    }
}

/// A presented preferences dialog with its background-activity footer.
struct FooterDialog {
    parent: adw::ApplicationWindow,
    bar: BackgroundBar,
    shell: PreferencesShell,
}

impl FooterDialog {
    fn present(application_id: &str) -> Self {
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id(application_id)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let parent = adw::ApplicationWindow::new(&app);
        parent.set_default_size(900, 760);
        parent.present();
        crate::ui::style::install();
        let bar = BackgroundBar::new();
        let shell = build(test_pages(), Some(bar.widget()));
        shell.dialog.present(Some(&parent));
        settle_layout();
        Self { parent, bar, shell }
    }

    fn header(&self) -> adw::HeaderBar {
        self.shell.content_header.clone()
    }

    fn origin_y(&self, widget: &gtk4::Widget) -> f32 {
        widget
            .compute_point(
                &self.shell.root_overlay,
                &gtk4::graphene::Point::new(0.0, 0.0),
            )
            .expect("the widget must be allocated inside the dialog")
            .y()
    }

    fn close(self) {
        self.shell.dialog.force_close();
        self.parent.close();
    }
}

/// The counterprobe for the whole feature: the arrangement this branch retired
/// really did shove the page down when a scan started, so the chrome that
/// replaced it is solving a real problem.
///
/// It is the *real* widget, not a stand-in. `ScanProgressView` is still alive
/// in the main window's sidebar, so the retired arrangement can be rebuilt from
/// it exactly as the dialog used to mount it — the revealer handed to
/// `AdwToolbarView::add_top_bar` as a second top bar — and driven through its
/// real `show` API with a real `ScanProgress`. Only the test builds this;
/// production must never parent it here again.
///
/// The measurement is taken after a plain main-context drain, while the card's
/// crossfade has not yet revealed it. That is the mechanism: a crossfade
/// animates opacity, so the card claims its full height from the very first
/// layout pass, long before anyone can see it.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_9_counterprobe_legacy_toolbar_status_moves_the_content() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    // The card is styled by the app stylesheet; an unstyled probe would measure
    // a card the user never sees.
    crate::ui::style::install();
    let legacy_status = ScanProgressView::new();
    let header = adw::HeaderBar::new();
    let content = gtk4::Label::new(Some("First content element"));
    content.set_valign(gtk4::Align::Start);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.add_top_bar(legacy_status.widget());
    toolbar.set_content(Some(&content));
    let window = gtk4::Window::builder()
        .default_width(760)
        .default_height(680)
        .child(&toolbar)
        .build();
    window.present();
    settle_layout();
    let content_y = || {
        content
            .compute_point(&window, &gtk4::graphene::Point::new(0.0, 0.0))
            .expect("the content must be allocated below the toolbar's top bars")
            .y()
    };
    let idle_y = content_y();
    assert_eq!(
        legacy_status.widget().height(),
        0,
        "a dormant scan card must reserve nothing, or there is no jump to measure"
    );

    legacy_status.show(&ScanProgress::Scanning {
        processed: 39,
        total: Some(100),
        current_path: PathBuf::from("/music/track.flac"),
    });
    while gtk4::glib::MainContext::default().iteration(false) {}
    let jump = content_y() - idle_y;
    let card_height = legacy_status.widget().height() as f32;

    assert!(
        !legacy_status.widget().is_child_revealed(),
        "the card must still be mid-crossfade, so the jump is proven to precede it"
    );
    assert_eq!(
        jump, card_height,
        "the content must move by exactly the height the retired card claims"
    );
    assert!(
        jump >= RETIRED_TOP_BAR_MIN_JUMP_PX,
        "the retired in-flow status path must reproduce its layout jump \
         (measured {jump} px, floor {RETIRED_TOP_BAR_MIN_JUMP_PX} px)"
    );

    window.close();
}

/// The head is the title's, on both axes: the footer is neither a descendant of
/// the header nor allocated across it, and the page's own top edge stays put
/// when jobs start.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_18_the_footer_sits_below_the_head_and_leaves_the_page_alone() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let dialog =
        FooterDialog::present("io.github.marvinbaudach.Reprise.PreferencesFooterPlacementTest");
    let header = dialog.header();
    let page_top = dialog.origin_y(dialog.shell.stack.upcast_ref());
    let title_top = dialog.origin_y(dialog.shell.content_title.upcast_ref());

    dialog
        .bar
        .publish(JobOwner::Artwork, Some(artwork_job_row()));
    dialog
        .bar
        .publish(JobOwner::OnlineLyrics, Some(lyrics_job_row()));
    settle_layout();

    assert!(
        !dialog.bar.widget().is_ancestor(&header),
        "the footer must never be parented into the head"
    );
    assert_eq!(
        dialog.origin_y(dialog.shell.content_title.upcast_ref()),
        title_top,
        "the title must not move when work starts"
    );
    assert_eq!(
        dialog.origin_y(dialog.shell.stack.upcast_ref()),
        page_top,
        "a bottom bar must not push the page down the way a top bar did"
    );
    let header_bottom = dialog.origin_y(header.upcast_ref()) + header.height() as f32;
    assert!(
        dialog.origin_y(dialog.bar.widget()) >= header_bottom,
        "the footer is below the head, never over it"
    );

    dialog.close();
}

/// The footer is fixed: it does not scroll with the page and it does not move
/// when the number of jobs changes — the label stays where the eye left it.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_18_the_footer_does_not_scroll_with_the_page() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let dialog =
        FooterDialog::present("io.github.marvinbaudach.Reprise.PreferencesFooterFixedTest");
    dialog
        .bar
        .publish(JobOwner::Artwork, Some(artwork_job_row()));
    settle_layout();
    let footer_bottom = dialog.origin_y(dialog.bar.widget()) + dialog.bar.widget().height() as f32;

    let Some(scroll) = find_scrolled_window(dialog.shell.stack.upcast_ref()) else {
        dialog.close();
        panic!("a settings page must live inside a scrolled window");
    };
    scroll.vadjustment().set_value(scroll.vadjustment().upper());
    settle_layout();

    assert_eq!(
        dialog.origin_y(dialog.bar.widget()) + dialog.bar.widget().height() as f32,
        footer_bottom,
        "scrolling the page must not move the footer"
    );

    dialog.close();
}

fn find_scrolled_window(root: &gtk4::Widget) -> Option<gtk4::ScrolledWindow> {
    if let Ok(found) = root.clone().downcast::<gtk4::ScrolledWindow>() {
        return Some(found);
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_scrolled_window(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_18_background_activity_never_reaches_the_dialog_head() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.PreferencesFooterGeometryTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let parent = adw::ApplicationWindow::new(&app);
    parent.set_default_size(900, 760);
    parent.present();
    crate::ui::style::install();
    let bar = BackgroundBar::new();
    let shell = super::build(test_pages(), Some(bar.widget()));
    shell.dialog.present(Some(&parent));
    settle_layout();

    let header_height = shell.content_header.height();

    bar.publish(JobOwner::Artwork, Some(artwork_job_row()));
    bar.publish(JobOwner::OnlineLyrics, Some(lyrics_job_row()));
    settle_layout();

    // The footer may cost the dialog height. It must never cost it width:
    // a wider dialog is a wider head, and a centred title visibly slides
    // sideways the moment a job starts.
    //
    // Measured, not observed. Watching the title's x would report the
    // *test window's* size instead of the rule: with no window manager
    // `set_default_size` does not take, the parent came up 630 px wide
    // rather than 900, and the dialog therefore never reached the 760 px
    // it asks for. Every widget below it was then squeezed by the harness,
    // which is why that assertion was green alone and red in the suite,
    // and by a different number each run. The minimum width does not care
    // how large the window happens to be.
    let (running_min_width, ..) = shell
        .root_overlay
        .measure(gtk4::Orientation::Horizontal, -1);
    assert!(
        running_min_width <= PREFERENCES_CONTENT_WIDTH,
        "running jobs must fit inside the authored dialog width: the \
         contents ask for {running_min_width} px, the dialog is \
         {PREFERENCES_CONTENT_WIDTH} px"
    );
    // The head keeps its own geometry: running jobs cannot make it taller
    // and nothing of theirs is laid over it.
    assert_eq!(shell.content_header.height(), header_height);
    assert!(
        !bar.widget().is_ancestor(&shell.content_header),
        "the footer must never be parented into the dialog head"
    );
    let header_origin = shell
        .content_header
        .compute_point(&shell.root_overlay, &gtk4::graphene::Point::new(0.0, 0.0))
        .expect("header and footer share the dialog");
    let bar_origin = bar
        .widget()
        .compute_point(&shell.root_overlay, &gtk4::graphene::Point::new(0.0, 0.0))
        .expect("header and footer share the dialog");
    let header_bottom = header_origin.y() + shell.content_header.height() as f32;
    assert!(
        bar_origin.y() >= header_bottom,
        "the footer sits below the head, not over it: {} < {header_bottom}",
        bar_origin.y()
    );

    shell.dialog.force_close();
    parent.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_18_a_running_job_keeps_the_count_it_is_reporting() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.PreferencesFooterBudgetTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let parent = adw::ApplicationWindow::new(&app);
    parent.present();
    crate::ui::style::install();
    let bar = BackgroundBar::new();
    let shell = super::build(test_pages(), Some(bar.widget()));
    shell.dialog.present(Some(&parent));
    settle_layout();
    bar.publish(JobOwner::Artwork, Some(artwork_job_row()));
    settle_layout();

    // Naming a job is the point of the footer, and the name is only half
    // of it: "Album covers · 1942 of 2132" carries the count. Ported
    // literally from the draft the fixed columns left it 101 px of the
    // 197 px it asks for, so the row read "Album cover…" instead.
    //
    // Computed, not looked at. The gate's own X server comes up 640 px
    // wide — narrower than the dialog — so no display test
    // ever sees the authored 760 px allocated. What it can measure is what
    // each column asks for, and that is what the budget is made of.
    let columns = footer_columns(&bar);
    let (detail_min, detail_natural, ..) = columns
        .get(1)
        .expect("the description is the row's second column")
        .measure(gtk4::Orientation::Horizontal, -1);
    let (footer_min, ..) = bar.widget().measure(gtk4::Orientation::Horizontal, -1);
    assert!(
        shell.sidebar.width() <= SIDEBAR_WIDTH_BUDGET_PX,
        "the pinned sidebar outgrew its budget: {} > {SIDEBAR_WIDTH_BUDGET_PX}",
        shell.sidebar.width()
    );
    let fixed_columns = footer_min - detail_min;
    let room = PREFERENCES_CONTENT_WIDTH - SIDEBAR_WIDTH_BUDGET_PX - fixed_columns;
    assert!(
        room >= detail_natural,
        "every other column of the footer takes {fixed_columns} px, which \
         leaves the description {room} px of the {detail_natural} px it \
         needs to state its count in full"
    );

    shell.dialog.force_close();
    parent.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_18_the_footer_keeps_one_place_across_all_pages() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.PreferencesFooterPagesTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let parent = adw::ApplicationWindow::new(&app);
    parent.set_default_size(900, 760);
    parent.present();
    crate::ui::style::install();
    let bar = BackgroundBar::new();
    bar.publish(JobOwner::Artwork, Some(artwork_job_row()));
    let shell = super::build(test_pages(), Some(bar.widget()));
    shell.dialog.present(Some(&parent));
    settle_layout();
    let bar_parent = bar.widget().parent();
    let bar_origin = bar
        .widget()
        .compute_point(&shell.root_overlay, &gtk4::graphene::Point::new(0.0, 0.0));

    for index in 0..PAGE_ORDER.len() as i32 {
        shell
            .sidebar
            .select_row(shell.sidebar.row_at_index(index).as_ref());
        settle_layout();
        assert!(bar.widget().is_visible());
        assert_eq!(bar.widget().parent(), bar_parent);
        assert_eq!(
            bar.widget()
                .compute_point(&shell.root_overlay, &gtk4::graphene::Point::new(0.0, 0.0)),
            bar_origin,
            "the footer must stay in exactly the same place on every page"
        );
    }

    shell.dialog.force_close();
    parent.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_18_visual_background_bar_fixture() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let parent = gtk4::Window::builder()
        .title("Reprise SET-18 Visual Fixture")
        .default_width(900)
        .default_height(760)
        .build();
    parent.present();
    settle_layout();
    assert!(parent.is_mapped());
    crate::ui::style::install();

    let bar = BackgroundBar::new();
    if std::env::var("REPRISE_FB9_VISUAL_STATE").as_deref() == Ok("running") {
        bar.publish(JobOwner::Artwork, Some(artwork_job_row()));
        bar.publish(JobOwner::OnlineLyrics, Some(lyrics_job_row()));
    }
    let shell = super::build(test_pages(), Some(bar.widget()));
    shell.dialog.present(Some(&parent));
    settle_layout();
    assert!(shell.dialog.is_mapped());

    let hold_ms = std::env::var("REPRISE_FB9_VISUAL_HOLD_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200);
    settle_for(std::time::Duration::from_millis(hold_ms));

    shell.dialog.force_close();
    parent.close();
}

/// Prints the footer's width budget against the dialog it lives in. A
/// tool, not a guard: the display-test runner drops it by its own
/// `measurement:` reason. Run it when a column width is in question —
/// guessing produced the 132/150/44 the draft's wider row implied.
#[test]
#[ignore = "measurement: prints the footer's width budget against the dialog"]
fn measure_background_bar_width_budget() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.PreferencesFooterMeasure")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let parent = adw::ApplicationWindow::new(&app);
    parent.present();
    crate::ui::style::install();
    let bar = BackgroundBar::new();
    let shell = super::build(test_pages(), Some(bar.widget()));
    shell.dialog.present(Some(&parent));
    settle_layout();
    bar.publish(JobOwner::Artwork, Some(artwork_job_row()));
    bar.publish(JobOwner::OnlineLyrics, Some(lyrics_job_row()));
    settle_layout();

    let (bar_min, bar_nat, ..) = bar.widget().measure(gtk4::Orientation::Horizontal, -1);
    let (contents_min, contents_nat, ..) = shell
        .root_overlay
        .measure(gtk4::Orientation::Horizontal, -1);
    println!(
        "MEASURE footer min={bar_min} nat={bar_nat} | contents min={contents_min} \
         nat={contents_nat} | sidebar={} | dialog={PREFERENCES_CONTENT_WIDTH}",
        shell.sidebar.width(),
    );
    for column in footer_columns(&bar) {
        let (min, nat, ..) = column.measure(gtk4::Orientation::Horizontal, -1);
        println!("MEASURE   column {}: min={min} nat={nat}", column.type_());
    }

    shell.dialog.force_close();
    parent.close();
}

/// The five widgets of the first job row, in the order they are packed:
/// owner, description, track, percent, cancel.
fn footer_columns(bar: &BackgroundBar) -> Vec<gtk4::Widget> {
    let rows = bar
        .widget()
        .first_child()
        .and_then(|header| header.next_sibling())
        .expect("the footer lists its rows straight after its header");
    let row = rows
        .first_child()
        .expect("a published job has a row of its own");
    let mut columns = Vec::new();
    let mut child = row.first_child();
    while let Some(column) = child {
        child = column.next_sibling();
        columns.push(column);
    }
    columns
}
