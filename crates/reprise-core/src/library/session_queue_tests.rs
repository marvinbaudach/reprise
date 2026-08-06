use super::*;

fn conn() -> Db {
    Db::open_in_memory().unwrap()
}

#[test]
fn que_12_restore_drops_episodes_from_the_manual_queue() {
    let conn = conn();
    let mut value = serde_json::to_value(SessionState::default()).unwrap();
    value["up_next"] = serde_json::json!([
        { "kind": "track", "id": 30 },
        { "kind": "episode", "id": 118 },
        { "kind": "track", "id": 40 },
    ]);
    crate::library::settings::set_setting(&conn, SESSION_KEY, &value.to_string()).unwrap();

    assert_eq!(
        load(&conn).up_next.ids(),
        &[QueueItem::Track(30), QueueItem::Track(40)]
    );
}

#[test]
fn que_12_restore_clears_a_current_manual_episode() {
    let conn = conn();
    let mut value = serde_json::to_value(SessionState::default()).unwrap();
    value["current_up_next"] = serde_json::json!({ "kind": "episode", "id": 118 });
    crate::library::settings::set_setting(&conn, SESSION_KEY, &value.to_string()).unwrap();

    assert_eq!(load(&conn).current_up_next, None);
}
