//! The three result cards.
//!
//! Each is a surface, not a paragraph: leading icon, heading, muted detail
//! lines, and the action inline at the trailing edge — never a full-width
//! button under the text. The emphasis order is fixed and is the point of the
//! whole page: the review card carries it, the applied card is a plain
//! surface, and the conflicts card — the optional, skippable one — is the
//! quietest thing on the screen and has no button at all.

use gtk4::prelude::*;
use libadwaita as adw;

/// Matches the mockup's `padding: 18px 20px` on the two filled cards.
const CARD_MARGIN_VERTICAL: i32 = 18;
const CARD_MARGIN_HORIZONTAL: i32 = 20;
/// The conflicts card sits one step tighter (`16px 20px`).
const QUIET_CARD_MARGIN_VERTICAL: i32 = 16;
const ICON_PIXEL_SIZE: i32 = 20;

pub(super) struct CardContent<'a> {
    pub(super) icon_name: &'a str,
    /// `accent` for the review card's stethoscope, `dim-label` for the muted
    /// warning, `None` for the applied card's check.
    pub(super) icon_class: Option<&'a str>,
    pub(super) heading: String,
    pub(super) heading_class: Option<&'a str>,
    pub(super) lines: Vec<String>,
    pub(super) action: Option<gtk4::Widget>,
}

/// The applied block: a plain card, `Undo` as its only control.
pub(super) fn applied_card(heading: String, lines: Vec<String>, undo: &gtk4::Button) -> adw::Bin {
    let card = card_shell(
        &CardContent {
            icon_name: "emblem-ok-symbolic",
            icon_class: Some("accent"),
            heading,
            heading_class: Some("heading"),
            lines,
            action: Some(undo.clone().upcast()),
        },
        CARD_MARGIN_VERTICAL,
    );
    card.add_css_class("card");
    card
}

/// The block that carries the emphasis: accent border, accent icon, primary
/// button.
pub(super) fn review_card(heading: String, lines: Vec<String>, review: &gtk4::Button) -> adw::Bin {
    let card = card_shell(
        &CardContent {
            icon_name: super::DOCTOR_GLYPH,
            icon_class: Some("accent"),
            heading,
            heading_class: Some("heading"),
            lines,
            action: Some(review.clone().upcast()),
        },
        CARD_MARGIN_VERTICAL,
    );
    card.add_css_class("card");
    card.add_css_class("doctor-card-accent");
    card
}

/// The quietest card: dashed outline, no fill, muted icon, no button. It is a
/// pointer to the bottom of the review page, not an action.
pub(super) fn conflicts_card(heading: String, body: String) -> adw::Bin {
    let card = card_shell(
        &CardContent {
            icon_name: "dialog-warning-symbolic",
            icon_class: Some("dim-label"),
            heading,
            heading_class: None,
            lines: vec![body],
            action: None,
        },
        QUIET_CARD_MARGIN_VERTICAL,
    );
    card.add_css_class("doctor-card-dashed");
    card
}

fn card_shell(content: &CardContent<'_>, margin_vertical: i32) -> adw::Bin {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
    row.set_margin_top(margin_vertical);
    row.set_margin_bottom(margin_vertical);
    row.set_margin_start(CARD_MARGIN_HORIZONTAL);
    row.set_margin_end(CARD_MARGIN_HORIZONTAL);

    let icon = gtk4::Image::builder()
        .icon_name(content.icon_name)
        .pixel_size(ICON_PIXEL_SIZE)
        .valign(gtk4::Align::Start)
        .build();
    if let Some(class) = content.icon_class {
        icon.add_css_class(class);
    }
    row.append(&icon);

    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    text.set_hexpand(true);
    let heading = gtk4::Label::builder()
        .label(&content.heading)
        .xalign(0.0)
        .wrap(true)
        .build();
    if let Some(class) = content.heading_class {
        heading.add_css_class(class);
    }
    text.append(&heading);
    for line in &content.lines {
        text.append(
            &gtk4::Label::builder()
                .label(line)
                .xalign(0.0)
                .wrap(true)
                .css_classes(["dim-label"])
                .build(),
        );
    }
    row.append(&text);

    if let Some(action) = &content.action {
        action.set_valign(gtk4::Align::Start);
        action.set_hexpand(false);
        row.append(action);
    }

    adw::Bin::builder().child(&row).build()
}

/// Detaches an action button from whatever card held it last, so the panel can
/// keep one button instance (and one connected signal) across re-renders.
pub(super) fn unparent_action(action: &impl IsA<gtk4::Widget>) {
    let action = action.as_ref();
    if let Some(parent) = action.parent() {
        if let Some(parent) = parent.downcast_ref::<gtk4::Box>() {
            parent.remove(action);
        }
    }
}

#[cfg(test)]
mod tests {
    use libadwaita::prelude::BinExt;

    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_9a_the_action_sits_inline_at_the_trailing_edge_top_aligned() {
        if gtk4::init().is_err() {
            return;
        }
        let undo = gtk4::Button::with_label("Undo");
        let card = applied_card("2 fixes already applied".into(), vec!["line".into()], &undo);
        let row = card
            .child()
            .expect("card has a row")
            .downcast::<gtk4::Box>()
            .expect("the card's child is the horizontal row");
        assert_eq!(row.orientation(), gtk4::Orientation::Horizontal);
        assert_eq!(
            row.last_child().as_ref(),
            Some(undo.upcast_ref::<gtk4::Widget>()),
            "the action is the trailing child, not a full-width button below the text"
        );
        assert_eq!(undo.valign(), gtk4::Align::Start);
        assert!(!undo.hexpands());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_9a_the_conflicts_card_is_the_quietest_and_carries_no_button() {
        if gtk4::init().is_err() {
            return;
        }
        let card = conflicts_card(
            "3 spelling conflicts, no clear winner".into(),
            "body".into(),
        );
        assert!(
            !card.has_css_class("card"),
            "the conflicts card has no fill — it is dashed and empty"
        );
        assert!(card.has_css_class("doctor-card-dashed"));
        let row = card
            .child()
            .expect("card has a row")
            .downcast::<gtk4::Box>()
            .expect("the card's child is the horizontal row");
        let last = row.last_child().expect("row has children");
        assert!(
            last.downcast_ref::<gtk4::Button>().is_none(),
            "no button belongs on the conflicts card"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_9a_only_the_review_card_carries_the_accent_emphasis() {
        if gtk4::init().is_err() {
            return;
        }
        let undo = gtk4::Button::with_label("Undo");
        let review = gtk4::Button::with_label("Review 3 changes");
        let applied = applied_card("2 fixes already applied".into(), Vec::new(), &undo);
        let reviewed = review_card("3 changes need your eye".into(), Vec::new(), &review);
        assert!(!applied.has_css_class("doctor-card-accent"));
        assert!(reviewed.has_css_class("doctor-card-accent"));
    }
}
