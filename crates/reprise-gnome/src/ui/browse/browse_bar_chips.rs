//! Chip and pill construction for the unified filter bar — the pure facet
//! helpers plus the FlowBox append, split out of `browse_bar.rs` to keep both
//! files under the repository's source-size limit.

use gtk4::prelude::*;
use reprise_core::queries::{BrowseFacet, BrowseFilter};
#[cfg(test)]
use reprise_view::search_scope::SearchScope;

use crate::ui::browse_filter_strings as filter_strings;

pub(super) const FACETS: [BrowseFacet; 5] = [
    BrowseFacet::Genre,
    BrowseFacet::Artist,
    BrowseFacet::Album,
    BrowseFacet::Year,
    BrowseFacet::Rating,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FilterChip {
    pub(super) facet: BrowseFacet,
    pub(super) label: String,
    pub(super) accessible_remove_label: String,
}

pub(super) fn apply_selection(
    current: &BrowseFilter,
    facet: BrowseFacet,
    value: Option<String>,
) -> BrowseFilter {
    match facet {
        // Genre → Artist → Album cascade: setting a shallower facet clears the
        // deeper ones, but the standalone Year/Rating constraints survive.
        BrowseFacet::Genre => BrowseFilter {
            genre: value,
            artist: None,
            album: None,
            ..current.clone()
        },
        BrowseFacet::Artist => BrowseFilter {
            artist: value,
            album: None,
            ..current.clone()
        },
        BrowseFacet::Album => BrowseFilter {
            album: value,
            ..current.clone()
        },
        // Year and Rating are additive: selecting one leaves every other
        // facet untouched.
        BrowseFacet::Year => BrowseFilter {
            year: value,
            ..current.clone()
        },
        BrowseFacet::Rating => BrowseFilter {
            rating: value,
            ..current.clone()
        },
    }
}

fn filter_value(filter: &BrowseFilter, facet: BrowseFacet) -> Option<&str> {
    match facet {
        BrowseFacet::Genre => filter.genre.as_deref(),
        BrowseFacet::Artist => filter.artist.as_deref(),
        BrowseFacet::Album => filter.album.as_deref(),
        BrowseFacet::Year => filter.year.as_deref(),
        BrowseFacet::Rating => filter.rating.as_deref(),
    }
}

pub(super) fn facet_label(facet: BrowseFacet) -> String {
    let message = match facet {
        BrowseFacet::Genre => filter_strings::BROWSE_GENRE,
        BrowseFacet::Artist => filter_strings::BROWSE_ARTIST,
        BrowseFacet::Album => filter_strings::BROWSE_ALBUM,
        BrowseFacet::Year => filter_strings::BROWSE_YEAR,
        BrowseFacet::Rating => filter_strings::BROWSE_RATING,
    };
    filter_strings::text(message)
}

pub(super) fn displayed_value(facet: BrowseFacet, value: &str) -> String {
    if !value.is_empty() {
        return value.to_string();
    }
    let message = match facet {
        BrowseFacet::Genre => filter_strings::UNKNOWN_GENRE,
        BrowseFacet::Artist => filter_strings::UNKNOWN_ARTIST,
        BrowseFacet::Album => filter_strings::UNKNOWN_ALBUM,
        BrowseFacet::Year => filter_strings::UNKNOWN_YEAR,
        BrowseFacet::Rating => filter_strings::UNKNOWN_RATING,
    };
    filter_strings::text(message)
}

pub(super) fn filter_chips(filter: &BrowseFilter) -> Vec<FilterChip> {
    FACETS
        .into_iter()
        .filter_map(|facet| {
            let value = displayed_value(facet, filter_value(filter, facet)?);
            let facet_name = facet_label(facet);
            Some(FilterChip {
                facet,
                label: filter_strings::chip_label(&facet_name, &value),
                accessible_remove_label: filter_strings::remove_filter_label(&facet_name, &value),
            })
        })
        .collect()
}

#[cfg(test)]
pub(in crate::ui) fn chip_labels(
    search: &str,
    filter: &BrowseFilter,
    is_library: bool,
) -> Vec<String> {
    let mut labels = Vec::new();
    if !search.trim().is_empty() {
        labels.push(filter_strings::scoped_search_chip_label(
            SearchScope::Tracks,
            search.trim(),
        ));
    }
    if is_library {
        labels.extend(filter_chips(filter).into_iter().map(|chip| chip.label));
    }
    labels
}

pub(super) fn available_facets(filter: &BrowseFilter) -> Vec<BrowseFacet> {
    FACETS
        .into_iter()
        .filter(|facet| filter_value(filter, *facet).is_none())
        .collect()
}

pub(super) fn remove_filter(filter: &BrowseFilter, facet: BrowseFacet) -> BrowseFilter {
    apply_selection(filter, facet, None)
}

pub(super) fn value_matches_search(value: &str, search: &str) -> bool {
    value.to_lowercase().contains(&search.trim().to_lowercase())
}

pub(super) fn restored_filter(filter: &BrowseFilter) -> BrowseFilter {
    filter.clone()
}

/// The place pill: outlined, prefixed with a back chevron, and deliberately
/// without a `×`. Its whole surface is the click target rather than a 20 px
/// cross, because leaving a location is a navigation, not a removal
/// (docs/ux-rules.md K, FIL-1c).
pub(super) fn build_place_pill(place: &str) -> gtk4::Button {
    let button = gtk4::Button::with_label(&format!("‹  {place}"));
    button.add_css_class("flat");
    button.add_css_class(super::browse_bar::PLACE_PILL_CSS_CLASS);
    button.set_size_request(20, 20);
    let leave_label = filter_strings::leave_place_label(place);
    button.set_tooltip_text(Some(&leave_label));
    button.update_property(&[gtk4::accessible::Property::Label(&leave_label)]);
    button
}

pub(super) fn append_chip(chips: &gtk4::FlowBox, widget: &impl IsA<gtk4::Widget>) {
    chips.append(widget);
    if let Some(wrapper) = widget
        .as_ref()
        .parent()
        .and_downcast::<gtk4::FlowBoxChild>()
    {
        wrapper.set_focusable(false);
    }
}
