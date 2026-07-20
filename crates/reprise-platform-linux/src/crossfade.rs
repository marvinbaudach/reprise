//! Crossfade plumbing for the `playbin3` backend (Phase B — see `player.rs`).
//!
//! Kernidee: Anders als Gapless (ein playbin, uri-Swap auf `about-to-finish`)
//! ist Crossfade *überlappend*. Kurz vor Track-Ende — `crossfade_seconds` vor
//! Schluss — baut der Position-Ticker eine **zweite** `playbin3`-Pipeline für den
//! vorgefütterten Nachfolger, startet sie leise (`volume = 0`) und blendet dann
//! in einem kurzlebigen Thread beide Volumes invers über: die alte Pipeline von
//! `user_volume` auf 0, die neue von 0 auf `user_volume`. Kein `audiomixer` —
//! zwei Sinks spielen kurz gleichzeitig, der Audio-Server mischt. Am Ende der
//! Rampe wird die neue Pipeline zur Primär-`playbin` befördert (in den geteilten
//! `Arc<Mutex>` getauscht), die alte auf `Null` verworfen und — wie beim
//! Gapless-Handoff — `AdvancedToNext` emittiert, damit das Frontend sein
//! Queue-Modell um einen Schritt nachzieht.
//!
//! Die Gain-Berechnung [`crossfade_gains`] ist als reine Funktion herausgezogen:
//! sie ist der headless deterministisch beweisbare Kern (Unit-Tests am Dateiende).
//! Die eigentliche akustische Überblendung ist nur per Hörtest verifizierbar.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::f64::consts::FRAC_PI_2;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use reprise_core::library::settings::TrackTransition;
use reprise_core::playback::{AudioEffects, PlayerEvent};

use crate::gapless::{HandoffFlag, NextUri};
use crate::player::{attach_bus_watch, build_playbin};

/// Geteilter (Modus, Sekunden)-Zustand. Der Ticker liest ihn zur Trigger-
/// Entscheidung, `set_transition` schreibt ihn, und der `about-to-finish`-
/// Handler (siehe `gapless.rs`) liest den Modus, um im Crossfade-Modus NICHT
/// gapless zu swappen (sonst konkurrierten Gapless-Swap und Crossfade).
pub(crate) type Transition = Arc<Mutex<(TrackTransition, u8)>>;

/// Slot für die eingehende Sekundär-Pipeline während einer laufenden
/// Überblendung. Der Rampen-Thread legt sie hier ab; ein Abbruch
/// (`play`/`stop`/`seek`) entnimmt sie und schaltet sie auf `Null`, damit die
/// zweite (kurz hörbare) Pipeline sofort verstummt statt bis zum nächsten
/// Rampenschritt weiterzulaufen.
pub(crate) type IncomingSlot = Arc<Mutex<Option<gst::Element>>>;

/// Schrittweite der Volume-Rampe. 50 ms ist fein genug, dass die Überblendung
/// als kontinuierlich wahrgenommen wird, aber grob genug, dass der Thread fast
/// die ganze Zeit schläft.
const RAMP_STEP: Duration = Duration::from_millis(50);

const MS_PER_SECOND: u64 = 1000;

/// Equal-power-Überblendungskurve. Liefert `(out, in)` für den bisher
/// verstrichenen Rampenanteil `t = elapsed_ms / total_ms`:
///
/// - `out = cos(t · π/2)` — die ausklingende (Primär-)Pipeline,
/// - `in  = sin(t · π/2)` — die einsetzende (Sekundär-)Pipeline.
///
/// Equal-power statt linear, weil bei unkorrelierten Signalen die *Leistung*
/// (∝ Amplitude²) addiert; `out² + in² = cos²+sin² = 1` hält die
/// wahrgenommene Gesamtlautstärke über die ganze Blende konstant und vermeidet
/// den hörbaren Lautstärke-Einbruch in der Mitte, den eine lineare Rampe
/// (out+in nur 0.5+0.5, Leistung 0.5) erzeugen würde.
///
/// Randfest: `elapsed_ms` wird auf `total_ms` geklemmt (t ≤ 1), `total_ms == 0`
/// ergibt sofort den Endzustand `(0, 1)` (keine Division durch null).
pub(crate) fn crossfade_gains(elapsed_ms: u64, total_ms: u64) -> (f64, f64) {
    if total_ms == 0 {
        return (0.0, 1.0);
    }
    let t = (elapsed_ms.min(total_ms) as f64) / (total_ms as f64);
    let angle = t * FRAC_PI_2;
    (angle.cos(), angle.sin())
}

