//! The removable search chip every list view puts first in its filter bar.
//!
//! FIL-1a made the header search a chip in the Library's filter row. FIL-1d
//! extends that to every other list: same shape, same × affordance, same
//! position — only the wording changes, because each view names the fields it
//! actually reads. Building it once here is what keeps that promise
//! mechanical rather than copied into six filter bars.

use gtk4::prelude::*;
use reprise_view::search_scope::SearchScope;

use super::browse_bar::CHIP_CSS_CLASS;
use crate::ui::browse_filter_strings as filter_strings;

/// The minimum ×-click target FIL-1a requires of the chip.
const CHIP_MIN_HIT_PX: i32 = 20;

/// Builds the chip for `query` in `scope`. `on_clear` runs when its × is
/// activated and must remove the query alone, never the facet chips beside it.
pub(in crate::ui) fn build(
    scope: SearchScope,
    query: &str,
    on_clear: impl Fn() + 'static,
) -> gtk4::Button {
    let query = query.trim();
    let button = gtk4::Button::with_label(&format!(
        "{}  ×",
        filter_strings::scoped_search_chip_label(scope, query)
    ));
    button.add_css_class("flat");
    button.add_css_class(CHIP_CSS_CLASS);
    button.set_size_request(-1, CHIP_MIN_HIT_PX);
    button.update_property(&[gtk4::accessible::Property::Label(
        &filter_strings::remove_search_label(query),
    )]);
    button.connect_clicked(move |_| on_clear());
    button
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    // UX FIL-1d: the chip carries the scoped wording and the FIL-1a remove
    // affordance — a ≥ 20 px × target whose accessible name says "Remove
    // search", not "Remove filter".
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_1d_chip_is_removable_and_names_its_scope() {
        gtk4::init().unwrap();
        let cleared = Rc::new(Cell::new(false));
        let flag = cleared.clone();

        let chip = build(SearchScope::Podcasts, "  wer  ", move || flag.set(true));

        assert_eq!(
            chip.label().as_deref(),
            Some("⌕ “wer” in episode titles  ×")
        );
        assert!(chip.has_css_class(CHIP_CSS_CLASS));
        assert_eq!(chip.height_request(), CHIP_MIN_HIT_PX);
        chip.emit_clicked();
        assert!(cleared.get(), "the × must clear the query");
    }
}
