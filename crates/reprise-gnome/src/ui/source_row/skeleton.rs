//! The geometry every source row shares.

use gtk4::prelude::*;

pub(in crate::ui) const ROW_MIN_HEIGHT: i32 = 56;
pub(in crate::ui) const MEDIA_WIDTH: i32 = 64;
pub(in crate::ui) const MEDIA_HEIGHT: i32 = 40;
const ROW_LEADING_MARGIN: i32 = 12;
/// Nested episode rows begin one complete media column after their group.
pub(in crate::ui) const EPISODE_INDENT: i32 = MEDIA_WIDTH;
pub(in crate::ui) const SIZE_SLOT_WIDTH: i32 = 110;
pub(in crate::ui) const ROW_CSS_CLASS: &str = "reprise-source-row";

/// The three places a caller may put widgets. Everything else about the row —
/// margins, spacing, height, and the media column's width — belongs to this
/// module, which is the whole reason the views stop drifting apart.
pub(in crate::ui) struct Skeleton {
    pub root: gtk4::Box,
    pub media: gtk4::Box,
    pub identity: gtk4::Box,
    pub trailing: gtk4::Box,
}

pub(in crate::ui) fn skeleton() -> Skeleton {
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    root.add_css_class(ROW_CSS_CLASS);
    root.set_margin_start(ROW_LEADING_MARGIN);
    root.set_margin_end(8);
    root.set_margin_top(6);
    root.set_margin_bottom(6);
    root.set_valign(gtk4::Align::Center);

    // A fixed-width host, not a fixed-width image: the podcast artwork is
    // 36×36 and the YouTube thumbnail 64×36, and they must still leave the
    // title at the same x position.
    let media = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    media.set_size_request(MEDIA_WIDTH, MEDIA_HEIGHT);
    media.set_halign(gtk4::Align::Center);
    media.set_valign(gtk4::Align::Center);
    root.append(&media);

    let identity = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
    identity.set_hexpand(true);
    identity.set_valign(gtk4::Align::Center);
    root.append(&identity);

    let trailing = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    trailing.set_valign(gtk4::Align::Center);
    root.append(&trailing);

    Skeleton {
        root,
        media,
        identity,
        trailing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SRC-16`: the geometry every source row shares is a constant, not a
    /// number each view picks for itself. A view that wants a different media
    /// width has to change it here, where the other views see it too.
    #[test]
    fn src_16_the_shared_row_geometry_is_one_set_of_constants() {
        let geometry = [
            MEDIA_WIDTH,
            MEDIA_HEIGHT,
            ROW_MIN_HEIGHT,
            SIZE_SLOT_WIDTH,
            EPISODE_INDENT,
        ];

        assert_eq!(geometry, [64, 40, 56, 110, geometry[0]]);
        assert!(geometry[4] > ROW_LEADING_MARGIN);
    }
}
