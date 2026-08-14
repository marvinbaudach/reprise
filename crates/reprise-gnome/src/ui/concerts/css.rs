//! Structural styling owned by the Concerts full view.

pub(in crate::ui) fn css() -> String {
    ".reprise-concerts-view { min-height: 1px; }\n\
     .reprise-concerts-radius-off {\n  \
       opacity: 0.62;\n  \
       border: 1px dashed alpha(currentColor, 0.52);\n\
     }\n\
     .reprise-concerts-location-banner {\n  \
       background-color: alpha(@accent_bg_color, 0.12);\n\
     }"
    .into()
}
