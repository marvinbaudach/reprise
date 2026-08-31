//! Explicit policies for source-artwork network access and loading.

#[derive(Clone, Copy)]
pub(crate) enum ArtworkNetworkPolicy {
    Allowed,
    Blocked,
}

impl ArtworkNetworkPolicy {
    pub(crate) fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }
}

impl From<bool> for ArtworkNetworkPolicy {
    fn from(value: bool) -> Self {
        if value {
            Self::Allowed
        } else {
            Self::Blocked
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ArtworkLoadPolicy {
    Load,
    Defer,
}
