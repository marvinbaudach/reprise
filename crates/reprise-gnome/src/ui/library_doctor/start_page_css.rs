pub(super) fn css() -> String {
    [
        ".doctor-start-body { font-size: 14.5px; color: color-mix(in srgb, currentColor 68%, transparent); }",
        ".doctor-start-scope-label { font-size: 12px; color: color-mix(in srgb, currentColor 62%, transparent); }",
        ".doctor-start-remote { padding: 14px 16px; }",
        ".doctor-start-run { padding: 9px 18px; font-size: 14.5px; }",
        ".doctor-start-estimate { font-size: 12.5px; color: color-mix(in srgb, currentColor 45%, transparent); }",
        ".doctor-start-last-scan { margin-top: 14px; padding-top: 20px; }",
        ".doctor-start-last-title { font-size: 13.5px; color: color-mix(in srgb, currentColor 75%, transparent); }",
        ".doctor-start-last-detail { font-size: 12.5px; color: color-mix(in srgb, currentColor 45%, transparent); }",
    ]
    .join(" ")
}
