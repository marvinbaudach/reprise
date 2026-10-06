use std::sync::mpsc;

use super::*;

const SIGNAL_A: i32 = 1;
const SIGNAL_B: i32 = 15;

/// A wedge limit no test waits out: the watchdog stays asleep.
const NEVER: Duration = Duration::from_secs(3600);
const SHORT_WEDGE_LIMIT: Duration = Duration::from_millis(50);
/// Long enough for a watchdog with `SHORT_WEDGE_LIMIT` to have fired.
const SETTLED: Duration = SHORT_WEDGE_LIMIT.saturating_mul(6);
const WAIT_LIMIT: Duration = Duration::from_secs(10);

/// The real listener thread, fed by hand, with a stand-in for ending the
/// process that reports every signal it was asked to end on.
struct Harness {
    feed: mpsc::Sender<i32>,
    received: async_channel::Receiver<i32>,
    ended: mpsc::Receiver<i32>,
    listener: JoinHandle<()>,
}

impl Harness {
    fn start(shared: &Arc<Shared>, wedge_limit: Duration) -> Self {
        let (feed, source) = mpsc::channel();
        let (sender, received) = async_channel::bounded(1);
        let (ended, ended_signals) = mpsc::channel();
        let ended = Mutex::new(ended);
        let listener = spawn(
            move |deliver| source.into_iter().for_each(deliver),
            shared.clone(),
            sender,
            move |signal| {
                let _ = ended.lock().unwrap().send(signal);
            },
            wedge_limit,
        )
        .unwrap();
        Self {
            feed,
            received,
            ended: ended_signals,
            listener,
        }
    }

    fn send(&self, signals: &[i32]) {
        for signal in signals {
            self.feed.send(*signal).unwrap();
        }
    }

    /// Blocks until the listener has forwarded a request to the main loop.
    fn forwarded(&self) -> i32 {
        let deadline = Instant::now() + WAIT_LIMIT;
        loop {
            if let Ok(signal) = self.received.try_recv() {
                return signal;
            }
            assert!(Instant::now() < deadline, "no request was forwarded");
            thread::yield_now();
        }
    }

    /// Ends the feed, waits for the listener to drain it and returns the
    /// requests the main loop would still find and the signals that ended
    /// the process.
    fn finish(self) -> (Vec<i32>, Vec<i32>) {
        drop(self.feed);
        self.listener.join().unwrap();
        let received = std::iter::from_fn(|| self.received.try_recv().ok()).collect();
        (received, self.ended.try_iter().collect())
    }
}

/// Runs `signals` through a listener whose watchdog never fires.
fn relay(shared: &Arc<Shared>, signals: &[i32]) -> (Vec<i32>, usize) {
    let relay = Harness::start(shared, NEVER);
    relay.send(signals);
    let (received, ended) = relay.finish();
    (received, ended.len())
}

#[test]
fn start_5c_the_first_request_is_forwarded() {
    assert_eq!(
        verdict(false, None, false, Instant::now()),
        Verdict::Forward
    );
}

#[test]
fn start_5c_a_repeat_before_the_main_loop_took_the_first_is_coalesced() {
    assert_eq!(
        verdict(true, None, false, Instant::now()),
        Verdict::Coalesce
    );
}

#[test]
fn start_5c_a_repeat_inside_the_grace_after_the_take_is_coalesced() {
    let taken = Instant::now();
    let now = taken + REPEAT_GRACE - Duration::from_millis(1);
    assert_eq!(verdict(true, Some(taken), false, now), Verdict::Coalesce);
}

#[test]
fn start_5c_a_repeat_after_the_grace_ends_the_process() {
    let taken = Instant::now();
    let now = taken + REPEAT_GRACE;
    assert_eq!(verdict(true, Some(taken), false, now), Verdict::EndProcess);
}

#[test]
fn start_5c_a_first_signal_after_the_application_stopped_ends_the_process() {
    assert_eq!(
        verdict(false, None, true, Instant::now()),
        Verdict::EndProcess
    );
}

#[test]
fn start_5c_a_repeat_inside_the_grace_is_coalesced_after_the_application_stopped() {
    let taken = Instant::now();
    let now = taken + Duration::from_millis(1);
    assert_eq!(verdict(true, Some(taken), true, now), Verdict::Coalesce);
}

#[test]
fn start_5c_a_repeat_after_the_grace_ends_the_process_once_the_application_stopped() {
    let taken = Instant::now();
    let now = taken + REPEAT_GRACE;
    assert_eq!(verdict(true, Some(taken), true, now), Verdict::EndProcess);
}

#[test]
fn start_5c_a_stopped_application_ends_a_request_nothing_will_take() {
    assert_eq!(
        verdict(true, None, true, Instant::now()),
        Verdict::EndProcess
    );
}

