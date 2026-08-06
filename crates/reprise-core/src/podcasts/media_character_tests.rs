use super::{character_from_category, MediaCharacter};

#[test]
fn youtube_categories_map_only_unambiguous_music_and_speech() {
    let cases = [
        ("Music", MediaCharacter::Music),
        ("News & Politics", MediaCharacter::Speech),
        ("Education", MediaCharacter::Speech),
        ("Science & Technology", MediaCharacter::Speech),
        ("People & Blogs", MediaCharacter::Speech),
        ("Comedy", MediaCharacter::Speech),
        ("Gaming", MediaCharacter::Speech),
        ("Sports", MediaCharacter::Speech),
        ("Howto & Style", MediaCharacter::Speech),
        ("Travel & Events", MediaCharacter::Speech),
        ("Autos & Vehicles", MediaCharacter::Speech),
        ("Pets & Animals", MediaCharacter::Speech),
        ("Nonprofits & Activism", MediaCharacter::Speech),
        ("Entertainment", MediaCharacter::Unknown),
        ("Film & Animation", MediaCharacter::Unknown),
        ("Documentary", MediaCharacter::Unknown),
        ("", MediaCharacter::Unknown),
    ];

    for (category, expected) in cases {
        assert_eq!(
            character_from_category(Some(category)),
            expected,
            "unexpected media character for {category:?}"
        );
    }
    assert_eq!(character_from_category(None), MediaCharacter::Unknown);
}
