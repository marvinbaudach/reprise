//! Deterministic dark cover colours for tracks without artwork.

use super::color::{hsla_to_rgb, hue_shift};

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;
const IDENTITY_SEPARATOR: u8 = 0x1f;
const SATURATION: f32 = 0.42;
const TOP_LIGHTNESS: f32 = 0.34;
const BOTTOM_LIGHTNESS: f32 = 0.18;
const BOTTOM_HUE_SHIFT: f32 = 34.0;
const NEUTRAL_TOP: u32 = 0x4a4a4a;
const NEUTRAL_BOTTOM: u32 = 0x292929;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FallbackCoverColours {
    /// The upper gradient colour as `0xRRGGBB`.
    pub top: u32,
    /// The lower gradient colour as `0xRRGGBB`.
    pub bottom: u32,
}

/// Derives a stable dark gradient from the normalized artist and title.
pub fn fallback_cover_colours(title: &str, artist: &str) -> FallbackCoverColours {
    let normalized_artist = artist.trim().to_lowercase();
    let normalized_title = title.trim().to_lowercase();
    if normalized_artist.is_empty() && normalized_title.is_empty() {
        return FallbackCoverColours {
            top: NEUTRAL_TOP,
            bottom: NEUTRAL_BOTTOM,
        };
    }

    let mut hash = FNV_OFFSET_BASIS;
    hash_bytes(&mut hash, normalized_artist.as_bytes());
    hash_byte(&mut hash, IDENTITY_SEPARATOR);
    hash_bytes(&mut hash, normalized_title.as_bytes());
    let hue = (hash % 36_000) as f32 / 100.0;

    FallbackCoverColours {
        top: pack(hsla_to_rgb(hue, SATURATION, TOP_LIGHTNESS)),
        bottom: pack(hue_shift(
            hsla_to_rgb(hue, SATURATION, BOTTOM_LIGHTNESS),
            BOTTOM_HUE_SHIFT,
        )),
    }
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        hash_byte(hash, byte);
    }
}

fn hash_byte(hash: &mut u64, byte: u8) {
    *hash ^= u64::from(byte);
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn pack((red, green, blue): (f32, f32, f32)) -> u32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    channel(red) << 16 | channel(green) << 8 | channel(blue)
}

#[cfg(test)]
mod tests {
    use super::fallback_cover_colours;
    use crate::visuals::color::rgb_hue;
    use std::collections::BTreeSet;

    #[test]
    fn the_same_normalized_identity_always_yields_the_same_pair() {
        let expected = fallback_cover_colours("  The Song  ", "  The Artist  ");

        assert_eq!(expected, fallback_cover_colours("the song", "the artist"));
        assert_eq!(expected, fallback_cover_colours("THE SONG", "THE ARTIST"));
        assert_eq!(
            expected,
            fallback_cover_colours("  The Song  ", "  The Artist  ")
        );
    }

    #[test]
    fn two_hundred_identities_reach_at_least_eight_hue_sectors() {
        let sectors = (0..200)
            .map(|index| fallback_cover_colours(&format!("Title {index}"), "Artist"))
            .map(|colours| hue_sector(colours.top))
            .collect::<BTreeSet<_>>();

        assert!(
            sectors.len() >= 8,
            "fallback identities only reached {} hue sectors: {sectors:?}",
            sectors.len(),
        );
    }

    #[test]
    fn an_empty_identity_has_a_defined_neutral_pair() {
        let colours = fallback_cover_colours("", "");

        assert!(is_grey(colours.top));
        assert!(is_grey(colours.bottom));
        assert_ne!(colours.top, colours.bottom);
    }

    #[test]
    fn every_generated_colour_keeps_white_above_three_to_one() {
        for index in 0..200 {
            let colours = fallback_cover_colours(
                &format!("A title with identity {index}"),
                &format!("Artist {}", index % 29),
            );
            for colour in [colours.top, colours.bottom] {
                let ratio = contrast_with_white(colour);
                assert!(
                    ratio >= 3.0,
                    "#{colour:06x} only has {ratio:.3}:1 contrast with white",
                );
            }
        }
    }

    fn hue_sector(colour: u32) -> u8 {
        let rgb = unpack(colour);
        (rgb_hue(rgb) / 30.0).floor() as u8 % 12
    }

    fn is_grey(colour: u32) -> bool {
        let red = (colour >> 16) & 0xff;
        let green = (colour >> 8) & 0xff;
        let blue = colour & 0xff;
        red == green && green == blue
    }

    fn contrast_with_white(colour: u32) -> f32 {
        1.05 / (relative_luminance(colour) + 0.05)
    }

    fn relative_luminance(colour: u32) -> f32 {
        let (red, green, blue) = unpack(colour);
        0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
    }

    fn linear(channel: f32) -> f32 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    fn unpack(colour: u32) -> (f32, f32, f32) {
        (
            ((colour >> 16) & 0xff) as f32 / 255.0,
            ((colour >> 8) & 0xff) as f32 / 255.0,
            (colour & 0xff) as f32 / 255.0,
        )
    }
}
