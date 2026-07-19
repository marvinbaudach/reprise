//! Isolated E2E evidence for GTK's real focus widget.

use std::path::Path;

use gtk4::prelude::*;
use libadwaita as adw;

const FOCUS_STATE_ENV: &str = "REPRISE_SMOKE_FOCUS_STATE";

fn evidence_line_with_label(widget_type: Option<&str>, label: Option<&str>) -> String {
    let mut evidence = format!("widget={}\n", widget_type.unwrap_or("none"));
    if let Some(label) = label {
        let single_line = label.replace(['\n', '\r'], " ");
        evidence.push_str(&format!("label={single_line}\n"));
    }
    evidence
}

fn first_label(widget: &gtk4::Widget) -> Option<String> {
    if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
        let text = label.text();
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    let mut child = widget.first_child();
    while let Some(next) = child {
        if let Some(label) = first_label(&next) {
            return Some(label);
        }
        child = next.next_sibling();
    }
    None
}

fn preferred_focus_label<'a>(
    descendant_label: Option<&'a str>,
    tooltip: Option<&'a str>,
) -> Option<&'a str> {
    descendant_label
        .filter(|label| !label.is_empty())
        .or_else(|| tooltip.filter(|label| !label.is_empty()))
}

fn write_current(window: &adw::ApplicationWindow, path: &Path) {
    let focus = gtk4::prelude::GtkWindowExt::focus(window);
    let widget_type = focus
        .as_ref()
        .map(|widget| widget.type_().name().to_string());
    let label = focus.as_ref().and_then(|widget| {
        let descendant_label = first_label(widget);
        let tooltip = widget.tooltip_text();
        preferred_focus_label(descendant_label.as_deref(), tooltip.as_deref()).map(str::to_owned)
    });
    if let Err(error) = std::fs::write(
        path,
        evidence_line_with_label(widget_type.as_deref(), label.as_deref()),
    ) {
        tracing::warn!(%error, path = %path.display(), "could not write focus evidence");
    }
}

pub(in crate::ui) fn install(window: &adw::ApplicationWindow) {
    let Ok(path) = std::env::var(FOCUS_STATE_ENV) else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    write_current(window, &path);
    window.connect_focus_widget_notify(move |window| write_current(window, &path));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_evidence_distinguishes_missing_and_concrete_focus() {
        assert_eq!(evidence_line_with_label(None, None), "widget=none\n");
        assert_eq!(
            evidence_line_with_label(Some("GtkColumnView"), None),
            "widget=GtkColumnView\n"
        );
    }

    #[test]
    fn focus_evidence_identifies_the_active_collection_item() {
        assert_eq!(
            evidence_line_with_label(Some("GtkListBoxRow"), Some("Music")),
            "widget=GtkListBoxRow\nlabel=Music\n"
        );
    }

    #[test]
    fn focus_evidence_uses_tooltips_for_icon_only_controls() {
        assert_eq!(
            preferred_focus_label(None, Some("Pause (Space)")),
            Some("Pause (Space)")
        );
        assert_eq!(
            preferred_focus_label(Some("Visible label"), Some("Tooltip")),
            Some("Visible label")
        );
    }
}
