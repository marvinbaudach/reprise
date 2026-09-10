//! Display-gated geometry contract for the two shapes the filter bar leads
//! with: the search pill and "+ Add filter". Both carry `min-height: 36px`
//! in CSS, but they are different widget trees — a `GtkBox` chip and a
//! `GtkMenuButton` — so the authored value alone does not prove they render
//! at the same height. Adwaita's own `button` padding stacks on top of the
//! `min-height` on the MenuButton side; nothing does on the chip's. This
//! measures the rendered allocation rather than trusting the stylesheet.

use std::time::Duration;

use gtk4::prelude::*;
use reprise_view::filter_chip::FilterChipModel;

use super::filter_bar_chip::{build_chip, CHIP_MIN_HEIGHT};
use super::filter_bar_layout::{self, FilterBarLayout};

/// The height each pill *asks* for, not the height the row happens to hand
/// it. A filter-bar slot stretches its child to whatever the surrounding
/// window gives (`valign: fill`), so comparing allocations in a test window
/// compares the window, not the two shapes — both come back identical no
/// matter what the stylesheet says. `measure` on the vertical orientation is
/// what the authored `min-height` actually feeds.
struct PillHeights {
    search: (i32, i32),
    add_filter: (i32, i32),
}

fn measure() -> PillHeights {
    let layout = FilterBarLayout::new();

    let chip = build_chip(
        &FilterChipModel::search("lorna").expect("non-blank query is a chip"),
        || {},
    );
    layout.fill_search(&chip);

    // The exact tree `browse_bar` builds: a MenuButton in a horizontal box,
    // carrying "pill" plus the filter-bar's own dashed-outline class.
    let add_label = gtk4::Label::new(Some("+ Add filter"));
    let add_filter = gtk4::MenuButton::new();
    add_filter.set_child(Some(&add_label));
    add_filter.add_css_class("pill");
    filter_bar_layout::style_add_filter(&add_filter);
    let filter_actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    filter_actions.append(&add_filter);
    layout.fill_add_filter(&filter_actions);

    let window = gtk4::Window::builder()
        .default_width(1_120)
        .child(layout.root())
        .build();
    window.present();
    assert!(crate::ui::test_settle::settle_until_mapped(layout.root()));
    crate::ui::test_settle::settle_for(Duration::from_millis(20));

    let vertical = |widget: &gtk4::Widget| {
        let (minimum, natural, _, _) = widget.measure(gtk4::Orientation::Vertical, -1);
        (minimum, natural)
    };
    let heights = PillHeights {
        search: vertical(chip.upcast_ref()),
        add_filter: vertical(add_filter.upcast_ref()),
    };
    window.close();
    heights
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_search_pill_and_add_filter_render_at_the_same_height() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());

    let heights = measure();

    // The authored height plus the 1px border each shape carries. Naming the
    // expected number rather than only comparing the two keeps the test
    // honest if both sides ever drift together.
    let expected = CHIP_MIN_HEIGHT + 2;
    assert_eq!(
        heights.search,
        (expected, expected),
        "the search pill asks for {:?}px, not the authored {CHIP_MIN_HEIGHT}px plus its border",
        heights.search
    );
    assert_eq!(
        heights.add_filter,
        (expected, expected),
        "\"+ Add filter\" asks for {:?}px against the search pill's {:?}px — \
         Adwaita's button padding is stacking on the authored {CHIP_MIN_HEIGHT}px again",
        heights.add_filter,
        heights.search
    );
}
