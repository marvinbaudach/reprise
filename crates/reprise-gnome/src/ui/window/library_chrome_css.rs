pub(in crate::ui) fn css() -> String {
    use crate::ui::style::tokens::{RADIUS_SURFACE, SURFACE_SHADOW};

    format!(
        ".reprise-library-split .reprise-library-sidebar {{ \
       background-color: @sidebar_bg_color; \
       border-right: 1px solid rgba(255, 255, 255, 0.06); }}\n\
     .reprise-library-header {{ \
       background-color: @headerbar_bg_color; \
       border-bottom: 1px solid rgba(255, 255, 255, 0.06); }}\n\
     .reprise-search-popover > contents {{ \
       background-color: @headerbar_bg_color; \
       border: 1px solid alpha(@window_fg_color, 0.16); \
       border-radius: {RADIUS_SURFACE}; \
       box-shadow: {SURFACE_SHADOW}; \
       padding: 9px; }}\n\
     .reprise-search-popover > contents entry {{ \
       min-width: 318px; \
       border-color: @accent_color; }}\n\
     .reprise-search-popover-caption {{ \
       color: @reprise_secondary_fg_color; \
       font-size: 11px; }}\n\
     .reprise-search-popover-caption-row {{ margin: 0 2px; }}\n\
     .reprise-library-sidebar .caption-heading {{ \
       color: @reprise_secondary_fg_color; }}"
    )
}
