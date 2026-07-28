use super::{badge_text, fetch_allowed, updates_badge, Feed, FeedBadge, FeedRefresh};

fn ready(unseen: i64) -> FeedBadge {
    FeedBadge {
        enabled: true,
        ready: true,
        unseen,
    }
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
