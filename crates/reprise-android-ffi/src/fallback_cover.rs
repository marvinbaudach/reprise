#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AndroidFallbackCoverColours {
    pub top: u32,
    pub bottom: u32,
}

#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI free functions own Kotlin strings.
pub fn android_fallback_cover_colours(
    title: String,
    artist: String,
) -> AndroidFallbackCoverColours {
    let colours = reprise_core::visuals::fallback_cover::fallback_cover_colours(&title, &artist);
    AndroidFallbackCoverColours {
        top: colours.top,
        bottom: colours.bottom,
    }
}

#[cfg(test)]
mod tests {
    use super::android_fallback_cover_colours;

    #[test]
    fn pure_core_colours_cross_the_android_boundary_unchanged() {
        let expected =
            reprise_core::visuals::fallback_cover::fallback_cover_colours("A Track", "An Artist");

        let actual = android_fallback_cover_colours("A Track".into(), "An Artist".into());

        assert_eq!(actual.top, expected.top);
        assert_eq!(actual.bottom, expected.bottom);
    }
}
