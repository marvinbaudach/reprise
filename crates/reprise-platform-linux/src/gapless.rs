//! Gapless-playback plumbing for the `playbin3` backend (see `player.rs`).
//!
//! Kernidee: `playbin3` fragt kurz vor Track-Ende per `about-to-finish` nach
//! dem nächsten URI. Setzen wir dort `uri` *ohne* `set_state(Null)`, geht die
//! Wiedergabe nahtlos in den nächsten Track über — kein Puffer-Neuaufbau, keine
//! Stille. Dieses Modul kapselt die beiden thread-heiklen Bausteine dafür:
//!
//! 1. [`NextUri`] / [`HandoffFlag`] — der geteilte Zustand zwischen Steuer-
//!    Thread (`set_next`/`play`), GStreamer-Streaming-Thread (`about-to-finish`)
//!    und Bus-Watch (`StreamStart`).
//! 2. [`connect_about_to_finish`] — verdrahtet das Signal am playbin.
//! 3. [`note_stream_start`] — entscheidet beim `StreamStart` der Bus-Watch, ob
//!    dieser Stream-Wechsel ein Gapless-Handoff war (dann `AdvancedToNext`) oder
//!    nur der ganz normale erste Track eines `play()` (dann nichts).

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use reprise_core::library::settings::TrackTransition;
use reprise_core::playback::PlayerEvent;

use crate::crossfade::Transition;

/// Slot für den vorgefütterten nächsten URI. `None` = nichts vorgefüttert
/// (Gapless aus, Queue-Ende, oder manueller Sprung hat ihn invalidiert).
/// "Last write wins": das Frontend füttert bei jeder Queue-Änderung neu.
pub(crate) type NextUri = Arc<Mutex<Option<String>>>;

/// Wird vom `about-to-finish`-Handler auf `true` gesetzt, sobald er einen
/// vorgefütterten URI tatsächlich in den playbin gereicht hat, und vom
/// nachfolgenden `StreamStart` der Bus-Watch wieder auf `false` gedreht — genau
/// dann (und nur dann) ist der beobachtete Stream-Wechsel ein Gapless-Handoff.
pub(crate) type HandoffFlag = Arc<AtomicBool>;

/// Verbindet `about-to-finish` am `playbin`. Läuft auf einem GStreamer-
/// Streaming-Thread: der Handler liest nur den Mutex und setzt Properties/das
/// Atomic — keine Cross-Thread-UI-Callbacks. Der URI wird per `take()`
/// entnommen, damit derselbe Nachfolger nie doppelt gehandet wird.
///
/// **Modus-abhängig:** Der uri-Swap feuert NUR im `Gapless`-Modus. Im
/// `Crossfade`-Modus wird die Überblendung schon früher, positionsgetrieben,
/// gestartet (siehe `crossfade.rs`) und entnimmt den Nachfolger selbst — ein
/// gleichzeitiger Gapless-Swap hier würde damit konkurrieren. Im `Off`-Modus
/// füttert das Frontend gar keinen Nachfolger, sodass hier ebenfalls nichts zu
/// tun ist; der strikte `== Gapless`-Check ist die sichere Formulierung davon.
///
/// Das emittierende Element kommt aus `values[0]` (statt den playbin in seinen
/// eigenen Signal-Handler zu capturen — das erzeugte einen Referenzzyklus, der
/// das Element am Aufräumen hindern würde).
pub(crate) fn connect_about_to_finish(
    playbin: &gst::Element,
    next_uri: NextUri,
    handoff_pending: HandoffFlag,
    transition: Transition,
) {
    playbin.connect("about-to-finish", false, move |values| {
        let Ok(playbin) = values[0].get::<gst::Element>() else {
            tracing::error!("about-to-finish: emitter was not a gst::Element");
            return None;
        };
        let mode = transition.lock().unwrap_or_else(PoisonError::into_inner).0;
        if mode != TrackTransition::Gapless {
            return None;
        }
        let queued = next_uri
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(uri) = queued {
            tracing::debug!(%uri, "gapless: feeding next uri on about-to-finish");
            playbin.set_property("uri", &uri);
            handoff_pending.store(true, Ordering::SeqCst);
        }
        None
    });
}

/// Reagiert auf ein `StreamStart` der Bus-Watch. `StreamStart` feuert bei jedem
/// Stream-Start — sowohl beim ganz normalen ersten Track eines `play()` als auch
/// beim gapless vorgefütterten Nachfolger. Nur im zweiten Fall steht
/// `handoff_pending` auf `true` (vom `about-to-finish`-Handler gesetzt); dann
/// wird es zurückgesetzt und `AdvancedToNext` gefeuert, damit das Frontend sein
/// Queue-Modell um genau einen Schritt nachzieht.
pub(crate) fn note_stream_start(
    handoff_pending: &HandoffFlag,
    on_event: &(dyn Fn(PlayerEvent) + Send + Sync),
) {
    if handoff_pending.swap(false, Ordering::SeqCst) {
        tracing::debug!("gapless: StreamStart after handoff -> AdvancedToNext");
        on_event(PlayerEvent::AdvancedToNext);
    }
}
