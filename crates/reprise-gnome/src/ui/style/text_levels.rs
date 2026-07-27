//! Shared foreground levels for app-authored text hierarchy.

pub(super) fn css() -> String {
    ".reprise-text-primary { color: @reprise_primary_fg_color; }\n\
     .reprise-text-secondary { color: @reprise_secondary_fg_color; }\n\
     .reprise-text-hint { color: @reprise_hint_fg_color; }\n\
     .reprise-rhythmbox-import-option .subtitle {\n\
         color: @reprise_secondary_fg_color;\n\
     }"
    .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn rhythmbox_option_subtitles_use_the_verified_secondary_level() {
        let css = super::css();

        assert!(css.contains(".reprise-rhythmbox-import-option .subtitle"));
        assert!(css.contains("color: @reprise_secondary_fg_color"));
    }
}
