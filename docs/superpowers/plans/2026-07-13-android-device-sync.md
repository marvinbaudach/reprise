# Android-Gerätesynchronisation — Implementierungsplan

## Globale Randbedingungen

- Grundlage ist
  `docs/superpowers/specs/2026-07-13-android-device-sync-design.md`.
- TDD für jede Aufgabe: RED beobachten, kleinstes GREEN implementieren,
  gezielte Tests, vollständige Gates, adversarielle Diff-Prüfung, Commit.
- Englischer Code, Kommentare, UI-Texte und Commits; deutsche interne
  Dokumentation und vollständige deutsche gettext-Übersetzung.
- Keine reale Musik, Datenbank, Desktop-Session oder USB-/MTP-Hardware in
  automatischen Läufen. Geräte-I/O wird ausschließlich gegen temporäre lokale
  GIO-Fixtures geprüft.
- Jeder App-/Displaylauf enthält vollständig `dbus-run-session`, `xvfb-run`,
  frische `XDG_DATA_HOME`/`XDG_CACHE_HOME`, `GDK_BACKEND=x11`, leeres
  `WAYLAND_DISPLAY` und `REPRISE_AUDIO_SINK=fakesink`.
- `reprise-core` bleibt frei von gtk4/libadwaita/gstreamer/zbus/glib/gio.
- Jede erstellte oder wesentlich geänderte Datei endet unter 800 Zeilen.
- Keine Lösch-, Spiegel-, Transcoding-, ADB-, direkte USB- oder `libmtp`-
  Funktion hinzufügen.

## Aufgabe 1 — Reines Queue-, Pfad- und Playlistmodell

**Dateien:**

- erstellen: `crates/reprise-core/src/device_sync.rs`
- ändern: `crates/reprise-core/src/lib.rs`

**Schnittstellen:**

```rust
pub const REPRISE_DEVICE_DIR: &str = "Reprise";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncTrack {
    pub id: i64,
    pub source_path: PathBuf,
    pub original_name: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: i64,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncJob {
    pub id: u64,
    pub playlist: String,
    pub tracks: Vec<SyncTrack>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncPhase { Idle, Preparing, Copying, PausedDisconnected, Cancelling, Complete, Failed }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncSnapshot {
    pub phase: SyncPhase,
    pub current_name: Option<String>,
    pub current_bytes: u64,
    pub current_total: Option<u64>,
    pub completed_tracks: usize,
    pub total_tracks: usize,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub queued_jobs: usize,
    pub copied: usize,
    pub skipped: usize,
    pub failed: usize,
    pub message: Option<String>,
}

pub struct DeviceQueue { /* private FIFO and active state */ }

impl DeviceQueue {
    pub fn new() -> Self;
    pub fn enqueue(&mut self, job: SyncJob);
    pub fn start_next(&mut self) -> Option<SyncJob>;
    pub fn begin_track(&mut self, name: &str, total: Option<u64>);
    pub fn set_track_bytes(&mut self, copied: u64);
    pub fn finish_track(&mut self, outcome: TrackOutcome);
    pub fn finish_job(&mut self);
    pub fn request_cancel(&mut self);
    pub fn pause_disconnected(&mut self);
    pub fn resume(&mut self);
    pub fn snapshot(&self) -> SyncSnapshot;
}

pub fn safe_component(input: &str, fallback: &str) -> String;
pub fn track_relative_path(playlist: &str, track: &SyncTrack) -> String;
pub fn merge_playlist_entries(existing: &[M3uEntry], appended: &[M3uExportEntry]) -> String;
```

1. RED: Tests anlegen, die Traversal (`../`, `/`, `\\`, Steuerzeichen,
   `.`/`..`), Unicode, leere Namen, kollidierende Originaldateinamen und
   relative `Playlist/<id>-<name>`-Ziele festnageln. Ausführen und den fehlenden
   Modul-/Symbolfehler beobachten.
2. GREEN: `safe_component` und `track_relative_path` ohne Dateisystemzugriff
   implementieren. Kein Pfad darf absolut sein oder `ParentDir` enthalten.
3. RED/GREEN: M3U8-Merge testen und implementieren: bestehende Reihenfolge
   bleibt, neue Pfade werden stabil/eindeutig angehängt, `#EXTINF` wird aus den
   neuen Trackdaten erzeugt.
4. RED/GREEN: FIFO-Tests für drei Jobs desselben Geräts, genau einen aktiven
   Job, monotone/clamped Bytewerte, Copied/Skipped/Failed-Zähler, Cancel nur
   aktiv, Disconnect/Resume und automatischen Übergang zum nächsten Job.
