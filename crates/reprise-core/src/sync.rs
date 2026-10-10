//! Lock helpers shared by the engine's process-wide state.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, recovering the guard when a previous holder panicked.
///
/// Recovery is safe only for values that stay valid after an interrupted
/// critical section: a single `Option`, `String` or map that is replaced or
/// updated in one step and never carries an invariant across two statements
/// under the lock. A panic on one worker thread must not turn every later
/// `lock()` into a second panic, so such mutexes route through this helper.
/// Do not use it for state whose consistency spans several fields.
pub(crate) fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn lock_unpoisoned_returns_the_guard_after_a_holder_panicked() {
        let mutex = Arc::new(Mutex::new(vec![1, 2, 3]));
        let poisoner = Arc::clone(&mutex);
        let joined = std::thread::spawn(move || {
            let _guard = poisoner.lock().expect("lock before poisoning");
            panic!("poison the mutex while holding the guard");
        })
        .join();
        assert!(joined.is_err(), "the spawned thread must have panicked");
        assert!(mutex.is_poisoned(), "the mutex must now be poisoned");

        let mut guard = lock_unpoisoned(&mutex);
        assert_eq!(*guard, vec![1, 2, 3], "the data survives the poisoning");
        guard.push(4);
        drop(guard);
        assert_eq!(*lock_unpoisoned(&mutex), vec![1, 2, 3, 4]);
    }
}
