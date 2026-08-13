---
slug: concerts
worktree:
branch:
phase: shipped
codex_session:
created: 2026-07-25
---
# Plan: Concerts — Tourdaten für Bibliotheks-Artists

Neuer Smart-Sidebar-Ort **Concerts**: kommende Konzerte aller Bibliotheks-Artists
in einer eigenen Tabellen-View mit Distanz-Filter und externen Ticket-Links.
Datenquellen Bandsintown (primär) und Ticketmaster Discovery (Fallback) hinter
einem `EventProvider`-Trait; Fetch-Infrastruktur nach dem New-Releases-Muster
(Ledger, TTL+Jitter, 1 req/s, Worker-Thread). Das bestehende Release-Popover
wird zum gruppierten **Updates-Popover** (Mockup-Frame 2a): zwei Sektionen
NEW RELEASES + CONCERTS, summiertes Unseen-Badge, Sprungzeilen in die
Vollansichten. Dazu — Grill-Beschlüsse 2026-07-25 — vier weitere v1-Pakete:
**Releases-Vollansicht** (User-Mockup Frame 3a, eigener Sidebar-Ort „Releases"
über dem bestehenden new_releases-Cache, Paket R), **Similar Artists**
(ListenBrainz Labs primär, Paket S), **Systemstandort** (XDG-Location-Portal,
Paket G) und eine **read-only CLI/MCP-Surface** für Concerts (Paket M).
Basis `dev`, eigener Feature-Branch (`feat/concerts`).

## 1. Kontext & Ziel

