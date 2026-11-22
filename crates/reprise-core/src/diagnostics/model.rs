use std::collections::VecDeque;

/// Session events retained. Each is already reduced to safe text when it is pushed.
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
    pub gvfs_version: Option<String>,
    pub remembered_device_count: Option<usize>,
    /// Ids of the optional modules switched on; `None` when they could not be read.
    pub enabled_modules: Option<Vec<String>>,
    /// The end of this run's log file, as written: its lines were safe when they were written.
    pub log_tail: Option<String>,
    /// The end of the previous run's log file, the crashed run's after a crash.
    pub previous_log_tail: Option<String>,
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

/// A session warning as the report shows it.
///
/// `message` is the body of a `SafeEvent` (its static template and kept
/// fields) and `location` its own call site: both are safe when stored, and
/// the report embeds them as they are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticEvent {
    pub seconds_since_midnight: u32,
    pub level: DiagnosticLevel,
    pub target: String,
    pub message: String,
    /// `<file>:<line>` of an own call site; `None` for a foreign event.
    pub location: Option<String>,
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
            location: None,
        }
    }

    /// The same event with its call site's location.
    #[must_use]
    pub fn with_location(self, location: Option<&str>) -> Self {
        Self {
            location: location.map(str::to_owned),
            ..self
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

    /// The retained events, oldest first.
    pub fn events(&self) -> impl Iterator<Item = &DiagnosticEvent> {
        self.events.iter()
    }

    pub(super) fn latest(&self, limit: usize) -> impl Iterator<Item = &DiagnosticEvent> {
        self.events.iter().rev().take(limit)
    }
}
