use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Default)]
pub(in crate::ui) struct TagWriteGate {
    occupied: Arc<AtomicBool>,
}

impl TagWriteGate {
    pub(in crate::ui) fn try_acquire(&self) -> Option<TagWriteLease> {
        self.occupied
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
            .then(|| TagWriteLease {
                occupied: self.occupied.clone(),
            })
    }

    #[cfg(test)]
    fn is_busy(&self) -> bool {
        self.occupied.load(Ordering::Acquire)
    }
}

pub(in crate::ui) struct TagWriteLease {
    occupied: Arc<AtomicBool>,
}

impl Drop for TagWriteLease {
    fn drop(&mut self) {
        self.occupied.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::TagWriteGate;

    #[test]
    fn doc_6b_tag_write_gate_has_one_owner_and_releases_on_every_exit() {
        let gate = TagWriteGate::default();
        let lease = gate.try_acquire().expect("first writer must acquire");

        assert!(gate.is_busy());
        assert!(gate.try_acquire().is_none());
        drop(lease);

        assert!(!gate.is_busy());
        assert!(gate.try_acquire().is_some());
    }
}
