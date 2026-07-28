//! `SRC-7`: the one row-level add action shared by Podcasts, YouTube and Radio.
//!
//! Every discovery row in all three add dialogs offers the same compact
//! `+ Add`. Once a source is in the library the very same control becomes an
//! inactive `✓ Added` — the row is not removed, so the user sees that the add
//! landed. Only the next submitted search drops it (`SRC-5`).
//!
//! The visible label is deliberately short, so it cannot name the source on its
//! own. The accessible name and tooltip therefore always carry the full
//! sentence.

use gtk4::prelude::*;

use crate::ui::strings;

/// What adding this row means for the user — subscribing to a feed or channel,
/// or adding a station.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AddActionKind {
    Subscribe,
    Add,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AddActionState {
    Offered,
    Added,
}

pub(super) const fn action_label(state: AddActionState) -> &'static str {
    match state {
        AddActionState::Offered => strings::SOURCE_ADD,
        AddActionState::Added => strings::SOURCE_ADDED,
    }
}

pub(super) const fn action_icon(state: AddActionState) -> &'static str {
    match state {
        AddActionState::Offered => "list-add-symbolic",
        AddActionState::Added => "object-select-symbolic",
    }
}

/// The full sentence a screen reader announces. Never just "Add".
pub(super) fn accessible_name(kind: AddActionKind, state: AddActionState, source: &str) -> String {
    match state {
        AddActionState::Added => strings::source_added_accessible(source),
        AddActionState::Offered => match kind {
            AddActionKind::Subscribe => strings::source_subscribe_accessible(source),
            AddActionKind::Add => strings::source_add_accessible(source),
        },
    }
}

fn apply(button: &gtk4::Button, kind: AddActionKind, state: AddActionState, source: &str) {
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    content.append(&gtk4::Image::from_icon_name(action_icon(state)));
    content.append(&gtk4::Label::new(Some(&strings::text(action_label(state)))));
    button.set_child(Some(&content));

    let name = accessible_name(kind, state, source);
    button.set_tooltip_text(Some(&name));
    button.update_property(&[gtk4::accessible::Property::Label(&name)]);
}

/// A row's offered `+ Add` action.
pub(super) fn add_button(kind: AddActionKind, source: &str) -> gtk4::Button {
    let button = gtk4::Button::new();
    button.add_css_class("reprise-source-add");
    // A compact row action, not a heavy primary button — the dialog's own
    // footer keeps the suggested styling.
    button.add_css_class("flat");
    apply(&button, kind, AddActionState::Offered, source);
    button
}

/// Turn an offered action into the inactive `✓ Added` acknowledgement.
pub(super) fn mark_added(button: &gtk4::Button, kind: AddActionKind, source: &str) {
    apply(button, kind, AddActionState::Added, source);
    button.add_css_class("reprise-source-added");
    // Muted through the theme's own token rather than a bespoke colour, so the
    // acknowledgement reads as settled in both light and dark.
    button.add_css_class("dim-label");
    button.set_sensitive(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_7_every_source_offers_the_same_compact_add_action() {
        assert_eq!(action_label(AddActionState::Offered), strings::SOURCE_ADD);
        assert_eq!(action_label(AddActionState::Added), strings::SOURCE_ADDED);
        assert_ne!(
            action_icon(AddActionState::Offered),
            action_icon(AddActionState::Added),
            "offered and added must not rely on two near-identical theme glyphs"
        );
    }

    #[test]
    fn src_7_the_accessible_name_always_names_the_source() {
        let subscribe = accessible_name(
            AddActionKind::Subscribe,
            AddActionState::Offered,
            "Dark Cabin Riffs",
        );
        let add = accessible_name(AddActionKind::Add, AddActionState::Offered, "RADIO BOB");

        assert!(subscribe.contains("Dark Cabin Riffs"));
        assert!(add.contains("RADIO BOB"));
        assert_ne!(
            subscribe, add,
            "subscribing to a channel and adding a station must not read alike"
        );
    }

    #[test]
    fn src_7_an_added_source_is_acknowledged_rather_than_removed() {
        let added = accessible_name(
            AddActionKind::Subscribe,
            AddActionState::Added,
            "Dark Cabin Riffs",
        );

        assert!(added.contains("Dark Cabin Riffs"));
        assert_ne!(
            added,
            accessible_name(
                AddActionKind::Subscribe,
                AddActionState::Offered,
                "Dark Cabin Riffs"
            )
        );
    }
}