5. Gezielte Core-Tests, vollständige Gates, Core-Purity und Dateigröße.
6. Commit: `feat: add device sync queue model`.

**Erwartete neue Tests:** mindestens 14 reine Core-Tests.

## Aufgabe 2 — GIO-MTP-Geräte und begrenzter Storage-Adapter

**Dateien:**

- ändern: `crates/reprise-platform-linux/Cargo.toml`
- ändern: `crates/reprise-platform-linux/src/lib.rs`
- erstellen: `crates/reprise-platform-linux/src/device_sync.rs`

**Schnittstellen:**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceDescriptor {
    pub id: String,
    pub name: String,
    pub root_uri: String,
    pub icon: gio::Icon,
    pub reconnectable: bool,
}

pub fn descriptor_from_mount(mount: &gio::Mount) -> Option<DeviceDescriptor>;

pub struct DeviceMonitor { /* VolumeMonitor + subscriptions */ }

impl DeviceMonitor {
    pub fn new() -> Self;
    pub fn devices(&self) -> Vec<DeviceDescriptor>;
    pub fn subscribe(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>);
}

#[derive(Clone)]
pub struct DeviceStorage { root: gio::File }

impl DeviceStorage {
    pub fn from_root(root: &gio::File) -> Self;
    pub async fn inspect(&self) -> Result<DeviceContents, DeviceIoError>;
    pub async fn available_bytes(&self) -> Result<Option<u64>, DeviceIoError>;
    pub async fn copy_track(
        &self,
        source: &gio::File,
        relative_target: &str,
        expected_size: u64,
        cancellable: &gio::Cancellable,
        progress: impl FnMut(u64, u64) + 'static,
    ) -> Result<CopyOutcome, DeviceIoError>;
    pub async fn read_playlist(&self, name: &str) -> Result<Vec<M3uEntry>, DeviceIoError>;
    pub async fn replace_playlist(&self, name: &str, contents: Vec<u8>) -> Result<(), DeviceIoError>;
}
```

1. `gio = "0.22"` ergänzen. RED: reine URI-Projektion hinter einer kleinen
   Hilfsfunktion testen: nur `mtp` akzeptieren; UUID bevorzugen; URI-Fallback
   als nicht reconnectable markieren. Danach Mountprojektion implementieren.
2. `DeviceMonitor` auf dem GTK-Hauptkontext implementieren. Mount-added,
   mount-changed und mount-removed projizieren jeweils eine vollständig neu
   gelesene immutable Liste; keine Callback-Ausleihe über Signalaufrufe halten.
3. RED: lokale temporäre `gio::File`-Fixture für leeren/nicht vorhandenen
   `Music/Reprise`-Baum, rekursive Audioerkennung, `.m3u8`-Erkennung und
   Nicht-Audio-Dateien anlegen. `inspect` asynchron implementieren.
4. RED: Copy-Test mit zwei Dateien und Progresscallback. GREEN: notwendige
   Verzeichnisse anlegen, Same-size als Skipped erkennen, sonst zuerst in
   `<target>.reprise-part` kopieren und erst vollständig ersetzen. Nur
   validierte relative Unterpfade von `Music/Reprise` akzeptieren.
5. RED/GREEN: Cancel löscht die eigene Partialdatei bestmöglich; fremde Datei
   bleibt. M3U8 wird über eigene temporäre Datei ersetzt. Freien Speicher über
   `filesystem::free` lesen und fehlendes Attribut als `None` behandeln.
6. Gezielte Plattformtests, Gates, Core-Purity und Dateigröße.
7. Commit: `feat: add gio android device adapter`.

**Erwartete neue Tests:** mindestens 9 Plattformtests.

## Aufgabe 3 — Anwendungslanger Runtime und Datenbankvalidierung

**Dateien:**

- erstellen: `crates/reprise-gnome/src/ui/device_sync_runtime.rs`
- ändern: `crates/reprise-gnome/src/ui/mod.rs`
- ändern: `crates/reprise-gnome/src/ui/window.rs`
- ändern: `crates/reprise-core/src/queries/maintenance.rs`
- ändern: `crates/reprise-core/src/queries/mod.rs`

**Schnittstellen:**

```rust
pub fn query_sync_tracks(conn: &Connection, ids: &[i64]) -> Result<Vec<SyncTrack>, rusqlite::Error>;

pub struct DeviceSyncRuntime { /* monitor, per-device queues/workers, observers */ }

