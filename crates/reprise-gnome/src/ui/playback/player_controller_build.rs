use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::media_integration::DEFAULT_VOLUME;
use reprise_core::queue::Queue;
use reprise_core::up_next::UpNextQueue;

use crate::ui::compact_player::CompactPlayer;
use crate::ui::cover_download_worker::CoverDownloadRuntime;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::mpris_mirror;
use crate::ui::player_bar::PlayerBar;
use crate::ui::player_controller_wiring;
use crate::ui::player_lyrics::PlayerLyrics;

use super::player_controller::PlayerController;
use super::player_controller_types::PlayerControllerBackends;
use super::scrobble_runtime::ScrobbleRuntime;
use super::scrobble_session::ScrobbleSession;

impl PlayerController {
    /// Builds the controller and the event bridge around injected platform
    /// backends assembled by the window composition root.
    /// `conn` is the same UI-owned database connection `track_list.rs`
    /// holds, used to record plays. Platform construction failures are handled
    /// before this function is called so feature code only sees core contracts.
    pub(in crate::ui) fn new(
        conn: Rc<Db>,
        cover_download: CoverDownloadRuntime,
        listenbrainz: Rc<ScrobbleRuntime>,
        lastfm: Rc<ScrobbleRuntime>,
        backends: PlayerControllerBackends,
        app: &adw::Application,
    ) -> Rc<Self> {
        let PlayerControllerBackends {
            playback: player,
            playback_events: receiver,
            media: handles,
            waveform,
        } = backends;
        let initial_effects = super::audio_effects::apply_initial(player.as_ref(), &conn);
        let library_has_tracks = super::queue_transport::initial_library_availability(&conn);
        {
            // Apply the stored transition mode to the backend up front so
            // Gapless/Crossfade is active from the first track (feed_next then
            // pre-feeds once playback starts).
            let conn_ref = &conn;
            player.set_transition(
                reprise_core::library::settings::get_track_transition(conn_ref),
                reprise_core::library::settings::get_crossfade_seconds(conn_ref),
            );
        }
        // Media integration is always on. Its platform handles are assembled
        // by the window composition root and remain failure-tolerant.
        let mpris_state = handles.shared_state;
        let agent_queue_state = handles.queue_state;
        let mpris_receiver = handles.commands;
        let mpris_seek_notify = handles.seek_notify;
        let _device_sync_state = handles.device_sync_state;
        let _device_sync_commands = handles.device_sync_commands;

        let lyrics = PlayerLyrics::new(&conn);
        let controller = Rc::new(Self {
            player,
            active_audio_effects: RefCell::new(initial_effects),
            bar: PlayerBar::new(),
            compact_player: CompactPlayer::new(),
            conn,
            random_start_chooser: RefCell::new(Box::new(
                reprise_core::queries::query_random_live_track_ids,
            )),
            pending_random_start: RefCell::new(None),
            current_track: Cell::new(None),
            max_position_ms: Cell::new(0),
            listenbrainz,
            lastfm,
            scrobble_session: RefCell::new(ScrobbleSession::default()),
            queue: RefCell::new(Queue::new()),
            library_has_tracks: Cell::new(library_has_tracks),
            last_composed_tail: RefCell::new(None),
            restored_placement_intact: Cell::new(false),
            pending_start_mark: Cell::new(None),
            up_next: RefCell::new(UpNextQueue::default()),
            current_up_next: Cell::new(None),
            prefed_next_track: Cell::new(None),
            history: RefCell::default(),
            deferred_queue_purge_id: Cell::new(None),
            play_origin: RefCell::new(None),
            external: RefCell::new(super::external_media::ExternalPlaybackState::default()),
            pending_local_seek: RefCell::new(None),
            toast_overlay: glib::WeakRef::new(),
            reload_track_list: RefCell::new(None),
            listen_event_recorded: RefCell::new(None),
            queue_changed: RefCell::new(Vec::new()),
            current_track_changed: RefCell::new(Vec::new()),
            playback_state_changed: RefCell::new(Vec::new()),
            bass_changed: RefCell::new(Vec::new()),
            now_playing_panel_track_changed: RefCell::new(None),
            now_playing_panel_state_changed: RefCell::new(None),
            song_visual_spectrum_changed: RefCell::new(None),
            song_visuals_module: Cell::new(false),
            view_refill_ids: RefCell::new(None),
            consecutive_skips: Cell::new(0),
            failure_skip_limit: Cell::new(0),
            consecutive_episode_skips: Cell::new(0),
            mpris_state,
            agent_queue_state,
            now_playing: Rc::new(RefCell::new(None)),
            volume: Cell::new(DEFAULT_VOLUME),
            mpris_seek_notify,
            cover_loader: CoverLoader::new(cover_download),
            bar_cover_generation: Rc::new(Cell::new(0)),
            compact_cover_generation: Rc::new(Cell::new(0)),
            lyrics,
            waveform_generation: Rc::new(Cell::new(0)),
            waveform_cancel: RefCell::new(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            ))),
            waveform_backend: waveform,
            application: {
                let weak = glib::WeakRef::new();
                weak.set(Some(app.upcast_ref::<gio::Application>()));
                weak
            },
        });

        player_controller_wiring::wire_bar_controls(&controller);
        player_controller_wiring::wire_compact_controls(&controller);
        controller.sync_transport_enabled(false);

        let song_visuals_enabled = reprise_core::modules::is_enabled(
            &controller.conn,
            &reprise_core::modules::SONG_VISUALS_MODULE,
        )
        .unwrap_or(reprise_core::modules::SONG_VISUALS_MODULE.default_enabled);
        controller.song_visuals_module.set(song_visuals_enabled);
        if let Err(error) = controller.sync_audio_reactive() {
            tracing::warn!(%error, "could not restore live song visuals; using the static profile");
        }
        crate::ui::player_event_handling::spawn_event_drain(&controller, receiver);

        // MPRIS-command drain: see `mpris_mirror.rs`'s `spawn_command_drain`
        // doc comment (moved there — Stage-3 close-out — to keep this file's
        // line count comfortably under the split-file gate).
        mpris_mirror::spawn_command_drain(&controller, mpris_receiver);

        controller
    }
}