/// Bündel geteilter Zustandshandles, das der Position-Ticker (zum Starten einer
/// Überblendung) und der Rampen-Thread (zum Durchführen und Befördern) teilen.
/// Alles `Arc`-basiert; `Clone` ist ein reiner Refcount-Bump. Der `Player` hält
/// dieselben Handles direkt und konstruiert dieses Bündel via `Player::engine`.
#[derive(Clone)]
pub(crate) struct CrossfadeEngine {
    pub(crate) playbin: Arc<Mutex<gst::Element>>,
    pub(crate) bus_watch: Arc<Mutex<gst::bus::BusWatchGuard>>,
    pub(crate) on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    pub(crate) effects: Arc<Mutex<AudioEffects>>,
    pub(crate) next_uri: NextUri,
    pub(crate) handoff_pending: HandoffFlag,
    pub(crate) transition: Transition,
    /// Guard „gerade läuft eine Überblendung". Verhindert Doppel-Trigger im
    /// Ticker und lässt die Bus-Watch das EOS der ausklingenden Pipeline
    /// unterdrücken (kein spurioses `TrackFinished` mitten in der Blende).
    pub(crate) crossfading: Arc<AtomicBool>,
    /// Ziel-/Decken-Volume der Rampe (was `set_volume` zuletzt wollte). Die
    /// einsetzende Pipeline blendet auf diesen Wert hoch und bleibt danach dort.
    pub(crate) user_volume: Arc<Mutex<f64>>,
    /// Monoton steigender Generationszähler. Jeder Start und jeder Abbruch
    /// erhöht ihn; der Rampen-Thread merkt sich seinen Wert bei Spawn und bricht
    /// ab, sobald er nicht mehr passt — so terminiert ein verwaister Thread
    /// sicher, ohne verworfene Elemente anzufassen.
    pub(crate) generation: Arc<AtomicU64>,
    pub(crate) incoming: IncomingSlot,
    pub(crate) spectrum_enabled: Arc<AtomicBool>,
}

impl CrossfadeEngine {
    /// Vom Position-Ticker jeden Tick gerufen. Startet — falls Modus
    /// `Crossfade`, ein Nachfolger vorgefüttert ist, keine Blende läuft und die
    /// Position im letzten `seconds`-Fenster liegt — eine Überblendung: sie
    /// entnimmt den Nachfolger-URI, markiert `crossfading`, und spawnt den
    /// Rampen-Thread. Der eigentliche Pipeline-Aufbau läuft im Thread, damit der
    /// Ticker weiter zügig Positionen liefert.
    pub(crate) fn maybe_start(&self, position_ms: i64, duration_ms: i64) {
        let (mode, seconds) = *self
            .transition
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if mode != TrackTransition::Crossfade || self.crossfading.load(Ordering::SeqCst) {
            return;
        }
        if duration_ms <= 0 {
            return;
        }
        let seconds = seconds.max(1) as i64;
        let trigger_at = duration_ms - seconds * MS_PER_SECOND as i64;
        if position_ms < trigger_at {
            return;
        }
        // Nachfolger entnehmen; ohne einen gibt es nichts zu überblenden.
        let uri = {
            let mut slot = self.next_uri.lock().unwrap_or_else(PoisonError::into_inner);
            match slot.take() {
                Some(uri) => uri,
                None => return,
            }
        };
        // Blende beanspruchen, bevor der Thread läuft: der nächste Tick darf
        // nicht erneut triggern, und ein Abbruch erhöht `generation` wieder.
        self.crossfading.store(true, Ordering::SeqCst);
        let my_generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let total_ms = (seconds as u64) * MS_PER_SECOND;
        let engine = self.clone();
        std::thread::spawn(move || engine.run(&uri, my_generation, total_ms));
    }

