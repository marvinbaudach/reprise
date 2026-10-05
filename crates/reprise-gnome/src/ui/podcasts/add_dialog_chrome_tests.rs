use super::*;

fn children(widget: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut found = Vec::new();
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        found.push(current);
    }
    found
}

fn kind_names(widgets: &[gtk4::Widget]) -> Vec<String> {
    widgets
        .iter()
        .map(|widget| widget.type_().name().to_owned())
        .collect()
}

fn class_set(widget: &impl IsA<gtk4::Widget>) -> Vec<String> {
    let mut classes: Vec<String> = widget
        .css_classes()
        .iter()
        .map(ToString::to_string)
        .collect();
    classes.sort();
    classes
}

fn button_label(widget: &gtk4::Widget) -> String {
    widget
        .clone()
        .downcast::<gtk4::Button>()
        .expect("a footer child must be a button")
        .label()
        .map(|label| label.to_string())
        .unwrap_or_default()
}

/// Pins the dialog chrome the Podcasts/YouTube dialog builds today: child
/// order (which is focus order), the widget kinds, every label and class, the
/// spacing, margins and the dialog's own title and size.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn add_dialog_chrome_is_built_in_the_documented_order() {
    gtk4::init().unwrap();
    let surface = build_surface(PodcastKind::Rss, Connectivity::Online, true, "DE", None);

    let toolbar = surface
        .dialog
        .child()
        .expect("the dialog carries a child")
        .downcast::<adw::ToolbarView>()
        .expect("the dialog child is a toolbar view");
    let content = toolbar
        .content()
        .expect("the toolbar carries the content box")
        .downcast::<gtk4::Box>()
        .expect("the content is a box");
    let kids = children(content.upcast_ref());

    // The `Button` is the Apple charts chip (`SRC-19`) for an online Rss dialog.
    assert_eq!(
        kind_names(&kids),
        [
            "GtkSearchEntry",
            "GtkButton",
            "GtkLabel",
            "GtkScrolledWindow",
            "GtkLabel",
            "GtkBox"
        ]
    );
    assert_eq!(content.spacing(), 12);
    assert_eq!(content.orientation(), gtk4::Orientation::Vertical);
    for margin in [
        content.margin_top(),
        content.margin_bottom(),
        content.margin_start(),
        content.margin_end(),
    ] {
        assert_eq!(margin, 18);
    }

    let entry = kids[0]
        .clone()
        .downcast::<gtk4::SearchEntry>()
        .expect("the entry comes first");
    assert_eq!(
        entry.placeholder_text().as_deref(),
        Some(strings::text(strings::PODCAST_DIALOG_HINT).as_str())
    );

    let status = kids[2]
        .clone()
        .downcast::<gtk4::Label>()
        .expect("the status label follows the chip");
    assert_eq!(class_set(&status), ["reprise-text-secondary"]);
    assert!(status.xalign().abs() < f32::EPSILON);
    assert!(!status.wraps());

    let footnote = kids[4]
        .clone()
        .downcast::<gtk4::Label>()
        .expect("the footnote follows the results");
    assert_eq!(
        footnote.text().as_str(),
        strings::text(strings::SOURCE_SUBSCRIBED_DROP_OUT)
    );
    assert_eq!(class_set(&footnote), ["caption", "reprise-text-secondary"]);
    assert!(footnote.wraps());
    assert!(footnote.xalign().abs() < f32::EPSILON);

    let footer = kids[5]
        .clone()
        .downcast::<gtk4::Box>()
        .expect("the footer is last");
    assert_eq!(footer.halign(), gtk4::Align::End);
    assert_eq!(footer.orientation(), gtk4::Orientation::Horizontal);
    assert_eq!(footer.spacing(), 8);
    let buttons = children(footer.upcast_ref());
    assert_eq!(kind_names(&buttons), ["GtkButton", "GtkButton"]);
    assert_eq!(
        button_label(&buttons[0]),
        strings::text(strings::PODCAST_CANCEL)
    );
    assert_eq!(
        button_label(&buttons[1]),
        strings::text(strings::PODCAST_SEARCH)
    );
    assert!(buttons[1].has_css_class("suggested-action"));
    assert!(!buttons[1].is_sensitive());

    assert_eq!(
        surface.dialog.title().as_str(),
        strings::text(strings::PODCAST_DIALOG_TITLE)
    );
    assert_eq!(surface.dialog.content_width(), 620);
    assert_eq!(surface.dialog.content_height(), 560);
}
