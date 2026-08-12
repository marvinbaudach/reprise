use std::collections::VecDeque;

/// Session events retained before privacy redaction at report-render time.
pub const DIAGNOSTIC_EVENT_CAPACITY: usize = 200;

/// Package shape reported by the desktop collector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackageKind {
    Native,
    Flatpak { app_id: Option<String> },
}

/// Optional facts rendered into the fixed, line-oriented debug report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticFacts {
    pub version: Option<String>,
    pub git_sha: Option<String>,
    pub build_profile: Option<String>,
    pub package: Option<PackageKind>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub gnome_version: Option<String>,
    pub display_server: Option<String>,
    pub gtk_version: Option<String>,
    pub libadwaita_version: Option<String>,
    pub rust_version: Option<String>,
    pub gstreamer_version: Option<String>,
    pub audio_backend: Option<String>,
    pub locale: Option<String>,
    pub db_schema: Option<i64>,
    pub db_journal_mode: Option<String>,
    pub track_count: Option<i64>,
    pub db_size_bytes: Option<u64>,
    pub libmtp_version: Option<String>,
    pub remembered_device_count: Option<usize>,
}

/// Values whose literal appearance must be removed from session log messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RedactionContext {
    pub music_dir: Option<String>,
    pub home_dir: Option<String>,
    pub username: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Warn,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticEvent {
    pub seconds_since_midnight: u32,
    pub level: DiagnosticLevel,
    pub target: String,
    pub message: String,
}

impl DiagnosticEvent {
    pub fn new(
        seconds_since_midnight: u32,
        level: DiagnosticLevel,
        target: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            seconds_since_midnight,
            level,
            target: target.into(),
            message: message.into(),
        }
    }
}

/// Fixed-capacity session log. Oldest events leave first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticLog {
    events: VecDeque<DiagnosticEvent>,
}

impl DiagnosticLog {
    pub fn push(&mut self, event: DiagnosticEvent) {
        if self.events.len() == DIAGNOSTIC_EVENT_CAPACITY {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub(super) fn latest(&self, limit: usize) -> impl Iterator<Item = &DiagnosticEvent> {
        self.events.iter().rev().take(limit)
    }
}
