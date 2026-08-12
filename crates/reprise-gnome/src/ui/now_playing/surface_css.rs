//! CSS for the Now Playing panel shell.

use crate::ui::style::tokens::{
    NOW_PLAYING_FOOTER_SIZE, NOW_PLAYING_GLOW_ALPHA, NOW_PLAYING_PILL_ACTIVE_ALPHA,
    NOW_PLAYING_PILL_BG_ALPHA, NOW_PLAYING_PILL_RADIUS, NOW_PLAYING_SUBTITLE_SIZE,
    NOW_PLAYING_TITLE_SIZE, RADIUS_SURFACE,
};

pub(super) fn css() -> String {
    format!(
        ".reprise-now-playing-stage {{ \
       background-color: @sidebar_bg_color; color: @sidebar_fg_color; min-width: 300px; \
       border-left: 1px solid rgba(255, 255, 255, 0.06); }}\n\
     .reprise-now-playing-glow {{ \
       min-height: 300px; \
       background-image: radial-gradient(ellipse at center, \
         alpha(@reprise_player_accent, {NOW_PLAYING_GLOW_ALPHA}) 0%, \
         alpha(@sidebar_bg_color, 0) 70%); }}\n\
     .reprise-now-playing-idle .reprise-now-playing-glow {{ \
       background-image: none; }}\n\
     .reprise-now-playing-head {{ padding: 22px 18px 16px; }}\n\
     .reprise-now-playing-cover {{ \
       border-radius: {RADIUS_SURFACE}; \
       box-shadow: inset 0 0 0 1px alpha(@sidebar_fg_color, 0.12); }}\n\
     .reprise-now-playing-title {{ \
       color: @reprise_primary_fg_color; font-size: {NOW_PLAYING_TITLE_SIZE}; font-weight: 700; }}\n\
     .reprise-now-playing-subtitle {{ \
       color: @reprise_secondary_fg_color; \
       font-size: {NOW_PLAYING_SUBTITLE_SIZE}; }}\n\
     .reprise-now-playing-tabs {{ \
       background-color: alpha(@sidebar_fg_color, {NOW_PLAYING_PILL_BG_ALPHA}); \
       border-radius: {NOW_PLAYING_PILL_RADIUS}; \
       padding: 2px; margin: 0 18px 12px; }}\n\
     .reprise-now-playing-tabs button {{ \
       background-color: transparent; background-image: none; \
       border: none; border-radius: {NOW_PLAYING_PILL_RADIUS}; box-shadow: none; \
       color: @reprise_secondary_fg_color; min-height: 0; \
       padding: 5px 18px; }}\n\
     .reprise-now-playing-tabs button:checked {{ \
       background-color: alpha(@sidebar_fg_color, {NOW_PLAYING_PILL_ACTIVE_ALPHA}); \
       color: @reprise_primary_fg_color; font-weight: 700; }}\n\
     .reprise-now-playing-footer {{ \
       color: @reprise_secondary_fg_color; \
       font-size: {NOW_PLAYING_FOOTER_SIZE}; \
       min-height: 14px; margin: 8px 12px 12px; }}"
    )
}
