# Last.fm-Scrobbling — Implementierungsplan

## Globale Randbedingungen

TDD RED→GREEN; englischer Code/UI und deutsche interne Dokumentation; keine realen
Konten, API-Keys, Secrets, Session-Keys, Musikdateien oder Nutzerdaten; kein
Produktionsnetzwerk vor Opt-in; Core-Reinheit ohne GTK/libadwaita/GStreamer/zbus;
Credentials ausschließlich im Secret Service; jede neu angelegte oder wesentlich
bearbeitete Datei unter 800 Zeilen; vor jedem Task-Commit `cargo fmt --check`,
striktes Workspace-Clippy, `cargo test --workspace`, `cargo audit` und bei
Core-Änderungen der Reinheitsbeweis; adversariales Diff-Review nach jeder Aufgabe;
niemals pushen.

Basis: `886849f` mit 596 bestandenen und 14 ignorierten Tests. Parallel in `main`
landende Arbeit kann absolute Zahlen erhöhen; verbindlich sind Testzuwächse und
grüne Gates.

## Aufgabe 1 — Signierter Last.fm-Core-Transport

**Dateien:**

- aufteilen: `crates/reprise-core/src/scrobbling.rs` nach
  `crates/reprise-core/src/scrobbling/{mod.rs,listenbrainz.rs,lastfm.rs}`
- ändern: `crates/reprise-core/Cargo.toml`, `Cargo.lock`

**Schnittstellen:**

```rust
pub struct LastFmClient { /* api_key, shared_secret, roots, agent */ }
pub struct LastFmSession { pub user_name: String, pub session_key: String }
impl LastFmClient {
    pub fn new(api_key: String, shared_secret: String) -> Result<Self, MetadataError>;
    pub fn request_token(&self) -> Result<String, TransportError>;
    pub fn authorization_url(&self, token: &str) -> Result<String, MetadataError>;
    pub fn exchange_token(&self, token: &str) -> Result<LastFmSession, TransportError>;
}
impl ScrobblerTransport for LastFmClient { /* user.getInfo, now playing, scrobble */ }
```

RED: Tests für sortierte Signatur ohne `format`/`callback`, leere Credentials,
Auth-URL-Encoding, Request-/Session-JSON, Now-Playing-Parameter, 50er-Batch,
Unix-Startzeit, Success/ignored, Fehler 9/4, temporäre Fehler und Secret-Leakage
anlegen; gezielt kompilieren und erwartetes Fehlschlagen sehen. GREEN: mit
`md-5`, `ureq`, `BTreeMap` und begrenzter JSON-Antwort minimal implementieren.
ListenBrainz-Tests müssen durch die reine Dateiaufteilung unverändert bleiben.
Erwarteter Zuwachs: mindestens 12 Tests.

Commit: `feat: add Last.fm signed scrobbling transport`.

## Aufgabe 2 — Unabhängige dauerhafte Last.fm-FIFO

**Dateien:**

- ändern: `crates/reprise-core/src/db.rs`
- ändern: `crates/reprise-core/src/scrobbling/mod.rs`

**Schnittstellen:**

```rust
pub enum ScrobbleProvider { ListenBrainz, LastFm }
pub fn enqueue_for(conn: &Connection, provider: ScrobbleProvider, listen: &Listen) -> Result<i64, QueueError>;
pub fn pending_for(conn: &Connection, provider: ScrobbleProvider, limit: usize) -> Result<Vec<Listen>, QueueError>;
pub fn acknowledge_for(conn: &Connection, provider: ScrobbleProvider, ids: &[i64]) -> Result<(), QueueError>;
pub fn clear_pending_for(conn: &Connection, provider: ScrobbleProvider) -> Result<usize, QueueError>;
pub fn pending_count_for(conn: &Connection, provider: ScrobbleProvider) -> Result<usize, QueueError>;
```

RED: Schema-v6-Upgrade von v5, frische DB, Erhalt vorhandener ListenBrainz-Zeilen,
Providerisolation, Last.fm-FIFO, 50er Requestlimit, selektives Ack, Clear nur eines
Providers und Reopen testen. GREEN: atomare v6-Migration mit `lastfm_queue`,
vertrauenswürdige Enum-Tabellennamen und Wrapper für bestehende ListenBrainz-API.
Erwarteter Zuwachs: mindestens 8 Tests.

Commit: `feat: persist provider-specific scrobble queues`.

## Aufgabe 3 — Gemeinsamer Worker und Dual-Provider-Playback

**Dateien:**

- verschieben/ändern: `crates/reprise-gnome/src/ui/listenbrainz_runtime.rs` nach
  `crates/reprise-gnome/src/ui/scrobble_runtime.rs`
