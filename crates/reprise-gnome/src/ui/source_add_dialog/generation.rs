/// Which request an async result belongs to. A dialog bumps it when it starts a new request and drops
/// every result that carries an older value. Wraps on overflow, like the `u64` counters it replaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) struct Generation(u64);

impl Generation {
    #[must_use]
    pub(in crate::ui) fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[cfg(test)]
#[path = "generation_tests.rs"]
mod tests;
