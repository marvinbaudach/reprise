---
slug: accent-color-source
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-05
---

# Akzentfarbe: App-Akzent oder Systemfarbe — nie mehr das Cover

## Ziel

Die Akzentfarbe der App richtet sich **nicht mehr nach dem Albumcover**. Statt
der cover-abgeleiteten Farbe gibt es genau zwei Quellen, zwischen denen der
Nutzer in den Einstellungen wählt:

1. **App-Akzent** (Standard) — der Logo-Teal `#4FDBD4`, einheitlich für alle
   drei Themes.
2. **Systemfarbe** — die GNOME-Akzentfarbe, die libadwaita als `@accent_color`
   / `@accent_bg_color` / `@accent_fg_color` und über
   `adw::StyleManager::accent_color_rgba()` liefert.

Die gesamte cover-getriebene Farbextraktion fällt weg: Median-Cut,
OKLCH-Clamping, der Override-CSS-Provider, die Cross-Fade-Animation, der
Generation-Guard in der Player-Verdrahtung und die Cover-Farben des
Song-Visualizers.

## Gegrillte Entscheidungen

1. **Cover-Akzent ersatzlos raus** — inklusive Visualizer und Now-Playing-Glow.
   Kein Restpfad, der noch Cover-Pixel für Farben liest.
2. **App-Akzent ist eine einzige Farbe: `#4FDBD4`** — exakt die dicke Barline
   des neuen Logos (siehe `docs/plans/logo-coral.md`). Nicht pro Theme
   verschieden. Die drei Themes (Perpetual Rain, Night Terrain, Muted Bloom)
   bleiben erhalten und unterscheiden sich weiterhin in Flächen- und
   Textfarben — der Akzent ist ab jetzt themeübergreifend gleich. Insbesondere
   verliert Muted Bloom sein Pink `#c98bd0` als Akzent.
3. **Standard ist der App-Akzent.** Eine frische Installation sieht nach
   Reprise aus; die Systemfarbe ist eine bewusste Wahl.
4. **Eine einzige Wahrheit im Code.** Die effektive Akzentfarbe wird an genau
   einer Stelle bestimmt (`style::accent`), sowohl für CSS als auch für alle
   Rust-seitigen Leser. Zwei Stellen, die dieselbe Entscheidung treffen, sind
   in diesem Projekt schon zweimal als hörbarer/sichtbarer Bug aufgeschlagen —
   dupliziere sie nicht.

## Umfang

### 1. Neue Akzentquelle: `crates/reprise-gnome/src/ui/style/accent.rs`

Neues Modul als alleinige Wahrheit über die Akzentfarbe.

```rust
/// Die Markenfarbe: die dicke Barline des Logos (docs/plans/logo-coral.md).
pub(in crate::ui) const APP_ACCENT: &str = "#4FDBD4";

/// Settings-Key, der die Wahl persistiert.
pub(in crate::ui) const ACCENT_SOURCE_SETTING_KEY: &str = "ui.accent-source";

pub(in crate::ui) enum AccentSource { App, System }
```

- `AccentSource::DEFAULT == App`; `id()`/`from_id()` mit den stabilen Keys
  `"app"` und `"system"`, analog zu `Theme::id`/`Theme::from_id` (unbekannte
  Werte → `DEFAULT`).
- `current()` liest die im Prozess gesetzte Quelle aus einem `Cell` (analog zu
  `CURRENT_THEME` in `style/mod.rs`).
- `accent_rgba() -> gtk4::gdk::RGBA` liefert die **effektive** Farbe: bei `App`
  den geparsten `APP_ACCENT`, bei `System`
  `adw::StyleManager::default().accent_color_rgba()`. Das ist der einzige
  Einstieg für Rust-seitige Leser.
- `accent_fg()` liefert die Vordergrundfarbe für Flächen im App-Akzent. Prüfe
  den Kontrast von `#04140f` (heutiger Wert) auf `#4FDBD4` und wähle
  bewusst — WCAG AA für Text ist die Latte. Bei `System` wird nichts geliefert,
  weil Adwaita `accent_fg_color` selbst passend setzt.

