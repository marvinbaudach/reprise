# Now-Playing-Panel — Taskplan (2026-07-18)

Setzt `docs/superpowers/plans/2026-07-18-npp-beschluesse.md` um (NPP-1..10 +
Beschlüsse 1–6, 9). Branch `feat/now-playing-panel`, Basis `main@fec994c`.
Der Paralleltask (Beschlüsse 7/8) läuft auf `feat/theme-surface-hierarchy` —
siehe Datei-Ownership unten.

**Arbeitsweise:** TDD wo ein Verhalten testbar ist (Red → Green), ein Commit
pro Task mit der angegebenen Message. Vor jedem Commit die Gates (unten).
Status-Flips `[geplant] → [aktiv]` im Beschlussdokument passieren **im
Implementierungs-Commit** der jeweiligen Regel (Zuordnung pro Task).

## Datei-Ownership (verbindlich, Konfliktschutz)

Dieser Branch darf **nicht anfassen**:
`crates/reprise-gnome/src/ui/style/theme.rs`,
`crates/reprise-gnome/src/ui/style/cover_accent.rs`,
`crates/reprise-gnome/src/ui/window/library_chrome.rs`,
`crates/reprise-gnome/src/ui/window/library_shell.rs`,
`docs/ux-rules.md`.
Diese Dateien gehören dem Paralleltask bzw. dem Regelwerk-Owner. Der
Akzent-Zugriff läuft ausschließlich über die benannte Farbe
`@reprise_player_accent` (wird von der Cover-Pipeline pro Track überschrieben;
ihr Fallback-Wert ist Sache des Paralleltasks).

## Ist-Zustand (verifiziert am 2026-07-18)

- Rechte Spalte: `ui/info_panel/` — `InfoPanel` (Tabs Information | Lyrics in
  einem `gtk::Stack`, `adw::HeaderBar` mit ⟳/Spinner/×,
  `StackSwitcher`), folgt der **Library-Selektion** (`set_context`), Artist
  News über `artist_news_worker.rs`. Spaltenbreite `PANEL_WIDTH = 340`
  (`information_column.rs`).
- Linke Sidebar: `sidebar_presentation.rs` `SIDEBAR_MIN_WIDTH 220 /
  SIDEBAR_MAX_WIDTH 280 / FRACTION 0.22`.
- Lyrics: `ui/lyrics/` (LyricsView, lyrics_state, player_lyrics — folgt
  bereits dem spielenden Track), CSS-Sektion `lyrics_view::css()` in
  `style/mod.rs` registriert.
- Tab-Persistenz über Neustart: `reprise_core::library::settings::{get,set}_info_panel_tab`.
- Motion: `ui/motion.rs` — `MICRO` 150 ms ease-out, `STANDARD` 250 ms
  EaseOutCubic, `animations_enabled()`, `timed(...)`; CSS-Transitions folgen
  `gtk-enable-animations=false` (T-V-Probe in `style/mod.rs`).
