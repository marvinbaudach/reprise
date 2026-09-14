use gtk4::prelude::*;

use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_14_numeric_columns_align_right() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let cells = ["2025", "12:34"].map(|text| {
        let label = build_text_cell_label(CellAlignment::Numeric);
        label.set_text(text);
        label.set_width_request(120);
        label
    });
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    for cell in &cells {
        row.append(cell);
    }
    let window = gtk4::Window::builder().child(&row).build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    for cell in cells {
        let (text_x, _) = cell.layout_offsets();
        let (text_width, _) = cell.layout().pixel_size();
        assert_eq!(cell.xalign(), 1.0);
        assert!(
            text_x + text_width >= cell.width() - 1,
            "numeric text must render against the cell's right edge"
        );
        assert!(cell.has_css_class("numeric"));
    }
    window.close();
}
