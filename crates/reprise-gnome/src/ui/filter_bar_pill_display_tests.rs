//! Display-gated geometry contract for the two shapes the filter bar leads
//! with: the search pill and "+ Add filter". Both carry `min-height: 36px`
//! in CSS, but they are different widget trees — a `GtkBox` chip and a
//! `GtkMenuButton` — so the authored value alone does not prove they render
//! at the same height. On the MenuButton side Adwaita stacks two things on
//! top of the `min-height` that nothing stacks on the chip's: the `button`
//! node's own padding, and 1px of vertical margin. This measures what the two
//! shapes ask for rather than trusting the stylesheet.

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

/// The magnifier renders at its authored size and divides the chip evenly.
/// `set_pixel_size` is only a request — a theme rule on the `image` node can
/// override it — and an icon whose height leaves an odd remainder inside the
/// chip is centred on a half pixel, which reads as "slightly off" rather than
/// as a size problem. The `×` beside it is the control: it has always divided
/// evenly, which is why only the magnifier ever looked misplaced.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_search_icon_renders_at_its_size_and_divides_the_chip_evenly() {
    use crate::ui::filter_bar_chip::{
        child_with_css_class, CHIP_ICON_CSS_CLASS, CHIP_ICON_SIZE, CHIP_REMOVE_CSS_CLASS,
    };

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());

    let chip = build_chip(
        &FilterChipModel::search("lorna").expect("non-blank query is a chip"),
        || {},
    );
    // The bar is content-sized in the app; without this the test window
    // stretches the chip and the centring being probed becomes the window's.
    chip.set_valign(gtk4::Align::Start);
    let window = gtk4::Window::builder().child(&chip).build();
    window.present();
    assert!(crate::ui::test_settle::settle_until_mapped(&chip));
    crate::ui::test_settle::settle_for(Duration::from_millis(20));

    let bounds_of = |class: &str| {
        let widget = child_with_css_class(&chip, class).expect("chip child is present");
        widget
            .compute_bounds(&chip)
            .expect("chip child has chip-relative bounds")
    };
    let icon = bounds_of(CHIP_ICON_CSS_CLASS);
    let remove = bounds_of(CHIP_REMOVE_CSS_CLASS);
    window.close();

    assert_eq!(
        (icon.width(), icon.height()),
        (CHIP_ICON_SIZE as f32, CHIP_ICON_SIZE as f32),
        "the magnifier renders {}×{}px, not the authored {CHIP_ICON_SIZE}px square — \
         a theme rule on the image node is overriding `set_pixel_size`",
        icon.width(),
        icon.height()
    );
    // The `×` is the control: it divides the chip evenly and has always looked
    // right, so sharing its centre line is the claim worth pinning. Comparing
    // the two against each other needs no assumption about where the chip's
    // content box begins — which is exactly what the earlier probe got wrong.
    let centre = |bounds: gtk4::graphene::Rect| bounds.y() + bounds.height() / 2.0;
    assert_eq!(
        centre(icon),
        centre(remove),
        "the magnifier is centred at {}px and the × at {}px",
        centre(icon),
        centre(remove)
    );
    // An odd remainder puts the glyph on a half pixel, which is what "slightly
    // off" looks like even when the centre line itself is right.
    assert_eq!(
        (centre(icon) * 2.0) % 2.0,
        0.0,
        "the magnifier's centre falls on a half pixel at {}px",
        centre(icon)
    );
}
