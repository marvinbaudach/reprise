use super::{badge_text, delta_batch, fetch_allowed, updates_badge, Feed, FeedBadge, FeedRefresh};

fn ready(unseen: i64) -> FeedBadge {
    FeedBadge {
        enabled: true,
        ready: true,
        unseen,
    }
}

#[test]
fn unseen_items_are_the_current_delta_batch() {
    let batch = delta_batch(vec![("first", None), ("second", None)], |item| item.1, 5);

    assert_eq!(batch.shown, [("first", None), ("second", None)]);
    assert_eq!(batch.total, 2);
    assert!(batch.unseen, "nothing here has been read yet");
}

#[test]
fn the_most_recent_seen_visit_is_the_fallback_batch() {
    let batch = delta_batch(
        vec![("older", Some(10)), ("newer", Some(20)), ("same", Some(20))],
        |item| item.1,
        5,
    );

    assert_eq!(batch.shown, [("newer", Some(20)), ("same", Some(20))]);
    assert_eq!(batch.total, 2);
    assert!(
        !batch.unseen,
        "a batch held over from the last visit is not new, and announcing it \
         as new would contradict a badge that has already cleared"
    );
}

/// The badge counts unseen entries, the header count describes the batch on
/// screen. Both must answer the same question, or the surface contradicts
/// itself — one saying "nothing new" while the other claims "2 new".
#[test]
fn a_batch_is_only_new_while_something_in_the_feed_is_unseen() {
    let mixed = delta_batch(
        vec![("read", Some(10)), ("fresh", None)],
        |item| item.1,
        5,
    );
    assert!(mixed.unseen);
    assert_eq!(mixed.shown, [("fresh", None)]);

    let all_read = delta_batch(vec![("read", Some(10))], |item| item.1, 5);
    assert!(!all_read.unseen);
    assert_eq!(all_read.total, 1, "it still renders, it just is not new");

    let empty = delta_batch(Vec::<(&str, Option<i64>)>::new(), |item| item.1, 5);
    assert!(!empty.unseen, "an empty feed has nothing new to announce");
}

#[test]
fn a_delta_batch_preserves_input_order_while_applying_its_cap() {
    let batch = delta_batch(
        vec![("first", None), ("second", None), ("third", None)],
        |item| item.1,
        2,
    );

    assert_eq!(batch.shown, [("first", None), ("second", None)]);
    assert_eq!(batch.total, 3);
}

#[test]
fn an_empty_feed_has_an_empty_delta_batch() {
    let batch = delta_batch(Vec::<(&str, Option<i64>)>::new(), |item| item.1, 5);

    assert!(batch.shown.is_empty());
    assert_eq!(batch.total, 0);
}

#[test]
fn a_zero_cap_keeps_the_full_batch_count() {
    let batch = delta_batch(vec![("first", None), ("second", None)], |item| item.1, 0);

    assert!(batch.shown.is_empty());
    assert_eq!(batch.total, 2);
}

#[test]
fn unseen_items_exclude_every_already_seen_item_from_the_batch() {
    let batch = delta_batch(
        vec![
            ("seen", Some(30)),
            ("unseen-first", None),
            ("older", Some(20)),
            ("unseen-second", None),
        ],
        |item| item.1,
        5,
    );

    assert_eq!(
        batch.shown,
        [("unseen-first", None), ("unseen-second", None)]
    );
    assert_eq!(batch.total, 2);
}

#[test]
fn a_fetch_needs_an_enabled_idle_feed_that_is_due() {
    assert!(fetch_allowed(true, false, true));
    assert!(!fetch_allowed(false, false, true), "disabled");
    assert!(!fetch_allowed(true, true, true), "already fetching");
    assert!(!fetch_allowed(true, false, false), "not due yet");
}

#[test]
fn a_run_is_complete_only_once_every_feed_has_answered() {
    let mut run = FeedRefresh::start(&[Feed::NewReleases, Feed::Concerts]);
    assert!(!run.is_complete());

    run.finish(Feed::Concerts, false);
    assert!(!run.is_complete(), "new releases has not answered yet");
    assert!(run.is_pending(Feed::NewReleases));

    run.finish(Feed::NewReleases, false);
    assert!(run.is_complete());
    assert!(run.failed().is_empty());
}

#[test]
fn a_run_over_one_feed_completes_with_that_feed() {
    let mut run = FeedRefresh::start(&[Feed::Concerts]);
    run.finish(Feed::Concerts, true);

    assert!(run.is_complete());
    assert_eq!(run.failed(), &[Feed::Concerts]);
    assert!(run.has_failed(Feed::Concerts));
    assert!(!run.has_failed(Feed::NewReleases));
}

#[test]
fn a_run_with_no_participating_feed_is_already_complete() {
    let run = FeedRefresh::start(&[]);

    assert!(run.is_complete());
    assert!(run.failed().is_empty());
}

#[test]
fn a_duplicate_answer_cannot_complete_a_run_early() {
    let mut run = FeedRefresh::start(&[Feed::NewReleases, Feed::Concerts]);

    run.finish(Feed::Concerts, true);
    run.finish(Feed::Concerts, true);

    assert!(!run.is_complete(), "new releases is still outstanding");
    assert_eq!(
        run.failed(),
        &[Feed::Concerts],
        "and the failure is counted once"
    );
}

#[test]
fn an_answer_from_a_feed_that_never_started_is_ignored() {
    let mut run = FeedRefresh::start(&[Feed::Concerts]);

    run.finish(Feed::NewReleases, true);

    assert!(!run.is_complete());
    assert!(run.failed().is_empty());
}

#[test]
fn the_badge_hides_itself_rather_than_showing_a_zero() {
    assert_eq!(badge_text(0), None);
    assert_eq!(badge_text(-3), None);
    assert_eq!(badge_text(1).as_deref(), Some("1"));
    assert_eq!(badge_text(9).as_deref(), Some("9"));
    assert_eq!(badge_text(10).as_deref(), Some("9+"));
}

#[test]
fn the_badge_sums_both_feeds() {
    assert_eq!(updates_badge(ready(2), ready(3)).as_deref(), Some("5"));
}

#[test]
fn a_feed_that_is_disabled_or_has_never_fetched_contributes_nothing() {
    let never_fetched = FeedBadge {
        ready: false,
        ..ready(7)
    };
    let disabled = FeedBadge {
        enabled: false,
        ..ready(7)
    };

    assert_eq!(updates_badge(never_fetched, ready(2)).as_deref(), Some("2"));
    assert_eq!(updates_badge(disabled, ready(2)).as_deref(), Some("2"));
    assert_eq!(updates_badge(never_fetched, disabled), None);
}

#[test]
fn a_negative_unseen_count_cannot_reduce_the_other_feed() {
    let broken = FeedBadge {
        unseen: -5,
        ..ready(0)
    };

    assert_eq!(updates_badge(broken, ready(4)).as_deref(), Some("4"));
}
