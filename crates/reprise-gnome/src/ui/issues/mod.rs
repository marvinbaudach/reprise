//! Shared card, row, and lazy-collapse building blocks for issue views.

mod issue_card;
#[allow(dead_code)] // The standalone constructor is consumed by Task 3.3.
mod issue_collapse;
mod issue_row;
mod missing_view;

pub(in crate::ui) use issue_card::IssueCard;
pub(in crate::ui) use issue_collapse::CollapsedList;
pub(in crate::ui) use issue_row::{IssuePill, IssueRow, RowSpec};
pub(in crate::ui) use missing_view::{purge_startup_tombstones, MissingFilesView};

/// Structural styles shared by every issue-card consumer.
pub(in crate::ui) fn css() -> String {
    use super::style::tokens::RADIUS_SURFACE;

    format!(
        ".issue-card {{ background-color: alpha(white, 0.035); \
           border: 1px solid alpha(white, 0.05); border-radius: {RADIUS_SURFACE}; \
           overflow: hidden; }}\n\
         .issue-card-header {{ background-color: alpha(white, 0.03); padding: 10px 12px; }}\n\
         .issue-card-icon {{ font-size: 16px; }}\n\
         .issue-card-title {{ font-size: 13px; font-weight: 700; }}\n\
         .issue-card-meta {{ color: alpha(@window_fg_color, 0.50); }}\n\
         .issue-card-list {{ background: transparent; }}\n\
         .issue-card-list > row {{ min-height: 42px; padding: 0 12px; }}\n\
         .issue-card-list > row:selected {{ background-color: alpha(@accent_bg_color, 0.16); }}\n\
         .issue-row-cover {{ min-width: 30px; min-height: 30px; border-radius: 4px; }}\n\
         .issue-row-primary {{ font-weight: 700; }}\n\
         .issue-row-secondary, .issue-row-tertiary {{ \
           color: alpha(@window_fg_color, 0.58); }}\n\
         .issue-row-idle {{ font-size: 11.5px; color: alpha(@window_fg_color, 0.40); }}\n\
         .issue-row-pill {{ padding: 3px 10px; min-height: 24px; }}\n\
         .issue-collapse-footer {{ padding: 5px 12px 8px; background: transparent; }}
         .issue-remove-pill {{ color: #f38ba8; background-color: alpha(#f38ba8, 0.10); }}
         .missing-info-card {{ background-color: alpha(@accent_bg_color, 0.07); \
           border: 1px solid alpha(@accent_color, 0.18); border-radius: {RADIUS_SURFACE}; \
           padding: 12px; }}
         .missing-clear-state image {{ color: @accent_color; }}
         .import-hint-row {{ background-color: alpha(@accent_bg_color, 0.07); }}"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn issue_css_parses_without_errors() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&super::css());
        assert!(errors.is_empty(), "CSS parse errors: {errors:?}");
    }
}
