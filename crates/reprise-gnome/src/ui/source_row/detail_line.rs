//! The second line of a source row: a dot-separated chain and one chip.

use gtk4::prelude::*;

/// Joins with " · " and drops anything that trims to nothing, so a caller can
/// pass every field it has and let the absent ones disappear.
pub(in crate::ui) fn detail_line<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

#[allow(dead_code)]
pub(in crate::ui) struct ChipSpec {
    pub label: String,
    pub css_class: &'static str,
}

#[allow(dead_code)]
pub(in crate::ui) fn chip(spec: &ChipSpec) -> gtk4::Label {
    let label = gtk4::Label::new(Some(&spec.label));
    label.add_css_class("reprise-source-row-chip");
    label.add_css_class(spec.css_class);
    label.set_valign(gtk4::Align::Center);
    label
}

/// Whole percent of the episode already heard, clamped away from the two ends
/// that would misread as "new" or "played".
#[allow(dead_code)]
pub(in crate::ui) fn resume_percent(position_ms: i64, duration_secs: Option<i64>) -> Option<u8> {
    let duration_secs = duration_secs.filter(|secs| *secs > 0)?;
    let total_ms = duration_secs.saturating_mul(1_000);
    let percent = (position_ms.max(0) as f64 / total_ms as f64 * 100.0).round();
    Some(percent.clamp(1.0, 99.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SRC-16`: a missing value leaves no trace. Radio will lean on this
    /// hardest — a station with only a bitrate must read "128k", never
    /// "— · 128k · — · —".
    #[test]
    fn src_16_the_detail_line_drops_empty_values() {
        assert_eq!(detail_line(["31 Jul", "36 min"]), "31 Jul · 36 min");
        assert_eq!(detail_line(["", "36 min", ""]), "36 min");
        assert_eq!(detail_line(["  ", ""]), "");
    }

    /// The percentage is the point of the Resume chip: "Resume" alone says
    /// nothing a play button does not already say.
    #[test]
    fn src_16_resume_reports_a_whole_percent_and_omits_it_without_a_duration() {
        assert_eq!(resume_percent(1_800_000, Some(3_600)), Some(50));
        assert_eq!(resume_percent(0, Some(3_600)), Some(1));
        assert_eq!(resume_percent(3_600_000, Some(3_600)), Some(99));
        assert_eq!(resume_percent(1_000, None), None);
        assert_eq!(resume_percent(1_000, Some(0)), None);
    }
}
