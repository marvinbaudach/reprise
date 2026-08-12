use gtk4::prelude::*;

use super::ScanProgressView;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_1_scan_job_card_minimum_width_fits_the_sidebar() {
    gtk4::init().unwrap();
    crate::ui::style::install();
    let view = ScanProgressView::new();
    view.show_batch(
        "Lyrics batch check complete",
        "2,177 of 2,177 checked · 0 downloaded · 0 unavailable",
        1.0,
    );

    let minimum = view.widget().measure(gtk4::Orientation::Horizontal, -1).0;
    assert!(
        minimum <= 232,
        "the visible scan job card must fit inside the 240px sidebar's 232px card slot, got {minimum}px"
    );
    assert_eq!(
        view.inner.title.ellipsize(),
        gtk4::pango::EllipsizeMode::End
    );
    assert_eq!(
        view.inner.detail.ellipsize(),
        gtk4::pango::EllipsizeMode::End
    );
    assert_eq!(
        view.widget().measure(gtk4::Orientation::Vertical, 232).0,
        crate::ui::scan_card_css::JOB_CARD_HEIGHT_PX
    );
    let window = gtk4::Window::builder()
        .default_width(232)
        .default_height(200)
        .child(view.widget())
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(view.widget().width(), 232);
    assert!(
        !view.inner.title.layout().is_ellipsized(),
        "the title's dedicated row must keep a realistic 27-character job name readable at sidebar width"
    );
    window.close();
}
