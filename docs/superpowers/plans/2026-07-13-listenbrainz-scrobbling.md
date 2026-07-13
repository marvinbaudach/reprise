# ListenBrainz-Scrobbling — Implementierungsplan

## Globale Randbedingungen

TDD RED→GREEN; englischer Code/UI und deutsche interne Dokumentation; keine realen
Konten, Tokens, Musikdateien oder Nutzerdaten; keine Netzwerkübertragung vor Opt-in;
Core-Reinheit ohne GTK/libadwaita/GStreamer/zbus; Token ausschließlich im Secret
Service/Portal; jede neu angelegte oder wesentlich bearbeitete Datei unter 800
Zeilen; vor jedem Commit `cargo fmt --check`, striktes Workspace-Clippy,
`cargo test --workspace`, `cargo audit` und bei Core-Änderungen der Reinheitsbeweis;
adversariales Diff-Review nach jeder Aufgabe; niemals pushen.

Basis: `b967226` mit 543 bestandenen und 8 ignorierten Tests. Die absoluten Zahlen
können durch parallel in `main` landende Arbeit steigen; verbindlich sind die
jeweiligen Testzuwächse und grünen Gates.

## Aufgabe 1 — Core-Vertrag und ListenBrainz-HTTP

**Dateien:**

- neu: `crates/reprise-core/src/scrobbling.rs`
- ändern: `crates/reprise-core/src/lib.rs`

**Schnittstellen:**

```rust
pub struct TrackMetadata { /* artist, track, optional release, duration */ }
pub struct Listen { pub id: Option<i64>, pub listened_at: i64, pub track: TrackMetadata }
pub fn should_scrobble(position_ms: i64, duration_ms: i64) -> bool;
pub trait ScrobblerTransport: Send + 'static { /* validate, playing_now, submit */ }
pub struct ListenBrainzClient;
```

Zuerst Tests für exakte Hälfte, Vier-Minuten-Grenze langer Titel, ungültige
Dauer, leere Pflichtmetadaten, `playing_now` ohne `listened_at`, permanenter Listen
mit Startzeit, Authorization-Header und Fehlerklassifikation schreiben und rot
sehen. Danach serde-Datentypen, reinen Payload-Builder und `ureq`-Client mit
injizierbarer Basis-URL minimal implementieren. Ein lokaler TCP-Testserver ersetzt
das Internet. Erwarteter Zuwachs: mindestens 8 Tests.

Commit: `feat: add ListenBrainz scrobbling contract`.

## Aufgabe 2 — Dauerhafte Offline-Warteschlange

**Dateien:**

- ändern: `crates/reprise-core/src/db.rs`
- ändern: `crates/reprise-core/src/scrobbling.rs`

**Schnittstellen:**

```rust
pub fn enqueue(conn: &Connection, listen: &Listen) -> Result<i64, QueueError>;
pub fn pending(conn: &Connection, limit: usize) -> Result<Vec<Listen>, QueueError>;
pub fn acknowledge(conn: &Connection, ids: &[i64]) -> Result<(), QueueError>;
pub fn clear_pending(conn: &Connection) -> Result<usize, QueueError>;
```

Zuerst Migration-v5- und Queue-Tests rot sehen: Upgrade von v4, frische DB,
Pflichtfeldvalidierung, FIFO, Limit maximal 1000, Ack nur der bestätigten IDs,
Persistenz nach erneutem Öffnen und explizites Leeren. Dann die atomare Migration
und parameterisierten Queries implementieren. Erwarteter Zuwachs: mindestens 7
Tests.

Commit: `feat: persist pending ListenBrainz listens`.

## Aufgabe 3 — Worker und Wiedergabesitzung verdrahten

**Dateien:**

- neu: `crates/reprise-gnome/src/ui/listenbrainz_runtime.rs`
- neu: `crates/reprise-gnome/src/ui/scrobble_session.rs`
- ändern: `crates/reprise-gnome/src/ui/mod.rs`
- ändern: `crates/reprise-gnome/src/ui/player_controller.rs`
- ändern: `crates/reprise-gnome/src/ui/window.rs`

**Schnittstellen:**