### 2. `crates/reprise-gnome/src/ui/style/theme.rs` — Akzent aus der Palette nehmen

- `Palette`: die Felder `accent`, `accent_fg` und `player_accent` entfallen,
  inklusive aller Werte in `palette()` und `light_palette()`.
- `theme_css(theme, is_dark)` bekommt die Akzentquelle als Parameter
  (`theme_css(theme, is_dark, source)`) und emittiert:
  - bei `AccentSource::App`:
    ```
    @define-color accent_bg_color #4FDBD4;
    @define-color accent_fg_color <accent_fg>;
    @define-color accent_color    #4FDBD4;
    @define-color reprise_player_accent @accent_color;
    ```
  - bei `AccentSource::System`: **keine** der drei `accent_*`-Definitionen
    (Adwaita liefert die Systemfarbe), nur
    `@define-color reprise_player_accent @accent_color;`.
- `@reprise_player_accent` bleibt also in beiden Fällen bestehen und zeigt auf
  `@accent_color`. Damit bleiben alle bestehenden Leser dieses Tokens
  unverändert: Player-Bar, Compact-Player, Waveform, Lyrics, Playing-Marker,
  Now-Playing-Glow.
- Modul-Doc-Kommentar und der Doc-Kommentar am ehemaligen `player_accent`-Feld
  (Verweis auf „Decision 8" und die Cover-Pipeline) werden ersetzt durch die
  Erklärung der zwei Quellen.

### 3. `crates/reprise-gnome/src/ui/style/mod.rs` — Verdrahtung

- `mod accent;`, `mod color_math;` ergänzen, `mod cover_accent;` entfernen.
- `install()`: `cover_accent::install(&display)` entfällt. Das Theme-CSS wird
  mit `AccentSource::DEFAULT` geladen.
- Neu: `set_accent_source(source: AccentSource)` — setzt das `Cell` und lädt
  das Theme-CSS neu (dieselbe Mechanik wie `set_theme`).
- `set_theme` und `reload_theme_for_appearance` reichen die aktuelle
  Akzentquelle an `theme_css` durch.
- In `install()` zusätzlich `StyleManager::connect_accent_color_notify` (bzw.
  `notify::accent-color`) verbinden: ändert der Nutzer die GNOME-Akzentfarbe
  im laufenden Betrieb **und** steht Reprise auf `System`, muss das Theme-CSS
  neu geladen und die Rust-seitigen Leser (Visualizer) aktualisiert werden.
  Bei `App` ist das Signal ein No-op.

### 4. Einstellungen — der Umschalter

`crates/reprise-gnome/src/ui/preferences/preference_appearance.rs`:

- Neue `AppearanceSection::AccentColor`, einsortiert **nach** `Theme` und
  **vor** `ColorScheme`. Der bestehende Test
  `appearance_page_lists_theme_color_scheme_then_window_decorations` wird
  entsprechend erweitert (und umbenannt).
- `accent_row(context)` als `adw::ComboRow`, exakt nach dem Muster von
  `theme_row`/`color_scheme_row`: Titel „Accent colour", Untertitel, der die
  Wahl erklärt; Einträge „App accent" (Index 0) und „System" (Index 1);
  gespeicherter Wert via `settings::get_setting(&conn, ACCENT_SOURCE_SETTING_KEY)`,
  Schreiben via `set_setting`, Live-Anwendung via `style::set_accent_source`.
  Fehler beim Persistieren werden wie beim Theme mit `tracing::warn!` geloggt,
  nicht verschluckt.
- Sichtbare Strings gehören in die Lokalisierung: lege sie im vorhandenen
  Strings-Modul an (`ui/strings_app_shell.rs` trägt bereits
  `COLOR_SCHEME`/`COLOR_SCHEME_SUBTITLE`/`SCHEME_SYSTEM`) und ziehe
  `po/POTFILES.in` sowie `po/reprise.pot` nach, falls eine neue Datei dazukommt.
  Bestehende `.po`-Übersetzungen werden **nicht** von Hand ausgefüllt.

### 5. Anwenden beim Start

`crates/reprise-gnome/src/ui/window/window.rs` (~Zeile 78–91) lädt bereits
Theme und Color-Scheme aus den Settings. Ergänze dort — vor `set_theme`, damit
das Theme-CSS nur einmal geladen wird — das Lesen der Akzentquelle und
`style::set_accent_source(source)`.

### 6. `cover_accent.rs` auflösen

Zu **löschen**: `SAMPLE_EDGE`, `CHROMA_FLOOR`/`CHROMA_CEIL`, `oklch_clamp`,
`median_cut_buckets`, `dominant_accent`, `is_usable`, `accent_css`,
`ACCENT_PROVIDER`, `CURRENT_ANIMATION`, `install`, `set_cover_accent`, `lerp`,
`theme_fallback_rgb`, `accent_during_fade`, `cross_fade_accent`,
`accent_from_cover_file`, `struct Rgb` — samt ihrer Tests.

Zu **erhalten**: die reine Farbmathematik `scale_chroma` und ihre Helfer
(`to_linear`, `from_linear`, `linear_rgb_to_oklab`, `oklab_to_linear_rgb`).
`waveform_seek.rs` skaliert damit zur Zeichenzeit die Chroma der ungespielten
Bars — das bleibt richtig, egal woher die Grundfarbe kommt. Verschiebe sie nach
`crates/reprise-gnome/src/ui/style/color_math.rs`, passe den Import in
`waveform_seek.rs` an und lösche `cover_accent.rs`. Der Test
`chroma_scaling_is_draw_local_and_leaves_provider_state_untouched` verliert
seinen Provider-Teil; der reine Chroma-Teil wandert mit.

### 7. Player-Verdrahtung — Cover-Akzent-Pfad entfernen

- `ui/playback/now_playing_wiring.rs`: `apply_cover_accent` und
  `reset_cover_accent` samt ihrer Aufrufe und dem `one_shot_task`-Spawn
  entfernen; der `Rgb`-Import entfällt.
- `ui/playback/player_controller.rs`: die Felder `cover_accent_generation` und
  `cover_accent_last`, ihre Initialisierung und der `AccentRgb`-Import
  entfallen.

### 8. Visualizer — Cover-Farben entfernen

- `crates/reprise-core/src/visuals/engine.rs`: `cover_accent2`,
  `set_cover_colors` und `clear_cover_colors` entfallen. `accent2()` liefert
  immer `hue_shift(self.accent, FALLBACK_ACCENT2_HUE_SHIFT)`. `set_accent`
  bleibt und ist ab jetzt der einzige Farb-Eingang.
- `crates/reprise-core/src/visuals/color.rs`: `secondary_accent` wird ungenutzt
  und entfällt samt Tests; `hue_shift` bleibt.
- `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs`: der Akzent kommt
  aus `style::accent::accent_rgba()` (**nicht** direkt aus dem `StyleManager`,
  siehe Entscheidung 4) und wird neu gesetzt, wenn sich die Akzentquelle oder
  die Systemfarbe ändert. Prüfe, ob `set_cover(…)` danach noch einen Zweck hat;
  wenn nicht, entfernen — inklusive des `set_cover(None)`-Aufrufs in
  `ui/now_playing/now_playing_effects.rs` und seines Kommentars.
- `ui/now_playing/surface_css.rs`: der Glow liest `@reprise_player_accent` und
  bleibt unverändert korrekt — nur der Kommentar, der von „Cover-Akzent"
  spricht, wird angepasst.

### 9. Weitere Rust-seitige Akzentleser vereinheitlichen

- `ui/track_list/match_highlight.rs:54` ruft heute direkt
  `StyleManager::default().accent_color_rgba()`. Umstellen auf
  `style::accent::accent_rgba()`, sonst bleibt die Suchtreffer-Hervorhebung bei
  `App` auf der Systemfarbe hängen.
- `ui/updates/release_cover.rs`: `fallback_accent_for_artist` ist die letzte
  Nutzerin von `accent_from_cover_file` und färbt die Platzhalterkachel einer
  noch nicht verdrahteten New-Releases-Ansicht (`#![allow(dead_code)]`). Die
  Funktion entfällt; `LazyReleaseCover` nimmt für den Hintergrund
  `style::accent::accent_rgba()` statt `DEFAULT_ACCENT`. Aufrufer und Tests
  entsprechend anpassen; wenn ein Aufrufer einen `&str` erwartet, liefere
  `#RRGGBB`.
- Grep selbst nach weiteren direkten `accent_color_rgba()`-Aufrufen und ziehe
  sie mit.

### 10. `docs/ux-rules.md` — bindendes Regelwerk nachziehen

`docs/ux-rules.md` outranked den Code (siehe `AGENTS.md`), muss also in
**denselben Commits** mitgehen. Treffer auf `origin/dev` u. a. in den Zeilen
289, 309, 1583, 1624, 1683, 1694, 2380, 2384, 2514, 2556, 2877, 3005 — betroffen
sind mindestens das MOT-5/MOT-6-Umfeld, **NPP-3** (Glow als
Cover-Akzent-Gradient), das NPP-5-Umfeld, **STYLE-3** (`[planned]`: „zwei
Akzentrollen bleiben getrennt"), das BTN-1/BTN-2- und **AC-24**-Umfeld sowie die
Visualizer-Regel bei 3005. Suche selbst nach allen Treffern von `cover accent`,
`Cover-Akzent`, `cover_accent`, `cover-accent`.

Konvention aus `AGENTS.md`:
- Ändert sich nur die **Farbquelle**, das beschriebene Verhalten aber nicht
  (Glow, aktive Lyric-Zeile, Playing-Marker, Visualizer-Leinwand): Wortlaut
  anpassen — „Cover-Akzent" → „Akzentfarbe (`@accent_color`)". Rule-ID und
  Status bleiben, der bestehende Test wird nur an der Farberwartung nachgezogen.
- Kippt eine Regel inhaltlich (STYLE-3 gilt nicht mehr, weil es keine zweite
  Akzentrolle mehr gibt): als `[replaced durch <neue ID>]` markieren, im selben
  Abschnitt die neue Regel mit der nächsten freien ID anlegen, etwaige Tests im
  selben Commit umhängen.
- Der **Umschalter ist neues user-facing Verhalten** und braucht eine eigene
  Regel im STYLE-Abschnitt: nächste freie ID, `[active]`, mit rule-named Test
  (`fn style_<n>_…`) im selben Commit. Sie hält fest: zwei Quellen, App-Akzent
  ist der Standard, die Wahl liegt in Einstellungen › Erscheinungsbild zwischen
  Theme und Farbschema, und sie wirkt sofort ohne Neustart.
- Erfinde keine weiteren `[active]`-Regeln ohne Test. Für ungedeckte Fälle:
  `[planned]`-Entwurf mit `<!-- REVIEW: Regelvorschlag -->` und im Ergebnis
  melden.

`AGENTS.md` und `docs/plans/*.md` nur dort anfassen, wo sie die
Cover-Akzent-Pipeline als bestehendes Verhalten beschreiben — keine breiten
Umformulierungen.

### 11. Tests, die mitgezogen werden müssen

Mindestens (Vollständigkeit selbst per Grep sicherstellen):
- `ui/style/theme.rs`: `theme_css_defines_core_named_colors` prüft heute
  `accent_bg_color` — ersetzen durch je einen Test pro Quelle: bei `App` sind
  `accent_color`/`accent_bg_color`/`accent_fg_color` auf `#4FDBD4` gesetzt, bei
  `System` fehlen sie **komplett**; `reprise_player_accent @accent_color` ist in
  beiden Fällen da. `distinct_themes_produce_distinct_css` muss grün bleiben
  (Flächenfarben unterscheiden die Themes weiterhin) — prüfen, nicht
  abschwächen.
- `ui/compact/compact_player.rs` (~671–674): Kommentar „cover accent"
  korrigieren; die Assertion auf `.waveform-seek { color: @reprise_player_accent; }`
  bleibt gültig.
- `ui/now_playing/now_playing_tests.rs` (~241, ~657
  `npp_3_glow_is_a_cover_accent_gradient_over_a_neutral_stage`): Test auf die
  neue Farbquelle umbenennen, Assertion auf `@reprise_player_accent` bleibt.
- `ui/now_playing/song_visualizer_tests.rs`: `set_accent`-Aufrufe bleiben; alles,
  was Cover-Farben füttert, entfällt.
- `reprise-core/src/visuals/engine.rs`: u. a.
  `secondary_accent_falls_back_to_hue_shift` — der Fallback ist jetzt der
  einzige Pfad; umformulieren statt löschen.
- `ui/preferences/preference_appearance.rs`: Sektionsreihenfolge-Test erweitern,
  plus Test für Persistenz-Roundtrip der Akzentquelle
  (`AccentSource::from_id(source.id()) == source`, unbekannte ID → `DEFAULT`).
- `scripts/check-ux-traceability.sh` muss grün bleiben: jede `[active]`-Regel
  hat weiterhin ihren rule-named Test.

## Vorgehen

Test-first, in dieser Reihenfolge, mit fokussierten Commits pro Schritt:

1. `style/accent.rs` anlegen (Enum, Konstante, Roundtrip-Tests) — reine Logik,
   noch nicht verdrahtet.
2. `theme.rs`: Tests für beide Quellen schreiben (rot), dann Palette und
   `theme_css` umbauen; `style/mod.rs` und `window.rs` nachziehen.
3. Umschalter in `preference_appearance.rs` samt Strings.
4. `cover_accent.rs` → `color_math.rs` verschieben, Cover-Pipeline löschen,
   `waveform_seek.rs` nachziehen.
5. Player-Verdrahtung (`now_playing_wiring.rs`, `player_controller.rs`)
   entschlacken.
6. Visualizer: `engine.rs`/`color.rs` entschlacken, `song_visualizer.rs` auf
   `accent_rgba()` umstellen.
7. `match_highlight.rs` und `release_cover.rs` nachziehen.
8. `docs/ux-rules.md` (+ berührte Docs) nachziehen, Tests umbenennen/anpassen.

## Verifikation

- `cargo fmt --all` und `cargo clippy --workspace --all-targets -- -D warnings`
  grün. Insbesondere: **keine** `dead_code`-Warnungen und keine verwaisten
  `use`-Zeilen nach dem Löschen.
- `cargo test --workspace` grün. Die Display-Tests sind bekannt flaky im Rudel —
  betroffene Display-Tests einzeln nachfahren
  (`xvfb-run -a cargo test … -- --exact --test-threads=1`) und getrennt
  ausweisen; nichts als grün melden, was nur „nicht gelaufen" ist.
- `scripts/check-ux-traceability.sh` grün.
- Grep über den Worktree liefert keine lebenden Treffer mehr für
  `cover_accent`, `accent_from_cover_file`, `set_cover_colors`,
  `secondary_accent` (Historie in `docs/plans/` ausgenommen) und keinen direkten
  `accent_color_rgba()`-Aufruf außerhalb von `style/accent.rs`.

## Nicht-Ziele

- Kein Farbwähler mit freier Farbe — nur die zwei Quellen.
- Keine Änderung an den Flächen-/Textfarben der drei Themes.
- `scale_chroma` und die Waveform-Chroma-Abstufung bleiben im Verhalten
  unverändert.
- Die Coral-Farbe des Logos (`#FF6F5E`) wird **nicht** zur Akzentfarbe.
