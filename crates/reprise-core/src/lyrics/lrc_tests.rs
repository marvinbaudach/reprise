use super::*;

#[test]
fn lrc_parser_orders_multiple_timestamps_and_scales_fractions() {
    let lines = parse_lrc(
        "[00:02.5][00:01.25] repeated\n\
         [00:03.005] final\n\
         [ar:metadata]\n\
         malformed",
    );

    assert_eq!(
        lines,
        vec![
            TimedLine::new(1_250, "repeated"),
            TimedLine::new(2_500, "repeated"),
            TimedLine::new(3_005, "final"),
        ]
    );
}

#[test]
fn active_line_uses_the_last_timestamp_not_after_position() {
    let lines = vec![
        TimedLine::new(1_000, "first"),
        TimedLine::new(2_000, "second"),
    ];

    assert_eq!(active_line_index(&lines, 999), None);
    assert_eq!(active_line_index(&lines, 1_000), Some(0));
    assert_eq!(active_line_index(&lines, 2_500), Some(1));
}

#[test]
fn malformed_timestamps_are_ignored_without_losing_valid_lines() {
    let lines = parse_lrc(
        "[00:60.00] invalid seconds\n\
         [-1:02.00] negative minutes\n\
         [00:01.1234] long fraction\n\
         [00:02.10] valid",
    );

    assert_eq!(lines, vec![TimedLine::new(2_100, "valid")]);
}
