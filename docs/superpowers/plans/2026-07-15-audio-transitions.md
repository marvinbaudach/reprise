# Umsetzungsplan: Audio Transitions (Gapless + Crossfade)

**Datum:** 2026-07-15 · **Status:** Entwurf, wartet auf Freigabe
**Kontext:** Redesign-Feature „Audio Transitions" aus dem Mock. Ziel: nahtlose
Übergänge zwischen Tracks. Zwei Stufen sehr unterschiedlicher Größe — dieser
Plan trennt sie sauber und liefert Gapless zuerst.

---

## 0. Ist-Zustand (verifiziert)

- **Engine:** `playbin3`, ein einzelnes `gst::Element`, gekapselt in
  `reprise-platform-linux/src/player.rs` (`Player`), hinter der Trait
  `PlaybackBackend` (`reprise-core/src/playback.rs`).
- **Queue-Advance:** rein im UI-Layer. EOS → Bus-Watch feuert
  `PlayerEvent::TrackFinished` → `apply_event` (`player_controller.rs:710`) →
  `advance_playback(Automatic)` (`up_next_transport.rs:55`) berechnet den
  nächsten Track aus `Queue` + `UpNextQueue` → `play_track_id(id)` →
  `PlaybackBackend::play(path)`. `play` fährt die Pipeline auf **`Null`**,
  setzt das neue `uri` und geht auf `Playing`.
- **Der Gap** entsteht ausschließlich in diesem `Null`→`Playing`-Zyklus.
- **Play-Tracking/Scrobble:** `evaluate_play_tracking()` (`play_tracking.rs`)
  wertet den *gerade beendeten* Track aus (max_position vs. duration) und
  schreibt `listen_event` + Scrobble. Läuft heute bei jedem Track-Ende über
  `reset_to_stopped`/`play_track_id`.
- **Events:** `PlayerEvent { StateChanged, Position, TrackFinished, Error }`.
- **Settings-Muster:** String-persistierte Enums wie `ReplayGainMode`
  (`settings.rs:172`, `REPLAY_GAIN_MODE_KEY = "playback.replay_gain_mode"`,
  `get_/set_`-Paar).

**Kernproblem für Gapless:** `playbin3`s `about-to-finish`-Signal feuert auf
einem **GStreamer-Streaming-Thread**. Die „nächster Track"-Entscheidung lebt
aber im Main-Thread-UI-Layer (`Rc`/`RefCell`, **nicht `Send`**). Wir dürfen
aus dem Streaming-Thread nicht in den Controller zurückrufen.

---

## Phase A — Gapless (empfohlen, zuerst)

Nativer `playbin3`-Weg. Kein zweiter Decode, kein Mixer. Perceived-Quality-Win
für Live-Alben, Klassik, Konzeptalben, DJ-Mixes.

### A1. Architektur-Entscheidung: „Next-URI vorfüttern"

Statt den Streaming-Thread rechnen zu lassen, **füttert der Controller den
nächsten URI vorab** in den Backend-Slot. Das Signal liest nur diesen Slot.

```
Controller (Main-Thread)                Backend (player.rs)
────────────────────────                ───────────────────
track startet / Queue ändert sich
  → compute next id (peek, ohne mutieren)
  → resolve path → set_next(Some(path)) ─────► speichert Arc<Mutex<Option<String>>>
                                               │
GStreamer streaming-thread:                    │
  about-to-finish  ───────────────────────────► liest Slot, set uri = next,
                                                 KEIN Null-Zyklus
  stream-start (neue group-id) ──────────────► emit PlayerEvent::AdvancedToNext
                                               │
Controller apply_event(AdvancedToNext) ◄───────┘
  → Queue-Modell 1 Schritt weiter (ohne play())
  → evaluate_play_tracking() für den beendeten Track
  → now-playing/cover/waveform/MPRIS/scrobble sync
  → nächsten Next-URI neu vorfüttern
```