```rust
pub enum ConnectionStatus { Disabled, Connecting, Connected { user_name: String, pending: usize }, Offline { pending: usize }, Unauthorized }
pub struct ListenBrainzRuntime;
impl ListenBrainzRuntime {
    pub fn new(database_path: PathBuf) -> Rc<Self>;
    pub fn configure(&self, token: String);
    pub fn disable(&self);
    pub fn playing_now(&self, track: TrackMetadata);
    pub fn flush(&self);
}
```

Zuerst pure Sitzungs- und Worker-State-Tests rot sehen: kein Event unter Schwelle,
genau ein Listen bei Titelwechsel/Ende, deaktiviert kein Einreihen, `playing_now`
erst nach erfolgreichem Start, FIFO-Ack bei Erfolg, Offline-Erhalt, 401 stoppt,
Generation verwirft alte Statuswerte. Dann minimalen dedizierten Worker, Backoff
und den einen bestehenden `evaluate_play_tracking`-/`play_track_id`-Pfad
verdrahten. Keine GTK- oder DB-RefCell-Borrows dürfen Callback/Send überleben.
Erwarteter Zuwachs: mindestens 8 Tests.

Commit: `feat: scrobble completed playback sessions`.

## Aufgabe 4 — Sichere Kontokonfiguration und Plugin-UI

**Dateien:**

- neu: `crates/reprise-gnome/src/ui/listenbrainz_secret.rs`
- neu: `crates/reprise-gnome/src/ui/preference_listenbrainz.rs`
- ändern: `crates/reprise-gnome/Cargo.toml`
- ändern: `Cargo.lock`
- ändern: `crates/reprise-core/src/modules.rs`
- ändern: `crates/reprise-gnome/src/ui/mod.rs`
- ändern: `crates/reprise-gnome/src/ui/preferences.rs`
- ändern: `crates/reprise-gnome/src/ui/strings.rs`
- ändern: `po/de.po`
- ändern: `flatpak/cargo-sources.json`

Zuerst Registry-, Live-Plugin-, Statusformat-, Dialogentscheidungs- und Secret-
Attributtests rot sehen. Danach `oo7` ohne Tokio-Feature integrieren, Token verdeckt
speichern/laden/löschen, Start-Bootstrap, Aktivierung ohne Token, Konto-Status und
Trennen samt Queue-Leerung implementieren. Keyring-Fehler bleiben sichtbar und
haben keinen Klartext-Fallback. gettext-Katalog und gepinnte Flatpak-Cargoquellen
aktualisieren. Erwarteter Zuwachs: mindestens 6 Tests plus ein ignorierter
Displaytest.

Commit: `feat: configure ListenBrainz securely`.

## Aufgabe 5 — Datenschutzdokumentation und Stage-Abschluss

**Dateien:**

- ändern: `README.md`
- ändern: `data/org.reprise.Reprise.metainfo.xml`
- ändern: `RELEASING.md`
- ändern: `docs/agent-workflow/MANUAL-QA.md`
- ändern: `docs/agent-workflow/STATUS.md` erst beim koordinierten Merge

README/AppStream nennen ListenBrainz, Opt-in, übertragene Felder, sicheren
Tokenstore und lokale Offline-Warteschlange. Manual-QA erhält die echten Keyring-,
Netzunterbrechungs- und Kontoprüfungen. Einen vollständig isolierten Headless-Smoke
mit privatem XDG-Daten-/Cachepfad, eigener D-Bus-Session, Xvfb, X11, leerem
Wayland-Display, fakesink und lokalem Fake-Endpunkt ausführen; keine
Produktionsanfrage.

Danach `scripts/check-release.sh`, Core-Reinheit, Dateigrößen und Whole-Branch-
Review. Kritische/wichtige Befunde beheben und Gates erneut ausführen. Ledger
aktualisieren. Beim Merge den Main-Lock erst beanspruchen, wenn er frei ist,
aktuelles `main` in den Feature-Branch integrieren, Konflikte und Gates erneut
prüfen, Feature-Branch nach `main` fast-forward/mergen, STATUS aktualisieren und
Lock wieder freigeben. Nicht pushen.

Commit: `docs: document ListenBrainz privacy and QA`.
