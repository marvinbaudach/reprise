# Suchleiste + New Releases — Taskplan (2026-07-18)

Setzt `docs/superpowers/plans/2026-07-18-search-nr-beschluesse.md` um.
Branch `feat/search-and-new-releases`, Basis `main@e0493d0`.

**Reihenfolge ist bindend: Teil A komplett, dann Teil B.** Beide fassen
dieselbe Headerbar an; B baut auf der von A geschaffenen Platzsituation auf.

**Arbeitsweise:** TDD wo ein Verhalten testbar ist (Red → Green), ein Commit pro
Task mit der angegebenen Message. Vor jedem Commit die Gates. Regeln werden in
`docs/ux-rules.md` als **Sektion Q (SEARCH)** und **Sektion R (NR)** angelegt —
beim Anlegen `[geplant]`, Flip auf `[aktiv]` im Implementierungs-Commit der
jeweiligen Regel.

## Ist-Zustand (auditiert am 2026-07-18, mit Zeilenangaben)

- **Suchfeld**: `gtk4::SearchEntry`, lokal in `window::build` gebaut
  (`ui/window/window.rs:118`), gestylt in `library_chrome.rs:54`
  (`SEARCH_WIDTH = 300`), **gepackt** erst in
  `window_runtime_wiring.rs:462` (`header.pack_end`). Kein Struct besitzt es.
- **Query-Wahrheit**: `TrackList::shared.filter: RefCell<String>`. Pfad:
  `search-changed` → `view_session::wire_search` (200 ms Debounce,
  `view_session.rs:78`) → `TrackList::set_filter` →
  `set_filter_and_reload` (`track_list_reload.rs:182`).
  **Zweiter Konsument**: Albums-Grid, undebounced
  (`window_runtime_wiring.rs:515`).
- **Chip**: FIL-1a ist `[aktiv]` und implementiert — `rebuild_chips`
  (`browse_bar.rs:430`) baut den Such-Chip bereits; `chip_labels`
  (`browse_bar.rs:145`) ist `#[cfg(test)]`-only.
- **Ctrl+F**: `shortcuts.rs:186` (`win.focus-search`), macht heute nur
  `grab_focus` + `select_region`.
- **Esc**: `shortcuts.rs:214`, hängt am `stop-search`-Signal des Entry —
  feuert **nur bei fokussiertem Entry**. Zweistufig via
  `escape_action_for` (`shortcuts.rs:114`).
- **Kein `GtkSearchBar` im Repo.** Kein Vorbild, kein
  `set_key_capture_widget` irgendwo.
- **MusicBrainz**: `reprise-core/src/musicbrainz.rs` — 1 req/s prozessweit
  (`MIN_REQUEST_INTERVAL`, `static LAST_REQUEST`), User-Agent, Fixture-Seam
  (`REPRISE_MUSICBRAINZ_FIXTURE_DIR`, versteht heute nur zwei URL-Formen).
- **artist_news**: `reprise-core/src/artist_news.rs` (Typen, Fenster, Filter,
  JSON-Datei-Cache) + `ui/artist_news/artist_news_worker.rs` (Thread,
  `async_channel`, Generation-Token). **Einziger lebender Konsument ist der
  Preferences-Schalter** (`preferences.rs:684`).
- **Keine Artist-MBID** in irgendeiner Tabelle. Nur eine *Release*-MBID wird
  transient aus Tags gelesen (`cover.rs:48`).
- **Badge-Muster**: `count_new_*(conn, last_viewed)` in
  `queries/issues.rs:481`, `last_viewed_*`-Keys in `library/settings.rs:629`,
  gestempelt in `view_session.rs:247`. Regel FB-4 (`ux-rules.md:281`).
- **Module**: `reprise-core/src/modules.rs` — `ALL_MODULES` (`:52`), Key
  `module.<id>.enabled`, **keine Migration nötig** für ein neues Modul.
  Live-Toggle ist heute per String-ID sondergelockt (`preferences.rs:684`).
- **Migrationen**: `db.rs`, Kopf bei v11, `PRAGMA user_version`, DDL + Bump in
  einer Transaktion, ausgelieferte Migrationen nie editieren (`:289`).

## Teil A — Suchleiste wird Lupe

### A1 · Regeln anlegen (Sektion Q)

- `docs/ux-rules.md`: Sektion **Q. Suche** mit SEARCH-1…5, alle `[geplant]`,
  Level-Tags `[gtk]`. FIL-4 auf `[ersetzt durch SEARCH-3]` setzen; NAV-6
  vorerst `[geplant]` lassen.
- Die beiden FIL-4-Tests (`library_chrome.rs`, `interactions.rs`) **in
  diesem Commit** entfernen — Traceability verbietet Tests auf ersetzte IDs.