**Ziel:** Sidebar-Eintrag `Concerts` in der SMART-Sektion mit Live-Zähler;
Hauptfläche = eigene ColumnView (Spalten `Date · Artist · City · Venue ·
Distance · Tickets`), Default-Sortierung Datum aufsteigend, Datum und Distanz
sortierbar; Filter-Pills (Radius / Land / Zeitraum / Quelle) im Stil der
bestehenden Browse-Chips; Ticket-Zelle öffnet die Angebots-URL extern
(`gtk4::UriLauncher`). Daneben ein zweiter Smart-Sidebar-Eintrag **`Releases`**
(Sparkle-Icon, vor Concerts): Tabelle `Date · Title · Artist · Type · Status`
über den bestehenden new_releases-Cache — keine neuen APIs, „In library" wird
lokal zur Query-Zeit bestimmt (8.8). Das Header-Popover (heute „New
Releases", ✦) wird zur gemeinsamen **Updates**-Fläche beider Feeds: Badge =
Summe der ungesehenen Releases und Konzerte (owned Releases zählen nie mit),
Concerts-Sektion zeigt nur Deltas („new near you"), Sprungzeilen führen in
beide Vollansichten (8.7). Alles funktioniert ohne Location, ohne API-Keys
(Feature degradiert mit Hinweis statt Crash) und offline (Cache +
„Updated X ago").

**Einordnung:** Concerts ist das Schwester-Feature von New Releases — es erbt
dessen **Fetch-Infrastruktur-Muster** (Resolution-Ledger, TTL+deterministischer
Jitter, prozessweiter 1-req/s-Limiter, Fixture-Seam, Worker-Thread mit eigener
DB-Connection, Modul-Gate nach NET-1). Beide Vollansichten sind
**Sidebar-Views, keine Popover**; das bestehende Popover wird zur gruppierten
Updates-Fläche erweitert statt dupliziert, seine History-Unterseite entfällt
zugunsten der Releases-Vollansicht (NR-12 → NR-12a). Optisch folgen die
Vollansichten dem Redesign (Frame 1e: Tabellen-Ansicht mit Sidebar; Frame 3a:
Releases-Tabelle mit Status-Pills), das Popover Frame 2a (gruppierte
Update-Karten mit Sektions-Headern); kanonische Design-Quelle ist der
Share-Link, nicht das PDF in `docs/design/`.

**Nicht-Ziele (v1, siehe Abschnitt 13):** keine Kartenansicht, keine
Meilen-Einheit (nur km), kein Kalender-Export, keine Preis-Anzeige, keine
mehreren Locations, keine **Desktop-Benachrichtigungen** für Updates (die App
kennt nur Now-Playing-`gio::Notification`s in `ui/notifications.rs`; ein
Updates-Kanal bleibt die einzige unentschiedene Ausbaustufe N). Similar
Artists, Systemstandort und CLI/MCP sind seit dem Grill **reguläre
v1-Pakete**, keine Ausbaustufen mehr (Beschluss 1 in Abschnitt 15).

## 2. Architekturüberblick & Crate-Schnitt

Leitplanke wie im Release-Popover-Rework: **alle Entscheidungslogik lebt als
pure, testbare Funktionen in `reprise-core`**; GTK-Widgets bleiben dünn und
konsumieren Presentation-Structs. Dateigrenze < 800 Zeilen (Ziel 200–400);
`window.rs` bleibt < 600 (`check-architecture.sh`).

- **`crates/reprise-core`** (pur, kein gtk4/zbus — `cargo tree`-Gate):
  Facade `src/concerts.rs` + Verzeichnis `src/concerts/` (Muster
  `browser.rs`+`browser/`): HTTP-Boundary mit eigenem Rate-Limiter und
  Fixture-Seam, `EventProvider`-Trait, Bandsintown- und Ticketmaster-Provider
  (URL-Builder + Parser pur), Dedupe, Haversine, Nominatim-Geocoding,
  Kandidaten-Auswahl, **Similar-Artists-Quelle (`concerts/similar.rs`,
  Abschnitt 6)**, Refresh-Policy (24 h + Jitter), Backoff-Policy, Pipeline,
  Query/Filter/Sortierung. Dazu `src/db_concerts.rs` (Migration V31) und der
  **Releases-Datenseiten-Umbau** in den bestehenden `artist_news_*`-Modulen
  (owned Releases behalten, Query-Zeit-Abgleich, View-Query — 8.8.1).
  **Keine neuen Dependencies**: ureq, serde_json, chrono, thiserror,
  unicode-normalization sind bereits Core-Dependencies.
- **`crates/reprise-gnome`**: `src/ui/concerts/` (View, Spalten, Modell,
  Filter-Bar, Empty-States, Worker/Runtime, CSS, Presentation) und
  `src/ui/releases/` (View, Spalten, Modell, Filter-Bar, Empty-States, CSS,
  Presentation — dasselbe Schnittmuster, ohne eigenen Worker: der bestehende
  News-Fetch liefert die Daten), `src/ui/strings_concerts.rs` +
  `src/ui/strings_releases.rs`, Preferences-Erweiterung
  (`preferences/preference_concerts.rs`), Sidebar-/Routing-Verdrahtung.
  Das Popover-Modul `src/ui/new_releases/` wird per `git mv` zu
  `src/ui/updates/` (die Shell heißt künftig ehrlich „Updates"; der reine
  Move ist ein eigener mechanischer Task, Dateiliste in 8.7) und erhält
  die Concerts-Sektion; die Core-Domäne behält ihre `artist_news_*`-Namen.
- **`crates/reprise-platform-linux`**: neues **`src/location.rs`** (Paket G,
  Abschnitt 7.3) — One-Shot-Systemstandort über das XDG-Location-Portal;
  zbus liegt dort bereits, `trash.rs` liefert das Portal-/Flatpak-Idiom.
  reprise-gnome konsumiert die API direkt als Crate-Dependency (wie
  `trash`/`device_sync` heute), Aufruf blockierend aus `one_shot_task`.
- **CLI/MCP (Paket M, Abschnitt 2.1):** read-only Concerts-Surface in v1 —
  `reprise-cli concerts list` und MCP-Resource `reprise://concerts`. Beide
  konsumieren ausschließlich die frontend-freie Core-API
  (`concerts::query_events(conn, &ConcertFilter, location, today)`,
  `ConcertFilter`, `ConcertRow`) plus `concerts/config.rs`-Reads; keine
  Keys nötig, kein Fetch — reine Cache-Reads. **Releases bekommt in v1
  bewusst keine CLI/MCP-Surface** (Spec-Scope: nur Concerts).

Modul-Gate: neuer `ModuleDescriptor` **`CONCERTS_MODULE`** in
`crates/reprise-core/src/modules.rs` (`id: "concerts"`,
`default_enabled: false` — NET-1: automatische Netzabrufe sind opt-in; NET-2
braucht keine Übernahmelogik, es gibt keine Bestandsnutzung). Die
Releases-Vollansicht hängt am bestehenden `NEW_RELEASES_MODULE`
(`id: "new_releases"`) — kein zweites Gate, sie ist nur eine neue Sicht auf
denselben Cache.

### 2.1 CLI & MCP (Paket M — read-only, ohne Keys)

**CLI (`crates/reprise-cli`):**

- `cli.rs`: neue Variante `Command::Concerts { action: ConcertsAction }` +
  `enum ConcertsAction { List { /** alle kommenden Events statt des
  persistierten Filters */ #[arg(long)] all: bool, /** Ausgabe kappen */
  #[arg(long)] limit: Option<usize> } }` — `--json` und `--db` sind bereits
  globale Flags.
- `commands/mod.rs`: `pub mod concerts;`-Zeile. `main.rs`: Dispatch-Arm
  `Command::Concerts { action } => commands::concerts::run(&conn, action,
  json)` neben den bestehenden Armen (Z. 71 ff.).
- `commands/concerts.rs` (neu, Muster `commands/library.rs`): liest
  `concerts::config::persisted_filter(conn)` (bzw. `ConcertFilter::default()`
  bei `--all`) + `concerts::config::location(conn)`, ruft
  `concerts::query_events(conn, &filter, location, today)`, kappt auf
  `limit`. Human: eine Zeile pro Event (`2026-10-17  Lorna Shore — Zenith,
  München (DE) · 418 km · https://…`; Distanz nur mit Location, sonst
  weggelassen). JSON via `output::print_json`: `{ "events": [{ "date",
  "starts_at", "artist", "venue", "city", "region", "country",
  "distance_km": null|f64, "ticket_url", "ticket_source", "event_url",
  "provider", "is_similar", "similar_to" }], "filter_applied": bool,
  "latest_fetch_at": i64|null }` (`latest_fetch_at` aus dem Ledger-Max,
  Abschnitt 3). Keine Pfade im Output (Konvention der bestehenden Surface).
- Integrationstest `crates/reprise-cli/tests/concerts.rs` (Muster
  `tests/library.rs`): migrierte Scratch-DB, `concert_events`-Zeilen direkt
  seeden, `--json`/`--limit`/`--all`/leer prüfen; kein Netz.

**MCP (`crates/reprise-mcp`):**

- `server.rs`: `pub const RESOURCE_CONCERTS: &str = "reprise://concerts";`,
  Eintrag in `list_resources` (Beschreibung „Upcoming concerts for library
  artists after saved filters: dates, venues, cities, ticket links. No file
  paths.") und Arm in `read_resource` nach dem
  `RESOURCE_PLAYLISTS`-Idiom (spawn_blocking → `data::list_concerts` →
  `error::serialize_resource`).
- `data.rs`: `pub fn list_concerts(path: &Path) -> Result<ConcertsResource,
  DataError>` + `#[derive(Serialize)] struct ConcertsResource` mit exakt der
  CLI-JSON-Form (eine Serialisierungs-Wahrheit; der Struct lebt in
  `data.rs`, die CLI baut ihr JSON über dieselben Felder). Read-only, keine
  Capability nötig (Klasse der bestehenden Resources).
- Test: `tests/resources.rs` erweitern (Resource gelistet; Read liefert
  gültiges JSON gegen eine geseedete DB; unbekannte URI unverändert).

## 3. Datenmodell & Migration V31

Neue Datei **`crates/reprise-core/src/db_concerts.rs`** (Muster
`db_artist_news_fetch.rs`): `pub(crate) fn migrate_v31(conn)` — idempotenter
Version-Check, `unchecked_transaction`, `execute_batch`, `user_version`-Bump.
`db.rs`: `SUPPORTED_SCHEMA_VERSION` 30 → **31** (Z. 15) + Aufrufzeile nach
`db_artist_news_fetch::migrate_v30` (Z. 677). Migrationstests in
`db_concerts_migration_tests.rs` (Muster `db_artist_news_fetch_migration_
tests.rs` / `db_recent_migration_tests.rs`: Upgrade v30→v31, Idempotenz,
Downgrade-Schutz via `SUPPORTED_SCHEMA_VERSION`-Test).

```sql
-- Resolution-Ledger + Fetch-Zustand je Artist (vereinigt, wie in der Spec:
-- "concert_artists (resolution + last_fetch)"). Key = normalisierter Name,
-- wie artist_news_fetch: Artists ohne MBID sind genau die, die Tracking
-- brauchen.
CREATE TABLE IF NOT EXISTS concert_artists (
  artist_key      TEXT PRIMARY KEY,          -- normalize(name), s. 4.4
  artist_name     TEXT NOT NULL,             -- Anzeigename (MIN(trim(artist)))
  artist_mbid     TEXT,                      -- aus lokalen Tags (tracks.artist_mbid)
  provider        TEXT,                      -- 'bandsintown' | 'ticketmaster' | NULL
  provider_id     TEXT,                      -- Bandsintown: kanonischer Name; TM: attraction id
  mbid_verified   INTEGER NOT NULL DEFAULT 0,-- Provider-mbid == Tag-mbid
  is_similar      INTEGER NOT NULL DEFAULT 0,-- 1 = Similar-Kandidat (Paket S)
  similar_to      TEXT,                      -- Anzeigename der Quelle ("Lorna Shore")
  last_attempt_at INTEGER,                   -- NULL = nie versucht
  last_outcome    TEXT,                      -- 'ok' | 'unmatched' | 'failed'
  events_found    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS concert_events (
  id            INTEGER PRIMARY KEY,
  artist_key    TEXT NOT NULL,
  artist_name   TEXT NOT NULL,
  starts_at     TEXT NOT NULL,               -- ISO-8601 wie vom Provider (lokale Venue-Zeit)
  date_key      TEXT NOT NULL,               -- 'YYYY-MM-DD'-Anteil, Sort-/Dedupe-/Cleanup-Basis
  venue         TEXT NOT NULL,
  city          TEXT NOT NULL,
  region        TEXT,
  country       TEXT,
  latitude      REAL,                        -- NULL erlaubt: Distanz zeigt dann "—"
  longitude     REAL,
  ticket_url    TEXT,                        -- bestes Offer (NULL = kein Offer)
  ticket_source TEXT,                        -- Anzeigename der Quelle ("Eventim", …)
  event_url     TEXT,                        -- Plain-Event-Link (Bandsintown/TM-Seite)
  provider      TEXT NOT NULL,
  is_similar    INTEGER NOT NULL DEFAULT 0,
  similar_to    TEXT,
  fetched_at    INTEGER NOT NULL,
  seen_at       INTEGER,                     -- NULL = ungesehen (Updates-Popover, 8.7)
  dedupe_key    TEXT NOT NULL UNIQUE         -- normalize(date_key|city|venue), s. 4.4
);
CREATE INDEX IF NOT EXISTS idx_concert_events_date   ON concert_events(date_key);
CREATE INDEX IF NOT EXISTS idx_concert_events_artist ON concert_events(artist_key);
```

- **Fetch-Metadaten:** kein eigenes Meta-Objekt — „Updated X ago" liest
  `MAX(last_attempt_at)` aus `concert_artists` (exakt das
  `artist_news_refresh::latest_fetched_at`-Idiom: der Ledger, nicht die
  Ergebnistabelle, sonst sieht eine Bibliothek ohne Konzerte immer „nie
  aktualisiert" aus).
- **Upsert:** `INSERT INTO concert_events … ON CONFLICT(dedupe_key) DO UPDATE`
  (aktualisiert `ticket_url`/`starts_at`/`fetched_at`; `is_similar` wird nur
  0-wärts überschrieben — ein Library-Treffer schlägt einen Similar-Treffer).
  **`seen_at` bleibt beim Upsert unangetastet** (nur ein neu eingefügtes
  Event ist ungesehen); Episodenbeginn-Semantik wie `first_seen` bei den
  Releases (FB-4-Geist).
- **Reconcile statt Replace:** Beim Re-Fetch eines Artists wird die frische
  Event-Menge per Upsert geschrieben und anschließend gelöscht, was der
  Provider nicht mehr liefert: `DELETE WHERE artist_key = ? AND date_key >=
  today AND dedupe_key NOT IN (frische Keys)`. Ein nacktes
  DELETE-then-INSERT würde `seen_at` verlieren und nach jedem Fetch alles
  neu badgen; das Reconcile erhält Seen-Zustände und entfernt abgesagte
  Konzerte trotzdem korrekt.
- **Aufräumen:** `delete_past_events(conn, today)` löscht hart
  `date_key < today` — am Ende jedes Pipeline-Laufs (Muster
  `enforce_retention` am Ende von `refresh_with`). Zusätzlich filtert jede
  Query auf `date_key >= today`, damit auch ohne Lauf nie Vergangenes
  erscheint.
- **Query-Oberfläche** (`concerts/query.rs`, Paket B): `query_events(conn,
  &ConcertFilter, location, today) -> Vec<ConcertRow>` ·
  `count_upcoming(…)` · `query_unseen(…, limit)` (Delta-Zeilen fürs
  Popover) · `count_unseen(…)` (Badge-Beitrag) · `mark_scope_seen(conn,
  &ConcertFilter, today, now)` (stempelt die gesamte Delta-Menge im
  aktuellen Scope — 8.7).
- Settings (bestehende `settings`-Tabelle, `library::settings`): `concerts.
  bandsintown_app_id`, `concerts.ticketmaster_apikey`, `concerts.location_
  lat/lon/name`, `concerts.window_days` (Default 90), `concerts.default_
  radius_km`, `concerts.similar_enabled`, `concerts.similar_count`,
  Filter-Sticky-Keys `concerts.filter.*` (s. 8.4) und `releases.filter.*`
  (s. 8.8.3). Begründung settings statt Keyring: Beschluss 2. Alle
  Concerts-Reads gebündelt in **`concerts/config.rs`** (neu):
  `credentials(conn)`, `location(conn)`, `window_days(conn)`,
  `persisted_filter(conn)`, `similar_config(conn)`.
- **Key-Bootstrapping vor dem Preferences-UI** (die Prefs-Welle kommt per
  Arbeitsreihenfolge zuletzt): `credentials()` liest die Settings-Keys und
  fällt bei leerem Wert auf die Env-Variablen `REPRISE_BANDSINTOWN_APP_ID` /
  `REPRISE_TICKETMASTER_APIKEY` zurück — Entwicklung/Smoke-Läufe setzen Keys
  damit ohne DB-Gefummel (oder per `sqlite3 … "INSERT INTO settings …"`);
  Tests brauchen gar keine Keys (Fixture-Seam 4.1). Der
  „kein Key → Feature degradiert mit Hinweis"-Pfad ist damit ab der
  Datenschicht-Welle testbar, nicht erst mit dem Prefs-UI.

## 4. `EventProvider`-Trait, Bandsintown, Ticketmaster

### 4.1 HTTP-Boundary (`crates/reprise-core/src/concerts/http.rs`, neu)

Analog `musicbrainz.rs`, aber eigener Limiter (Bandsintown/Ticketmaster teilen
sich das MusicBrainz-Budget nicht):

- eigener `static LAST_REQUEST: Mutex<Option<Instant>>` + `wait_for_request_
  slot`-Klon mit 50-ms-Slices, `MIN_REQUEST_INTERVAL = 1 s` (Spec: serielle
  Queue ≤ 1 req/s — gilt providerübergreifend für den gesamten
  Concerts-Verkehr, ein Limiter für alle Provider + Nominatim + die
  Similar-Quellen aus Abschnitt 6).
- `ureq`, `HTTP_TIMEOUT = 15 s`, User-Agent
  `Reprise/{version} ( musicbrainz::CONTACT_URL )` — `CONTACT_URL` ist
  bereits `pub`, wird wiederverwendet (Nominatim-Pflicht: eigener UA).
- Fehlerklassifikation `ProviderError` (thiserror):
  `RateLimited { retry_after: Option<u64> }` (Status 429, `Retry-After`
  geparst), `HttpStatus(u16)`, `Timeout`, `Transport`, `Body`, `Parse`,
  `MissingCredentials`. 404 ist providerabhängig **kein** Fehler, sondern
  „unmatched" (Bandsintown antwortet für unbekannte Artists teils 404/leer).
- **Fixture-Seam** `REPRISE_CONCERTS_FIXTURE_DIR` (+ optionales Log
  `REPRISE_CONCERTS_FIXTURE_LOG`), Routen-Enum wie `FixtureRequest` in
  `musicbrainz.rs`: `bandsintown-artist-{name}.json`,
  `bandsintown-events-{name}.json`, `ticketmaster-attractions-{keyword}.json`,
  `ticketmaster-events-{id}.json`, `nominatim-{query}.json`,
  `listenbrainz-similar-{mbid}.json`, `lastfm-similar-{name}.json`. Kein Test
  macht echtes HTTP; Parser-Tests nutzen Inline-Fixture-Strings (Muster
  `artist_news_parsing`).

### 4.2 Trait (`crates/reprise-core/src/concerts/provider.rs`, neu)

```rust
pub enum ProviderKind { Bandsintown, Ticketmaster }

pub struct ArtistRef<'a> { pub name: &'a str, pub mbid: Option<&'a str> }

pub enum Resolution {
    Resolved { provider_id: String, mbid_verified: bool },
    Unmatched,
}

pub struct ProviderEvent {
    pub starts_at: String, pub date_key: String,
    pub venue: String, pub city: String,
    pub region: Option<String>, pub country: Option<String>,
    pub latitude: Option<f64>, pub longitude: Option<f64>,
    pub ticket_url: Option<String>, pub ticket_source: Option<String>,
    pub event_url: Option<String>,
}

pub trait EventProvider {
    fn kind(&self) -> ProviderKind;
    /// Ein Artist → Provider-Identität (inkl. MBID-Verifikation).
    fn resolve(&self, artist: &ArtistRef) -> Result<Resolution, ProviderError>;
    /// Kommende Events für eine aufgelöste Identität.
    fn events(&self, provider_id: &str) -> Result<Vec<ProviderEvent>, ProviderError>;
}
```

Die Pipeline (Abschnitt 5) arbeitet gegen `&[Box<dyn EventProvider>]`; die
konkreten Provider kapseln nur URL-Bau + `http::get` + Parse. Alle
`parse_*`-Funktionen sind **pur** (`&str -> Result<…, ProviderError>`) und
einzeln getestet.

### 4.3 Bandsintown (`crates/reprise-core/src/concerts/bandsintown.rs`, neu)

- Konstruktion nur mit `app_id` (aus Settings); ohne `app_id` wird der
  Provider gar nicht erst instanziiert (Feature-Degradierung, s. 8.5).
- `resolve`: `GET https://rest.bandsintown.com/artists/{urlencode(name)}?app_id={id}`
  → Antwort u. a. `{"name": "...", "id": 510, "mbid": "…", "url": "…"}`.
  MBID-Verifikation: Tag-MBID vorhanden ∧ Provider-`mbid` vorhanden ∧ gleich
  (case-insensitiv) → `mbid_verified = true`; ungleich → `Resolved` mit
  `mbid_verified = false` („unverified", Spec: „mark unverified on
  mismatch"); Antwort leer/`{"error":"Not Found"}`/404 → `Unmatched`.
  `provider_id` = der von Bandsintown zurückgegebene kanonische Name
  (URL-encodiert wiederverwendbar).
- `events`: `GET https://rest.bandsintown.com/artists/{urlencoded}/events?app_id={id}`
  — Array von Events:

  ```json
  [{ "datetime": "2026-10-17T19:00:00",
     "venue": { "name": "Zenith", "city": "München", "region": "BY",
                "country": "Germany", "latitude": "48.174", "longitude": "11.555" },
     "offers": [{ "type": "Tickets", "url": "https://www.bandsintown.com/t/…", "status": "available" }],
     "lineup": ["Lorna Shore"], "url": "https://www.bandsintown.com/e/…" }]
  ```

  Mapping: `starts_at = datetime`, `date_key = datetime[0..10]` (validiert
  als `NaiveDate`), lat/long tolerant als String **oder** Zahl parsen
  (Bandsintown liefert Strings), erstes Offer mit `status == "available"` →
  `ticket_url`; `ticket_source` via Domain-Mapping (4.5); `event_url = url`
  (Spec-Akzeptanzkriterium: Rows ohne Offer zeigen den Plain-Event-Link).
  Vergangene Events (`date_key < today`) verwirft schon der Parser-Aufrufer.

### 4.4 Ticketmaster (`crates/reprise-core/src/concerts/ticketmaster.rs`, neu)

- Konstruktion nur mit `apikey`.
- `resolve`: `GET https://app.ticketmaster.com/discovery/v2/attractions.json?keyword={urlencode(name)}&apikey={key}`
  → `_embedded.attractions[] { id, name, externalLinks?.musicbrainz[]{url} }`.
  Best match = erste Attraction mit case-insensitiv/trim-gleichem Namen;
  keine → `Unmatched` (kein Fuzzy-Match — falsche Konzerte sind schlimmer als
  fehlende). MBID-Verifikation: wenn `externalLinks.musicbrainz` eine URL mit
  der Tag-MBID enthält → verified, sonst unverified.
- `events`: `GET https://app.ticketmaster.com/discovery/v2/events.json?attractionId={id}&size=50&apikey={key}`
  → `_embedded.events[] { name, url, dates.start.{localDate, localTime},
  _embedded.venues[0].{ name, city.name, state.stateCode?,
  country.{name, countryCode}, location.{latitude, longitude} } }`.
  Mapping: `starts_at = localDate + "T" + localTime.unwrap_or("00:00:00")`,
  `date_key = localDate`, `ticket_url = url` (Ticketmaster-Event-Seite IST
  das Angebot), `ticket_source = "Ticketmaster"`, `event_url = url`.

### 4.5 Dedupe & Normalisierung (`crates/reprise-core/src/concerts/dedupe.rs`, neu)

- `normalize_component(s)`: trim, lowercase, Unicode-NFKD + Combining-Marks
  droppen (Wiederverwendung des `unicode-normalization`-Idioms aus
  `library::group_key`), Mehrfach-Whitespace falten.
- `dedupe_key(date_key, city, venue) -> String` =
  `"{date}|{norm(city)}|{norm(venue)}"` — Spec: merged results by normalized
  `(date, city, venue)`. Uhrzeit gehört bewusst NICHT hinein (Provider
  differieren in Minuten).
- `merge(events: Vec<ProviderEvent>) -> Vec<ProviderEvent>`: pure Funktion,
  stabile Präferenz Bandsintown-vor-Ticketmaster bei gleichem Key (bessere
  Offer-Vielfalt); Persistenz-seitig sichert `UNIQUE(dedupe_key)` dasselbe
  über Runs hinweg.
- `ticket_source_label(url) -> Option<String>`: pure Domain-Tabelle
  (`eventim.` → "Eventim", `ticketmaster.` → "Ticketmaster", `bandsintown.com`
  → "Bandsintown", …) + Fallback: registrierbarer Domain-Teil kapitalisiert.
  (Bandsintown-Offers tragen keinen Vendor-Namen — der Quellname in der
  Ticket-Zelle kommt aus der URL; ausgewiesene Interpretation der Spec.)

### 4.6 Resolution-Cache (`crates/reprise-core/src/concerts/resolution.rs`, neu)

- `load(conn, artist_key) -> Option<StoredResolution>`;
  `store(conn, …)` schreibt `provider/provider_id/mbid_verified/last_outcome`.
- Negative Treffer (`last_outcome = 'unmatched'`) werden gecacht und erst
  nach `RESOLUTION_RETRY_SECONDS = 7 Tage` erneut versucht (Spec: „so misses
  aren't retried every run"; 7 d = `FETCH_TTL_SECONDS`-Idiom der News).
  `failed` (Netz/5xx) zählt NICHT als negativ — nächster Lauf versucht es
  wieder (TTL 24 h regelt die Frequenz).
- MBID-Priorität: `tracks.artist_mbid` (V13, Index vorhanden) ist die
  Wahrheit; die Resolution verifiziert nur dagegen, sie überschreibt nie
  Tag-Daten.

## 5. Fetch-Orchestrierung

### 5.1 Kandidaten (`crates/reprise-core/src/concerts/candidates.rs`, neu)

Spec: nur Artists mit Plays in den letzten 90 Tagen (konfigurierbar). Quelle
ist `listen_events` (V24: `artist`, `artist_mbid`, `played_at`, Index
`idx_listen_events_played_at`):

```sql
SELECT MIN(trim(artist)), MAX(artist_mbid), COUNT(*) AS plays
FROM listen_events
WHERE played_at >= :cutoff AND trim(artist) <> ''
GROUP BY lower(trim(artist))
ORDER BY plays DESC, lower(MIN(trim(artist))) ASC
```

- `cutoff = now − concerts.window_days · 86 400` (Setting, Default 90).
- Pro Lauf höchstens `MAX_ARTISTS_PER_RUN = 30` tatsächliche Fetches
  (2 Requests/Artist ⇒ ≤ 60 s Netzzeit bei 1 req/s): TTL-fällige Kandidaten
  werden nach dem `artists_for_fetch`-Staleness-Idiom geordnet — nie
  versuchte zuerst (`last_attempt_at IS NULL`), dann älteste zuerst.
  Übersprungene sind beim nächsten Lauf einfach die ältesten.
  Similar-Kandidaten (Abschnitt 6) zählen in dieselbe Kappe und werden
  hinter den Library-Kandidaten eingereiht (Library gewinnt bei
  Budget-Knappheit).
- Die Top-N-Auswahl derselben Query dient auch als Similar-Seed-Liste
  (`pub(crate) fn seed_artists(conn, cutoff, limit)` — Abschnitt 6).

### 5.2 Refresh-Policy (`crates/reprise-core/src/concerts/refresh.rs`, neu)

- Pro Artist: `artist_due(last_attempt_at, now, force)` — TTL
  `FETCH_TTL_SECONDS = 24 h`; `force = true` („Fetch now") ignoriert TTL,
  nicht aber den Resolution-Negativ-Cache aus 4.6 (7-Tage-Sperre gilt auch
  für force — bewusst: ein Klick auf „Fetch now" soll nicht 30 bekannte
  Nieten erneut abfragen; Neubewertung erzwingt erst ein Tag-/MBID-Wechsel
  oder Ablauf).
- Global: `refresh_due(latest_attempt, now, jitter)` mit Basisintervall
  **24 h** + deterministischem Jitter `[0, 2 h]` aus FNV-1a über den DB-Pfad.
  Der FNV-Helfer `fnv1a_64` wird in `artist_news_refresh.rs` `pub(crate)`
  gemacht und wiederverwendet (eine Zeile, gehört dem Fundament-Paket);
  Rückwärts-Uhr-Semantik identisch zu `artist_news_refresh::refresh_due`.

### 5.3 Backoff (`crates/reprise-core/src/concerts/backoff.rs`, neu — NEU gegenüber News)

Pure Policy, von `http.rs`-Aufrufern (Pipeline) angewandt:

```rust
/// attempt ist 1-basiert und zählt den GESCHEITERTEN Versuch.
/// None = aufgeben (Outcome 'failed' im Ledger, weiter mit nächstem Artist).
pub fn backoff_delay(attempt: u32, retry_after: Option<u64>) -> Option<Duration>
```

- Nur bei `RateLimited` und `HttpStatus(500..=599)`; alle anderen Fehler
  scheitern sofort (ein Timeout wiederholen lohnt bei 15-s-Timeout nicht).
- Basis `2 s`, Verdopplung pro Versuch (2 s, 4 s, 8 s), Cap
  `MAX_BACKOFF = 60 s`, `MAX_ATTEMPTS = 3`.
- `Retry-After` (Sekunden) gewinnt, wenn größer als der berechnete Delay,
  gedeckelt auf `MAX_BACKOFF`; ein `Retry-After` jenseits des Caps bricht den
  **gesamten Lauf** ab (Provider bittet um Ruhe — Rest wandert per Staleness
  in den nächsten Lauf).
- Das Warten selbst läuft im Worker-Thread in 50-ms-Slices mit
  Abbruch-Prüfung (Muster `wait_for_request_slot`), damit App-Quit nicht
  60 s hängt.

### 5.4 Pipeline (`crates/reprise-core/src/concerts/pipeline.rs`, neu)

`pub fn refresh(conn, providers: &[Box<dyn EventProvider>], today, now, force)
-> Result<RefreshSummary, ConcertError>` — seriell (die 1-req/s-Queue ist der
Takt). Kandidatenliste = Library-Kandidaten (5.1) + Similar-Kandidaten
(Abschnitt 6, nur bei `concerts.similar_enabled`), gemeinsam gekappt auf
`MAX_ARTISTS_PER_RUN`. Je Kandidat:

1. Resolution-Cache lesen (4.6); frisch-negativ → skip (`unmatched` bleibt).
2. Kein Cache/abgelaufen: Provider-Kette — **Bandsintown zuerst; Ticketmaster
   nur, wenn Bandsintown fehlt (kein app_id) oder `Unmatched` liefert**
   (Beschluss 4: Fallback statt additivem Merge; Dedupe bleibt als
   Sicherheitsnetz für Provider-Wechsel zwischen Runs).
3. `events(provider_id)` → Parser → Vergangenes verwerfen → `merge` →
   Transaktion: Upsert per `dedupe_key` + Reconcile-Delete der nicht mehr
   gelieferten Zukunfts-Zeilen des Artists (Abschnitt 3 — `seen_at`
   überlebt), Ledger `store(…, last_outcome, events_found)`. Similar-Zeilen
   tragen `is_similar = 1` + `similar_to`; die 0-wärts-Regel des Upserts
   lässt einen späteren Library-Treffer gewinnen.
4. Fehlerpfad: Backoff (5.3); nach `MAX_ATTEMPTS` Outcome `failed`,
   `events_found` unverändert lassen (Cache bleibt sichtbar — Offline-Fall).
5. Am Lauf-Ende: `delete_past_events(conn, today)`.

`RefreshSummary { attempted, resolved, unmatched, failed, events_upserted }`
für Logging + Footer-Status. **Kein Live-Fetch bei Track-Wechsel oder
Navigation** — die einzigen Trigger sind View-Open-Staleness, der stündliche
Timer-Check (der nur `refresh_due` mit 24-h-Basis prüft) und „Fetch now"
(alle in 5.5); jede andere Stelle liest ausschließlich Cache.

### 5.5 Worker (`crates/reprise-gnome/src/ui/concerts/concerts_worker.rs`, neu)

1:1 nach `artist_news_worker.rs`: `ConcertsRuntime { enabled:
Rc<Cell<bool>>, worker: async_channel::Sender<ConcertsRequest>, subscribers:
EnabledSubscribers-Klon }`, `setup(conn)` liest `modules::is_enabled(conn,
&CONCERTS_MODULE)`, eigener `std::thread` „reprise-concerts" mit eigener
Connection via `db::open_migrated(database_path)`, Antworten über
`async_channel` + `glib::spawn_future_local` auf der UI-Seite.
`ConcertsRequest { generation: u64, force: bool, response: Sender<…> }` —
die Antwort trägt `RefreshSummary` bzw. den klassifizierten Fehler; das
anschließende Neuladen der Zeilen macht die View selbst über
`concerts::query_events` (Haupt-Connection, reiner Cache-Read). Trigger:

- **View-Open:** `refresh_due(latest_attempt, now, jitter)` → höchstens ein
  Hintergrund-Fetch (`force = false`).
- **Stündlicher Check:** `glib::timeout_add_seconds_local(3600, …)` im
  View-Lebenszyklus, gekoppelt an die Enabled-Subscription (Start/Stop wie
  Beschluss 8 des Release-Rework); Callback nur `!fetching && refresh_due`.
- **„Fetch now":** `force = true`, Spinner im Footer, Inline-Failure statt
  Toast (NR-6-Idiom).
- Modul aus → Worker nimmt keine Requests an (`request()` prüft `enabled`),
  View wird aus der Sidebar entfernt (8.1).

## 6. Similar Artists (Paket S — v1, default OFF)

Schema, Filter und Zellen-Tag sind ab V31 da (is_similar/similar_to in beiden
Tabellen); Paket S liefert den Fetch-Pfad dazu. Default bleibt AUS
(`concerts.similar_enabled`, SwitchRow in Paket D) — NET-1-konform ist das
eine zweite, eigene Zustimmung innerhalb des Concerts-Moduls.

**Quelle (Beschluss 8): ListenBrainz Labs similar-artists (MBID-basiert)
primär**, Last.fm `artist.getSimilar` (`match ≥ 0.4`) NUR als Namens-Fallback
für Seeds ohne Tag-MBID und nur, wenn der Compile-Time-Key gebündelt ist
(`scrobbling::lastfm::BUNDLED_API_KEY`, das bestehende
`REPRISE_LASTFM_API_KEY`-Idiom aus `library/lastfm_stats.rs`).

**`crates/reprise-core/src/concerts/similar.rs` (neu):**

- URL-Builder + Parser pur, Abruf über `concerts::http::get` (gleicher
  1-req/s-Limiter, gleicher Fixture-Seam):
  - `listenbrainz_similar_url(mbid)` →
    `https://labs.api.listenbrainz.org/similar-artists/json?artist_mbids={mbid}&algorithm={LB_SIMILAR_ALGORITHM}`.
    `LB_SIMILAR_ALGORITHM` ist eine Konstante (aktuell dokumentierter Wert:
    `session_based_days_7500_session_300_contribution_5_threshold_10_limit_100_filter_True_skip_30`);
    **URL-Form + Algorithmus-String sind vor Paket S einmal gegen die echte
    Labs-API zu verifizieren** (Risiko-Item in 13 — Labs ist explizit ein
    Experimental-Endpoint). Anders als `library/listenbrainz.rs` (eigener
    ureq-Agent) läuft der Abruf hier über die Concerts-HTTP-Boundary, weil
    er Teil des gedrosselten Pipeline-Verkehrs ist.
  - `parse_listenbrainz_similar(body) -> Result<Vec<SimilarArtist>, ProviderError>`
    — tolerantes Parsen eines Arrays von Objekten mit `artist_mbid`, `name`,
    `score`; Sortierung Score absteigend; `SimilarArtist { name, mbid:
    Option<String>, score: f64 }`.
  - `lastfm_similar_url(name, api_key, limit)` →
    `https://ws.audioscrobbler.com/2.0/?method=artist.getsimilar&artist={urlencode(name)}&api_key={key}&format=json&limit={limit}`;
    `parse_lastfm_similar(body)` — `similarartists.artist[] { name, mbid?,
    match }`, `match` tolerant als String ODER Zahl parsen, Schwelle
    `LASTFM_MIN_MATCH = 0.4`.
  - Fixture-Routen: `listenbrainz-similar-{mbid}.json`,
    `lastfm-similar-{name}.json` (4.1).
- `pub fn similar_candidates(conn, seeds: &[SeedArtist], fetch, config)
  -> Vec<SimilarCandidate>` — pro Seed: MBID vorhanden → ListenBrainz;
  sonst Last.fm-Fallback (nur mit Bundle-Key, sonst Seed überspringen).
  Kappung pro Seed auf `concerts.similar_count` (Default **10**, max **25**,
  SpinRow in D), Gesamt-Cap `MAX_SIMILAR_ARTISTS = 50` nach Score-Ordnung.
  **Dedupe gegen die Library-Artist-Keys** (normalize-Vergleich gegen die
  Kandidaten-Query aus 5.1) und untereinander VOR der Resolution — ein
  Similar-Kandidat, der schon Bibliotheks-Artist ist, wird verworfen.
- Seeds: Top-`SIMILAR_SEEDS = 5` Play-Artists der Kandidaten-Query
  (`candidates::seed_artists`, 5.1) — bewusst Konstante statt Setting
  (YAGNI; `similar_count` ist der wirksame Regler).
- Integration (Pipeline, 5.4): Similar-Kandidaten laufen durch **dieselben
  Provider, denselben Ledger, dieselbe TTL** — `concert_artists`-Zeilen mit
  `is_similar = 1`, `similar_to` = Seed-Anzeigename; Events erben beides.
  Schalter aus → keine neuen Similar-Fetches; vorhandene Zeilen bleiben bis
  Reconcile/Cleanup und sind über den Source-Filter ausblendbar.

**UI-Folgen (in C/D enthalten):** Artist-Zelle trägt bei `is_similar` die
dimme Caption „similar to {seed}" (8.3); die Source-Filter-Pill ist sichtbar,
sobald Similar aktiviert ist ODER `is_similar`-Zeilen existieren (8.4);
Preferences-Rows in Paket D (9.). **CONC-6 wird in v1 geflippt** (Task C3).

## 7. Location

### 7.1 Manuelle Stadt — Nominatim (`crates/reprise-core/src/concerts/geocode.rs`, neu)

- `geocode_url(query)` →
  `https://nominatim.openstreetmap.org/search?q={urlencode}&format=json&limit=1`;
  Abruf über `concerts::http::get` (gleicher 1-req/s-Limiter, **Pflicht-UA**
  aus 4.1 — Nominatim-Policy), `parse_geocode(body) ->
  Option<GeocodedLocation { lat, lon, display_name }>` pur (Response:
  `[{"lat":"48.13","lon":"11.57","display_name":"München, Bayern, …"}]`,
  lat/lon als Strings).
- Ausgelöst NUR durch Apply in den Preferences (9.), einmalig; Ergebnis in
  Settings (`concerts.location_lat/lon/name`). Kein Geocoding im
  Fetch-Pfad, keine Re-Geocodierung beim Start.

### 7.2 Haversine (`crates/reprise-core/src/concerts/geo.rs`, neu)

`pub fn haversine_km(lat1, lon1, lat2, lon2) -> f64` — pure Formel
(Erdradius 6 371.0 km), Unit-Tests mit bekannten Paaren (München↔Berlin
≈ 504 km ± 1 %). Distanz wird zur **Query-Zeit lokal** berechnet (8.3), nie
persistiert (Location-Wechsel invalidiert sonst den Cache).

### 7.3 Systemstandort (Paket G — v1)

**`crates/reprise-platform-linux/src/location.rs` (neu)** + `mod`-Zeile in
`lib.rs` dort. GeoClue2 über das **XDG-Location-Portal** via zbus
(blocking-Idiom exakt wie `trash.rs`):

- Ein Portal-Pfad für Host UND Flatpak: `zbus::blocking::Connection::
  session()` → Proxy auf `org.freedesktop.portal.Desktop` /
  `/org/freedesktop/portal/desktop`, Interface
  `org.freedesktop.portal.Location`. xdg-desktop-portal bedient beide Welten
  (derselbe Grund, aus dem der Trash-Portal-Zweig existiert); die
  `/.flatpak-info`-Probe aus `trash.rs` wird als geteilter Helfer
  wiederverwendet, hier nur für den Fehlertext-Kontext (Sandbox vs. Host),
  nicht als Backend-Weiche.
- Ablauf One-Shot `pub fn current_location(timeout: Duration) ->
  Result<PortalLocation, String>`:
  1. `CreateSession(options { session_handle_token, accuracy:
     ACCURACY_CITY })` → Session-`ObjectPath`. `ACCURACY_CITY = 2u32`
     (Portal-Enum NONE=0 … EXACT=5; CITY reicht — wir rechnen Radien in
     Dutzenden km).
  2. `LocationUpdated`-Signal des Location-Interfaces abonnieren
     (blocking Signal-Iterator, auf die eigene Session gefiltert), DANN
     `Start(session_handle, parent_window: "", options { handle_token })` —
     Reihenfolge wichtig, sonst kann das erste Update verloren gehen.
  3. Erstes `LocationUpdated` innerhalb `timeout` (Default 30 s) nehmen →
     `PortalLocation { latitude: f64, longitude: f64, accuracy_m:
     Option<f64> }`; danach `Close` auf `org.freedesktop.portal.Session`
     der Session — kein Tracking, kein Daemon.
  4. Jeder Fehlerpfad (kein Portal-Dienst, Nutzer lehnt ab, Timeout,
     kaputtes Vardict) → `Err(String)` mit benanntem Schritt
     (`trash.rs`-Fehlertext-Idiom). Ein Fehler ist final; die manuelle
     Stadt (7.1) bleibt der Weg.
- Pure, headless testbare Helfer (Muster `backend_for`/`portal_result`):
  `location_from_vardict(&HashMap<String, OwnedValue>) -> Option<(f64, f64,
  Option<f64>)>`, `ACCURACY_CITY`-Konstante, Timeout-Policy. Der eigentliche
  D-Bus-Roundtrip ist nicht headless testbar (manueller Pass, Z1-Checkliste).
- **Prefs-Anteil (Paket D):** Button „Use current location" neben der
  City-Row → `one_shot_task::spawn("reprise-location", …)` →
  `current_location()`; Erfolg schreibt **dieselben Settings-Keys**
  (`concerts.location_lat/lon/name` mit `name = "Current location"`),
  Fehler → Fehlerzeile als Row-Subtitle (Nominatim-Fehlerpfad-Idiom). Core
  und UI kennen weiter nur „es gibt lat/lon oder nicht".
- **Ohne Location funktioniert alles:** Distance-Spalte zeigt „—",
  Radius-Pill ist disabled mit Tooltip (8.4), Sortierung nach Distanz
  behandelt `None` als `+∞` (stabil ans Ende).

## 8. UI

### 8.1 ViewSource, Sidebar, Badges

- **`crates/reprise-core/src/view_source.rs`:** Varianten
  `ViewSource::Concerts` (`label() = "concerts"`) und
  `ViewSource::Releases` (`label() = "releases"`) + Test-Erweiterung.
- **`crates/reprise-core/src/browser.rs`:** `BrowserPlace::Concerts` und
  `BrowserPlace::Releases` analog `MyStats` (alle `match`-Arme: kein
  Album/Artist/Track-Kontext ⇒ `None`; `From<ViewSource>`-Paare).
  **`browser/navigation.rs`:** `SidebarTarget::Concerts` +
  `SidebarTarget::Releases` + Intent-/`sidebar_place`-Arme;
  **`ui/nav_history.rs`:** `intent_for`-Arme →
  `NavigationIntent::Sidebar(SidebarTarget::…)`. Session:
  `deserialize_optional_browser_place` ist bereits nachsichtig — ein
  Downgrade-JSON mit `"Concerts"`/`"Releases"` fällt auf die Library-Root
  zurück, kein Sondercode nötig (Roundtrip-Tests neu→neu).
- **Sidebar** (`ui/sidebar/sidebar_rebuild.rs`): zwei Rows in der
  SMART-Sektion direkt vor `My Stats`, **Reihenfolge laut Mockup: Releases
  vor Concerts**. Beide gated (Gate-Idiom der Conversions-Row, Z. 187–195;
  Beschluss 6): Releases nur bei aktivem `NEW_RELEASES_MODULE`, Concerts nur
  bei aktivem `CONCERTS_MODULE`. Counts im bestehenden Count-Block der
  `rebuild()`:
  `concerts::count_upcoming(conn, &persisted_filter(conn), today)` bzw.
  `artist_news::count_releases_view(conn, &persisted_releases_filter(conn),
  today)` (8.8.1).
- **Badge-Zählweise (Beschluss 5, gilt für BEIDE Views):** Es gibt zwei
  Zähler mit klar getrennten Rollen. **Sidebar-Badge** = genau die Zeilen,
  die das Öffnen der View zeigt — nach persistierten Filtern
  (Smart-Playlist-Badge-Parität: „the badge always matches what opening the
  list shows", `sidebar_rebuild.rs` Z. 81–86); kein FB-4-„neu seit
  last_viewed" — die Sidebar benennt einen Ort, kein Ereignis. Mockup-Beleg
  Frame 3a: „8" = „8 releases" bei aktivem Not-in-library-Chip.
  **Neuheit** (ungesehene Einträge) ist der Kanal des Updates-Popovers und
  seines Header-Badges (8.7); owned Releases zählen dort nie mit (8.8.1).
  Refresh-Anstoß nach Fetch-Ende über den bestehenden
  `sidebar.refresh(reason)`-Pfad (Rebuild-on-refresh, kein inkrementelles
  Update).
- **`ui/sidebar/sidebar_presentation.rs`:** `NavIcon::Concerts` mit
  `icon_name() = "ticket-symbolic"` und Laufzeit-Fallback
  `IconTheme::has_icon` → `"x-office-calendar-symbolic"`; `NavIcon::Releases`
  mit `"star-new-symbolic"` (Sparkle-Charakter des Mockups) und Fallback
  `"starred-symbolic"` — beides in `build_nav_row`
  (Fallback-Ketten-Idiom aus dem Release-Rework 3.7; wir bündeln keine neuen
  Icons: Akzeptanzkriterium „existing symbolic set").

### 8.2 Routing & Content-Stack

- **`ui/window/window.rs`:** `content_stack.add_named(…, Some("concerts"))`
  und `add_named(…, Some("releases"))` neben `"stats"` (Z. 330–333);
  Konstruktion der Views + Concerts-Runtime davor. window.rs-Budget (< 600)
  beachten: Aufbau in `ui/concerts/mod.rs::install(…)` bzw.
  `ui/releases/mod.rs::install(…)` kapseln, window.rs erhält nur je 3–4
  Zeilen. Die Releases-View erhält denselben `OnShowAlbum`-Callback wie das
  Popover (window.rs Z. 400 — „Show in library"-Parität, 8.8.2).
- **`ui/window/library_shell.rs`::wire_source_routing:** Zweige
  `ViewSource::Concerts` und `ViewSource::Releases` analog `MyStats`
  (Z. 188–190): `view.refresh(&conn)` (Cache-Read + Staleness-Trigger) +
  `content_stack.set_visible_child_name(…)`.
- Smoke-Parität: `track_list_smoke::parse_smoke_source` um `"concerts"` und
  `"releases"` erweitern (REPRISE_SMOKE_SOURCE-Konvention,
  `arm_smoke_my_stats`-Idiom).

### 8.3 ConcertsView & Tabelle (`crates/reprise-gnome/src/ui/concerts/`, neu)

**Beschluss 3 (Abweichung von der Spec „reuse the existing table
component"):** `track_list/` ist hart auf Track-Zeilen verdrahtet (Queries,
Selektion, Aktivierung, 500er-Fenster, Kontextmenü) — Concerts bekommt eine
**eigene, kleinere ColumnView nach demselben Idiom** statt eines Umbaus der
TrackList. Ebenfalls bewusst: **kein windowed Model** — die Kardinalität ist
zwei bis drei Größenordnungen kleiner (Dutzende bis wenige Hundert Zeilen);
`query_events` lädt alle kommenden Events, Filter + Sortierung sind pure
Rust-Funktionen über den `Vec<ConcertRow>`.

Dateien (Ziel je 150–350 Z.):

- `mod.rs` — Modul-Deklarationen + `install(window-Kontext) -> ConcertsView`
  (alle `mod`-Zeilen von Anfang an, damit parallele Pakete die Datei nicht
  mehr anfassen müssen).
- `concerts_view.rs` — Aufbau: äußerer `GtkBox` = Filterzeile (8.4) +
  `GtkStack` (Seiten `"list"` = ScrolledWindow+ColumnView, `"status"` =
  geteilte `adw::StatusPage`) + Footer-Zeile (8.6); `refresh(conn)` lädt
  Zeilen, wählt Empty-State, aktualisiert Zählung; Staleness-Trigger beim
  Öffnen; Stack-Crossfade mit Standard-Token; Hintergrund-Fetch-Ergebnis
  spielt **hart** ein (MOT-2: kein Row-Fade, Zählung/Footer wechseln in
  place).
- `concerts_model.rs` — `ConcertObject` (glib-Wrapper um
  `reprise_core::concerts::ConcertRow`), `gio::ListStore` +
  `SingleSelection` (keine Mehrfachaktionen ⇒ kein `MultiSelection`).
- `concerts_columns.rs` — sechs Spalten über `SignalListItemFactory`
  (setup/bind/unbind, Label-Recycling wie `track_list_columns::
  append_column`): **Date** (formatiert „Fri, Oct 17" + Jahr wenn ≠ aktuelles
  Jahr; `set_id("date")`), **Artist** (Name + bei `is_similar` dim
  Caption-Zeile „similar to {seed}" — Paket S liefert die Daten), **City**
  (mit Region/Land-Tooltip), **Venue**, **Distance** (rechtsbündig-tabular,
  „418 km" / „—"; `set_id("distance")`), **Tickets** (flacher Link-Button
  mit Quellname, s. 8.5). Nur Date/Distance bekommen den Dummy-`CustomSorter`
  + Header-Klick-Verdrahtung (`wire_sort_clicks`-Idiom, Re-Sort in Rust statt
  SQL-Reload); Default Datum aufsteigend; Distanz-Sort mit `None`-ans-Ende.
- `concerts_presentation.rs` — pure Funktionen, headless getestet:
  `format_event_date(date_key, today)`, `format_distance_km(Option<f64>)`,
  `row_distance(location: Option<(f64,f64)>, event) -> Option<f64>`
  (Haversine-Aufruf), `sort_rows(rows, key, direction)`,
  `count_line(shown, total)`, `ticket_button_label(…)`,
  `updated_ago(latest_attempt, now)` (Formulierungs-Klon von
  `new_releases_updated_ago`, eigene Strings).
- `concerts_filter_bar.rs` (8.4), `concerts_empty_state.rs` (8.5),
  `concerts_worker.rs` (5.5), `css.rs` — Registrierung in
  `style/mod.rs::app_css()` (+ bestehender
  `app_css_contains_every_feature_section`-Test wächst mit).

**Zeilen-Interaktion (ausgewiesene Entscheidung, Vertrag = CONC-3):**
Doppelklick/Enter auf eine Row =
Ticket-Link öffnen (bzw. `event_url` ohne Offer) — NAV-4-Analogie
„Aktivierung = Primäraktion der Zeile"; ein Play-Pfad existiert hier nicht.
Einfachklick selektiert. Der Tickets-Button in der Zelle tut dasselbe für
Maus-Nutzer sichtbar (Tastaturparität: Row-Aktivierung genügt, ACC-Skripte
`check-input-parity.sh`/`check-accessibility-semantics.sh` beachten).
`gtk4::UriLauncher::launch` mit Fehler-Toast bei Launch-Fehlschlag (FB-1:
ein Ereignis-Toast).

### 8.4 Filterzeile (FIL-Regeln als Vorbild, neue CONC-Regeln als Vertrag)

FIL-1a/FIL-2/FIL-3/FIL-6 sind wörtlich auf **Track-Quellen** gemünzt;
Concerts übernimmt ihr Muster als eigene Regeln (10.). Umsetzung
(`concerts_filter_bar.rs`):

- **Permanenter Header** über der Liste, feste Mindesthöhe
  (`FILTER_BAR_MIN_HEIGHT`-Idiom aus `browse_bar.rs` Z. 36–40 — kein
  Layout-Shift beim Aktivieren, FIL-2-Geist). Idle: „+ Add filter"-Pill +
  neutrale Gesamtzahl rechts (dim, caption). Aktiv: „FILTER"-Label + Chips +
  akzentuierte Trefferzahl „5 of 23 concerts" + „Clear all ×".
- **Chips = eine Wahrheit** (FIL-1a-Geist): jede aktive Einschränkung ist ein
  Chip mit eigenem ×-Klickziel ≥ 20 px; CSS-Klassen `.reprise-filter-chip`
  werden wiederverwendet (`CHIP_CSS_CLASS` wird dafür `pub(in crate::ui)` —
  eine Fundament-Zeile in `browse_bar.rs`; **kein** Umbau der browse_bar
  selbst, sie bleibt track-query-gekoppelt).
- **Facetten** (2-Seiten-Popover FACET→VALUE nach `browse_chooser`-Idiom,
  eigener kleiner Chooser):
  - `Radius`: off / 50 / 100 / 250 / 500 km. Ohne Location **disabled** mit
    Tooltip „Set a location in Preferences" (Spec-Wortlaut; Tooltip-Regeln
    Sektion M gelten).
  - `Country`: `DISTINCT country` aus dem Cache, alphabetisch.
  - `Date range`: All upcoming / Next 30 days / Next 3 months / Next 6 months.
  - `Source`: Library artists only / Include similar artists. Die Pill ist
    sichtbar, sobald Similar aktiviert ist ODER `is_similar`-Zeilen
    existieren (kein toter Filter; Beschluss 8/B4 — mit Paket S in v1 ist
    das der Normalfall nach dem Einschalten des Schalters).
- **Persistenz:** sticky über Sessions in Settings-Keys
  (`concerts.filter.radius_km/country/horizon/include_similar`) —
  FIL-7-Parität und Voraussetzung für Badge = View-Count (Beschluss 5).
- 0 Treffer BEI aktiven Filtern → StatusPage mit genau einem Schritt
  „Show all N concerts" (= Clear all; FIL-6-Geist, führt garantiert zu
  Inhalt).

### 8.5 Empty-/Status-Zustände (`concerts_empty_state.rs`)

Pure `concerts_empty_state_for(row_count, has_filter, has_credentials,
never_fetched) -> ConcertsEmptyState` + StatusPage-Mutation (ein geteiltes
`adw::StatusPage`, `track_list_empty_state`-Idiom):

| Zustand | Bedingung | StatusPage |
|---|---|---|
| `List` | rows > 0 | — (Liste sichtbar) |
| `NoCredentials` | kein `bandsintown_app_id` UND kein `ticketmaster_apikey` | „Concerts needs an API key" + Untertitel (Bandsintown app_id ODER Ticketmaster key) + Button „Open Preferences" (Deep-Link Plugins-Seite, LYR-3-Idiom). Fetch-Pfad ist gleichzeitig hart deaktiviert — Feature degradiert im Status, crasht nie (Spec). |
| `NeverFetched` | Ledger leer, Keys vorhanden | „No concert data yet" + genau ein Button „Fetch now" |
| `NoResults` | 0 rows, Filter aktiv | FIL-6-Analog: ein Button „Show all N concerts" |
| `Empty` | 0 rows, ohne Filter, gefetcht | „No upcoming concerts for your artists" + „Fetch now" (FB-5a-Ton: ✓-Charakter, ein nächster Schritt) |

Offline ist **kein** Empty-State: Liste rendert aus dem Cache, der Footer
sagt „Updated X ago", ein fehlgeschlagener Hintergrund-Fetch zeigt die
Inline-Failure-Zeile im Footer (NR-6-Idiom), nie einen Toast-Regen (FB-3).

### 8.6 Footer

Schmale Zeile unter der Liste: links „Updated 2 h ago" (bzw. „Updated just
now"), rechts Ghost-Button „Fetch now" mit Spinner-`GtkStack` und
Inline-Failure-Text (Klon des Popover-Footer-Musters). Während des Fetch
bleibt die Liste voll bedienbar (kein Overlay, kein Jank — alles Netz läuft
im Worker-Thread, 5.5).

### 8.7 Updates-Popover (Mockup-Frame 2a)

Das bestehende Release-Popover wird zur gruppierten Updates-Fläche — **kein
zweites Header-Icon**; Fetch-, Badge- und Footer-Infrastruktur werden
wiederverwendet.

**Datei-Move (mechanischer Task, eigener Commit):**
`crates/reprise-gnome/src/ui/new_releases/` → `crates/reprise-gnome/src/ui/
updates/` per `git mv` — betroffen: `mod.rs`, `popover.rs`, `popover_
tests.rs`, `badge.rs`, `release_row.rs`, `history_page.rs`,
`release_cover.rs`, `css.rs` plus alle `use`-/Pfad-Referenzen
(`ui/mod.rs`, `ui/window/window.rs`, `ui/artist_news/artist_news_worker.rs`
Fallback-Accent-Pfad, `ui/preferences/*`, `style/mod.rs`) — rein
compilergeführt, keine Verhaltensänderung. `strings_news.rs` und die
Core-Module (`artist_news_*`) behalten ihre Namen: die Domäne heißt weiter
Artist-News, nur die UI-Shell heißt Updates. (`history_page.rs` wird
mitbewegt und erst in U3 entfernt — der Move bleibt semantikfrei.)

**Shell (`ui/updates/popover.rs`, Umbau):** Kopfzeile „UPDATES"; darunter
zwei Sektionen mit Sektions-Headern:

- **NEW RELEASES** — die bestehenden Release-Rows (`release_row.rs`),
  Verhalten unverändert (NR-10/NR-11/NR-13). Owned Releases erscheinen nach
  dem Fetcher-Umbau (8.8.1) ebenfalls als Rows — `release_row` kann das
  heute schon (Chip `In library`, Primäraktion „Show in library" mit
  NR-13-Carve-out); sie zählen nur nie ins Badge (NR-9a).
- **CONCERTS** — Untertitel „new near you" (bei aktivem Radius; sonst
  „newly announced" — pure `concerts_section_subtitle(radius_active)`).
  Zeigt **nur Deltas**: `query_unseen(conn, persisted_filter, today,
  limit = 3)` — neu angekündigte Konzerte oder durch Radius-/
  Location-Änderung neu in den Scope gerückte. Beides fällt aus einer
  Definition heraus: ungesehen = `seen_at IS NULL` ∧ im aktuellen
  persistierten Filter-Scope ∧ kommend — ein Event außerhalb des Radius
  wurde nie gelistet, also nie gestempelt, und badgt beim Hereinrücken von
  selbst. Row-Format laut Mockup: Artist-Zeile, Meta „Wed, 12. Aug ·
  Cologne · Palladium", rechts Tickets-Pill (öffnet Offer-/Event-URL wie
  CONC-3), darunter dim „38 km" (nur mit Location). Zeilen-Aktivierung =
  Tickets-Ziel; keine eigene Detailfläche.
- **Sprungzeilen** (explizite Zeilen-Aktionen im Sinne von NR-5b —
  navigieren regulär und schließen das Popover): „Show all concerts (14) →"
  routet auf `ViewSource::Concerts` (Zahl = Sidebar-Count aus 8.1);
  **„Show all releases (8) →" routet auf `ViewSource::Releases`** (Zahl =
  `count_releases_view` nach persistierten Filtern — dieselbe Quelle wie das
  Releases-Sidebar-Badge). Die Verlaufs-Unterseite (`history_page.rs`,
  `show_history`, Stack-Seite `HISTORY_PAGE`) **entfällt ersatzlos**
  (Beschluss 7; Funktionsparität wandert in die Releases-View, 8.8):
  NR-12 → NR-12a, NR-5a → NR-5b (10.).

**Badge-Umbau (Header-Benachrichtigungs-Badge, `ui/updates/badge.rs`):**

- Wert = **Summe** `unseen_releases + unseen_concerts`; Darstellung
  unverändert über das `badge_presentation`-Idiom („1"–„9", „9+" ab 10,
  `None` bei 0). `unseen_releases` kommt aus dem owned-bereinigten
  `unseen_release_count` (8.8.1) — owned Releases badgen nie.
- Sichtbarkeits-Gating verallgemeinert das heutige
  `effect.badge_allowed = enabled && has_releases && fetch_completed`
  (`popover.rs`) auf **Beiträge pro Modul**: pure Funktion
  `updates_badge(news: FeedBadgeInput, concerts: FeedBadgeInput) ->
  Option<String>` mit `FeedBadgeInput { enabled: bool, ready: bool,
  unseen: i64 }` — ein Feed trägt nur bei, wenn `enabled && ready`
  (Releases-`ready` = fetch_completed wie heute; Concerts-`ready` =
  fetch_completed ∧ Credentials vorhanden). Ist nur ein Feed aktiv, zählt
  das Badge exakt dessen Count — Concerts ohne Key/disabled erzeugt keinen
  Fehlzustand, alles verhält sich wie heute (nur Releases). Beide Feeds
  inaktiv → Button unsichtbar (NR-3a). Deterministisch, headless testbar
  (analog `badge_presentation`).
- **Seen-Semantik:** Popover-Öffnen stempelt beide Sektionen — Releases wie
  heute (alle gelisteten Einträge, owned eingeschlossen) und Concerts über
  `mark_scope_seen(conn, persisted_filter, today, now)`: gestempelt wird
  die **gesamte ungesehene Delta-Menge im aktuellen Scope**, nicht nur die
  drei sichtbaren Zeilen (NR-9-Parität „Badge verschwindet mit dem Öffnen";
  die 3er-Kappung ist rein visuell, „Show all concerts (N)" bleibt der Weg
  zur Menge). Events außerhalb des Scopes bleiben ungestempelt und badgen
  später korrekt. Badge nach Öffnen = 0, deterministisch aus denselben
  puren Zähl-Funktionen (`count_unseen` beider Feeds) ableitbar — Codex
  schreibt die Tests gegen diese Funktionen, nicht gegen Widget-Zustand.

**Sichtbarkeit & Gates:** Sektion nur bei aktivem jeweiligem Modul; nur ein
aktives Modul ⇒ Popover zeigt nur dessen Sektion unter dem
„UPDATES"-Header. Concerts-Modul an, aber keine Credentials ⇒ die Sektion
zeigt eine einzeilige Hinweiszeile (Kurzform von CONC-4) statt Rows und
trägt nie zum Badge bei.

**Gemeinsamer Footer:** ein „Fetch now" forciert **beide** Fetcher (News-
und Concerts-Worker parallel, `force = true`); Spinner läuft, bis beide
geantwortet haben; Inline-Failure benennt den fehlgeschlagenen Teil
(„Concerts fetch failed", NR-6-Idiom, nie ein Banner). „Updated X ago" =
**ältester** der beiden Ledger-Stände (`MIN(latest_news_attempt,
latest_concerts_attempt)` über aktive Feeds) — konservativ ehrlich: nichts
wird frischer ausgegeben als der älteste Feed; bei nur einem aktiven Feed
dessen Stand.

**Regelwerk-Folgen (Prozesspflicht Bedeutungsänderung ⇒ Ersatzregel, Tests
im selben Commit umhängen):** NR-3 → `[ersetzt durch NR-3a]` (Button =
Updates-Auslöser; sichtbar, wenn irgendein aktiver Feed Einträge oder
Erstlauf-Zustände nach NR-8 hat; Badge ausschließlich für ungesehene
Einträge beider Feeds), NR-9 → `[ersetzt durch NR-9a]` (Badge = Summe,
„9+", Öffnen stempelt beide Sektionen im Scope, owned zählt nie, 0 rendert
nichts), NR-5a → `[ersetzt durch NR-5b]` und NR-12 → `[ersetzt durch
NR-12a]` (History-Entfall zugunsten der Vollansicht — Wortlaute in 10.).
NR-6, NR-8, NR-10, NR-11, NR-13 bleiben unverändert gültig.

### 8.8 Releases-Vollansicht (User-Mockup Frame 3a — Paket R)

Eigener Smart-Sidebar-Eintrag **„Releases"** (8.1) mit Tabelle
`Date · Title · Artist · Type · Status` über den **bestehenden
new_releases-Cache** — keine neuen APIs, kein eigener Worker. Default-Sort
**Datum absteigend** (Mockup: 29. May 26 oben — neueste zuerst).

#### 8.8.1 Core-Datenseite (Umbau `artist_news_*`)

**„In library" wird lokal zur Query-Zeit bestimmt, nicht persistiert.** Die
Mechanik existiert bereits vollständig: `artist_news_query::
local_album_track_counts` + `presence_for` (Schwelle
`OWNED_ALBUM_MIN_TRACKS = 2`) annotieren heute schon jede Query mit
`LibraryPresence::{Absent, Partial, Complete}` — die Releases-View
wiederverwendet exakt diese Funktionen; es gibt **keine neue
Owned-Erkennung**.

- **Fetcher-Umbau (`artist_news_parsing.rs`):** Heute verwirft
  `parse_release_group` erschienene Releases, deren Titel als owned Album
  erkannt ist (Z. 137–143, der `local.contains(&normalize(&title))`-Block —
  er greift nur für `NewsKind::New`; Upcoming wurde nie verworfen, siehe
  Kommentar dort). Der Block **entfällt**; damit verliert
  `parse_release_groups` seinen `local_albums: &[String]`-Parameter →
  neue Signatur `parse_release_groups(json, today, include_singles)`
  (und `parse_release_group` seinen `local`-Parameter).
- **Pipeline (`artist_news_pipeline.rs`):** der Aufruf `local_albums(conn,
  &candidate.name)` (Z. 164) und die Helfer `local_albums` (Z. 245–276) +
  `local_albums_for_test` (Z. 279) werden ersatzlos entfernt (inkl. der
  test-only Re-Exports in `artist_news.rs`). `OWNED_ALBUM_MIN_TRACKS`
  bleibt (Query-Seite nutzt es weiter); sein Doc-Kommentar („Shared by the
  refresh pipeline's `local_albums` filter …") wird auf die alleinige
  `presence_for`-Nutzung umformuliert.
- **Bestands-Caches:** bisher verworfene owned Releases liegen nicht in der
  Tabelle — sie erscheinen erst nach dem **nächsten Fetch des jeweiligen
  Artists** (News-TTL 7 Tage, also binnen ~einer Woche vollständig). Das ist
  okay und wird nicht migriert (kein Backfill; ausgewiesen in 13). Neu
  upsertete owned Releases sind `seen_at IS NULL`, zählen aber nie ins
  Badge (nächster Punkt) und werden beim ersten Popover-Öffnen gestempelt.
  Nebenwirkung: owned Titel konkurrieren jetzt um die `MAX_ITEMS = 20`
  Slots pro Artist-Fetch (NR-1a-Kappung) — bei 90-Tage-Fenster + Typfilter
  praktisch irrelevant, ausgewiesen.
- **`unseen_release_count` (artist_news_query.rs Z. 131) wird
  owned-bereinigt:** Signatur bleibt `(conn) -> Result<i64>`; Implementierung
  lädt die ungesehenen `(artist_name, title)`-Paare
  (`SELECT … WHERE seen_at IS NULL`) plus die `local_album_track_counts`-Map
  und zählt nur Zeilen mit `presence_for(…) != Complete`. `Partial` (nur die
  Lead-Single owned) zählt weiter — genau dafür existiert das Feature.
  Damit sind Header-Badge UND „N new"-Tag im Popover automatisch konsistent
  (beide rufen diese eine Funktion). Hidden-Verhalten bleibt unverändert
  (keine Scope-Ausweitung). Testbar gegen In-Memory-DB mit geseedeten
  `tracks` (owned-Album ≥ 2 Tracks) + `new_releases`-Zeilen.
- **View-Query (neue Datei `crates/reprise-core/src/artist_news_view.rs`):**
  Filter-/Sortier-/Status-Entscheidungen der Vollansicht als pure
  Funktionen, Re-Export über die `artist_news`-Facade. Datenquelle ist das
  bestehende `artist_news_history::query_history(conn, today)` — es liefert
  bereits alles, was die View braucht (`HistoryEntry` mit `hidden`,
  `hidden_at`, `seen_at`, `presence`-Annotation, `announce_url`):
  - `pub struct ReleasesFilter { pub not_in_library: bool, pub release_type:
    Option<ReleaseTypeFilter /* Album|Ep|Single */>, pub hidden: bool }`
    + `persisted_releases_filter(conn)` (Settings-Keys
    `releases.filter.not_in_library/type/hidden`, sticky).
  - `pub enum ReleaseStatus { InLibrary, Upcoming, Released }` +
    `pub fn release_status(entry, today)` — `InLibrary` (presence
    `Complete`) gewinnt immer (Parität zu `release_row::chip_presentation`);
    sonst `Upcoming` bei `first_release_date > today`
    (`parse_partial_date`; unparsbar → konservativ `Released`).
  - `pub fn filter_rows(rows, &ReleasesFilter) -> Vec<HistoryEntry>` —
    `hidden = false` (Default) zeigt nur Sichtbare; der Hidden-Chip
    (`hidden = true`) zeigt NUR Versteckte; `not_in_library` filtert
    `presence != Complete`; `release_type` matcht case-insensitiv.
  - `pub fn sort_rows(rows, direction)` — nach `first_release_date` über
    `parse_partial_date` (Fallback-ans-Ende), Default absteigend,
    Tie-Break Titel.
  - `pub fn query_releases_view(conn, &ReleasesFilter, today) ->
    Vec<HistoryEntry>` (query_history → filter → sort) und
    `pub fn count_releases_view(conn, &ReleasesFilter, today) -> i64`
    (Badge/Sprungzeilen-Zahl; Invariante `count == query(…).len()` als
    Test).
- **Hide/Restore-Backend:** unverändert vorhanden —
  `artist_news::set_release_hidden(conn, mbid, bool)` und
  `artist_news_history::restore_release(conn, mbid)`. Retention bleibt
  komplett unangetastet (`enforce_retention`: 6 Monate ∧ max 200,
  90-Tage-Fetch-Fenster-Schutz — NR-12a übernimmt den Wortlaut).
  `group_history`/`HistoryGroup`/`MONTH_NAMES` verlieren mit der
  History-Subpage ihren einzigen Konsumenten und werden **in U3** entfernt
  (nicht in R — bis U3 hängt das Popover noch daran).

#### 8.8.2 View & Tabelle (`crates/reprise-gnome/src/ui/releases/`, neu)

Dateien nach dem Concerts-Schnittmuster (je 150–350 Z.): `mod.rs`
(Deklarationen + `install(…) -> ReleasesView`), `releases_view.rs`
(Filterzeile + Stack list/status + Footer; `refresh(conn)` = Cache-Read;
MOT-2 hartes Einspielen), `releases_model.rs` (`ReleaseObject`-Wrapper um
`HistoryEntry`, `gio::ListStore` + `SingleSelection`),
`releases_columns.rs`, `releases_presentation.rs`,
`releases_filter_bar.rs`, `releases_empty_state.rs`, `css.rs`
(Registrierung in `style/mod.rs::app_css()`; CSS-Feature-Sektions-Test
wächst mit). **Kein eigener Worker** — Fetch bleibt beim News-Worker.

- **Spalten:** **Date** (Default-Sort absteigend, einzige sortierbare
  Spalte v1, Richtungs-Toggle per Header-Klick; Format pure
  `format_release_date(first_release_date, today)`: volle Daten kompakt im
  Mockup-Idiom „29 May 26", `YYYY-MM` → „May 2026", `YYYY` → „2026"),
  **Title**, **Artist**, **Type** (Album/EP/Single aus `release_type`),
  **Status** (Pill: `In library` / `released` / `upcoming` aus
  `release_status`; Pill-CSS eigenes `.reprise-release-pill`-Set in
  `css.rs`, Farben semantisch — In library = Accent, upcoming = dim).
- **Zeilen-Aktivierung** (Doppelklick/Enter) = Primäraktion nach exakt der
  bestehenden `history_page::history_action`-Dreiweg-Logik (sie wird als
  pure Funktion `releases_row_action(entry, today)` nach
  `releases_presentation.rs` übernommen, die Tests wandern mit): Hidden →
  **Restore**; `presence == Complete` ∧ erschienen → **Show in library**
  (OnShowAlbum-Callback, navigiert + fokussiert, kein Play-Pfad —
  NR-13-Parität inkl. Carve-out: upcoming-in-library öffnet das
  Announcement); sonst **Open announcement** über
  `artist_news_links::announce_url_or_fallback` (Fallback =
  MusicBrainz-Release-Group-Seite) — **die Aktivierung ist damit nie ein
  No-op** (Code-Befund, präzisiert Beschluss 7-Wortlaut; `UriLauncher` +
  Fehler-Toast wie CONC-3).
- **Row-Aktionen** (Hover/Fokus, NR-10-Idiom): sichtbare Zeilen → „Hide"
  (`set_release_hidden(…, true)`); unter dem Hidden-Chip → „Show again"
  (`restore_release`; bestehender String `SHOW_AGAIN`). Nach der Aktion
  Reload in place (kein Row-Fade, MOT-2) + `sidebar.refresh`.
- **Footer:** links „Updated X ago" (`artist_news::latest_fetched_at`),
  Mitte/rechts dezenter Retention-Hinweis (bestehender String
  `RETENTION_SIX_MONTHS` — Funktionsparität zum alten History-Footer),
  rechts Ghost-Button „Fetch now" über die bestehende
  News-Worker-Request-API (Handle wird in V durchgereicht; Spinner +
  Inline-Failure, NR-6-Idiom).

#### 8.8.3 Filterzeile (CONC-2-Analog, Vertrag = NR-14)

Muster + CSS identisch zur Concerts-Filter-Bar (8.4), eigene Facetten:

- `Not in library` — boolescher Chip (`presence != Complete`); der
  Mockup-Zustand „8 releases" ist genau dieser Chip aktiv.
- `Type` — Album / EP / Single.
- `Hidden` — zeigt NUR Versteckte (dort ersetzt „Show again" die
  „Hide"-Aktion); Default aus.
- Sticky in Settings (`releases.filter.*`), „Clear all ×", Zählzeile
  „X of Y releases"; 0 Treffer bei aktiven Filtern → StatusPage mit genau
  einem Schritt „Show all N releases" (FIL-6-Geist).

#### 8.8.4 Empty-/Status-Zustände (`releases_empty_state.rs`)

Pure `releases_empty_state_for(row_count, has_filter, never_fetched)`
(Modul-aus ist kein View-Zustand — die Row ist dann gar nicht in der
Sidebar, 8.1):

| Zustand | Bedingung | StatusPage |
|---|---|---|
| `List` | rows > 0 | — |
| `NeverFetched` | News-Ledger leer (`latest_fetched_at` = None) | „No release data yet" + „Fetch now" |
| `NoResults` | 0 rows, Filter aktiv | ein Button „Show all N releases" |
| `Empty` | 0 rows, ohne Filter, gefetcht | „No releases from your artists yet" + „Fetch now" (FB-5a-Ton) |

Offline wie bei Concerts kein eigener Zustand (Cache + Footer).

## 9. Preferences

**Kein neuer `PageId`** (SET-1: neue Features = neue Seite ODER Sektion):
Concerts wird Modul-Eintrag auf der **Plugins-Seite** — `preference_plugins.rs`
listet `ALL_MODULES` bereits generisch und hängt für `descriptor.id ==
"new_releases"` Extra-Rows an (Z. 124–133); dasselbe Muster für
`descriptor.id == "concerts"` mit Rows aus neuem
**`ui/preferences/preference_concerts.rs`**:

- Credentials: `adw::PasswordEntryRow` „Bandsintown app_id" +
  `adw::PasswordEntryRow` „Ticketmaster API key" — `connect_changed` →
  `set_setting` sofort (SET-4; `preference_lastfm.rs`-Idiom für die Rows,
  Persistenz aber in der settings-Tabelle statt Keyring — Beschluss 2).
  PasswordEntryRow bewusst trotzdem: Keys gehören nicht in Screenshots.
- Location: `adw::EntryRow` „City" mit Apply-Button
  (`show_apply_button`); Apply → `one_shot_task::spawn("reprise-geocode",
  …)` → Nominatim (7.1) → Subtitle = `display_name` bzw. Fehlerzeile
  „Could not find that place". Apply ist die Nutzeraktion, damit nicht jeder
  Tastendruck ein Netz-Call wird (SET-4-konform ausgelegt). Daneben Buttons
  „Use current location" (Paket-G-Anteil: One-Shot-Portal, 7.3 — Erfolg
  schreibt dieselben Keys mit Name „Current location", Fehler →
  Fehlerzeile; die Erfolg/Fehler-Entscheidung ist eine pure Funktion) und
  „Clear location".
- `adw::ComboRow` „Default radius" (off/50/100/250/500 km — setzt nur den
  Filter-Default für frische Installationen).
- `adw::SpinRow` „Consider artists played in the last N days" (30–365,
  Default 90) → `concerts.window_days`.
- Similar (Paket-S-Anteil): `adw::SwitchRow` „Include similar artists"
  (`concerts.similar_enabled`, Default aus) + `adw::SpinRow` „Similar
  artists per top artist" (1–25, Default 10, nur mit Schalter aktiv) →
  `concerts.similar_count`.
- Modul-Schalter selbst kommt vom generischen Plugins-Rendering
  (`CONCERTS_MODULE` in `ALL_MODULES` genügt); Toggle benachrichtigt die
  `ConcertsRuntime` (Enabled-Subscription) → Sidebar-Row erscheint/
  verschwindet via `sidebar.refresh("concerts module toggled")`. Die
  Releases-Row hängt am bestehenden new_releases-Toggle — dieselbe
  Refresh-Leitung, kein neuer Code.

## 10. UX-Regelwerk (docs/ux-rules.md)

Neue Sektion **„AE. Concerts"** (nächster freier Buchstabe nach AD;
beim Einfügen gegen den dev-Stand verifizieren — Kommentar-Idiom wie bei
Sektion O). Regeln als `[geplant]` mit `<!-- REVIEW: Regelvorschlag -->` im
Fundament-Task, Flip auf `[aktiv]` im jeweiligen Implementierungs-Commit mit
regelbenannten Tests (`fn conc_1_…`, `scripts/check-ux-traceability.sh`):

- **CONC-1** [gtk] — Sidebar-Ort in der SMART-Sektion, nur bei aktivem Modul;
  Badge = Anzahl der Zeilen, die das Öffnen zeigt (kommende Konzerte nach
  persistierten Filtern), 0 → kein Badge.
- **CONC-2** [gtk] — Filterzeile ist permanenter Header (Idle leise:
  Gesamtzahl + „+ Add filter"); jede aktive Einschränkung ist ein Chip mit
  ×-Ziel ≥ 20 px; aktive Zählung „X of Y concerts" + „Clear all". Der
  Radius-Filter ist ohne gesetzte Location disabled mit Tooltip
  „Set a location in Preferences".
- **CONC-3** [gtk] — Zeilen-Aktivierung (Doppelklick/Enter) und die
  Ticket-Zelle öffnen dasselbe externe Ziel: Offer-URL, sonst
  Event-Seite; ohne beides ist die Zelle leer und die Aktivierung ein No-op
  mit Tooltip. Es gibt keinen Play-Pfad.
- **CONC-4** [gtk] — Zustandsvertrag: kein Credential → StatusPage mit
  Preferences-Deep-Link (Fetch hart aus, kein Crash); nie gefetcht →
  „Fetch now"; 0 Treffer mit Filtern → genau ein „Show all"-Schritt;
  offline/Fehler → Cache + „Updated X ago" + Inline-Failure im Footer.
- **CONC-5** [core] — Netz nur im Worker: Trigger sind View-Open-Staleness
  (24 h + Jitter), stündlicher Due-Check und „Fetch now"; Track-Wechsel und
  Navigation lesen ausschließlich Cache. Ergebnis-Einspielen animiert nicht
  (MOT-2).
- **CONC-6** [gtk] — Similar-Zeilen tragen ein dimmes „similar to {seed}" in
  der Artist-Zelle und verschwinden mit dem Source-Filter
  „Library artists only"; die Source-Pill ist sichtbar, sobald Similar
  aktiviert ist oder Similar-Zeilen existieren. (v1, Flip in C3 — Paket S
  liefert die Daten in Welle 1.)
- **CONC-7** [gtk] — Das Updates-Popover gruppiert beide Feeds: Sektion nur
  bei aktivem Modul; die Concerts-Sektion zeigt höchstens drei ungesehene
  Einträge des persistierten Filter-Scopes plus „Show all concerts (N) →"
  (navigiert regulär zur Vollansicht, Popover schließt — NR-5b-Klasse);
  Öffnen stempelt die gesamte gelistete Delta-Menge beider Sektionen; das
  Header-Badge zeigt die Summe der ungesehenen Einträge aller aktiven,
  fetch-bereiten Feeds nach dem `badge_presentation`-Idiom.

**Sektion R wächst statt einer neuen REL-Sektion** — die Releases-Vollansicht
ist dieselbe Feature-Domäne wie das bestehende NR-Regelwerk, und ein zweiter
Prefix würde die Ersatzregel-Ketten dieser Domäne (NR-4 → NR-12 → NR-12a)
über zwei Sektionen zerreißen. Append-only-Prozess wie gehabt; Ersatzregeln
mit Test-Renames im selben Commit wie der jeweilige Verhaltens-Flip:

- **NR-3 → `[ersetzt durch NR-3a]`**, **NR-9 → `[ersetzt durch NR-9a]`** —
  Wortlaute in 8.7 (Summen-Badge; NR-9a enthält zusätzlich: Einträge, deren
  Album vollständig in der Bibliothek liegt — presence `Complete` —, zählen
  nie in den Unseen-Badge, werden aber gelistet und beim Öffnen
  mitgestempelt). Flips + Renames (`nr_3_*`→`nr_3a_*`, `nr_9_*`→`nr_9a_*`)
  in U2.
- **NR-5a → `[ersetzt durch NR-5b]`** — NR-5a schreibt fest „der Verlauf ist
  eine Popover-interne Unterseite"; das kippt mit dem History-Entfall
  (Bedeutungsänderung ⇒ Ersatzregel). **NR-5b** [gtk]: Das Popover ist
  transient; Öffnen/Schließen verändert den Navigations-Stack nie. Explizite
  Zeilen-Aktionen (Show in library) und die Sprungzeilen („Show all
  releases/concerts →") navigieren regulär und schließen das Popover. Das
  Popover hat keine internen Unterseiten; der Verlauf lebt in der
  Releases-Vollansicht (NR-12a). Flip + Rename (`nr_5a_*`→`nr_5b_*`,
  `popover_tests.rs` Z. 21) in U3.
- **NR-12 → `[ersetzt durch NR-12a]`** — **NR-12a** [gtk]: Die persistente
  Historie aller je gezeigten Meldungen lebt in der Releases-Vollansicht
  (eigener Sidebar-Ort, NR-15), nicht mehr als Popover-Unterseite;
  ausgeblendete Einträge sind dort über den Hidden-Filter einzeln rückholbar
  („Show again"). Retention unverändert: 6 Monate UND höchstens 200 Einträge
  (strengere Grenze gewinnt), hartes Löschen, nie innerhalb des
  90-Tage-Fetch-Fensters. Ersetzt NR-12 (und transitiv NR-4). Flip + Renames
  (`nr_12_*`→`nr_12a_*` wo die Semantik überlebt, z. B. der Restore-Test in
  `artist_news_history.rs`; Gruppierungs-Tests entfallen mit
  `group_history`) in U3.
- **NR-14** [gtk] (neu, Flip in R3/R4) — Vollansicht-Vertrag: Tabelle
  `Date · Title · Artist · Type · Status`, Default-Sort Datum absteigend;
  Status-Pill `In library` (presence Complete, gewinnt immer), sonst
  `upcoming` (Datum > heute), sonst `released`. Zeilen-Aktivierung =
  Primäraktion im Dreiweg: Hidden → Show again; In library ∧ erschienen →
  Show in library (NR-13-Carve-out bleibt: upcoming öffnet das
  Announcement); sonst Open announcement (`announce_url_or_fallback` — nie
  ein No-op). Filterzeile nach CONC-2-Muster mit Chips
  `Not in library / Type / Hidden`, sticky, „X of Y releases", „Clear all";
  0 Treffer mit Filtern → genau ein „Show all N releases"-Schritt.
- **NR-15** [gtk] (neu, Flip in V) — Sidebar-Ort „Releases" in der
  SMART-Sektion (vor Concerts), nur bei aktivem new_releases-Modul; Badge =
  Anzahl der Zeilen, die das Öffnen zeigt (nach persistierten Filtern),
  0 → kein Badge.

## 11. i18n

- Neue Datei `crates/reprise-gnome/src/ui/strings_concerts.rs` (N_!-Katalog +
  Formatter: Spaltentitel, Facetten-/Chip-Labels, StatusPage-Texte,
  „similar to {artist}", „{shown} of {total} concerts", „Updated {age} …",
  „Set a location in Preferences", „Use current location", …), via
  `strings.rs` re-exportiert (Pfad-`mod`-Idiom).
- Neue Datei `crates/reprise-gnome/src/ui/strings_releases.rs` — eigener
  Katalog analog `strings_concerts.rs` (konsistente Datei-pro-Feature-Form):
  Spaltentitel, Status-Pill-Texte (`In library` / `released` / `upcoming`),
  Chip-Labels (`Not in library`, `Hidden`, Typen), „{shown} of {total}
  releases", Empty-State-Texte, „Hide". Bereits vorhandene Strings werden
  wiederverwendet, nicht dupliziert: `SHOW_AGAIN`, `SHOW_IN_LIBRARY`,
  `OPEN_ANNOUNCEMENT`, `RETENTION_SIX_MONTHS` (strings_news.rs Z. 144–150).
- `strings_news.rs` wird im Fundament um die Shell-Strings erweitert
  („UPDATES", „NEW RELEASES"-Sektionstitel, „new near you",
  „newly announced", „Show all concerts ({count})",
  „Show all releases ({count})", „Concerts fetch failed") — ein Katalog,
  ein Owner (F1). `SHOW_HISTORY`/History-spezifische Strings entfallen in
  U3 (Katalog-Cleanup im selben Commit wie der Konsument).
- **`po/POTFILES.in`**: `crates/reprise-gnome/src/ui/strings_concerts.rs`
  UND `crates/reprise-gnome/src/ui/strings_releases.rs` eintragen (sonst
  fehlen die Strings in allen sieben Katalogen).
- Alle Strings englisch; Datums-/Zahlenformat über chrono/bestehende Helfer;
  keine Klartext-Strings an Widget-Call-Sites.

## 12. Teststrategie (TDD)

Jeder Task rot-zuerst; Gates pro Commit (Abschnitt „Verifikation" unten).

- **Pure Core-Units (ohne Netz, ohne Display):** `parse_artist`/
  `parse_events` je Provider (Fixture-Strings inline: Normalfall,
  String-Koordinaten, fehlende venue-Felder, leeres Array, kaputtes JSON →
  `Parse`), `dedupe_key`-Normalisierung (Diakritika, Case, Whitespace),
  `merge`-Präferenz, `ticket_source_label`, `haversine_km`-Referenzpaare,
  `parse_geocode`, `backoff_delay`-Tabelle (Versuch 1..4, Retry-After
  kleiner/größer/jenseits Cap), `artist_due`/`refresh_due`-Grenzwerte +
  Jitter-Determinismus, Kandidaten-Query (In-Memory-DB mit
  `listen_events`-Seeds: Fenster, Cap, Staleness-Reihenfolge),
  Resolution-Cache (negativ 7 d, failed ≠ negativ), Pipeline-Endtoend gegen
  `REPRISE_CONCERTS_FIXTURE_DIR` (Upsert, Reconcile-Delete mit
  `seen_at`-Erhalt, `delete_past_events`, Ledger-Outcomes, Fallback-Kette
  Bandsintown→TM), `query_events`-Filter (Radius, Land, Horizont,
  include_similar, `None`-Distanz), `count_upcoming` =
  `query_events(...).len()`-Invariante, Seen-Zyklus (`query_unseen` →
  `mark_scope_seen` → `count_unseen` = 0; Out-of-Scope-Events bleiben
  ungestempelt und badgen nach Radius-Erweiterung), Env-Fallback der
  Credentials.
- **Similar (Paket S):** `parse_listenbrainz_similar` (Normalfall, leeres
  Array, kaputtes JSON), `parse_lastfm_similar` (match als String UND Zahl,
  Schwelle 0.4), Quellen-Wahl (MBID → LB; ohne MBID + Bundle-Key → Last.fm;
  ohne beides → skip), Caps (pro Seed `similar_count`, global 50,
  Score-Ordnung), Dedupe gegen Library-Keys, Pipeline-Erweiterung
  (is_similar=1-Fluss, 0-wärts-Überschreibregel, gemeinsame
  30er-Lauf-Kappe mit Library-Vorrang).
- **Releases-Datenseite (Paket R):** `parse_release_groups` ohne
  Owned-Drop (owned Titel bleiben; die bisherigen Drop-Tests in
  `artist_news_parsing_tests.rs` werden invertiert), owned-bereinigter
  `unseen_release_count` (owned unseen zählt nicht; Partial zählt; seen
  zählt nie), `release_status`-Matrix (Complete gewinnt; partial dates;
  unparsbar → Released), `filter_rows` (not_in_library / Type /
  Hidden-Exklusivität), `sort_rows` (absteigend, partial dates,
  Tie-Break), `count_releases_view`-Invariante,
  `releases_row_action`-Dreiweg (übernommene `history_action`-Tests inkl.
  NR-13-Carve-out), `format_release_date`.
- **Location (Paket G):** pure Helfer headless (`location_from_vardict`
  mit vollständigem/fehlendem/degeneriertem Vardict, `ACCURACY_CITY`,
  Timeout-Policy, Fehlertext-Formung); der echte Portal-Roundtrip ist
  manueller Pass (Z1-Checkliste).
- **CLI/MCP (Paket M):** `crates/reprise-cli/tests/concerts.rs`
  (geseedete Scratch-DB: human, `--json`-Shape, `--limit`, `--all` vs.
  persistierter Filter, leere DB); `crates/reprise-mcp/tests/resources.rs`
  (Resource gelistet, Read liefert das dokumentierte JSON, unbekannte URI
  weiter Fehler).
- **Migrationstests:** `db_concerts_migration_tests.rs` — v30→v31, Tabellen/
  Indizes/UNIQUE vorhanden, Idempotenz, `SUPPORTED_SCHEMA_VERSION = 31`.
- **GTK-seitig:** UI-kritische Logik ausschließlich als pure Funktionen in
  `concerts_presentation.rs`/`concerts_empty_state.rs` bzw.
  `releases_presentation.rs`/`releases_empty_state.rs` headless testen
  (Format, Sortierung inkl. `None`-ans-Ende, Empty-State-Matrizen,
  Zählzeilen, Status-Pills, Row-Action-Dreiweg); Popover-Logik ebenso pur:
  `updates_badge`-Matrix (beide Feeds an/aus/not-ready, Summen, „9+", 0 →
  `None`, owned-Ausschluss über die Zählfunktion),
  `concerts_section_subtitle`, Opening-Effekt (stempelt beide Sektionen)
  core-nah über die Query-Funktionen; bestehende `popover_tests.rs` werden
  beim Move mechanisch mitgeführt und bei NR-3a/NR-5b/NR-9a/NR-12a
  umbenannt; Widget-Tests `#[ignore = "requires a display; run via
  xvfb-run"]` und einzeln via `dbus-run-session -- xvfb-run -a cargo test
  -p reprise-gnome <name> -- --ignored --test-threads=1`
  (MainContext-Disziplin: Display-Tests nie im Rudel bewerten).
- **Regelbenannte Tests** für jeden Flip (`conc_1_badge_counts_filtered_
  upcoming`, `conc_3_row_activation_opens_ticket_target`,
  `conc_6_similar_rows_carry_seed_caption`, `nr_14_hidden_chip_shows_only_
  hidden_rows`, `nr_15_releases_row_gated_on_module`, …) —
  `check-ux-traceability.sh`.
- **Kein Test kontaktiert das Netz**; der Fixture-Seam ist die einzige
  HTTP-Quelle in Tests (4.1).

## 13. Risiken & Abgrenzung

**Out of scope (v1):** Kartenansicht, Meilen, Kalender-Export, Preis-Anzeige,
mehrere Locations, **Desktop-Benachrichtigungen für Updates** (die App sendet
heute ausschließlich Now-Playing-`gio::Notification`s via
`ui/notifications.rs`; ein Update-Kanal über `gio::Notification` bleibt als
einzige Ausbaustufe N unentschieden). Ebenfalls bewusst nicht in v1: eine
CLI/MCP-Surface für **Releases** (nur Concerts hat eine — Spec-Scope).

**Risiken:**

- **Bandsintown-`app_id`-Vergabe:** wird „auf Anfrage" erteilt — bis dahin
  trägt Ticketmaster (sofort verfügbarer Key) das Feature allein; genau
  dafür existiert die Fallback-Kette. Kein Key darf je im Repo landen
  (Settings-Eingabe, nie hardcoded, keine Compile-Time-Injektion in v1).
- **API-Formate sind Annahmen aus der Doku:** Feldnamen (`offers[].status`,
  `externalLinks.musicbrainz`, String-Koordinaten) vor Paket A einmal gegen
  die echten APIs verifizieren; Parser sind tolerant gegen fehlende Felder
  gebaut, Fixtures werden bei Abweichung nachgezogen (Restrisiko-Idiom des
  Release-Rework).
- **ListenBrainz-Labs-Verfügbarkeit (Paket S):** `labs.api.listenbrainz.org`
  ist ein Experimental-Endpoint ohne Stabilitätszusage — URL-Form,
  `algorithm`-String und Response-Shape vor Paket S einmal live
  verifizieren; bei Ausfall degradiert Similar sauber (Fehler = `failed` im
  Ledger, Library-Feed unberührt), Last.fm bleibt nur Namens-Fallback.
- **Location-Portal ohne Flatpak:** der Portal-Pfad braucht einen laufenden
  `xdg-desktop-portal` samt Location-Backend (GeoClue) — auf GNOME-Hosts
  gegeben, auf schlanken Setups nicht garantiert. Der Fehlerpfad
  (Fehlerzeile, manuelle Stadt bleibt) ist deshalb Pflicht-Test; kein
  GeoClue2-Direktzugriff als zweiter Backend-Zweig (YAGNI).
- **Owned-Umbau-Migrationsverhalten (Paket R):** bisher verworfene owned
  Releases erscheinen erst mit dem nächsten Fetch je Artist (News-TTL
  7 Tage) — die Releases-View ist bis dahin für owned Einträge unvollständig;
  kein Backfill (bewusst, 8.8.1). Owned Titel konkurrieren zudem um die
  `MAX_ITEMS = 20`-Kappung pro Fetch (NR-1a) — praktisch irrelevant,
  beobachten.
- **Namens-Kollisionen** („Genesis") liefern falsche Konzerte, wenn keine
  MBID vorhanden ist: exakter Namensmatch + `mbid_verified`-Flag begrenzen
  das; unverified Zeilen sind v1 nicht markiert (bewusst schlank — nur
  intern im Ledger), bei Beschwerden ist ein dezenter Marker eine kleine
  Folgeänderung. Similar-Kandidaten verschärfen das leicht (mehr Namen ohne
  Tag-MBID) — LB-Labs liefert MBIDs mit, die als `artist_mbid` in den
  Ledger wandern und die Verifikation füttern.
- **Nominatim-Policy** (1 req/s, UA-Pflicht): erfüllt durch den geteilten
  Limiter + Pflicht-UA; Geocoding feuert nur auf Apply.
- **Icon-Verfügbarkeit** `ticket-symbolic`/`star-new-symbolic`:
  Laufzeit-Fallbacks vorhanden (8.1); Optik je Theme nur manuell prüfbar.
- **Ticketmaster-Ratenbudget** (Discovery: 5 000/Tag, 5 req/s): unser
  1-req/s-Takt + 24-h-TTL + Cap 30 Artists/Lauf bleibt weit darunter — auch
  mit Similar (die Kappe ist gemeinsam).
- **`sidebar_rebuild`-Kosten:** zwei zusätzliche Count-Queries pro Rebuild —
  `count_upcoming` ist ein indexierter Aggregat-Read über eine kleine
  Tabelle; `count_releases_view` lädt `new_releases` (≤ 200 Zeilen,
  NR-12a-Retention) plus die Track-Titel-Map. Unkritisch, im Zweifel im
  Task messen.
- **Move-Churn `ui/new_releases/` → `ui/updates/`:** rein mechanisch und
  compilergeführt, aber er berührt viele `use`-Pfade und läuft parallel
  laufenden Branches in die Quere — deshalb ein eigener, isolierter Commit
  direkt vor dem Shell-Umbau (Paket U), nie mit Verhaltensänderungen
  vermischt.
- **NR-Test-Renames (NR-3a/NR-5b/NR-9a/NR-12a):** `check-ux-traceability.sh`
  erzwingt, dass Flip, Ersatz-Verweis und Test-Umbenennung im selben Commit
  landen — U2/U3 sind genau so geschnitten.

## 14. Akzeptanzkriterien (konkretisiert aufs Repo)

| # | Kriterium | Verifikation |
|---|---|---|
| 1 | Sidebar-Eintrag „Concerts" (SMART-Sektion, Ticket-Icon mit Fallback) mit Live-Badge = gefilterte kommende Konzerte; Badge aktualisiert nach Fetch und Filterwechsel | `conc_1_*`-Tests, Sidebar-Rebuild-Test, Display-Smoke |
| 2 | Tabelle rendert `Date · Artist · City · Venue · Distance · Tickets`; Default Datum aufsteigend; Date+Distance per Header-Klick sortierbar (Distanz: `None` stabil ans Ende) | `sort_rows`-Units, Display-Test Header-Klick |
| 3 | Filter Radius/Country/Date range/Source als Chips mit ×; Radius disabled ohne Location mit Tooltip; „Clear all" räumt alles; Zählung „X of Y concerts" | `conc_2_*`-Tests, Filter-Units, Display-Test |
| 4 | Ticket-Zelle zeigt Quellnamen und öffnet die Offer-URL extern; Rows ohne Offer öffnen den Plain-Event-Link (Bandsintown/TM-Seite) | `ticket_source_label`-Units, `conc_3_*`-Display-Test (UriLauncher-Seam) |
| 5 | Ohne Location: Distance „—", Radius-Pill disabled; ohne Keys: StatusPage + Preferences-Deep-Link, kein Netzversuch, kein Crash; offline: Cache + „Updated X ago" + Inline-Failure; 0 Treffer: genau ein nächster Schritt | Empty-State-Matrix-Units, `conc_4_*`, Pipeline-Test ohne Provider |
| 6 | Kein UI-Jank: alles Netz im `reprise-concerts`-Thread; Trigger nur Open/Timer/„Fetch now"; Track-Wechsel liest nie live | `conc_5_*`, Worker-Test, Code-Audit „kein `http::get` außerhalb der Pipeline" |
| 7 | Dedupe: derselbe Gig aus zwei Providern erscheint einmal ((date, city, venue) normalisiert); Re-Fetch entfernt abgesagte/vergangene Events, `seen_at` überlebt das Reconcile | Dedupe-/Pipeline-Units, Migrationstest UNIQUE |
| 8 | Updates-Popover: Header-Badge = Summe ungesehener Releases + Konzerte („9+"-Idiom); owned Releases zählen nie; nur enabled+ready-Feeds tragen bei (Concerts ohne Key ⇒ Verhalten wie heute, nur Releases); Öffnen stempelt beide Sektionen, Badge → 0 | `updates_badge`-Matrix, Seen-Zyklus-Units, owned-Zähl-Units, `nr_9a_*`/`conc_7_*`-Tests |
| 9 | Concerts-Sektion zeigt max. 3 Delta-Rows (Format Artist / „Wed, 12. Aug · Cologne · Palladium" / Tickets-Pill / „38 km"); „Show all concerts (N) →" und „Show all releases (N) →" navigieren in die jeweilige Vollansicht und schließen das Popover; es gibt keine Popover-Unterseite mehr | Display-Tests Popover-Navigation, Delta-Query-Units, `nr_5b_*` |
| 10 | Sidebar-Eintrag „Releases" (Sparkle-Icon mit Fallback, vor Concerts, nur bei aktivem new_releases-Modul) mit Badge = gefilterte View-Zeilen; Tabelle `Date · Title · Artist · Type · Status` (Pills `In library`/`released`/`upcoming`), Default Datum absteigend | `nr_15_*`-, `release_status`-, `sort_rows`-Units, Display-Smoke |
| 11 | Releases-Filter `Not in library`/`Type`/`Hidden` als sticky Chips mit „Clear all"; Hidden-Chip zeigt nur Versteckte mit „Show again"; Hide/Restore wirken sofort (Cache-Reload in place) | `nr_14_*`-Tests, Filter-Units, Display-Test |
| 12 | Releases-Aktivierung: Hidden → Restore; owned+erschienen → „Show in library" (Navigieren+Fokussieren); sonst Announcement via `announce_url_or_fallback` — nie ein No-op; owned Releases werden gelistet, badgen aber nie | `releases_row_action`-Units (NR-13-Carve-out), owned-Zähl-Units |
| 13 | Similar Artists (default OFF): Schalter + Count in den Prefs; LB-Labs primär, Last.fm nur als Namens-Fallback mit Bundle-Key; Caps 10/25/50 greifen; Similar-Zeilen tragen Caption + verschwinden mit „Library artists only" | `conc_6_*`, Similar-Units, Pipeline-Erweiterungs-Test |
| 14 | „Use current location": One-Shot-Portal-Read schreibt lat/lon/„Current location" in Settings; Ablehnung/Timeout/fehlendes Portal → Fehlerzeile, manuelle Stadt unberührt | pure Vardict-/Decision-Units, manueller Portal-Pass (Z1) |
| 15 | `reprise-cli concerts list` liefert human + `--json` (dokumentiertes Shape), `--limit`, `--all` vs. persistierter Filter; MCP `reprise://concerts` gelistet + lesbar, read-only, keine Pfade | CLI-Integrationstests, MCP-Resource-Test |
| 16 | Strings vollständig übersetzbar (`strings_concerts.rs` + `strings_releases.rs` + erweiterte `strings_news.rs` + POTFILES), Icons aus dem System-Set, Gates grün (fmt/clippy/test/audit/tree-Purity/Skript-Gates) | Gate-Battery pro Commit, `check-ux-traceability.sh` |

## 15. Gegrillte Beschlüsse (2026-07-25)

Ergebnisprotokoll des Grills; keine offenen Fragen mehr. Nummerierung =
Entscheidungspunkte des Drafts.

1. **v1-Schnitt — GEKIPPT:** Similar Artists (S), Systemstandort (G) und
   CLI/MCP (M) sind **reguläre v1-Pakete**, keine Ausbaustufen. Out of scope
   bleiben nur Kartenansicht, Meilen, Kalender-Export, Preise, mehrere
   Locations und Desktop-Benachrichtigungen (N bleibt die einzige,
   unentschiedene Option).
2. **Provider-Credentials in der `settings`-Tabelle — BESTÄTIGT.**
   `app_id`/`apikey` sind Client-Identifier ohne Kontozugriff; der
   Core-Worker liest sie ohne zbus/async-Umweg; `PasswordEntryRow` gegen
   Shoulder-Surfing; Env-Fallback bleibt (Abschnitt 3).
3. **Eigene `ConcertsView` statt TrackList-Umbau — BESTÄTIGT** und
   verallgemeinert: auch die Releases-Vollansicht ist eine eigene, kleine
   ColumnView nach demselben Schnittmuster (kein windowed Model, Filter/Sort
   pur).
4. **Ticketmaster als Fallback pro Artist, kein additiver Merge —
   BESTÄTIGT.** Dedupe-UNIQUE bleibt Sicherheitsnetz für Provider-Wechsel.
5. **Zwei Zähler-Kanäle — BESTÄTIGT, gilt für beide Views:** Sidebar-Badge =
   View-Count nach persistierten Filtern; Neuheit lebt ausschließlich im
   Updates-Popover-Badge. Ergänzt um die owned-Ausnahme: vollständig owned
   Releases zählen nie in den Unseen-Kanal (NR-9a).
6. **Sidebar-Rows nur bei aktivem Modul — BESTÄTIGT, beide Rows:** Concerts
   am `CONCERTS_MODULE`, Releases am `NEW_RELEASES_MODULE`
   (Conversions-Gate-Idiom).
7. **„Show all releases"-Ziel — GEKIPPT durch User-Mockup Frame 3a:** statt
   der Popover-Verlaufs-Unterseite kommt eine **echte Releases-Vollansicht**
   als eigener Smart-Sidebar-Eintrag (Paket R, Abschnitt 8.8). Die
   History-Subpage (`history_page.rs`) entfällt ersatzlos; ihre gesamte
   Funktionsparität (Restore einzelner Einträge, Sichtbarkeit der
   Retention-Grenzen, Dreiweg-Primäraktion) wandert in die View. NR-12 →
   NR-12a, NR-5a → NR-5b. Damit ist Release-Rework-Beschluss 2 („kein
   Digest-Ort") bewusst revidiert — der neue Ort ist eine gefilterte
   Tabellen-View mit eigenem Vertrag, kein Digest-Feed.
8. **Similar-Quelle: ListenBrainz Labs primär — BESTÄTIGT, jetzt v1.**
   Last.fm `getSimilar` (match ≥ 0.4) nur als Namens-Fallback für Seeds ohne
   Tag-MBID und nur mit gebündeltem Compile-Time-Key (Abschnitt 6).

**Beim Nachlesen im Code präzisiert (bindet Codex):**

- Der Query-Zeit-Owned-Abgleich existiert bereits als
  `LibraryPresence`/`presence_for`/`local_album_track_counts`
  (`artist_news_query.rs`) — Paket R führt **keine** neue Owned-Erkennung
  ein, sondern entfernt nur den Fetch-Zeit-Drop (der ohnehin nur
  `NewsKind::New` betraf; Upcoming wurde nie verworfen).
- Die Releases-Zeilen-Aktivierung ist **nie ein No-op**:
  `announce_url_or_fallback` fällt immer auf die
  MusicBrainz-Release-Group-Seite zurück, und `history_action`
  (Dreiweg inkl. NR-13-Carve-out) wird als `releases_row_action`
  übernommen — das ersetzt die Grill-Formulierung „sonst No-op mit
  Tooltip".
- NR-5a pinnt den Verlauf als Popover-Unterseite fest — der History-Entfall
  ist damit eine Bedeutungsänderung auch für NR-5a, nicht nur für NR-12
  ⇒ zusätzliche Ersatzregel NR-5b (Test `nr_5a_opening_the_popover_…`
  wird in U3 umbenannt).
- `group_history`/`HistoryGroup`/`MONTH_NAMES` (`artist_news_history.rs`)
  verlieren mit der Subpage ihren einzigen Konsumenten und werden in U3
  entfernt; `query_history`, `restore_release` und `enforce_retention`
  bleiben und tragen die View.
- Das Popover rendert owned Rows heute schon korrekt
  (`release_row::chip_presentation`/`primary_action` kennen
  `LibraryPresence::Complete`) — „owned wird gelistet und mitgestempelt"
  braucht dort null neuen Code; nur die Zählfunktion ändert sich.

---

## 16. Arbeitspakete als Wellen (Datei-Ownership)

Die **Wellen-Reihenfolge ist vorgegeben**: 0) Fundament → 1) Datenschicht →
2) Views → 3) Updates-Popover → 4) Preferences zuletzt. Konsequenz: die
Datenschicht bringt ihr eigenes Key-Bootstrapping mit (Env-/SQL-Weg,
Abschnitt 3), damit „kein Key → Hinweis statt Crash" und echte Fetch-Läufe
lange vor dem Prefs-UI funktionieren; Location ist bis Welle 4 nur per
Settings-Key setzbar, die Distanz-Spalte zeigt ohne Location ohnehin „—".

Regeln: **kein Paket teilt Dateien mit einem parallel laufenden Paket**;
alle Konfliktpunkte (db.rs, view_source.rs, browser*, modules.rs, alle
String-Kataloge, POTFILES, ux-rules.md, mod.rs-Deklarationen) liegen im
Fundament. Die gemeinsamen Verdrahtungspunkte beider Views
(`sidebar_rebuild.rs`, `sidebar_presentation.rs`, `window.rs`,
`library_shell.rs`, `track_list_smoke.rs`) liegen in **einem** Task V, der
nach C und R läuft — C und R selbst berühren sie nie. Sequenzielle
Ownership-Übergaben (ausgewiesen, kein Konflikt): `window.rs` V → U3
(Popover-Install-Signatur); `ui/artist_news/artist_news_worker.rs` V
(Fetch-Handle-Durchreichung an die Releases-View, falls eine Signaturzeile
nötig) → U1 (Move-Referenzfix); `docs/ux-rules.md` append-only durch F1,
C-Tasks, R-Tasks, V, U2, U3. Jeder Task: TDD (Red zuerst), volle
Gate-Battery, ein Commit.

### Welle 0 — Fundament (ein Owner, sequenziell)

- **F1 · Regeln + Strings + Modul.** Dateien: `docs/ux-rules.md`
  (Sektion AE mit CONC-1..7 `[geplant]`; in Sektion R die Ankündigungen
  NR-3a/NR-5b/NR-9a/NR-12a als `[geplant]`-Ersatzentwürfe + NR-14/NR-15
  `[geplant]`), `crates/reprise-gnome/src/ui/strings_concerts.rs` (neu,
  vollständiger Katalog + Formatter), `ui/strings_releases.rs` (neu,
  vollständiger Katalog + Formatter), `ui/strings_news.rs` (Shell-Strings:
  „UPDATES", „new near you", „Show all concerts ({count})",
  „Show all releases ({count})", …), `ui/strings.rs` (mod-Zeilen),
  `po/POTFILES.in` (beide neuen Kataloge),
  `crates/reprise-core/src/modules.rs` (`CONCERTS_MODULE` + `ALL_MODULES`).
  TDD: Formatter-Units, Modul-Default-off-Test.
- **F2 · Migration V31.** Dateien: `crates/reprise-core/src/db_concerts.rs`
  (neu, inkl. `seen_at`) + `db_concerts_migration_tests.rs` (neu), `db.rs`
  (SUPPORTED=31 + Aufrufzeile). TDD: Migrationstests zuerst.
- **F3 · Enum-/Facade-Verdrahtung.** Dateien:
  `crates/reprise-core/src/view_source.rs` (Concerts + Releases),
  `browser.rs` (beide BrowserPlaces), `browser/navigation.rs` (beide
  SidebarTargets), `artist_news_refresh.rs` (`fnv1a_64` `pub(crate)`),
  `lib.rs` (concerts-Export), `concerts.rs` (neu: Facade mit den
  öffentlichen Typen `ConcertRow`, `ConcertFilter`, `DateHorizon`,
  `ConcertError` + Re-Export-Gerüst), `crates/reprise-gnome/src/ui/
  nav_history.rs` (beide Intent-Arme), `ui/concerts/mod.rs` +
  `ui/releases/mod.rs` (neu, nur mod-Deklarationen + `install`-Signaturen
  als `todo!`-freie Minimal-Stubs, die kompilieren: View-Aufbau folgt in
  C/R). TDD: label-/BrowserPlace-Roundtrip-Tests. Danach ist `cargo build`
  grün und alle Folge-Pakete berühren nur noch eigene Dateien.

### Welle 1 — Datenschicht (zwei Owner parallel, dann Pipeline, dann Similar)

- **Paket A · Provider (Owner A).** Dateien (alle neu):
  `crates/reprise-core/src/concerts/http.rs`, `backoff.rs`, `provider.rs`,
  `bandsintown.rs`, `ticketmaster.rs`, `dedupe.rs` + zugehörige
  `*_tests.rs`-Nachbarn. TDD: Parser-/Backoff-/Dedupe-Units, Fixture-Routen,
  MBID-Verifikations-Fälle.
- **Paket B · Domäne & Query (Owner B, parallel zu A).** Dateien (alle neu):
  `crates/reprise-core/src/concerts/geo.rs`, `geocode.rs`, `candidates.rs`
  (inkl. `seed_artists`), `refresh.rs`, `config.rs` (Settings-Reads +
  Env-Fallback + `similar_config`), `query.rs` (inkl.
  `query_unseen`/`count_unseen`/`mark_scope_seen`) + Tests. TDD: Haversine,
  Geocode-Parse, Kandidaten-Query, Due-Grenzwerte,
  Filter-/Sortier-/Seen-Semantik gegen In-Memory-V31.
- **Paket P · Pipeline & Resolution (Owner B, nach A+B).** Dateien (neu):
  `crates/reprise-core/src/concerts/pipeline.rs`, `resolution.rs`
  (+ Tests). TDD: Fixture-getriebene End-to-End-Läufe (Fallback-Kette
  Bandsintown→TM, Backoff-Abbruch, Upsert/Reconcile mit `seen_at`-Erhalt,
  Cleanup, Ledger-Outcomes, Summary, „keine Provider konfiguriert"-Pfad).
- **Paket S · Similar Artists (Owner B, nach P — Abschluss der
  Datenschicht).** Dateien: `crates/reprise-core/src/concerts/similar.rs`
  (neu + Tests), `concerts/pipeline.rs` (Similar-Kandidaten-Schritt),
  `concerts/config.rs` (nur falls `similar_config` nicht schon in B
  vollständig war) — gleicher Owner wie P/B, sequenziell, daher
  konfliktfrei. TDD: LB-/Last.fm-Parser, Quellen-Wahl, Caps, Dedupe gegen
  Library-Keys, Pipeline-Erweiterung (Abschnitt 12).
- **Paket M · CLI & MCP (eigener Owner, startet nach B; läuft parallel zu
  P/S und Welle 2).** Dateien: `crates/reprise-cli/src/cli.rs`, `main.rs`,
  `commands/mod.rs`, `commands/concerts.rs` (neu),
  `tests/concerts.rs` (neu); `crates/reprise-mcp/src/server.rs`, `data.rs`,
  `tests/resources.rs`. Keine Datei-Überschneidung mit irgendeinem
  anderen Paket. TDD: Integrationstests zuerst (Abschnitt 2.1/12).

### Welle 2 — Views (zwei Owner parallel + G, dann Verdrahtung)

- **Paket C · Concerts-View (Owner C; hängt an F3, Fetch-Teile an P).**
  Dateien: `crates/reprise-gnome/src/ui/concerts/{concerts_view,
  concerts_model, concerts_columns, concerts_presentation,
  concerts_filter_bar, concerts_empty_state, concerts_worker, css}.rs`
  (alle neu) + `ui/concerts/mod.rs` (F3-Stub ausfüllen) + `ui/style/mod.rs`
  (css-Registrierung — Achtung geteilte Datei mit R: die Registrierungszeile
  für `releases/css.rs` macht R **nicht** selbst, beide Zeilen legt C an?
  Nein — Regel „keine geteilten Dateien": **beide** css-Registrierungen
  (`concerts`, `releases`) übernimmt F3 als leere Sektions-Stubs, C und R
  füllen nur ihre eigenen css.rs) + `ui/browse/browse_bar.rs` (nur die eine
  `pub(in crate::ui)`-Zeile für `CHIP_CSS_CLASS` — liegt in F3, nicht in C,
  aus demselben Grund). Tasks sequenziell:
  **C1** Presentation+Model (pur) → **C2** View+Spalten+Empty-States
  (Flip CONC-3/CONC-4) → **C3** Filter-Bar (Flip CONC-2 + CONC-6:
  Source-Pill + Similar-Caption-Tests) → **C4** Worker+Footer+Trigger
  (nach P; Flip CONC-5). TDD je Task: pure Units zuerst, Display-Tests
  `#[ignore]`.
- **Paket R · Releases (Owner R, parallel zu C — keine gemeinsamen
  Dateien).** Core-Dateien: `crates/reprise-core/src/
  artist_news_parsing.rs` (+ `artist_news_parsing_tests.rs`),
  `artist_news_pipeline.rs` (+ `artist_news_pipeline_tests.rs`),
  `artist_news_query.rs` (+ `artist_news_query_tests.rs`),
  `artist_news.rs` (Facade-Re-Exports), `artist_news_view.rs` (neu +
  Tests). Gnome-Dateien: `crates/reprise-gnome/src/ui/releases/
  {releases_view, releases_model, releases_columns, releases_presentation,
  releases_filter_bar, releases_empty_state, css}.rs` (alle neu) +
  `ui/releases/mod.rs` (F3-Stub ausfüllen). Tasks sequenziell:
  **R1** Core-Datenseite (Owned-Drop raus, `unseen_release_count`
  owned-bereinigt, `artist_news_view.rs` mit Filter/Sort/Status/Counts) →
  **R2** Presentation+Model (pur: `releases_row_action`,
  `format_release_date`, Pills, Empty-Matrix) →
  **R3** View+Spalten+Status-Pills+Aktivierung (Flip NR-14-Teil
  Aktivierung/Empty) → **R4** Filter-Bar + Hide/Restore + Footer
  (Flip NR-14 vollständig). TDD je Task.
- **Paket G · Systemstandort (eigener Owner, unabhängig — parallel zu
  C/R/M).** Dateien: `crates/reprise-platform-linux/src/location.rs` (neu)
  + `src/lib.rs` (mod-Zeile). TDD: pure Vardict-/Policy-Units; der
  Prefs-Anteil folgt in D. Kein anderes Paket berührt platform-linux.
- **Task V · Verdrahtung beider Views (ein Owner, nach C und R).**
  Dateien: `ui/sidebar/sidebar_presentation.rs` (NavIcon::Concerts +
  NavIcon::Releases + Laufzeit-Fallbacks), `ui/sidebar/sidebar_rebuild.rs`
  (beide Rows + Counts + Gates, Reihenfolge Releases→Concerts→My Stats),
  `ui/window/window.rs` (beide Stack-Seiten + installs; Budget < 600),
  `ui/window/library_shell.rs` (beide Routing-Zweige),
  `ui/track_list/track_list_smoke.rs` (Smoke-Quellen `concerts` +
  `releases`), `ui/artist_news/artist_news_worker.rs` (nur falls die
  Releases-Footer-„Fetch now"-Durchreichung eine Handle-Zeile braucht).
  Flips: CONC-1 + NR-15. TDD: Sidebar-Rebuild-/Routing-Tests, Display-Smoke.

### Welle 3 — Updates-Popover (ein Owner, sequenziell)

- **Paket U · Popover-Umbau (hängt an V für die Sprungziele).**
  - **U1 · Move.** `git mv crates/reprise-gnome/src/ui/new_releases
    crates/reprise-gnome/src/ui/updates` + Referenz-Fix (`ui/mod.rs`,
    `ui/window/window.rs`, `ui/artist_news/artist_news_worker.rs`,
    `ui/preferences/*`, `ui/style/mod.rs`). Rein mechanisch, eigener
    Commit, keine Verhaltensänderung; Tests laufen unverändert
    (`history_page.rs` zieht mit um und stirbt erst in U3).
  - **U2 · Shell + Badge-Umbau.** Dateien: `ui/updates/popover.rs`
    (Sektions-Layout, Öffnungs-Stempeln beider Sektionen, gemeinsamer
    Footer „Fetch now beide Fetcher" + ältester Stand), `ui/updates/
    badge.rs` (`updates_badge(FeedBadgeInput, FeedBadgeInput)`),
    `ui/updates/popover_tests.rs`, `docs/ux-rules.md` (NR-3 →
    `[ersetzt durch NR-3a]`, NR-9 → `[ersetzt durch NR-9a]`, beide neu
    `[aktiv]`; Test-Renames `nr_3_*`→`nr_3a_*`, `nr_9_*`→`nr_9a_*` im
    selben Commit). TDD: `updates_badge`-Matrix zuerst.
  - **U3 · Concerts-Sektion + Sprungzeilen + History-Entfall.** Dateien:
    `ui/updates/concerts_section.rs` (neu: Delta-Rows im Mockup-Format,
    Hinweiszeile ohne Credentials), `ui/updates/popover.rs` (Integration,
    „Show all concerts (N) →" via Sidebar-Routing, History-Row wird zur
    „Show all releases (N) →"-Sprungzeile auf `ViewSource::Releases`;
    `show_history`/`HISTORY_PAGE`-Stack-Seite raus),
    `ui/updates/history_page.rs` (**gelöscht**),
    `crates/reprise-core/src/artist_news_history.rs`
    (`group_history`/`HistoryGroup`/`MONTH_NAMES` + Gruppierungs-Tests
    raus; Restore-Test → `nr_12a_*`), `ui/strings_news.rs`
    (History-Strings raus), `ui/window/window.rs` (Install-Signatur erhält
    die Routing-Callbacks), `docs/ux-rules.md` (NR-5a →
    `[ersetzt durch NR-5b]`, NR-12 → `[ersetzt durch NR-12a]`, beide neu
    `[aktiv]`; Renames `nr_5a_*`→`nr_5b_*`, `nr_12_*`→`nr_12a_*` im selben
    Commit). Flips: CONC-7, NR-5b, NR-12a. TDD: Delta-Query-Units (aus B) +
    Display-Navigation beider Sprungzeilen.

### Welle 4 — Preferences (ein Owner) + Abschluss

- **Paket D · Preferences.** Dateien: `ui/preferences/preference_
  concerts.rs` (neu), `ui/preferences/preference_plugins.rs`
  (concerts-Zweig), `ui/preferences/mod.rs` (mod-Zeile). Credentials-,
  Location-Rows (Nominatim-Apply via `one_shot_task`, „Use current
  location" via `platform_linux::location`, „Clear location"), Radius-,
  window_days-, Similar-Rows. TDD: Geocode-/Location-Apply-Effekte als pure
  Decision-Funktionen + Display-Test; Settings-Roundtrips (Env-Fallback
  weicht dem gesetzten Setting; Similar-SpinRow nur mit Schalter aktiv).
- **Z1 · Traceability + Headless-Smoke + Ledger.** `check-ux-traceability.
  sh` grün (CONC-1..7, NR-3a, NR-5b, NR-9a, NR-12a, NR-14, NR-15; keine
  Referenzen auf ersetzte IDs); End-to-End-Smoke mit vollständiger
  Isolation (`dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp
  -d) XDG_CACHE_HOME=$(mktemp -d) GDK_BACKEND=x11 WAYLAND_DISPLAY=
  REPRISE_AUDIO_SINK=fakesink REPRISE_CONCERTS_FIXTURE_DIR=… cargo run`):
  Modul an, Keys via Env, Fetch, beide Tabellen, Filter beider Views,
  Ticket-/Announcement-Aktivierung (UriLauncher-Seam), Hide/Restore,
  Popover-Badge-Summe + Stempeln (owned zählt nicht), Sprungzeilen,
  Offline-Neustart; `reprise-cli concerts list --json` gegen dieselbe DB;
  manueller Portal-Pass für „Use current location". Ledger-Zeile in
  `.superpowers/sdd/progress.md`.

### Ausbaustufen (nach v1)

- **N · Update-Benachrichtigungen** (optional, unentschieden):
  `gio::Notification` für neue Deltas — bewusst NICHT v1 (Abschnitt 13).

### Verifikation (jeder Commit)

`cargo fmt --check` · `cargo clippy --all-targets --workspace -- -D warnings`
· `cargo test --workspace` · `cargo audit` (einzige akzeptierte Advisory
RUSTSEC-2024-0436) · nach Core-Änderungen
`cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` leer ·
Skript-Gates `check-architecture.sh`, `check-motion-tokens.sh`,
`check-input-parity.sh`, `check-accessibility-semantics.sh`,
`check-display-tests.sh`, `check-ux-traceability.sh`. Nicht headless
verifizierbar (manueller Pass): Icon-Optik je Theme, Browser-Öffnen echter
Ticket-/Announcement-Links, Location-Portal-Dialog, Scrollgefühl,
Hover-Haptik der Chips.
