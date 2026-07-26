---
slug: podcasts-radio
worktree: /home/marvin/Projects/reprise-podcasts-radio
branch: feature/podcasts-radio
phase: planned
codex_session:
created: 2026-07-25
foundation_schema: 32
foundation_ux_section: AF
---
# Plan: Podcasts & Radio — zwei neue Quellen, eine UX-Grammatik

Zwei neue Bibliotheks-Quellen unter LIBRARY: **Podcasts** (RSS-Feeds + YouTube-Kanäle/-Playlists via
yt-dlp, nur Audio) und **Radio** (Internet-Radio-Favoriten über radio-browser.info). Beide teilen
eine UX-Grammatik: Sidebar-Ort mit Live-Zähler, Tabellen-View mit Filter-Pills, getinteter
rechteckiger Add-Button (nie Chip-Form), ein Add-Dialog mit einem Eingabefeld für Suche ODER URL,
Entfernen per Kontextmenü/Hover-Star mit Undo-Toast. Tiefste Einzelbaustelle ist das **Playback
nicht-lokaler Medien**: der bestehende `PlaybackBackend` spielt heute ausschließlich Dateipfade,
ICY-Tags fließen nirgends, die Controller-Welt ist Track-ID-basiert. Der Plan generalisiert den
Instrumental-Preview-Mechanismus (`PlaybackMode`) zu einem External-Media-Pfad (Podcast-Episode /
Radio-Station) mit Resume-Positionen, ICY-Now-Playing und Live-MPRIS — strukturell ohne Scrobbling
und ohne `listen_events`. Basis `dev`, Branch `feature/podcasts-radio`, eigener Worktree.

