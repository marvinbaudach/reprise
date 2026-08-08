pub(crate) const VARIOUS_ARTISTS_MBID: &str = "89ad4ac3-39f7-470e-963a-56509c546377";

pub(crate) const PLACEHOLDER_ARTIST_NAMES: [&str; 3] = ["Various Artists", "Various", "VA"];

pub(crate) const SPECIFICITY_CONFIDENCE_CAP: u8 = 49;

pub(crate) fn is_placeholder_artist(value: &str, artist_mbid: Option<&str>) -> bool {
    artist_mbid == Some(VARIOUS_ARTISTS_MBID)
        || PLACEHOLDER_ARTIST_NAMES
            .iter()
            .any(|placeholder| value.trim().to_lowercase() == placeholder.to_lowercase())
}

pub(crate) fn reduces_specificity(
    current: &super::super::DoctorValue,
    proposed: &super::super::DoctorValue,
    field: super::super::DoctorField,
) -> bool {
    use super::super::{DoctorField, DoctorValue};

    match (field, current, proposed) {
        (DoctorField::Artist | DoctorField::AlbumArtist, _, DoctorValue::Text(proposed)) => {
            is_placeholder_artist(proposed, None)
        }
        (DoctorField::Title, DoctorValue::Text(current), DoctorValue::Text(proposed)) => {
            let current = crate::library::group_key::normalize_group_key(current);
            let proposed = crate::library::group_key::normalize_group_key(proposed);
            !proposed.is_empty() && proposed.len() < current.len() && current.starts_with(&proposed)
        }
        (DoctorField::Year, DoctorValue::Year(current), DoctorValue::Year(proposed)) => {
            proposed < current
        }
        _ => false,
    }
}
