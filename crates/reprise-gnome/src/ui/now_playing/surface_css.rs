//! CSS for the Now Playing panel shell.

use crate::ui::style::tokens::{
    NOW_PLAYING_FOOTER_SIZE, NOW_PLAYING_HEAD_TOP, NOW_PLAYING_LIST_RULE_ABOVE,
    NOW_PLAYING_LIST_RULE_BELOW, NOW_PLAYING_LIST_RULE_RUN_OUT, NOW_PLAYING_PILL_BG_ALPHA,
    NOW_PLAYING_SEGMENT_INNER_RADIUS, NOW_PLAYING_SEGMENT_RADIUS, NOW_PLAYING_SUBTITLE_SIZE,
    NOW_PLAYING_TITLE_SIZE, RADIUS_SURFACE,
};

pub(super) fn css() -> String {
    format!(
        ".reprise-now-playing-stage {{ \
       background-color: @sidebar_bg_color; color: @sidebar_fg_color; min-width: 300px; \
       border-left: 1px solid @reprise_hairline; }}\n\
     .reprise-now-playing-glow {{ \
       min-height: 300px; \
       background-image: radial-gradient(ellipse at center, \
         @reprise_now_playing_glow 0%, \
         alpha(@sidebar_bg_color, 0) 70%); }}\n\
     .reprise-now-playing-idle .reprise-now-playing-glow {{ \
       background-image: none; }}\n\
     .reprise-now-playing-head {{ padding: {NOW_PLAYING_HEAD_TOP}px 18px 0; }}\n\
     .reprise-now-playing-metadata {{ padding: 0 18px 16px; }}\n\
     .reprise-now-playing-cover {{ \
       border-radius: {RADIUS_SURFACE}; \
       box-shadow: 0 12px 30px @reprise_cover_shadow; }}\n\
     .reprise-now-playing-title {{ \
       color: @reprise_primary_fg_color; font-size: {NOW_PLAYING_TITLE_SIZE}; font-weight: 700; }}\n\
     .reprise-now-playing-artist {{ \
       color: @reprise_secondary_fg_color; \
       font-size: {NOW_PLAYING_SUBTITLE_SIZE}; font-weight: 500; }}\n\
     .reprise-now-playing-album {{ \
       color: @reprise_tertiary_fg_color; \
       font-size: {NOW_PLAYING_SUBTITLE_SIZE}; font-weight: 400; }}\n\
     .reprise-now-playing-tabs {{ \
       background-color: alpha(@sidebar_fg_color, {NOW_PLAYING_PILL_BG_ALPHA}); \
       border-radius: {NOW_PLAYING_SEGMENT_RADIUS}; \
       padding: 2px; margin: 0 18px 0; }}\n\
     .reprise-now-playing-tabs toggle-group {{ \
       padding: 0; border: none; background: none; box-shadow: none; \
       min-height: 0; border-radius: 0; }}\n\
     /* Libadwaita gives separators horizontal margins; clearing them keeps the measured gap at 2px. */\n\
     .reprise-now-playing-tabs separator {{ \
       min-width: 2px; margin: 0; background: none; opacity: 0; }}\n\
     .reprise-now-playing-tabs toggle {{ \
       background: transparent; border: none; box-shadow: none; \
       border-radius: {NOW_PLAYING_SEGMENT_INNER_RADIUS}; min-height: 0; \
       padding: 0; color: @reprise_secondary_fg_color; }}\n\
     .reprise-now-playing-tabs toggle:checked {{ \
       background-color: @reprise_tab_active_bg; \
       box-shadow: 0 1px 2px @reprise_tab_active_shadow; \
       color: @reprise_primary_fg_color; }}\n\
     .reprise-now-playing-list-rule {{ \
       min-height: 1px; margin: {NOW_PLAYING_LIST_RULE_ABOVE}px 0 {NOW_PLAYING_LIST_RULE_BELOW}px; \
       background-image: linear-gradient(to right, alpha(@borders, 0), \
         @borders {NOW_PLAYING_LIST_RULE_RUN_OUT}px, \
         @borders calc(100% - {NOW_PLAYING_LIST_RULE_RUN_OUT}px), alpha(@borders, 0)); }}\n\
     .reprise-now-playing-footer {{ \
       color: @reprise_secondary_fg_color; \
       font-size: {NOW_PLAYING_FOOTER_SIZE}; \
       min-height: 14px; margin: 8px 12px 12px; }}"
    )
}