**Warum elegant:** Die Entscheidung bleibt vollständig im bestehenden
`Queue`/`UpNextQueue`-Code (Main-Thread, kein `Send` nötig). Das Backend hält
nur einen dummen String-Slot. Genau der API-Fit, den die Trait erlaubt.

### A2. Core-Trait & Event-Modell (`reprise-core/src/playback.rs`)

- `PlaybackBackend` erweitern:
  ```rust
  /// Queue the next track for gapless handoff, or `None` to clear it
  /// (end of queue / gapless disabled). Idempotent; safe to call on every
  /// queue mutation.
  fn set_next(&self, path: Option<&str>);
  ```
- `PlayerEvent` erweitern:
  ```rust
  /// The queued next track has taken over gaplessly (about-to-finish
  /// consumed the pre-fed URI). The UI advances its queue model WITHOUT
  /// issuing a new play().
  AdvancedToNext,
  ```
- **Peek statt Advance:** `up_next_transport`/`Queue` brauchen eine
  nicht-mutierende Vorschau. `next_target` heute mutiert (`pop_front`,
  `advance_auto`). Neu: `fn peek_next_target(&…) -> Option<i64>` — dieselbe
  Logik, aber ohne State-Änderung (Repeat-One, Up-Next-Front, `advance_auto`
  read-only spiegeln). DRY: gemeinsame Kernfunktion, einmal mit, einmal ohne
  Commit. **Unit-testbar in `reprise-core`, ganz ohne GTK/GStreamer.**

### A3. Backend (`reprise-platform-linux/src/player.rs`)

- Feld `next_uri: Arc<Mutex<Option<String>>>`.
- `set_next(path)`: `path_to_uri` auflösen, in den Slot legen (oder `None`).
- In `build_playbin`: `about-to-finish`-Signal verbinden:
  ```rust
  playbin.connect("about-to-finish", false, move |vals| {
      if let Some(uri) = next_uri.lock()… .take() {
          playbin.set_property("uri", uri);   // KEIN set_state(Null)
      }
      None
  });
  ```
- Übergang erkennen: auf der Bus die `StreamStart`-Message (neue `group-id`)
  auswerten → `PlayerEvent::AdvancedToNext` emittieren. (Fallback prüfen:
  ob `playbin3` bei about-to-finish-Handoff zuverlässig `StreamStart`
  liefert; sonst über den `source-setup`/`uri`-Wechsel triggern.)
- **`play()` bleibt** der harte Sprung (manuelles Next/Prev, neue Auswahl,
  Fehler-Recovery). Gapless betrifft nur den *automatischen* Track-Wechsel.
- Wedged-Recovery (`rebuild_playbin`) unangetastet — `next_uri`-Slot beim
  Rebuild leeren.

### A4. Controller-Wiring (`reprise-gnome/src/ui/playback/…`)

- **`apply_event`:** neuer Arm `PlayerEvent::AdvancedToNext`:
  1. `evaluate_play_tracking()` für den beendeten Track (Scrobble/Stats),
  2. Queue-Modell committen (das, was `peek_next_target` vorhergesagt hat —
     jetzt real `advance_playback`-Kernlogik ohne `play_track_id`),
  3. now-playing/cover/waveform/MPRIS-Mirror auf den neuen Track sync
     (die vorhandenen `sync_*`-Pfade wiederverwenden),
  4. `feed_next()` erneut aufrufen.
- **`feed_next()`** (neu, `pub(super)`): berechnet `peek_next_target`,
  löst den Pfad auf, ruft `player.set_next(...)`. Aufgerufen nach:
  `play_track_id`, `AdvancedToNext`, jeder Queue-/Up-Next-/Repeat-/Shuffle-
  Mutation, und `set_next(None)` bei Stop/leerer Queue oder wenn Gapless aus.
- **`TrackFinished` bleibt** als Sicherheitsnetz: wenn kein Next vorgefüttert
  war (Gapless aus, Queue-Ende, Repeat-Off am Schluss), läuft der Track auf
  echtes EOS → bisheriger Pfad (`advance_playback`) greift unverändert.
