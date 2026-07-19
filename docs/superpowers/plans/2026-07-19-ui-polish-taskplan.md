# UI-Politur — Taskplan Batch A (2026-07-19)

Setzt `docs/superpowers/plans/2026-07-18-ui-polish-beschluesse.md` um, **Batch A**
(Flächen, Kontrast, kleine Verhaltensfixes). Branch
`feat/sidebar-visual-improvements`, Basis `main@b0965905`.

Regeln gehen als **Sektion U** nach `docs/ux-rules.md`. `feat/network-opt-in`
fügt parallel Sektion T an; **append-only ans Dateiende nach dem, was beim
Rebase vorliegt** — nicht T vorwegnehmen, nicht umsortieren.

**Nicht in diesem Batch** (Batch B, architektonisch): QUE-7 virtueller
Kontext-Tail, QUE-8 DnD, NAV-5/NAV-10 mit Badge-Extraktion, NPP-11
ViewSwitcher.

## Ist-Zustand (auditiert, mit Zeilenangaben)

- **Flächenhierarchie existiert in der Palette, ist aber nicht verdrahtet.**
  `style/theme.rs:81-170` definiert je Theme `window_bg`, `view_bg`,
  `sidebar_bg`, `headerbar_bg`, `card_bg`. `theme.rs:193` emittiert
  `@define-color sidebar_bg_color`. **Es gibt keinen einzigen Aufrufer** von
  `@sidebar_bg_color` außerhalb `theme.rs`.
- **Das Now-Playing-Panel opted sich aus dem Themensystem aus.**
  `style/tokens.rs:104` `NOW_PLAYING_STAGE_BG = "#17191c"` ist ein fixer Hex.
  `now_playing_tests.rs:90-91` zementiert das und assertet ausdrücklich, dass
  `@sidebar_bg_color` **nicht** vorkommt. Folge: Bei PerpetualRain liegt das
  Panel auf `#17191c` ≈ `window_bg` `#16181b`, also **dunkler** als die
  Tabelle (`view_bg` `#1b1e22`) — es liest sich als Loch statt als erhobene
  Fläche. Bei MutedBloom (warmes `#1a1518`) bleibt es kaltgrau.
- **Die Statuszeile ist keine Leiste.** `track_list/track_content.rs:10-16`
  baut ein `gtk4::Overlay` und hängt das Label per `add_overlay` über die
  Trackliste. Kein Hintergrund, kein Container, **kein reservierter Platz** —
  die unterste Trackzeile ist dauerhaft halb verdeckt. Styling:
  `status_bar.rs:56-57` `.dim-label` + `.caption`.
- **Ctrl+F togglet nicht.** `shortcuts.rs:192` ruft `set_search_mode(true)` —
  öffnet immer, schließt nie.
- **Lyrics-Zentrierung ohne Klemmung.** NPP-6 ist aktiv; am Songanfang bleibt
  die obere Panelhälfte leer.
- **Scroll-Sprung bei Tabellen-Aktivierung.** `track_list_model.rs:380`
  `invalidate_window_at` feuert `items_changed(position, 1, 1)` und erzeugt das
  fokussierte Zeilen-Widget neu; GTKs Fokus-Wiederherstellung scrollt dann
  selbst. Der zentrierende Pfad löst das durch synchrones Zentrieren im selben
  Frame (Kommentar `current_track_selection.rs:310`), der unterdrückte Pfad
  kehrt vorher zurück (`if suppress_scroll { return; }`) und lässt den Fokus an
  den Listenanfang fallen.
- **FMT-1 ist bereits erfüllt** — `reprise_core::format::format_thousands` ist
  eine geteilte Funktion mit fünf Aufrufern. Nichts zu tun.

## Tasks (strikt in Reihenfolge)

### T1 · Regeln anlegen (Sektion U)

- SEARCH-6, LYR-4, STYLE-2, STYLE-3, CONTRAST-1, CONTRAST-2, CONTRAST-3 als
  `[geplant]`. NAV-10, QUE-7, QUE-8, NPP-11 **ebenfalls anlegen und dauerhaft
  `[geplant]` lassen** (Batch B) mit Verweis auf das Beschlussdokument.
- FMT-1 **nicht** anlegen — bereits erfüllt, siehe Beschlussdokument.
- Commit: `docs(ux-rules): add section U — surfaces, contrast and search toggle`

### T2 · Seitenflächen an die Themenhierarchie verdrahten (STYLE-2)

- Red: `style_2_side_surfaces_follow_the_theme` [gtk] — linke Sidebar **und**
  Now-Playing-Panel tragen in **jedem** der sechs Themes `sidebar_bg` des
  jeweiligen Palettes; keine hartkodierten Hex-Flächen. Ergänzend
  `style_2_side_surfaces_sit_above_the_table` — `sidebar_bg` ist in allen drei
  Dark-Themes **heller** als `view_bg` (Kanalsumme), in den Light-Themes
  dunkler. Das ist ein Ergebnis-Test im Sinne von STYLE-1, kein Property-Test.