#[test]
fn start_5c_two_signals_in_quick_succession_reach_the_main_loop_once_and_end_nothing() {
    let (received, ended) = relay(&Arc::new(Shared::default()), &[SIGNAL_A, SIGNAL_B]);

    assert_eq!(received, vec![SIGNAL_A]);
    assert_eq!(
        ended, 0,
        "the repeat must not end the process before the save"
    );
}

#[test]
fn start_5c_a_repeat_stays_coalesced_while_the_main_loop_is_saving() {
    let shared = Arc::new(Shared::default());
    shared.mark_taken(SIGNAL_A);

    let (received, ended) = relay(&shared, &[SIGNAL_A, SIGNAL_B, SIGNAL_A]);

    assert_eq!(received, vec![SIGNAL_A]);
    assert_eq!(ended, 0);
}

#[test]
fn start_5c_a_first_signal_reaching_a_released_listener_ends_the_process() {
    let shared = Arc::new(Shared::default());
    shared.release();

    let (received, ended) = relay(&shared, &[SIGNAL_A]);

    assert!(received.is_empty());
    assert_eq!(ended, 1);
}

#[test]
fn start_5c_a_repeat_during_teardown_does_not_end_the_process_inside_the_grace() {
    let shared = Arc::new(Shared::default());
    let relay = Harness::start(&shared, NEVER);
    relay.send(&[SIGNAL_A]);
    assert_eq!(relay.forwarded(), SIGNAL_A);
    shared.mark_taken(SIGNAL_A);
    shared.release();
    relay.send(&[SIGNAL_B]);

    let (_, ended) = relay.finish();

    assert!(ended.is_empty(), "the repeat cut the teardown short");
}

#[test]
fn start_5c_a_request_the_main_loop_never_takes_ends_the_process_without_a_repeat() {
    let relay = Harness::start(&Arc::new(Shared::default()), SHORT_WEDGE_LIMIT);
    relay.send(&[SIGNAL_A]);

    let ended = relay.ended.recv_timeout(WAIT_LIMIT);

    assert_eq!(ended, Ok(SIGNAL_A), "the watchdog ended the wedged process");
    relay.finish();
}

#[test]
fn start_5c_the_watchdog_leaves_a_request_the_main_loop_took_alone() {
    let shared = Arc::new(Shared::default());
    shared.mark_taken(SIGNAL_A);
    let relay = Harness::start(&shared, SHORT_WEDGE_LIMIT);
    relay.send(&[SIGNAL_A]);

    let ended = relay.ended.recv_timeout(SETTLED);

    assert_eq!(ended, Err(mpsc::RecvTimeoutError::Timeout));
    relay.finish();
}

#[test]
fn start_5c_the_wedge_limit_outlasts_the_repeat_grace() {
    assert!(WEDGE_LIMIT > REPEAT_GRACE);
}

#[test]
fn start_5e_a_handled_signal_ends_the_process_the_way_that_signal_would() {
    let shared = Shared::default();
    shared.mark_taken(SIGNAL_B);
    let ended = std::cell::RefCell::new(Vec::new());

    end_as_handled(&shared, |signal| ended.borrow_mut().push(signal));

    assert_eq!(*ended.borrow(), vec![SIGNAL_B]);
}

#[test]
fn start_5e_a_normal_exit_without_a_handled_signal_ends_nothing() {
    let ended = std::cell::Cell::new(0);

    end_as_handled(&Shared::default(), |_| ended.set(ended.get() + 1));

    assert_eq!(ended.get(), 0);
}

#[test]
fn start_5e_the_first_handled_signal_decides_the_exit() {
    let shared = Shared::default();
    shared.mark_taken(SIGNAL_A);
    shared.mark_taken(SIGNAL_B);

    assert_eq!(shared.handled_signal(), Some(SIGNAL_A));
}

#[test]
fn start_5f_a_second_start_registers_nothing() {
    let cell = OnceLock::new();
    let registered = std::cell::Cell::new(0);
    let register = || {
        registered.set(registered.get() + 1);
        Ok(Some(registered.get()))
    };

    let first = start_once(&cell, register).unwrap();
    let second = start_once(&cell, register).unwrap();

    assert_eq!(first, Some(&1));
    assert_eq!(second, None);
    assert_eq!(registered.get(), 1, "the second start must not register");
}

#[test]
fn start_5f_a_start_that_armed_nothing_may_be_retried() {
    let cell: OnceLock<u8> = OnceLock::new();

    assert_eq!(start_once(&cell, || Ok(None)).unwrap(), None);
    assert_eq!(start_once(&cell, || Ok(Some(7))).unwrap(), Some(&7));
}

#[test]
fn start_5d_signals_that_were_ignored_at_start_are_not_armed() {
    let armed = armed(&[SIGNAL_A, SIGNAL_B, 2], |signal| signal == SIGNAL_A);

    assert_eq!(armed, vec![SIGNAL_B, 2]);
}