- **Repeat-One:** darf **nicht** gapless denselben Track vorfüttern, wenn das
  Doppel-Zählungen bei Stats verursacht — hier bewusst weiter über EOS +
  `advance_playback` laufen lassen (oder Next = derselbe Pfad, aber
  Play-Tracking sauber pro Runde auswerten). **Entscheidung beim Bau messen.**

### A5. Setting + UI

- `settings.rs`: `GAPLESS_ENABLED_KEY = "playback.gapless_enabled"` (bool) —
  bzw. gemeinsame Enum mit Crossfade, siehe A7. `get_/set_`-Paar + Migration
  (Default: **an**, ist der erwartete moderne Default).
- Preferences → Playback: Toggle „Gapless playback" (`adw::SwitchRow`) im
  neuen Transitions-Bereich. Beim Umschalten sofort `feed_next()` bzw.
  `set_next(None)`.

### A6. Verifikation (headless möglich)

- **`reprise-core` Unit:** `peek_next_target` == `next_target`-Ergebnis für
  alle Fälle (Up-Next, Repeat-All/One, Shuffle, Queue-Ende), aber ohne
  State-Mutation (vorher/nachher gleich).
- **`reprise-platform-linux` headless** (fakesink, `AUDIO_SINK_TEST_LOCK`):
  `set_next` + kurzer Track → `about-to-finish` feuert → zweiter Track spielt
  **ohne** dass `set_state(Null)` dazwischen lag; `AdvancedToNext` wird
  emittiert; ein durchgehender Position-Verlauf ohne Reset auf 0-dann-Sprung.
  (Zwei sehr kurze Fixtures nötig — `sine.flac` + ein zweites.)
- **Controller-Logik:** `AdvancedToNext`-Arm testbar über die vorhandenen
  Controller-Tests (Queue-Modell rückt genau einen Schritt, kein doppeltes
  `play`).
- **Ohren (du):** finaler A/B-Test an einem gapless-Album (z. B. Live-Set).

### A7. Aufwand & Risiken (Phase A)

- **Aufwand:** mittel. Trait/Event/Peek (klein, core), Backend-Signal (klein),
  Controller-Wiring (der Löwenanteil: Sync-Pfade + Mutations-Trigger sauber
  auf `feed_next()` verdrahten).
- **Risiken:** (a) `StreamStart`/group-id-Timing bei playbin3-Handoff —
  Fallback-Trigger einplanen. (b) Scrobble-Doppelzählung am Übergang —
  `evaluate_play_tracking` exakt einmal pro Track. (c) Race: Queue-Mutation
  *während* about-to-finish gerade den Slot liest — `Mutex` schützt den Slot,
  Semantik „letzter gewinnt" ist akzeptabel (kurzes Fenster, worst case ein
  suboptimaler Übergang, nie ein Crash).

---

## Phase B — Crossfade (separates Follow-up)

Deutlich größer. `playbin3` kann es **nicht**; braucht parallelen Decode.

### B1. Architektur (ENTSCHIEDEN — umgesetzt)

**Crossfade = interner Modus des bestehenden `Player`**, NICHT ein separater
Backend-Typ. Grund: **laufzeit-umschaltbar** (Preference wirkt sofort ohne
Neustart) und **Null-Regressionsrisiko** — bei Off/Gapless verhält sich der
Player exakt wie zuvor. Trait-Fläche: `set_transition(mode, crossfade_seconds)`
(bereits gemergt); Frontend pusht Mode+Sekunden beim Start und bei jeder
Preference-Änderung (`apply_transition`).