- Gate `scripts/check-ux-traceability.sh` muss grün sein.
- Commit: `docs(ux-rules): add section Q — search (SEARCH-1..5), retire FIL-4`

### A2 · SearchBar ersetzt das Dauerfeld (SEARCH-1, SEARCH-2)

- Red: `search_1_idle_is_icon_not_field` [gtk] — in der Headerbar liegt ein
  `ToggleButton` mit Lupen-Icon und **kein** `SearchEntry`;
  `search_2_ctrl_f_reveals_and_focuses` [gtk] — Ctrl+F setzt
  `search_mode_enabled` und der Entry hat Fokus.
- Green:
  - `gtk::SearchBar` als **zweite Top-Bar** der bestehenden
    `adw::ToolbarView` (`library_chrome.rs:43`), Entry hineinverlagert.
    `set_key_capture_widget(&window)` damit Tippen die Bar öffnet.
    Reveal-Transition = Standard-Token 250 ms (MOT-1/3).
  - Lupen-`ToggleButton` (`system-search-symbolic`, Klassen `flat` +
    `reprise-panel-toggle`) per `pack_end`; bidirektional an
    `search-mode-enabled` gebunden.
  - `shortcuts.rs:186` erweitern: Ctrl+F **enthüllt** die Bar und fokussiert,
    statt nur zu fokussieren.
  - `SEARCH_WIDTH`, `SEARCH_ACTIVE_CLASS` und die `.reprise-search-active`-
    CSS entfernen.
- Flip: **SEARCH-1 → [aktiv]**, **SEARCH-2 → [aktiv]**.
- Commit: `feat(search): replace the persistent entry with a revealed search bar (SEARCH-1/2)`

### A3 · Sichtbarkeit der aktiven Query (SEARCH-3, SEARCH-5)

- Red: `search_3_active_query_shows_chip_when_collapsed` [gtk] — Query
  gesetzt, Bar eingeklappt → Chip vorhanden **und** Lupe `is_active()`.
- Green: Lupe trägt bei nicht-leerer Query den `:checked`-Zustand
  (**Beschluss 5: Toggle-Zustand, kein Punkt** — der Badge-Punkt bleibt der
  Bitte-Rolle vorbehalten, FB-4 ohne Ausnahme). Der Chip existiert bereits
  (FIL-1a); nur sicherstellen, dass er beim Einklappen bleibt.
- Flip: **SEARCH-3 → [aktiv]**, **SEARCH-5 → [aktiv]**.
- Commit: `feat(search): keep an active query visible via chip and toggle state (SEARCH-3/5)`

### A4 · Esc zweistufig an der Bar (SEARCH-4, NAV-6)

- Red: `search_4_escape_clears_then_collapses` [gtk] und ein
  `nav_6_`-benannter Test. Fälle: Bar offen mit Text → Esc leert, Bar bleibt
  offen und fokussiert; Bar offen und leer → Esc klappt ein; Bar mit Inhalt
  klappt **nie** zu, ohne dass der Chip die Query trägt.
- Green: Escape-Handling von `stop-search` (feuert nur bei fokussiertem
  Entry) auf die SearchBar heben; `escape_action_for` um die Stufe
  „collapse" erweitern.
- Flip: **SEARCH-4 → [aktiv]**, **NAV-6 → [aktiv]**.
- Commit: `feat(search): two-stage escape on the search bar (SEARCH-4, NAV-6)`

### A5 · Pill nach links, Testverträge nachziehen (Beschluss 7)

- Green: View-Switcher per `pack_start` neben den Sidebar-Toggle; mittigen
  Titel entfernen; `CenteringPolicy::Loose` samt Notlösungs-Kommentar
  streichen (`library_chrome.rs:54`).
- **Brechende Verträge — alle drei anfassen:**
  - `library_chrome.rs:179` `header_spans_the_navigation_with_loose_centering`
    prüft `width_request() == 300` und `Loose` → neu fassen.
  - `scripts/cua-e2e/run.sh:206` und `scripts/tests/cua-e2e.sh:121` tippen in
    das AT-SPI-Element „Search all fields" → vorher die Lupe aktivieren.
  - `scripts/ptr-e2e/geometry.sh:24` rechnet Offsets vom rechten Rand
    (`INFO_TOGGLE_FROM_RIGHT=222` usw.) → neu vermessen, da 300 px wegfallen.
  - `help.rs:121` prüft die exakte Accelerator-Liste → Liste bleibt gleich,
    aber Escape-Beschreibung prüfen.
- Commit: `feat(header): move the view switcher left and drop loose centering`

## Teil B — New Releases

### B1 · Regeln anlegen (Sektion R)

- `docs/ux-rules.md`: Sektion **R. New Releases** mit NR-1…7, alle
  `[geplant]`. „Remind me" bleibt dauerhaft `[geplant]` (Beschluss: braucht
  einen Scheduler, den es nicht gibt).
