//! The chip model every filter bar builds from: a search chip (a magnifier
//! and the bare query), a facet chip (a muted field prefix and its value),
//! or a bare chip that is its own name (such as the "Hide AI music"
//! toggle). Moved out of `reprise-gnome`'s GTK chip widget so every surface
//! shares the same lead/value/remove-label decisions; only the widget tree
//! itself stays behind in the frontend.

use crate::search_chip::{committed_query, SearchSurface};
use crate::strings::browse::{remove_filter_label, remove_search_label};
use crate::strings::Message;

/// What a chip shows ahead of its value. Owns its field content, unlike the
/// GTK-only predecessor this replaces, so it can cross the crate boundary
/// into every frontend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChipLead {
    /// A chip that came from the search: the magnifier marks its origin.
    Search,
    /// A chip that came from "+ Add filter": its field, muted, as a prefix.
    Field(String),
    /// A chip that is its own name, such as the "Hide AI music" toggle.
    Bare,
}

/// The accessible name a chip's × carries. Most chips resolve to a shared,
/// translatable [`Message`]; a chip built from a surface this crate does not
/// own — such as the GTK-only "Hide AI music" toggle — hands over its own
/// already-rendered text instead, rather than this crate inventing a catalog
/// entry it cannot back with a translation. `strings.rs`'s `Plural` doc
/// comment explains the same trade for a related case: a "message" whose id
/// never reaches gettext belongs in a variant that says so, not in a
/// `Message` whose id nothing backs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChipRemoveLabel {
    Translatable(Message),
    Resolved(String),
}

/// Everything one filter-bar chip needs to be built: what it leads with, the
/// value it shows, and the accessible name of its remove affordance. A
/// frontend renders this; it decides nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterChipModel {
    pub lead: ChipLead,
    pub value: String,
    pub accessible_remove_label: ChipRemoveLabel,
}

impl FilterChipModel {
    /// FIL-1d: the search chip built from a committed query. A blank or
    /// whitespace-only query is not a chip — `None`, not an empty one.
    ///
    /// Reuses [`committed_query`]'s trim-and-empty rule rather than
    /// repeating it: that function decides whether an *editable* search
    /// surface commits its query at all, this one decides whether an
    /// already-committed query renders as a chip, and the two must agree on
    /// what counts as empty.
    pub fn search(query: &str) -> Option<Self> {
        let query = committed_query(query, SearchSurface::Closed)?;
        Some(Self {
            lead: ChipLead::Search,
            value: query.to_owned(),
            accessible_remove_label: ChipRemoveLabel::Translatable(remove_search_label(query)),
        })
    }

    /// A facet chip built from "+ Add filter": its field, muted, ahead of
    /// the value.
    pub fn facet(field: &str, value: &str) -> Self {
        Self {
            lead: ChipLead::Field(field.to_owned()),
            value: value.to_owned(),
            accessible_remove_label: ChipRemoveLabel::Translatable(remove_filter_label(
                field, value,
            )),
        }
    }

    /// A chip that is its own name, such as "Hide AI music" (FIL-7). Its
    /// caller already rendered the accessible remove label — see
    /// [`ChipRemoveLabel::Resolved`].
    pub fn bare(value: &str, accessible_remove_label: impl Into<String>) -> Self {
        Self {
            lead: ChipLead::Bare,
            value: value.to_owned(),
            accessible_remove_label: ChipRemoveLabel::Resolved(accessible_remove_label.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_rejects_blank_and_whitespace_only_queries() {
        assert!(FilterChipModel::search("").is_none());
        assert!(FilterChipModel::search("   ").is_none());
    }

    #[test]
    fn search_builds_a_search_lead_chip_from_the_trimmed_query() {
        let chip = FilterChipModel::search("  wer  ").expect("non-blank query is a chip");
        assert_eq!(chip.lead, ChipLead::Search);
        assert_eq!(chip.value, "wer");
        assert_eq!(
            chip.accessible_remove_label,
            ChipRemoveLabel::Translatable(remove_search_label("wer"))
        );
    }

    #[test]
    fn facet_builds_a_field_lead_chip_with_the_shared_remove_message() {
        let chip = FilterChipModel::facet("Genre", "Metal");
        assert_eq!(chip.lead, ChipLead::Field("Genre".to_owned()));
        assert_eq!(chip.value, "Metal");
        assert_eq!(
            chip.accessible_remove_label,
            ChipRemoveLabel::Translatable(remove_filter_label("Genre", "Metal"))
        );
    }

    #[test]
    fn bare_builds_a_leadless_chip_with_its_caller_s_resolved_label() {
        let chip = FilterChipModel::bare("Hide AI music", "Remove filter: Hide AI music");
        assert_eq!(chip.lead, ChipLead::Bare);
        assert_eq!(chip.value, "Hide AI music");
        assert_eq!(
            chip.accessible_remove_label,
            ChipRemoveLabel::Resolved("Remove filter: Hide AI music".to_owned())
        );
    }
}
