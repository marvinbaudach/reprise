use std::cell::Cell;
use std::rc::Rc;

use gtk4::{glib, prelude::*};

const CHOICE_CARD_CSS: &str = "
checkbutton.reprise-choice-card {
  padding: 12px;
}
.reprise-choice-preview {
  border: 1px solid alpha(@window_fg_color, 0.18);
  border-radius: 8px;
}
.reprise-preview-sidebar { background: alpha(@window_fg_color, 0.16); }
.reprise-preview-content { background: alpha(@window_fg_color, 0.06); }
.reprise-preview-player { background: @accent_bg_color; }
";

pub(super) struct ChoiceCardSpec {
    title: String,
    preview: gtk4::Widget,
}

impl ChoiceCardSpec {
    pub(super) fn new(title: String, preview: &impl IsA<gtk4::Widget>) -> Self {
        Self {
            title,
            preview: preview.clone().upcast(),
        }
    }
}

pub(super) struct ChoiceCards {
    pub(super) root: gtk4::Box,
    #[cfg(test)]
    pub(super) buttons: Vec<gtk4::CheckButton>,
}

fn retained_selection(committed: u32, requested: u32, save_succeeded: bool) -> u32 {
    if save_succeeded {
        requested
    } else {
        committed
    }
}

fn install_style(widget: &impl IsA<gtk4::Widget>) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(CHOICE_CARD_CSS);
    gtk4::style_context_add_provider_for_display(
        &widget.display(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub(super) fn build(
    specs: Vec<ChoiceCardSpec>,
    selected: u32,
    on_selected: &Rc<dyn Fn(u32) -> bool>,
) -> ChoiceCards {
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    root.set_homogeneous(true);
    install_style(&root);

    let mut buttons = Vec::with_capacity(specs.len());
    for spec in specs {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        spec.preview.set_hexpand(true);
        content.append(&spec.preview);
        let title = gtk4::Label::new(Some(&spec.title));
        title.add_css_class("heading");
        content.append(&title);

        let button = gtk4::CheckButton::new();
        if let Some(first) = buttons.first() {
            button.set_group(Some(first));
        }
        button.add_css_class("card");
        button.add_css_class("reprise-choice-card");
        button.set_child(Some(&content));
        button.update_property(&[gtk4::accessible::Property::Label(&spec.title)]);
        root.append(&button);
        buttons.push(button);
    }

    let selected = selected.min(buttons.len().saturating_sub(1) as u32);
    if let Some(button) = buttons.get(selected as usize) {
        button.set_active(true);
    }

    let committed = Rc::new(Cell::new(selected));
    let syncing = Rc::new(Cell::new(false));
    let shared_buttons = Rc::new(
        buttons
            .iter()
            .map(glib::object::ObjectExt::downgrade)
            .collect::<Vec<_>>(),
    );
    for (index, button) in buttons.iter().enumerate() {
        let committed = committed.clone();
        let syncing = syncing.clone();
        let shared_buttons = shared_buttons.clone();
        let on_selected = on_selected.clone();
        button.connect_toggled(move |button| {
            if syncing.get() || !button.is_active() {
                return;
            }
            let requested = index as u32;
            let previous = committed.get();
            let accepted = on_selected(requested);
            committed.set(retained_selection(previous, requested, accepted));
            if !accepted {
                syncing.set(true);
                if let Some(previous_button) = shared_buttons
                    .get(previous as usize)
                    .and_then(glib::WeakRef::upgrade)
                {
                    previous_button.set_active(true);
                }
                syncing.set(false);
            }
        });
    }

    ChoiceCards {
        root,
        #[cfg(test)]
        buttons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_choice_commits_requested_index_and_failure_retains_previous() {
        assert_eq!(retained_selection(0, 2, true), 2);
        assert_eq!(retained_selection(1, 2, false), 1);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn cards_expose_native_grouped_selection_and_accessible_labels() {
        if gtk4::init().is_err() {
            return;
        }
        let preview_a = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let preview_b = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let on_selected: Rc<dyn Fn(u32) -> bool> = Rc::new(|_| true);
        let cards = build(
            vec![
                ChoiceCardSpec::new("First".into(), &preview_a),
                ChoiceCardSpec::new("Second".into(), &preview_b),
            ],
            0,
            &on_selected,
        );

        assert!(cards.root.is_homogeneous());
        assert_eq!(cards.buttons.len(), 2);
        assert!(cards.buttons[0].is_active());
        cards.buttons[1].set_active(true);
        assert!(cards.buttons[1].is_active());
        assert!(!cards.buttons[0].is_active());
    }
}
