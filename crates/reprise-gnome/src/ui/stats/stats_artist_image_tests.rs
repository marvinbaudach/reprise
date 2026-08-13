use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;

/// The walk down the candidate list is plain arithmetic and must hold without a
/// display: the first candidate is tried first, each failure advances by one,
/// and a list that runs out ends the walk instead of wrapping.
#[test]
fn stats_23_the_candidate_walk_advances_once_per_failure() {
    let candidates = vec!["/music/one.flac".to_string(), "/music/two.flac".to_string()];

    assert_eq!(next_candidate(&candidates, 0), Some("/music/one.flac"));
    assert_eq!(next_candidate(&candidates, 1), Some("/music/two.flac"));
    assert_eq!(next_candidate(&candidates, 2), None);
    assert_eq!(next_candidate(&[], 0), None);
}

/// With the module off no name may reach the fetch queue — the setting is the
/// only thing standing between the stats page and a request per artist.
#[test]
fn stats_23_a_disabled_module_queues_no_portrait_request() {
    let resolver_calls = Arc::new(AtomicUsize::new(0));
    let runtime = ArtistPortraitRuntime::for_test(false, {
        let resolver_calls = resolver_calls.clone();
        move |_| {
            resolver_calls.fetch_add(1, Ordering::SeqCst);
            None
        }
    });
    let result = Rc::new(RefCell::new(Vec::new()));

    assert!(!runtime.is_enabled());
    runtime.request("Lorna Shore".to_string(), {
        let result = result.clone();
        move |path| result.borrow_mut().push(path)
    });

    assert_eq!(resolver_calls.load(Ordering::SeqCst), 0);
    assert_eq!(&*result.borrow(), &[None]);
}
