pub(crate) const VARIOUS_ARTISTS_MBID: &str = "89ad4ac3-39f7-470e-963a-56509c546377";

pub(crate) const PLACEHOLDER_ARTIST_NAMES: [&str; 3] = ["Various Artists", "Various", "VA"];

pub(crate) const SPECIFICITY_CONFIDENCE_CAP: u8 = 49;

pub(crate) fn is_placeholder_artist(value: &str, artist_mbid: Option<&str>) -> bool {
    artist_mbid == Some(VARIOUS_ARTISTS_MBID)
        || PLACEHOLDER_ARTIST_NAMES
            .iter()
            .any(|placeholder| value.trim().to_lowercase() == placeholder.to_lowercase())
}

/// Where a proposed year comes from.
///
/// Only the release group's year is a specificity loss against a track tag: it
/// dates the work, not the release the file was tagged from. An earlier year
/// that a matched release itself carries is a correction — punishing it would
/// cap the ordinary case of a file tagged from a reissue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum YearProvenance {
    /// A matched release carries this year.
    Release,
    /// Only the release group's original release year yields this value.
    ReleaseGroupFallback,
    /// Not established at this call site, so the year guard stays out of the
    /// way. Callers without remote evidence — the review's spelling ties —
    /// cannot tell the two apart until release matching lands.
    Unknown,
}

pub(crate) fn reduces_specificity(
    current: &super::super::DoctorValue,
    proposed: &super::super::DoctorValue,
    field: super::super::DoctorField,
    year_provenance: YearProvenance,
) -> bool {
    use super::super::{DoctorField, DoctorValue};

    match (field, current, proposed) {
        (DoctorField::Artist | DoctorField::AlbumArtist, _, DoctorValue::Text(proposed)) => {
            is_placeholder_artist(proposed, None)
        }
        // A shortened name loses information whether it names the recording or
        // the release: "… (Deluxe Edition)" cut down to "…" is the same loss.
        (
            DoctorField::Title | DoctorField::Album,
            DoctorValue::Text(current),
            DoctorValue::Text(proposed),
        ) => {
            let current = crate::library::group_key::normalize_group_key(current);
            let proposed = crate::library::group_key::normalize_group_key(proposed);
            !proposed.is_empty() && proposed.len() < current.len() && current.starts_with(&proposed)
        }
        (DoctorField::Year, DoctorValue::Year(current), DoctorValue::Year(proposed)) => {
            year_provenance == YearProvenance::ReleaseGroupFallback && proposed < current
        }
        _ => false,
    }
}
