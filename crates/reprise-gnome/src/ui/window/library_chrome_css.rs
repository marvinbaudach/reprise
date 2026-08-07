pub(in crate::ui) fn css() -> String {
    ".reprise-library-split .reprise-library-sidebar { \
       background-color: @sidebar_bg_color; \
       border-right: 1px solid rgba(255, 255, 255, 0.06); }\n\
     .reprise-library-header { \
       background-color: @headerbar_bg_color; \
       border-bottom: 1px solid rgba(255, 255, 255, 0.06); }\n\
     .reprise-search-strip { \
       background-color: @headerbar_bg_color; \
       border-bottom: 1px solid rgba(255, 255, 255, 0.06); }\n\
     .reprise-library-sidebar .caption-heading { \
       color: @reprise_secondary_fg_color; }"
        .to_string()
}

#[cfg(test)]
mod tests {
    /// UX STYLE-1: every chrome surface that should read as its own plane
    /// declares a background and a bottom edge explicitly.
    #[test]
    fn style_1_chrome_surfaces_declare_background_and_edge() {
        let css = super::css();

        for class in [".reprise-library-header", ".reprise-search-strip"] {
            let block = css
                .split(class)
                .nth(1)
                .unwrap_or_else(|| panic!("{class} has no rule in the chrome CSS"));
            let block = block.split('}').next().unwrap_or_default();
            assert!(
                block.contains("background-color:"),
                "{class} inherits its background"
            );
            assert!(
                block.contains("border-bottom:"),
                "{class} has no bottom edge against the content"
            );
        }
    }
}
