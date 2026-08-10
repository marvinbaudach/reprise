//! TEMPORARY diagnostic — do not ship.
//!
//! Names every writer of a scroll adjustment so a display run can show which
//! one produces an intermediate value. Silent unless `REPRISE_SCROLL_PROBE`
//! is set, so it cannot affect an ordinary run.

pub(in crate::ui) fn probe(writer: &str, adjustment: &gtk4::Adjustment, value: f64) {
    use gtk4::prelude::AdjustmentExt;

    if std::env::var_os("REPRISE_SCROLL_PROBE").is_none() {
        return;
    }
    eprintln!(
        "SCROLLWRITE writer={writer} want={value:.1} from={:.1} upper={:.1} page={:.1}",
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size(),
    );
}

pub(in crate::ui) fn probe_upper(writer: &str, adjustment: &gtk4::Adjustment, upper: f64) {
    use gtk4::prelude::AdjustmentExt;

    if std::env::var_os("REPRISE_SCROLL_PROBE").is_none() {
        return;
    }
    eprintln!(
        "SCROLLUPPER writer={writer} want={upper:.1} from={:.1} value={:.1} page={:.1}",
        adjustment.upper(),
        adjustment.value(),
        adjustment.page_size(),
    );
}

/// Lets one build run both the shipped behaviour and the experiment: with
/// `REPRISE_NO_SET_UPPER` set, `apply_scroll_anchor_if_allocated` stops
/// pre-seeding the bound it then reads back as proof of readiness.
pub(in crate::ui) fn set_upper_suppressed() -> bool {
    std::env::var_os("REPRISE_NO_SET_UPPER").is_some()
}

/// TEMPORARY: is there a realized row widget to measure, at the moment the
/// restore runs? Answers whether a measured row height can close the
/// allocation window, or whether only the density token can.
pub(in crate::ui) fn probe_rows(where_: &str, column_view: &gtk4::ColumnView) {
    use gtk4::glib::prelude::Cast;

    if std::env::var_os("REPRISE_SCROLL_PROBE").is_none() {
        return;
    }
    fn walk(widget: &gtk4::Widget, out: &mut Vec<(String, i32, i32)>) {
        use gtk4::glib::prelude::ObjectExt;
        use gtk4::prelude::WidgetExt;
        let name = widget.type_().name().to_string();
        if name.contains("ColumnViewRow") || name.contains("ColumnViewCell") {
            let (min, nat, _, _) = widget.measure(gtk4::Orientation::Vertical, -1);
            out.push((name, widget.height(), if nat > 0 { nat } else { min }));
        }
        let mut child = widget.first_child();
        while let Some(node) = child {
            walk(&node, out);
            child = node.next_sibling();
        }
    }
    let mut found = Vec::new();
    walk(column_view.upcast_ref::<gtk4::Widget>(), &mut found);
    let rows: Vec<_> = found
        .iter()
        .filter(|(name, _, _)| name.contains("ColumnViewRow"))
        .collect();
    eprintln!(
        "SCROLLROWS at={where_} row_widgets={} first={:?} distinct_heights={:?}",
        rows.len(),
        rows.first(),
        {
            let mut heights: Vec<i32> = rows.iter().map(|(_, h, _)| *h).collect();
            heights.sort_unstable();
            heights.dedup();
            heights.into_iter().take(6).collect::<Vec<_>>()
        }
    );
}
