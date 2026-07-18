---
slug: motion-player
worktree: /home/marvin/Projects/reprise/.worktrees/transitions
branch: feat/motion-player
phase: planned
codex_session:
created: 2026-07-18
---
# MOT-5 Player-Motion (Folge-Branch) — Implementierungsplan

**Status:** gegrillt 2026-07-18, bereit für /code (Phase A)
**Branch:** feat/motion-player (auf origin/main `3d878d4`)

Ziel: Flip von MOT-5 `[geplant]→[aktiv]` (docs/ux-rules.md, Sektion O) gemäß
Flip-Kriterium: Scale-Puls, Waveform-Crossfade und Pause-Entsättigung
implementiert und per `[gtk]`-Test gedeckt.

## 1. Gegrillte Beschlüsse (2026-07-18, final)

| Nr. | Frage | Beschluss |
|---|---|---|
| G1 | Scale-Puls-Mechanik | **CSS-`@keyframes` one-shot** per Klassen-Toggle beim Play⇄Pause-Wechsel. Dauer/Easing aus `motion::MICRO_MS`/`motion::MICRO_CSS_EASING` per `format!` in den CSS-String (kein rohes Literal im Quelltext). Klasse wird beim State-Wechsel gesetzt und per `glib::timeout_add_local_once(Duration::from_millis(motion::MICRO_MS as u64))` wieder entfernt; erneuter Wechsel während des Pulses: Klasse entfernen → im nächsten Frame neu setzen (sauberer Retrigger). Kein zusätzliches Gating nötig: GTK-CSS respektiert `gtk-enable-animations=false` nachweislich hart (T-V-Sentinel `mot_7_css_honours_enable_animations_setting`). |
| G2 | Waveform-Crossfade | **Alpha-Crossfade** alt→neu: Zwei-Pass-Draw (alte Display-Bars mit fallendem, neue mit steigendem Alpha) über **Ambient (400 ms)**. Erst-Track (keine alten Peaks) behält den bestehenden Stagger-Build-up. |
| G3 | Pause-Entsättigung | **OKLCH-Chroma-Reduktion ×0.45** zur Draw-Zeit; die Farbmathematik aus `cover_accent.rs` wird auf `pub(in crate::ui)` geöffnet statt dupliziert. Übergang **animiert über Standard (250 ms)** via `motion::timed` (desaturation_progress 0↔1); `gtk-enable-animations=false` → harter Flip. Die `cover_accent`-Pipeline (globaler `@reprise_player_accent`-Provider) bleibt unberührt — Entsättigung ist rein lokal im Waveform-`draw()`. |
| G4 | Queue-Animation | **Weggelassen.** Die MOT-4-Ausnahme ist erlaubend, nicht fordernd, und blockiert keinen Flip. Umsetzung erst, wenn der TAG-1-Reload-Pfad in einem eigenen Queue-Branch angefasst wird. |
| G5 | Sequenzierung | **Zweiphasig.** Phase A sofort (T0, T2 Waveform-Crossfade, T3 Entsättigung — Territorium frei). Phase B erst nach Merge der drei aktiven Player-Bar-Branches (`feat/artist-cover-playerbar-fixes`, `feat/keyboard-nav`, `feat/minor-improvements`): T1 Scale-Puls + T5 Flip. |

## 2. Audit-Kernbefunde (verifiziert)

- `ui/motion.rs` liefert MICRO/STANDARD/AMBIENT, `half()`, `timed()`
  (follow-enable-animations), `animations_enabled()`, `replace_animation()`
  (Skip). Icon-Crossfade (2×75) und Track-Crossfade (2×125) existieren
  bereits tokenisiert — sie allein flippen MOT-5 nicht.
- `set_peaks()` überschreibt `raw_peaks`/`display_peaks` hart
  (`waveform_seek.rs:317-319`); Build-up: `build_progress`/`build_start_us`
  (`:130-131`), Tick `ensure_tick_callback()` (`:407-449`), Stagger im
  `draw()` (`:486-504`). Kein Alt-Zustand, kein Opacity-Mechanismus im Draw.
