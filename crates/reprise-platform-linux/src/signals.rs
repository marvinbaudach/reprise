//! Process signal dispositions, queried without touching them.
//!
//! The one place the platform layer asks the C library about signals, so the
//! frontend stays free of `unsafe`.

/// Whether the process currently ignores `signal` (`SIG_IGN`).
///
/// A parent can start a process with a signal ignored (`nohup`, a shell
/// background job); a caller that installs its own handler checks this first
/// so it does not override that choice. A signal number the kernel rejects is
/// reported as not ignored.
pub fn signal_is_ignored(signal: i32) -> bool {
    let mut current = std::mem::MaybeUninit::<libc::sigaction>::zeroed();
    // SAFETY: a null new action only queries the current disposition, and
    // `current` is a valid, zero-initialised out-pointer that `sigaction`
    // fills when it returns 0. An all-zero `sigaction` is a valid value.
    let queried = unsafe { libc::sigaction(signal, std::ptr::null(), current.as_mut_ptr()) } == 0;
    // SAFETY: the call above succeeded, so `current` is initialised.
    queried && unsafe { current.assume_init() }.sa_sigaction == libc::SIG_IGN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_5d_the_inherited_ignore_disposition_is_detected() {
        // SIGUSR1 and SIGUSR2 are free in the test binary: nothing listens.
        // SAFETY: `signal` only swaps the disposition of a signal nothing else
        // in this process uses, and the test restores it before returning.
        unsafe {
            libc::signal(libc::SIGUSR1, libc::SIG_IGN);
        }
        let ignored = signal_is_ignored(libc::SIGUSR1);
        let default_kept = signal_is_ignored(libc::SIGUSR2);
        // SAFETY: as above.
        unsafe {
            libc::signal(libc::SIGUSR1, libc::SIG_DFL);
        }

        assert!(ignored, "SIG_IGN is detected");
        assert!(!default_kept, "the default disposition is not ignored");
    }

    #[test]
    fn start_5d_an_invalid_signal_number_is_not_ignored() {
        assert!(!signal_is_ignored(-1));
    }
}
