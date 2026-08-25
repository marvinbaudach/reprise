//! Environment-gated scroll diagnostics for display regressions.
//!
//! Names every writer of a scroll adjustment so a display run can show which
//! one produces an intermediate value. Silent unless `REPRISE_SCROLL_PROBE`
//! is set, so it cannot affect an ordinary run.

pub(in crate::ui) fn probe(writer: &str, adjustment: &gtk4::Adjustment, value: f64) {
    use gtk4::prelude::AdjustmentExt;

    #[cfg(test)]
    trail::record(trail::Entry::Write {
        writer: writer.to_owned(),
        value,
    });
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

pub(in crate::ui) fn probe_scroll_to(writer: &str, adjustment: &gtk4::Adjustment, position: u32) {
    use gtk4::prelude::AdjustmentExt;

    #[cfg(test)]
    trail::record(trail::Entry::ScrollTo {
        writer: writer.to_owned(),
        position,
    });
    if std::env::var_os("REPRISE_SCROLL_PROBE").is_none() {
        return;
    }
    eprintln!(
        "SCROLLTO writer={writer} position={position} from={:.1} upper={:.1} page={:.1}",
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size(),
    );
}

pub(in crate::ui) fn probe_value_change(writer: &str, adjustment: &gtk4::Adjustment, from: f64) {
    use gtk4::prelude::AdjustmentExt;

    if std::env::var_os("REPRISE_SCROLL_PROBE").is_none() {
        return;
    }
    eprintln!(
        "SCROLLVALUE writer={writer} from={from:.1} to={:.1} upper={:.1} page={:.1}",
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size(),
    );
}

/// Watches one adjustment so GTK-owned movement appears beside named writes.
///
/// A `SCROLLOBSERVED` line without a preceding `SCROLLWRITE` is movement no
/// application writer claimed. The handler stays behavior-free when the
/// environment probe is disabled; tests can still opt into the in-process
/// trail independently.
pub(in crate::ui) fn observe(scope: &'static str, adjustment: &gtk4::Adjustment) {
    use gtk4::prelude::AdjustmentExt;

    let previous = std::cell::Cell::new(adjustment.value());
    let enabled = std::env::var_os("REPRISE_SCROLL_PROBE").is_some();
    adjustment.connect_value_changed(move |changed| {
        let value = changed.value();
        let from = previous.replace(value);
        #[cfg(test)]
        trail::note_observed(value);
        if enabled {
            eprintln!(
                "SCROLLOBSERVED scope={scope} from={from:.1} to={value:.1} upper={:.1} page={:.1}",
                changed.upper(),
                changed.page_size(),
            );
        }
    });
}

/// Places a named boundary around a mutation without writing the adjustment.
pub(in crate::ui) fn probe_snapshot(at: &str, adjustment: &gtk4::Adjustment) {
    use gtk4::prelude::AdjustmentExt;

    if std::env::var_os("REPRISE_SCROLL_PROBE").is_none() {
        return;
    }
    eprintln!(
        "SCROLLSNAPSHOT at={at} value={:.1} upper={:.1} page={:.1}",
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

/// Names which geometry source the pre-seed actually used. The measured run
/// that accepted the sectioned pre-seed reported `Assumed` here, so this is
/// the line that distinguishes a warm cache from the fail-closed fallback.
pub(in crate::ui) fn probe_preseed_source(source: &str) {
    if std::env::var_os("REPRISE_SCROLL_PROBE").is_none() {
        return;
    }
    // Keep this wording: the accepted measurement and the plan both cite
    // `QUEUEPROBE preseed header_source=…` verbatim as their evidence line.
    eprintln!("QUEUEPROBE preseed header_source={source}");
}

/// Lets one build run both the shipped behaviour and the experiment: with
/// `REPRISE_NO_PRESEED` set, the restore leaves the stale allocation range in
/// place as a counterprobe for the first-frame jump.
pub(in crate::ui) fn preseed_suppressed() -> bool {
    std::env::var_os("REPRISE_NO_PRESEED").is_some()
}

/// Keeps the A1 experiment's successful arm available as a counterprobe:
/// force the whole anchor restore to wait for the first allocated viewport.
/// The experiment established that ordering, rather than any individual
/// writer, decides whether a fresh-start track list is allocated.
pub(in crate::ui) fn restore_after_allocation_enabled() -> bool {
    std::env::var_os("REPRISE_RESTORE_AFTER_ALLOCATION").is_some()
}

/// Records realized row widgets at the moment a restore runs.
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

/// In-process counterpart to the `eprintln!` probes above.
///
/// The stderr lines answer "which writer produced this value" for a human
/// reading a display run. A test cannot read them: they leave the process
/// before the assertion runs, and a scoped capture races tracing's callsite
/// cache. So every probe also appends to a thread-local trail while a
/// recording is active, and the test asserts on that instead.
///
/// The point is the *order*: a viewport that reaches its target in one step
/// records one write, one that hops records two. Interleaving the writers'
/// own entries with the values the adjustment actually took (`note_observed`,
/// fed by a `value_changed` handler the test installs) is what separates our
/// writes from GTK's — a value nobody claims came from the allocation pass.
#[cfg(test)]
pub(in crate::ui) mod trail {
    use std::cell::RefCell;

    #[derive(Clone, Debug, PartialEq)]
    pub(in crate::ui) enum Entry {
        /// A writer asked the adjustment for `value`.
        Write { writer: String, value: f64 },
        /// A writer asked the view to bring `position` into view.
        ScrollTo { writer: String, position: u32 },
        /// The adjustment actually took `value`, whoever caused it.
        Observed { value: f64 },
    }

    thread_local! {
        static TRAIL: RefCell<Option<Vec<Entry>>> = const { RefCell::new(None) };
    }

    /// Starts a fresh recording, discarding anything a previous one left.
    pub(in crate::ui) fn start() {
        TRAIL.with(|trail| *trail.borrow_mut() = Some(Vec::new()));
    }

    /// Ends the recording and returns what it saw.
    pub(in crate::ui) fn take() -> Vec<Entry> {
        TRAIL.with(|trail| trail.borrow_mut().take().unwrap_or_default())
    }

    pub(in crate::ui) fn record(entry: Entry) {
        TRAIL.with(|trail| {
            if let Some(entries) = trail.borrow_mut().as_mut() {
                entries.push(entry);
            }
        });
    }

    pub(in crate::ui) fn note_observed(value: f64) {
        record(Entry::Observed { value });
    }
}
