# Search-Streifen + Queue-Vereinheitlichung — Taskplan (2026-07-18)

Setzt `docs/superpowers/plans/2026-07-18-search-queue-ux-beschluesse.md` um.
Basis: `feat/search-and-new-releases` nach dem Merge von `main` (`2783fa4`).

**Reihenfolge:** Teil A (klein, abgeschlossen) vor Teil B (groß, berührt
Playerleiste, Panel und Queue-Transport).

## Ist-Zustand (auditiert, mit Zeilenangaben)

- **SearchBar**: `library_chrome.rs:56-66` — bereits zweite Top-Bar der
  `ToolbarView`, `add_top_bar(&search_bar)` nach dem Header. Schiebt den
  Content korrekt, **kein Overlay**. Kein CSS zielt heute auf sie.
  `set_top_bar_style(Flat)` (`:63`) unterdrückt ihren Hintergrund.
- **Adw.Clamp-Vorbilder**: `preferences/preference_sync.rs:123` (Clamp als
  ToolbarView-Content), `new_releases/digest.rs:44`, Helfer
  `stats/stats_view.rs:427` (`fn adw_clamp`).
- **Filterzeile**: `browse_bar.rs:210` — horizontale Box mit `.toolbar`,
  feste Höhe 34 px, erstes Kind der Track-List-Box
  (`track_list_builder.rs:80`). Sie ist **Content**, kein Geschwister der
  Bar — wird also mitgeschoben.
- **Up-Next-Panel**: `up_next_panel.rs:57` — `Stack` über `ScrolledWindow`
  über `Box` mit flachen `Button`-Zeilen. Keine Sektionen, kein Recycling.
  Datenquelle `now_playing_panel_up_next_entries()` (`:184`), **eine
  `query_track_summary` pro Zeile**, unbegrenzt.
- **Refresh**: `window.rs:444-453` — `add_on_queue_changed`, **ohne**
  Sichtbarkeits-Guard; baut jedes Mal alle Zeilen neu (`:106`).
- **Queue-ColumnView**: Sektionen via `queue_sections.rs:48` (`compose`),
  Header-Factory `:129`, Clear-Button `:167`. DnD über
  `queue_row_mapping.rs` (`classify` `:52`, `reorder_op` `:80`) und
  `queue_transport.rs:448`.
- **Playerleisten-Icon**: `player_bar_layout.rs:210`, Signal
  `player_bar.rs:591`, **einzige** Verdrahtung
  `window_runtime_wiring.rs:162-166` → `refresh_and_select(ViewSource::Queue)`.
  Doc-Kommentare `player_bar.rs:100` und `:589` sind veraltet.
- **Sprung**: `up_next_transport.rs:256` `play_up_next_at` → `take_through`
  (`up_next.rs:51`) — **verwirft alle davorliegenden Einträge**.
- **Formatierung**: `format_thousands` (`reprise-core/src/format.rs:82`)
  überall außer `strings_news.rs:46` (`up_next_footer`, `count.to_string()`).
- **Vorbild fürs Icon-Routing**: `now_playing.rs:336` `show_lyrics()` —
  ein `show_up_next()` existiert nicht.

## Teil A — Such-Streifen

### A1 · Regeln korrigieren (Sektion Q)

- SEARCH-2 neu fassen: vollbreiter Streifen bündig unter der Headerbar,
  eigene Fläche + untere Trennlinie, Entry per Clamp (~450 px) zentriert,
  Reveal schiebt den Content. **Dauer:** „bei GTK-eigenen Revealern gilt
  deren Default, sofern er dem Standard-Token entspricht" — der Test prüft
  die Existenz des Reveals, nicht die Millisekunden.
- SEARCH-3 präzisieren: `:checked` bei offener Bar **oder** aktiver Query.
- Commit: `docs(ux-rules): restate SEARCH-2/3 for the full-width search strip`

### A2 · Streifen gestalten (SEARCH-2)

- Red: `search_2_bar_reveals_flush_under_headerbar` [gtk] — die Bar ist
  Top-Bar der ToolbarView (nicht in einem `Overlay`), ihr Kind ist ein
  `adw::Clamp`, und das CSS trägt Hintergrund + untere Trennlinie.
- Green: CSS-Klasse für die Bar analog `.reprise-library-header`
  (`library_chrome.rs:163`) — Hintergrund explizit setzen, weil `Flat` ihn
  sonst schluckt; 1-px-Hairline unten. Entry in `adw::Clamp`
  (`maximum_size(450)`), Bar selbst `hexpand`.
  **Doppelte Linie vermeiden**: Der Streifen sitzt direkt über der
  `.toolbar`-Filterzeile — beide Kanten zusammen prüfen, nicht zwei
  Hairlines stapeln.
- `search_1` (`library_chrome.rs:205`) neu fassen: Kind der Bar ist jetzt der
  Clamp, das Entry sein Nachfahre.
- Flip: **SEARCH-2 → [aktiv]**.
- Commit: `feat(search): full-width search strip with a clamped entry (SEARCH-2)`

### A3 · Lupen-Zustand (SEARCH-3)

- Red: `search_3_lens_checked_when_active` [gtk] — `:checked` sowohl bei
  offener Bar als auch bei eingeklappter Bar mit nicht-leerer Query.
- Green: Toggle-Zustand an beide Bedingungen koppeln.
- Flip: **SEARCH-3 → [aktiv]**.
- Commit: `feat(search): lens carries the active state for query or open bar (SEARCH-3)`