**Kein `audiomixer`.** Zwei `playbin3` spielen kurz gleichzeitig, jede mit
eigenem Sink; der Audio-Server mischt sie. Der Übergang wird **positions-
getrieben** (nicht via about-to-finish): der Position-Ticker startet die
Sekundär-`playbin` `crossfade_seconds` vor Ende und rampt beide `volume`-
Properties invers (Primär 1→0, Sekundär 0→user_volume) über einen kurzlebigen
Rampen-Thread. Rampenkurve: **equal-power** (`cos`/`sin`), um den Mitten-
Einbruch zu vermeiden. Nach der Rampe wird die Sekundär zur neuen Primär
(Swap im `Arc<Mutex>`, frischer bus_watch), `AdvancedToNext` wird gefeuert.
Im Crossfade-Modus ist der Gapless-about-to-finish-Swap unterdrückt (Modus-
Check), damit sich beide Mechanismen nicht überlagern. Abbruch bei
play/stop/seek über einen Generation-/Abort-Guard, den der Rampen-Thread prüft.

### B2. Verifikation

- **Unit (headless):** die Volume-Automations-Kurven (Control-Source-Werte an
  Stützstellen: bei t=0 A=1/B=0, bei t=dur A=0/B=1, monoton) — deterministisch
  testbar ohne Audio-Device.
- **Mischung selbst:** nur mit Ohren final verifizierbar. → braucht deinen
  Hörtest.

### B3. Aufwand & Risiken (Phase B)

- **Aufwand:** hoch. Neuer Backend-Typ, Mixer-Topologie, Controller-Kurven,
  Zusammenspiel mit EQ/ReplayGain-Filter (der heute pro `playbin` hängt →
  jetzt pro Kette), Seek/Pause-Semantik über zwei Ketten, saubere Teardown.
- **Risiken:** Zustandsmaschine über zwei Pipelines (Pause/Seek/Skip mitten im
  Fade), Ressourcen (zwei Decoder gleichzeitig), Interaktion mit MPRIS-Position
  und Play-Tracking (welcher Track „läuft" während der Überblendung?).

---

## Gemeinsames Setting-Modell (A + B)

Statt zwei Bools ein Enum (analog `ReplayGainMode`):

```rust
pub enum TrackTransition { None, Gapless, Crossfade { seconds: u8 } }
// TRANSITION_MODE_KEY = "playback.transition_mode"  (+ crossfade_seconds)
```

Preferences → Playback → **Transitions**: `adw::ComboRow` (Off / Gapless /
Crossfade) + bei Crossfade eine Dauer-`adw::SpinRow`. Default **Gapless**.
Phase A liefert None/Gapless; Crossfade-Option erst mit Phase B aktiv (vorher
ausgegraut/nicht gelistet).

---

## Lieferreihenfolge & Parallelisierung

1. **A-core** (parallelisierbar, keine GTK/GStreamer-Abhängigkeit):
   `PlaybackBackend::set_next` + `PlayerEvent::AdvancedToNext` + `peek_next_
   target` + Setting-Enum + `reprise-core`-Unit-Tests. → 1 Agent.
2. **A-backend** (nach A-core-Trait steht): `about-to-finish`-Wiring +
   `next_uri`-Slot + `StreamStart`-Event + headless-Test. → 1 Agent.
3. **A-controller** (nach A-core + A-backend): `feed_next()` + `AdvancedToNext`-
   Arm + Mutations-Trigger + Preferences-Toggle. → 1 Agent (der integrative,
   größte Teil; ich selbst).
4. **B** erst nach A grün + deinem Gapless-Hörtest — eigener Plan-Abschnitt,
   eigene Freigabe.

Schnitt 1↔2↔3: 1 und 2 können nach kurzem Trait-Freeze parallel laufen; 3
integriert. So bleibt die Merge-Fläche klein und jede Stufe ist einzeln
verifizierbar, bevor die nächste darauf aufsetzt.

---

## Offene Punkte für die Bau-Phase (bewusst dort entschieden)

- Repeat-One gapless vs. über EOS (Stats-Korrektheit) — A4.
- `StreamStart`-Zuverlässigkeit vs. Fallback-Trigger — A3.
- Crossfade als eigener Backend-Typ vs. interner Modus — B1 (Empfehlung: eigen).