- Commit: `docs(ux-rules): add section R — new releases (NR-1..7)`

### B2 · Schema + Artist-MBIDs (Beschluss 2)

- Red: Migrationstests (frische DB und v-alt-DB laufen dieselbe Sequenz);
  Tag-Extraktion liest `ItemKey::MusicBrainzArtistId`.
- Green: **eine** neue Migration (nächste freie Version):
  - Spalte `artist_mbid` + Negativ-Markierung (nicht gefunden / mehrdeutig),
    damit nicht ewig neu gesucht wird.
  - Tabelle für Releases: `release_group_mbid` (PK), Artist, Titel, Typ,
    `first_release_date`, `fetched_at`, `seen_at` (NULL = ungesehen),
    `hidden`, Fallback-Akzent als Hex.
  - **Bestandsschutz** (Beschluss 8): für bereits existierende Datenbanken
    `module.cover_download.enabled = true` und
    `module.artist_portraits.enabled = true` schreiben.
  - Scanner liest die Artist-MBID aus Tags mit.
- Commit: `feat(db): schema for new releases, artist MBIDs and module grandfathering`

### B3 · Fetch-Pipeline, artist_news wird Query-Schicht (NR-1, Beschluss 1)

- Red: `nr_1_fetch_respects_rate_limit` [core] mit Mock-Clock;
  Filtertests für Beschluss 4 (Album/EP ≥ heute−90 d; Single **nur**
  zukünftig; unvollständiges Datum gilt **nicht** als zukünftig;
  Sekundärtypen raus; Kappung 5/Artist).
- Green:
  - Fetch-Queue über `musicbrainz.rs` (Rate-Limit ist dort schon prozessweit
    — **nicht neu bauen**), Top-Artists nach Play-Count zuerst, Rest über
    Tage verteilt.
  - MBID-Auflösung: Tag-Wert bevorzugen, sonst Namenssuche, Ergebnis
    (auch negatives) persistieren.
  - `artist_news.rs` auf die neue Tabelle umbauen: Fetch/Parse/Filter bleiben,
    der JSON-Datei-Cache entfällt. Fixture-Seam um die neue URL-Form
    erweitern.
- Flip: **NR-1 → [aktiv]**.
- Commit: `feat(new-releases): unified MusicBrainz fetch into the releases table (NR-1)`

### B4 · Bilder und Fallback-Kachel (NR-2)

- Red: `nr_2_missing_cover_uses_fallback_tile` — 404 von CAA führt zur
  Fallback-Kachel, nie zu einem Loch oder Spinner.
- Green: neuer URL-Builder `/release-group/{mbid}/front-250` (vorhanden ist
  nur `/release/{mbid}/front`). 404 ist der **Normalfall** bei kommenden
  Releases → Fallback-Kachel aus dem Akzent des meistgespielten Albums des
  Artists (`accent_from_cover_file`) plus Initialen; Akzent **beim Anlegen**
  des Eintrags berechnen und als Hex speichern. Laden lazy, erst wenn die
  Karte sichtbar wird.
- Flip: **NR-2 → [aktiv]**.
- Commit: `feat(new-releases): release-group covers with an accent fallback tile (NR-2)`

### B5 · Badge, Popover, Gesehen-Zustand (NR-3, NR-5, NR-6)

- Red: `nr_3_opening_marks_seen_clears_badge`,
  `nr_3_seen_item_not_rebadged` — Badge zählt `seen_at IS NULL`; Öffnen
  stempelt alle gelisteten; ein bereits gesehenes Release badgt **nie**
  wieder, ein neuer Fund schon (Episoden-Logik analog FB-4).
- Green: ✦-`MenuButton` in der Headerbar, **nur sichtbar wenn Einträge
  existieren** (wie ISSUES); Popover transient, `popover_lifecycle` nutzen;
  Fuß mit „Fetch now" (Spinner ersetzt ⟳) und „updated"-Alter; offline/Fehler
  → letzter Cache mit Alter, **nie** ein Fehlerbanner, dezenter Inline-Hinweis
  im Fuß.
- Flip: **NR-3 → [aktiv]**, **NR-5 → [aktiv]**, **NR-6 → [aktiv]**.
- Commit: `feat(new-releases): headerbar button, popover and seen-state (NR-3/5/6)`

### B6 · Digest-View und Hidden (NR-4, Beschluss 3)

- Red: „See all" erscheint, sobald `total > sichtbar` **oder**
  Hidden-Einträge existieren; „Hide" setzt `hidden = 1`; die View zeigt die
  Fußzeile „N hidden · Show".
