use std::cell::Cell;

/// Startup-only gate that collapses any number of model-load requests into
/// one load when construction and session routing have both finished.
pub(in crate::ui) struct StartupLoad {
    deferred: Cell<bool>,
    pending: Cell<bool>,
}

impl StartupLoad {
    #[cfg(test)]
    pub(in crate::ui) const fn immediate() -> Self {
        Self {
            deferred: Cell::new(false),
            pending: Cell::new(false),
        }
    }

    pub(in crate::ui) const fn deferred() -> Self {
        Self {
            deferred: Cell::new(true),
            pending: Cell::new(false),
        }
    }

    /// Returns whether the caller should perform the requested load now.
    pub(in crate::ui) fn request(&self) -> bool {
        if !self.deferred.get() {
            return true;
        }
        self.pending.set(true);
        false
    }

    /// Ends startup deferral and returns whether one pending load is owed.
    pub(in crate::ui) fn finish(&self) -> bool {
        if !self.deferred.replace(false) {
            return false;
        }
        self.pending.replace(false)
    }

    pub(in crate::ui) fn is_deferred(&self) -> bool {
        self.deferred.get()
    }
}
