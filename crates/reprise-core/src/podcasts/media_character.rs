/// What a source says its media carries.
///
/// `Unknown` is not a guess: nobody has supplied an unambiguous category yet,
/// so the caller retains its own source default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCharacter {
    Music,
    Speech,
    Unknown,
}

pub fn character_from_category(category: Option<&str>) -> MediaCharacter {
    match category {
        Some("Music") => MediaCharacter::Music,
        Some(
            "News & Politics"
            | "Education"
            | "Science & Technology"
            | "People & Blogs"
            | "Comedy"
            | "Gaming"
            | "Sports"
            | "Howto & Style"
            | "Travel & Events"
            | "Autos & Vehicles"
            | "Pets & Animals"
            | "Nonprofits & Activism",
        ) => MediaCharacter::Speech,
        Some(_) | None => MediaCharacter::Unknown,
    }
}