- ändern: `crates/reprise-gnome/src/ui/{mod.rs,play_tracking.rs,player_controller.rs,window.rs,window_smoke.rs}`
- ändern: `crates/reprise-gnome/src/ui/preference_listenbrainz.rs`

**Schnittstellen:**

```rust
pub struct ScrobbleRuntime;
impl ScrobbleRuntime {
    pub fn new(database_path: PathBuf, provider: ScrobbleProvider, service: &'static str) -> Rc<Self>;
    pub fn configure(self: &Rc<Self>, credential: String, transport: Box<dyn ScrobblerTransport>);
}
```

RED: vorhandene Worker-Tests providerselektiert umstellen; zusätzlich beweisen,
dass Last.fm-Ack ListenBrainz-Zeilen nicht löscht, ein Last.fm-Fehler
ListenBrainz-Erfolg nicht blockiert, beide aktiven Provider Now-Playing erhalten
und ein Abschlusslisten unabhängig zweimal einreiht. GREEN: Worker ohne
ListenBrainz-Namen generalisieren, Clients von außen injizieren, zwei Runtimes im
Controller/Window verdrahten und genau eine ScrobbleSession behalten. Bestehender
ListenBrainz-Smoke erhält explizit seinen Loopback-Client. Erwarteter Zuwachs:
mindestens 5 Tests.

Commit: `refactor: share scrobble runtime across providers`.

## Aufgabe 4 — Sichere Last.fm-Desktop-Authentifizierung und Plugin-UI

**Dateien:**

- neu: `crates/reprise-gnome/src/ui/lastfm_secret.rs`
- neu: `crates/reprise-gnome/src/ui/preference_lastfm.rs`
- ändern: `crates/reprise-core/src/modules.rs`
- ändern: `crates/reprise-gnome/src/ui/{mod.rs,preferences.rs,strings.rs,window.rs}`
- ändern: `crates/reprise-gnome/Cargo.toml`, `Cargo.lock`, `po/de.po`,
  `flatpak/cargo-sources.json`

**Schnittstellen:**

```rust
pub struct LastFmCredentials { /* api key, shared secret, session key, user */ }
pub async fn load() -> Result<Option<LastFmCredentials>, SecretError>;
pub async fn store(credentials: &LastFmCredentials) -> Result<(), SecretError>;
pub async fn delete() -> Result<(), SecretError>;
```

RED: Registry/default-off/key, Live-Plugin, stabile Secret-Attribute ohne Werte,
Credential-JSON-Roundtrip ohne `Debug`, Auth-Dialogzustände, URI-Launch-Entscheid
und Statusformat testen. GREEN: maskierte Secret-Zeile, zweistufige
Browserautorisierung, off-main Token-/Sessioncalls, Keyring-only-Persistenz,
Startup-Bootstrap, Status und Trennen inklusive ausschließlich Last.fm-Queue
implementieren. Browserstart geschieht nur durch Nutzeraktion. gettext und
Flatpak-Cargoquellen regenerieren. Erwarteter Zuwachs: mindestens 8 Tests plus
ein ignorierter Displaytest.

Commit: `feat: configure Last.fm securely`.

## Aufgabe 5 — Datenschutz, Smoke und Stage-Abschluss

**Dateien:**

- ändern: `README.md`, `data/org.reprise.Reprise.metainfo.xml`, `RELEASING.md`
- ändern: `docs/agent-workflow/MANUAL-QA.md`
- ändern: `crates/reprise-gnome/src/ui/window_smoke.rs`
- ändern: `docs/agent-workflow/STATUS.md` erst beim koordinierten Merge

README/AppStream nennen Opt-in, BYO-API-Zugangsdaten, exakte übertragenen Felder,
Keyring und getrennte Offline-Queue. Manual-QA erhält API-Konto-, Browser-,
Widerrufs- und Offlineprüfungen. Ein Debug-only Smoke akzeptiert ausschließlich
`http://127.0.0.1:<port>` oder `http://[::1]:<port>`, verwendet feste synthetische
Credentials/Metadaten und prüft signiertes `user.getInfo` plus
`track.scrobble` gegen einen lokalen Fake-Endpunkt. Der App-Run enthält zwingend
private XDG-Daten/Cache, eigene D-Bus-Session, Xvfb, X11, leeres
`WAYLAND_DISPLAY`, fakesink und Auto-Quit.

Danach `scripts/check-release.sh`, Core-Reinheit, Dateigrößen, ignorierten
Last.fm-Displaytest und Whole-Branch-Review ausführen; kritische/wichtige Befunde
beheben und Gates wiederholen. Ledger aktualisieren. Beim Merge aktuellen Main in
den Feature-Branch integrieren und erneut prüfen, Main-Lock erst bei sauberem
`FREE` claimen, Feature mergen, STATUS/Lock abschließen. Nicht pushen.

Commit: `docs: document Last.fm privacy and QA`.