    /// Rampen-Thread-Rumpf: baut die Sekundär-Pipeline, spielt sie leise an,
    /// blendet beide Volumes invers über und befördert am Ende. Bricht bei jedem
    /// Schritt ab, sobald `generation` nicht mehr `my_generation` ist.
    fn run(self, uri: &str, my_generation: u64, total_ms: u64) {
        if self.generation.load(Ordering::SeqCst) != my_generation {
            return; // schon abgebrochen, bevor der Thread loslief
        }
        let effects = self
            .effects
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let secondary = match build_playbin(
            &effects,
            self.next_uri.clone(),
            self.handoff_pending.clone(),
            self.transition.clone(),
        ) {
            Ok(element) => element,
            Err(error) => {
                tracing::warn!(%error, "crossfade: could not build secondary pipeline; aborting fade");
                self.crossfading.store(false, Ordering::SeqCst);
                return;
            }
        };
        if let Err(error) = crate::player_effects::set_playbin_spectrum_messages(
            &secondary,
            self.spectrum_enabled.load(Ordering::SeqCst),
        ) {
            tracing::warn!(%error, "crossfade: could not configure spectrum analyzer");
        }
        secondary.set_property("uri", uri);
        secondary.set_property("volume", 0.0_f64);
        if let Err(error) = secondary.set_state(gst::State::Playing) {
            tracing::warn!(%error, "crossfade: secondary pipeline refused Playing; aborting fade");
            let _ = secondary.set_state(gst::State::Null);
            self.crossfading.store(false, Ordering::SeqCst);
            return;
        }
        *self.incoming.lock().unwrap_or_else(PoisonError::into_inner) = Some(secondary.clone());

        let start = Instant::now();
        loop {
            if self.generation.load(Ordering::SeqCst) != my_generation {
                // Abbruch: der Abbruch-Pfad hat den incoming-Slot bereits geleert
                // und ggf. auf Null geschaltet; unser Klon hier zur Sicherheit auch.
                let _ = secondary.set_state(gst::State::Null);
                return;
            }
            let elapsed = start.elapsed().as_millis() as u64;
            let (out_gain, in_gain) = crossfade_gains(elapsed, total_ms);
            let user_volume = *self
                .user_volume
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            {
                let primary = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
                primary.set_property("volume", out_gain * user_volume);
            }
            secondary.set_property("volume", in_gain * user_volume);
            if elapsed >= total_ms {
                break;
            }
            std::thread::sleep(RAMP_STEP);
        }
        self.promote(&secondary, my_generation);
    }

