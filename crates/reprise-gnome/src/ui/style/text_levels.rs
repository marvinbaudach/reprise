//! Shared foreground levels for app-authored text hierarchy.

pub(super) fn css() -> String {
    ".reprise-text-primary { color: @reprise_primary_fg_color; }\n\
     .reprise-text-secondary { color: @reprise_secondary_fg_color; }\n\
     .reprise-text-hint { color: @reprise_hint_fg_color; }"
        .into()
}
