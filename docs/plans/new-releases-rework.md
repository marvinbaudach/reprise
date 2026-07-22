---
slug: new-releases-rework
worktree: /home/marvin/Projects/reprise/.worktrees/new-releases-rework
branch: feat/new-releases-rework
phase: shipped
codex_session:
created: 2026-07-21
---
# Release-Popover-Rework („Kompakt", Mockup 1a) — Implementierungsplan (final)

Branch `feat/new-releases-rework`, Basis `main`. Referenz: Mockup
`Release-Popover.dc.html`, Option 1a (liegt nicht im Repo; alle maßgeblichen
Details stehen in diesem Plan). Alle offenen Fragen des Entwurfs sind im
Grilling vom 2026-07-21 entschieden — siehe Abschnitt 6; dieser Plan ist ohne
weiteren Kontext umsetzbar.

## 1. Ziel & Nicht-Ziele

**Ziel:** Das bestehende Neuerscheinungen-Popover (Sparkle-`GtkMenuButton`
in der Headerbar) wird auf das Kompakt-Layout umgebaut: Zähler-Badge am
Auslöser, Header-Zeile mit „N new"-Tag, scrollende Release-Liste (keine
UI-Kappung) mit Status-Chip und Hover-Aktionen (Open announcement / Hide /
Show in library), Verlaufs-**Unterseite im Popover** (innerer `GtkStack`),
Fußzeile mit Ghost-„Fetch now" + relativer Zeit. Die separate Digest-View
samt `BrowserPlace::NewReleases` wird **entfernt**. Core-Erweiterung:
Ankündigungs-URLs, Verlauf/Retention, In-Library-Markierung,
Staleness-Refresh inkl. stündlichem Hintergrund-Check.