## Teil B — Queue: ein Modell, zwei Flächen

### B1 · Regeln (Sektion J)

- QUE-1 umformulieren (zwei Flächen, unterschiedliche Tiefe), QUE-2
  (zwei Sektionen, bedingte Header), QUE-5 umformulieren (Sprung verwirft
  nichts), QUE-6 neu (gemeinsames Model, Sammelabfrage, Guard, Recycling).
  QUE-3/4 bleiben inhaltlich.
- Commit: `docs(ux-rules): one queue model, two surfaces (QUE-1..6)`

### B2 · Sprung verwirft nichts mehr (QUE-5)

- Red: `que_5_jump_keeps_preceding_manual_entries` [core] — Klick auf den
  4. manuellen Eintrag spielt ihn und lässt 1–3 stehen.
- Green: `play_up_next_at` (`up_next_transport.rs:256`) von `take_through`
  auf „nur den gewählten Eintrag entnehmen" umstellen (`up_next.rs` bekommt
  die passende Operation; `take_through` bleibt, falls anderweitig genutzt —
  sonst entfernen).
- Flip: **QUE-5 → [aktiv]**.
- Commit: `fix(queue): jumping consumes only the clicked entry (QUE-5)`

### B3 · Gemeinsames Model + Sammelabfrage + Guard (QUE-6)

- Red: `que_6_metadata_loads_in_one_query` [core] — für N Queue-IDs genau
  **eine** Abfrage (Zähler im Test-Seam); `que_6_closed_panel_does_not_render`
  [gtk] — bei geschlossenem Panel oder anderem aktiven Tab löst
  `notify_queue_changed` kein Row-Rebuild aus.
- Green:
  - Eine Metadaten-Sammelabfrage über die Queue-IDs in `reprise-core`
    (`WHERE id IN (…)`), die **beide** Flächen speisen kann.
  - Sichtbarkeits-Guard im Refresh (`window.rs:444`): Model aktualisieren,
    rendern nur wenn Panel offen **und** Tab „Up Next".
  - Nur das sichtbare Fenster laden.
- Flip: **QUE-6 → [aktiv]**.
- Commit: `perf(queue): shared model with a batched metadata query and a render guard (QUE-6)`

### B4 · Zwei Sektionen im Panel (QUE-2)

- Red: `que_2_two_sections_headers_conditional` [gtk] — Header nur bei
  nicht-leerer Sektion; leere manuelle Sektion → nur „Continuing from …";
  nie beide Header ohne Einträge.
- Green: Panel-Liste um Sektionsköpfe erweitern; Kontextlabel aus
  `play_origin` (dieselbe Quelle wie die ColumnView, `queue_sections.rs:48`) —
  **kein zweiter Pfad**. Kein Reorder, kein DnD (Beschluss 1).
- Flip: **QUE-2 → [aktiv]**.
- Commit: `feat(now-playing): two conditional sections in the Up Next tab (QUE-2)`

### B5 · Playerleisten-Icon öffnet das Panel (QUE-1)

- Red: `que_1_bar_icon_opens_same_list` [gtk] — Klick öffnet das Panel und
  schaltet auf „Up Next"; es navigiert **nicht** mehr zur ColumnView.
- Green: `show_up_next()` analog `show_lyrics()` (`now_playing.rs:336`);
  `window_runtime_wiring.rs:162` darauf umhängen. NPP-4 beachten
  (Sichtbarkeit persistiert, Tab nicht). Veraltete Doc-Kommentare in
  `player_bar.rs:100` und `:589` korrigieren.
- Flip: **QUE-1 → [aktiv]**.
- Commit: `feat(player-bar): the queue icon opens the Up Next panel (QUE-1)`

### B6 · Remove im Panel + Zahlenformat (QUE-3, QUE-4)

- Red: `que_4_footer_uses_the_shared_thousands_format` [core] — 1652 → der
  gleiche Trenner wie in der Library; `que_3_played_manual_entries_removed`
  [core] pinnt das bestehende Verhalten.
- Green: `up_next_footer` (`strings_news.rs:46`) auf `format_thousands`
  umstellen (Tests dort ziehen mit); „Remove" pro Panel-Zeile, das aus der
  Queue entfernt, nie aus der Library.
- Flip: **QUE-3 → [aktiv]**, **QUE-4 → [aktiv]**.
- Commit: `feat(now-playing): remove from queue and shared number format (QUE-3/4)`

## Gates vor jedem Commit

- `cargo fmt --check`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
- `scripts/check-ux-traceability.sh`
- `scripts/check-architecture.sh`
- Display-Tests **einzeln je Prozess**: `xvfb-run -a scripts/check-display-tests.sh`
  — mehrere GTK-Display-Tests in einem Prozess scheitern an `gtk4::init()`.

## Abnahme (manuell)

Suche per Lupe/Ctrl+F → voller Streifen bündig unter der Headerbar mit
sichtbarer Fläche und Trennlinie, Content rückt nach, Entry mittig geklammert;
Esc zweistufig; eingeklappt mit Query → Chip + akzentuierte Lupe.
„Add to queue" auf drei Tracks → sie stehen unter „Next in Queue", der Rest
unter „Continuing from …"; Klick auf den dritten spielt ihn und **lässt die
ersten beiden stehen**; Queue-Icon unten öffnet das Panel auf „Up Next";
Zahlenformat identisch zur Library; bei geschlossenem Panel kostet ein
Trackwechsel kein Row-Rendering.
