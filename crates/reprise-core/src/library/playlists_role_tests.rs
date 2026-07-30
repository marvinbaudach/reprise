use super::*;

#[test]
fn ensure_role_playlist_creates_once_and_finds_by_role() {
    let conn = seeded_conn();
    let first = ensure_role_playlist(&conn, "Conversion", "conversion").unwrap();
    let again = ensure_role_playlist(&conn, "Conversion", "conversion").unwrap();
    assert_eq!(first, again, "role playlists are singletons");
    assert_eq!(
        find_role_playlist(&conn, "conversion").unwrap(),
        Some(first)
    );
    assert_eq!(find_role_playlist(&conn, "other").unwrap(), None);
}

#[test]
fn playlist_role_is_none_for_a_user_playlist_and_a_missing_id() {
    let conn = seeded_conn();
    let user = create(&conn, "My Mix").unwrap();
    assert_eq!(playlist_role(&conn, user).unwrap(), None);
    assert_eq!(playlist_role(&conn, 999_999).unwrap(), None);
    let role = ensure_role_playlist(&conn, "Conversion", "conversion").unwrap();
    assert_eq!(
        playlist_role(&conn, role).unwrap().as_deref(),
        Some("conversion")
    );
}
