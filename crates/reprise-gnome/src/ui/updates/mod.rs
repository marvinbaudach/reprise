pub(in crate::ui) mod badge;
mod concerts_section;
pub(in crate::ui) mod css;
mod feed_row;
mod feed_snapshot;
mod footer_state;
pub(in crate::ui) mod popover;
pub(in crate::ui) mod release_cover;
pub(in crate::ui) mod release_row;
mod release_row_actions;
mod shell;

/// The New Releases feature's CSS section (D1), composed here so
/// `style::app_css` can call `updates::css()` the same way it calls
/// every other feature's aggregator (see e.g. `now_playing::css`).
pub(in crate::ui) fn css() -> String {
    css::css()
}
