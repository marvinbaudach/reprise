//! Test-only semantic walker for custom focus stops.
//!
//! Native GTK controls supply their role, name, state, and action contract.
//! Reprise must explicitly provide that contract when it makes a passive
//! drawing area, box, or progress indicator focusable.

use gtk4::prelude::*;

pub(crate) fn custom_semantic_violations(root: &gtk4::Widget) -> Vec<String> {
    let mut violations = Vec::new();
    walk(root, &mut violations);
    violations
}

fn walk(widget: &gtk4::Widget, violations: &mut Vec<String>) {
    check_custom_focus_stop(widget, violations);
    let mut child = widget.first_child();
    while let Some(next) = child {
        walk(&next, violations);
        child = next.next_sibling();
    }
}

fn check_custom_focus_stop(widget: &gtk4::Widget, violations: &mut Vec<String>) {
    if !widget.is_focusable() {
        return;
    }
    if widget.is::<gtk4::DrawingArea>() {
        require_role(widget, gtk4::AccessibleRole::Slider, violations);
        for property in [
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::ValueMin,
            gtk4::AccessibleProperty::ValueMax,
            gtk4::AccessibleProperty::ValueNow,
            gtk4::AccessibleProperty::ValueText,
        ] {
            require_property(widget, property, violations);
        }
    } else if widget.is::<gtk4::Box>() {
        require_role(widget, gtk4::AccessibleRole::Group, violations);
        for property in [
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::HasPopup,
            gtk4::AccessibleProperty::KeyShortcuts,
        ] {
            require_property(widget, property, violations);
        }
    } else if widget.is::<gtk4::ProgressBar>() {
        require_role(widget, gtk4::AccessibleRole::ProgressBar, violations);
        require_property(widget, gtk4::AccessibleProperty::Label, violations);
    }
}

fn require_role(widget: &gtk4::Widget, role: gtk4::AccessibleRole, violations: &mut Vec<String>) {
    if !gtk4::test_accessible_has_role(widget, role) {
        violations.push(format!(
            "focusable {} has role {:?}, expected {role:?}",
            widget.type_().name(),
            widget.accessible_role()
        ));
    }
}

fn require_property(
    widget: &gtk4::Widget,
    property: gtk4::AccessibleProperty,
    violations: &mut Vec<String>,
) {
    if !gtk4::test_accessible_has_property(widget, property) {
        violations.push(format!(
            "focusable {} lacks {property:?}",
            widget.type_().name()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_2_every_interactive_surface_has_name_role_state_and_action() {
        gtk4::init().unwrap();
        let waveform = crate::ui::player_bar::waveform_seek::WaveformSeek::new();

        assert!(custom_semantic_violations(waveform.widget().upcast_ref()).is_empty());
        assert!(gtk4::test_accessible_has_role(
            waveform.widget(),
            gtk4::AccessibleRole::Slider
        ));
        for property in [
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::KeyShortcuts,
            gtk4::AccessibleProperty::ValueMin,
            gtk4::AccessibleProperty::ValueMax,
            gtk4::AccessibleProperty::ValueNow,
            gtk4::AccessibleProperty::ValueText,
        ] {
            assert!(gtk4::test_accessible_has_property(
                waveform.widget(),
                property
            ));
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn custom_widget_walk_rejects_an_unnamed_focus_stop() {
        gtk4::init().unwrap();
        let area = gtk4::DrawingArea::new();
        area.set_focusable(true);

        let violations = custom_semantic_violations(area.upcast_ref());

        assert!(violations
            .iter()
            .any(|item| item.contains("expected Slider")));
        assert!(violations.iter().any(|item| item.contains("Label")));
    }
}
