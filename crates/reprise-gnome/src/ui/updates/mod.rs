pub(in crate::ui) mod badge;
pub(in crate::ui) mod css;
pub(in crate::ui) mod history_page;
pub(in crate::ui) mod popover;
pub(in crate::ui) mod release_cover;
pub(in crate::ui) mod release_row;

/// The New Releases feature's CSS section (D1), composed here so
/// `style::app_css` can call `updates::css()` the same way it calls
/// every other feature's aggregator (see e.g. `now_playing::css`).
pub(in crate::ui) fn css() -> String {
    css::css()
}
