---
slug: ux-rules-motion
worktree: /home/marvin/Projects/reprise/.worktrees/transitions
branch: feat/ux-rules-motion
phase: reviewed
codex_session:
created: 2026-07-17
---
# UX-Motion (Sektion O) — Finaler Implementierungsplan

**Status:** gegrillt 2026-07-17, bereit für /code
**Branch:** feat/ux-rules-motion (aus Worktree `.worktrees/transitions`, s. Operative Notizen)
**Datum:** 2026-07-17

> Alle Entscheidungen sind final (Beschlüsse-Tabelle unten). Dieses Dokument ist
> die vollständige Anweisung für die headless code-Phase. **Die code-Phase
> implementiert AUSSCHLIESSLICH Phase 1 (Abschnitt 4.2) und stoppt dann.**
> Phase 2 startet erst nach dem harten Gate-Check in Abschnitt 4.3 — niemals
> im selben Lauf „einfach weitermachen".

---

## 1. Beschlüsse (Grilling 2026-07-17, alle final)

| Nr. | Frage | Beschluss |
|---|---|---|
| G1 | Sektionsbuchstabe & Integrationsreihenfolge | Sektion **O**. `docs/ux-rules.md` wird in diesem Branch erst angefasst, nachdem CTX („N. Track-Kontextmenü", feature/context-menu-unification) auf main ist und main integriert wurde. HTML-Kommentar im Sektionsvorspann dokumentiert die Buchstabenlage (M Tooltips auf main, N durch CTX beansprucht). |
| G2 | Token-Zuschnitt | **Micro 150 ms ease-out** · **Standard 250 ms ease-out-cubic**. Bestands-Flächen migrieren bewusst 150→250; Risiko „App fühlt sich anders an" bleibt als expliziter Abnahme-Hinweis. |
| G2i | Icon-Crossfade Play/Pause | **Micro-Hälften (2×75 ms)** — keine Sonderzahl. |
| G2ii | Akzentfade 400 ms | Viertes Token **„Ambient" 400 ms** für atmosphärische, nicht-interaktive Übergänge (Akzentfarben-Crossfade; künftig z. B. Artist-Hero-Glow). |
| G3 | Adw-interne Animationen im MOT-1-Wortlaut | Wortlaut „jede von Reprise selbst konfigurierte Animation"; Adw-interne Animationen ohne Dauer-API (OverlaySplitView, NavigationSplitView, ToastOverlay, Banner, Dialog, Popover) gelten als systemgegeben und sind ausgenommen — inkl. der Push/Pop-Slides der Einstellungs-Unterseiten. KEINE Anlehnungs-Soll-Klausel. |
| G4 | Spatial-Token | Bleibt im Regeltext („AdwSpringAnimation, Adw-Default-Springparameter, ab dem ersten gerichteten Navigationsfall"); in `ui/motion.rs` erst beim ersten Konsumenten angelegt (YAGNI). |
| G5 | Widget-Wahl linke Sidebar | Linke Sidebar wird **`adw::OverlaySplitView` (Position Start)** — exakt das Widget der rechten Spalte. `apply_sidebar_visibility`/`manually_hidden`/Breakpoint(<800 px)-Logik portieren; Tests um Breakpoint-Fälle erweitern. |
| G6 | Lint-Mechanik | `scripts/check-motion-tokens.sh` verbietet außerhalb `motion.rs`/`tokens.rs`: `TimedAnimation::new` mit Integer-Literal-Dauer und `set_transition_duration(`/`.transition_duration(` mit Literal. Ticks/CSS bleiben Review-Sache. |
| G7 | EQ-Dauerloops | `reprise-eq` und `mini-eq-bar` = benannte MOT-5-Ausnahme: „EQ-Indikatoren laufen nur während aktiver Wiedergabe." |
| G8 | MOT-7-Scope | Adw-Animationen via follow-enable-animations-Property + eigene Tick-Callbacks (Waveform-Positions-Glättung → Position hart setzen; Progress-Interpolation) + Pulse-Timer zentral gaten. `gtk::Spinner` und GTK-CSS-Mechanik = Systemverhalten, nicht gaten. T-V (CSS-Verhalten unter `gtk-enable-animations=false`) ist Pflicht-Verifikation; negativer Befund geht zurück ans Grilling, kein stiller Beschluss. |
| G9 | Scope-Schnitt | **Kernschnitt.** Dieser Branch: Sektion O + `ui/motion.rs` + Lint + Sidebar-Symmetrie (MOT-3 inkl. Satz 2) + Token-Migration (MOT-1) + MOT-7 + MOT-6-Skip + MOT-2. MOT-5-Neuverhalten (Scale-Puls, Waveform-Crossfade, Pause-Entsättigung) und Queue-Drop/Remove-Animation (MOT-4-Ausnahme) = Folge-Branch; MOT-5 bleibt `[geplant]` mit Flip-Kriterium-Kommentar (Tooltip-Muster TIP-1b/2b). |
| G10 | Pause-Entsättigung | Wortlaut: „Pause entsättigt den Waveform-Fill leicht (zur Draw-Zeit), Play kehrt es um" — Akzent-Pipeline (`cover_accent`) bleibt unberührt. |
| G-Pfad | Modul-Ort | **`ui/motion.rs`** (UI-Root-Ebene, neben `nav_history.rs`/`notifications.rs`): Token-Konstanten, `timed()`-Konstruktor (setzt follow-enable-animations), Gate-Helper `animations_enabled()`, Slot-Helfer `replace_animation()` mit `skip()`. `tokens.rs` behält die CSS-`TRANSITION`-Konstante und konsumiert künftig die Micro-Dauer aus `motion.rs`. |
| G-Sym | MOT-3 Satz 2 | Bleibt: innerer Tracks/Albums/Artists-Stack + StatusPage⇄Liste-Stacks crossfaden mit Standard-Token wie der äußere Library/Stats/Device-Stack. |
| G-Seq | Sequenzierung | **Zweiphasig** nach verifiziertem Ownership-Befund: Phase 1 sofort und konfliktfrei (T0, T2, T7, T8, T3); Phase 2 GATED hinter dem Merge von feat/missing-import-errors UND feature/context-menu-unification auf main (T1, T4, T5, T6, T9, T10). ALLE Status-Flips liegen in Phase 2 (sie brauchen die Sektion). Details Abschnitt 4. |

---

## 2. Audit-Inventar (kondensiert, selbst verifiziert)

Kein `.ui`/`.blp`/`.css`-Dateibestand — UI komplett imperativ, CSS inline als
Strings (`ui/style/mod.rs::app_css()` → `CssProvider::load_from_string`).

### 2.1 Explizit konfigurierte Widget-Transitions

| Stelle | Widget / Transition | Auslöser | Ziel-Token |
|---|---|---|---|
| `window/window.rs:368-369` | äußerer Stack (Library/Stats/Device), Crossfade 150 ms | Nutzer | Standard (Phase 2, T4) |
| `compact/compact_player_layouts.rs:140-141` | Revealer Crossfade 150 ms (Hover-Overlay Mini-Player, 1000-ms-Linger) | Nutzer | Micro (T7) |
| `scan/scan_progress.rs:155-156` | Revealer Crossfade 150 ms (Scan-Karte) | **Hintergrund** | Standard (Phase 2, T5) |
| `sidebar/sidebar_device_card.rs:250-254` | 2×Stack + 2×Revealer, 150 ms bzw. 0 bei enable-animations=false (dynamisch, Z.248-255) | **Hintergrund** (Sync) | Standard (Phase 2, T5) |
| `info_panel/info_panel.rs:148` | Stack Crossfade, Default-Dauer (~200 ms) | Nutzer (Tab) | Standard (T8) |
| `lyrics/lyrics_view.rs:82` | Stack Crossfade, Default | **Hintergrund** (Ladezustand) | Standard (T8) |
| `browse/browse_chooser.rs:28` | Stack SlideLeftRight, Default | Nutzer | Standard (T8) |
| `preferences/preference_rhythmbox.rs:323` | Stack SlideLeft, Default | Nutzer | Standard (T8) |

**Ohne Transition (harte Schnitte):** innerer Tracks/Albums/Artists-Stack
(`library_shell.rs:43-66` — `gtk4::Stack::new()`, nichts gesetzt),
StatusPage⇄Liste-Stack der Track-Tabelle, linke Sidebar
(`window_navigation.rs:10-27`: `sidebar_page.set_visible(false)` — GTK animiert
`visible` nie; Header-Toggle-Pfad Z.72-85 rein strukturell via
`set_collapsed`/`set_show_content`), Mini⇄Voll
(`minimal_view.rs`: `ToolbarView::set_content`-Tausch).

**Adw-intern, keine Dauer-API in den Bindings (systemgegeben, G3):**
OverlaySplitView (rechte Spalte, `information_column.rs:20-28`),
NavigationSplitView, ToastOverlay, Banner, Dialog, Popover/PopoverMenu — dazu
die einzigen echten `AdwNavigationView`-Pushes der App: die
Settings-/Preferences-Subseiten (`preferences.rs:363`,
`preference_sync.rs:150`, `preferences_window.rs:305`). Animation jeweils in C
hart codiert; respektiert `gtk-enable-animations` nativ.

### 2.2 Adw-Animationen und handgebaute Ticks

| Stelle | Was | Dauer | Gating heute | Ziel |
|---|---|---|---|---|
| `player_bar/player_bar.rs:281-339` | Track-Crossfade Cover+Titel+Artist, ein Slot | 125+125 ms | ja (Z.282) | Standard-Hälften 2×125 (T7) |
| `player_bar/player_bar.rs:371-397` | Play/Pause-Icon-Crossfade, ein Slot | **60+60 ms** (Doc-Kommentar behauptet 120) | ja (Z.372) | Micro-Hälften 2×75 (T7, G2i) |
| `compact/compact_player.rs:403-428` | Mini-Player Titel/Artist-Crossfade | 125+125 ms (`CROSSFADE_HALF_MS`) | ja (Z.133) | Standard-Hälften (T7) |
| `style/cover_accent.rs:317-343` | Akzent-Crossfade, **globaler Ein-Slot** `CURRENT_ANIMATION` | 400 ms | ja (Z.322) | Ambient (T7, G2ii) |
| `sidebar/sidebar_device_card.rs:339-372` | Tick: Progress-Interpolation, handgebautes ease-out-cubic | 150 ms | ja (Z.342) | Micro + zentraler Gate (Phase 2, T5) |
| `player_bar/waveform_seek.rs:398ff` | Tick: Peaks-Build-up mit Pro-Balken-Stagger | 300 ms (`BUILD_DURATION_S`), Stagger 2 ms | ja (Z.314) | Ambient (T7; Stagger bleibt Implementierungsdetail) |
| `player_bar/waveform_seek.rs:398ff` | Tick: Positions-Glättung (velocity-basiert) | kontinuierlich | **nein** | Gate: Position hart setzen (T7, G8) |
| `scan/scan_progress.rs:298ff` | `ProgressBar::pulse()` alle 100 ms (`PULSE_INTERVAL`) | Dauerloop während Scan | **nein** | Gate: Timer startet nicht (Phase 2, T5, G8) |

`AdwAnimation::skip()` wird **nirgends** aufgerufen; neue Animationen ersetzen
den alten Slot-Handle stillschweigend (die alte Adw-Animation läuft dabei
weiter, da Adw sie während `play()` selbst referenziert) → MOT-6, T7.

### 2.3 CSS (inline)

- Zentrales Token `style/tokens.rs:61`:
  `TRANSITION = "150ms cubic-bezier(0.16, 1, 0.3, 1)"` — in ~10 CSS-Sections
  verwendet (Hover/Focus app-weit). Bleibt 150 ms = Micro; konsumiert künftig
  `motion::MICRO_MS` (T8).
- Press-Scale: `transform 120ms ease-out`, `:active scale(0.94)`
  (`player_bar_layout.rs:273-278`) → Micro-Kaskade via `tokens.rs` (T7).
- Zwei `@keyframes`-Dauerloops, beide nur während Wiedergabe:
  `reprise-eq` 1100 ms infinite (Now-Playing-Zeile; Datei ist
  **`ui/eq_bars.rs`**) und `mini-eq-bar` 650 ms infinite alternate
  (`player_bar_layout.rs:305-309`) — benannte MOT-5-Ausnahme (G7), Dauern
  bleiben (Dauerloops sind keine Transition-Tokens).
- **Unverifiziert:** ob GTKs CSS-Transitions/`@keyframes`
  `gtk-enable-animations` respektieren → Pflicht-Verifikation **T-V in T2**
  (Phase 1, damit ein negativer Befund VOR Phase 2 zurück ans Grilling geht).

### 2.4 Tragende Befunde

1. **Der Bestand ist eine 150-ms-Welt** — Migration auf Standard 250 ist ein
   bewusster, app-weiter Tempo-Change (G2, Abnahme-Hinweis).
2. **Spatial hat heute null Konsumenten** (kein Album-Detail-Push; NavigationView
   der Hauptansicht statisch; Mini⇄Voll unanimierter Content-Tausch) → G4.
3. **Zentrale Gating-API existiert:** libadwaita 0.9 (Feature `v1_9`,
   `Cargo.toml`) bindet `AnimationExt::set_follow_enable_animations_setting`
   (Default der Property laut Adw-Doku **false** — deshalb gaten heute alle
   sechs Stellen von Hand; T2 verifiziert das im Test).
4. **Lint auf `Duration::from_millis` wäre wirkungslos** (59 Falschpositive,
   echte Dauern sind rohe `u32`) → G6-Zuschnitt.
5. **Parallel-Lage (verifiziert, Stand 2026-07-17):** siehe Sperrliste in
   Abschnitt 4.1 — Grundlage des Zwei-Phasen-Schnitts (G-Seq).

---

## 3. Finaler Sektionstext für `docs/ux-rules.md` (Phase 2, T1)

Alle Regeln starten `[geplant]`; Flips nur in den benannten Task-Commits.
Ebenen-Tags: `[gtk]`, wo ein Widget-Zustand headless prüfbar ist (Vorbild
`sidebar_device_card.rs:579ff` mit `set_gtk_enable_animations`); `[manuell]`
nur, wo ehrlich nichts Mechanisches greift (MOT-4-Sichtprüfung).

```markdown
## O. Motion & Transitions

<!-- Sektionsbuchstabe: M (Tooltips) ist auf main vergeben; N ist durch
     feature/context-menu-unification („N. Track-Kontextmenü") beansprucht.
     Motion nimmt daher O; die Buchstabenlage wurde beim Einfügen dieser
     Sektion gegen den main-Stand verifiziert. -->

Motion illustriert, sie informiert nie exklusiv: jede Transition bestätigt
eine Zustandsänderung, die auch ohne sie vollständig sichtbar wäre —
`gtk-enable-animations=false` ist der Beweis (MOT-7). Animationen folgen
direkten Nutzeraktionen; Hintergrundprozesse schalten hart oder faden an
Ort und Stelle (MOT-2, die Motion-Lesart von P-4).

- **MOT-1** [geplant] [gtk] — Vier Tokens, keine freien Zahlen: jede von
  Reprise selbst konfigurierte Animation nutzt eines von vier Tokens aus
  `ui/motion.rs`: **Micro** 150 ms ease-out für Control-Zustand
  (Icon-Wechsel Play⇄Pause, Hover-Pills, Chips, Rating, Press-Scale;
  Icon-Crossfades laufen als zwei Micro-Hälften à 75 ms) · **Standard**
  250 ms ease-out-cubic für Flächen (Sidebar-/Panel-Reveal, Toast rein,
  Card-Collapse, Crossfades Cover/StatusPage⇄Liste) · **Ambient** 400 ms
  ease-out-cubic für atmosphärische, nicht-interaktive Übergänge
  (Akzentfarben-Crossfade) · **Spatial** = AdwSpringAnimation mit
  Adw-Default-Springparametern für gerichtete Navigation, im Code angelegt
  ab dem ersten gerichteten Navigationsfall. Ease-in nur für Verlassendes
  (Toast raus, Micro-Dauer); linear nur für echte Fortschrittsbalken.
  Adw-interne Widget-Animationen ohne Dauer-API (OverlaySplitView,
  NavigationSplitView, ToastOverlay, Banner, Dialog, Popover — z. B. die
  Push/Pop-Slides der Einstellungs-Unterseiten) gelten als systemgegeben
  und sind vom Token-Zwang ausgenommen.
  <!-- Flip-Kriterium MOT-1: alle Call-Sites aus dem Audit-Inventar des
       Motion-Plans konsumieren Tokens; scripts/check-motion-tokens.sh ist
       scharf und ohne Restlisten-Allowlist. -->
- **MOT-2** [geplant] [gtk] — Nutzeraktion animiert, Hintergrund nie:
  Transitions folgen direkten Nutzeraktionen. Scan/Watcher/Mount/Sync
  schalten hart bzw. faden ohne Verschiebung (P-4 in Motion-Sprache).
  Ausnahme: die vom Nutzer gestartete Prozess-Karte darf füllen/pulsieren.
- **MOT-3** [geplant] [gtk] — Symmetrie: gleiches Muster = gleiches Widget
  + gleiches Token. Konkret: die linke Bibliotheks-Sidebar nutzt exakt das
  Widget und damit exakt die Transition der rechten Info-Spalte
  (`adw::OverlaySplitView`, Position Start — Auslöser dieser Sektion); der
  innere Tracks/Albums/Artists-Wechsel und die StatusPage⇄Liste-Stacks
  crossfaden mit dem Standard-Token wie der äußere
  Library/Stats/Device-Stack.
- **MOT-4** [geplant] [manuell] — Listen bewegen sich nicht: kein
  Stagger/Fade-in pro Row (windowed Model, 200er-Fenster, Bibliotheken
  jenseits 1 600 Rows). Erlaubt: ein Crossfade der gesamten Fläche beim
  View-Wechsel; benannte Ausnahme: die Queue darf DnD-Drop und
  Einzel-Remove animieren.
  <!-- Die Queue-Ausnahme ist erlaubend, nicht fordernd; ihre Umsetzung
       liegt im Folge-Branch und blockiert den MOT-4-Flip nicht. -->
- **MOT-5** [geplant] [gtk] — Player-Leiste lebt, aber leise: Play→Pause =
  Icon-Crossfade (zwei Micro-Hälften) + Scale-Puls (1.0→0.92→1.0, Micro);
  Track-Wechsel = Cover/Titel-Crossfade; die Waveform crossfadet zum neuen
  Track statt auf 0 zu fahren; Pause entsättigt den Waveform-Fill leicht
  (zur Draw-Zeit), Play kehrt es um — die Akzent-Pipeline (`cover_accent`)
  bleibt unberührt. Die EQ-Indikatoren (Trackliste, Mini-Player) laufen
  nur während aktiver Wiedergabe; die Idle-Leiste ist statisch — kein
  Dauerloop ohne Wiedergabe.
  <!-- Flip-Kriterium MOT-5 (Folge-Branch, Muster TIP-1b/2b): Scale-Puls,
       Waveform-Crossfade und Pause-Entsättigung sind implementiert und
       per [gtk]-Test gedeckt. Icon- und Track-Crossfade existieren
       bereits tokenisiert; sie allein flippen die Regel nicht. -->
- **MOT-6** [geplant] [gtk] — Nichts blockiert: das Modell ändert sich am
  Frame 0, die Animation illustriert nur. Eine zweite Aktion während einer
  laufenden Animation springt per `AdwAnimation::skip()` zum Endzustand und
  startet dann die neue; Animations-Slots (Track-Crossfade, Icon-Crossfade,
  Akzent-Fade) rufen `skip()` statt den alten Handle stillschweigend zu
  droppen.
- **MOT-7** [geplant] [gtk] — `gtk-enable-animations=false` gewinnt
  ausnahmslos: jedes Token degradiert zentral in `ui/motion.rs` zum
  Hard-Switch (`follow-enable-animations-setting` bzw. der zentrale
  Gate-Helper `animations_enabled()`), nicht an 30 Call-Sites. Gilt auch
  für eigene Tick-Callbacks (Waveform-Positions-Glättung: Position hart
  setzen; Progress-Interpolation) und Pulse-Timer. `gtk::Spinner` und
  GTK-interne CSS-Mechanik sind Systemverhalten und werden nicht gegated.
```

---

## 4. Taskplan — zwei Phasen

> Format wie `docs/superpowers/plans/2026-07-17-ux-tooltips-taskplan.md`:
> ein Commit pro Task, TDD wo ein Test benannt ist, Flips nur im benannten
> Task-Commit, Commit-Titel trägt die Regel-ID.
>
> **HARTE ANWEISUNG AN DIE CODE-PHASE: Implementiere NUR Phase 1
> (T0, T2, T7, T8, T3 in dieser Reihenfolge/Wellung), dann STOPP.**
> Phase 2 (T1, T4, T5, T6, T9, T10) ist gesperrt, bis der Gate-Check in
> 4.3 `GATE OPEN` liefert UND main integriert UND der Ownership-Scan
> wiederholt wurde. Kein Task aus Phase 2 darf „vorgezogen" werden — auch
> nicht teilweise, auch nicht als „nur der Test".

### 4.1 Globale Constraints

- Gates vor JEDEM Commit: `cargo fmt --check` ·
  `cargo clippy --locked --all-targets --workspace -- -D warnings` ·
  `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace` ·
  `scripts/check-ux-traceability.sh` · `scripts/check-architecture.sh`.
- Display-Tests headless über `scripts/check-display-tests.sh`
  (Ignore-Marker `requires a display; run via xvfb-run` zählt als Coverage).
- Statuswechsel `[geplant]→[aktiv]` NUR im Task-Commit, der es sagt — und
  ausschließlich in Phase 2 (vorher existiert die Sektion nicht).
- Neue User-facing Strings (voraussichtlich keine) über `N_!`-Kataloge +
  `de.po` im selben Commit.
- Commits englisch, kein Attribution-Footer, kein Push.
- **Sperrliste Phase 1 (verifizierter Überlapp, Stand 2026-07-17 — NICHT
  anfassen):**
  - feat/missing-import-errors (OFFEN, Worktree `../reprise-issues`) besitzt:
    `crates/reprise-gnome/src/ui/scan/**` (komplett),
    `crates/reprise-gnome/src/ui/sidebar/**` (weitgehend, inkl.
    `sidebar_presentation.rs`), `ui/style/mod.rs`,
    `ui/window/library_shell.rs`, `ui/window/window.rs`,
    `ui/window/window_runtime_wiring.rs`.
  - feature/context-menu-unification (OFFEN) besitzt:
    `ui/window/window_action_wiring.rs` und `docs/ux-rules.md` (Sektion N).
  - `window_navigation.rs` und `information_column.rs` liegen im
    T4-Umbaugebiet und bleiben in Phase 1 ebenfalls unangetastet.
  - Gemergt und damit FREI: feat/tag-editor-rework,
    feat/queue-playlist-improvements, feat/context-menu-improvements,
    feat/global-search-rework (`ui/browse/**` ist frei).
- **`docs/ux-rules.md` wird in diesem Branch ausschließlich in Phase 2
  angefasst** (G1) — einer integriert, nie zwei parallel.

### 4.2 Phase 1 — sofort, konfliktfrei

Parallelisierungs-Karte:

```text
T0 → T2 → { T7 ∥ T8 } → T3 → STOPP (Phasen-Gate 4.3)
```

Datei-Ownership Welle Phase 1 (disjunkt, T7/T8 parallel durch getrennte
Agenten; `ui/motion.rs` ist nach T2 für beide **read-only**):

| Task | Exklusive Dateien |
|---|---|
| T2 | `ui/motion.rs` (neu), `ui/mod.rs` (Modul-Registrierung) |
| T7 | `ui/player_bar/**`, `ui/compact/**`, `ui/style/cover_accent.rs` |
| T8 | `ui/style/tokens.rs`, `ui/eq_bars.rs`, `ui/browse/browse_chooser.rs`, `ui/preferences/preference_rhythmbox.rs`, `ui/info_panel/info_panel.rs`, `ui/lyrics/lyrics_view.rs` |
| T3 | `scripts/check-motion-tokens.sh` (neu), CI-/Gate-Einbindung |

#### Task 0: Plan committen

Diese Datei (`docs/plans/ux-rules-motion.md`) committen.

```bash
git commit -m "docs: add motion rules plan (grilled 2026-07-17)"
```

#### Task 2: `ui/motion.rs` anlegen (+ Pflicht-Verifikation T-V)

- Token-Konstanten: `MICRO_MS = 150`, `STANDARD_MS = 250`,
  `AMBIENT_MS = 400`; Crossfade-Hälften-Helfer (`half(token)`); Easings als
  Konstanten für `adw::Easing` (Micro: EaseOutQuad/„ease-out", Standard und
  Ambient: EaseOutCubic) und für CSS-Strings. **Kein Spatial-Code** (G4).
- `timed(widget, from, to, token, target) -> adw::TimedAnimation` — setzt
  `set_follow_enable_animations_setting(true)`.
- Gate-Helper `animations_enabled() -> bool` (eine Stelle statt sechs
  Formulierungen) für Ticks/Timer.
- Slot-Helfer `replace_animation(slot, new)` mit `skip()` auf dem Vorgänger
  (für MOT-6, T7).
- Unit-Tests für Token-Werte; `[gtk]`-Test, dass `timed(…)` die
  Follow-Property setzt (verifiziert zugleich, dass der Adw-Default false
  ist — Befund 2.4.3).
- **T-V (Pflicht):** headless/xvfb-Spike, ob CSS-Transitions und
  `@keyframes` unter `gtk-enable-animations=false` stillstehen. Ergebnis
  als Kommentar in `ui/motion.rs` dokumentieren. **Fällt T-V negativ aus
  (CSS ignoriert das Setting): Befund festhalten, Task normal abschließen,
  aber im Abschlussbericht der code-Phase als GRILLING-RÜCKLÄUFER melden —
  kein stiller Beschluss über einen CSS-Degradationspfad.**

```bash
git commit -m "feat(motion): add motion tokens and central animation helpers"
```

#### Task 7: Player/Compact/Akzent — Token-Migration + MOT-6-Skip-Semantik

- Track-Crossfade (`player_bar.rs`) → Standard-Hälften (2×125, unverändertes
  Tempo); Icon-Crossfade → **Micro-Hälften 2×75** (G2i — gewollt langsamer
  als die bisherigen 2×60; Doc-Kommentar korrigieren);
  `compact_player.rs::CROSSFADE_HALF_MS` → Standard-Hälfte;
  Hover-Overlay-Revealer (`compact_player_layouts.rs`) → Micro;
  Akzent-Crossfade (`cover_accent.rs`) → **Ambient** (G2ii);
  Waveform-Peaks-Build-up (`waveform_seek.rs`) → Ambient (atmosphärisch,
  nicht-interaktiv; Pro-Balken-Stagger bleibt Implementierungsdetail).
- Alle Adw-Animationen dieser Dateien über `motion::timed()` bzw. Follow-
  Property statt Hand-Gating; Waveform-Positions-Glättung gaten
  (`animations_enabled()` false → Position hart setzen) (G8, Player-Seite).
- MOT-6-Skip-Semantik: die Ein-Slot-Systeme (Track-Crossfade,
  Icon-Crossfade, Akzent-Fade, compact_player) auf
  `motion::replace_animation()` mit `skip()` umstellen.
- TDD: `[gtk]`-Test `mot_6_…` (zweiter `set_track` während laufender
  Animation → Endzustand des ersten sofort sichtbar, kein Zwischenzustand;
  Modellzustand ändert sich vor Animationsende); `[gtk]`-Test
  `set_gtk_enable_animations(false)` → sofortiger Endzustand inkl.
  Positions-Glättung (Vorbild `sidebar_device_card.rs:579ff`).
- KEIN Flip (Sektion existiert noch nicht; Flips in Phase 2, T9).

```bash
git commit -m "feat(motion): MOT-6 skip semantics and player-side token migration"
```

#### Task 8: Token-Migration der konfliktfreien Rest-Call-Sites

- `tokens.rs`: `TRANSITION` konsumiert `motion::MICRO_MS` (Hover/Focus/Press:
  Dauer bleibt 150 ms; Easing folgt dem Micro-Token (ease-out), abgenommen im
  Review 2026-07-18).
- `info_panel.rs:148`, `lyrics_view.rs:82`, `browse_chooser.rs:28`,
  `preference_rhythmbox.rs:323`: explizite `set_transition_duration` mit
  Standard-Token (bisher Default ~200 → bewusst 250).
- `eq_bars.rs`: Kommentar auf die MOT-5-EQ-Ausnahme (G7) — Loop-Dauern
  (1100/650 ms) sind keine Transition-Tokens und bleiben.
- `[gtk]`-Test `mot_1_…` (Stichproben-Widgets tragen Token-Dauer).
- KEIN Flip.

```bash
git commit -m "feat(motion): migrate conflict-free call sites to motion tokens"
```

#### Task 3: Lint `scripts/check-motion-tokens.sh`

Stil von `check-ux-traceability.sh`. Verbietet außerhalb
`ui/motion.rs`/`ui/style/tokens.rs` (G6):
1. `TimedAnimation::new(…)` mit Integer-Literal als Dauer,
2. `set_transition_duration(`/`.transition_duration(` mit Integer-Literal.

Ticks/CSS bleiben Review-Sache. **Phase-2-Restliste:** die noch nicht
migrierten Dateien (`ui/sidebar/sidebar_device_card.rs`,
`ui/scan/scan_progress.rs`, `ui/window/window.rs`) stehen in einer im
Script dokumentierten Allowlist mit Kommentar `# Phase 2 — wird in T4/T5
migriert und aus der Allowlist entfernt`. In die lokale Gate-Batterie der
Folge-Tasks aufnehmen; Einbindung an derselben Stelle, an der
`check-ux-traceability.sh` hängt.

```bash
git commit -m "ci: add motion-token lint (MOT-1 gate)"
```

**→ ENDE PHASE 1. STOPP. Kein weiterer Task ohne Gate-Check 4.3.**

### 4.3 Phasen-Gate (hartes Kriterium)

Phase 2 darf erst beginnen, wenn ALLE drei Bedingungen erfüllt sind:

1. **Merge-Bedingung (maschinell):** das folgende Kommando gibt
   `GATE OPEN` aus:

   ```bash
   git fetch origin --prune
   gate_open=1
   for b in feat/missing-import-errors feature/context-menu-unification; do
     if git rev-parse --verify --quiet "origin/$b" >/dev/null; then
       git merge-base --is-ancestor "origin/$b" origin/main \
         || { echo "GATE CLOSED: $b not merged into origin/main"; gate_open=0; }
     fi
   done
   # Zusatzprobe: Sektion N muss real auf main sein (deckt Branch-Löschung
   # ohne Merge ab)
   git grep -q "## N. Track-Kontextmenü" origin/main -- docs/ux-rules.md \
     || { echo "GATE CLOSED: section N not on origin/main"; gate_open=0; }
   [ "$gate_open" = "1" ] && echo "GATE OPEN"
   ```

2. **Integration:** `origin/main` ist in `feat/ux-rules-motion` gemergt,
   Konflikte aufgelöst, volle Gate-Batterie grün.
3. **Ownership-Scan wiederholt:** die Sperrlisten-Prüfung aus 4.1 wird
   gegen den DANN aktuellen Stand wiederholt (offene Branches/Worktrees
   auflisten, Überlapp mit den T4/T5/T6-Dateien prüfen). Neue Überlappungen
   werden Handoffs, keine Edits — im Zweifel zurück an den Nutzer.

### 4.4 Phase 2 — nach dem Gate

Parallelisierungs-Karte:

```text
GATE → T1 → { T4 ∥ (T5 → T6) } → T9 → T10
```

Datei-Ownership Welle Phase 2 (disjunkt, T4 und T5/T6 parallel durch
getrennte Agenten; `ui/motion.rs` read-only):

| Task | Exklusive Dateien |
|---|---|
| T1 | `docs/ux-rules.md` |
| T4 | `ui/window/library_shell.rs`, `ui/window/window_navigation.rs`, `ui/window/window.rs`, `ui/sidebar/sidebar_presentation.rs`, `ui/info_panel/information_column.rs` |
| T5+T6 | `ui/scan/scan_progress.rs`, `ui/sidebar/sidebar_device_card.rs` (+ zugehörige Tests) |
| T9 | `RELEASING.md`, `docs/ux-rules.md` (Flips), `scripts/check-motion-tokens.sh` (Restlisten-Check) |
| T10 | `.superpowers/sdd/progress.md` |

#### Task 1: Sektion O in `docs/ux-rules.md`

Sektionstext aus Abschnitt 3 wörtlich hinter die letzte Sektion (erwartet:
N), vor den Schluss-Absatz. Buchstabenlage gegen den realen main-Stand
verifizieren (sollte N nicht die letzte Sektion sein: Kommentar anpassen,
Buchstabe bleibt O nur, wenn O frei ist — sonst nächster freier Buchstabe
plus Anpassung ALLER MOT-Verweise; das ist eine mechanische Umbenennung,
kein neuer Beschluss). `scripts/check-ux-traceability.sh` grün (alle MOT
`[geplant]`, Präfix wird dynamisch erkannt — kein Script-Update nötig).

```bash
git commit -m "docs: add ux-rules section O (MOT motion rules, planned)"
```

#### Task 4: MOT-3 — Sidebar-Symmetrie (der Auslöser-Bug) → Flip MOT-3

TDD: `[gtk]`-Displaytest `mot_3_…` zuerst (beide Seitenflächen nutzen
dasselbe Widget-Muster; Toggle-Roundtrip bei breitem UND schmalem Fenster).
Umbau laut G5: linke Sidebar auf `adw::OverlaySplitView` (Position Start);
`apply_sidebar_visibility`, `manually_hidden`-Logik und
Breakpoint(<800 px)-Zusammenspiel aus `window_navigation.rs` portieren;
bestehende Tests um Breakpoint-Fälle erweitern (Fokus-Klau beim Verstecken
prüfen). Innerer Tracks/Albums/Artists-Stack + StatusPage⇄Liste-Stacks
erhalten den Standard-Crossfade (MOT-3 Satz 2); äußerer Stack
(`window.rs:368`) migriert 150→Standard und fällt aus der Lint-Allowlist.
Flip MOT-3 in diesem Commit.

```bash
git commit -m "feat(ui): MOT-3 — left sidebar slides like the right panel"
```

#### Task 5: MOT-7 — Gating-Zentralisierung abschließen → Flip MOT-7

Restliche manuelle `is_gtk_enable_animations`-Stellen
(`sidebar_device_card.rs`) auf motion.rs-Helfer; Progress-Interpolation
(Tick) über `animations_enabled()` gaten (false → Wert hart setzen);
Scan-Pulse-Timer gaten (false → Timer startet nicht, Balken bleibt
statisch-determiniert); Transitions dieser Dateien 150→Standard-Token,
Dateien aus der Lint-Allowlist entfernen. TDD nach Vorbild
`sidebar_device_card.rs:579ff` (`set_gtk_enable_animations(false)` →
Endzustand sofort, Pulse-Timer startet nicht). Flip MOT-7 in diesem Commit
(Player-Seite ist seit T7/Phase 1 gedeckt, T-V-Befund liegt aus T2 vor).

```bash
git commit -m "feat(motion): MOT-7 — centralize enable-animations gating"
```

#### Task 6: MOT-2 — Hintergrundflächen härten → Flip MOT-2

Scan-Karte/Device-Card/Lyrics-Ladezustand: Crossfade ohne Verschiebung
festschreiben (heute schon Crossfade — Test fixiert das), keine
Slide-Transitions an Hintergrund-Auslösern. `[gtk]`-Test `mot_2_…`:
Transition-Typen der Hintergrund-Widgets sind Crossfade/None;
Scan-Karten-Reveal verschiebt keine Nachbarn (Allocation-Vergleich).
Gleicher Agent wie T5 (Datei-Ownership überlappt), eigener Commit.
Flip MOT-2 in diesem Commit.

```bash
git commit -m "feat(ui): MOT-2 — background surfaces fade in place, never slide"
```

#### Task 9: RELEASING.md + verbleibende Flips (MOT-1, MOT-4, MOT-6)

- MOT-4 `[manuell]`: Bullet in „## Manual GNOME QA" (englisch, IDs
  wörtlich, Muster RELEASING.md:174-184) → Flip MOT-4.
- Flip MOT-1: Lint-Allowlist ist leer (T4/T5 erledigt), alle
  Inventar-Call-Sites konsumieren Tokens — Commit-Body verweist auf die
  Phase-1-Commits (T7/T8).
- Flip MOT-6: Implementierung liegt in T7 (Phase 1) — Commit-Body verweist
  darauf.
- MOT-5 bleibt `[geplant]` (G9, Folge-Branch) — Flip-Kriterium-Kommentar
  steht bereits im Sektionstext.

```bash
git commit -m "docs: MOT-1/4/6 — manual QA entry and flip completed motion rules to active"
```

#### Task 10: Abschluss

Volle Gate-Batterie inkl. Display-Tests; Ledger-Eintrag
`.superpowers/sdd/progress.md` (nennt den Folge-Branch-Scope: MOT-5-
Neuverhalten + Queue-Drop-Animation); Handoff-Notizen für in Phase 2 neu
entdeckte Überlappungen; Merge-Titel dokumentiert die finale
Sektionsbuchstaben-Lage.

---

## 5. Teststrategie je Regel

| Regel | Ebene | Mechanisch prüfbar (headless, xvfb) | Ehrlich manuell |
|---|---|---|---|
| MOT-1 | [gtk] | Token-Konstanten (Unit); Stichproben: Widget-`transition_duration()` == Token; Lint T3 flankiert (zählt nicht als Coverage — der regelbenannte Test schon) | Gefühlte Stimmigkeit der Dauern (bewusster 150→250-Change!) |
| MOT-2 | [gtk] | Transition-Typ der Hintergrund-Widgets (Crossfade/None, nie Slide); Scan-Karten-Reveal verschiebt keine Nachbarn (Allocation-Vergleich) | „Nichts animiert unter dem Cursor" in echt |
| MOT-3 | [gtk] | Beide Seitenflächen: gleicher Widget-Typ (`OverlaySplitView`), gleiche Konfiguration; Toggle-Roundtrip bei breitem/schmalem Fenster; Breakpoint-Fälle | Optischer Gleichlauf der Slides (Adw-intern) |
| MOT-4 | [manuell] | — (Negativregel über nicht-existenten Code) | Reload/Scroll/DnD einer 10k-Liste: keine Zeilenbewegung |
| MOT-5 | [gtk] | **Dieser Branch:** Icon-Name nach Skip korrekt; bestehende Crossfades nutzen Micro-/Standard-Hälften. **Folge-Branch:** Puls-/Waveform-Crossfade-/Entsättigungs-Tests (`set_gtk_enable_animations(false)` → sofortiger Endzustand) | Wirkung von Puls/Entsättigung (Folge-Branch) |
| MOT-6 | [gtk] | Zweiter `set_track`/`set_state` während laufender Animation → Endzustand Frame-genau; Modellzustand (`playback_state`) ändert sich vor Animationsende | Reaktionsgefühl unter schnellem Klicken |
| MOT-7 | [gtk] | `set_gtk_enable_animations(false)`: Follow-Property gesetzt, Tick-Callbacks setzen hart, Pulse-Timer startet nicht (Vorbild `sidebar_device_card.rs:579ff`) | CSS-Verhalten auf realen Desktops (nach T-V) |

---

## 6. Abnahme-Checkliste

**Phase 1:**

- [ ] `ui/motion.rs` existiert (Micro 150 / Standard 250 / Ambient 400,
      kein Spatial-Code); `timed()` setzt die Follow-Property (Test).
- [ ] T-V-Befund als Kommentar in `motion.rs` dokumentiert; bei negativem
      Befund als Grilling-Rückläufer gemeldet.
- [ ] Player: Icon-Crossfade 2×75, Track-Crossfade Standard-Hälften,
      Akzentfade Ambient; Skip-Semantik aktiv (MOT-6-Test grün).
- [ ] Konfliktfreie Call-Sites tokenisiert; `tokens::TRANSITION` konsumiert
      `MICRO_MS`.
- [ ] `check-motion-tokens.sh` grün mit dokumentierter Phase-2-Restliste.
- [ ] KEINE Datei der Sperrliste angefasst; `docs/ux-rules.md` unberührt;
      keine Flips.

**Phase 2:**

- [ ] Gate-Check 4.3 lief und lieferte `GATE OPEN`; main integriert;
      Ownership-Scan wiederholt.
- [ ] Beide Sidebars gleiten identisch (`OverlaySplitView` beidseitig);
      Breakpoint-/`manually_hidden`-Fälle getestet.
- [ ] Innerer Library-Stack und StatusPage⇄Liste crossfaden wie der äußere
      Stack (MOT-3 Satz 2 — bisher harte Schnitte).
- [ ] Kein Hintergrundereignis (Scan/Sync/Mount/Lyrics-Laden) animiert
      etwas unter dem Cursor oder verschiebt Layout.
- [ ] `gtk-enable-animations=false` → komplett instant, inkl.
      Positions-Glättung und Pulse.
- [ ] Keine Animation verzögert je eine Aktion; zweite Aktion skippt.
- [ ] Lint-Allowlist leer; keine Integer-Dauern an Animations-APIs
      außerhalb `motion.rs`/`tokens.rs`.
- [ ] `check-ux-traceability.sh` grün; MOT-1/2/3/4/6/7 `[aktiv]`, MOT-5
      `[geplant]` mit Flip-Kriterium; MOT-4-Bullet wörtlich in
      RELEASING.md.
- [ ] Ledger-Eintrag; Merge-Titel nennt die finale Sektionsbuchstaben-Lage.
- [ ] **Bewusster Tempo-Change abgesegnet:** die App fühlt sich durch
      150→250 auf Flächen messbar anders an — explizit abnehmen, nicht als
      Nebenwirkung entdecken (G2).

**Folge-Branch (nicht hier):** Play/Pause-Scale-Puls, Waveform-Crossfade
beim Trackwechsel, Pause-Entsättigung des Waveform-Fills (→ Flip MOT-5),
Queue-Drop/Remove-Animation (MOT-4-Ausnahme nutzen).

---

## 7. Operative Notizen

- Worktree `.worktrees/transitions` existiert (Branch `feat/transitions`,
  sauber), hängt hinter origin/main. Vor Start: `git pull --ff-only`, dann
  `git branch -m feat/transitions feat/ux-rules-motion`. **Kein Build beim
  Worktree-Setup.**
- Pipeline: dieser Plan → code-Phase headless im Worktree (NUR Phase 1) →
  Gate-Check → separater Phase-2-Lauf.
- `docs/ux-rules.md` bleibt bis zum Gate unangetastet (G1); Phase 1
  (T2, T7, T8, T3) ist davon vollständig unabhängig.

---

## 8. Offene Risiken

1. **CSS-Transitions vs. enable-animations unverifiziert** (T-V in T2).
   Fällt der Test negativ aus, braucht MOT-7 einen CSS-Degradationspfad
   (animationslose Token-Variante laden) — das ist ein Grilling-Rückläufer,
   kein stiller Beschluss; Aufwand heute nicht geschätzt.
2. **OverlaySplitView-Animation nicht tokenisierbar** — die prominenteste
   Transition der App bleibt per G3-Wortlaut außerhalb des Token-Systems;
   Symmetrie (MOT-3) hängt daran, dass BEIDE Seiten dieselbe Adw-interne
   Animation nutzen (deshalb G5: exakt dasselbe Widget).
3. **Sidebar-Umbau (G5) berührt Fokus-/Breakpoint-Sonderfälle**
   (`manually_hidden`, Fokus-Klau, <800 px) — Regressionsfläche; die
   bestehenden Tests werden in T4 gezielt um Breakpoint-Fälle erweitert.
4. **Default von `follow-enable-animations-setting`** ist laut Adw-Doku
   false — T2 verifiziert das im Test, statt sich auf die Doku zu
   verlassen.
5. **Fühlbarkeit der Token-Migration** (G2): 150→250 ms auf allen Flächen
   ist ein bewusster, app-weiter Tempo-Change — Abnahme segnet das
   explizit ab (Checkliste Phase 2).
6. **Phase-2-Wartezeit:** bis feat/missing-import-errors und
   feature/context-menu-unification auf main sind, kann sich das
   Territorium erneut verschieben (weitere Branches, umbenannte Dateien,
   neue Sektionen in ux-rules.md). Deshalb wiederholt der Gate-Check 4.3
   den Ownership-Scan zwingend; neue Überlappungen werden Handoffs, keine
   Edits. Auch die Buchstabenlage O wird in T1 gegen den realen main-Stand
   verifiziert.
