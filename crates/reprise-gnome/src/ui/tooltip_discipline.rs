//! Test-only widget walk asserting UX TIP-1a: icon-only buttons carry a
//! tooltip, visibly labeled buttons carry none (the label is the statement).
use gtk4::prelude::*;

pub(crate) fn tooltip_violations(root: &gtk4::Widget) -> Vec<String> {
    let mut violations = Vec::new();
    walk(root, &mut violations);
    violations
}

fn walk(widget: &gtk4::Widget, violations: &mut Vec<String>) {
    check(widget, violations);
    let mut child = widget.first_child();
    while let Some(next) = child {
        walk(&next, violations);
        child = next.next_sibling();
    }
}

fn check(widget: &gtk4::Widget, violations: &mut Vec<String>) {
    let (icon, label) = if let Some(button) = widget.downcast_ref::<gtk4::Button>() {
        (button.icon_name(), button.label())
    } else if let Some(menu_button) = widget.downcast_ref::<gtk4::MenuButton>() {
        (menu_button.icon_name(), menu_button.label())
    } else {
        return;
    };
    let tooltip = widget.tooltip_text();
    match (icon, label) {
        (Some(icon), None) if tooltip.as_deref().unwrap_or("").is_empty() => {
            violations.push(format!("icon-only button `{icon}` has no tooltip"));
        }
        (_, Some(label)) if tooltip.is_some() => {
            violations.push(format!(
                "labeled button `{label}` carries a redundant tooltip"
            ));
        }
        _ => {}
    }
}