- `draw()` liest die Fill-Farbe pro Frame via `area.color()` (`:476`) —
  `.waveform-seek { color: @reprise_player_accent; }`. `WaveformSeek` kennt
  den Playback-State NICHT (kein Setter); `PlayerBar::set_state()`
  (`player_bar.rs:370-386`) leitet nicht an die Waveform weiter.
- OKLCH-Konvertierung liegt in `cover_accent.rs:44-140` (modul-privat);
  `lerp`-Helfer `cover_accent.rs:309-313`. Kein gemeinsamer State zwischen
  cover_accent-Provider und Waveform-Draw (Unabhängigkeit bestätigt).
- Button-CSS: `.player-bar-play` mit `transition: … transform` und
  `:active { transform: scale(0.94) }` (`player_bar_layout.rs:268-278`) —
  CSS-Transform auf dem Button ist bewiesen. Keyframes-Präzedenz:
  `eq_bars.rs:91-115`, `player_bar_layout.rs:304-312`.
- Test-Vorbilder: `mot_6_second_track_and_state_changes_finish_the_previous_visual_state`,
  `mot_7_player_bar_hard_switches_when_system_animations_are_disabled`
  (player_bar_tests.rs), `mot_7_waveform_completes_build_up_when_animations_disabled_mid_build`
  (waveform_seek_tests.rs). Borrow-Scope-Disziplin vor `window.close()` beachten.
- Idle bestätigt: kein Dauerloop ohne Wiedergabe (Mini-EQ `.playing`,
  Trackliste `.playback-paused`, Skeleton statisch, Tick stoppt sich selbst).

## 3. Taskplan

> Ein Commit pro Task, TDD, Commit-Titel trägt die Regel-ID. Gates vor JEDEM
> Commit: `cargo fmt --check` · `cargo clippy --locked --all-targets
> --workspace -- -D warnings` · `env XDG_DATA_HOME=$(mktemp -d)
> XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace` ·
> `scripts/check-ux-traceability.sh` · `scripts/check-motion-tokens.sh` ·
> `scripts/check-architecture.sh`. Display-Tests headless EINZELN via
> `dbus-run-session -- xvfb-run -a env … cargo test -p reprise-gnome --
> --ignored --exact <pfad>` (gebündelt in einem Prozess → bekannter SIGSEGV).
> Statuswechsel `[geplant]→[aktiv]` NUR im benannten Task-Commit (T5, Phase B).

### Phase A — sofort (Waveform-Territorium ist frei)

#### Task 0: Plan committen

```bash
git commit -m "docs: add MOT-5 player-motion plan (grilled 2026-07-18)"
```

#### Task 2: Waveform-Crossfade beim Trackwechsel (nur `waveform_seek.rs` + Tests)

- `State` erweitern: `previous_bars: Vec<…>` (Kopie der zuletzt gezeichneten
  Display-Bars inkl. deren Höhen), `crossfade_progress: f64` (1.0 = kein
  Fade), `crossfade_start_us: i64`.
- `set_peaks()`: Sind alte Display-Bars vorhanden UND `animations_enabled()`,
  werden sie nach `previous_bars` kopiert und `crossfade_progress = 0.0`
  gestartet (Dauer `motion::AMBIENT_MS`); der Stagger-Build-up wird in diesem
  Fall NICHT gestartet (`build_progress = 1.0`). Ohne alte Bars (Erst-Track)
  bleibt der bestehende Build-up-Pfad unverändert. Disabled →
  `previous_bars.clear()`, `crossfade_progress = 1.0` (Hard-Switch, Muster
  des bestehenden disabled-Pfads).
- Tick (`ensure_tick_callback`): treibt `crossfade_progress` analog zu
  `build_progress`; settled-Bedingung um den Crossfade erweitert.
- `draw()`: während `crossfade_progress < 1.0` Zwei-Pass — alte Bars mit
  Alpha `(1 - p)`, neue mit Alpha `p` (jeweils multipliziert auf die
  bestehenden Alpha-Konstanten); danach exakt der heutige Ein-Pass-Pfad.
