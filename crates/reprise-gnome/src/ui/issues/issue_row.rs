//! Shared issue row with hover actions.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

const IDLE_CHILD: &str = "idle";
const ACTIONS_CHILD: &str = "actions";
const HOVER_CROSSFADE_MS: u32 = 100;

fn actions_visible(pointer_inside: bool, focus_inside: bool) -> bool {
    pointer_inside || focus_inside
}

/// One keyboard-accessible action shown on row hover.
#[derive(Clone)]
pub(in crate::ui) struct IssuePill {
    label: String,
    css_class: Option<String>,
    on_clicked: Rc<dyn Fn()>,
}

impl IssuePill {
    pub(in crate::ui) fn new(label: impl Into<String>, on_clicked: impl Fn() + 'static) -> Self {
        Self {
            label: label.into(),
            css_class: None,
            on_clicked: Rc::new(on_clicked),
        }
    }

    pub(in crate::ui) fn with_css_class(mut self, css_class: impl Into<String>) -> Self {
        self.css_class = Some(css_class.into());
        self
    }

    fn button(&self) -> gtk4::Button {
        let button = gtk4::Button::with_label(&self.label);
        button.add_css_class("flat");
        button.add_css_class("pill");
        button.add_css_class("issue-row-pill");
        if let Some(css_class) = &self.css_class {
            button.add_css_class(css_class);
        }

        let on_clicked = self.on_clicked.clone();
        button.connect_clicked(move |_| on_clicked());
        button
    }
}

/// Text, optional artwork, resting status, and hover actions for one row.
pub(in crate::ui) struct RowSpec {
    pub(in crate::ui) cover: Option<gtk4::Widget>,
    pub(in crate::ui) primary: String,
    pub(in crate::ui) secondary: String,
    pub(in crate::ui) tertiary: String,
    pub(in crate::ui) right_idle: String,
    pub(in crate::ui) pills: Vec<IssuePill>,
}

/// A selectable 42-pixel issue row whose right edge crossfades on hover.
pub(in crate::ui) struct IssueRow {
    root: gtk4::ListBoxRow,
}

impl IssueRow {
    pub(in crate::ui) fn new(spec: RowSpec) -> Self {
        let root = gtk4::ListBoxRow::new();
        root.set_selectable(true);

        let grid = gtk4::Grid::new();
        grid.set_column_spacing(12);
        grid.set_hexpand(true);
        grid.set_valign(gtk4::Align::Center);

        if let Some(cover) = spec.cover {
            cover.set_size_request(30, 30);
            cover.add_css_class("issue-row-cover");
            grid.attach(&cover, 0, 0, 1, 1);
        }

        let primary = row_label(&spec.primary, "issue-row-primary", true);
        grid.attach(&primary, 1, 0, 1, 1);

        let secondary = row_label(&spec.secondary, "issue-row-secondary", false);
        secondary.set_width_chars(18);
        grid.attach(&secondary, 2, 0, 1, 1);

        let tertiary = row_label(&spec.tertiary, "issue-row-tertiary", false);
        tertiary.set_width_chars(20);
        grid.attach(&tertiary, 3, 0, 1, 1);

        let right = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .transition_duration(HOVER_CROSSFADE_MS)
            .halign(gtk4::Align::End)
            .build();

        let idle = gtk4::Label::new(Some(&spec.right_idle));
        idle.add_css_class("issue-row-idle");
        right.add_named(&idle, Some(IDLE_CHILD));

        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let has_actions = !spec.pills.is_empty();
        for pill in spec.pills {
            actions.append(&pill.button());
        }
        right.add_named(&actions, Some(ACTIONS_CHILD));
        right.set_visible_child_name(IDLE_CHILD);
        grid.attach(&right, 4, 0, 1, 1);

        if has_actions {
            let pointer_inside = Rc::new(Cell::new(false));
            let focus_inside = Rc::new(Cell::new(false));
            let motion = gtk4::EventControllerMotion::new();
            let enter_stack = right.clone();
            let enter_pointer = pointer_inside.clone();
            let enter_focus = focus_inside.clone();
            motion.connect_enter(move |_, _, _| {
                enter_pointer.set(true);
                set_actions_visibility(&enter_stack, true, enter_focus.get());
            });
            let leave_stack = right.clone();
            let leave_pointer = pointer_inside.clone();
            let leave_focus = focus_inside.clone();
            motion.connect_leave(move |_| {
                leave_pointer.set(false);
                set_actions_visibility(&leave_stack, false, leave_focus.get());
            });
            root.add_controller(motion);

            let focus = gtk4::EventControllerFocus::new();
            let focus_stack = right.clone();
            let focus_pointer = pointer_inside.clone();
            let focus_inside_enter = focus_inside.clone();
            focus.connect_enter(move |_| {
                focus_inside_enter.set(true);
                set_actions_visibility(&focus_stack, focus_pointer.get(), true);
            });
            let blur_stack = right;
            focus.connect_leave(move |_| {
                focus_inside.set(false);
                set_actions_visibility(&blur_stack, pointer_inside.get(), false);
            });
            root.add_controller(focus);
        }

        root.set_child(Some(&grid));
        Self { root }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ListBoxRow {
        &self.root
    }
}

fn set_actions_visibility(stack: &gtk4::Stack, pointer_inside: bool, focus_inside: bool) {
    stack.set_visible_child_name(if actions_visible(pointer_inside, focus_inside) {
        ACTIONS_CHILD
    } else {
        IDLE_CHILD
    });
}

fn row_label(text: &str, css_class: &str, expand: bool) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_hexpand(expand);
    label.add_css_class(css_class);
    label
}

#[cfg(test)]
mod tests {
    use super::actions_visible;

    #[test]
    fn row_actions_remain_visible_for_keyboard_focus() {
        assert!(actions_visible(false, true));
        assert!(actions_visible(true, false));
        assert!(actions_visible(true, true));
        assert!(!actions_visible(false, false));
    }
}
