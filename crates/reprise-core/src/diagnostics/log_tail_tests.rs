use super::{read_log_tail, tail_of, TAIL_MAX_BYTES, TAIL_MAX_LINES};

#[test]
fn a_short_log_is_kept_whole() {
    assert_eq!(tail_of("one\ntwo\n", false).as_deref(), Some("one\ntwo"));
}

#[test]
fn only_the_last_lines_survive() {
    let text: String = (0..TAIL_MAX_LINES + 5)
        .map(|n| format!("line {n}\n"))
        .collect();

    let tail = tail_of(&text, false).unwrap();

    assert_eq!(tail.lines().count(), TAIL_MAX_LINES);
    assert!(tail.starts_with("line 5\n"));
    assert!(tail.ends_with(&format!("line {}", TAIL_MAX_LINES + 4)));
}

#[test]
fn a_cut_start_drops_the_fragment_of_the_first_line() {
    assert_eq!(tail_of("rtial\nwhole\n", true).as_deref(), Some("whole"));
}

#[test]
fn blank_text_has_no_tail() {
    assert_eq!(tail_of("", false), None);
    assert_eq!(tail_of("\n \n", false), None);
}

#[test]
fn a_missing_file_has_no_tail() {
    let root = tempfile::tempdir().unwrap();

    assert_eq!(read_log_tail(&root.path().join("absent.log")), None);
}

#[test]
fn a_huge_file_is_read_only_from_its_end() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("reprise.log");
    let filler = "x".repeat(100);
    let mut contents: String = (0..2_000).map(|_| format!("{filler}\n")).collect();
    contents.push_str("the last line\n");
    assert!(contents.len() as u64 > TAIL_MAX_BYTES * 4);
    std::fs::write(&path, contents).unwrap();

    let tail = read_log_tail(&path).unwrap();

    assert!(tail.ends_with("the last line"));
    assert!(tail.len() as u64 <= TAIL_MAX_BYTES);
    assert!(tail
        .lines()
        .all(|line| line.len() == 100 || line == "the last line"));
}
