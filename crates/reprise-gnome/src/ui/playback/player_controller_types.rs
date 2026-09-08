//! Small transport and dependency types shared by the player controller's
//! sibling implementation modules.

use std::rc::Rc;
use std::sync::Arc;

use reprise_core::db::Db;
use reprise_core::media_integration::MediaIntegrationHandles;
use reprise_core::playback::{PlaybackBackend, PlayerEvent};
use reprise_core::waveform::RenderDataBackend;

/// The visible track view as the playback paths see it.
///
/// `ids` is what a refill plays: the visible query's id list, which stops at
/// `queries::QUEUE_LIMIT` rows. `total` is the same view's row count, and it
/// is *not* capped. PLAY-11 needs both, because the ids alone cannot tell a
/// complete library from the first 10,000 rows of a filtered one — see
/// `library_continuation::cleared_library_filter_handoff`.
///
/// Both come from one provider call so they always describe the same moment;
/// two providers could be read either side of a reload and disagree.
pub(in crate::ui) struct VisibleView {
    pub(in crate::ui) ids: Vec<i64>,
    pub(in crate::ui) total: usize,
}

impl VisibleView {
    /// The view a playback path must not refill from — the Queue view itself,
    /// or a track list that is already gone.
    pub(in crate::ui) fn none() -> Self {
        Self {
            ids: Vec::new(),
            total: 0,
        }
    }
}

pub(in crate::ui) type ViewRefillIds = Rc<dyn Fn() -> VisibleView>;
pub(in crate::ui) type RandomStartChooser = dyn FnMut(&Db) -> Result<Vec<i64>, rusqlite::Error>;

/// Whether presenting a track should start the playback pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum StartPlayback {
    /// Ordinary playback starts the newly presented track.
    Yes,
    /// Gapless handoff already started it; only presentation catches up.
    No,
}

/// Contract-only platform resources assembled by the window composition root.
/// Feature modules consume this bundle without naming a concrete OS backend.
pub(in crate::ui) struct PlayerControllerBackends {
    pub(in crate::ui) playback: Box<dyn PlaybackBackend>,
    pub(in crate::ui) playback_events: async_channel::Receiver<PlayerEvent>,
    pub(in crate::ui) media: MediaIntegrationHandles,
    pub(in crate::ui) waveform: Arc<dyn RenderDataBackend>,
}
