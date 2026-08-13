use gtk4::prelude::*;

#[derive(Clone, Copy)]
pub(super) enum Fallback<'a> {
    Icon(&'a str),
    Initials(&'a str),
}

pub(super) fn widget(fallback: Fallback<'_>, width: i32, height: i32) -> gtk4::Widget {
    match fallback {
        Fallback::Icon(icon_name) => {
            let image = gtk4::Image::from_icon_name(icon_name);
            image.set_pixel_size(width.min(height));
            image.set_halign(gtk4::Align::Center);
            image.set_valign(gtk4::Align::Center);
            image.set_hexpand(false);
            image.set_vexpand(false);
            image.upcast()
        }
        Fallback::Initials(label) => {
            let initials = gtk4::Label::new(Some(&initials_text(label)));
            initials.set_size_request(width, height);
            initials.set_halign(gtk4::Align::Fill);
            initials.set_valign(gtk4::Align::Fill);
            initials.set_hexpand(true);
            initials.set_vexpand(true);
            initials.add_css_class("reprise-radio-initials-tile");
            initials.upcast()
        }
    }
}

impl super::SourceImage {
    /// Source-table artwork that appears automatically during window startup.
    /// Explicit previews and playback artwork keep using [`Self::new`].
    pub(crate) fn new_after_startup(
        image_url: Option<&str>,
        fallback_icon: &str,
        size: i32,
        images_allowed: bool,
    ) -> Self {
        Self::new_after_startup_with_fallback(
            image_url,
            Fallback::Icon(fallback_icon),
            size,
            images_allowed,
            reprise_core::remote_image::CacheScope::Persistent,
        )
    }

    /// Source-table artwork with a station-specific initials tile when no
    /// remote image is available.
    pub(crate) fn new_after_startup_with_initials(
        image_url: Option<&str>,
        label: &str,
        size: i32,
        images_allowed: bool,
    ) -> Self {
        Self::new_after_startup_with_fallback(
            image_url,
            Fallback::Initials(label),
            size,
            images_allowed,
            reprise_core::remote_image::CacheScope::Persistent,
        )
    }

    fn new_after_startup_with_fallback(
        image_url: Option<&str>,
        fallback: Fallback<'_>,
        size: i32,
        images_allowed: bool,
        cache_scope: reprise_core::remote_image::CacheScope,
    ) -> Self {
        let image = Self::build(fallback, size, size);
        image.set_urls(
            super::ArtworkRequest::new(
                image_url,
                None,
                (size, size),
                images_allowed,
                cache_scope,
                super::StartupTiming::AfterQuiet,
            ),
            |_| {},
        );
        image
    }
}

fn initials_text(label: &str) -> String {
    crate::ui::library_views::artist_avatar::initials(label)
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    fn rad_7_radio_initials_reuse_the_artist_avatar_rules() {
        assert_eq!(super::initials_text("Radio Bob"), "RB");
        assert_eq!(super::initials_text("  "), "?");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn rad_7_radio_initials_fallback_is_the_visible_source_image_page() {
        gtk4::init().unwrap();
        let image = super::super::SourceImage::new_after_startup_with_initials(
            None,
            "Radio Bob",
            36,
            false,
        );

        let label = image
            .widget()
            .visible_child()
            .and_downcast::<gtk4::Label>()
            .expect("initials fallback label");
        assert_eq!(label.label(), "RB");
        assert!(label.has_css_class("reprise-radio-initials-tile"));
    }
}