- Queue-Signale existieren (Sidebar refresht auf „up next changed");
  Transport: `ui/playback/up_next_transport.rs`.

## Tasks (für einen Einzel-Agenten strikt in Reihenfolge 1 → 9)

### T1 · Geometrie 240/300 (NPP-1)

- Red: bestehende Metrik-Tests anpassen — `sidebar_presentation.rs`-Test
  erwartet 240/240, `info_panel`-Tests erwarten `PANEL_WIDTH == 300`.
- Green: `SIDEBAR_MIN_WIDTH = 240.0`, `SIDEBAR_MAX_WIDTH = 240.0` (Fraction
  bleibt, wird geklemmt); `PANEL_WIDTH: i32 = 300`.
- Flip: **NPP-1 → [aktiv]** (Slide-Transition liefert `OverlaySplitView`
  bereits; im Flip-Text kurz vermerken).
- Commit: `feat(layout): fix side panels at 240 and 300 px (NPP-1)`

### T2 · Mechanischer Umzug info_panel → now_playing

Reiner Struktur-Commit, null Verhaltensänderung:

- `git mv crates/reprise-gnome/src/ui/info_panel crates/reprise-gnome/src/ui/now_playing`
- `artist_news_worker.rs` dabei nach `crates/reprise-gnome/src/ui/artist_news/`
  ausgliedern (eigenes Modul `mod.rs`; Wiederverwendung durch den
  22a-Folge-Task). Modul-Doku-Zeile: „Konsument z. Zt. das Now-Playing-Panel;
  zieht mit Frame 22a in die Artist-Detail-View um."
- Typen umbenennen: `InfoPanel` → `NowPlayingPanel`, `InformationColumn` →
  `NowPlayingColumn`; Modulpfade/`use`-Stellen nachziehen. Keine Logik anfassen.
- Commit: `refactor(now-playing): rename info_panel to now_playing (mechanical)`

### T3 · Panel folgt dem Player; Information-Tab raus (Beschlüsse 1, 9)

- Red: Tests für den neuen Kontext — Panel zeigt geladenen Track (spielend
  **und** pausiert identisch), Idle ohne geladenen Track; Struktur-Test: im
  Panel existiert keine `adw::HeaderBar`, kein ⟳/×-Button mehr.
- Green:
  - Information-Seite, HeaderBar (⟳/Spinner/×), `StackSwitcher`,
    Artist-News-Rendering, Enable-SwitchRow und den
    `REPRISE_SMOKE_ARTIST_NEWS`-Pfad aus dem Panel entfernen. Das
    `artist_news`-Modul bleibt kompilierend und getestet (Worker-Tests
    behalten), nur ohne Panel-Konsumenten.
  - Kontextquelle umhängen: statt Track-List-Selektion (`set_context` aus der
    Fensterverdrahtung) hört das Panel auf den Player (dieselbe Quelle wie
    Playerleiste/`now_playing_wiring`). Pausiert = geladen; `None` = Idle.
  - Schließen ausschließlich über den bestehenden App-Header-Toggle
    (Sichtbarkeits-Persistenz unverändert lassen).
- Commit: `feat(now-playing): follow the playing track, drop the Information tab`

### T4 · Kopf, dunkle Bühne, Pill-Toggle (NPP-2, NPP-3, Beschlüsse 4, 5)

- Red: `npp_2_no_volume_in_panel` [gtk] — Widget-Baum des Panels enthält
  keinen Volume-Regler (`gtk::VolumeButton`/`gtk::Scale`); plus Kopf-Tests:
  Cover-Größe 168, Titel-/Untertitel-Labels, Toggle mit exakt zwei Segmenten
  (Up Next, Lyrics), Idle-Zustand („Nothing playing", Platzhalter-Cover,
  kein Glow).
- Green:
  - Kopf-Widget: Cover 168 px (Radius 12, Schatten, 1 px Inset-Hairline —
    CSS), Titel 15 px bold, „Artist · Album" 12 px @ 55 %.
  - Glow: CSS-Radialverlauf über `alpha(@reprise_player_accent, 0.4)` im
    oberen Drittel des Panels, auslaufend auf die Bühnenfarbe; Idle-Klasse
    ohne Glow. (Die Cover-Pipeline überschreibt die benannte Farbe pro Track —
    kein eigener Textur-Code.)
  - Dunkle Bühne: feste neutral-dunkle Fläche als eigene CSS-Klasse des
    Panels, **nicht** über `@sidebar_bg_color` (gilt in beiden Schemes,
    Beschluss 5).
  - Pill-Toggle: zwei verlinkte Segmente als runde Pill (border-radius 99px,
    Grund weiß 6 %, aktiv weiß 14 % + bold — Muster analog
    `.reprise-view-switcher`, aber rund; kein Tab-Bar-Widget). Fußzeilen-Slot
    10.5 px @ 35 % unter dem Tab-Inhalt (Inhalt liefern T6/T7).
  - Neue CSS-Sektion `now_playing::css()` in `style/mod.rs` registrieren
    (+ Marker im `app_css_contains_every_feature_section`-Test).
- Flip: **NPP-2 → [aktiv]**, **NPP-3 → [aktiv]**.
- Commit: `feat(now-playing): 21a head with accent glow, dark stage, pill toggle (NPP-2/3)`

### T5 · Tab-Gedächtnis session-only (NPP-4)

- Red: `npp_4_tab_persists_in_session` [gtk] — Tab wechseln, Panel-Zustand
  innerhalb der Session neu aufbauen → Tab bleibt; frischer Session-Zustand
  (simulierter Neustart) → Up Next.
- Green: `{get,set}_info_panel_tab` aus `reprise-core` samt Aufrufern
  entfernen; Tab-Gedächtnis nur im Prozess (Session), Start-Default Up Next.
- Flip: **NPP-4 → [aktiv]**.
- Commit: `feat(now-playing): session-only tab memory, restart lands on Up Next (NPP-4)`

### T6 · Up-Next-Tab (Beschluss 3)

- Red: reine Präsentations-Fns testen — „kommende Tracks" = Queue-Einträge
  nach dem aktuellen Index (Grenzfälle: leere Queue, aktueller = letzter);
  Fußzeilen-Format `„{n} tracks · {Restdauer}"`; [gtk] Klick auf Row springt
  zu genau diesem Queue-Eintrag (bestehende Sprung-API des Transports; PLAY-5:
  explizite Nutzeraktion, Historie bleibt).
- Green: Up-Next-Liste (Rows: Cover 32 px, Titel 13.5 px, Artist dim), Klick =
  Sprung, Empty-State „Queue is empty", Fußzeile; aktualisiert sich über das
  bestehende Queue-Änderungs-Signal. Kein Reorder, kein Entfernen, kein
  Kontextmenü im Panel.
- Commit: `feat(now-playing): Up Next tab with jump-to-entry (Beschluss 3)`

### T7 · Lyrics-Styling + Fallbacks (NPP-5, NPP-9, Beschluss 9)

- Red: Stufen-Logik als pure Fn (Abstand → Alpha 100/45/32/28 %),
  Instrumental-Gap-Logik (>10 s → aktiv dimmt auf 60 %), Fußzeilen-Inhalt je
  Quelle („synced · LRCLIB" / „lyrics · tags"); [gtk] Fehlerzustand zeigt
  Inline-Retry.
- Green: Zeilen zentriert 13 px mit ~13 px Gap, aktive Zeile 15 px bold weiß
  + Unterstrich 26×2.5 px in `@reprise_player_accent` (Element der aktiven
  Zeile, faded mit — kein wanderndes Extra-Widget); unsynced statisch 65 %
  ohne Highlight/Auto-Scroll; „No lyrics found"-Leerzustand ohne Such-CTA;
  Labels nicht selektierbar. Alles in der bestehenden
  `lyrics_view::css()`-Sektion.
- Flip: **NPP-5 → [aktiv]**, **NPP-9 → [aktiv]**.
- Commit: `feat(lyrics): 21a line hierarchy, fallbacks, inline retry (NPP-5/9)`

### T8 · Auto-Scroll, User-Pause, Klick = Seek (NPP-6, NPP-7, NPP-8)

- Red: `npp_7_user_scroll_pauses_autoscroll` [gtk] — Timer injizierbar
  (mockbare Uhr/Handle statt hartem `glib::timeout`): User-Scroll pausiert
  4 s, jedes weitere User-Event resettet, Rück-Glide wird von User-Scroll
  abgebrochen, programmatischer Scroll resettet nie;
  `npp_8_line_click_seeks` [gtk] — Klick seekt zum Zeilen-Timestamp und
  scrollt sofort (kein 4-s-Timer); externer Seek (Waveform) ebenso sofort.
- Green: Zeilenwechsel-Fade Micro-Token (CSS-Transition reicht — folgt dem
  Animations-Setting); Auto-Scroll zentriert die aktive Zeile über eine
  animierte `vadjustment`-Fahrt (`motion::timed`, STANDARD, EaseOutCubic;
  `animations_enabled() == false` → Sprung). Scroll-State-Machine
  (Auto | UserPause | Rückkehr) als eigene, ohne Display testbare Einheit;
  Unterscheidung User- vs. programmatischer Scroll über die eigenen
  Event-Controller (Scroll/Drag), nie über Adjustment-Deltas. Hover weiß
  65 % + Pointer.
- Flip: **NPP-6 → [aktiv]**, **NPP-7 → [aktiv]**, **NPP-8 → [aktiv]**.
- Commit: `feat(lyrics): centered auto-scroll with user pause and click-to-seek (NPP-6/7/8)`

### T9 · Trackwechsel-Crossfade + Abnahme (NPP-10)

- Red: [gtk] Trackwechsel ersetzt Kopf + Tab-Inhalt in einem gemeinsamen
  Crossfade-Container (ein Fade, kein Slide); `animations_enabled() == false`
  → Hard-Switch; Lyrics stehen danach auf Zeile 0 zentriert.
- Green: gemeinsamer Crossfade (STANDARD, MOT-5) über Kopf+Inhalt; Glow
  wechselt im selben Fade (die benannte Farbe wechselt mit dem
  Cover-Akzent-Provider).
- `RELEASING.md`: manuelle Abnahmepunkte per Regel-ID ergänzen (NPP-5/6-Optik:
  Stufen, Unterstrich, ruhiges Gleiten; NPP-3-Glow; Panel-Slide beim
  Ein-/Ausklappen).
- Flip: **NPP-10 → [aktiv]**.
- Commit: `feat(now-playing): shared track-change crossfade and release checklist (NPP-10)`

## Gates vor jedem Commit

- `cargo fmt --check`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
- `scripts/check-ux-traceability.sh`
- `scripts/check-architecture.sh`
- Display-Tests (`#[ignore]`-markiert) des berührten Bereichs via
  `xvfb-run -a` ausführen (siehe `TESTING.md`).

## Abnahme (manuell, nach Abschluss)

Song mit LRC abspielen → Zeilen gleiten mittig durch; manuell scrollen →
4 s Ruhe → Rückkehr; Zeile klicken → Seek + sofortiger Scroll; Track wechseln
→ ein gemeinsamer Crossfade; unsynced → statisch; Panel ein-/ausklappen
gleitet wie links; Idle zeigt „Nothing playing"; Up-Next-Klick startet den
Eintrag.

## Parallel Execution Map

Für Multi-Agent-Setups: T1 ∥ (T2→T3→T4) — danach T5 ∥ T6 ∥ T7 — dann T8 → T9.
Ein Einzel-Agent ignoriert die Wellen und arbeitet strikt 1 → 9.