- Green: Digest-View als `NavPlace` mit Back/Forward (NAV-2/9), **ohne**
  Sidebar-Eintrag, erreichbar nur über „See all" — wie die Artist-Detail-View.
  Hero-Cover 72 px; Popover-Hero 56 px, Rows 34 px; alle mit Inset-Hairline,
  Fallback-Kachel in denselben Maßen.
- Flip: **NR-4 → [aktiv]** (Teilregel „Remind me" bleibt `[geplant]`).
- Commit: `feat(new-releases): digest view with hidden entries (NR-4)`

### B7 · Nur das New-Releases-Modul (NR-7, Beschluss 6)

> **PLANÄNDERUNG 2026-07-18, nach Beginn des Laufs — bindend.** Dieser Task
> war ursprünglich breiter (drei Module) und hatte einen Nachfolger B8
> (Entdeckungszeile). Beides ist **hierher nicht mehr zuständig**: Die
> allgemeine Opt-in-Regel für Netz-Features, die Module `cover_download`,
> `artist_portraits` und `online_lyrics`, die Lyrics-Zustände und das
> **gesamte** Entdeckungssystem (Evidenz-Trigger, Kombinationsregel) gehören
> in den Folge-Branch `feat/network-opt-in`. Hier entsteht **nur** das
> Modul, das NR-7 selbst braucht. **B8 ist gestrichen — nicht bauen.**

- Red: `new_releases` hat `default_enabled: false`; ✦ erscheint nicht,
  solange das Modul aus ist.
- Green: `artist_news` → `new_releases` umwidmen (Plugins-Seite,
  Privacy-Untertitel „contacts MusicBrainz"), ComboRow „nur Top-Artists /
  alle". Die String-ID-Sonderlocke für den Live-Toggle
  (`preferences.rs:684`) mitziehen. **`cover_download` und
  `artist_portraits` hier NICHT anfassen** — sie bleiben vorerst ungated und
  werden im Folge-Branch gegated.
- Der Bestandsschutz-Teil der Migration aus B2 entfällt damit ebenfalls: B2
  schreibt **keine** `module.*.enabled`-Werte mehr, das macht der
  Folge-Branch mit seinen Evidenzkriterien.
- Flip: **NR-7 → [aktiv]**.
- Commit: `feat(preferences): opt-in module for new releases (NR-7)`

### B8 · GESTRICHEN

Die Entdeckungszeile wandert vollständig in `feat/network-opt-in`, wo sie mit
Evidenz-Triggern und der Kombinationsregel zusammen entsteht. Würde sie hier
in einfacher Form gebaut, müsste der Folge-Branch sie sofort umschreiben.
**Diesen Task überspringen; DISCOVER-1 nicht in Sektion R aufnehmen.**

### B8 · Entdeckungszeile (DISCOVER-1, Beschluss 9)

- Red: `discover_1_hint_shows_once_and_never_again` — die Zeile erscheint nur
  wenn (a) das Modul aus ist, (b) der erste Scan durch ist, (c) Artists mit
  MB-MBID existieren und (d) das Settings-Flag noch nicht gesetzt ist; nach
  Wegklicken **oder** einmaligem Anzeigen wird das Flag gesetzt und sie
  erscheint nie wieder.
- Green: dezente Zeile am Kopf der Artists-Ansicht mit Link auf die
  Plugins-Seite (`PreferencesContext::present_page`) und ×. **Genau eine
  solche Zeile im ganzen Produkt** — die Implementierung wird so angelegt,
  dass eine spätere gemeinsame „Netz-Features aktivieren?"-Zeile für
  Cover/Portraits denselben Slot benutzt und nicht daneben erscheint.
- Regel **DISCOVER-1** in Sektion R ergänzen, Flip auf `[aktiv]`.
- Commit: `feat(discovery): one-time hint for the opt-in network features (DISCOVER-1)`

## Gates vor jedem Commit

- `cargo fmt --check`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
- `scripts/check-ux-traceability.sh`
- `scripts/check-architecture.sh` (jede Datei < 800 Zeilen, Orchestratoren < 600)
- Display-Tests (`#[ignore]`) via `xvfb-run -a`, sofern die Sandbox es zulässt;
  sonst als „pending display verification" im Ledger vermerken.

## Abnahme (manuell)

Suche per Lupe/Ctrl+F öffnen → Bar gleitet herunter, Fokus im Feld; tippen →
Chip erscheint; Bar einklappen → Chip bleibt, Lupe ist akzentuiert; Esc leert,
zweites Esc klappt ein. Artist mit kommendem Album → ✦ mit Badge; Popover
öffnen → Badge weg, Karten bleiben; Release ohne Cover → Fallback-Kachel statt
Loch; „See all" nur bei langer Liste oder vorhandenen Hidden-Einträgen; Modul
aus → ✦ verschwindet ganz.