- Breitenwechsel während des Fades (`ensure_resampled` invalidiert
  Display-Bars): Fade auf Endzustand skippen (`previous_bars.clear()`,
  `crossfade_progress = 1.0`) — MOT-6-Geist, kein Resample der alten Bars.
- Schneller Trackwechsel während laufendem Fade: alter Fade endet hart
  (neue Bars werden `previous_bars`), neuer Fade startet — kein Stapeln.
- Tests (waveform_seek_tests.rs, Display-ignored wie Vorbilder):
  `mot_5_waveform_crossfades_to_the_new_track_instead_of_rebuilding`
  (alte Bars vorhanden → crossfade_progress startet, build_progress bleibt
  1.0; Endzustand nach Ablauf) und
  `mot_7_waveform_crossfade_hard_switches_when_animations_are_disabled`
  (disabled → sofort neue Bars, previous leer, kein Tick).

```bash
git commit -m "feat(motion): MOT-5 — waveform crossfades to the new track"
```

#### Task 3: Pause-Entsättigung (`waveform_seek.rs`, `player_bar.rs`-Wiring, `style/cover_accent.rs` Sichtbarkeit)

- `cover_accent.rs`: die benötigten Konvertierungs-Helfer (RGB↔OKLCH bzw.
  Chroma-Skalierung; minimal nötige Fläche, z. B. eine neue Funktion
  `pub(in crate::ui) fn scale_chroma(r, g, b, factor) -> (f64, f64, f64)`
  neben den bestehenden privaten Helfern) öffnen — KEINE Änderung an
  Provider/Slot/Extraktion.
- `WaveformSeek::set_paused(paused: bool)`: treibt `desaturation_progress`
  (0.0 = voll gesättigt, 1.0 = entsättigt) via `motion::timed(&area, from,
  to, motion::STANDARD, CallbackAnimationTarget{ set progress + queue_draw })`,
  Slot + `motion::replace_animation` (MOT-6: erneuter Wechsel skippt).
  `!animations_enabled()` → Wert hart setzen, kein Tick (Vorbild
  Positions-Glättung).
- `draw()`/`draw_fallback()`: nach `area.color()` die Fill-Farbe mit
  `scale_chroma(…, 1.0 - 0.55 * desaturation_progress)` dämpfen — bei
  `desaturation_progress = 1.0` ist das Chroma ×0.45 (Beschluss G3).
  Gilt für Played-Fill und Playhead; Unplayed-/Ghost-Alphas unverändert.
- **Wiring NICHT in Phase A:** `player_bar.rs` ist G5-gesperrt. Phase A
  liefert die `set_paused`-API + Tests auf Widget-Ebene; der Aufruf aus
  `PlayerBar::set_state()` (`state != PlaybackState::Playing`, Stopped zählt
  wie Paused) und die identische Verdrahtung im Compact-Player folgen in
  Phase B (T1-Commit).
- Tests: `mot_5_pause_desaturates_the_waveform_fill_and_play_restores_it`
  (set_paused(true) → progress-Ziel 1.0, Slot trägt STANDARD_MS +
  follows_enable_animations; set_paused(false) → zurück) und
  `mot_7_waveform_desaturation_hard_switches_when_animations_are_disabled`.
  Zusätzlich eine Unit-Assertion, dass `cover_accent`s Provider-Zustand von
  `set_paused` unberührt bleibt (kein Aufruf in die Provider-API — reine
  Draw-Zeit-Rechnung; z. B. per Testkommentar/Design, kein Mock-Zwang).

```bash
git commit -m "feat(motion): MOT-5 — pause desaturates the waveform fill"
```

**→ ENDE PHASE A. STOPP. T1/T5 sind gesperrt (Phase-B-Gate unten).**

### Phase-B-Gate (hartes Kriterium)

Phase B startet erst, wenn ALLE drei Branches
`feat/artist-cover-playerbar-fixes`, `feat/keyboard-nav`,
`feat/minor-improvements` in origin/main gemergt sind (inhaltlich — bei
Squash-Merges zählt der Datei-Stand, nicht der Branch-Zeiger), main
integriert wurde und ein erneuter Ownership-Scan
(`git diff --name-only origin/main...<branch> | grep player_bar`) für
player_bar.rs/player_bar_layout.rs leer ist.

