//! Declaring a fieldless enum together with the complete list of its variants.
//!
//! Several decisions here iterate over "every variant" — every playback mode
//! the player links must answer for, every filter facet a jump may have to
//! relax. Rust has no way to ask an enum for its variants, so those lists were
//! hand-written arrays sitting next to exhaustive `match`es. A new variant
//! forces the `match` to grow and leaves the array quietly short: the mode is
//! never checked, the facet is never relaxed, and nothing fails.
//!
//! No test can close that gap — a test can only look at the array, which is
//! the very thing that is incomplete. (Verified: dropping a variant from
//! `PlaybackMode::ALL` left every `PLAY-12` test green.) Deriving the list is
//! the only fix, and this macro is the derive, without taking on a dependency
//! for it: the variants are written once, and the array is generated from that
//! same list, so the two cannot drift.

/// Declares the enum and its variant list from one set of variants.
///
/// ```ignore
/// enumerated! {
///     #[derive(Clone, Copy, PartialEq, Eq)]
///     pub(super) enum Facet { Genre, Country }
///     /// Every facet, in declaration order.
///     pub(super) const FACETS;
/// }
/// ```
///
/// The list can be a free constant, as above, or an associated one — write
/// `pub(super) const Self::ALL;` for `Facet::ALL`.
macro_rules! enumerated {
    (
        $(#[$enum_attr:meta])*
        $enum_vis:vis enum $name:ident { $($(#[$variant_attr:meta])* $variant:ident),+ $(,)? }
        $(#[$all_attr:meta])*
        $all_vis:vis const $all:ident;
    ) => {
        $(#[$enum_attr])*
        $enum_vis enum $name { $($(#[$variant_attr])* $variant),+ }

        $(#[$all_attr])*
        $all_vis const $all: [$name; [$(stringify!($variant)),+].len()] =
            [$($name::$variant),+];
    };
    (
        $(#[$enum_attr:meta])*
        $enum_vis:vis enum $name:ident { $($(#[$variant_attr:meta])* $variant:ident),+ $(,)? }
        $(#[$all_attr:meta])*
        $all_vis:vis const Self::$all:ident;
    ) => {
        $(#[$enum_attr])*
        $enum_vis enum $name { $($(#[$variant_attr])* $variant),+ }

        impl $name {
            $(#[$all_attr])*
            $all_vis const $all: [$name; [$(stringify!($variant)),+].len()] =
                [$($name::$variant),+];
        }
    };
}

pub(in crate::ui) use enumerated;

#[cfg(test)]
mod tests {
    enumerated! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        enum Sample {
            #[default]
            First,
            Second,
            Third,
        }
        const SAMPLES;
    }

    enumerated! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Associated {
            Only,
        }
        const Self::ALL;
    }

    /// The list is the declaration, so it holds every variant in order and
    /// keeps whatever attributes the variants carry.
    #[test]
    fn the_variant_list_follows_the_declaration() {
        assert_eq!(SAMPLES, [Sample::First, Sample::Second, Sample::Third]);
        assert_eq!(Sample::default(), Sample::First);
        assert_eq!(Associated::ALL, [Associated::Only]);
    }
}