    /// Befördert die fertige Sekundär-Pipeline zur neuen Primär-`playbin`:
    /// frischen Bus-Watch anhängen, unter dem `playbin`-Lock atomar tauschen,
    /// alte Pipeline auf `Null`, Ziel-Volume wiederherstellen, `AdvancedToNext`.
    fn promote(self, secondary: &gst::Element, my_generation: u64) {
        // Vor dem Anhängen des Watches etwaige *veraltete* Bus-Nachrichten
        // verwerfen (z. B. ein EOS, das ein sehr kurzer Nachfolger schon während
        // der Blende gepostet hat). In der Praxis (echte Songs) steht hier
        // nichts an; die Drainage schützt nur gegen Stale-Events aus Tests mit
        // Winz-Fixtures — der neue Handoff (`AdvancedToNext`) wird ohnehin
        // manuell emittiert, nicht aus dem StreamStart der Sekundär abgeleitet.
        if let Some(bus) = secondary.bus() {
            while bus.pop().is_some() {}
        }
        let new_watch = match attach_bus_watch(
            secondary,
            self.on_event.clone(),
            self.handoff_pending.clone(),
            self.crossfading.clone(),
        ) {
            Ok(watch) => watch,
            Err(error) => {
                tracing::warn!(%error, "crossfade: could not attach bus watch to promoted pipeline");
                let _ = secondary.set_state(gst::State::Null);
                self.crossfading.store(false, Ordering::SeqCst);
                return;
            }
        };
        let user_volume = *self
            .user_volume
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        {
            let mut primary = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
            // Letzte Abbruch-Prüfung unter dem Lock: ist zwischenzeitlich
            // abgebrochen worden, die frisch gebaute Sekundär verwerfen und die
            // Primär in Ruhe lassen (der Abbruch-Pfad hat schon aufgeräumt).
            if self.generation.load(Ordering::SeqCst) != my_generation {
                drop(new_watch);
                let _ = secondary.set_state(gst::State::Null);
                return;
            }
            let _ = self
                .incoming
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            let old = std::mem::replace(&mut *primary, secondary.clone());
            if let Err(error) = old.set_state(gst::State::Null) {
                tracing::debug!(%error, "crossfade: outgoing pipeline refused Null (dropping anyway)");
            }
            secondary.set_property("volume", user_volume);
        }
        // Ersetzt den Watch-Guard der alten Primär (dessen Drop den alten Watch
        // entfernt) durch den der neuen.
        *self
            .bus_watch
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = new_watch;
        self.crossfading.store(false, Ordering::SeqCst);
        (self.on_event)(PlayerEvent::AdvancedToNext);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gains_at_start_are_full_out_silent_in() {
        let (out_gain, in_gain) = crossfade_gains(0, 1000);
        assert!((out_gain - 1.0).abs() < 1e-9, "out should be 1.0 at t=0");
        assert!(in_gain.abs() < 1e-9, "in should be 0.0 at t=0");
    }

    #[test]
    fn gains_at_end_are_silent_out_full_in() {
        let (out_gain, in_gain) = crossfade_gains(1000, 1000);
        assert!(out_gain.abs() < 1e-9, "out should be 0.0 at t=1");
        assert!((in_gain - 1.0).abs() < 1e-9, "in should be 1.0 at t=1");
    }

    #[test]
    fn gains_are_monotonic_over_the_fade() {
        let total = 1000;
        let mut previous = crossfade_gains(0, total);
        for step in 1..=20 {
            let elapsed = step * total / 20;
            let current = crossfade_gains(elapsed, total);
            assert!(
                current.0 <= previous.0 + 1e-12,
                "out gain must be non-increasing"
            );
            assert!(
                current.1 >= previous.1 - 1e-12,
                "in gain must be non-decreasing"
            );
            previous = current;
        }
    }

    #[test]
    fn equal_power_holds_across_the_fade() {
        // out² + in² ≈ 1 an jedem Punkt — das ist die definierende Eigenschaft
        // der Equal-Power-Kurve (konstante Gesamtleistung, kein Mitten-Einbruch).
        let total = 1000;
        for step in 0..=10 {
            let elapsed = step * total / 10;
            let (out_gain, in_gain) = crossfade_gains(elapsed, total);
            let power = out_gain * out_gain + in_gain * in_gain;
            assert!(
                (power - 1.0).abs() < 1e-9,
                "expected out²+in² ≈ 1 at elapsed={elapsed}, got {power}"
            );
        }
    }

    #[test]
    fn elapsed_beyond_total_clamps_to_end_state() {
        let (out_gain, in_gain) = crossfade_gains(5000, 1000);
        assert!(out_gain.abs() < 1e-9);
        assert!((in_gain - 1.0).abs() < 1e-9);
    }

    #[test]
    fn zero_total_yields_end_state_without_dividing_by_zero() {
        let (out_gain, in_gain) = crossfade_gains(0, 0);
        assert_eq!(out_gain, 0.0);
        assert_eq!(in_gain, 1.0);
    }
}
