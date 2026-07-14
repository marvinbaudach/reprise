# Refactoring-Plan (vor dem Design-Feintuning)

**Datum:** 2026-07-14 · **Status:** geplant, nicht begonnen
**Kontext:** Die nächste große Phase ist ein Design-Feintuning der GNOME-UI (User-getrieben).
Dieses Refactoring bereitet das vor und räumt reale Verstöße gegen die Dateigrößen-Regel
(800 Zeilen max) auf. **Keine Verhaltensänderungen.** Jede Task einzeln unter Lock, mit
grünen Gates (fmt, clippy, Workspace-Tests, Display-Tests via `scripts/check-display-tests.sh`)
committen.

## R1 — Zentrales Style-Modul `ui/style/` (Enabler — zuerst, vor dem Design-Feintuning)

**Befund:** 8 Dateien installieren jeweils eigene `CssProvider` mit Inline-CSS-Strings:
`browse_bar`, `column_layout_editor`, `list_density`, `lyrics_view`, `preference_choice_cards`,
`rating`, `track_list_header_style`, `track_list_row_interaction`. Mehrere `install`-Funktionen
(z. B. `track_list_header_style::install`) fügen pro Widget-Konstruktion einen display-globalen
Provider hinzu — bei Rekonstruktion akkumulieren Provider (heute harmlos, aber unsauber).

**Ziel:**

- Ein `ui/style`-Modul mit genau **einem** Provider, einmal beim App-Start installiert,
  zusammengesetzt aus Feature-Sektionen (eine Datei pro Feature-CSS bleibt möglich).
- Design-Tokens als benannte Konstanten an einer Stelle (Foreground-Alphas wie `0.78`,
  Abstände, Radii, Akzentverwendung) — das Design-Feintuning ändert dann nur dieses Modul
  statt acht verstreute Strings.
- Bestehende CSS-Unit-Tests migrieren; zusätzlich ein Test gegen Doppel-Installation.

**Akzeptanz:** keine `CssProvider`-Nutzung außerhalb `ui/style`; Gates grün; optische Parität
(ptr-e2e-Screenshots unverändert).

## R2 — Dateigrößen-Verstöße in `reprise-core`

**Befund:** `queue.rs` (1223) und `library/playlists.rs` (1221) überschreiten das 800er-Limit
deutlich — in beiden ist ~60 % Testcode (`mod tests` ab Zeile 463 bzw. 546).
`library/scanner_tests.rs` (805) liegt knapp drüber.

**Ziel:**

- Tests nach dem bestehenden Muster (`scanner_tests.rs`) in Geschwister-Dateien
  `queue_tests.rs` / `playlists_tests.rs` extrahieren. Falls der Impl-Teil danach noch >400
  Zeilen hat, kohäsiv splitten (playlists: CRUD / Positions-Logik inkl. `move_position` /
  Smart-Playlist-SQL).
- `scanner_tests.rs` in zwei Szenario-Module teilen.

**Akzeptanz:** keine Datei >800 Zeilen; identische Testanzahl und -namen (kein Coverage-Verlust).

## R3 — `ui/` nach Features gruppieren

**Befund:** 136 flache Dateien mit 37 320 Zeilen in einem Verzeichnis; Feature-Zugehörigkeit
existiert nur als Namenspräfix (`track_list_*`, `preference_*`, …). Navigation und Reviews leiden.

**Ziel:** Untermodule nach Feature — `track_list/`, `compact/`, `preferences/`, `playback/`,
`sidebar/`, `info_panel/`, `device_sync/`, `scrobbling/`, `style/` (aus R1). Reine `git mv`
plus `mod.rs`-/Sichtbarkeits-Anpassungen, **keine Logikänderung**.

**Hinweis:** erzeugt breite Pfad-Konflikte — als EINE atomare Task unter Lock ausführen,
niemals parallel zu anderer Arbeit.

**Akzeptanz:** Gates grün; echte Moves (`git log --follow` funktioniert).

## R4 — Edge-tight-Dateien (Regel, kein Vorab-Split)

`track_list.rs` (796), `scrobbling.rs` (795, bereits in STATUS vermerkt), `browse_bar.rs` (792),
`track_actions.rs` (790), `track_list_context_menu.rs` (784), `sidebar.rs` (780),
`player_controller.rs` (776), `window.rs` (772): **kein** vorsorglicher Split (YAGNI) — aber der
nächste inhaltliche Edit an einer dieser Dateien muss ein kohäsives Submodul extrahieren.

## Reihenfolge

1. **R1** (Enabler) — danach kann das Design-Feintuning starten.
2. **R2** — unabhängig von der UI, gut parallel delegierbar (z. B. Codex), sobald der Lock frei ist.
3. **R3** — als atomarer Slot zwischen zwei Feature-Tasks; nicht zwingend vor dem Feintuning,
   aber vor weiterem UI-Wachstum.
4. **R4** — laufende Regel, keine eigene Task.
