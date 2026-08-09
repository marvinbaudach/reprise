use super::{ColumnKey, Pin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Probe {
    Lead,
    Free,
    Trail,
}

impl ColumnKey for Probe {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lead => "lead",
            Self::Free => "free",
            Self::Trail => "trail",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "lead" => Some(Self::Lead),
            "free" => Some(Self::Free),
            "trail" => Some(Self::Trail),
            _ => None,
        }
    }

    fn all() -> &'static [Self] {
        &[Self::Lead, Self::Free, Self::Trail]
    }

    fn default_visible() -> &'static [Self] {
        &[Self::Free]
    }

    fn pin(self) -> Option<Pin> {
        match self {
            Self::Lead => Some(Pin::Leading),
            Self::Trail => Some(Pin::Trailing),
            Self::Free => None,
        }
    }
}
