//! Podcasts source surface.

mod css;

pub(in crate::ui) fn css() -> String {
    css::css()
}
