//! The one painted placeholder shared by releases, updates and concerts.

const CORNER_RADIUS: f64 = 4.0;
const DARK_TOP_ALPHA: f64 = 0.20;
const DARK_BOTTOM_ALPHA: f64 = 0.08;
const LIGHT_TOP_ALPHA: f64 = 0.16;
const LIGHT_BOTTOM_ALPHA: f64 = 0.06;
const DARK_HAIRLINE_ALPHA: f64 = 0.14;
const LIGHT_HAIRLINE_ALPHA: f64 = 0.10;
const FONT_EDGE_RATIO: f64 = 0.34;
const LETTER_SPACING_EM: f64 = 0.04;

pub(super) struct Appearance {
    pub(super) is_dark: bool,
    pub(super) foreground: gtk4::gdk::RGBA,
    pub(super) surface: [u8; 3],
}

pub(super) fn draw(
    context: &gtk4::cairo::Context,
    pango: &gtk4::pango::Context,
    width: f64,
    height: f64,
    initials: &str,
    appearance: &Appearance,
) {
    let accent = crate::ui::style::accent::accent_rgba();
    let (top_alpha, bottom_alpha, hairline_alpha) = if appearance.is_dark {
        (DARK_TOP_ALPHA, DARK_BOTTOM_ALPHA, DARK_HAIRLINE_ALPHA)
    } else {
        (LIGHT_TOP_ALPHA, LIGHT_BOTTOM_ALPHA, LIGHT_HAIRLINE_ALPHA)
    };

    rounded_rectangle(context, 0.0, 0.0, width, height, CORNER_RADIUS);
    context.clip();
    let gradient = gtk4::cairo::LinearGradient::new(0.0, 0.0, 0.0, height);
    gradient.add_color_stop_rgba(
        0.0,
        f64::from(accent.red()),
        f64::from(accent.green()),
        f64::from(accent.blue()),
        top_alpha,
    );
    gradient.add_color_stop_rgba(
        1.0,
        f64::from(accent.red()),
        f64::from(accent.green()),
        f64::from(accent.blue()),
        bottom_alpha,
    );
    let _ = context.set_source(&gradient);
    let _ = context.paint();

    if !initials.is_empty() {
        draw_initials(
            context,
            pango,
            (width, height),
            initials,
            accent,
            top_alpha,
            appearance,
        );
    }

    context.reset_clip();
    context.set_source_rgba(
        f64::from(appearance.foreground.red()),
        f64::from(appearance.foreground.green()),
        f64::from(appearance.foreground.blue()),
        hairline_alpha,
    );
    context.set_line_width(1.0);
    rounded_rectangle(
        context,
        0.5,
        0.5,
        (width - 1.0).max(0.0),
        (height - 1.0).max(0.0),
        CORNER_RADIUS - 0.5,
    );
    let _ = context.stroke();
}

fn draw_initials(
    context: &gtk4::cairo::Context,
    pango: &gtk4::pango::Context,
    size: (f64, f64),
    initials: &str,
    accent: gtk4::gdk::RGBA,
    top_alpha: f64,
    appearance: &Appearance,
) {
    let (width, height) = size;
    let layout = gtk4::pango::Layout::new(pango);
    layout.set_text(initials);
    let edge = width.min(height);
    let font_size = edge * FONT_EDGE_RATIO;
    let mut font = gtk4::pango::FontDescription::new();
    font.set_weight(gtk4::pango::Weight::Bold);
    font.set_absolute_size(font_size * f64::from(gtk4::pango::SCALE));
    layout.set_font_description(Some(&font));
    let attributes = gtk4::pango::AttrList::new();
    attributes.insert(gtk4::pango::AttrInt::new_letter_spacing(
        (font_size * LETTER_SPACING_EM * f64::from(gtk4::pango::SCALE)).round() as i32,
    ));
    layout.set_attributes(Some(&attributes));

    let accent_rgb = rgba_rgb(accent);
    let ground = crate::ui::style::color_math::composite(accent_rgb, appearance.surface, top_alpha);
    let text = crate::ui::style::accent::accent_text_color(accent_rgb, ground, appearance.is_dark);
    let text = gtk4::gdk::RGBA::parse(&text).expect("the derived accent text color is valid");
    context.set_source_rgb(
        f64::from(text.red()),
        f64::from(text.green()),
        f64::from(text.blue()),
    );
    let (ink, _) = layout.pixel_extents();
    let x = (width - f64::from(ink.width())) / 2.0 - f64::from(ink.x());
    let y = (height - f64::from(ink.height())) / 2.0 - f64::from(ink.y());
    context.move_to(x, y);
    pangocairo::functions::show_layout(context, &layout);
}

fn rgba_rgb(color: gtk4::gdk::RGBA) -> [u8; 3] {
    [
        (color.red() * 255.0).round() as u8,
        (color.green() * 255.0).round() as u8,
        (color.blue() * 255.0).round() as u8,
    ]
}

fn rounded_rectangle(
    context: &gtk4::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    context.new_sub_path();
    context.arc(
        x + width - radius,
        y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    context.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    context.arc(
        x + radius,
        y + height - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    context.arc(
        x + radius,
        y + radius,
        radius,
        std::f64::consts::PI,
        std::f64::consts::PI * 1.5,
    );
    context.close_path();
}
