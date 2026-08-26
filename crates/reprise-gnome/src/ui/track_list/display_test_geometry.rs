use gtk4::prelude::*;
use std::collections::BTreeMap;

pub(super) fn realized_row_measurements(column_view: &gtk4::ColumnView) -> Vec<(i32, i32)> {
    let mut measurements = Vec::new();
    let mut pending = vec![column_view.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = pending.pop() {
        if widget.type_().name().contains("ColumnViewRow")
            && widget.css_name() == "row"
            && widget.height() > 0
        {
            let (minimum, natural, _, _) = widget.measure(gtk4::Orientation::Vertical, -1);
            measurements.push((
                widget.height(),
                if natural == 0 { minimum } else { natural },
            ));
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    measurements
}

pub(super) fn measured_row_height(column_view: &gtk4::ColumnView) -> Option<f64> {
    let measurements = realized_row_measurements(column_view);
    let mut counts = BTreeMap::<i32, usize>::new();
    for allocated in measurements
        .into_iter()
        .filter(|(allocated, natural)| allocated >= natural)
        .map(|(allocated, _)| allocated)
    {
        *counts.entry(allocated).or_default() += 1;
    }
    if counts.values().sum::<usize>() < 3 {
        return None;
    }
    let max_count = counts.values().copied().max()?;
    let modes = counts
        .into_iter()
        .filter_map(|(height, count)| (count == max_count).then_some(height))
        .collect::<Vec<_>>();
    (modes.len() == 1).then(|| f64::from(modes[0]))
}
