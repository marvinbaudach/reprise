pub(super) fn css() -> String {
    [
        ".doctor-album-header-later { border-top: 1px solid color-mix(in srgb, currentColor 7%, transparent); padding-top: 20px; }",
        ".doctor-album-check, .doctor-review-select-all { min-width: 16px; min-height: 16px; border-radius: 4px; }",
        ".doctor-album-check:checked, .doctor-review-select-all:checked { background: var(--accent-bg-color); color: var(--window-bg-color); }",
        ".doctor-album-check:not(:checked), .doctor-review-select-all:not(:checked) { box-shadow: inset 0 0 0 1.5px color-mix(in srgb, currentColor 30%, transparent); }",
        ".doctor-album-cover { background: color-mix(in srgb, currentColor 8%, transparent); border-radius: 5px; -gtk-icon-size: 16px; }",
        ".doctor-album-title { font-size: 15px; font-weight: 500; }",
        ".doctor-album-detail { font-size: 13px; color: color-mix(in srgb, currentColor 50%, transparent); }",
        ".doctor-album-caret { color: color-mix(in srgb, currentColor 40%, transparent); }",
        ".doctor-review-row { font-size: 13.5px; }",
        ".doctor-review-row-deselected { opacity: 0.55; }",
        ".doctor-album-wide-track { color: color-mix(in srgb, currentColor 45%, transparent); }",
        ".doctor-review-arrow { color: color-mix(in srgb, currentColor 32%, transparent); }",
        ".doctor-review-current { color: color-mix(in srgb, currentColor 52%, transparent); }",
        ".doctor-current-empty { color: color-mix(in srgb, currentColor 42%, transparent); }",
        ".doctor-review-source { font-size: 12.5px; color: color-mix(in srgb, currentColor 55%, transparent); }",
        ".doctor-review-source.accent { color: var(--accent-color); }",
        // The review card is the only one that carries emphasis: an accent
        // hairline on top of the plain `.card` surface.
        ".doctor-card-accent { box-shadow: inset 0 0 0 1px alpha(@accent_color, 0.45); }",
        // The conflicts card is the quietest thing on the page: an outline, no
        // fill, no shadow.
        ".doctor-card-dashed { border: 1px dashed alpha(@borders, 0.9); border-radius: 12px; }",
        ".doctor-review-meta { padding: 12px 28px; background: color-mix(in srgb, var(--card-bg-color) 45%, var(--window-bg-color)); }",
        ".doctor-review-meta-heading { font-size: 18px; font-weight: 700; }",
        ".doctor-review-meta-hint { font-size: 13px; color: color-mix(in srgb, currentColor 45%, transparent); }",
        ".doctor-review-footer { padding: 14px 28px; background: color-mix(in srgb, var(--card-bg-color) 55%, var(--window-bg-color)); border-top: 1px solid color-mix(in srgb, currentColor 10%, transparent); }",
        ".doctor-review-footer-summary { font-size: 13.5px; color: color-mix(in srgb, currentColor 62%, transparent); }",
        ".doctor-review-apply { font-size: 14.5px; padding: 9px 18px; }",
    ]
    .join(" ")
}

#[cfg(test)]
mod tests {
    #[test]
    fn doctor_css_keeps_review_start_and_conflict_rules() {
        let css = crate::ui::library_doctor::css();

        for selector in [
            ".doctor-album-check",
            ".doctor-review-row",
            ".doctor-card-accent",
            ".doctor-review-footer",
            ".doctor-review-apply",
            ".doctor-start-run",
            ".doctor-conflicts-dashed",
        ] {
            assert!(css.contains(selector), "missing CSS selector {selector}");
        }
    }
}