- Green: `NOW_PLAYING_STAGE_BG` entfällt zugunsten von `@sidebar_bg_color`;
  die linke Sidebar bekommt dieselbe Fläche. 1-px-Hairlines
  `rgba(255,255,255,0.06)` an den Innenkanten beider Flanken.
  **`now_playing_tests.rs:90-91` muss mitgezogen werden** — die dortige
  Assertion zementiert exakt den Zustand, der hier fällt.
- Flip: **STYLE-2 → [aktiv]**.
- Commit: `feat(style): wire both side surfaces to the theme hierarchy (STYLE-2)`

### T3 · Statuszeile bekommt eine Fläche (CONTRAST-2)

- Red: `contrast_2_status_bar_has_its_own_surface` [gtk] — die Statuszeile ist
  **kein** Overlay-Kind mehr, sondern eine untere Leiste mit eigener Fläche und
  Hairline; `contrast_2_status_bar_reserves_space` — die Trackliste wird nicht
  von ihr überdeckt (gemessen: Listen-Allocation endet über der Leiste).
- Green: `track_content.rs` von `gtk4::Overlay` auf eine vertikale Box
  umstellen. Fläche `@sidebar_bg_color` (dieselbe Stufe wie die Flanken),
  1-px-Hairline oben. Erst **danach** den Textton anheben.
- Flip: **CONTRAST-2 → [aktiv]**.
- Commit: `feat(status): give the list status line a real bar (CONTRAST-2)`

### T4 · Textstufen vereinheitlichen (CONTRAST-1, CONTRAST-3)

- Red: `contrast_1_secondary_text_meets_ratio` [gtk] — misst Alpha bzw.
  Named-Color **gegen die jetzt definierte Surface-Farbe**, nicht das
  Rendering; Sekundärtext erreicht ≥ 4.5:1. Fälle: Statuszeile, Spaltenköpfe,
  Sidebar-Sektionslabels, Kartenmetazeilen.
- Green: Drei Stufen (Primär ~0.95 / Sekundär ~0.7 / Hint ~0.5), wo möglich
  über Adwaita-Named-Colors statt eigener Alphas. Die Statuszeile von Hint auf
  Sekundär. **`.caption` + Sekundär zählt als Kleinschrift** und braucht
  dieselbe Prüfung wie Hint bei Normalgröße.
- Flip: **CONTRAST-1 → [aktiv]**, **CONTRAST-3 → [aktiv]**.
- Commit: `feat(style): three text levels with verified contrast (CONTRAST-1/3)`

### T5 · Ctrl+F und Lupe togglen beidseitig (SEARCH-6)

- Red: `search_6_ctrl_f_toggles_open_and_closed` — zweites Ctrl+F schließt;
  `search_6_hidden_query_survives_as_chip` — Verstecken mit Inhalt löscht die
  Query nicht, der Chip trägt sie, die Lupe bleibt `:checked`.
- Green: `shortcuts.rs:192` von `set_search_mode(true)` auf Toggle.
- Flip: **SEARCH-6 → [aktiv]**.
- Commit: `feat(search): make the magnifier and Ctrl+F toggle both ways (SEARCH-6)`

### T6 · Lyrics-Zentrierung nach oben klemmen (LYR-4)

- Red: `lyr_4_start_of_song_is_not_centered` [gtk] — solange weniger
  Kontextzeilen über der aktiven liegen als die halbe Panelhöhe fasst, sitzt
  der Block oben; ab genug Vorlauf zentriert er.
- Green: Klemmung in der Zentrierungsrechnung, kein zweiter Scrollpfad.
- Flip: **LYR-4 → [aktiv]**.
- Commit: `feat(lyrics): clamp centering at the start of a song (LYR-4)`

### T7 · Scroll-Sprung bei Tabellen-Aktivierung

- Red: `track_activation_keeps_the_viewport` [gtk] — Doppelklick auf eine
  sichtbare Zeile lässt die **Viewport-Position unverändert** (gemessen am
  `vadjustment`-Wert, nicht an „kein Scroll-Aufruf" — STYLE-1).
- Green: Im unterdrückten Pfad die Adjustment-Position vor dem Invalidieren
  sichern und im selben Frame zurückschreiben, statt früh zurückzukehren.
- Commit: `fix(track-list): keep the viewport when activating a row from the table`

## Gates vor jedem Commit

- `cargo fmt --check`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
- `scripts/check-ux-traceability.sh`
- `scripts/check-architecture.sh`
- Display-Tests **einzeln je Prozess**: `xvfb-run -a scripts/check-display-tests.sh`
- Neue UI-Strings **im selben Commit** übersetzen; `po/de.po` ohne
  unübersetzte und ohne fuzzy Einträge. Glyphen nie mit `N_!` markieren.

## Abnahme (manuell)

Panel und linke Sidebar liegen in allen sechs Themes sichtbar **über** der
Tabelle, mit Hairline an der Innenkante · Statuszeile steht auf eigener Fläche
und verdeckt keine Trackzeile mehr · Sekundärtext überall lesbar · zweites
Ctrl+F schließt die Suche, die Query überlebt als Chip · Lyrics starten oben ·
Doppelklick in der Tabelle bewegt den Viewport nicht.
