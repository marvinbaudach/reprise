//! FIL-3: the end-of-results line. An overlay positioned from the
//! ColumnView's measured content height keeps row virtualization intact and
//! appears only where the list actually ends.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use super::Shared;

/// Vertical space between the final result row and the explanatory line.
const LINE_TOP_GAP: i32 = 12;
/// Vertical space between the final result row and the recovery pill.
const PILL_TOP_GAP: i32 = 44;

pub(in crate::ui) fn end_line_margin(
    content_height: f64,
    viewport_height: f64,
    scroll_value: f64,
) -> Option<i32> {
    if content_height <= 0.0 || viewport_height <= 0.0 {
        return None;
    }
    let end_in_viewport = content_height - scroll_value;
    if end_in_viewport > viewport_height {
        return None;
    }
    Some(end_in_viewport.max(0.0) as i32)
}

fn recompute(
    shared: &Shared,
    scrolled: &gtk4::ScrolledWindow,
    line_box: &gtk4::Box,
    line: &gtk4::Label,
    pill: &gtk4::Button,
) {
    let source = shared.source.borrow().clone();
    let browse = if matches!(source, reprise_core::view_source::ViewSource::Library) {
        shared.browse_filter.borrow().clone()
    } else {
        reprise_core::queries::BrowseFilter::default()
    };
    let search = shared.filter.borrow().clone();
    // FIL-7: the AI-exclude filter also restricts; the `hidden == 0` guard below
    // handles the experimental-off case (no rows are actually hidden then).
    let restricted = crate::ui::browse::filter_restriction::is_restricted(
        &search,
        &browse,
        shared.browse_bar.exclude_ai(),
    );
    let counts = shared.browse_bar.result_count();
    let filtered = shared.model.n_items() as usize;
    let Some((_, total)) = counts.filter(|_| restricted && filtered >= 1) else {
        line_box.set_visible(false);
        pill.set_visible(false);
        return;
    };
    let hidden = total.saturating_sub(filtered);
    if hidden == 0 {
        line_box.set_visible(false);
        pill.set_visible(false);
        return;
    }
    let adjustment = scrolled.vadjustment();
    let (_, natural) = shared.column_view.preferred_size();
    let margin = end_line_margin(
        f64::from(natural.height()),
        adjustment.page_size(),
        adjustment.value(),
    );
    match margin {
        Some(margin) => {
            let hidden_str = reprise_core::format::format_thousands(hidden as i64);
            let query = search.trim().to_string();
            let text = match (query.is_empty(), browse.is_empty()) {
                (false, true) => {
                    crate::ui::strings::end_of_results_hidden_by_search(&hidden_str, &query)
                }
                (true, false) => crate::ui::strings::end_of_results_hidden_by_filters(&hidden_str),
                _ => crate::ui::strings::end_of_results_hidden_by_both(&hidden_str),
            };
            line.set_text(&text);
            pill.set_label(&crate::ui::strings::show_all_tracks_label(
                &reprise_core::format::format_thousands(total as i64),
            ));
            line_box.set_margin_top(margin + LINE_TOP_GAP);
            pill.set_margin_top(margin + PILL_TOP_GAP);
            line_box.set_visible(true);
            pill.set_visible(true);
        }
        None => {
            line_box.set_visible(false);
            pill.set_visible(false);
        }
    }
}

pub(in crate::ui) fn install(
    shared: &Rc<Shared>,
    overlay: &gtk4::Overlay,
    scrolled: &gtk4::ScrolledWindow,
) {
    let line = gtk4::Label::new(None);
    line.add_css_class("dim-label");
    line.add_css_class("caption");
    line.set_halign(gtk4::Align::Center);

    let line_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    line_box.set_valign(gtk4::Align::Start);
    line_box.set_halign(gtk4::Align::Fill);
    line_box.set_can_target(false);
    line_box.append(&line);

    let pill = gtk4::Button::new();
    pill.add_css_class("pill");
    pill.set_valign(gtk4::Align::Start);
    pill.set_halign(gtk4::Align::Center);
    pill.set_action_name(Some("win.clear-all-filters"));

    line_box.set_visible(false);
    pill.set_visible(false);
    overlay.add_overlay(&line_box);
    overlay.add_overlay(&pill);

    let run: Rc<dyn Fn()> = {
        let shared = Rc::downgrade(shared);
        let scrolled = scrolled.clone();
        let line_box = line_box.clone();
        let line = line.clone();
        let pill = pill.clone();
        Rc::new(move || {
            let Some(shared) = shared.upgrade() else {
                return;
            };
            recompute(&shared, &scrolled, &line_box, &line, &pill);
        })
    };

    let adjustment = scrolled.vadjustment();
    {
        let run = run.clone();
        adjustment.connect_value_changed(move |_| run());
    }
    {
        let run = run.clone();
        adjustment.connect_changed(move |_| run());
    }
    {
        let run = run.clone();
        shared.selection.connect_items_changed(move |_, _, _, _| {
            let run = run.clone();
            glib::idle_add_local_once(move || run());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // UX FIL-3: with a short list the line sits directly under the last row,
    // never at the viewport bottom (grilled acceptance case).
    #[test]
    fn fil_3_line_sits_under_the_last_row_of_a_short_list() {
        assert_eq!(end_line_margin(300.0, 800.0, 0.0), Some(300));
    }

    // UX FIL-3: with a long list the line only exists once the list end
    // scrolls into the viewport.
    #[test]
    fn fil_3_line_appears_only_when_the_end_scrolls_into_view() {
        assert_eq!(end_line_margin(5000.0, 800.0, 3000.0), None);
        assert_eq!(end_line_margin(5000.0, 800.0, 4300.0), Some(700));
        assert_eq!(end_line_margin(5000.0, 800.0, 4200.0), Some(800));
    }

    // UX FIL-3: degenerate geometry never yields a position.
    #[test]
    fn fil_3_no_line_without_geometry() {
        assert_eq!(end_line_margin(0.0, 800.0, 0.0), None);
        assert_eq!(end_line_margin(300.0, 0.0, 0.0), None);
    }
}