### Phase B — nach dem Gate

#### Task 1: Scale-Puls Play⇄Pause (`player_bar.rs`, `player_bar_layout.rs`)

- CSS in `player_bar_layout.rs::css()`: `@keyframes reprise-play-pulse`
  (`0% scale(1.0)` → `50% scale(0.92)` → `100% scale(1.0)`) + Klasse
  `.player-bar-play.pulsing { animation: reprise-play-pulse {MICRO_MS}ms
  {MICRO_CSS_EASING} 1; }` — Werte per `format!` aus `motion`-Konstanten.
- `animate_play_icon_change()` (oder `set_state()`): beim echten
  State-Wechsel Klasse `pulsing` entfernen und (im Idle/nächsten Frame)
  neu setzen; Entfernen nach `MICRO_MS` per `timeout_add_local_once`.
  Koexistenz mit `:active`-Scale (Press) und Icon-Crossfade prüfen.
- **Entsättigungs-Wiring nachziehen (aus T3 verschoben):**
  `PlayerBar::set_state()` ruft `self.waveform.set_paused(state !=
  PlaybackState::Playing)`; Compact-Player identisch verdrahten.
  Display-Test: Pause über `set_state` → Waveform-Desaturation läuft an.
- Tests: `mot_5_play_pause_pulses_on_state_change` (Klasse gesetzt nach
  set_state, nach Ablauf entfernt — Display-Test mit Loop-Pump) +
  Verifikation, dass bei `gtk-enable-animations=false` der Endzustand
  identisch ist (Keyframes laufen nicht — T-V-gedeckt, Assertion:
  Klassen-Logik läuft trotzdem durch, Button-Zustand korrekt).

```bash
git commit -m "feat(motion): MOT-5 — play/pause scale pulse"
```

#### Task 5: Flip MOT-5 + Abschluss

- `docs/ux-rules.md`: MOT-5 `[geplant]→[aktiv]`; Flip-Kriterium-Kommentar
  entfernen. Vorher gegen main-Stand verifizieren (Ownership: einer
  integriert, nie zwei parallel).
- Ledger `.superpowers/sdd/progress.md`: Phase A+B, Beschlüsse G1–G5,
  Queue-Ausnahme bewusst nicht umgesetzt (G4).
- Volle Gate-Batterie inkl. aller mot_-Display-Tests einzeln.

```bash
git commit -m "docs: MOT-5 — flip player-bar motion rule to active"
```

## 4. Abnahme

- [ ] Trackwechsel: Waveform blendet alt→neu über 400 ms (Ambient); kein
      Absturz auf 0; Erst-Track behält Build-up. (Phase A)
- [ ] Pause entsättigt den Waveform-Fill sichtbar (Chroma ×0.45), animiert
      über 250 ms; Play kehrt es um; `cover_accent` unberührt. (Phase A)
- [ ] `gtk-enable-animations=false` → alles instant (Hard-Switch-Tests
      grün, kein hängender Tick). (Phase A)
- [ ] Play⇄Pause pulst 1.0→0.92→1.0 (Micro) zusätzlich zum Icon-Crossfade.
      (Phase B)
- [ ] MOT-5 `[aktiv]`; Traceability, Motion-Lint, volle Batterie grün.
      (Phase B)
- [ ] Kein Eingriff in die drei aktiven Player-Bar-Branches vor deren Merge.

## 5. Risiken

1. Crossfade × `ensure_resampled()`: Breitenwechsel während des Fades →
   Fade skippt auf Endzustand (beschlossen, T2) — kein Resample alter Bars.
2. Schnelles Play/Pause-Hämmern (Phase B): Klassen-Retrigger muss Style-
   Invalidierung erzwingen (remove → idle → add).
3. Player-Bar-Territorium bewegt sich aktiv — Phase-B-Gate mit erneutem
   Ownership-Scan ist verbindlich.