**Grill-Beschlüsse (2026-07-26)** — alle neun Zweige entschieden und unten eingearbeitet:
External-Modus bestätigt (Episoden-MPRIS voll funktional bis auf `CanGoNext`/`CanGoPrevious=false`,
Artwork als `mpris:artUrl`-Pass-through, „Play next episode" als Toast + persistenter Bar-Button,
GUID als stabile Episoden-Identität) · Radio-Modul default AN + YouTube-Schalter default AN +
Refresh app-weit, aber gedeckelt und metered-gegatet · iTunes Search keyless mit `country=` aus der
System-Locale (Podcast Index nur als optionaler Provider mit nutzereigenem Key, v1.1) ·
Unsubscribe-Kette: Commit-Zeit-Toast „{n} downloads kept · [Delete files]" → Papierkorb, nie hart;
Mehrfach-Unsubscribe koalesziert · Radio-Pause = Disconnect, präsentiert als Pause (Reconnect „live
now", letzter ICY-Titel gedimmt, Reconnect-Fehler nie leere Bar) · Glyph-Tiles v1 tragen die
Quellen-Unterscheidung (Remote-Artwork = v1.1) · CLI/MCP = v1.1 · Boundary-Klone bestätigt + fester
Konsolidierungs-Task nach Landung beider Features · ein Branch, Wellen wie geplant.

## 1. Kontext & Ziel

**Ziel:** Zwei Sidebar-Einträge in der LIBRARY-Sektion (Mockup-Reihenfolge Music → Podcasts → Radio
→ Queue), Zähler = ungespielte Episoden bzw. Favoriten-Anzahl. Je eine eigene ColumnView:

- **Podcasts:** `Date · Episode · Show · Length · Source · Status` — Datum relativ („Today",
  „Yesterday", „22. Jul"), Länge H:MM, Source-Pill RSS/YouTube (Icon + Label, outlined neutral),
  Status-Pill New (Accent) / Resume (Accent-outline) / Played (dim). Default-Sort Datum absteigend.
  Filter: Unplayed / Show / Source.
- **Radio:** `Zustands-Icon(24px) · Station · Genre · Bitrate · Country · Now playing` — spielende
  Station komplett akzentuiert (Icon, Name, Now-playing, Row-Tint accent 7 %), idle „—"; Bitrate
  „320k", Country als ISO-Code. Filter: Genre / Country.

Toolbar über jeder Tabelle: **getinteter rechteckiger Add-Button** (accent-bg 16 %, Radius 8,
Plus-Icon + „Add podcast"/„Add station" — KEINE Pill), daneben „+ Add filter"-Chip + aktive Pills,
rechts dim „23 episodes"/„12 stations". Add-Dialog: ein `adw::Dialog`, ein Eingabefeld für
Suchbegriffe oder URL — Suche → gruppierte Ergebnisse mit Subscribe/Add-Buttons; URL → Typ-Erkennung
→ Preview-Karte + Optionen → Confirm.

**Playback:** alles durch die bestehende GStreamer-Pipeline (`playbin3` spielt http(s) via
souphttpsrc); Radio und YouTube nur Audio. Radio ist live: kein Seek, keine Dauer, Elapsed +
ICY-Now-Playing in Player-Bar und MPRIS. Podcasts: seekbar, Resume-Position persistiert bei
Pause/Stop/Wechsel/Quit. **Podcasts/Radio scrobbeln nie** und erzeugen keine `listen_events` (My
Stats bleibt reine Musik-Statistik).

**Einordnung — drei geerbte Muster, zwei Neuland-Zonen:** Geerbt: (a) Fetch-Infrastruktur der
News/Concerts-Familie (Worker-Thread mit eigener DB-Connection, TTL + deterministischer Jitter,
Fixture-Seam, Modul-Gate nach NET-1) für den Podcast-Refresh; (b) View-Schnittmuster der
Concerts-Views (eigene kleine ColumnView statt TrackList-Umbau, Filter/Sort pur, kein windowed Model
— Concerts-Beschluss 3 verallgemeinert); (c) Tombstone-Undo (`removed_at` + 10-s-High-Toast,
`missing_view.rs`-Idiom). Neuland (Code-Befund, korrigiert die Scout-Karte): **Subprozess-Wrapper**
— kein `std::process::Command` im Produktionscode — und **Tag-/ICY-Plumbing** — der Bus-Watch in
`player.rs` behandelt nur Eos/StreamStart/Element/Error; ein `MessageView::Tag`-Arm existiert NICHT
und wird neu gebaut.

**Kanonische Design-Quelle:** claude.ai/design-Projekt `8fb24732-431c-447f-9a74-08d3229a0c33`,
`Tourdaten Varianten.dc.html`, Turn 4 (Podcasts, ~Z. 205–413), Turn 5 (Radio, ~Z. 21–205). Dark
`#0c0e0f/#1b1e1f/#1f2324`, Accent `#35c793` = bestehendes Redesign, keine neuen Farb-Tokens.

## 2. Architekturüberblick & Crate-Schnitt

Leitplanke: **alle Entscheidungslogik als pure, testbare Funktionen in `reprise-core`** (kein
gtk4/gstreamer/zbus — `cargo tree`-Gate); GTK dünn; Dateien < 800 Zeilen (Ziel 200–400), `window.rs`
< 600 (`check-architecture.sh`).

- **`crates/reprise-core`:** Facades `src/podcasts.rs`+`src/podcasts/` und
  `src/radio.rs`+`src/radio/` (Muster `browser.rs`+`browser/`). Podcasts: Feed-Parser (quick-xml —
  **bereits Core-Dependency** via Rhythmbox-Import), iTunes-Search, yt-dlp-Wrapper +
  YouTube-Provider, URL-Erkennung, Refresh (conditional GET), Store/Query/Status, Downloads. Radio:
  radio-browser-Boundary (Server-Discovery, Suche, Klick/Re-Resolve), M3U/PLS-Parser, ICY-Probe,
  Favoriten-Store. Dazu `src/db_podcasts_radio.rs` (Migration **V32**, s. 3 und 13) und
  Playback-Erweiterungen in `src/playback.rs` + `src/media_integration.rs` (s. 6). **Keine neuen
  Dependencies** (ureq, quick-xml, serde_json, chrono, url, thiserror vorhanden; yt-dlp braucht nur
  `std::process`).
- **`crates/reprise-platform-linux`:** `player.rs` erhält (a) Trait-Methode `play_uri` — heute lehnt
  `path_to_uri` alles ohne führenden `/` ab, http(s) ist aktuell UNSPIELBAR — und (b) den neuen
  `MessageView::Tag`-Arm (ICY `title`/`organization` → `PlayerEvent::StreamTags`). `mpris/` lernt
  Live-Streams (CanSeek=false, Metadata ohne Länge, Nicht-Track-Identität).
- **`crates/reprise-gnome`:** `src/ui/podcasts/` und `src/ui/radio/` (View, Modell, Spalten,
  Presentation, Filter-Bar, Empty-States, Add-Dialog, CSS; Podcasts zusätzlich Worker),
  `src/ui/playback/external_media.rs` (External-Modus, generalisiert `preview.rs`),
  Player-Bar-Live-Zustand, `strings_podcasts.rs` + `strings_radio.rs`, Preferences,
  Sidebar-/Routing-Verdrahtung.
- **CLI/MCP:** in v1 **keine** Surface (Grill-Beschluss: benannter v1.1-Kandidat, s. 12 —
  Concerts-Grill kippte das dort, hier ist der Playback-Umbau der Risiko-Schwerpunkt; die Surface
  bleibt additiv nachrüstbar, ein Paket-M-Klon ohne Datei-Konflikte).

**Modul-Gates** (`modules.rs`, `ALL_MODULES`): `PODCASTS_MODULE` (`id: "podcasts"`,
`default_enabled: false` — geplanter Feed-Refresh ist AUTOMATISCHES Netz, NET-1; `applies_live:
true`; Description nennt Feeds UND yt-dlp) und `RADIO_MODULE` (`id: "radio"`, **`default_enabled:
true` — Grill-Beschluss**: die präzisierte Regel lautet „Module mit AUTOMATISCHEM Netz starten
aus", und Radio funkt ausschließlich auf Nutzeraktion; die Description legt den
radio-browser-Klick-Zähler offen. Verbindliche Bedingung des Default-AN ist der Radio-Empty-State
mit Add-station-CTA, 7.5/SRC-1 — ein sichtbarer Menüpunkt ohne Inhalt darf nie eine Sackgasse
sein). Sidebar-Rows nur bei aktivem Modul (Conversions-Gate-Idiom); Toggle →
`sidebar.refresh(reason)`.

**Threading-Modell (Entscheidung):** EIN dauerhafter Worker nur für Podcast-Refresh + Downloads
(`reprise-podcasts`-Thread, Klon des `artist_news_worker`-Idioms: async_channel, eigene Connection
via `db::open_migrated`). Alle **nutzerausgelösten Einzeloperationen** — Suche
(iTunes/radio-browser/ytsearch), URL-Preview/Probe, yt-dlp-Play-Resolve, Klick-Zähler, Re-Resolve,
manueller Download-Anstoß — laufen als `one_shot_task::spawn`-Threads mit Generation-Guard
(latest-wins), damit sie nie hinter einem Refresh anstehen. Radio hat **keinen** Worker (nichts ist
periodisch).

## 3. Datenmodell & Migration V32

Neue Datei **`crates/reprise-core/src/db_podcasts_radio.rs`** (Muster `db_artist_news_fetch.rs`):
`migrate_v32(conn)` — idempotenter Version-Check, `unchecked_transaction`, `execute_batch`,
`user_version`-Bump. `db.rs`: `SUPPORTED_SCHEMA_VERSION` → **32** + Aufrufzeile nach
`db_concerts::migrate_v31`. **Nummern-Vorbehalt:** dev steht heute auf 30, Concerts plant 31 — der
Fundament-Task verifiziert den dev-HEAD beim Start und nimmt die nächste freie Nummer (Regel in 13).
Migrationstests in `db_podcasts_radio_migration_tests.rs` (Upgrade, Idempotenz, Downgrade-Schutz).

```sql
CREATE TABLE IF NOT EXISTS podcast_subscriptions (
  id              INTEGER PRIMARY KEY,
  kind            TEXT NOT NULL,              -- 'rss' | 'youtube'
  feed_url        TEXT NOT NULL UNIQUE,       -- Feed-URL bzw. kanonische Kanal-/Playlist-URL
  title           TEXT NOT NULL,
  author          TEXT,
  image_url       TEXT,                       -- v1 nur gespeichert, nicht gerendert
  etag            TEXT,                       -- conditional GET (nur RSS)
  last_modified   TEXT,
  last_fetch_at   INTEGER,
  last_outcome    TEXT,                       -- 'ok' | 'not_modified' | 'failed'
  auto_download   INTEGER NOT NULL DEFAULT 0,
  added_at        INTEGER NOT NULL,
  removed_at      INTEGER                     -- Tombstone (Undo-Fenster)
);

CREATE TABLE IF NOT EXISTS podcast_episodes (
  id              INTEGER PRIMARY KEY,
  subscription_id INTEGER NOT NULL REFERENCES podcast_subscriptions(id) ON DELETE CASCADE,
  guid            TEXT NOT NULL,              -- Dedupe; Fallback = enclosure-/Video-URL
  title           TEXT NOT NULL,
  audio_url       TEXT NOT NULL,              -- enclosure-URL bzw. YouTube-watch-URL (NIE Stream-URL)
  page_url        TEXT,
  published_at    INTEGER,                    -- NULL erlaubt (flat-playlist ohne Datum)
  duration_secs   INTEGER,                    -- itunes:duration; NULL → Probe beim ersten Play
  downloaded_path TEXT,
  played_at       INTEGER,                    -- NULL = unplayed
  position_ms     INTEGER NOT NULL DEFAULT 0, -- Resume-Position
  first_seen_at   INTEGER NOT NULL,
  UNIQUE(subscription_id, guid)
);
CREATE INDEX IF NOT EXISTS idx_podcast_episodes_sub ON podcast_episodes(subscription_id);
CREATE INDEX IF NOT EXISTS idx_podcast_episodes_pub ON podcast_episodes(published_at);
CREATE INDEX IF NOT EXISTS idx_podcast_episodes_unplayed ON podcast_episodes(played_at) WHERE played_at IS NULL;

CREATE TABLE IF NOT EXISTS radio_stations (
  id              INTEGER PRIMARY KEY,
  uuid            TEXT UNIQUE,                -- radio-browser stationuuid; NULL bei manueller URL
  name            TEXT NOT NULL,
  stream_url      TEXT NOT NULL UNIQUE,       -- aufgelöste Stream-URL (nach M3U/PLS-Downparse)
  homepage        TEXT,
  favicon_url     TEXT,                       -- v1 nur gespeichert
  genre           TEXT,
  codec           TEXT,
  bitrate_kbps    INTEGER,
  country_code    TEXT,
  votes           INTEGER,
  added_at        INTEGER NOT NULL,
  removed_at      INTEGER                     -- Tombstone (Undo-Fenster)
);
```

- **Status-Ableitung (pur, `podcasts/status.rs`):** `Played` ⇔ `played_at IS NOT NULL`; sonst
  `Resume` ⇔ `position_ms > 0`; sonst `New`. Zähler „unplayed" = `played_at IS NULL` (New + Resume).
  Episodenende (`TrackFinished`) setzt `played_at = now`, `position_ms = 0`.
- **Tombstone-Semantik:** jede Query filtert `removed_at IS NULL` (Zähler fallen sofort). Undo =
  `removed_at = NULL`; Commit (Toast-Ablauf/-Dismiss) = harte Löschung (Episoden via CASCADE).
  Re-Subscribe im Undo-Fenster: `INSERT … ON CONFLICT(feed_url) DO UPDATE SET removed_at = NULL` —
  belebt den Bestand statt zu duplizieren (analog `radio_stations` über `stream_url`/`uuid`).
- **Episoden-Identität = GUID (Grill-Beschluss, Reversibilität):** `UNIQUE(subscription_id, guid)`
  ist der EINZIGE stabile Schlüssel — Resume-Position, Played-Zustand und `downloaded_path` hängen
  ausnahmslos am GUID, nie an Datei- oder URL-Schlüsseln; YouTube-GUID = Video-ID. Damit ist die
  v1.1-Option „Episoden als Queue-Bürger" (12) eine Migration (GUID → Queue-Identität) statt
  Re-Keying, und Re-Subscribe findet verwaiste Downloads deterministisch wieder (7.4).
- **Settings** (`settings`-Tabelle, `library::settings`): `podcasts.import_count` (25),
  `podcasts.auto_download_default` (false), `podcasts.cleanup_policy` (`keep_all` |
  `delete_played_7d` | `keep_last_5`), `podcasts.youtube_enabled` (true), `podcasts.ytdlp_path`
  (leer = PATH), `sources.refresh_hours` (6, 1–24), `radio.search_order` (`votes` | `name` |
  `clicks`), Sticky-Filter `podcasts.filter.*` / `radio.filter.*`. Gebündelte Reads in
  `podcasts/config.rs` bzw. `radio/config.rs`.
- **Download-Ablage:** `{XDG_DATA_HOME}/reprise/podcasts/{fnv1a(feed_url)}/{fnv1a(guid)}.{ext}`
  (`dirs`-Crate wie DB-Pfad) — **GUID-gekeyt statt Row-ID-gekeyt** (Grill-Beschluss): der Pfad ist
  stabil über Unsubscribe/Re-Subscribe hinweg; existiert die Zieldatei beim Refresh bereits, wird
  `downloaded_path` gesetzt statt neu geladen (Reclaim verwaister Dateien). Pfad in
  `downloaded_path`; Cleanup-Policy am Ende jedes Refresh-Laufs (Muster `enforce_retention`).

## 4. Podcasts-Core

### 4.1 HTTP-Boundary (`podcasts/http.rs`, neu)

Klon des `musicbrainz.rs`-Idioms (bewusste Duplikation statt Cross-Branch-Refactor, s. 13): eigener
`Mutex<Option<Instant>>`-Limiter (1 req/s), `HTTP_TIMEOUT = 15 s`, UA `Reprise/{version} (
musicbrainz::CONTACT_URL )`, `PodcastError` (thiserror:
`Timeout`/`Transport`/`HttpStatus(u16)`/`Body`/`Parse`/`NotModified`/`YtDlp(String)`),
**Fixture-Seam** `REPRISE_PODCASTS_FIXTURE_DIR` mit Routen-Enum. **Conditional GET** (neu im Repo,
kein Präzedenzfall): Request trägt `If-None-Match`/`If-Modified-Since` aus der Subscription; 304 →
`NotModified` (nur `last_fetch_at`-Bump); 200 → `ETag`/`Last-Modified` zurück in die Subscription.

### 4.2 Feed-Parser (`podcasts/feed.rs`, neu — pur)

quick-xml-Streaming über RSS 2.0 UND Atom: `parse_feed(xml, limit) -> Result<ParsedFeed,
PodcastError>` mit `ParsedFeed { title, author, image_url, episodes }`, `ParsedEpisode { guid,
title, audio_url, page_url, published_at, duration_secs }`:

- `enclosure` mit `type` beginnend `audio/` (Fallback: erstes enclosure); Items ohne enclosure
  werden verworfen (kein Audio = keine Episode).
- `guid` (Atom: `id`); fehlt → enclosure-URL als Dedupe-Key (ausgewiesene Interpretation).
- `pubDate` RFC-2822 via chrono, Atom `published`/`updated` RFC-3339; unparsbar → `None` (stabil ans
  Ende, Zelle „—").
- `itunes:duration` tolerant („4533", „75:33", „1:15:33" → Sekunden); fehlt oft → `None`, Dauer wird
  beim ersten Play aus dem Position-Tick nachgetragen (6.3).
- `image`/`itunes:image` → `image_url`; Namespace-Präfixe über den lokalen Namen matchen (Feeds sind
  schmutzig).

### 4.3 Such-Provider & URL-Erkennung

- **iTunes Search** (`podcasts/itunes.rs`): `GET
  https://itunes.apple.com/search?media=podcast&term={q}&limit=12&country={CC}` — keyless; Parse
  `results[] { collectionName, artistName, feedUrl, trackCount }`, Zeilen ohne `feedUrl` fliegen.
  **`country=` aus der System-Locale (Grill-Leitplanke):** iTunes ist Store-gescoped — ohne
  Territory fehlen z. B. deutsche Feeds. Pure Funktion `locale_country(locale) -> &str`
  (Territory-Teil: `de_DE.UTF-8` → `DE`; unparsebar/leer/`C` → `US`) + Test. **Entscheidung
  (Mock-Abweichung, gegrillt):** iTunes statt Podcast Index — Podcast Index verlangt
  Key-Registrierung (Concerts-Lehre: Key-Beschaffung ist der Adoptions-Killer); Header ehrlich
  „PODCASTS · APPLE PODCASTS" (Mockup wird nachgezogen). Podcast Index kommt, wenn überhaupt, NUR
  als optionaler Provider mit NUTZEREIGENEM Key hinter derselben Provider-Schnittstelle — nie als
  eingebetteter Shared-Key im OSS-Repo; benannter v1.1-Kandidat (12).
- **URL-Erkennung** (`podcasts/url_detect.rs`, pur): `detect(input) -> { Search | YoutubeUrl |
  ProbableFeedUrl }` — `http(s)://` + Host-Match
  (`youtube.com/@…|/channel/|/playlist?list=|youtu.be/`) → YouTube; andere URLs → Feed-Kandidat
  (Preview verifiziert: Content-Type xml / Body beginnt `<?xml`/`<rss`/`<feed`); sonst Suche.
- **Preview:** Feed-URL → ein `http::get` + `parse_feed` (Titel, Episodenzahl); YouTube-URL → ein
  `--flat-playlist -J` (Titel, Video-Zahl). Läuft im one_shot_task des Dialogs.

### 4.4 yt-dlp-Wrapper (`podcasts/ytdlp.rs`, neu — Subprozess-Neuland)

Dünner `std::process::Command`-Wrapper, komplett in core (nur std):

- **Binary-Discovery:** `REPRISE_YTDLP_BIN` (Env, zugleich Test-Seam fürs Fake-Skript) → Setting
  `podcasts.ytdlp_path` → `"yt-dlp"` im PATH. `probe_version()` (`--version`, 10 s) speist
  Preferences-Row und Verfügbarkeits-Gate.
- **Aufrufe** (alle `--no-warnings`, stdout=JSON, Timeout mit Kill via `try_wait`-Schleife in
  100-ms-Slices): `list(url)` = `--flat-playlist -J {url}` (60 s); `search(terms)` =
  `--flat-playlist -J ytsearch5:{terms}` (60 s); `resolve(video_url)` = `-f bestaudio -j {url}` (45
  s) → `.url` + `.duration`; `download(video_url, out)` = `-f bestaudio -x -o {out} {url}` (600 s,
  nur im Worker).
- **Fehler-Mapping → lesbare Meldungen (nie Crash):** stderr-Klassifikation als pure
  Tabellen-Funktion: `Sign in to confirm`/`not a bot`/`429` → „YouTube blocked the request — update
  yt-dlp (Preferences)"; `ENOENT` → „yt-dlp is not installed — YouTube sources are disabled"; sonst
  gekürzte stderr-Zeile. Alles `PodcastError::YtDlp(msg)`.
- **Flat-Playlist-Realität:** Einträge liefern `id`, `title`, `duration` (oft null), **kein
  verlässliches Datum** → `published_at = None`, Reihenfolge = Playlist-Ordnung (Restrisiko in 12).
  `audio_url` = `https://www.youtube.com/watch?v={id}`; die **bestaudio-Stream-URL wird
  ausschließlich zur Play-Zeit aufgelöst und NIE persistiert** (läuft nach Stunden ab).
- **Feature-Gate:** `podcasts.youtube_enabled` (**Default an — Grill-Beschluss**: das
  Modul-Opt-in ist der Zustimmungsmoment, der informierte Moment ist der Add-Dialog „audio only via
  yt-dlp"; der Schalter ist der Not-Aus). Fehlt oder bricht das Binary, ist die **Degradierung
  reine ANZEIGE, nie Auto-Toggle**: der Preferences-Schalter zeigt den Zustand lesbar (Subtitle
  „yt-dlp not found — install it or set a path", s. 8), das Setting wird NIE still umgelegt;
  Add-Dialog zeigt statt YouTube-Sektion die Hinweiszeile; bestehende YouTube-Subs zeigen beim Play
  die lesbare Meldung.
- **Flatpak (Zukunftsnotiz, kein v1-Task):** yt-dlp als Flatpak-Modul bündeln; `-U` funktioniert nur
  für Standalone-Binaries — die Update-Row reicht sonst die Paketmanager-Meldung durch.

### 4.5 Refresh-Pipeline & Worker

- **`podcasts/refresh.rs` (pur):** `refresh_due(last_fetch_at, now, jitter)` mit Basisintervall
  `sources.refresh_hours` (Default 6 h) + deterministischem Jitter (FNV-1a über den DB-Pfad —
  Helfer-Klon aus `artist_news_refresh.rs`; landet Concerts zuerst und macht `fnv1a_64`
  `pub(crate)`, wird er wiederverwendet — s. 13).
- **`podcasts/pipeline.rs`:** `refresh(conn, fetch, ytdlp, now, force) -> RefreshSummary` — seriell
  über aktive Subscriptions: RSS → conditional GET → parse → Upsert per `(subscription_id, guid)`
  (**`first_seen_at`/`played_at`/`position_ms` bleiben beim Upsert unangetastet**, nur Metadaten
  aktualisieren); YouTube → `list()` → dieselbe Upsert-Mechanik. Fehler pro Subscription →
  `last_outcome = 'failed'`, Lauf geht weiter (FB-3). Danach Auto-Downloads (max. 3 neue Episoden je
  Lauf und Subscription) + Cleanup-Policy.
- **Worker (`ui/podcasts/podcasts_worker.rs`):** 1:1 nach `artist_news_worker.rs` (`PodcastsRuntime
  { enabled, worker, subscribers }`, Requests `Refresh { generation, force }` / `Download {
  episode_id }`, Antworten via async_channel + `glib::spawn_future_local`). **Trigger (Abweichung
  von Concerts — Grill-Beschluss: app-weit, gedeckelt):** App-Start-Check + stündlicher Due-Check
  auf **Fenster-Ebene** (nicht View-gebunden) — der Unplayed-Badge soll ohne View-Besuch stimmen.
  **Deckel:** der Timer läuft nur, wenn das Modul an ist UND ≥ 1 aktive Subscription existiert;
  Start-Check und Stunden-Timer **koaleszieren über die `refresh_due`-TTL** (derselbe pure Check
  entscheidet beide — nie ein Doppel-Fetch). **Metered-Gate:**
  `gio::NetworkMonitor::is_network_metered()` wird am GTK-Trigger geprüft (core bleibt frei von
  Netz-Zustand): metered ⇒ Auto-Refresh setzt aus, manuelles „Refresh now" bleibt erlaubt. Die
  Gating-Entscheidung selbst ist eine pure Funktion (Inputs: enabled, sub_count, metered, due) mit
  Test. Dazu View-Open-Staleness + „Refresh now" im Footer (NR-6-Idiom: Spinner + Inline-Failure,
  nie Toast-Regen).

## 5. Radio-Core

### 5.1 radio-browser-Boundary (`radio/http.rs` + `radio/servers.rs`, neu)

Server-Discovery: `GET https://all.api.radio-browser.info/json/servers` → Serverliste; zufällig
wählen, pro Prozess cachen; bei Fehler nächsten probieren (max. 3). Pure Auswahl-/Rotations-Policy;
Fixture-Seam `REPRISE_RADIO_FIXTURE_DIR` (`servers.json`, `search-{term}.json`,
`click-{uuid}.json`). Gleiche Limiter/UA/Timeout-Idiome wie 4.1, eigenes `RadioError`;
aussagekräftiger UA ist radio-browser-Pflicht — der bestehende UA-String erfüllt sie.

### 5.2 Suche, Klick, Re-Resolve

- **Suche (`radio/search.rs`):** `GET
  {server}/json/stations/search?name={q}&order={votes|name|clickcount}&reverse=true&limit=50&hidebroken=true`
  → `StationCandidate { uuid, name, url_resolved, codec, bitrate, country_code, tags, votes, favicon
  }`; Sub-Zeile „Metal · 320 kbit/s · US · 4.2k votes" als pure Formatter.
- **Klick + Re-Resolve in einem (`radio/click.rs`):** jeder Play einer uuid-Station schickt `GET
  {server}/json/url/{uuid}` (die Etikette) — die Antwort enthält zugleich die **frische
  Stream-URL**, die den gespeicherten Wert aktualisiert. Endpoint down → mit gespeicherter
  `stream_url` spielen (Klick ist best effort, blockiert nie > 5 s). **Toter Stream**
  (GStreamer-Error nach Connect): einmal via uuid re-resolven + erneut spielen; erst dann lesbarer
  Fehler-Toast (6.3). Stationen ohne uuid überspringen beides.

### 5.3 M3U/PLS & ICY-Probe (`radio/playlist.rs` + `radio/icy.rs`, pur)

- `resolve_playlist(body, kind) -> Option<String>`: PLS (`[playlist]`, `File1=`) und M3U/M3U8 (erste
  Nicht-`#`-Zeile). **HLS:** Body enthält `#EXT-X-` → die Eingabe-URL selbst ist die Stream-URL
  (Manifest gehört GStreamer). Verschachtelung max. Tiefe 1.
- `parse_icy_headers(headers) -> IcyProbe { name, bitrate_kbps, genre, content_type }`:
  Dialog-Preview sendet `Icy-MetaData: 1`, liest nur Response-Header (`icy-name`, `icy-br`,
  `icy-genre`, `Content-Type`), schließt ohne Body — pure Header-Map-Funktion, Boundary-Aufruf im
  one_shot_task.
- Add-Option „Fetch logo & tags from radio-browser" (an): ein `/json/stations/byurl?url=`-Call
  ergänzt uuid/favicon/tags/votes, wenn bekannt.

## 6. Playback-Integration (der kritische Pfad)

### 6.1 Backend (`reprise-core/src/playback.rs` + `platform-linux/src/player.rs`)

- **`PlaybackBackend::play_uri(&self, uri: &str)`** (neue Trait-Methode): akzeptiert
  `http`/`https`/`file`; `Player`-Impl teilt `reset_transition` + `try_play` + Rebuild-Retry mit
  `play` (interner Helfer). `play(path)` bleibt dateipfad-strikt.
- **`PlayerEvent::StreamTags { title: Option<String>, organization: Option<String> }`** (neu):
  `attach_bus_watch` erhält einen `MessageView::Tag`-Arm — `gst::tags::Title`/`Organization` aus der
  TagList, nur bei Änderung emittiert (letzter Wert im Watch-State, wie `spectrum_analyzer`).
- **Gapless-Kontrakt:** External-Play parkt den Pre-Feed (`set_next(None)` im Controller + Reset in
  `play_uri`) — Radio/YouTube geraten nie in den `about-to-finish`-Handoff (exakt der
  Preview-Kontrakt).
- **Duration-Probe:** der bestehende Position-Ticker liefert `duration_ms` — kein
  Backend-Sondercode.

### 6.2 MPRIS (`media_integration.rs` + `mpris/`)

`MprisState` wächst um `live_stream: bool` und `external_ref: Option<String>` (Kennung
`podcast/{id}` bzw. `radio/{id}`). Pure Prädikate angepasst + getestet: `can_pause`/`can_play`
zählen `external_ref` als geladen; **`can_seek` = Track ODER (external ∧ !live_stream)**;
`build_metadata` baut bei `external_ref` den trackid-Pfad `/org/reprise/Reprise/external/{ref}` und
lässt **`mpris:length` bei `live_stream` weg**; `metadata_differs` sieht die neuen Felder
(ICY-Wechsel → PropertiesChanged). Radio: `xesam:title` = StreamTitle (Fallback Stationsname),
`xesam:artist` = [Stationsname]; Podcast: Episodentitel / [Show].

Drei Schärfungen aus dem Grill (External sieht nach außen nie kaputt aus):

- **`can_go_next`/`can_go_previous` = false, sobald `external_ref` gesetzt ist** (External kennt
  keine Queue-Nachbarn — sauberes Prädikat statt No-op-Buttons); Play/Pause/Seek bleiben für
  Episoden voll funktional.
- **`mpris:artUrl` = Remote-URL-Pass-through:** Podcast → persistierte `image_url`, Radio →
  `favicon_url` (falls vorhanden). GNOME Shell lädt selbst — es gibt weiterhin **keinen
  In-App-Bild-Downloader** (Nicht-Ziel 8 bleibt unangetastet).
- **Radio-Pause ist auch die MPRIS-Wahrheit:** der präsentierte Pause-Zustand (6.3/6.4) meldet
  `PlaybackStatus = Paused` und `CanPause = true`, obwohl die Pipeline getrennt ist.

### 6.3 Controller: External-Media-Modus (`ui/playback/external_media.rs`, neu)

Generalisierung des Preview-Musters (`preview.rs` bleibt funktional, sein Enum wird erweitert):

```rust
pub(in crate::ui) enum PlaybackMode { Queue, Preview, Podcast, Radio } // advances_queue_on_finish: nur Queue
pub(in crate::ui) enum ExternalMedia {
    Podcast { episode_id: i64, title: String, show: String,
              source: EpisodeSource /* Url(String) | File(String) */,
              resume_ms: i64, duration_ms: Option<i64> },
    Radio   { station_id: i64, name: String, stream_url: String, uuid: Option<String> },
}
```

- **`play_external(media)`:** wie `play_preview` — `evaluate_play_tracking` (schließt die vorige
  Session), `current_track = None` (**strukturell kein Play-Credit, kein Scrobble, kein
  listen_event** — `begin_scrobble` wird nie erreicht), `sync_lyrics_track(None)`, `set_next(None)`,
  markiertes `NowPlaying`, dann `play_uri` (bzw. `play(path)` bei heruntergeladener Episode).
  Podcast: nach `Ok` einmal `seek_to(resume_ms)`; schlägt der frühe Seek fehl, holt ihn das erste
  Position-Event mit `duration_ms > 0` einmalig nach (pure Resume-Policy, getestet).
- **YouTube-Play:** Aktivierung → sofortige Reaktion (P-2): Bar zeigt Episode + „Resolving
  audio…"-Zustand; one_shot_task ruft `ytdlp::resolve`; Generation-Guard verwirft veraltete
  Auflösungen; Fehler → lesbarer Toast (FB-1), Bar fällt auf Stopped zurück.
- **Events (Podcast):** `TrackFinished` → `mark_played` + `end_external()` (Stop, kein
  Auto-Advance — benannter v1.1-Kandidat, 12) + Sidebar-/View-Refresh, **plus „Play
  next"-Anschluss (Grill-Beschluss):** die pure Query
  `podcasts::query::next_unplayed_of_show(subscription_id, after_published_at)` liefert die
  nächste UNGESPIELTE Episode DERSELBEN Show nach Datum — nie „die nächste Tabellenzeile". Sie
  speist **zwei Angebote derselben Aktion**: (a) Toast ~10 s „Play next: “{title}”" mit
  Action-Button direkt nach dem Episodenende, (b) einen **persistenten „Play next
  episode"-Button in der leeren/gestoppten Player-Leiste** (6.4), der nicht mit dem Toast
  verschwindet. Gespielt wird nie automatisch.
- **Events (Radio, Pause=Disconnect — Grill-Beschluss):** Pause (Bar-Taste/MPRIS) → Pipeline
  stoppt (Disconnect), der Controller hält die Station als **präsentiert-pausiert** (Bar/MPRIS:
  Paused); Play → Reconnect „live now" (frisches `play_uri`, Elapsed startet neu).
  Stream-Abriss/`PlayerEvent::Error` → einmal Re-Resolve via uuid (5.2) + Reconnect; schlägt auch
  das fehl, bleibt die Station **präsentiert-pausiert mit lesbarem Inline-Fehler + Retry in der
  Bar — nie zurück auf eine leere Bar**; die Tabelle zeigt für die pausierte Station „—" (RAD-1).
  Der Queue-Skip-Pfad (`playback_faults.rs`, FB-6) bleibt Queue-only.
- **Positions-Persistenz (Podcast):** gedrosselt alle 5 s aus dem Position-Tick + bei
  Pause/Stop/Wechsel/App-Quit (`podcasts::store::save_position`); erste Duration > 0 bei
  `duration_secs IS NULL` → nachtragen. Quit-Hook an derselben Stelle wie die Session-Persistenz.
- **StreamTags:** Controller hält `on_stream_tags`-Callbacks; Radio-View (Now-playing-Zelle),
  Player-Bar und MPRIS-Mirror werden aus **einem** Event gespeist. Session-Restore stellt
  External-Playback nicht wieder her (Nicht-Ziel; Episode bleibt via „Resume" greifbar).

### 6.4 Player-Bar & Mini-Player

`player_bar_state.rs` erhält einen Anzeige-Modus (pure Ableitung): **Radio/Live:** Waveform
versteckt, geometriegleicher Platzhalter (P-4/PLAY-7b: nichts verschiebt sich), Zeit = Elapsed-only
(Wanduhr seit Play-Start — Live-Positionswerte sind quellenabhängig unzuverlässig; pure Formatter),
Seek/Drag deaktiviert, Titel = ICY-StreamTitle, Unterzeile = Stationsname. **Radio pausiert
(Grill-Beschluss):** die Bar behält die Station (Play-Symbol, MPRIS Paused), der **letzte
ICY-Titel bleibt GEDIMMT stehen** (Vergangenheit, keine Live-Info); Reconnect-Fehler erscheinen
als Inline-Zeile mit Retry, nie als leere Bar. Der Split ist Absicht: **die Tabelle ist
Live-Wahrheit (pausierte Station = nicht verbunden = „—", RAD-1), die Bar ist
Session-Gedächtnis** (gedimmter letzter Titel). **Podcast:** Waveform im Fallback-Shape (flach —
der Draw-Pfad kann das, es gibt nur keine Peaks), Seek aktiv, „Elapsed / Total" sobald Dauer
bekannt; die gestoppte/leere Bar zeigt nach einem Episodenende den **persistenten „Play next
episode"-Button** (6.3), solange eine ungespielte Folge derselben Show existiert.
**Mini-Player-Audit (MINI-1..4):** derselbe Zustand speist die 46-Bar-Kompakt-Waveform — im
Live-Modus ebenfalls Platzhalter, Pause-Darstellung inklusive; Checklisten-Punkt in E3.

## 7. UI

### 7.1 ViewSource, Sidebar, Routing

- `view_source.rs`: `ViewSource::Podcasts` + `ViewSource::Radio` (+ Label-Tests); `browser.rs`
  BrowserPlace-Paare analog `MyStats`; `browser/navigation.rs` SidebarTargets; `ui/nav_history.rs`
  Intent-Arme. Session-Deserialisierung ist nachsichtig (Downgrade → Library-Root, Concerts-Befund).
- **Sidebar** (`sidebar_rebuild.rs`): zwei Rows in der LIBRARY-Sektion **zwischen Music und Queue**,
  Modul-gegatet; Counts über den bestehenden Count-Block: `podcasts::count_unplayed(conn)` /
  `radio::count_stations(conn)` via `nonzero_count`. `sidebar_presentation.rs`: `NavIcon::Podcasts`
  = `audio-input-microphone-symbolic` (Adwaita/devices, verifiziert), `NavIcon::Radio` =
  `network-wireless-symbolic` (Airwaves-Metapher; **`radio-symbolic` ist der RadioBUTTON-Glyph —
  nicht verwenden**), Laufzeit-Fallback via `IconTheme::has_icon` → `network-cellular-symbolic`.
  Optik = manueller Pass.
- **Routing:** `window.rs` `content_stack.add_named(…, Some("podcasts"))`/`Some("radio")` neben
  `"stats"` (~Z. 330; Aufbau gekapselt in `ui/podcasts/mod.rs::install` / `ui/radio/mod.rs::install`
  — window.rs-Budget), `library_shell.rs::wire_source_routing` (~Z. 140) beide Zweige,
  `track_list_smoke::parse_smoke_source` um `"podcasts"`/`"radio"`.

### 7.2 Tabellen-Views (`ui/podcasts/`, `ui/radio/`, neu)

Schnittmuster = Concerts-View: eigene kleine ColumnView, `gio::ListStore` + `SingleSelection`,
Filter/Sort pur über `Vec<Row>`, kein windowed Model (Dutzende Stationen, hunderte–wenige tausend
Episoden; Schwelle „> 5000 → windowing nachrüsten" als Risiko notiert). Dateien je Feature (150–350
Z.): `mod.rs` (+ `install`), `*_view.rs` (Filterzeile + `GtkStack` list/status + Footer),
`*_model.rs`, `*_columns.rs` (SignalListItemFactory, Label-Recycling), `*_presentation.rs` (pure
Formatter: relative Daten, H:MM, „320k", Pill-Mappings, Sortierung, Zählzeilen, Elapsed),
`*_filter_bar.rs`, `*_empty_state.rs`, `add_dialog.rs`, `css.rs`; Podcasts zusätzlich
`podcasts_worker.rs`.

- **Podcasts-Spalten:** Date (relativ; einzige sortierbare Spalte, Default absteigend) · Episode
  (1.65fr) · Show (0.95fr) · Length · Source (Pill `application-rss+xml-symbolic`+„RSS" bzw.
  `video-x-generic-symbolic`+„YouTube", outlined neutral) · Status (Pill New/Resume/Played).
  Aktivierung (Doppelklick/Enter) = **Play** (NAV-4-Geist; Resume ab gespeicherter Position);
  spielende Episode trägt Row-Tint accent 7 %.
- **Radio-Spalten:** Zustands-Icon (spielend `audio-volume-high-symbolic` accent, idle
  `network-wireless-symbolic` dim) · Station · Genre · Bitrate · Country · Now playing (ICY nur der
  spielenden Station, sonst „—"). Aktivierung = Play; erneute Aktivierung der spielenden Station =
  **Stop** (Grill-bestätigt, unstrittig); die Pause-Taste hat ihr eigenes Modell
  (Disconnect-präsentiert-als-Pause, 6.3/6.4) — die **pausierte** Station gilt als nicht verbunden
  und zeigt in der Tabelle „—". Sort: Station A–Z.
- **Toolbar:** Add-Button links (`buttons.rs` erhält `ADD_ACTION_CLASS` „reprise-btn-add" —
  accent-bg 16 %, Radius 8, BTN-1..4-Zustände zentral; bewusst KEIN `.reprise-filter-chip`), dann „+
  Add filter"-MenuButton + Chips (`CHIP_CSS_CLASS` wird `pub(in crate::ui)` — identische
  Fundament-Zeile wie im Concerts-Plan, s. 13), rechts dim Gesamtzahl; `FILTER_BAR_MIN_HEIGHT`-Idiom
  gegen Layout-Shift (FIL-2-Geist).
- **Filter:** Podcasts `Unplayed` (bool) / `Show` (Facette: Subscription-Titel) / `Source`
  (RSS|YouTube); Radio `Genre` / `Country` (DISTINCT-Facetten). Sticky, „Clear all ×", „X of Y
  episodes/stations"; 0 Treffer bei Filtern → StatusPage mit genau einem „Show all N …"-Schritt
  (FIL-6-Geist).

### 7.3 Add-Dialoge (`add_dialog.rs` je Feature)

Ein `adw::Dialog` (Präzedenz Tag-Editor-Form; SET-3: Ebene 1), Titel zentriert, ✕ rechts, ein
Eingabefeld mit Hint „or paste RSS / YouTube URL" bzw. „or paste a stream / M3U / PLS URL".
Zustandsmaschine pur: `Idle → Searching → Results | UrlDetected → Previewing → Preview | Error`.

- **Suche** feuert auf Enter/Submit (nie pro Tastendruck); Podcasts fächert in zwei one_shot_tasks:
  iTunes (schnell) + `ytsearch5:` (langsam, Sektion füllt nach mit Zeilen-Spinner). Sektions-Header
  small-caps „PODCASTS · APPLE PODCASTS" / „YOUTUBE · audio only" / „RADIO-BROWSER.INFO · {n}
  matches · by votes". Ergebnis-Rows: 40px-Glyph-Tile — v1 keine Remote-Artworks (Grill-Beschluss;
  Remote-Artwork-Modul = benannter v1.1-Kandidat, s. 12 Nr. 8). **Die Glyphe trägt die
  Quellen-Unterscheidung** (Grill-Leitplanke): RSS-Podcast = Mikrofon
  (`audio-input-microphone-symbolic`), YouTube = Video-Glyph (`video-x-generic-symbolic` — die App
  bündelt keine Brand-Logos), Radio = Antennen-Glyph (`network-wireless-symbolic`); gilt für
  Ergebnis- UND Preview-Tile und ist konsistent mit den Source-Pills der Tabelle (7.2). Daneben
  Titel, Sub-Zeile, outlined Accent-Button „Subscribe"/„Add" — Klick wirkt sofort (Button
  → Spinner → ✓, Dialog bleibt für Mehrfach-Adds offen), Fehler inline an der Row.
- **URL-Modus:** Erkennungs-Karte („YouTube channel detected — videos become episodes · audio only
  via yt-dlp" / „Playlist file detected (PLS) — resolved to {host}" / „RSS feed detected"),
  Preview-Zeile (Titel + „487 videos · updated today" bzw. „MP3 · 128 kbit/s · name from ICY
  header"), Options-Rows: Podcasts `Import the latest {N} episodes` (Switch an; aus = leer starten,
  nur Zukünftiges — Entscheidung) + `Download new episodes automatically` (aus); Radio `Fetch logo &
  tags from radio-browser` (an). Footer Cancel / Subscribe bzw. Add station (Confirm disabled bis
  Preview ok — `dialogs.rs`-Idiom); Fußnoten „YouTube subscriptions are played audio-only via
  yt-dlp." / „Community database — a play sends the etiquette click count to radio-browser."

### 7.4 Entfernen: Kontextmenü, Hover-Star, Undo

- **Kontextmenüs** (gio::Menu + SimpleActionGroup am Klickpunkt — `track_list_context_menu`-Idiom,
  eigene kleine Builder): Episode: `Play`/`Resume` · `Copy episode URL` · `Mark as played`/`Mark as
  unplayed` · `Download episode`/`Delete download` · ── · `Unsubscribe from “{show}”` (destructive).
  Station: `Play`/`Stop` · `Copy stream URL` · `Edit station…` (kleiner adw::Dialog: Name/Genre/URL)
  · ── · `Remove favorite` (destructive). CTX-5a-Geist: destruktiv unten, kontextbenannt.
  **Queue-Einträge fehlen bewusst (Grill-Beschluss):** „Play Next"/„Add to Queue" erscheinen für
  Episoden und Stationen GAR NICHT — weggelassen, nicht ausgegraut; die Menüs sind eigene Builder,
  External ist in v1 kein Queue-Bürger.
- **Hover-Star:** Zelle nach dem `rating.rs`-Rezept — **echte `gtk::Button`s, kein GestureClick in
  ColumnView-Zellen** (dokumentierter Befund), MotionController-Reveal, Re-Bind bei Zell-Recycling.
  Radio: gefüllter Accent-Star = Favorit, Klick entfernt. Podcast-Episode: Star wirkt auf die
  **Show** (Tooltip „Unsubscribe from {show}", TIP-1d).
- **Undo-Flow** (Klon `missing_view::tombstone_with_undo`): `removed_at = now` → Views/Zähler sofort
  → `adw::Toast` mit `set_button_label("Undo")`, `set_timeout(10)`, `ToastPriority::High` (FB-1) →
  Undo = `removed_at = NULL` + Refresh; Commit (Dismiss/Timeout, Pending-Zähler) = harte Löschung.
  Läuft gerade die entfernte Station / eine Episode der entfernten Show → Playback stoppt mit
  (kein verwaister External-Zustand).
- **Downloads beim Unsubscribe (Grill-Beschluss: Toast-Kette):** Der Unsubscribe löscht nie
  stillschweigend Dateien. Existieren Downloads, folgt zur **Commit-Zeit** ein zweiter Toast
  „Unsubscribed from “{show}” — {n} downloads kept · [Delete files]"; ignorieren = Dateien bleiben
  (die Cleanup-Policy räumt langfristig). **[Delete files] = Papierkorb** via `gio::File::trash()`,
  NIE Hard-Delete — sonst wäre es die einzige unwiderrufliche Ein-Klick-Aktion der App. Bewusste
  Konsens-Unterscheidung: die KONFIGURIERTE Cleanup-Policy (3/8) löscht weiterhin hart —
  Policy-Zustimmung in den Preferences ≠ Ein-Klick-Toast. **Mehrfach-Unsubscribe koalesziert:**
  laufen mehrere Commits mit Downloads auf, aggregiert EIN Toast („3 shows — 12 downloads kept ·
  [Delete files]") — pure Aggregations-Funktion + Test, Accumulator im View-Controller.
  Re-Subscribe findet verwaiste Dateien deterministisch über den GUID-gekeyten Download-Pfad
  wieder (3).

### 7.5 Empty-/Status-Zustände

Pure `*_empty_state_for(...)` + geteiltes `adw::StatusPage` (`track_list_empty_state`-Idiom):

| View | Zustand | Bedingung | StatusPage |
|---|---|---|---|
| Podcasts | `Empty` | keine Subscriptions | „No podcasts yet" + genau ein Button „Add podcast" (öffnet Dialog; FB-5a-Ton) |
| Podcasts | `NoEpisodes` | Subs da, 0 Episoden | „No episodes yet" + „Refresh now" |
| Podcasts | `NoResults` | 0 Zeilen, Filter aktiv | ein Button „Show all N episodes" (FIL-6) |
| Radio | `Empty` | keine Favoriten | „No stations yet" + „Add station" |
| Radio | `NoResults` | 0 Zeilen, Filter aktiv | ein Button „Show all N stations" |

Der Radio-`Empty`-State mit Add-station-CTA ist **verbindliche Bedingung des
Modul-Default-AN** (Grill-Beschluss, in SRC-1 verankert): Radio wird für alle sichtbar geboren —
der erste Blick muss in einem Klick zum Add-Dialog führen, nie in eine Sackgasse.

Offline ist kein Empty-State: Tabellen rendern aus der DB; Podcasts-Footer „Updated X ago" +
Inline-Failure (NR-6-Idiom). Fetch-Ergebnisse spielen **hart** ein (MOT-2).

## 8. Preferences

Kein neuer `PageId` (SET-1: Sektion statt Seite): beide Module über `ALL_MODULES` auf der
**Plugins-Seite** (SET-6a-Gruppe der Quellen-Absicht, wie New Releases/Concerts), Extra-Rows nach
dem `scope_row`-Helfer-Idiom (`preference_plugins.rs` ~Z. 154: `descriptor.id ==
"podcasts"`/`"radio"`-Zweige + Display-Name-/Description-Arme):

- **`preference_podcasts.rs` (neu):** SpinRow „Import latest N episodes" (5–100, Default 25) ·
  SwitchRow „Download new episodes automatically (default for new subscriptions)" · ComboRow
  „Downloads cleanup" (Keep all / Delete played after 7 days / Keep last 5 per show; löscht hart —
  Policy-Konsens, s. 7.4) · ActionRow „yt-dlp" (Version als Subtitle via one_shot-Probe,
  Update-Button `yt-dlp -U` mit lesbarer Ausgabe/Fehlerzeile) + SwitchRow „YouTube sources"
  (Default an; fehlt das Binary, zeigt der Subtitle lesbar „yt-dlp not found — install it or set a
  path" — **reiner Anzeige-Zustand als pure Decision-Funktion, das Setting wird nie automatisch
  umgelegt**, Grill-Leitplanke) · SpinRow „Refresh every N hours" (1–24, Default 6 —
  `sources.refresh_hours`).
- **`preference_radio.rs` (neu):** ComboRow „Search order" (Votes / Name / Clicks).
- SET-4: alles wirkt sofort (`connect_*` → `set_setting`); Modul-Toggles benachrichtigen die
  Runtimes (Enabled-Subscription) → Sidebar-Row erscheint/verschwindet.

## 9. UX-Regelwerk (docs/ux-rules.md)

Neue Sektion **„AF. Podcasts & Radio"** — AD ist heute die letzte Sektion, Concerts reserviert AE;
beim Einfügen gegen den dev-Stand verifizieren (Regel in 13). Regeln als `[geplant]` im Fundament,
Flip auf `[aktiv]` im jeweiligen Implementierungs-Commit mit regelbenannten Tests
(`check-ux-traceability.sh`):

- **SRC-1** [gtk] — Sidebar-Orte in der LIBRARY-Sektion (Music → Podcasts → Radio → Queue), nur bei
  aktivem jeweiligem Modul; Zähler: Podcasts = ungespielte Episoden, Radio = Favoriten; 0 → kein
  Zähler. Radio ist default-aktiv (nur Module mit AUTOMATISCHEM Netz starten aus); verbindliche
  Bedingung dieses Defaults ist der Empty-State mit Add-station-CTA (7.5).
- **SRC-2** [gtk] — Add-Aktionen sind getintete rechteckige Buttons (Accent-Fläche, Radius 8, Plus +
  Label), nie Chip-förmig; Filter-Chips bleiben outlined Pills. Beide Views teilen eine
  Toolbar-Grammatik: Add-Button · „+ Add filter" · aktive Chips mit ×-Ziel ≥ 20 px · Zählung rechts
  (FIL-1a/FIL-2-Geist).
- **SRC-3** [gtk] — Ein Add-Dialog je Quelle mit genau einem Eingabefeld für Suchbegriffe oder URL:
  Suche liefert gruppierte Ergebnisse mit Row-Buttons; eine URL führt über Typ-Erkennung zu
  Preview-Karte + Optionen + einem Confirm. Netzabrufe feuern nur auf Submit und laufen nie auf dem
  Main-Loop.
- **SRC-4** [gtk] — Entfernen ist sofort + Undo-Toast (10 s, unverdrängbar, FB-1): Row-Kontextmenü
  mit destruktivem Unsubscribe/Remove unten plus Hover-Star; bis zum Toast-Commit ist der Eintrag
  nur tombstoned. Kontextmenüs von Episoden/Stationen zeigen nie „Play Next"/„Add to Queue"
  (weggelassen, nicht ausgegraut). Podcasts: Unsubscribe löscht nie stillschweigend Dateien —
  existieren Downloads, bietet ein Commit-Zeit-Toast „{n} downloads kept · [Delete files]" den
  Papierkorb an (`gio::File::trash`, nie Hard-Delete; Mehrfach-Unsubscribe aggregiert zu einem
  Toast).
- **POD-1** [core] — Episoden-Status ist eine pure Ableitung: Played ⇔ `played_at` gesetzt, sonst
  Resume ⇔ `position_ms > 0`, sonst New; Episodenende setzt Played und löscht die Position. Tabelle
  `Date · Episode · Show · Length · Source · Status`, Default-Sort Datum absteigend.
- **POD-2** [core] — RSS ist die API: enclosure/guid/pubDate/itunes:duration; der GUID (Fallback
  enclosure-URL; YouTube = Video-ID) ist die EINZIGE Episoden-Identität — Dedupe, Resume, Played
  und Download hängen daran; Refresh per conditional GET (ETag/Last-Modified) auf Worker-Thread mit
  Intervall + Jitter; Upserts überschreiben nie Seen-/Positions-Zustand. Auto-Refresh läuft nur bei
  aktivem Modul mit ≥ 1 Abo und setzt auf metered Verbindungen aus (manueller Refresh bleibt).
- **POD-3** [core] — YouTube existiert nur hinter der Provider-Grenze via yt-dlp: Listing
  flat-playlist, Audio-URL-Auflösung ausschließlich zur Play-Zeit (nie persistiert), Fehler werden
  zu lesbaren Meldungen klassifiziert und crashen nie; ohne Binary degradiert der Provider sichtbar.
  Schalter in den Preferences (Default an); die Degradierung ist Anzeige am Schalter — das Setting
  wird nie automatisch umgelegt.
- **POD-4** [gtk] — Episoden-Playback resümiert an der gespeicherten Position; die Position wird bei
  Pause/Stop/Wechsel/Quit und gedrosselt während der Wiedergabe persistiert. Nach dem Episodenende
  bietet die App „Play next" derselben Show an (Toast + persistenter Bar-Button, Query nach Datum),
  spielt aber nie automatisch weiter. Podcasts erzeugen nie Scrobbles, listen_events oder
  Play-Counts.
- **POD-5** [gtk] — Downloads sind opt-in (pro Subscription), liegen unter dem XDG-Datenpfad der App
  und folgen der Cleanup-Policy; heruntergeladene Episoden spielen lokal (Offline-Pfad).
- **RAD-1** [gtk] — Die spielende Station ist der einzige akzentuierte Tabellenzustand (Icon, Name,
  Now-playing, Row-Tint); idle Stationen zeigen „—". Now-playing-Text existiert nur während einer
  Verbindung (ICY), nie aus dem Cache — die pausierte Station gilt als nicht verbunden und zeigt
  „—"; nur die Player-Bar darf den letzten Titel gedimmt erinnern (Session-Gedächtnis), die
  Tabelle nie.
- **RAD-2** [gtk] — Live-Playback hat kein Seek und keine Dauer: Player-Bar zeigt Elapsed +
  ICY-Now-Playing, die Waveform weicht einem geometriegleichen Platzhalter (P-4), MPRIS meldet
  CanSeek=false und Metadata ohne Länge. Radio scrobbelt nie. Erneute Aktivierung der spielenden
  Row stoppt. Pause ist Disconnect, präsentiert als Pause: die Bar behält die Station
  (Play-Symbol, letzter ICY-Titel gedimmt), MPRIS meldet Paused/CanPause=true, Play verbindet
  „live now" neu (Elapsed startet neu); ein fehlgeschlagener Reconnect lässt die Station pausiert
  stehen (lesbarer Inline-Fehler + Retry), nie eine leere Bar.
- **RAD-3** [core] — radio-browser-Etikette: Server-Wahl über `all.api.radio-browser.info` mit
  Fallback-Rotation; jeder Play einer uuid-Station sendet den Klick-Zähler; ein toter Stream wird
  vor jeder Fehleranzeige genau einmal über die uuid re-resolved.
- **RAD-4** [core] — Eine eingefügte URL wird bis zur Stream-URL heruntergeparst (PLS/M3U eine
  Ebene; HLS-Manifeste bleiben die Stream-URL); der Preview prüft per ICY-Header-Probe (Name,
  Bitrate), ohne den Body zu streamen.

## 10. i18n

Neue Kataloge `ui/strings_podcasts.rs` und `ui/strings_radio.rs` (N_!-Konstanten + Formatter:
Spaltentitel, Pills, Dialog-Texte, Fehlerklassen inkl. „YouTube blocked the request — update
yt-dlp", Zählzeilen, Empty-States, Undo-Texte), via `strings.rs` re-exportiert; **beide in
`po/POTFILES.in`**. Alle Strings englisch; keine Klartext-Strings an Widget-Call-Sites.

## 11. Teststrategie (TDD)

Jeder Task rot-zuerst; Gate-Battery pro Commit (s. 15). Kein Test kontaktiert das Netz; kein Test
startet das echte yt-dlp.

- **Pure Core-Units:** `parse_feed` (RSS 2.0, Atom, itunes-Namespace, guid-Fallback, enclosure-Drop,
  kaputtes XML, `limit`), `parse_duration`-Tabelle, pubDate-Formate, `detect`-URL-Erkennung,
  iTunes-/radio-browser-/servers-Parser (Fixtures), `resolve_playlist`
  (PLS/M3U/HLS-Passthrough/Tiefe-1), `parse_icy_headers`, yt-dlp-stderr-Klassifikations-Tabelle,
  `refresh_due` + Jitter-Determinismus, POD-1-Status-Matrix, Cleanup-Policy-Fälle, Resume-Policy
  (früher Seek fehlgeschlagen → einmalige Nachholung), Elapsed-Formatter. Neu aus dem Grill:
  `next_unplayed_of_show`-Ordnung (gleiche Show, Datum nach Referenz-Episode, überspringt Played,
  None am Ende der Show), `locale_country`-Mapping (`de_DE.UTF-8`→DE, `C`/leer/kaputt→US),
  Unsubscribe-Aggregation (1 Show / n Shows / 0 Downloads → kein Toast),
  Auto-Refresh-Gating-Entscheidung (enabled × sub_count × metered × due — nur eine Kombination
  feuert; Start-Check und Timer koaleszieren über dieselbe TTL).
- **Store-/Pipeline (In-Memory-V32):** Upsert erhält `played_at`/`position_ms`/`first_seen_at`;
  conditional-GET-Zyklus (200 → ETag gespeichert; 304 → nur Bump); Tombstone-Zyklus (Remove → Zähler
  0 → Undo → zurück; Commit → hart weg, CASCADE); Re-Subscribe belebt Tombstone;
  `count_unplayed`-Invariante; Radio-Klick/Re-Resolve gegen Fixtures (Server-Rotation, url-Update,
  Fallback).
- **Subprozess:** Fake-yt-dlp als Shell-Skript, vom Test ins Tempdir geschrieben
  (`REPRISE_YTDLP_BIN`): flat-playlist-JSON, resolve-JSON, Exit-1-mit-Bot-stderr, Hänger →
  Timeout-Kill, ENOENT, Version-Probe.
- **Playback-Core:** `play_uri`-Schema-Validierung; MPRIS-Prädikat-Matrix
  (`can_seek`/`can_pause`/`build_metadata` mit `live_stream`/`external_ref`;
  `can_go_next`/`can_go_previous` = false bei External; Länge weggelassen; `artUrl` aus
  `image_url`/`favicon_url`; `metadata_differs` bei ICY-Wechsel); `PlaybackMode`-Matrix (nur Queue
  advanced; Podcast-Finish → played + Play-next-Angebot; Radio-Finish → Reconnect-Policy);
  **Radio-Pause-Zustandsmatrix** (paused→Play = Reconnect mit Elapsed-Reset; Reconnect-Fehler →
  paused + Inline-Fehler, nie leerer Zustand; Aktivierung = Stop; Bar dimmt letzten Titel, Tabelle
  „—"); **Scrobble-Ausschluss** als regelbenannter Test (simulierte External-Session erzeugt weder
  `listen_events` noch Scrobble-Queue-Zeilen).
- **GTK-seitig:** UI-Logik ausschließlich pur in
  `*_presentation.rs`/`*_empty_state.rs`/Dialog-State-Machine headless; Display-Tests `#[ignore =
  "requires a display; run via xvfb-run"]`, einzeln via `dbus-run-session -- xvfb-run -a cargo test
  -p reprise-gnome <name> -- --ignored --test-threads=1` (MainContext-Races: Display-Tests nie im
  Rudel bewerten).
- **Regelbenannte Tests** je Flip: `src_2_add_action_is_tinted_button_not_chip`,
  `src_4_remove_is_tombstone_until_toast_commit`,
  `src_4_unsubscribe_commit_toast_trashes_never_hard_deletes`, `pod_1_status_matrix`,
  `pod_3_ytdlp_errors_are_readable_never_panic`, `pod_4_external_session_never_scrobbles`,
  `pod_4_finish_offers_next_unplayed_of_show`,
  `rad_2_live_state_disables_seek_and_reports_no_length`,
  `rad_2_pause_is_disconnect_presented_as_paused`, `rad_3_dead_stream_reresolves_once`, … —
  `check-ux-traceability.sh`.
- **Fixtures:** Inline-Strings für Parser; Dateien unter
  `REPRISE_PODCASTS_FIXTURE_DIR`/`REPRISE_RADIO_FIXTURE_DIR` für Pipeline-End-to-End (Feed-XML über
  2 Läufe: neue Episode, geänderter Titel, 304; radio-browser servers/search/click-JSON).

## 12. Risiken & Abgrenzung

**Nicht-Ziele v1 (je begründet):**

1. **OPML-Import/-Export** — eigener Adoptions-Schnitt, ohne Schema-Änderung nachrüstbar.
2. **Download-Manager-UI** — Policy + Kontextaktionen decken v1; Fortschritts-UI wäre FB-2b-Terrain.
3. **Kapitel & Transkripte** — heterogene Datenlage, eigener Player-UI-Teil.
4. **Playback-Speed** — braucht Rate-Plumbing im Backend + Bar-UI; orthogonal. **Benannter
  v1.1-Kandidat Nr. 1** (Podcast-Hörer wollen das; Grill-Beschluss).
5. **Auto-Advance zur nächsten Episode** — Ende → Played + Stop + „Play next"-Angebot (6.3);
  Auto-Play wäre ein neues Queue-Konzept für Nicht-Tracks. **Benannter v1.1-Kandidat** (zusammen
  mit der Queue-Bürgerschaft der Episoden; die GUID-Identität (3) macht daraus eine Migration
  statt Re-Keying — Grill-Beschluss zur Reversibilität).
6. **Desktop-Benachrichtigungen** neue Episoden (Parität zur Concerts-Entscheidung).
7. **CLI/MCP-Surface** — **benannter v1.1-Kandidat** (Grill-Beschluss): read-only `reprise-cli
  podcasts list` / `reprise://podcasts` + `reprise://radio` (reine Cache-Reads, ein Paket-M-Klon
  ohne Datei-Konflikte), nach Welle 1 jederzeit additiv anhängbar.
8. **Remote-Artwork** (Podcast-Cover, Station-Logos) — URLs werden persistiert; Rendering bräuchte
  einen generischen Bild-Downloader außerhalb des Cover-Moduls; v1 nutzt Glyph-Tiles mit
  Quellen-Glyphen (7.3, ausgewiesene Mock-Abweichung; Mockup wird nachgezogen). **Benanntes
  v1.1-Kandidaten-Modul** — rein additiv, alle Daten liegen bereits in der DB.
9. **Session-Restore von External-Playback** — Episode bleibt über „Resume" greifbar; Radio-Streams
  sind flüchtig.
10. **Video-Pfad** — nie (Spec: audio only).
11. **Podcast Index als Suchprovider** — nur als optionaler Provider mit nutzereigenem Key hinter
  derselben Provider-Schnittstelle (4.3); nie eingebetteter Shared-Key. **Benannter
  v1.1-Kandidat.**

**Risiken:**

- **yt-dlp-Bruch/Bot-Checks** (Kernrisiko): begrenzt durch Fehlerklassifikation (lesbar, nie Crash),
  Feature-Schalter, Update-Row, Fake-Binary-Tests; Flatpak-Bundling ist Zukunftsarbeit — bis dahin
  Host-Binary-Abhängigkeit (Prefs zeigen Version/Fehlen).
- **googlevideo-403/URL-Ablauf mitten im Play:** ein Re-Resolve-Versuch (Generation-Guard), dann
  lesbarer Toast.
- **flat-playlist ohne Daten:** YouTube-Episoden ohne `published_at` sortieren hinter datierten,
  Zelle „—"; Nach-Datierung wäre teurer Einzel-Fetch — akzeptiert.
- **radio-browser-Churn:** Server-Rotation; Klick best effort; Totalausfall trifft nur
  Suche/Logo/Klick — Favoriten spielen weiter (URLs lokal).
- **ICY-Zeichensatz:** Legacy-Streams senden Latin-1; kaputte Sequenzen lossy ersetzen (nie Panik);
  Streams ohne ICY zeigen dauerhaft „—" — beides getestete Pfade.
- **HLS-Radio:** hängt an `hlsdemux` (gst-plugins-bad); Fehlerpfad = normaler Playback-Fehler mit
  Toast; kein eigener HLS-Code.
- **HTTP-Seek (Podcast):** braucht Range-Support des Servers; Fehlschlag wird geloggt, Position
  läuft weiter.
- **MPRIS-Live-Edge-Cases:** Clients, die `mpris:length` erwarten — Weglassen ist spec-konform;
  manueller Pass mit GNOME-Shell-Widget in Z1.
- **Kardinalität:** Import-Kappung (N) + Schwelle 5000 für windowing-Nachrüstung.
- **Undo-Fenster vs. laufendes Playback:** Entfernen der spielenden Quelle stoppt Playback sofort
  (7.4) — sonst hielte der External-Zustand eine tombstoned Zeile.
- **Schema-/Sektions-Kollision mit Concerts:** Abschnitt 13.

## 13. Koordination mit dem parallelen Concerts-Feature

Concerts (docs/plans/concerts.md, Branch feature/concerts, Phase planned) berührt dieselben
Nahtstellen:

| Nahtstelle | Concerts | Dieses Feature | Strategie |
|---|---|---|---|
| `db.rs` SUPPORTED_SCHEMA_VERSION | 31 | 32 | **Regel: die Migrationsnummer gehört dem Branch, der zuerst auf dev merged.** Jeder Fundament-Task verifiziert den dev-HEAD und nimmt die nächste freie Nummer (Plan-Nummern sind Platzhalter). Landet Concerts nicht zuerst, wird hier 31. |
| `docs/ux-rules.md` Sektionsbuchstabe | AE | AF | Gleiche Regel: nächster freier Buchstabe am dev-HEAD beim F1-Commit. Append-only ⇒ Merge-Konflikt trivial. |
| `modules.rs` ALL_MODULES | +CONCERTS | +PODCASTS, +RADIO | Additive Einfügungen, benachbarte Zeilen, semantisch unabhängig. |
| `view_source.rs`, `browser.rs`, `browser/navigation.rs`, `nav_history.rs` | +Concerts/+Releases | +Podcasts/+Radio | Additive Enum-Arme, mechanisch lösbar. |
| `sidebar_rebuild.rs` / `sidebar_presentation.rs` | SMART-Sektion | LIBRARY-Sektion | Verschiedene Einfügepunkte; je ein kompakter Block. |
| `window.rs` / `library_shell.rs` / `track_list_smoke.rs` | +2 Seiten/Zweige | +2 Seiten/Zweige | Beide kapseln in `install(…)` (3–4 Zeilen); Konflikt = benachbarte Zeilen. |
| `browse_bar.rs` `CHIP_CSS_CLASS` → `pub(in crate::ui)` | ja | ja | **Identische Ein-Zeilen-Änderung** — wer zweiter merged, droppt seine Version. |
| `artist_news_refresh.rs::fnv1a_64` `pub(crate)` | ja | Klon, bis Concerts gelandet ist | Nach Concerts-Landung Klon auf den geteilten Helfer umstellen (Aufräum-Zeile). |
| `strings.rs` mod-Zeilen, `po/POTFILES.in`, `style/mod.rs` app_css | +2 Kataloge | +2 Kataloge | Append-only-Listen, trivial. |
| `preference_plugins.rs` | +concerts-Zweig | +podcasts/+radio-Zweige | Additive match-Arme; bewusst je ein kompakter Arm pro Modul. |

**Kein vorgezogener Shared-Refactor auf dev** (HTTP-Boundary, generische Filter-Bar): beide Features
brauchen nur winzige additive Zeilen an geteilten Dateien; ein Refactor jetzt blockierte beide
Branches. Duplizieren — die Boundary-Klone Nr. 3 und 4 (`podcasts/http.rs`, `radio/http.rs`) sind
**Grill-bestätigt**. **Die Konsolidierung ist ein BESCHLOSSENER Folge-Task** (nicht bloß eine
Option): nach Landung BEIDER Features ein fester dev-Task — `sources_http`-Helfer (Limiter, UA,
Timeout, Fixture-Seam einmal), ggf. generische Source-Filter-Bar, plus Umstellung des
`fnv1a_64`-Klons auf den geteilten Helfer (Aufräum-Zeile aus der Tabelle oben). Der Task wird als
Memory-Notiz mitgeführt, damit er nicht verdunstet. Rebase-Disziplin: dieser Branch rebased auf
dev, sobald Concerts gemerged ist; die F-Tasks prüfen beide „nächste freie Nummer"-Regeln erneut.

## 14. Akzeptanzkriterien (konkretisiert aufs Repo)

| # | Kriterium | Verifikation |
|---|---|---|
| 1 | Beide Sidebar-Einträge (LIBRARY, Music→Podcasts→Radio→Queue) mit Live-Zählern (unplayed / Favoriten), Modul-gegatet, Icons aus dem System-Set mit Laufzeit-Fallback; Radio default AN, Podcasts default AUS; Radio-Empty-State führt in einem Klick zum Add-Dialog | `src_1_*`, Sidebar-Rebuild-Test, Modul-Default-Tests, Display-Smoke |
| 2 | Podcasts-Tabelle rendert/sortiert/filtert wie spezifiziert (relative Daten, H:MM, Source-/Status-Pills; Filter Unplayed/Show/Source sticky) | Presentation-Units, `pod_1_*`, Filter-Units, Display-Test |
| 3 | Radio-Tabelle mit Zustands-Icon, akzentuierter spielender Row, Now-playing nur live, Filter Genre/Country | Presentation-Units, `rad_1_*`, Display-Test |
| 4 | Add-Dialog Podcasts: Suche (iTunes mit `country=` aus der Locale + ytsearch, gruppiert, „audio only"-Label, Quellen-Glyph-Tiles) UND URL-Paste (RSS + YouTube → Preview + Optionen) | Dialog-State-Units, URL-Detect-Units, `locale_country`-Test, Fixture-E2E, Display-Test |
| 5 | Add-Dialog Radio: Suche by votes mit Add-Buttons; URL-Paste direkte Streams UND M3U/PLS (Downparse) mit ICY-Preview | `rad_4_*`, Playlist-/ICY-Units, Display-Test |
| 6 | Add-Buttons sind getintete rechteckige Buttons, klar von Chips unterschieden (eigene CSS-Klasse, nie `.reprise-filter-chip`) | `src_2_*`, CSS-Klassen-Test, manueller Optik-Pass |
| 7 | Kontextmenü-Entfernen + Hover-Star mit Undo-Toast (10 s, tombstone-basiert); Unsubscribe behält Downloads, Commit-Zeit-Toast bietet [Delete files] → Papierkorb (nie hart), Mehrfach-Unsubscribe aggregiert; Menüs zeigen nie Play Next/Add to Queue | `src_4_*` inkl. Trash-Test, Aggregations-Units, Tombstone-Zyklus-Units, Display-Test |
| 8 | YouTube: Audio-only, bestaudio-Resolve pro Play (nie persistiert), yt-dlp-Fehler lesbar, ohne Binary degradiert als Anzeige (Schalter-Subtitle, Setting nie auto-umgelegt) | `pod_3_*`, Fake-Binary-Tests, Resolve-Generation-Test, Prefs-Decision-Units |
| 9 | Podcast-Resume: Position persistiert (Pause/Stop/Wechsel/Quit + Drossel), Wiedergabe setzt fort; Ende → Played; Dauer-Probe beim ersten Play | `pod_4_*`-Resume-Units, Store-Roundtrips |
| 10 | Radio live: ICY-Now-Playing in Tabelle + Player-Bar + MPRIS aus einem Event; kein Seek, keine Dauer, Elapsed-only; Pause = Disconnect-präsentiert-als-Pause (Bar dimmt letzten Titel, Tabelle „—", Reconnect „live now", Fehler → pausiert + Inline-Retry, nie leere Bar); tote Favoriten re-resolven via uuid | `rad_2_*`/`rad_3_*`, Pause-Zustandsmatrix, MPRIS-Matrix, StreamTags-Plumbing-Test |
| 11 | Podcasts/Radio erzeugen nie Scrobbles/listen_events/Play-Counts | `pod_4_external_session_never_scrobbles` + Radio-Pendant |
| 12 | Alles Netz + yt-dlp off-main-loop (Worker/one_shot); Auto-Refresh nur bei Modul an ∧ ≥ 1 Abo, TTL-koalesziert, metered-gegatet (manuell bleibt); Strings übersetzbar (beide Kataloge + POTFILES); Gates grün | Code-Audit „kein http/Command außerhalb der Boundaries", Gating-Decision-Units, Gate-Battery, `check-ux-traceability.sh` |
| 13 | Episodenende bietet „Play next" derselben Show nach Datum an (Toast + persistenter Bar-Button), spielt nie automatisch; MPRIS External voll funktional bis auf CanGoNext/CanGoPrevious=false, artUrl = Remote-Pass-through | `pod_4_finish_offers_next_unplayed_of_show`, Ordnungs-Units, MPRIS-Prädikat-Matrix, manueller GNOME-Shell-Pass |

## 15. Arbeitspakete als Wellen (Datei-Ownership)

Scope Grill-bestätigt: EIN Branch `feature/podcasts-radio`, Wellenplan unverändert.
Reihenfolge: 0) Fundament → 1) Core-Datenschicht + Playback → 2) Views + Verdrahtung → 3)
Preferences + Abschluss. Regeln: **kein Paket teilt Dateien mit einem parallel laufenden Paket**;
alle Konfliktpunkte (db.rs, view_source.rs, browser*, modules.rs, Kataloge, POTFILES, ux-rules.md,
style/buttons.rs, mod-Stubs) liegen im Fundament. Beim Wellen-Start schreibt der Koordinator die
**Datei-Ownership-Tabelle der laufenden Welle nach AGENTS.md** (main bewegt sich unter parallelen
Agenten — gelernte Lektion). Jeder Task: TDD (Red zuerst), volle Gate-Battery, ein Commit.

### Welle 0 — Fundament (ein Owner, sequenziell)

- **F1 · Regeln + Strings + Module.** Dateien: `docs/ux-rules.md` (Sektion AF — Buchstabe gegen
  dev-HEAD verifizieren — mit SRC-1..4, POD-1..5, RAD-1..4 `[geplant]`), `ui/strings_podcasts.rs` +
  `ui/strings_radio.rs` (neu, vollständige Kataloge + Formatter), `ui/strings.rs` (mod-Zeilen),
  `po/POTFILES.in`, `modules.rs` (beide Deskriptoren + ALL_MODULES). TDD: Formatter-Units,
  Modul-Default-Tests.
- **F2 · Migration V32.** Dateien: `db_podcasts_radio.rs` (neu) +
  `db_podcasts_radio_migration_tests.rs` (neu), `db.rs` (SUPPORTED-Nummer nach 13-Regel +
  Aufrufzeile). TDD: Migrationstests zuerst.
- **F3 · Enums/Facades/Stubs.** Dateien: `view_source.rs`, `browser.rs`, `browser/navigation.rs`,
  `ui/nav_history.rs` (je beide Arme), `lib.rs`-Exports, `podcasts.rs` + `radio.rs` (Facades mit
  öffentlichen Typen `EpisodeRow`, `SubscriptionRow`, `StationRow`, `EpisodeStatus`, Fehler-Enums —
  Re-Export-Gerüst, damit A/B/C/E nie dieselben Dateien brauchen), `ui/podcasts/mod.rs` +
  `ui/radio/mod.rs` (kompilierende Minimal-Stubs), `ui/podcasts/css.rs` + `ui/radio/css.rs`
  (Sektions-Stubs), `ui/style/mod.rs` (app_css-Registrierung + Sektions-Test), `ui/style/buttons.rs`
  (`ADD_ACTION_CLASS` inkl. BTN-Zustände), `ui/browse/browse_bar.rs` (nur die
  `CHIP_CSS_CLASS`-pub-Zeile; entfällt, falls Concerts sie schon brachte). TDD:
  Label-/Roundtrip-Tests. Danach ist `cargo build` grün.

### Welle 1 — Core-Datenschicht + Playback (vier Owner parallel)

- **Paket A · Podcasts-Core (Owner A).** Dateien (alle neu): `podcasts/http.rs`, `feed.rs`,
  `itunes.rs` (inkl. `locale_country` + `country=`-Param, Test), `url_detect.rs`, `store.rs`,
  `status.rs`, `query.rs` (inkl. `next_unplayed_of_show` + Ordnungs-Test), `config.rs`,
  `refresh.rs`, `downloads.rs` (GUID-gekeyte Ablage + Reclaim), `pipeline.rs` +
  `*_tests.rs`-Nachbarn. TDD: Parser/Store/Pipeline (s. 11).
- **Paket B · yt-dlp & YouTube (Owner B, nach F3, parallel zu A).** Dateien (neu):
  `podcasts/ytdlp.rs`, `podcasts/youtube.rs` + Tests + Fake-Binary-Helfer. Berührt KEINE A-Dateien
  (Provider-Typen aus der F3-Facade). TDD: Subprozess-Matrix.
- **Paket C · Radio-Core (Owner C).** Dateien (alle neu): `radio/http.rs`, `servers.rs`,
  `search.rs`, `station.rs` (Store/Query/Tombstone), `playlist.rs`, `icy.rs`, `click.rs`,
  `config.rs` + Tests. TDD: s. 11.
- **Paket E · Playback-Integration (Owner E; E1→E2→E3 sequenziell).**
  - **E1 · Backend + MPRIS-Core.** Dateien: `reprise-core/src/playback.rs` (StreamTags,
    play_uri-Trait), `media_integration.rs` (MprisState-Felder + Prädikate inkl.
    `can_go_next`/`can_go_previous` = false bei External; `artUrl`-Pass-through in
    `build_metadata`), `platform-linux/src/player.rs` (play_uri-Impl, Tag-Arm), `mpris/state.rs` +
    `mpris/mod.rs`. TDD: Prädikat-Matrix, Schema-Validierung.
  - **E2 · Controller-External-Modus.** Dateien: `ui/playback/external_media.rs` (neu), `preview.rs`
    (PlaybackMode-Erweiterung), `player_event_handling.rs` (Finish-/Error-Arme),
    `player_controller.rs` (play_external, StreamTags-Fanout, Positions-Drossel, Quit-Hook;
    Podcast-Finish → `next_unplayed_of_show`-Aufruf + Play-next-Toast; Radio-Pause-Zustandsmaschine
    Disconnect/Reconnect/Fehler-bleibt-pausiert). Hängt an E1 + A/C-Stores (save_position,
    mark_played, next_unplayed_of_show, Klick/Re-Resolve). TDD: Modus-Matrix, Resume-Policy,
    Pause-Zustandsmatrix, Scrobble-Ausschluss, `pod_4_finish_offers_next_unplayed_of_show`.
  - **E3 · Player-Bar/Mini live.** Dateien: `player_bar_state.rs`, `player_bar.rs`,
    `waveform_seek.rs`-Gating, `player_bar_seek.rs` (Drag-Guard), Kompakt-Audit (`ui/compact/*`
    lesend prüfen, Änderungen minimal). Bar-Zustände aus dem Grill: Radio-pausiert (Play-Symbol,
    gedimmter letzter ICY-Titel, Inline-Reconnect-Fehler + Retry, nie leere Bar) + persistenter
    „Play next episode"-Button in der gestoppten/leeren Bar. Flips: RAD-2-Bar-Anteil. TDD: pure
    Zustands-/Formatter-Units (inkl. Dim-/Paused-Ableitung), Display-Tests `#[ignore]`.

### Welle 2 — Views + Verdrahtung

- **Paket P · Podcasts-View (Owner A; nach A/B/E2).** Dateien: `ui/podcasts/*` (F3-Stubs füllen).
  Sequenz: **P1** Presentation+Model (pur) → **P2** View+Spalten+Empty (Flip POD-1) → **P3**
  Filter-Bar + Toolbar-Add-Button (SRC-2-Anteil) → **P4** Add-Dialog (SRC-3-Anteil; iTunes mit
  country-Param + ytsearch + URL-Preview, Quellen-Glyph-Tiles) → **P5** Kontextmenü (ohne
  Play Next/Add to Queue) + Star + Undo + **Commit-Zeit-Toast-Kette ([Delete files] →
  `gio::File::trash`, Mehrfach-Aggregation)** + Download-Aktionen + Worker-Verdrahtung (Flips
  SRC-4, POD-2, POD-3, POD-5).
- **Paket R · Radio-View (Owner C; nach C/E2, parallel zu P — keine gemeinsamen Dateien).** Dateien:
  `ui/radio/*`. Sequenz: **R1** Presentation+Model → **R2** View+Spalten+Now-playing-Zelle
  (StreamTags-Anschluss; pausierte Station = „—"; Empty-State mit Add-station-CTA — Bedingung des
  Default-AN; Flip RAD-1) → **R3** Filter-Bar + Toolbar → **R4** Add-Dialog (Quellen-Glyph-Tiles;
  Flip RAD-4) → **R5** Kontextmenü (ohne Play Next/Add to Queue) + Star + Undo + Edit-Dialog +
  Klick/Re-Resolve- und Pause-/Reconnect-Anbindung an E2 (Flips RAD-2-Rest, RAD-3, SRC-4-Anteil).
- **Task V · Verdrahtung (ein Owner, nach P2 und R2).** Dateien: `sidebar_presentation.rs` (beide
  NavIcons + Fallbacks), `sidebar_rebuild.rs` (beide Rows + Counts + Gates), `ui/window/window.rs`
  (beide Stack-Seiten + installs; App-Start-/Timer-Trigger des Podcasts-Workers **mit dem
  Grill-Deckel: nur Modul an ∧ ≥ 1 Abo, TTL-Koaleszenz von Start-Check und Stunden-Timer,
  Metered-Gate via `gio::NetworkMonitor` am Trigger**), `ui/window/library_shell.rs`
  (Routing-Zweige), `ui/track_list/track_list_smoke.rs` (Smoke-Quellen). Flips: SRC-1 (inkl.
  Radio-Default-AN + Empty-CTA-Bedingung — den Empty-State liefert R2). TDD: Rebuild-/Routing-Tests,
  Gating-Decision-Units, Display-Smoke.

### Welle 3 — Preferences + Abschluss (ein Owner)

- **Paket S · Preferences.** Dateien: `ui/preferences/preference_podcasts.rs` +
  `preference_radio.rs` (neu), `preference_plugins.rs` (beide Zweige), `preferences/mod.rs`. Flips:
  POD-3-Prefs-Anteil. TDD: Settings-Roundtrips; yt-dlp-Row-/Schalter-Zustände
  (vorhanden/fehlt/Update-Fehler → Subtitle-Anzeige, **nie Auto-Toggle des Settings**) als pure
  Decision-Funktionen + Display-Test.
- **Z1 · Traceability + Headless-Smoke + Ledger.** `check-ux-traceability.sh` grün (SRC-1..4,
  POD-1..5, RAD-1..4); End-to-End-Smoke mit voller Isolation (`dbus-run-session -- xvfb-run -a env
  XDG_DATA_HOME=$(mktemp -d) REPRISE_AUDIO_SINK=fakesink REPRISE_PODCASTS_FIXTURE_DIR=…
  REPRISE_RADIO_FIXTURE_DIR=… REPRISE_YTDLP_BIN=… cargo run`): Module an, Subscribe (RSS + YouTube +
  Station via Fixture), Tabellen + Filter, Play/Resume/Played-Zyklus + Play-next-Angebot,
  ICY-Fanout, Remove/Undo + Commit-Toast-Kette, Prefs. Manueller Pass: echter Stream inkl.
  Pause/Reconnect, echtes yt-dlp, MPRIS im GNOME-Shell-Widget (artUrl, CanGoNext/Prev aus),
  Icon-Optik. Ledger-Zeile in `.superpowers/sdd/progress.md`.

### Verifikation (jeder Commit)

`cargo fmt --check` · `cargo clippy --all-targets --workspace -- -D warnings` · `cargo test
--workspace` · `cargo audit` (akzeptierte Advisory RUSTSEC-2024-0436) · nach Core-Änderungen `cargo
tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` leer · Skript-Gates
`check-architecture.sh`, `check-motion-tokens.sh`, `check-input-parity.sh`,
`check-accessibility-semantics.sh`, `check-display-tests.sh`, `check-ux-traceability.sh`. Nicht
headless verifizierbar (manueller Pass): echte Streams/Feeds, yt-dlp gegen YouTube, ICY realer
Sender, MPRIS-Live-Verhalten, Icon-Optik je Theme.
