//! Isolated display evidence that the preferences status chip is *measured*
//! against the header it floats over, on both axes, and stays measured while
//! the dialog is open — plus the counterprobe that the arrangement it replaced
//! really did displace the page.

use std::path::PathBuf;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::scanner::ScanProgress;

use super::tests::{settle_layout, test_pages};
use super::*;
use crate::ui::scan_chrome::ScanChromeView;
use crate::ui::scan_progress::ScanProgressView;

/// Floor for the counterprobe's measured displacement. The scan card really
/// occupies 88 px under the app stylesheet at Adwaita's default font metrics;
/// the floor sits below that so a different font size cannot make the
/// counterprobe flaky. What it has to prove is a whole card's worth of
/// displacement, not one exact pixel count.
const RETIRED_TOP_BAR_MIN_JUMP_PX: f32 = 80.0;

/// A presented preferences dialog whose scan chrome is already running, so the
/// chip is visible and allocated. The parent window keeps the application
/// alive for as long as the fixture lives.
struct ChromeDialog {
    parent: adw::ApplicationWindow,
    chrome: ScanChromeView,
    shell: PreferencesShell,
}

impl ChromeDialog {
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
        let chrome = ScanChromeView::new();
        chrome.show(&ScanProgress::Scanning {
            processed: 39,
            total: Some(100),
            current_path: PathBuf::from("/music/track.flac"),
        });
        let shell = build(
            test_pages(),
            Some(chrome.line_widget()),
            Some(chrome.chip_widget()),
        );
        shell.dialog.present(Some(&parent));
        settle_layout();
        Self {
            parent,
            chrome,
            shell,
        }
    }

    fn chip(&self) -> gtk4::Widget {
        self.chrome.chip_widget().clone()
    }

    fn header(&self) -> adw::HeaderBar {
        self.shell.content_header.clone()
    }

    fn close(self) {
        self.shell.dialog.force_close();
        self.parent.close();
    }
}

fn origin_in_header(widget: &gtk4::Widget, header: &adw::HeaderBar) -> f32 {
    widget
        .compute_point(header, &gtk4::graphene::Point::new(0.0, 0.0))
        .expect("the widget must be allocated inside the content header")
        .x()
}

/// The counterprobe for the whole feature: the arrangement this branch retired
/// really did shove the page down when a scan started, so the overlay chrome
/// that replaced it is solving a real problem.
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

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_9_chip_end_inset_is_measured_from_the_header_title_buttons() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let dialog = ChromeDialog::present("org.reprise.Reprise.PreferencesChromeInsetTest");
    let header = dialog.header();

    let strip = header_end_strip(&header)
        .expect("the header must expose an allocated end-title-button strip to measure");
    assert_eq!(
        dialog.chip().margin_end(),
        header_end_inset(&header).expect("an allocated header must yield a measured inset"),
        "the chip's horizontal inset must be measured, not assumed"
    );
    assert!(
        origin_in_header(&dialog.chip(), &header) + dialog.chip().width() as f32
            <= origin_in_header(&strip, &header),
        "the chip must end before the measured title-button strip begins"
    );

    dialog.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_9_chip_recenters_when_a_text_scale_change_grows_the_header() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let dialog = ChromeDialog::present("org.reprise.Reprise.PreferencesChromeRescaleTest");
    let header = dialog.header();
    let settled_height = header.height();
    let settled_margin = dialog.chip().margin_top();

    // A live font metric change — GNOME's text-scaling factor or a larger
    // interface font — is what grows the header under an open dialog.
    let settings = gtk4::Settings::default().expect("GTK settings must exist under a display");
    let original_font = settings.gtk_font_name();
    settings.set_gtk_font_name(Some("Cantarell 32"));
    settle_layout();
    let scaled_height = header.height();
    let scaled_margin = dialog.chip().margin_top();
    let scaled_chip_height = dialog.chip().height();
    settings.set_gtk_font_name(original_font.as_deref());

    assert!(
        scaled_height > settled_height,
        "the probe must actually grow the header ({settled_height} -> {scaled_height})"
    );
    assert_ne!(
        scaled_margin, settled_margin,
        "a taller header must move the chip's centring inset"
    );
    assert_eq!(
        scaled_margin,
        (scaled_height - scaled_chip_height).max(0) / 2,
        "the chip must stay centred in the header it actually got"
    );

    dialog.close();
}
