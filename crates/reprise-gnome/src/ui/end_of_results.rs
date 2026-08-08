//! FIL-3a: the shared end-of-results line. A measured overlay keeps each
//! list's native row virtualization intact and appears only where the list
//! actually ends.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

/// Vertical space between the final result row and the explanatory line.
const LINE_TOP_GAP: i32 = 12;
/// Vertical space between the final result row and the recovery pill.
const PILL_TOP_GAP: i32 = 44;

pub(in crate::ui) const LINE_CSS_CLASS: &str = "reprise-end-of-results-line";
pub(in crate::ui) const RECOVERY_CSS_CLASS: &str = "reprise-end-of-results-recovery";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) struct EndOfResultsInput {
    pub shown: usize,
    pub total: usize,
    pub query: String,
    pub facets_restrict: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResultsUnit {
    Tracks,
    Episodes,
    Videos,
    Gaps,
    Stations,
    Concerts,
    Settings,
}

#[derive(Debug, PartialEq, Eq)]
struct Presentation {
    line: String,
    recovery: String,
}

fn presentation(input: &EndOfResultsInput, unit: ResultsUnit) -> Option<Presentation> {
    let query = input.query.trim();
    if input.shown == 0
        || input.shown >= input.total
        || (query.is_empty() && !input.facets_restrict)
    {
        return None;
    }
    let hidden = input.total - input.shown;
    let line = match (query.is_empty(), input.facets_restrict) {
        (false, false) => crate::ui::strings::end_of_results_hidden_by_search(unit, hidden, query),
        (true, true) => crate::ui::strings::end_of_results_hidden_by_filters(unit, hidden),
        (false, true) => crate::ui::strings::end_of_results_hidden_by_both(unit, hidden),
        (true, false) => unreachable!("the unrestricted case returns above"),
    };
    Some(Presentation {
        line,
        recovery: crate::ui::strings::end_of_results_show_all(unit, input.total),
    })
}

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

pub(in crate::ui) struct EndOfResults {
    scrolled: gtk4::ScrolledWindow,
    content: gtk4::Widget,
    line_box: gtk4::Box,
    line: gtk4::Label,
    pill: gtk4::Button,
    unit: ResultsUnit,
    input: RefCell<EndOfResultsInput>,
}

impl EndOfResults {
    pub(in crate::ui) fn install(
        overlay: &gtk4::Overlay,
        scrolled: &gtk4::ScrolledWindow,
        content: &impl IsA<gtk4::Widget>,
        unit: ResultsUnit,
    ) -> Rc<Self> {
        let line = gtk4::Label::new(None);
        line.add_css_class("dim-label");
        line.add_css_class("caption");
        line.add_css_class(LINE_CSS_CLASS);
        line.set_halign(gtk4::Align::Center);

        let line_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        line_box.set_valign(gtk4::Align::Start);
        line_box.set_halign(gtk4::Align::Fill);
        line_box.set_can_target(false);
        line_box.append(&line);

        let pill = gtk4::Button::new();
        pill.add_css_class("pill");
        pill.add_css_class(RECOVERY_CSS_CLASS);
        pill.set_valign(gtk4::Align::Start);
        pill.set_halign(gtk4::Align::Center);

        line_box.set_visible(false);
        pill.set_visible(false);
        overlay.add_overlay(&line_box);
        overlay.add_overlay(&pill);

        let end = Rc::new(Self {
            scrolled: scrolled.clone(),
            content: content.clone().upcast(),
            line_box,
            line,
            pill,
            unit,
            input: RefCell::new(EndOfResultsInput::default()),
        });
        let adjustment = scrolled.vadjustment();
        {
            let end = Rc::downgrade(&end);
            adjustment.connect_value_changed(move |_| {
                if let Some(end) = end.upgrade() {
                    end.recompute();
                }
            });
        }
        {
            let end = Rc::downgrade(&end);
            adjustment.connect_changed(move |_| {
                if let Some(end) = end.upgrade() {
                    end.recompute();
                }
            });
        }
        end
    }

    pub(in crate::ui) fn set_recovery_action_name(&self, action_name: &str) {
        self.pill.set_action_name(Some(action_name));
    }

    pub(in crate::ui) fn connect_recover(&self, callback: impl Fn() + 'static) {
        self.pill.connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn update(self: &Rc<Self>, input: EndOfResultsInput) {
        self.input.replace(input);
        self.recompute();
        let end = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(end) = end.upgrade() {
                end.recompute();
            }
        });
    }

    fn recompute(&self) {
        let Some(presentation) = presentation(&self.input.borrow(), self.unit) else {
            self.hide();
            return;
        };
        let adjustment = self.scrolled.vadjustment();
        let (_, natural) = self.content.preferred_size();
        let margin = end_line_margin(
            f64::from(natural.height()),
            adjustment.page_size(),
            adjustment.value(),
        );
        match margin {
            Some(margin) => {
                self.line.set_text(&presentation.line);
                self.pill.set_label(&presentation.recovery);
                self.line_box.set_margin_top(margin + LINE_TOP_GAP);
                self.pill.set_margin_top(margin + PILL_TOP_GAP);
                self.line_box.set_visible(true);
                self.pill.set_visible(true);
            }
            None => self.hide(),
        }
    }

    fn hide(&self) {
        self.line_box.set_visible(false);
        self.pill.set_visible(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fil_3a_presentation_requires_a_hit_and_a_hidden_row() {
        let input = EndOfResultsInput {
            shown: 1,
            total: 44,
            query: "afd".into(),
            facets_restrict: false,
        };
        let result = presentation(&input, ResultsUnit::Episodes).unwrap();
        assert_eq!(
            result.line,
            "End of results — 43 episodes hidden by search “afd”"
        );
        assert_eq!(result.recovery, "Show all 44 episodes");

        assert!(presentation(
            &EndOfResultsInput {
                shown: 0,
                ..input.clone()
            },
            ResultsUnit::Episodes,
        )
        .is_none());
        assert!(presentation(
            &EndOfResultsInput { shown: 44, ..input },
            ResultsUnit::Episodes,
        )
        .is_none());
    }

    // UX FIL-3a: with a short list the line sits directly under the last row,
    // never at the viewport bottom (grilled acceptance case).
    #[test]
    fn fil_3a_line_sits_under_the_last_row_of_a_short_list() {
        assert_eq!(end_line_margin(300.0, 800.0, 0.0), Some(300));
    }

    // UX FIL-3a: with a long list the line only exists once the list end
    // scrolls into the viewport.
    #[test]
    fn fil_3a_line_appears_only_when_the_end_scrolls_into_view() {
        assert_eq!(end_line_margin(5000.0, 800.0, 3000.0), None);
        assert_eq!(end_line_margin(5000.0, 800.0, 4300.0), Some(700));
        assert_eq!(end_line_margin(5000.0, 800.0, 4200.0), Some(800));
    }

    // UX FIL-3a: degenerate geometry never yields a position.
    #[test]
    fn fil_3a_no_line_without_geometry() {
        assert_eq!(end_line_margin(0.0, 800.0, 0.0), None);
        assert_eq!(end_line_margin(300.0, 0.0, 0.0), None);
    }
}
