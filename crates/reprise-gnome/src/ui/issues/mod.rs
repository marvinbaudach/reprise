//! Shared card, row, and lazy-collapse building blocks for issue views.

#[allow(dead_code)] // Consumed by the issue views beginning in Task 3.2.
mod issue_card;
#[allow(dead_code)] // Consumed by the issue views beginning in Task 3.2.
mod issue_collapse;
#[allow(dead_code)] // Consumed by the issue views beginning in Task 3.2.
mod issue_row;

#[allow(unused_imports)]
pub(in crate::ui) use issue_card::IssueCard;
#[allow(unused_imports)]
pub(in crate::ui) use issue_collapse::CollapsedList;
#[allow(unused_imports)]
pub(in crate::ui) use issue_row::{IssuePill, IssueRow, RowSpec};

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
         .issue-collapse-footer {{ padding: 5px 12px 8px; background: transparent; }}"
    )
}
