//! Concerts-specific FIL-3a copy on the shared measured-overlay geometry.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

const LINE_TOP_GAP: i32 = 12;
const PILL_TOP_GAP: i32 = 44;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Input {
    pub shown: usize,
    pub total: usize,
    pub query: String,
    pub facets_restrict: bool,
    pub radius_km: Option<f64>,
    pub city: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Presentation {
    line: String,
    recovery: String,
}

fn presentation(input: &Input) -> Option<Presentation> {
    let query = input.query.trim();
    if input.shown == 0
        || input.shown >= input.total
        || (query.is_empty() && !input.facets_restrict)
    {
        return None;
    }
    let hidden = input.total - input.shown;
    let line = match (query.is_empty(), input.radius_km) {
        (true, Some(radius)) => crate::ui::strings::concerts_end_of_radius(
            hidden,
            radius,
            input.city.as_deref().filter(|city| !city.trim().is_empty()),
        ),
        (false, _) if input.facets_restrict => crate::ui::strings::end_of_results_hidden_by_both(
            crate::ui::end_of_results::ResultsUnit::Concerts,
            hidden,
        ),
        (false, _) => crate::ui::strings::end_of_results_hidden_by_search(
            crate::ui::end_of_results::ResultsUnit::Concerts,
            hidden,
            query,
        ),
        (true, None) => crate::ui::strings::end_of_results_hidden_by_filters(
            crate::ui::end_of_results::ResultsUnit::Concerts,
            hidden,
        ),
    };
    Some(Presentation {
        line,
        recovery: crate::ui::strings::show_all_concerts(input.total),
    })
}

pub(super) struct ConcertsEndOfResults {
    scrolled: gtk4::ScrolledWindow,
    content: gtk4::Widget,
    line_box: gtk4::Box,
    line: gtk4::Label,
    pill: gtk4::Button,
    input: RefCell<Input>,
}

impl ConcertsEndOfResults {
    pub(super) fn install(
        overlay: &gtk4::Overlay,
        scrolled: &gtk4::ScrolledWindow,
        content: &impl IsA<gtk4::Widget>,
    ) -> Rc<Self> {
        let line = gtk4::Label::new(None);
        line.add_css_class("dim-label");
        line.add_css_class("caption");
        line.add_css_class(crate::ui::end_of_results::LINE_CSS_CLASS);
        line.set_halign(gtk4::Align::Center);

        let line_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        line_box.set_valign(gtk4::Align::Start);
        line_box.set_halign(gtk4::Align::Fill);
        line_box.set_can_target(false);
        line_box.append(&line);

        let pill = gtk4::Button::new();
        pill.add_css_class("pill");
        pill.add_css_class(crate::ui::end_of_results::RECOVERY_CSS_CLASS);
        pill.set_valign(gtk4::Align::Start);
        pill.set_halign(gtk4::Align::Center);
        line_box.set_visible(false);
        pill.set_visible(false);
        overlay.add_overlay(&line_box);
        overlay.add_overlay(&pill);

        let result = Rc::new(Self {
            scrolled: scrolled.clone(),
            content: content.clone().upcast(),
            line_box,
            line,
            pill,
            input: RefCell::new(Input::default()),
        });
        let adjustment = scrolled.vadjustment();
        {
            let result = Rc::downgrade(&result);
            adjustment.connect_value_changed(move |_| {
                if let Some(result) = result.upgrade() {
                    result.recompute();
                }
            });
        }
        {
            let result = Rc::downgrade(&result);
            adjustment.connect_changed(move |_| {
                if let Some(result) = result.upgrade() {
                    result.recompute();
                }
            });
        }
        result
    }

    pub(super) fn connect_recover(&self, callback: impl Fn() + 'static) {
        self.pill.connect_clicked(move |_| callback());
    }

    pub(super) fn update(self: &Rc<Self>, input: Input) {
        self.input.replace(input);
        self.recompute();
        let result = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(result) = result.upgrade() {
                result.recompute();
            }
        });
    }

    fn recompute(&self) {
        let Some(presentation) = presentation(&self.input.borrow()) else {
            self.hide();
            return;
        };
        let adjustment = self.scrolled.vadjustment();
        let (_, natural) = self.content.preferred_size();
        let margin = crate::ui::end_of_results::end_line_margin(
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
    fn radius_copy_names_hidden_concerts_radius_and_city() {
        let result = presentation(&Input {
            shown: 3,
            total: 415,
            facets_restrict: true,
            radius_km: Some(500.0),
            city: Some("Zürich".into()),
            ..Input::default()
        })
        .unwrap();
        assert_eq!(
            result.line,
            "End of results — 412 concerts hidden by the 500 km radius around Zürich"
        );
        assert_eq!(result.recovery, "Show all 415 concerts");
    }
}