**Nicht-Ziele:** kein ListenBrainz `fresh_releases` (Beschluss 1 — die
MusicBrainz-Pipeline in `reprise-core/src/artist_news.rs` +
`musicbrainz.rs` bleibt die einzige Wahrheit); kein Deezer-Kontakt
(Beschluss 6); kein „Remind me"-Scheduler; kein direkter Play-Pfad aus dem
Popover — „Anhören" heißt navigieren + fokussieren (Beschluss 5); keine
deutschen UI-Strings (Repo-Regel „English everywhere", `AGENTS.md`; Texte
wie „NEW RELEASES", „3 new", „in 25 d", „released", „In library",
„Show history", „Fetch now", „Show in library"); kein Umbau der
Preferences-Seite (`preference_new_releases.rs` bleibt).

## 2. Ist-Zustand (auditiert im Worktree)

- **Popover:** `crates/reprise-gnome/src/ui/new_releases/popover.rs`
  (532 Z.) — Badge ist ein **Punkt** („•"), kein Zähler. Zeilen sind
  `gtk4::Box` mit permanentem „Hide"-Pill, max. 5 (`POPOVER_LIMIT`, Z. 15);
  „See all" → Digest (Fundstellen in 3.8). Footer mit
  „Fetch now"+Spinner-`GtkStack`, Inline-Failure (NR-6). `connect_show`
  stempelt nur die **Top 5** als gesehen. Pure Presentation-Strukturen
  (`OpeningEffect`, `FooterPresentation`, `ModuleEffect`) headless getestet
  (`popover_tests.rs`).
- **Digest (wird entfernt):** `…/new_releases/digest.rs` (181 Z.),
  Content-Stack-Seite `"new-releases"`. Fundstellenliste in 3.8.
- **Cover:** `crates/reprise-core/src/cover_download.rs` —
  `fetch_release_group_cover_with` (Z. 93–106) prüft und schreibt **keinen**
  Negativ-Marker; ein 404 wird bei jedem `map` erneut abgerufen. Der
  Album-Pfad (`fetch_and_cache`) hat das Muster bereits
  (`negative_marker_path`, `write_negative`).
- **Core-Pipeline:** `crates/reprise-core/src/artist_news.rs` (737 Z.) —
  Kandidaten nach Play-Count (Top 20 + 5/Tag-Rotation), MBID-Auflösung inkl.
  Negativ-Persistenz, Release-Group-Browse (`release_groups_url`, Z. 127),
  Filter (Album/EP ≤ 90 Tage, Singles nur zukünftig, Sekundärtypen raus,
  Kappung `MAX_ITEMS = 5` pro Artist), Upsert in `new_releases` (`seen_at`,
  `hidden`, `fetched_at`, `fallback_accent`). `query_releases` (Z. 427)
  **filtert In-Library-Alben komplett heraus**. Kein `first_seen`, kein
  `hidden_at`, keine Retention, keine Ankündigungs-URL.
- **HTTP:** `crates/reprise-core/src/musicbrainz.rs` — prozessweit 1 req/s,
  User-Agent `Reprise/<version> ( <contact> )`; `fixture_request` (Z. 83)
  matcht die Browse-URL über `type=album%7Cep%7Csingle` →
  `FixtureRequest::NewReleases` (Seam `REPRISE_MUSICBRAINZ_FIXTURE_DIR`).
- **Runtime:** `crates/reprise-gnome/src/ui/artist_news/artist_news_worker.rs`
  — `ArtistNewsRuntime` (Enabled-Subscription, Worker-Thread). Der
  Fetch-/Render-Lebenszyklus (Spinner, Badge, `fetching`-Flag,
  `fetch_from_database`) liegt aber in `popover.rs`, gekoppelt via
  `bind_runtime`/`enabled_changed`.
- **Session:** `crates/reprise-core/src/library/session.rs` — persistiert
  `browser_place`/`library_root`/`play_origin_place` als JSON;
  Deserialisierungsfehler verwerfen die **gesamte** Session (Z. 108–114).
  `place_is_resolvable` (Z. 156) ersetzt `NewReleases` schon heute durch
  die Library-Root. `NavHistory` selbst ist rein in-memory.
- **Migrationen:** `crates/reprise-core/src/db.rs` — zuletzt
  `db_library_exclusions::migrate_v25`; nächste freie Version **v26**.
  Testmuster: `db_recent_migration_tests.rs`.
- **Strings:** `crates/reprise-gnome/src/ui/strings_news.rs` **existiert
  bereits** (171 Z., via `strings.rs` re-exportiert) — A0 erweitert sie.
- **Stil/Motion:** Feature-CSS als `css()`-Sektion in
  `style/mod.rs::app_css()`; Zustandsvokabular zentral (`style/buttons.rs`,
  BTN-1..4); Themes liefern `@accent_bg_color`/`@accent_color`
  (`style/theme.rs`) — kein Blurple. Motion-Tokens Micro 150 ms / Standard
  250 ms (`ui/motion.rs`, MOT-1).
- **Regeln:** `docs/ux-rules.md` Sektion R (NR-1..8 `[aktiv]`). Prozess:
  IDs append-only; Bedeutungsänderung ⇒ Ersatzregel `[ersetzt durch <ID>]`,
  Tests im selben Commit umhängen; Flip auf `[aktiv]` im
  Implementierungs-Commit (`scripts/check-ux-traceability.sh`).

## 3. Architektur & Datenmodell

Leitplanke: alle Entscheidungslogik (Badge-Zahl, Chip-Text, URL-Priorität,
Gruppierung, Retention, Staleness) lebt als pure Funktionen/Typen in
`reprise-core` bzw. als GTK-freie Presentation-Structs im Popover-Modul
(bestehendes Muster `ModuleEffect`); Widgets bleiben dünn. Zieldateigrenze
< 800 Zeilen pro Quelldatei.

### 3.1 DB-Migration v26 (`crates/reprise-core/src/db_new_releases_history.rs`, neu)

```sql
ALTER TABLE new_releases ADD COLUMN first_seen INTEGER;   -- Backfill: fetched_at
ALTER TABLE new_releases ADD COLUMN hidden_at INTEGER;    -- Backfill: strftime('%s','now') WHERE hidden = 1
ALTER TABLE new_releases ADD COLUMN announce_url TEXT;    -- NULL = nur Fallback bekannt
```

`first_seen` wird beim **Insert** gesetzt und beim Upsert **nie**
überschrieben (Episodenbeginn; FB-4); `fetched_at` bleibt Cache-Alter.
`hidden_at` pflegt `set_release_hidden` mit. Muster:
`pub(crate) fn migrate_v26(conn)` + Aufrufzeile am Ende von
`db.rs::migrate_with_cache_dirs` (nach `migrate_v25`); Tests analog
`db_recent_migration_tests.rs` (Upgrade v25→v26 inkl. Backfill).

### 3.2 Verlauf & Retention (`crates/reprise-core/src/artist_news_history.rs`, neu)

- `HistoryEntry { release_group_mbid, artist_name, title, release_type,
  first_release_date, first_seen, seen_at, hidden, hidden_at, in_library,
  announce_url }` + abgeleiteter `HistoryStatus` (New/Seen/Hidden).
- `query_history(conn, today)` — **alle** Zeilen (inkl. hidden), neueste
  zuerst nach `first_seen`; In-Library-Annotation über den A3-Helper.
- `group_history(entries, today) -> Vec<HistoryGroup>` — pure Gruppierung:
  „This week" (ISO-Woche von `today`), sonst Monatsname + ggf. Jahr.
- `enforce_retention(conn, now)` — löscht hart: `first_seen < now − 6
  Monate` sowie alles jenseits der 200 neuesten (strengere Grenze gewinnt).
  **Schutz:** nie löschen, was noch im 90-Tage-Fetch-Fenster liegt
  (`first_release_date` ≥ heute − 90 d oder zukünftig/unvollständig) —
  sonst fügt ein Re-Fetch die Zeile neu ein und sie badgt erneut.
  Konstanten `HISTORY_RETENTION_SECONDS`, `HISTORY_MAX_ENTRIES = 200`.
  Aufruf am Ende von `refresh_with`.
- `restore_release(conn, mbid)` — setzt `hidden = 0, hidden_at = NULL` für
  genau einen Eintrag („Show again"). Ersetzt das pauschale
  `show_hidden_releases` (Entfernung in C2).

### 3.3 Ankündigungs-URL (`crates/reprise-core/src/artist_news_links.rs`, neu)

- Der bestehende Browse-Request bekommt `&inc=url-rels`
  (`release_groups_url`; MB erlaubt Relationship-Includes bei Browse).
  **Kein zusätzlicher Request**, Rate-Limit unverändert.
- `parse_announce_url(group_json) -> Option<String>`: Priorität aus den
  URL-Relations — `purchase for download`/`free streaming`
  (Bandcamp-Domains zuerst) → `official homepage`/`discography entry` →
  `None`. Ergebnis beim Upsert in `announce_url` persistiert
  (offline-fähig, kein Klick-Fetch).
- `announce_url_or_fallback(stored: Option<&str>, mbid) -> String` —
  Fallback `https://musicbrainz.org/release-group/{mbid}`. Keine
  Deezer-Stufe (Beschluss 6); Kette bleibt erweiterbar.
- Fixture-Route: `fixture_request` matcht auf `type=album%7Cep%7Csingle`
  und versteht die um `inc=url-rels` erweiterte URL weiterhin als
  `NewReleases(mbid)` — Test ergänzen.

### 3.4 In-Library-Markierung + Kappung (`artist_news.rs`, Umbau)

- `StoredRelease` erhält `in_library: bool` und `announce_url:
  Option<String>`. `query_releases` **markiert** statt zu filtern: der
  Retain-Block (Z. 464–466) wird durch Annotation über das bestehende
  normalisierte Artist/Album-Set ersetzt; das Set wandert in einen Helper
  (`local_album_set(conn)`), den auch `query_history` nutzt. Bereits
  erschienene und importierte Alben erscheinen wieder — mit „In
  library"-Chip und „Show in library"-Aktion (Beschluss 4).
- Der Parse-Zeit-Filter (`local`-Set in `parse_release_groups`) bleibt:
  Alben, die beim Fetch schon lokal sind, werden weiterhin nie eingefügt.
- `MAX_ITEMS` steigt von 5 auf **20** (Beschluss 3). Das ändert den
  Wortlaut von NR-1 („höchstens fünf") ⇒ prozesskonform Ersatzregel
  **NR-1a** (identischer Text, Kappung zwanzig), NR-1 →
  `[ersetzt durch NR-1a]`, alle `nr_1_*`-Tests im selben Commit auf
  `nr_1a_*` umhängen (6× `artist_news_tests.rs`, 1× `musicbrainz.rs`).

### 3.5 Staleness-Refresh + stündlicher Check (`artist_news.rs` + `popover.rs`)

- Core (A5): `refresh_due(last_fetch_at: Option<i64>, now: i64, jitter:
  i64) -> bool` mit Basisintervall **6 h**; `jitter_seconds(seed: &str) ->
  i64` deterministisch aus einem Hash (z. B. DB-Pfad) in `[0, 45 min]`;
  `latest_fetched_at(conn) -> Option<i64>` (MAX(`fetched_at`)).
  Per-Artist-TTL (`FETCH_TTL_SECONDS` = 7 Tage) unverändert.
- Nutzernahe Auslöser: App-Start (nur bei aktivem Modul) und
  Popover-Öffnen prüfen `refresh_due` und stoßen höchstens einen
  Hintergrund-Fetch an (bestehender `one_shot_task`-Pfad).
- **Zusätzlich (Beschluss 8):** leiser periodischer Check via
  `glib::timeout_add_seconds_local(3600, …)`. Audit: der
  Fetch-/Render-Lebenszyklus (Spinner, `fetching`-Flag, Badge) liegt in
  `NewReleasesPopover` (`popover.rs`), das via `bind_runtime` an der
  Enabled-Subscription hängt — der Timer lebt deshalb dort:
  `enabled_changed(true)` startet ihn, `enabled_changed(false)` entfernt
  die `SourceId`. Callback (Weak-Upgrade): nur wenn `!fetching` und
  `refresh_due(latest_fetched_at(conn), now, jitter_seconds(db_path))` →
  `fetch_now()`. Kein Netzkontakt bei deaktiviertem Modul; höchstens ein
  Fetch pro Auslösung.

### 3.6 Cover-Cache-Negativ-Marker (`crates/reprise-core/src/cover_download.rs`)

- `fetch_release_group_cover_with` prüft vor jedem Netzabruf
  `negative_marker_path(release_group_key(mbid))`; bei `NotFound` schreibt
  es den Marker (`write_negative`). Marker älter als 7 Tage (mtime) wird
  ignoriert und neu geprüft (Cover können nachträglich erscheinen).
  `TransientFailure` schreibt **keinen** Marker.

### 3.7 GTK-Aufbau (Zieldateien unter `crates/reprise-gnome/src/ui/new_releases/`)

- `popover.rs` — Zustand + Verdrahtung, `GtkPopover` mit innerem `GtkStack`
  (Seiten `"list"`/`"history"`, Slide mit Standard-Token), Breite ~336 px.
  **Keine UI-Kappung:** `POPOVER_LIMIT` und `see_all_visible` entfallen;
  Liste = `GtkListBox` in `GtkScrolledWindow` mit
  `propagate_natural_height(true)` + `max_content_height` ≈ 5 Zeilen
  (~288 px). Öffnen stempelt **alle** gelisteten (nicht versteckten)
  Einträge als gesehen → Badge 0. Unter der Liste die Verlaufszeile
  „Show history (N)" (ersetzt „See all" ersatzlos, Chevron
  `go-next-symbolic`). Footer, Spinner, Inline-Failure,
  Checking-Leerzustand: NR-6/NR-8 unverändert. Staleness-Check beim Öffnen
  + Timer (3.5).
- `release_row.rs` (neu) — `GtkListBox`-Row (selection none) mit Cover 40 px
  (Radius 4, `LazyReleaseCover`), Titel + Metazeile „Artist · Type · Date",
  rechts `GtkStack` mit Seiten `"chip"` (Status-Pille) und `"actions"`
  (zwei flache Icon-Buttons). `GtkEventControllerMotion` + Fokus
  (`state-flags-changed`/`EventControllerFocus`) schalten den Stack;
  Crossfade mit Micro-Token. Tastaturparität (ACC-1): Row focusable, Fokus
  zeigt Aktionen, Buttons per Tab/Enter. Pure: `chip_presentation(release,
  today) -> ChipPresentation` — kommend „in N d" / erschienen „released" /
  `in_library` „In library". Aktionen: primär = `in_library && erschienen`
  → **„Show in library"** (`MetadataNavigator::navigate(
  NavigationIntent::OpenAlbum { album, anchor_track_id: None }, …)`,
  Popover schließt vorher; **kein** Play-Pfad, Beschluss 5), sonst „Open
  announcement" (`gtk4::UriLauncher`, URL aus `announce_url_or_fallback`);
  sekundär = „Hide" mit `GtkRevealer`-Collapse (SlideUp, Standard-Token)
  vor dem Persistieren-Callback.
- `history_page.rs` (neu) — Popover-Unterseite: Kopf (Back-Chevron
  `go-previous-symbolic`, „HISTORY", Zähl-Pille), `GtkListBox` mit
  Gruppen-Headern aus `group_history`, Zeilen mit Status-Text + Aktion
  („Show in library" / Link / „Show again" bei hidden, 55 % Opacity),
  Fußzeile „All caught up" + „Retention: 6 months"; Scroll via
  `GtkScrolledWindow` mit `max_content_height`.
- `badge.rs` (neu) — ✦-`GtkMenuButton` + Overlay-Zähler-Label (~16 px
  Pille, `@accent_bg_color`-Füllung, `@accent_fg_color`-Ziffer, 2 px Ring
  in `@window_bg_color`), „9+" ab 10, `set_visible(false)` bei 0. Pure:
  `badge_presentation(unseen: i64) -> Option<String>`.
- `css.rs` (neu) — `pub(in crate::ui) fn css() -> String`, registriert in
  `style/mod.rs::app_css()`. Klassen `new-release-*`: Chip-Outline-Pillen
  (Akzent-Border + Tint `alpha(@accent_bg_color, …)` bzw. neutral gedimmt),
  Zeilen-Hover-Tint `alpha(currentColor, 0.04)`, Separator mit auslaufenden
  Enden (1-px-`linear-gradient`), Ghost-Button (Akzent-Text, kein Rahmen;
  Zustände aus `style/buttons.rs`, BTN-4), Meta-Text 55 %. Radien: Popover
  14 px, Elemente 8 px, Cover 4 px. **Akzent = Theme-Akzent**, kein
  Blurple (Beschluss 7).
- Icons (verbindlich, zur Laufzeit via `IconTheme::has_icon` mit Fallback):
  „Open announcement" = `external-link-symbolic` → `web-browser-symbolic`;
  „Show in library" = `go-jump-symbolic` → `folder-music-symbolic`
  (**bewusst kein** `media-playback-start-symbolic` — das Play-Dreieck des
  Mockups wäre eine falsche Zusage, Beschluss 5); Hide =
  `view-conceal-symbolic`; Restore = `view-reveal-symbolic`; Verlauf =
  `document-open-recent-symbolic`; Chevrons =
  `go-next-symbolic`/`go-previous-symbolic`; Refresh =
  `view-refresh-symbolic`. Kein Phosphor-Import.
- Strings: alle neuen Konstanten in `strings_news.rs` (ein Task, ein
  Owner).

### 3.8 Digest-Entfernung & Navigation (Beschluss 2)

Die Digest-View wird ersatzlos entfernt; der Verlauf übernimmt als
Popover-Unterseite. Auditierte Fundstellen (Stand dieses Branches):

- `crates/reprise-gnome/src/ui/new_releases/digest.rs` — Datei löschen
  (181 Z.); `mod.rs:1` (`pub(in crate::ui) mod digest;`).
- `crates/reprise-gnome/src/ui/window/window.rs:330` (Konstruktion), `:341`
  (`content_stack.add_named(…, "new-releases")`), `:407–421`
  (`open_new_releases`-Closure), `:422–429` (`popover::install`-Aufruf —
  Callback-Parameter entfällt hier; B3 führt später den
  `MetadataNavigator` ein).
- `crates/reprise-gnome/src/ui/window/library_shell.rs:243–256`
  (`route_to_place`-Zweig `place.is_new_releases()` inkl. Fokus-Idle).
- `crates/reprise-gnome/src/ui/nav_history.rs:39–41` (`is_new_releases`),
  `:100–115` (`record_new_releases`/`record_new_releases_from`), `:195`
  (`intent_for`-Arm), Test `new_releases_is_a_regular_back_forward_place`
  (Z. 253–265).
- `crates/reprise-core/src/browser.rs:172` (Variante `NewReleases`), Arme
  Z. 196/209/217/238; `crates/reprise-core/src/browser/navigation.rs:41`
  (`NavigationIntent::OpenNewReleases`), `:178`.
- `crates/reprise-gnome/src/ui/new_releases/popover.rs`: `see_all`-Feld
  (Z. 100), `on_see_all` (Z. 107/114/158), Aufbau (Z. 124–128/136), Handler
  (Z. 188–193), `see_all_visible` (Z. 52–54) + Aufruf (Z. 262–264).
- Tests: `popover_tests.rs:35–39`
  (`nr_4_see_all_appears_for_overflow_or_hidden_entries` — löschen),
  `:48–66` (`nr_4_popover_rows_offer_hide_without_the_digest_view` —
  umbenennen in `popover_rows_offer_hide`, Doc-Kommentar entschlacken),
  `:19–24` (`nr_5_…` → `nr_5a_…`);
  `crates/reprise-core/src/artist_news_tests.rs:462`
  (`nr_4_hide_sets_hidden_and_show_restores_hidden_releases` — umbenennen in
  `hide_sets_hidden_and_show_restores_hidden_releases`; C2/A2 liefern die
  regelbenannten `nr_12_*`-Nachfolger).
- Kommentare: `ui/library_views/artist_avatar.rs:2`,
  `ui/new_releases/release_cover.rs:3` (Digest-Erwähnung anpassen).
- **Persistierte Session:** `crates/reprise-core/src/library/session.rs`
  speichert `browser_place`/`library_root`/`play_origin_place` als JSON.
  Nach Entfernen der Enum-Variante würde eine alte Session mit
  `"NewReleases"` die **gesamte** Session verwerfen (Z. 108–114). Umgang:
  die drei Felder bekommen einen nachsichtigen Deserializer (Muster
  `deserialize_up_next`, Z. 286): erst in `serde_json::Value`, dann
  `from_value::<BrowserPlace>(…).ok()` — unbekannte Variante ⇒ `None` ⇒
  `normalize`/`resolve_persisted_places` fällt wie heute auf die
  Library-Root zurück; Geometrie/Queue bleiben erhalten. Test mit
  eingefrorenem Alt-JSON.
- Regelwerk: NR-4 → `[ersetzt durch NR-12]` (Wegweiser-Satz), NR-5 →
  `[ersetzt durch NR-5a]`; **NR-5a** `[aktiv]` [gtk]: „Das Popover ist
  transient; Öffnen/Schließen verändert den Navigations-Stack nie. Nur
  explizite Zeilen-Aktionen (Show in library) navigieren regulär und
  schließen das Popover; der Verlauf ist eine Popover-interne Unterseite
  ohne Navigation." Test-Umbenennung im selben Commit.

### 3.9 Regelwerk (docs/ux-rules.md, Sektion R — append-only)

Neue Regeln als `[geplant]`-Entwürfe mit `<!-- REVIEW: Regelvorschlag -->`
in A0, Flip auf `[aktiv]` im jeweiligen Implementierungs-Commit:

- **NR-9** [gtk] — setzt auf NR-3 auf (NR-3 bleibt `[aktiv]`): „Der Badge
  aus NR-3 zeigt die Anzahl der Einträge mit `seen_at IS NULL`, ‚9+' ab
  10, verschwindet mit dem Öffnen (alle gelisteten Einträge werden
  gestempelt), kein leeres Element." Flip in B2.
- **NR-10** [gtk] — Zeilen-Hover/Fokus zeigt Aktionen statt Chip, Chip
  kehrt beim Verlassen zurück; Tastaturparität. Flip in B3.
- **NR-11** [gtk] — Ankündigungs-URL-Priorität (url-rels →
  Fallback-MB-Release-Group-Seite), Öffnen extern. Flip in B3.
- **NR-12** [gtk] — Verlauf: persistente Historie aller je gezeigten
  Meldungen als Popover-Unterseite, gruppiert, Hidden pro Eintrag
  rückholbar, Retention 6 Monate ∧ max. 200 (hartes Delete, Schutz
  Fetch-Fenster). Ersetzt NR-4. Flip in C2.
- **NR-13** [gtk] — In-Library-Markierung + „Show in library"-Aktion
  (Navigieren + Fokus, kein Play). Flip in B3.
- Ersatzregeln aus Prozesspflicht: **NR-1a** (Kappung 20, A3) und
  **NR-5a** (3.8, R1) — jeweils sofort `[aktiv]` im selben Commit.

## 4. Task-Breakdown

Pakete mit disjunkter Datei-Ownership; Tasks innerhalb eines Pakets
sequenziell. Reihenfolge: A0 → R1 → A1..A6 → danach sind **B, C (C1-Aufbau),
D parallelisierbar**; C-Integration wartet auf B2, E zum Schluss. Jeder
Task: TDD (Red zuerst), Gates (Abschnitt 7), ein Commit.

### Paket A — Core & Fundament

- **A0 · Regeln + Strings.** Dateien: `docs/ux-rules.md` (NR-9..13
  `[geplant]` wie 3.9), `crates/reprise-gnome/src/ui/strings_news.rs`
  (**erweitern**, nicht neu anlegen: „N new", „in N d", „released",
  „In library", „Show history", „Show in library", „Open announcement",
  „Show again", „All caught up", „Retention: 6 months", „HISTORY", … +
  Helper `new_releases_days_until(days)`, `new_releases_new_count(n)`).
  TDD: String-Formatter-Unit-Tests zuerst. Abhängigkeiten: keine.
- **A1 · Migration v26.** Dateien:
  `crates/reprise-core/src/db_new_releases_history.rs` (neu), `db.rs` (eine
  Aufrufzeile). TDD zuerst: Upgrade-Test v25→v26 mit Backfill-Assertions
  (Muster `db_recent_migration_tests.rs`). Abhängigkeiten: keine.
- **A3 · In-Library-Annotation + StoredRelease + Kappung 20.** Dateien:
  `crates/reprise-core/src/artist_news.rs`, `artist_news_tests.rs`,
  `musicbrainz.rs` (nur Test-Rename), `docs/ux-rules.md` (NR-1 →
  `[ersetzt durch NR-1a]`, NR-1a `[aktiv]`). TDD zuerst:
  `nr_13_query_marks_local_albums_instead_of_dropping_them`,
  `nr_1a_secondary_types_are_excluded_before_the_twenty_item_cap`.
  `set_release_hidden` pflegt ab hier `hidden_at` mit. Compile-Fixes für
  `StoredRelease`-Konstruktionen in `popover_tests.rs` gehören zu diesem
  Task (mechanisch). Hängt an A1, R1.
- **A2 · Verlauf & Retention.** Datei:
  `crates/reprise-core/src/artist_news_history.rs` (neu) + `lib.rs`-Export +
  Retention-Aufruf am Ende von `refresh_with`. TDD zuerst:
  `nr_12_history_groups_by_week_and_month`,
  `nr_12_restore_returns_a_single_hidden_entry`, Retention-Grenzfälle
  (201. Eintrag, 6-Monats-Kante, kein Löschen im Fetch-Fenster). Hängt an
  A1, A3.
- **A4 · URL-Relations.** Dateien:
  `crates/reprise-core/src/artist_news_links.rs` (neu), `artist_news.rs`
  (`release_groups_url` + Upsert-Feld), `musicbrainz.rs`
  (Fixture-Routen-Test mit `inc=url-rels`). TDD zuerst:
  `nr_11_parse_announce_url_prefers_bandcamp_then_homepage`,
  Fallback-URL-Test. Hängt an A3.
- **A5 · Staleness-Policy.** Datei: `crates/reprise-core/src/artist_news.rs`
  (`refresh_due`, `jitter_seconds`, `latest_fetched_at`). TDD zuerst:
  Grenzwerte (5:59 h nein, 6 h + Jitter ja, `None` → ja), Jitter
  deterministisch & ≤ 45 min. Abhängigkeiten: keine (Sequenz nach A4).
- **A6 · CAA-Negativ-Marker.** Datei:
  `crates/reprise-core/src/cover_download.rs`. TDD zuerst:
  `fetch_release_group_cover_with`-Tests — NotFound schreibt Marker,
  Marker < 7 d verhindert Fetch, alter Marker re-checkt, TransientFailure
  schreibt nichts. Abhängigkeiten: keine.

### Paket R — Digest-Entfernung

- **R1 · Digest-View, NavPlace, Session-Altlast.** Genau die Fundstellen aus
  3.8: `digest.rs` löschen, `mod.rs`, `window.rs`, `library_shell.rs`,
  `nav_history.rs`, `browser.rs`, `browser/navigation.rs`,
  `library/session.rs` (nachsichtiger Deserializer + Test),
  `popover.rs`/`popover_tests.rs` (See-all-Minimalrückbau + Test-Renames),
  Kommentare, `docs/ux-rules.md` (NR-4 → `[ersetzt durch NR-12]`, NR-5 →
  `[ersetzt durch NR-5a]`, NR-5a `[aktiv]`). TDD zuerst (Red):
  Session-Test `session_with_removed_place_variant_falls_back_to_library_
  root` mit eingefrorenem Alt-JSON (`"NewReleases"`), erst dann Variante
  entfernen; `nr_5a_opening_the_popover_never_requests_navigation`
  (Rename + ggf. Assertion nachschärfen). Hängt an A0.

### Paket B — Popover-UI (GTK)

- **B1 · Badge-Zähler.** Dateien:
  `crates/reprise-gnome/src/ui/new_releases/badge.rs` (neu, übernimmt
  `build_button`), `mod.rs`. TDD zuerst: pure `badge_presentation`-Tests
  (`nr_9_badge_counts_unseen_and_caps_at_nine_plus`, 0 → `None`).
  Hängt an A0.
- **B2 · Popover-Grundgerüst.** Datei: `popover.rs` (Umbau: Header-Zeile
  „NEW RELEASES" + „N new"-Tag, `GtkListBox` im `GtkScrolledWindow`
  (max_content_height ≈ 5 Zeilen), Verlaufszeile, innerer `GtkStack`,
  Footer, Leerzustände, Staleness-Trigger beim Öffnen via A5).
  `POPOVER_LIMIT` entfällt; `opening_effect` stempelt **alle** gelisteten
  Einträge. NR-6/NR-8-Verhalten unverändert; bestehende `popover_tests.rs`
  bleiben grün bzw. werden minimal nachgeführt. Regel-Flip NR-9; TDD
  zuerst: `nr_9_opening_stamps_every_listed_release_seen` (core-nah über
  `opening_effect` + Display-Test Badge-Verschwinden). Hängt an A0, A3,
  A5, B1, R1.
- **B3 · Release-Zeile mit Chip + Hover-Aktionen.** Dateien:
  `release_row.rs` (neu), `mod.rs`, `popover.rs` (Listen-Integration),
  `crates/reprise-gnome/src/ui/window/window.rs` (`popover::install`
  erhält den `MetadataNavigator` bzw. eine daraus gebaute
  `on_show_album`-Closure — Navigator existiert dort ab Z. 347). TDD
  zuerst: `chip_presentation`-Unit-Tests (kommend/erschienen/in_library,
  unvollständige Daten); Display-Tests `nr_10_hover_swaps_chip_for_actions`
  + Tastaturpfad (ACC-1), `nr_11_row_opens_announce_url_or_fallback`,
  `nr_13_in_library_row_offers_show_in_library`. Icon-Konvention aus 3.7
  (kein Play-Icon). Regel-Flips NR-10, NR-11, NR-13. Hängt an A0, A3, A4,
  B2.
- **B4 · Hide mit Revealer-Collapse.** Dateien: `release_row.rs`,
  `popover.rs` (Callback setzt `hidden` + `hidden_at`). TDD: Display-Test —
  Zeile kollabiert (Standard-Token), Badge/„N new" aktualisieren, Eintrag
  bleibt in der DB (für den Verlauf). Hängt an B2, B3.
- **B5 · Stündlicher Hintergrund-Check.** Datei: `popover.rs` (Timer wie
  3.5; `Cell<Option<glib::SourceId>>`, Lebenszyklus in `enabled_changed`).
  TDD zuerst: pure Entscheidungsfunktion
  `periodic_fetch_due(enabled, fetching, due) -> bool` headless;
  Display-Test für Start/Stop des Timers über die Enabled-Subscription.
  Hängt an B2, A5.

### Paket C — Verlauf (GTK)

- **C1 · History-Unterseite.** Dateien: `history_page.rs` (neu), `mod.rs`;
  die Stack-Verdrahtung in `popover.rs` erst **nach** B2 (bis dahin nur
  eigene Datei — Parallel-Arbeit erlaubt). TDD zuerst: pure
  Presentation-Mapping-Tests (Gruppen-Reihenfolge, Hidden-Opacity-Klasse,
  Aktions-Auswahl Show-in-library/Link/Restore); Display-Test Navigation
  list↔history (`nr_12_history_page_lists_grouped_entries`). Hängt an A0,
  A2; Integration an B2.
- **C2 · Restore + Fußzeile + Aufräumen.** Dateien: `history_page.rs`,
  `crates/reprise-core/src/artist_news.rs` (**einzige** C-Ausnahme im
  A-Gebiet: `show_hidden_releases` entfernen, Nutzung durch
  `restore_release` ersetzt; Ausführung erst nach Paket A). TDD: Restore
  stellt einzelnen Eintrag zurück, Zählpille, „All caught up"-Zustand.
  Regel-Flip NR-12. Hängt an C1, A2.

### Paket D — Stil

- **D1 · CSS-Sektion.** Dateien: `css.rs` (neu),
  `crates/reprise-gnome/src/ui/style/mod.rs` (Registrierung in
  `app_css()`). TDD zuerst: `app_css_contains_every_feature_section`-
  Erweiterung + Parse-Fehler-Test (bestehendes Muster in `style/mod.rs`);
  Klassen-Existenz-Tests. BTN-4 beachten (keine lokalen
  `:active`-Regeln), STYLE-1 (Wirkung, nicht Property). Klassennamen in A0
  festgezurrt. Hängt an A0; parallel zu B/C.

### Paket E — Abschluss

- **E1 · Regel-Flips + Traceability.** `docs/ux-rules.md`: verifiziert, dass
  NR-1a/NR-5a/NR-9..13 `[aktiv]` sind und die Flips in den jeweiligen
  Implementierungs-Commits passiert sind; `scripts/check-ux-traceability.sh`
  grün; keine Tests referenzieren NR-4/NR-5/NR-1 mehr. Hängt an allem.
- **E2 · Headless-Smoke + Doku.** `AGENTS.md`-Rezept (dbus-run-session +
  xvfb-run + XDG-Isolation) einmal End-to-End: Modul einschalten
  (Fixture-Dir), Popover öffnen, Badge weg, Hide → Verlauf, Restore,
  „Show in library"-Navigation. Ledger-Zeile in
  `.superpowers/sdd/progress.md`. Hängt an allem.

## 5. Mapping auf Akzeptanzkriterien

| Kriterium | Tasks | Tests (Kern) |
|---|---|---|
| Badge korrekt („9+", 0 = unsichtbar), verschwindet nach Öffnen (alle gestempelt), übersteht Neustart | B1, B2 | `nr_9_badge_counts_unseen_and_caps_at_nine_plus`, `nr_9_opening_stamps_every_listed_release_seen`, Display-Test Re-Konstruktion |
| Hover zeigt Aktionen, Chip kehrt zurück, Tastatur erreichbar | B3 | `chip_presentation`-Units, `nr_10_*` Display-Tests (Hover + Fokus), `check-input-parity.sh` |
| „Open announcement" für alle Prioritätsstufen sinnvoll | A4, B3 | `nr_11_parse_announce_url_prefers_bandcamp_then_homepage`, Fallback-URL-Test, `nr_11_row_opens_announce_url_or_fallback` |
| Ausblenden sofort raus, im Verlauf, einzeln rückholbar | B4, A2, C1, C2 | Hide-Display-Test, `nr_12_restore_returns_a_single_hidden_entry`, History-Display-Test |
| In-Library markiert statt gefiltert, „Show in library" navigiert (kein Play) | A3, B3 | `nr_13_query_marks_local_albums_instead_of_dropping_them`, `nr_13_in_library_row_offers_show_in_library` |
| Rate-Limits (1 req/s, UA), async, Offline zeigt Cache + Zeit; leiser Auto-Refresh | bestehend + A5, B2, B5 | bestehende `nr_1a_fetch_respects_rate_limit` (Rename), `refresh_due`-Units, `periodic_fetch_due`-Units, bestehender NR-6-Footer-Test |
| Cover-Cache ohne Re-Fetch, 404-Fallback deterministisch | A6 | Marker-Unit-Tests; bestehender `parse_accent`-Test; Akzent bleibt persistiert |
| Digest restlos entfernt, alte Sessions überleben | R1 | `session_with_removed_place_variant_falls_back_to_library_root`, `cargo build` ohne `NewReleases`-Referenzen außerhalb des Popover-Moduls |

## 6. Beschlüsse (Grilling 2026-07-21)

1. **Primärquelle bleibt MusicBrainz-Browse.** ListenBrainz
   `fresh_releases` kommt nicht rein — beschlossene Abweichung von §4 der
   Arbeitsanweisung: NR-1(a) „eine Pipeline, eine Wahrheit",
   `mb_username` nicht verlässlich vorhanden, Privacy-Untertitel
   verspricht nur „contacts MusicBrainz".
2. **Digest-View wird ersetzt.** `digest.rs`, `BrowserPlace::NewReleases`
   und die NavPlace-/Window-Verdrahtung fliegen raus (R1); der Verlauf ist
   Popover-Unterseite. NR-4 → `[ersetzt durch NR-12]`. Alte Sessions mit
   der Variante fallen nachsichtig auf die Library-Root zurück.
3. **Keine UI-Kappung.** `POPOVER_LIMIT` entfällt; die Liste scrollt
   (`max_content_height` ≈ 5 Zeilen). Fetch-Kappung `MAX_ITEMS` 5 → 20.
   Öffnen stempelt **alle** gelisteten Einträge → Badge 0. „See all"
   entfällt ersatzlos; die Verlaufszeile übernimmt.
4. **In-Library markieren statt filtern.** `query_releases` annotiert;
   der Parse-Zeit-Filter bleibt (nie einfügen, was beim Fetch lokal ist).
5. **„Anhören" = Navigieren + Fokus** über das bestehende
   `MetadataNavigator`/`route_to_place`-Muster; kein Play-Pfad, kein
   Play-Icon — das Play-Dreieck des Mockups ist eine dokumentierte
   Abweichung („Show in library", `go-jump-symbolic`).
6. **Deezer raus.** URL-Kette = `inc=url-rels`-Parse → Fallback
   MB-Release-Group-Seite; kein neuer Dienstkontakt.
7. **Akzent = Theme-Akzent** (`@accent_bg_color`/`@accent_color`), kein
   hartkodiertes Blurple — das Themesystem bleibt die einzige Farbquelle.
8. **Refresh:** App-Start- und Popover-Öffnen-Trigger **plus** leiser
   stündlicher Check im Popover-Lebenszyklus (Audit: dort liegt der
   Fetch-/Render-Zustand, gekoppelt an die Enabled-Subscription), der nur
   `refresh_due` (≥ 6 h + deterministischer Jitter ≤ 45 min) prüft und
   höchstens einen Hintergrund-Fetch anstößt.
9. **Retention:** 6 Monate ∧ max. 200 (strengere gewinnt), hartes Delete,
   Schutz: nichts löschen, was noch im 90-Tage-Fetch-Fenster liegt;
   Enforcement am Ende von `refresh_with`.
10. **Regelwerk:** NR-3 bleibt `[aktiv]`; NR-9 setzt als neue Regel auf
    NR-3 auf. NR-4 → `[ersetzt durch NR-12]`; NR-10..13 wie geplant.
    Folgeentscheidungen aus dem Prozess (Bedeutungsänderung ⇒
    Ersatzregel): NR-1 → NR-1a (Kappung 20), NR-5 → NR-5a (Digest-Satz
    entfällt).

**Restrisiken (beobachtbar, nicht mehr entscheidbar):**

- **url-rels-Trefferquote:** viele Release-Groups tragen keine
  URL-Relations; dann greift immer der MB-Fallback. Akzeptiert — nach
  Release beobachten, Prioritätskette ist erweiterbar.
- **`inc=url-rels` beim Browse:** von der MB-Doku gedeckt, aber vor A4
  gegen die echte API verifizieren; schlägt es fehl, bleibt die Kette
  vollständig auf dem Fallback (kein Blocker).
- **Icon-Verfügbarkeit je Theme:** `external-link-symbolic`/
  `go-jump-symbolic` sind nicht in jedem Icon-Theme garantiert — deshalb
  Laufzeit-Fallback via `IconTheme::has_icon`; Optik pro Theme nur manuell
  prüfbar.
- **GTK-CSS-Badge-Ring:** falls `box-shadow` für den 2-px-Ring nicht wie
  erwartet rendert, ersatzweise `border` auf der Pille — rein optische
  Entscheidung in D1/B1.
- **Session-Kante:** Nutzer, deren letzte Sitzung exakt auf der Digest-View
  endete, landen nach dem Update auf der Library-Root (gewollt, minimal).

## 7. Verifikation

- **Gates vor jedem Commit** (`AGENTS.md`): `cargo fmt --check` ·
  `cargo clippy --all-targets --workspace -- -D warnings` ·
  `cargo test --workspace` · `cargo audit` (einzige akzeptierte Advisory
  RUSTSEC-2024-0436). Nach Core-Änderungen Purity-Proof:
  `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'`
  muss leer sein. Dateigrenze < 800 Zeilen je Datei.
- **Unit-Tests (core):** `artist_news_history.rs`, `artist_news_links.rs`,
  `db_new_releases_history.rs`, `cover_download.rs`-Marker,
  `refresh_due`/`jitter_seconds`, Session-Fallback — ohne Display, ohne
  Netz (Fixture-JSON inline bzw. `REPRISE_MUSICBRAINZ_FIXTURE_DIR`).
- **GTK-Tests:** pure Presentation-Structs headless (`badge_presentation`,
  `chip_presentation`, `opening_effect` über alle Einträge,
  `periodic_fetch_due`); alles mit Widgets `#[ignore = "requires a display;
  run via xvfb-run"]` und einzeln via `dbus-run-session -- xvfb-run -a
  cargo test -p reprise-gnome <name> -- --ignored --test-threads=1`
  (MainContext-Race: Display-Tests nie im Rudel bewerten;
  `test_main_context::lock_main_context` verwenden).
- **Skript-Gates:** `check-ux-traceability.sh` (regelbenannte Tests für
  NR-1a, NR-5a, NR-9..13; keine Referenzen auf ersetzte IDs),
  `check-motion-tokens.sh`, `check-architecture.sh` (falls neue Dateien
  `one_shot_task` nutzen), `check-input-parity.sh`,
  `check-accessibility-semantics.sh` (Hover-Aktionen brauchen
  Tastatur-/A11y-Marker wie in `link_activation.rs`),
  `check-display-tests.sh`.
- **Headless-E2E-Smoke (E2):** vollständige Isolation zwingend —
  `dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d)
  XDG_CACHE_HOME=$(mktemp -d) GDK_BACKEND=x11 WAYLAND_DISPLAY=
  REPRISE_AUDIO_SINK=fakesink REPRISE_MUSICBRAINZ_FIXTURE_DIR=… cargo run`;
  niemals Fenster auf dem echten Desktop, niemals die reale DB
  (`~/.local/share/reprise/reprise.db`) berühren.
- **Nicht headless verifizierbar** (manueller Pass am Ende): Hover-Haptik,
  Pointer-Cursor, Icon-Optik je Theme, Popover-Schatten, Scrollgefühl der
  gedeckelten Liste.
