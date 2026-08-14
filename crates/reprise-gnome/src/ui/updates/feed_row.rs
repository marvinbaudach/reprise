//! Shared geometry and activation model for both Updates feeds.

use std::rc::Rc;

use gtk4::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TagTone {
    Accent,
    Neutral,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Tag {
    pub text: String,
    pub tone: TagTone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Presentation {
    pub title: String,
    pub title_suffix: Option<String>,
    pub meta: String,
    pub tag: Option<Tag>,
    pub tooltip: String,
    pub activatable: bool,
}

pub(super) struct FeedRow {
    pub root: gtk4::Box,
    #[cfg(test)]
    pub activation: gtk4::Button,
    pub dismiss: gtk4::Button,
}

pub(super) fn build(
    cover: &impl IsA<gtk4::Widget>,
    presentation: Presentation,
    dismiss_tooltip: &str,
    on_activate: Rc<dyn Fn()>,
    on_dismiss: Rc<dyn Fn()>,
) -> FeedRow {
    let title = gtk4::Label::new(Some(&presentation.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::None);
    title.add_css_class("new-release-title");

    let suffix = gtk4::Label::new(presentation.title_suffix.as_deref());
    suffix.set_xalign(0.0);
    suffix.set_hexpand(true);
    suffix.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    suffix.add_css_class("new-release-title-suffix");
    suffix.set_visible(presentation.title_suffix.is_some());

    let title_line = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    title_line.append(&title);
    title_line.append(&suffix);

    let meta = gtk4::Label::new(Some(&presentation.meta));
    meta.set_xalign(0.0);
    meta.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    meta.add_css_class("new-release-meta");

    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.append(&title_line);
    text.append(&meta);

    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    content.append(cover);
    content.append(&text);
    if let Some(tag) = presentation.tag {
        let label = gtk4::Label::new(Some(&tag.text));
        label.set_valign(gtk4::Align::Center);
        label.add_css_class("updates-tag");
        label.add_css_class(match tag.tone {
            TagTone::Accent => "updates-tag-accent",
            TagTone::Neutral => "updates-tag-neutral",
        });
        content.append(&label);
    }

    let activation = gtk4::Button::builder()
        .child(&content)
        .css_classes(["flat", "new-release-activation"])
        .sensitive(presentation.activatable)
        .build();
    activation.update_property(&[gtk4::accessible::Property::Description(
        &presentation.tooltip,
    )]);
    activation.connect_clicked(move |_| on_activate());

    let dismiss = gtk4::Button::from_icon_name("view-conceal-symbolic");
    dismiss.add_css_class("flat");
    dismiss.add_css_class("new-release-action");
    dismiss.add_css_class("new-release-row-actions");
    dismiss.set_tooltip_text(Some(dismiss_tooltip));
    dismiss.update_property(&[gtk4::accessible::Property::Label(dismiss_tooltip)]);
    dismiss.connect_clicked(move |_| on_dismiss());

    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    root.add_css_class("new-release-row");
    // GTK picking skips insensitive widgets, so the tooltip must live on the
    // sensitive row wrapper even when its activation button is unavailable.
    root.set_tooltip_text(Some(&presentation.tooltip));
    root.append(&activation);
    root.append(&dismiss);

    FeedRow {
        root,
        #[cfg(test)]
        activation,
        dismiss,
    }
}
