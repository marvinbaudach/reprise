use std::rc::Rc;

use gtk4::prelude::*;

const BAND_LABELS: [&str; 10] = [
    "31 Hz", "62 Hz", "125 Hz", "250 Hz", "500 Hz", "1 kHz", "2 kHz", "4 kHz", "8 kHz", "16 kHz",
];

pub(in crate::ui) struct EqualizerSurface {
    pub(in crate::ui) root: gtk4::Box,
    pub(in crate::ui) scales: Vec<gtk4::Scale>,
}

fn gain_label(value: f64) -> String {
    if value.abs() < 0.5 {
        "0 dB".to_string()
    } else {
        format!("{value:+.0} dB")
    }
}

pub(in crate::ui) fn build_equalizer_surface(
    bands: [f64; 10],
    enabled: bool,
    on_changed: &Rc<dyn Fn(usize, f64)>,
) -> EqualizerSurface {
    let band_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 18);
    band_box.set_margin_top(18);
    band_box.set_margin_bottom(12);
    band_box.set_margin_start(18);
    band_box.set_margin_end(18);

    let mut scales = Vec::with_capacity(BAND_LABELS.len());
    for (index, label) in BAND_LABELS.into_iter().enumerate() {
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        column.set_width_request(52);

        let value = gtk4::Label::new(Some(&gain_label(bands[index])));
        value.add_css_class("caption");
        value.add_css_class("reprise-eq-value");
        let scale = gtk4::Scale::with_range(gtk4::Orientation::Vertical, -12.0, 12.0, 1.0);
        scale.set_height_request(180);
        scale.set_vexpand(true);
        scale.set_inverted(true);
        scale.set_draw_value(false);
        scale.set_value(bands[index]);
        scale.update_property(&[gtk4::accessible::Property::Label(label)]);
        let frequency = gtk4::Label::new(Some(label));
        frequency.add_css_class("caption");

        let value_for_change = value.clone();
        let on_changed = Rc::clone(on_changed);
        scale.connect_value_changed(move |scale| {
            value_for_change.set_label(&gain_label(scale.value()));
            on_changed(index, scale.value());
        });
        column.append(&value);
        column.append(&scale);
        column.append(&frequency);
        band_box.append(&column);
        scales.push(scale);
    }

    let scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .min_content_height(236)
        .propagate_natural_height(true)
        .child(&band_box)
        .build();
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.add_css_class("card");
    root.add_css_class("reprise-equalizer");
    root.set_overflow(gtk4::Overflow::Hidden);
    root.set_sensitive(enabled);
    root.append(&scroll);

    EqualizerSurface { root, scales }
}

/// Equalizer chrome: accent-coloured band fills/handles and an accent dB
/// readout, so the ten-band card reads as part of the redesign's accent
/// system. Installed app-wide by [`super::style`].
pub(in crate::ui) fn css() -> String {
    ".reprise-equalizer scale > trough > highlight { background-color: @accent_color; }\n\
     .reprise-equalizer scale > trough > slider { background-color: @accent_color; }\n\
     .reprise-eq-value { color: @accent_color; font-weight: bold; }\n\
     .reprise-crossfade { padding: 12px 6px; }\n\
     .reprise-crossfade > box > label.title { font-weight: bold; }\n\
     .reprise-crossfade-value { color: @accent_color; font-weight: bold; }\n\
     .reprise-crossfade-scale { margin-top: 4px; }\n\
     .reprise-crossfade-scale > trough > highlight { background-color: @accent_color; }\n\
     .reprise-crossfade-scale > trough > slider { background-color: @accent_color; }"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn equalizer_bands_share_one_scrollable_card_and_follow_enabled_state() {
        if gtk4::init().is_err() {
            return;
        }
        let on_changed: Rc<dyn Fn(usize, f64)> = Rc::new(|_, _| {});
        let surface = build_equalizer_surface([0.0; 10], false, &on_changed);

        assert!(surface.root.has_css_class("card"));
        assert!(surface.root.has_css_class("reprise-equalizer"));
        assert_eq!(surface.scales.len(), 10);
        assert!(surface
            .scales
            .iter()
            .all(|scale| scale.orientation() == gtk4::Orientation::Vertical));
        assert!(!surface.root.is_sensitive());
    }
}