impl DeviceSyncRuntime {
    pub fn new(conn: &Rc<RefCell<Connection>>, monitor: DeviceMonitor) -> Rc<Self>;
    pub fn devices(&self) -> Vec<DeviceView>;
    pub fn enqueue(&self, device_id: &str, playlist: &str, ids: &[i64]) -> Result<usize, EnqueueError>;
    pub fn cancel_current(&self, device_id: &str);
    pub fn refresh_contents(&self, device_id: &str);
    pub fn subscribe(&self, callback: Rc<dyn Fn(DeviceSyncState)>) -> Subscription;
}
```

1. RED: Querytests beweisen Inputreihenfolge, Deduplizierung, Ausschluss
   unbekannter/missing IDs, lokale existierende Pfade und vollständige
   SyncTrack-Metadaten. Implementieren, ohne Dateien anzufassen.
2. Einen injizierbaren `DeviceBackend`-Trait im Runtime-Modul definieren; der
   Produktadapter delegiert an Aufgabe 2, Tests nutzen einen deterministischen
   Fake. Dadurch werden Queue- und Fehlerpfade ohne Hardware vollständig
   ausführbar.
3. RED/GREEN: Zwei rasche Enqueues auf dasselbe Gerät dürfen im Fake maximal
   eine gleichzeitige Copy sehen und müssen FIFO enden. Zwei Geräte dürfen je
   einen eigenen Worker besitzen.
4. RED/GREEN: Snapshots enthalten Datei/Gesamtbytes, Titelzähler und wartende
   Jobs; Subscriber bekommt sofort den aktuellen Stand und spätere Updates.
   Callback immer aus `RefCell` herausklonen.
5. RED/GREEN: Cancel, Disconnect, stabile Wiederverbindung und nicht stabile
   Wiederverbindung gemäß Spec. Worker-Generation verhindert stale Updates.
6. Runtime am Window-Composition-Root genau einmal erzeugen und in den
   PreferencesContext injizieren; Preferences-Schließen zerstört sie nicht.
7. Gates, Core-Purity, Dateigrößen, adversarielle Nebenläufigkeitsprüfung.
8. Commit: `feat: run sequential device sync jobs`.

**Erwartete neue Tests:** mindestens 8 GNOME-Runtime- und 3 Core-Querytests.

## Aufgabe 4 — Synchronisationsseite, Gerätebrowser und Handy-Playlists

**Dateien:**

- erstellen: `crates/reprise-gnome/src/ui/preference_sync.rs`
- erstellen: `crates/reprise-gnome/src/ui/device_sync_strings.rs`
- ändern: `crates/reprise-gnome/src/ui/preferences_window.rs`
- ändern: `crates/reprise-gnome/src/ui/preferences.rs`
- ändern: `crates/reprise-gnome/src/ui/mod.rs`
- ändern: `po/POTFILES.in`, `po/de.po`, `po/reprise.pot`

**Schnittstellen:**

```rust
pub(super) fn build_page(runtime: &Rc<DeviceSyncRuntime>) -> adw::PreferencesPage;
fn device_row(device: &DeviceView, runtime: &Rc<DeviceSyncRuntime>) -> adw::ActionRow;
fn present_device(window: &adw::Window, device_id: &str, runtime: &Rc<DeviceSyncRuntime>);
```

1. RED: `PAGE_ORDER` erwartet Playback, Appearance, Layout, Library,
   Synchronization, Plugins und sechs Seiten. PageId/Builder-Array ohne
   `Vec`-Sonderpfad auf sechs erweitern; `preferences.rs` wegen 770 Zeilen nur
   durch Delegation an das neue Modul anfassen.
2. RED/GREEN Displaytest: keine Geräte zeigt StatusPage mit Anleitung zu
   Entsperren/USB-Dateiübertragung; Geräteprojektion zeigt Systemicon, Name,
   Speicher und Zustand. Runtime-Update aktualisiert vorhandene Zeilen ohne
   zweite Seite/zweiten Runtime.
3. Geräteklick öffnet eine nichtmodale Detailansicht mit Reprise-Playlists,
   Plus-Aktion und erkannter Musik (Dateiname, relativer Pfad, Größe). Scan-
   Spinner/Progress sowie Fehler/Disconnect sind sichtbar; Generation verwirft
   späte Scanergebnisse.
4. Plus-Aktion verwendet den bestehenden Namensdialog, normalisiert den Namen
   über Core und legt nur einen Runtime/UI-Playlistentwurf an. Kein Geräte-I/O
   vor dem ersten Drop.
5. Progresskarte projiziert jeden Snapshot mit aktuellem Dateinamen,
   Datei-/Gesamtfraction, Bytes, `x of y tracks`, queued jobs, Countern und
   Cancel-current-Aktion. Reopen-Displaytest beweist identischen Live-Snapshot.
6. Alle neuen Texte im eigenen Stringmodul markieren, deutsche Übersetzungen
   ergänzen, Kataloge aktualisieren, Displaytests isoliert ausführen.
7. Gates, Dateigrößen und Diff-Prüfung.
8. Commit: `feat: add synchronization preferences`.

**Erwartete neue Tests:** mindestens 4 reine und 3 Displaytests.

## Aufgabe 5 — Track-DnD bis zur echten sequenziellen Kopie

**Dateien:**

- ändern: `crates/reprise-gnome/src/ui/preference_sync.rs`
- ändern: `crates/reprise-gnome/src/ui/device_sync_runtime.rs`
- ändern: `crates/reprise-gnome/src/ui/track_list_dnd.rs` nur falls eine
  öffentliche Payload-Hilfsgrenze nötig ist
- erstellen: `crates/reprise-gnome/src/ui/device_sync_smoke.rs`
- ändern: `crates/reprise-gnome/src/ui/mod.rs`

1. RED: Payloadtests beweisen, dass ausschließlich der etablierte Reprise-
   Stringpayload akzeptiert wird und leere/fremde/ungültige IDs keinen Auftrag
   erzeugen. Parsing wiederverwenden, kein zweites Format einführen.
2. Auf jeder Handy-Playlist einen COPY-DropTarget installieren. Drop löst IDs
   erneut über `query_sync_tracks`, reiht genau einen Auftrag ein und bestätigt
   Anzahl/Queueposition sichtbar. Der GTK-Callback selbst kopiert nichts.
3. RED/GREEN Runtime-Integration mit lokalem GIO-Backend: zwei unmittelbare
   Drops A und B auf dasselbe Gerät ergeben auf dem Backend strikt
   `A1,A2,B1`, niemals parallele Calls; Datei- und Gesamtfortschritt sind
   monoton und die resultierende `.m3u8` enthält relative eindeutige Einträge.
4. Cancel-Integration: laufende Partialdatei verschwindet, fertige Datei und
   wartender zweiter Auftrag bleiben; zweiter Auftrag startet anschließend.
5. `REPRISE_SMOKE_DEVICE_ROOT=<temp>` plus explizite Track-/Playlistfixture
   implementieren. Der Hook ist nur bei gesetztem Env aktiv, verwendet keinen
   VolumeMonitor und loggt enqueue/progress/complete für den isolierten Smoke.
6. Isolierten App-Smoke mit zwei Drops/Jobs, erwarteten Dateien/M3U8 und
   Log-Gate ohne GTK/GLib/panic/RefCell-Fehler ausführen.
7. Vollständige Gates, Dateigrößen, Core-Purity und adversarielle Prüfung.
8. Commit: `feat: copy tracks to android playlists`.

**Erwartete neue Tests:** mindestens 5 Runtime-/Payloadtests und 1 Displaytest.

## Aufgabe 6 — Flatpak, Dokumentation und Stage-Abschluss

**Dateien:**

- ändern: `org.reprise.Reprise.yml`
- ändern: `README.md`
- ändern: `RELEASING.md`
- ändern: `docs/agent-workflow/MANUAL-QA.md`
- ändern: `docs/agent-workflow/STATUS.md`
- ändern: `.superpowers/sdd/progress.md` außerhalb des Git-Worktrees

1. RED: Release-/Manifestcheck ergänzen, falls die bestehenden Checks die zwei
   exakten GVfs-Rechte nicht abdecken. Erwarteten Fehler beobachten.
2. Manifest ergänzt nur `--talk-name=org.gtk.vfs.*` und
   `--filesystem=xdg-run/gvfsd`; keine direkte USB-/Host-Freigabe.
3. README/Release/Manual-QA dokumentieren MTP-/GVfs-Voraussetzungen,
   verwalteten Zielbereich, `.m3u8`, sequenzielle Queue, Fortschritt, Cancel,
   Disconnect/Resume und die echten Hardwarechecks.
4. Alle Displaytests einzeln vollständig isoliert ausführen. Danach zentralen
   Releasechecker, fmt, Clippy, Workspace-Tests, Audit, Rustdoc, Core-Purity,
   gettext, Flatpak-Quellen, Dateigrößen und Meson-DESTDIR-Install ausführen.
5. Whole-branch Review: keine Dateioperation außerhalb `Music/Reprise`, keine
   parallelen Same-device-Copies, keine stale Progressupdates, kein RefCell-
   Borrow über Callback/GIO, kein unvollständiges M3U8-Replace.
6. Progress-Ledger taskweise vervollständigen, Status aktualisieren, Lock
   freigeben.
7. Commit: `docs: complete android device synchronization`.

**Erwartete neue Tests:** mindestens 1 Releasecheck; Gesamtzuwachs des Plans
mindestens 48 Tests einschließlich Displaytests.

