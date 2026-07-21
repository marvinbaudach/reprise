---
slug: multi-frontend-core
worktree: /home/marvin/Projects/reprise-multi-frontend-core
branch: feature/multi-frontend-core
phase: planned
codex_session:
created: 2026-07-21
---
# Multi-Frontend-Core — Architekturplan

Gegrillt und beschlossen 2026-07-21 (Sektion 7); Basis `origin/dev`
`797afa2dfa`. Dieses Dokument ist das Ausführungsdokument. Bewusst offen
sind nur noch: (a) die ML-Runtime-Wahl — entscheidet der Spike in Paket E
faktenbasiert; (b) die Datei-Listen der Pakete C und F — beim Paket-Start
gegen den dann aktuellen Stand zu verifizieren und exklusiv festzuzurren.

Ziel: Alle Oberflächen — die bestehende GTK/GNOME-App, ein eigenständiges
CLI, ein MCP-Server, künftig KDE/Qt, Windows, Android, iOS — bauen auf
demselben Rust-Core (`crates/reprise-core`) auf. CLI und MCP laufen als
eigene Prozesse, der MCP-Server auch **während** die App läuft. Änderungen
aus MCP/CLI erscheinen **live** in der laufenden App, ohne Neustart.

Cross-Surface-Features dieses Plans:

1. **Playlists erstellen** (CLI + MCP, live sichtbar in der App); das CLI
   zusätzlich mit `rename`/`delete` (Beschluss 3).
2. **Instrumental-Fassungen (Vocal-Removal, experimentell)**: explizit
   ausgewählte Songs werden per ML-Stem-Separation (Demucs-Klasse-Qualität —
   Qualität ist die Einschlussbedingung) verarbeitet, landen zunächst als
   sofort abspielbare **Staging-Renders** in der Konvertierungs-Playlist
   und werden erst durch eine explizite **Speichern-Entscheidung** zu
   echten, dauerhaften Instrumental-Tracks in einem dedizierten Ordner —
   als reguläre, klar als KI-manipuliert gekennzeichnete Bibliothekstitel.
   Ein Bibliotheksfilter kann KI-Musik ausblenden (opt-in, Beschluss 17).
   Ausgelöst über Kontextmenü, die Konvertierungs-Playlist, CLI und MCP;
   Fortschritt überall live. Das Feature ist als **experimentell**
   geschaltet und als isoliertes, entfernbares Paket geplant — es liegt
   nicht auf dem kritischen Pfad der Architekturarbeit. Ausgegeben wird
   ausschließlich die Instrumental-Spur (Beschluss 19).

Zwei früh erwogene Modelle wurden verworfen: ein globaler
„Gesang entfernen“-Schalter mit transparenter Substitution beim Abspielen
sowie ein rollierendes Render-Fenster mit flüchtigem Cache. Beschluss
(2026-07-21): explizite, dauerhafte Instrumental-Tracks statt Toggle und
Cache — einfacher, vorhersagbar, keine Eviction-Maschinerie. Genre-Remixes
bleiben gestrichen (Qualität), ebenso der billige DSP-Center-Cancel-Modus.

Verhältnis zu bestehenden Dokumenten:

- `docs/ux-rules.md` bleibt bindend und rangiert über diesem Plan.
- `docs/superpowers/specs/2026-07-19-audio-character-mcp-design.md` und
  `docs/plans/audio-character-mcp.md` (phase: ready-for-review) bleiben für
  Klangprofil/Mix-Planung gültig. Dieser Plan **zieht deren Stufe-2-Task M1
  vor** (Gründung von `crates/reprise-mcp`) und **erweitert die Tool-Domäne**
  (Beschlüsse 2, 10). Die Markierung des dortigen M1-Absatzes als
  „superseded by multi-frontend-core“ und der Supersessions-Vermerk zu D17
  im Spec-Dokument sind benannte Tasks in Paket I; M2–M5 und Stufe 1B
  bleiben unberührt. Klangprofil-Analyse (Stufe 1A) ist auf `dev` gemergt
  und wird weder angefasst noch vorausgesetzt.
- `docs/plans/android-sync.md` / `android-sync-handoff.md` betreffen
  Geräte-Sync (MTP), **nicht** ein Android-Frontend; unberührt. Die dortige
  V2-„Companion-App“ ist ein späterer Keim eines Android-Surface.
  Instrumental-Fassungen sind normale Tracks und damit automatisch
  sync-fähig — keine Sonderlogik nötig. Der dort als V2 notierte
  einheitliche Bottom-Slot wird von diesem Plan nicht angefasst
  (Beschluss 18).
- `motion-player` (planned), `mystats-optimization` (shipped),
  `accessibility-keyboard`, `ux-rules-*`: keine Überschneidung außer der
  Pflicht, neues sichtbares Verhalten zuerst als `[geplant]`-Regeln zu
  verankern.

## 1. Ist-Zustand — ehrliche Inventur

### 1.1 Was die Trennung heute schon leistet

Der Workspace ist real dreigeteilt, die Richtung wird mechanisch erzwungen
(`scripts/check-architecture.sh`, Core-Purity via `cargo tree`):

- **`reprise-core`** (MIT, dependency-pur: kein gtk/glib/gstreamer/zbus)
  besitzt fast die gesamte Fachlogik: `db` (open/migrate, Schema v18, WAL +
  `busy_timeout=5000` + `foreign_keys=ON` bereits gesetzt), `library`
  (Scanner mit atomarem Mark-Vanished, `notify`-Watcher, Playlists,
  Smart-Playlists, M3U, Settings, Session, Stats, Tag-Edit inkl.
  Tag-**Schreib**pfad über lofty, Trash-Fassade), `queries`/`view_source`
  (gefensterte 200-Zeilen-Fenster), `queue` (+ Snapshot),
  `audio_analysis`/`sound_profile` (Stufe 1A gemergt), `device_sync`,
  `modules` (persistierte Feature-Flags), Cover-/Lyrics-/MusicBrainz-/
  Scrobbling-Pfade sowie die Plattform-**Verträge** `playback`,
  `media_integration`, `waveform`, `fingerprint`, `audio_analysis`-Backend.
- **`reprise-platform-linux`** (MIT): GStreamer-Player (playbin3, Effekte,
  Gapless/Crossfade), MPRIS via zbus, MTP inkl. Opus-Transcode-Pipeline,
  Trash, GStreamer-Analyse-Adapter (gestreamte PCM-Chunks über AppSink —
  ein vorhandener, wiederverwendbarer Decode-Pfad).
- **`reprise-gnome`** (GPL-3.0-or-later, einziges Binary `reprise`):
  Präsentation und Interaktion; SQL im Frontend ist per Gate verboten.

Für Mehrprozess-Betrieb günstig vorgefunden:

- Mehrere Connections über denselben DB-Pfad sind heute schon Alltag
  **innerhalb** der App: UI hält `Rc<RefCell<Connection>>`; Scan-Worker,
  Watcher-Thread und Analyse-Worker öffnen je eine **eigene** Connection
  (dokumentiert in `library/watcher.rs`). Der Schritt zu mehreren Prozessen
  ist bei SQLite/WAL derselbe Mechanismus.
- `notify` ist bereits Core-Dependency (bewusst als plattformübergreifende
  Abstraktion inotify/FSEvents/ReadDirectoryChangesW).
- GApplication ist single-instance (`org.reprise.Reprise`); ein zweiter
  App-Start fasst die DB gar nicht an. MPRIS existiert als
  prozessübergreifende Playback-Schnittstelle.
- LICENSING.md plant fremde/proprietäre Frontends über den MIT-Engine-Pfad
  ein und enthält bereits ein Lizenz-Gate für Audio-Analyse-Modelle, das
  sich wörtlich auf Separationsmodelle ausweiten lässt.

Seit PR #23/#29 liegen zusätzlich Similar Mix + Artist Discovery sowie die
Song-Visuals auf `dev`; PR #29 hat die Browser-Navigation zentralisiert
(`ui/window/window_navigation.rs`, `ui/window/library_shell.rs`,
`ui/browse/**`) und die alten Library-Modi durch eine scoped Track-List
ersetzt. Das prägt die Datei-Listen der Pakete C und F (Sektion 4) und den
Ideen-Parkplatz (Sektion 8).

Testbaseline: verbindlich ist der **aktuelle Ledger-Stand beim
Paket-Start** (`.superpowers/sdd/progress.md`) — die zuletzt parallel
gemergten Stände (Similar Mix, Song-Visuals, Single-Track-Browser) tragen
leicht unterschiedliche Zählungen; die beim P0-Start gemessene Zahl ist
die Referenz-Baseline, gegen die jedes Paket grün bleibt.

### 1.2 Wo die Nähte lecken

1. **Kein Änderungssignal zwischen Prozessen.** Kein Event-/Change-Konzept in
   Core. Die App refresht sich nach eigenen Aktionen selbst
   (`sidebar.refresh(reason)`, Watcher-Kanal für Dateisystem-Events); ein
   fremder DB-Schreiber bliebe unsichtbar. SQLite-Update-Hooks lösen das
   nicht: sie feuern nur für die **eigene** Connection, nie für fremde
   Prozesse.
2. **Kein Schema-Zukunftsschutz.** `db::migrate` läuft 1..=18 hoch, prüft
   aber nie `user_version >` Zielversion. Sobald App, CLI und MCP getrennt
   aktualisiert werden, arbeitet ein älteres Binary stumm auf einem neueren
   Schema. Wird fail-closed (Beschluss 8, P0).
3. **Orchestrierung klebt in `reprise-gnome`.** Scan-Worker, Analyse-
   Scheduler, Cover-Batch, Scrobble-Runtime sind GTK-frei denkbar, aber in
   `ui/*`-Runtimes verdrahtet. Für CLI/MCP v1 unkritisch (sie rufen
   Fassaden); für spätere native Frontends die größte offene Schuld. Dieser
   Plan verschiebt bewusst nichts davon (2.3).
4. **Lange Schreibtransaktion beim Scan.** `scan_folder` ist eine
   Transaktion (Walk + Mark-Vanished, bewusst atomar). Während großer
   Rescans kann ein externer Schreiber die 5 s `busy_timeout` reißen —
   Betriebsfenster, per Retry abzufedern.
5. **Kein CLI, kein MCP, keine Provenance.** Einziges Binary ist `reprise`.
   Kein `mcp`/`rmcp`-Code im Workspace (`crates/reprise-mcp` existiert nur
   als Plan). Das Schema kennt weder abgeleitete Tracks/Provenance noch
   Jobs (Tabellen: tracks, playlists(+tracks), smart_playlists, settings,
   listen_events, device_*, import_errors, new_releases,
   track_audio_analysis, Scrobble-Queues).
6. **Playback-Zustand ist Prozessbesitz der App.** Queue und Pipeline leben
   im App-Prozess; die DB hält nur die Session-Projektion. Extern steuerbar
   ist Playback heute nur via MPRIS (Linux) — genau darauf setzt das CLI
   auf (Beschluss 3).
7. **Genau eine Scan-Wurzel.** `settings.get_library_root` liefert einen
   einzelnen Pfad; der Watcher armiert darauf. Ein „dedizierter Ordner“ für
   Instrumentals ist ohne Multi-Root-Umbau nur **innerhalb** dieser Wurzel
   automatisch scanbar (prägt Beschluss 13).
8. **Smart-Playlists sind Regel-Abfragen, keine Drop-Ziele.**
   `smart_playlists` speichert eine validierte Feld-Whitelist, per `AND`
   verknüpft (`queries/smart.rs`); Mitgliedschaft ist Query-Ergebnis.
   „Songs hineinziehen“ gibt es dort konzeptionell nicht — die gewünschte
   Konvertierungs-Playlist braucht daher einen eigenen Playlist-Typ (3.2).

## 2. Zielarchitektur

### 2.1 Prozess-/Nebenläufigkeitsmodell — die zentrale Entscheidung

**Beschluss 1: (i) Jede Oberfläche bettet `reprise-core` als Bibliothek
ein und arbeitet auf derselben SQLite-Datei (WAL), plus dünner
Benachrichtigungsschicht (2.2). Kein Daemon. MPRIS bleibt die
Playback-IPC.**

```text
reprise (GTK, GPL)      reprise-cli (MIT)      reprise-mcp (stdio, MIT)
   | eigene Conn(s)         | eigene Conn           | eigene Conn
   +------------+-----------+-----------+-----------+
                |                       |
        reprise-core-Fassaden   (Commands/Queries)
                |                       |
          SQLite (WAL, busy_timeout, foreign_keys)
            +-- change_log      (Outbox, gleiche Transaktion)
            +-- audio_jobs / track_provenance   (Track 2, 2.4)
                |
        core::events::Notifier (notify auf DB/WAL + data_version-Fallback)
                |
   GTK-App: external_changes-Runtime -> koaleszierter Refresh
   GTK-App: AI-Job-Worker (experimentell) -> Batch-Fortschritt
   optional: reprise-cli jobs work (Feature `worker`) -> zweiter Worker-Host
```

Warum (i):

- **Die Forcing-Functions verlangen keinen Daemon.** „MCP läuft, während die
  App läuft“ ist mit WAL trivial (n Leser + 1 Schreiber zur Zeit,
  `busy_timeout` serialisiert kurze Schreiber). „Live sichtbar“ braucht nur
  Weckruf + Re-Read, keinen geteilten Prozesszustand.
- Es ist **das bestehende Modell** — die App ist heute schon ein
  Mehr-Connection-System über genau diesen Pfad; die GTK-Verdrahtung bleibt
  unberührt (kein Migrationsrisiko).
- **Portabel und lizenzkonform:** kein D-Bus/Socket im Core; Android/iOS/
  Windows nutzen denselben Embedded-Pfad. Die beschlossene MCP-Spec
  (D16/D20: „öffnet die lokale Datenbank über den normalen Core-Pfad“)
  setzt dieses Modell bereits voraus.
- CLI funktioniert **ohne** laufende App (headless Wartung), was ein
  Daemon-Modell zur Sonderlocke machen würde.

Verworfene Alternativen (Beschluss 1, Kurzform): (ii) Core-Daemon mit
D-Bus/Socket-IPC — größter denkbarer Umbau, IPC auf dem heiß optimierten
Fenster-Query-Pfad, nicht portabel, widerspricht der MCP-Spec; kein
Feature dieses Plans braucht geteilten Prozesszustand. (iii) App-hosted
Services + Standalone-Fallback — für Daten überflüssig (dort gilt (i));
der einzige echte App-Besitz (Playback/Queue) hat mit MPRIS bereits eine
standardisierte App-hosted-Schnittstelle; ein späterer
`org.reprise.Reprise1`-Service bleibt notierter Erweiterungspunkt
(Sektion 8), wird nicht gebaut.

Konsequenzen von (i), ehrlich: Busy-Fenster während Scans (Retry + klare
Fehler); kein externer Zugriff auf In-Memory-Queue/Position außer MPRIS
(akzeptiert; MCP exponiert ohnehin keine Transport-Tools); Schema-Guard
ist Pflicht (2.3, P0).

### 2.2 Änderungspropagation — Mechanismus, Ordnung, Races

**Beschluss 5: Transaktionale Outbox (`change_log`) als Wahrheit über das
*Was*; Weckruf via `notify`-Watch auf DB-/WAL-Datei mit 250 ms Debounce +
`PRAGMA data_version`-Check; Degradation auf 2-s-Polling. Alle Zahlen
sind benannte Konstanten (`WAKE_DEBOUNCE_MS`, `POLL_FALLBACK_SECS`,
Prune-Grenzen), keine Streu-Literale.**

1. **Schreiben:** Jede mutierende Core-Fassade (Playlist create/rename/
   delete/add/remove/move, Smart-Playlist create, Settings-/Modul-Wechsel,
   Scan-Abschluss als ein Sammel-Event, Job-/Track-Lifecycle aus 2.4)
   hängt **in derselben Transaktion** eine Zeile an `change_log` an:
   `(id AUTOINCREMENT, entity, entity_id, op, writer, at)`. `writer` ist
   ein pro Prozess zufälliges 64-bit-Token (fastrand, vorhanden). Atomar ⇒
   kein Event ohne Änderung und umgekehrt; Totalordnung über `id`.
2. **Wecken:** `core::events::Notifier` (eigener Thread, eigene Connection —
   exakt das Watcher-Muster) beobachtet DB + `-wal` per `notify`, prüft
   nach 250 ms Ruhe `PRAGMA data_version` (ändert sich nur bei Commits
   *anderer* Connections, mikrosekunden-billig). Kann `notify` nicht
   armiert werden (Netz-FS, inotify-Limit), degradiert er auf reines
   Polling (2 s). Sichtbarkeitsbudget: ≤ 1 s normal, ≤ 3 s Fallback.
3. **Konsumieren (GTK):** Neue `ui/external_changes`-Runtime hält
   `last_seen_id`, liest beim Weckruf neuere Zeilen, filtert das eigene
   Writer-Token (Eigen-Refresh existiert), **koalesziert pro Entität** und
   schickt grobe Refresh-Kommandos per `async_channel` in den MainLoop —
   dasselbe Muster wie `ui/scan/scan_watcher.rs`. Sidebar über vorhandenes
   `refresh(reason)`; Views über die vorhandenen Reload-Pfade. UX der
   externen Änderungen (Beschluss 6): **still** aktualisieren — kein
   Toast, kein Indikator; Selektion/Scroll bleiben erhalten; kein
   Fokus-Diebstahl (als `[geplant]`-Regeln in Paket C).
4. **Rückrichtung (GTK → MCP/CLI): gratis.** MCP/CLI sind pro Aufruf
   stateless Leser; jede Query sieht den letzten Commit (WAL-Snapshot pro
   Statement/Transaktion). Kein Subscription-Bedarf.

Fortschrittszahlen fließen über denselben Bus-Gedanken, aber gedrosselt:
Job-Progress wird in-place in der Job-Zeile aktualisiert (≤ 2 Writes/s,
Konstante); `change_log` erhält nur Lifecycle-Übergänge. Jede Oberfläche
liest **dieselben Zahlen** aus denselben Zeilen — GTK-Balken, CLI-Ausgabe
und MCP-Status zeigen identischen Fortschritt.

Ordnung und Races: Konsumenten spielen keine Operationen nach, sie
refreshen Zustand — at-least-once + Koaleszierung ist idempotent.
Rename/Delete unter offener View ⇒ Refresh liest Ist-Zustand,
verschwundene Entität nimmt den bestehenden Empty-Pfad (eigenes
Abnahmekriterium). Laufende Wiedergabe ist nie betroffen: die Queue ist
ein Snapshot (`queue::snapshot`); externe Änderungen ändern nur Ansichten
(Test, keine neue Regel nötig). Wachstum: Prune bei `open_migrated`
(behalte 10 000 Zeilen bzw. 7 Tage, benannte Konstanten); AUTOINCREMENT
verhindert Rowid-Reuse.

Verworfen (Kurzform): SQLite-Update-/Preupdate-Hooks (nur eigene
Connection); reines File-Watching ohne Outbox (kein „was“, Storm bei
Eigen-Writes); D-Bus-Signal vom CLI/MCP zur App (Linux-only, im
zbus-freien Core nicht abbildbar, nicht-transaktional) — Letzteres
höchstens später als zusätzlicher Latenz-Optimierer im Platform-Layer
(Sektion 8).

### 2.3 Die API-Naht von `reprise-core`

**Position: Die Naht bleibt „Fassaden-Funktionen über `&Connection`“ —
Commands, Queries (gefenstert), neu: Events. Kein Command-Bus, kein
Service-Objekt, kein async-Umbau.** Die Fassaden sind bereits die per Gate
erzwungene Grenze, synchron, headless testbar und FFI-tolerant (nur Werte,
keine GTK-/Runtime-Typen). Ein Command-Bus wäre Spekulation und zwänge die
GTK-App zum Mitziehen.

Neu in Core (Track 1, Architektur):

- `events`: `record` (nur aus Fassaden), `read_since`, `prune`,
  `writer_token`, `Notifier::start(db_path, on_wake) -> Option<Handle>`
  (Fehlschlag ⇒ App bleibt nutzbar, nur ohne Live-Updates —
  Degradationsmuster des Watchers).
- Schema-Guard (Beschluss 8, fail-closed): `open_migrated` lehnt
  `user_version >` Ziel mit typisiertem
  `DbError::SchemaTooNew { found, supported }` ab. Keine
  Read-only-Degradation, kein stilles Weiterarbeiten.

Neu in Core (Track 2, Feature — isoliert und entfernbar):

- `ai_jobs`: generische Job-Tabelle + Zustandsmaschine für KI-Audio-Jobs
  (2.4). „Instrumental“ ist die erste Job-Art (`kind`-Feld), nicht der
  Name des Systems.
- `provenance`: Herkunfts-Registry für KI-erzeugte/-manipulierte Tracks;
  `source_track_id` ist **optional**, damit später auch generierte Titel
  (ohne Quelltrack, mit Prompt/Parametern als Provenienz) hineinpassen.
- Plattform-Vertrag `stem_separation` (`StemSeparationBackend`-Trait +
  Fake für Tests) nach dem Muster der bestehenden Backends.

Zieht **nicht** um (bewusst): Scan-/Analyse-/Cover-Orchestrierung bleibt in
`reprise-gnome`; CLI/MCP rufen die synchronen Fassaden direkt. Die
Extraktion der Runtimes in einen GTK-freien `core::runtime`-Layer ist der
richtige nächste Portabilitätsschritt **nach** diesem Plan, wenn ein
zweites natives Frontend sie braucht.

### 2.4 Instrumental-Fassungen — Architektur des Feature-Slices

Semantik (User-Beschluss): **explizit, dauerhaft, gekennzeichnet.**

1. **Auslösung:** (a) Kontextmenü „Create instrumental“ auf einem oder
   mehreren ausgewählten Tracks; (b) eine spezielle
   **Konvertierungs-Playlist**: Songs hineinziehen = zur Konvertierung
   einreihen; (c) CLI/MCP (3.2). Wichtig: Smart-Playlists sind in dieser
   Codebase Regel-Abfragen ohne Drop-Semantik (1.2/8) — die
   Konvertierungs-Playlist wird deshalb als **System-Playlist mit Rolle**
   modelliert (neue `role`-Spalte bzw. Systemkennung auf `playlists`),
   die Drag-and-Drop annimmt und deren Einfügungen Jobs erzeugen. Der
   User-Begriff „Smart Playlist“ wird UX-seitig bedient, technisch ist es
   bewusst keine Regel-Playlist.
2. **Jobs:** `ai_jobs(id, kind='instrumental', batch_id, source_track_id,
   params_json, params_fingerprint (Modell+Version+Parameter),
   status queued|running|done|failed|cancelled, progress_permille,
   claimed_by, lease_expires_at, cancel_requested, error_kind,
   created/started/finished_at, result_track_id)`. Ein App-gehosteter
   Worker (1 Job gleichzeitig, eigene Connection, Muster
   Analyse-Scheduler) arbeitet die Queue ab; optional zusätzlich der
   CLI-Worker `reprise-cli jobs work` (Beschluss 3, Paket H1). Lease +
   Heartbeat machen abgestürzte Worker reclaimbar und koordinieren
   mehrere Worker-Hosts (genau ein Claimer pro Job; Reclaim nur nach
   Lease-Ablauf); Cancel wirkt zwischen Chunks. Multi-Select erzeugt
   einen Batch (`batch_id`) für Aggregat-Fortschritt.
   **Duplikat-/Lösch-Semantik (Beschluss 16):** Dedup via
   `UNIQUE(kind, source_track_id, params_fingerprint)` über offene und
   erfolgreiche Jobs — erneutes Anstoßen ist ein **Skip mit Verweis auf
   das Bestehende**, kein stilles Doppel-Rendern (ein späteres `--force`
   ist denkbar, nicht v1). Wird das **Original** gelöscht, bleibt die
   Fassung als eigenständiger Track erhalten; der Quell-Verweis wird zu
   reinem Provenienz-Text. Wird das **Instrumental** gelöscht, ist das
   ein normaler Track-Delete — jederzeit neu erzeugbar.
3. **Staging vor Speichern (Beschluss 15).** Job-Abschluss erzeugt zunächst
   ein **temporäres Staging-Render** (FLAC, einmal in finaler Qualität)
   unter `~/.local/share/reprise/staging/` — App-verwalteter Speicher,
   **nicht** im dedizierten Ordner, nicht in der Library, nicht unter
   Scan-Wurzeln. Es ist in der Konvertierungs-Playlist sofort abspielbar.
   Unentschiedene Renders **bleiben erhalten, auch über Neustarts** —
   Stunden Rechenzeit verdampfen nicht; die Plattenkosten sind in der
   Konvertierungs-Playlist sichtbar, es gibt keinen stillen Reaper.
   Erst die **Speichern-Entscheidung** (pro Eintrag; plus „Alle
   speichern“) **promotet** das Render: Move in den dedizierten Ordner,
   finale Tags inkl. KI-Provenienz, Registrierung in **einer**
   Transaktion (Track-Zeile über den vorhandenen
   Scanner-Metadatenpfad, `provenance`-Zeile, `change_log`-Events) —
   **kein Re-Render**. Verwerfen (Eintrag entfernen bzw. explizite
   Discard-Aktion) löscht das Staging-Render; Unentschiedenes erscheint
   nie in der Library. Dieses Staging-Modell vereint bewusst den
   ursprünglichen „Streaming“-Instinkt (flüchtig anhören) mit dem
   beschlossenen Persistenz-Modell (bewusst behalten). Ein späterer
   voller Rescan ist idempotent (Pfad-Identität); auf einer **frischen**
   DB rekonstruiert der Scanner die Kennzeichnung best-effort aus den
   eingebetteten Tags (Quell-Verknüpfung dann textuell, Beschlüsse
   13/14).
4. **Ablageort promoteter Fassungen (Beschluss 13):**
   `<library_root>/Reprise Instrumentals/<Artist>/<Titel> (Instrumental).flac`,
   **konfigurierbar**. Innerhalb der Library-Root, weil es heute genau
   eine Scan-Wurzel gibt (1.2/7) — so greifen Watcher, Android-Sync und
   alle Views ohne Multi-Root-Umbau. Das ist ein **explizit vom User
   beauftragtes** Schreiben in einen klar benannten eigenen Unterordner;
   der Grundsatz „nie ungefragt in die kuratierte Bibliothek schreiben“
   bleibt sonst unangetastet. Ein **Pfad-Guard mit Test** stellt sicher,
   dass Promotion ausschließlich unterhalb des konfigurierten
   Unterordners schreibt. Ein Ort außerhalb der Root erforderte
   Multi-Root-Support (benannter Aufpreis, nicht v1).
5. **KI-Provenienz, zweifach offengelegt (Beschlüsse 13/14):**
   - **UI:** Badge/Referenz am Track („Instrumental · KI-manipuliert“,
     Wortlaut/Platzierung per UX-Regeln), Verweis auf den Quelltitel,
     sofern verknüpft. Quell-Link: **DB primär** (`provenance`),
     Tag-Referenz sekundär.
   - **Datei-Tags** (Konvention, dokumentiert — kein erfundener
     „Standard“): Vorbis/FLAC/Opus-Felder `REPRISE_AI=vocals-removed`,
     `REPRISE_AI_MODEL=<name>@<version>`,
     `REPRISE_AI_SOURCE=<Artist> — <Titel>` (+ optional
     `REPRISE_AI_SOURCE_MBID`), zusätzlich menschenlesbar im
     Kommentarfeld „AI-manipulated: vocals removed (Reprise)“; ID3v2
     äquivalent als COMM + `TXXX:REPRISE_AI*`, MP4 als
     `----:com.reprise:AI*`. lofty (vorhandener Tag-Schreibpfad) kann
     alle drei. Die Quellreferenz ist **textuell + optional
     MusicBrainz-ID — niemals App-interne IDs in Tags** (sie überleben
     keine DB-Neuanlage). Die Offenlegung überlebt damit auch außerhalb
     von Reprise und trägt die Rescan-Rekonstruktion.
   - **Benennung (Beschluss 14):** Titel-Tag erhält das Suffix
     „(Instrumental)“; das **Album-Tag bleibt unverändert** — die
     Album-Ansicht zeigt beide Fassungen nebeneinander, Badge + Suffix
     disambiguieren (bewusst akzeptiert; keine fragmentierte
     Album-Liste).
6. **Wiedergabe-Regel (beschlossen): Warten mit Ladebalken.** Der Player
   spielt **ausschließlich fertige Dateien**. Klickt der User einen noch
   verarbeitenden Eintrag an, blockiert der Start mit sichtbarem
   Render-Fortschritt und beginnt nach Abschluss (kein Original-Fallback,
   kein Auto-Skip). Auf Hardware unter ~1× Echtzeit kann das bei einem
   4-Minuten-Track Minuten dauern — bewusst akzeptiert. Progressiver
   Frühstart („losspielen, sobald der Render dem Playhead sicher voraus
   ist“) ist eine notierte spätere Optimierung (Sektion 8), wird nicht
   entworfen.
7. **Konvertierungs-Playlist = Staging-Bereich (Beschlüsse 15, 18):** Die
   Ansicht zeigt einen **Aggregat-Fortschrittsbalken** (fertig/gesamt +
   Prozent, gespeist aus den Job-Events) und je Zeile den Zustand
   (queued/processing/done — ungespeichert/saved/failed). **Weitere
   Fortschritts-UI gibt es nicht**: kein Sidebar-/Statusleisten-Slot
   (der android-sync-V2-Bottom-Slot wird nicht angefasst), kein Toast.
   **Fertige Titel sind sofort spielbar** (aus dem Staging), während
   andere noch verarbeiten; „Playlist abspielen“ spielt die fertigen,
   ein Klick auf einen verarbeitenden Eintrag folgt der Warte-Regel aus
   Punkt 6. Pro Zeile: Speichern / Verwerfen; Kopfzeile: „Alle
   speichern“. Nach dem Speichern **wechselt die Zeile auf den
   promoteten Bibliothekstitel und bleibt**, bis der User aufräumt —
   „alle fertigen sind darin spielbar“. „Playlist leeren“ **warnt**,
   wenn unentschiedene Einträge existieren. Drag eines bereits
   konvertierten Tracks erzeugt einen **Hinweis statt Doppel-Job**
   (Dedup aus Punkt 2). Da Staging-Renders keine Bibliothekstitel sind,
   ist die Ansicht technisch eine Spezial-View über `ai_jobs` +
   Staging-Store (Wiedergabe per Dateipfad), auch wenn sie sich als
   Playlist anfühlt.
8. **Filter „KI-Musik ausblenden“ (Beschluss 17):** Ein Bibliotheksfilter
   blendet KI-manipulierte (und künftig KI-generierte) Titel aus.
   **Default: KI-Titel sichtbar, Filter opt-in** — die Fassungen sind
   gewollte Bibliotheksbürger. Der Filterzustand ist **sticky über
   Sessions** wie andere View-Zustände. Er schlüsselt auf das
   **Provenance-Flag in der DB** (Zeile in `track_provenance`), nie auf
   Ordnerpfade — der Ordner ist Ablage-Layout, das Flag die Wahrheit
   (Dateien können wandern; Tags tragen die Provenienz über Rescans).
   Er fügt sich in das **bestehende Filtersystem** ein
   (`docs/ux-rules.md` Sektion K: sichtbare Einschränkung in der
   Filter-Zeile nach FIL-1a, gezählter Zustand nach FIL-2 — gegrillte
   Beschlüsse, kein Parallelmechanismus) und wird als Query-Klausel im
   Core umgesetzt. **Keine Shuffle-/Auto-Queue-Sonderregel in v1**: das
   beschlossene Queue-Nachfüllen am Queue-Ende füllt aus der
   **sichtbaren Ansicht** nach — bei aktivem Filter sind KI-Titel nicht
   sichtbar und werden folglich auch nicht nachgefüllt. Eine
   Langform-Ausschlussregel (Meditations-Drone im Party-Shuffle)
   entsteht erst, falls Generierung real wird — dann als neue
   `[geplant]`-Regel, nicht implizit.
9. **Experimentell + schlank paketiert (Beschluss 11):** Sichtbar nur
   hinter einem „Experimental features“-Schalter in den Settings; raue
   Kanten sind akzeptiert. ML-Runtime-Gewichte werden **nicht** ins
   Default-Build/Flatpak gebündelt: **First-Use-Download** beim ersten
   Aktivieren, mit Checksum und Lizenznotiz (Muster
   Cover-Download-Modul); Bündeln ist verworfen (Flathub-Größe,
   Lizenz-Exposition), ein Flatpak-„Modell-Add-on“-Paket allenfalls
   später (Sektion 8). Modell-Lizenz-Gate: LICENSING.md verlangt für den
   MIT-Engine-Pfad Redistribution/kommerzielle Nutzung — Gewichte-Lizenz
   wird im Spike (Paket E) verifiziert; fällt sie durch, ist das Feature
   blockiert, nicht „irgendwie“ geliefert.
10. **Generische Pipeline, erste Job-Art.** Schema, Crate und API vermeiden
   instrumental-spezifische Namen, wo es nichts kostet (`ai_jobs.kind`,
   `provenance.kind`, optionaler Quelltrack). 1:1 generalisierbar:
   Job-Scheduler + Progress-Events, Provenienz-Tag-Schema,
   Staging-plus-Promotion, Ordner-und-Bibliotheksbürger-Muster,
   KI-Ausblende-Filter (Provenance-Flag deckt Manipuliertes wie
   Generiertes), Experimental-Gating, On-Demand-Runtime/Modell-Download.
   Gespeichert wird in v1 ausschließlich die Instrumental-Spur
   (Beschluss 19) — Modelle rechnen intern mehr Stems, abgelegt wird
   einer. Aufgeschobene Job-Arten: Sektion 8.

### 2.5 Neue Workspace-Mitglieder

| Crate | Binary | Zweck | Abhängigkeiten (erlaubt) | Lizenz (beschlossen) |
|---|---|---|---|---|
| `crates/reprise-cli` | `reprise-cli` | Headless-Oberfläche: Playlists (inkl. rename/delete), Suche, Summary, Scan, Instrumental-Jobs, Job-Status; Features: `mpris` (Linux-only, zbus direkt), `worker` (zieht `reprise-stems`) | `reprise-core`, `clap` v4 (derive), `serde_json`; hinter Features: `zbus`, `reprise-stems` | MIT |
| `crates/reprise-mcp` | `reprise-mcp` | Lokaler MCP-Server, stdio | `reprise-core`, offizieller Rust-SDK (`rmcp`, gepinnt), `serde`/`serde_json`, `tokio` (nur hier, vom SDK erzwungen) | MIT |
| `crates/reprise-stems` | — (lib) | `StemSeparationBackend`-Implementierung (ML-Inferenz; Runtime laut Spike: candle **oder** ort; libtorch und Python-Subprozess verworfen) | `reprise-core` + ML-Runtime | MIT (Runtime-/Modell-Lizenzen im Gate geprüft) |

Regeln (in `scripts/check-architecture.sh` zu verankern, Paket I):

- `reprise-cli`/`reprise-mcp` referenzieren aus dem Workspace **nur**
  `reprise-core`. Beschlossene, exakt umrissene Ausnahmen im CLI
  (Beschluss 3): `zbus` direkt hinter dem Linux-only-Feature `mpris`
  (weiterhin ohne platform-linux) und `reprise-stems` ausschließlich
  hinter dem Feature `worker`. Das **Default-Build des CLI bleibt
  nur-Core** — per `cargo tree`-Probe erzwungen.
- `reprise-stems` referenziert nur `reprise-core`; niemand außer den
  Binary-Hosts (App; CLI nur hinter `worker`) referenziert
  `reprise-stems`. Das Feature bleibt dadurch entfernbar, ohne die
  Core-Naht anzufassen.
- Kein SQL außerhalb von Core (bestehendes Gate ausgeweitet);
  MCP-Leak-Matrix aus Spec D19 gilt wörtlich (nie Pfade, XDG, Lyrics,
  Seriennummern, Credentials, rohe Hörereignisse).
- `default-members` bleibt `reprise-gnome`; `cargo test --workspace` deckt
  neue Crates automatisch.

MCP-Festlegungen (aus Spec D16/D18 übernommen, nicht neu erfunden):
stdio-only, stderr-Logging, stdout protokollrein, SDK gepinnt +
JSON-RPC-Fixtures als Drift-Schutz. Capabilities (Beschluss 7):
`library:read`, `playlist:create`, `ai:create` — **fail-closed off** als
Settings-Keys (`agent.capability.*`), pro Write-Aufruf frisch gelesen
(Entzug wirkt sofort, neue Freigaben nach Serverneustart —
Spec-Semantik). Verwaltung: bis auf Weiteres ausschließlich die
Settings-Keys; eine eigene Preferences-Unterseite „Agent Access“ ist ein
**benannter Folge-Task nach Paket F außerhalb dieses Plans**.

CLI-Festlegungen (Beschlüsse 3, 4): Name `reprise-cli`; `clap` v4 derive;
alles zusätzlich als `--json` (stabile Shapes); typisierte Exit-Codes;
`--db <path>` für Tests; destruktive Kommandos verlangen `--yes`
(`playlist delete`); `SchemaTooNew` ⇒ „Database schema is newer than this
reprise-cli — please update.“ (englisch nach AGENTS.md-Regel)

### 2.6 Portabilitätspfad (KDE/Qt, Windows, Android, iOS) — nur Fundament

Verankert wird ausschließlich:

1. **Der Beweis** durch CLI + MCP als zweite/dritte echte Oberfläche über
   dieselbe Naht.
2. **Gehaltene Eigenschaft statt Hoffnung:** Core-Dependencies sind bereits
   mobile-/windows-gängig (rusqlite bundled, notify, ureq/rustls, lofty,
   image). Beschlossen (12): CI-Check `cargo check` für
   `x86_64-pc-windows-msvc` und `aarch64-linux-android` — jetzt, in
   Paket I (billigster Zeitpunkt).
3. **Dokumentierte Richtung:** KDE/Qt und Windows linken Core direkt
   (cxx-qt o. ä.); Android/iOS später über einen UniFFI-Crate
   (`reprise-ffi`) mit handverlesener API-Teilmenge; je OS ein
   `reprise-platform-<os>` für die Core-Verträge. `reprise-stems` ist
   bewusst plattformneutral angelegt, damit die Job-Pipeline portabel
   bleibt.

**Out of scope:** jeder tatsächliche Frontend-Code, der UniFFI-Crate,
async-Vereinheitlichung, Daemon/IPC-Protokoll, Mobile-Packaging.

## 3. Feature-Slices end-to-end

### 3.1 Playlist erstellen (und pflegen)

CLI: `reprise-cli playlist create "Name" [--tracks 1,2,3] [--json]` →
`open_migrated` → `playlists::create(_with_tracks)` (Playlist **und**
`change_log`-Zeile in einer Transaktion) → Exit 0 mit ID. Dazu
(Beschluss 3): `playlist rename <id> "Neu"` und `playlist delete <id>
--yes` (ohne `--yes`: Fehlermeldung, keine Wirkung, Exit ≠ 0). Läuft die
App: Notifier weckt `external_changes` → Sidebar/Views refreshen ≤ 1 s,
still (Beschluss 6). Läuft sie nicht: der nächste Start liest ohnehin
frisch.

MCP: Tool `music_create_playlist` (Name + explizite Track-IDs; Limits wie
Spec: ≤ 500 IDs, PRESENT-Semantik; Antwort ohne Pfade), Capability
`playlist:create`, fail-closed off. **Beschlossene Erweiterung von Spec
D17** (Beschluss 2), die Writes bisher auf „Playlist aus freigegebenem
Mix-Draft“ beschränkte: direkte Erzeugung jetzt; der Draft-Weg koexistiert
später unter derselben Capability. Der Supersessions-Vermerk im
Spec-Dokument ist ein benannter Task in Paket I.
**Überschreiben/Löschen via Agent bleibt ausgeschlossen** — rename/delete
gibt es nur im CLI (menschlich bedient), nie im MCP.

Read-Surface v1 (beide, strikt über bestehende Queries): Library-Summary,
paginierte Track-Suche, Playlist-Liste/-Inhalt. MCP zusätzlich als
Resources `reprise://library/summary`, `reprise://playlists` (aus D17
vorgezogen).

### 3.2 Instrumental-Fassungen (experimentell)

- GTK: Kontextmenü-Aktion (Mehrfachauswahl → Batch), Konvertierungs-
  Playlist als Staging-Bereich (Aggregatbalken, Zeilenzustände,
  Speichern/Verwerfen pro Zeile, „Alle speichern“, Zeilenwechsel nach
  Speichern, Warnung bei „Playlist leeren“ mit Unentschiedenen), Badge +
  Quellverweis am promoteten Track, KI-Ausblende-Filter in der
  Filter-Zeile (opt-in, sticky), Warte-mit-Ladebalken beim Klick auf
  Verarbeitendes, Experimental-Schalter + Modell-Download-Flow in den
  Settings. Alle UX-Regeln zuerst `[geplant]`.
- CLI: `reprise-cli instrumental create <track-id…> [--stage] [--wait]`
  (Default **speichert** das Ergebnis direkt — Automation will das
  Endergebnis; `--stage` erzwingt die Staging-Entscheidung, Beschluss
  15), `reprise-cli instrumental save|discard <job-id…>`,
  `reprise-cli jobs status [--batch <id>] [--json]`, sowie hinter dem
  Cargo-Feature `worker`: `reprise-cli jobs work` — arbeitet die Queue
  ohne laufende App ab (Beschluss 3; Lease-Koordination mit dem
  App-Worker, 2.4/2).
- MCP: Tools `music_create_instrumental` (Capability `ai:create`;
  registriert Jobs, kehrt sofort mit Job-/Batch-IDs zurück; Parameter
  `save`, Default `true`, `save=false` staged) und
  `music_get_job_status` (read-only). Die laufende App zeigt neue Jobs,
  Fortschritt und schließlich den neuen Track **live** (change_log →
  external_changes) — der Showcase des Modells. Antworten folgen der
  D19-Leak-Matrix (keine Pfade).
- Läuft weder App noch CLI-Worker, bleiben Jobs `queued`; die
  MCP-/CLI-Antwort sagt das ehrlich und nennt beide Abarbeitungswege.

### 3.3 Steuer-/Lese-Umfang darüber hinaus (Beschluss 3)

- **Playback-Transport:** `reprise-cli playback play-pause|next|previous|
  status` als dünner MPRIS-Client hinter dem Linux-only-Feature `mpris`
  (zbus direkt im CLI, ohne platform-linux — beschlossene Gate-Ausnahme).
  Kein neues Protokoll; funktioniert nur bei laufender App (klare
  Meldung sonst).
- **Scan-Trigger:** `reprise-cli scan` ruft `scanner::scan_folder` (Core)
  und ist ein guter Live-Propagations-Showcase; läuft die App, scannt ihr
  Watcher ohnehin (Hinweis-Ausgabe, kein Doppel-Schaden — WAL + Retry).
- Nicht in v1 (beide Oberflächen): Tag-Writes von außen, Track-Delete/
  Trash, Queue-Mutation, beliebige Settings-Writes. Playlist-delete/
  rename ist die einzige destruktive CLI-Fläche (mit `--yes`); im MCP
  gibt es keinerlei Lösch-/Überschreib-Tools (Beschluss 2).

## 4. Migrationsplan — zwei Tracks, parallele Arbeitspakete

Harte Regeln: Datei-Ownership ist exklusiv — **kein zeitgleich laufendes
Paket berührt Dateien eines anderen**. Root-`Cargo.toml`, `db.rs` und
`core/src/lib.rs` gehören in P0 einem Agenten; danach ist Track 2-D der
einzige `db.rs`-Besitzer (Track-1-Pakete brauchen ihn nie).
`docs/ux-rules.md`-Edits serialisieren: erst C (Track 1), dann F (Track 2).
In der H-Welle gehört `crates/reprise-cli/**` exklusiv H1 und
`crates/reprise-mcp/**` exklusiv H2. Jedes Paket: TDD, alle
AGENTS.md-Gates, ein Commit pro Task, Ledger-Zeile, Rebase vor Merge.
Schemaversionen sind als „nächste freie Version“ zu verstehen (Basis
heute: v18; P0 vergibt die nächste, D die übernächste — beim Paket-Start
gegen `db.rs` verifizieren, parallele Branches können Nummern belegen).

**Track 1 (Architektur + Playlist/MCP/CLI) ist das Liefergut. Track 2
(experimentelle Instrumentals) hängt an P0, läuft parallel und darf
rutschen, ohne den Task zu gefährden.**

Wellenbild: P0 → {A, B, C parallel} → I → P3a (Track 1); nach P0
zusätzlich {D, E parallel} → {F nach D+C; G nach E; H1 nach D+A; H2 nach
D+B — parallel, disjunkte Ownership} → P3b (Track 2).

### P0 — Fundament (1 Agent, sequenziell; kritischer Pfad)

Ownership: `crates/reprise-core/src/db.rs`, `lib.rs`, neues Modul
`events/`, Instrumentierungs-Edits in `library/playlists.rs`,
`library/playlist_delete.rs`, `library/settings.rs`, `modules.rs`,
`library/scanner.rs` (nur Event-Append), Root-`Cargo.toml` + leere Stubs
`crates/reprise-cli|reprise-mcp|reprise-stems`.

- T0.1 Nächste freie Schemaversion (aktuell v19): `change_log` (+ Index)
  + `SchemaTooNew`-Guard (Beschluss 8, fail-closed). Tests:
  frisch/Upgrade identisch, Guard rot bei hochgesetztem `user_version`,
  Bestandsmigrationen grün.
- T0.2 `events`: record/read_since/prune/writer_token. Tests: Atomik
  (Rollback ⇒ kein Event), Ordnung, Prune, Writer-Filter.
- T0.3 Fassaden-Instrumentierung (append-only, keine Signaturänderung).
  Tests: je Fassade genau ein korrektes Event; Scan = ein Sammel-Event.
- T0.4 `Notifier` (notify + 250 ms Debounce + 2-s-Fallback, benannte
  Konstanten — Beschluss 5). Tests headless: Commit über zweite
  Connection weckt; Degradation ⇒ `None` statt Panik.
- T0.5 Crate-Stubs + Workspace-Membership.

Done: alle Gates, Core-Purity unverändert leer; erwartete neue Tests ≥ 25.

### Track 1 — P1: drei parallele Pakete (nach P0-Merge)

**A — CLI v1 (maximal geschnitten, Beschluss 3).** Ownership:
`crates/reprise-cli/**`. Subcommands: `playlist list|show|create|rename|
delete` (delete verlangt `--yes`; ohne: klare Meldung, keine Wirkung,
Exit ≠ 0), `search`, `library summary`, `scan` (Hinweis-Ausgabe, wenn
mutmaßlich die App läuft — ihr Watcher scannt ohnehin), `events tail`
(Debug), global `--json`, `--db`. Tests: Unit + Integration via
`CARGO_BIN_EXE_reprise-cli` gegen Temp-DB (Event-Zeile je Mutation,
Exit-Codes, `SchemaTooNew`-Meldung, `--yes`-Verweigerung, Busy-Retry
gegen gehaltene fremde Schreibtransaktion, Scan-Roundtrip gegen
Temp-Ordner). Done: Gates; `cargo tree`-Beweis: Default-Features
nur-Core; ≥ 28 Tests.

**B — MCP v1.** Ownership: `crates/reprise-mcp/**`. Vorgezogenes M1 +
Teilmenge: stdio-Server, Resources (summary, playlists), Tools
`music_search_tracks`, `music_create_playlist` (Capability
`playlist:create`, fail-closed), Pagination/Limits, stderr-Logging,
stdout-Reinheit. Tests: JSON-RPC-Fixtures über gespawnten Prozess
(Handshake, list/read, Discovery, D19-Leak-Negativmatrix, Capability off
⇒ verweigert, Entzug wirkt pro Aufruf, Busy ohne Hänger). Done: Gates;
Dependency-Grenze bewiesen; ≥ 25 Tests.

**C — GTK-Live-Refresh.** Ownership:
`crates/reprise-gnome/src/ui/external_changes/**` (neu) + exakt
`ui/window/window_runtime_wiring.rs` (existiert weiterhin; gegen
`797afa2dfa` verifiziert); dazu `docs/ux-rules.md` (nur neue Sektion,
append-only). Kontext nach PR #29: Browser-Navigation ist zentralisiert
(`ui/window/window_navigation.rs`, `ui/window/library_shell.rs`,
`ui/browse/**`), die Library-Modi sind durch eine scoped Track-List
ersetzt; die heutigen Refresh-Pfade sind `sidebar.refresh(reason)`
(`ui/sidebar/sidebar.rs`) und die Track-List-Reload-Pfade
(`ui/track_list/track_list_reload.rs`). **Diese Ownership-Liste ist beim
Paket-Start gegen den dann aktuellen Stand zu verifizieren und danach
exklusiv festzuzurren.** Zuerst `[geplant]`-Regeln (Beschluss 6): extern
erzeugte Inhalte erscheinen ohne Neustart und **still** (kein Toast, kein
Indikator); Selektion/Scroll bleiben erhalten; kein Fokus-Diebstahl durch
Hintergrund-Refresh; laufende Wiedergabe/Queue unberührt. Dann Runtime:
Notifier-Konsum, Koaleszierung (pure, unit-testbar), Kanal → MainLoop
(Muster `ui/scan/scan_watcher.rs`), Refresh über bestehende Pfade;
RefCell-Disziplin. Tests: Koaleszier-/Filterlogik headless; genau ein
isolierter Displaytest (Playlist per Zweit-Connection ⇒ Sidebar zeigt
sie), nur via `scripts/check-display-tests.sh`. Done: Gates +
UX-Traceability; ≥ 12 Tests.

### Track 1 — P2/P3: Abschluss

**I — Gates, Doku, Lizenz, Supersessions** (nach P1). Ownership:
`scripts/check-architecture.sh`, `LICENSING.md`, `README.md`/
`README.de.md` (Crate-Tabelle), `TESTING.md` (Cross-Process-Abschnitt),
`CONTEXT.md` (Begriffe „Instrumental-Fassung“, „KI-Provenienz“),
`docs/plans/audio-character-mcp.md` und
`docs/superpowers/specs/2026-07-19-audio-character-mcp-design.md` (nur
die zwei Vermerke unten). Inhalt:

- Dependency-Regeln mechanisch (inkl. roter Negativprobe; CLI-Default
  nur-Core, `mpris`-/`worker`-Ausnahmen exakt).
- Cross-Target-Check (Beschluss 12): `cargo check` für
  `x86_64-pc-windows-msvc` und `aarch64-linux-android` in CI.
- Lizenz-Zeilen CLI/MCP/Stems = MIT (Beschluss 9) + Modell-Gate-Absatz.
- **Benannter Task (Beschlüsse 2, 10):** In
  `docs/plans/audio-character-mcp.md` ausschließlich den M1-Absatz als
  „superseded by multi-frontend-core“ markieren (M2–M5/1B unberührt);
  im Spec-Dokument die dokumentierte Supersession von D17 vermerken
  (direktes `music_create_playlist` jetzt; Draft-Weg koexistiert später
  unter derselben Capability `playlist:create`).

**P3a — Architektur-Abnahme** (seriell): Zwei-Prozess-Smokes auf dem Host
(headless-Rezept aus AGENTS.md wörtlich: CLI erstellt Playlist, Xvfb-App
zeigt sie live), volle Merge-Readiness-Batterie gegen `origin/dev`,
isolierte Displaytests, adversariale Review; README-Roadmap-Zeile erst
jetzt. Sandbox-verweigerte Sockets exakt als `deferred host check`
dokumentieren.

Folge-Task (außerhalb dieses Plans, nach Paket F): Preferences-Unterseite
„Agent Access“ für die Capability-Verwaltung (Beschluss 7); bis dahin
wirken die Settings-Keys.

### Track 2 — experimentelle Instrumentals (nach P0; darf rutschen)

**D — Feature-Core.** Ownership: `crates/reprise-core/src/db.rs`
(nächste freie Schemaversion nach P0, aktuell v20: `ai_jobs`,
`track_provenance`, `playlists.role`), neue Module `ai_jobs.rs`,
`provenance.rs`, `stem_separation.rs` (Vertrag + Fake),
`library/playlists.rs`-Erweiterung um die Rollen-Playlist sowie eine
Query-Klausel für den KI-Ausblende-Filter (`queries/clauses.rs`) (nach
P1-Merge — D ist alleiniger Core-Besitzer in Track 2). Inhalt:
Job-Zustandsmaschine (Lease/Heartbeat/Reclaim mit injizierter Uhr,
Cancel, Batch-Aggregate, Dedup-UNIQUE mit Skip-und-Verweis-Semantik,
Beschluss 16), Staging-Store (deterministische Pfade unter dem Data-Dir,
Discard, Restart-Erhalt — Beschluss 15), Promotion-Fassade „Staging ⇒
Move + finale Tags + Track + Provenance + Events atomar, kein Re-Render“
inkl. **Pfad-Guard** (schreibt nur unterhalb des konfigurierten
Instrumental-Ordners, Beschluss 13), Provenance-Registry (source
optional; Original-Delete ⇒ Verweis wird Provenienz-Text),
Konvertierungs-Playlist-Semantik, Tag-Schema-Schreiber/-Leser (lofty;
textuelle Quellreferenz + optionale MBID, nie App-IDs — Beschlüsse
13/14) + Rescan-Rekonstruktion best-effort,
Provenance-Flag-Filterklausel. Done: Gates; ≥ 40 Tests.

**E — ML-Spike (zeitboxt, entscheidungspflichtig).** Ownership:
`crates/reprise-stems/**` (Spike-Code) +
`docs/research/stem-separation-runtime.md`. Misst auf dem Zielrechner:
Echtzeitfaktor, Peak-RSS, Modellgröße, Kaltstart für (a) candle +
Demucs-Klasse, (b) ort/ONNX + MDX-Klasse; prüft Gewichte-Lizenzen gegen
LICENSING.md und Flatpak-Offline-Build-Machbarkeit. libtorch und
Python-Subprozess sind verworfen (Beschluss 11) und werden nicht
vermessen. Done-Kriterium ist der **Report mit Empfehlung**, nicht
Produktionscode. Gated G; beantwortet die letzte offene Runtime-Frage
faktenbasiert. (Parallel zu D möglich — disjunkte Dateien.)

**F — GTK Instrumental-UX** (nach D und Track-1-C; einziges
gnome-berührendes Paket seiner Welle). Ownership:
`crates/reprise-gnome/src/ui/instrumental/**` (neu: Worker-Host,
Staging-/Konvertierungs-View mit Speichern/Verwerfen, Badges,
Warte-Zustand) + exakt benannte Wiring-Dateien, Stand `797afa2dfa`
(2026-07-21) verifiziert:

- `ui/window/window_runtime_wiring.rs` (Worker-/Runtime-Start),
- Kontextmenü: `ui/track_list/track_menu.rs` (+
  `ui/strings_track_menu.rs`),
- KI-Filter: `ui/browse/browse_bar.rs`, `ui/browse/browse_filter_count.rs`,
  `ui/browse/filter_restriction.rs` (FIL-1a/FIL-2-Mechanik), ggf.
  `ui/track_list/track_list_filter_actions.rs`,
- Experimental-Schalter + Modell-Download: `ui/preferences/**`
  (Modul-Muster `preference_plugins.rs`, Registrierung
  `preferences_window.rs`),

— **diese Liste ist beim Paket-Start gegen den dann aktuellen Stand zu
verifizieren und danach exklusiv festzuzurren** — + `docs/ux-rules.md`
(neue Sektion inkl. Sektion-K-Ergänzung für den Filter; die Beschlüsse
15/17/18 werden hier zu Regeln: Zeilenwechsel nach Speichern, Warnung
bei „Playlist leeren“, Hinweis statt Doppel-Job, Filter opt-in + sticky,
nur Aggregatbalken + Zeilenzustände — kein Toast, kein Sidebar-Slot).
Fortschritt ausschließlich aus den Job-Zeilen/Events (dieselben Zahlen
wie CLI/MCP). Tests: headless Worker-Host mit Fake-Backend; 3 isolierte
Displaytests (Batch-Fortschritt; „fertig ⇒ sofort spielbar, verarbeitend
⇒ Warte-Regel“; Filter blendet KI-Titel aus und die Filter-Zeile zählt
nach FIL-2). Done: Gates + UX-Traceability.

**G — Stems-Backend produktiv** (nach E). Ownership:
`crates/reprise-stems/**`. Spike-Empfehlung umgesetzt: Inferenz, Chunking
mit Überlappung, Cancel zwischen Chunks, Progress-Callbacks,
deterministische Ausgabe (nur Instrumental-Spur, Beschluss 19),
Modell-Download/-Verifikation (Checksum, Lizenznotiz neben der Datei —
Beschluss 11). Tests: synthetische/lizenzgeklärte Kurz-Fixtures,
Determinismus über Chunkgrenzen, Cancel-Latenz, kein Netz außer
explizitem Download-Pfad. Done: Gates; Echtzeitfaktor-Report im
Release-Profil.

**H1 — CLI: Instrumental, Worker, Playback** (nach D + A; parallel zu
H2). Ownership: `crates/reprise-cli/**` (in dieser Welle exklusiv H1).
Inhalt: `instrumental create <track-id…> [--stage] [--wait]` (Default
speichert, Beschluss 15), `instrumental save|discard <job-id…>`,
`jobs status [--batch <id>] [--json]`; Cargo-Feature `worker` mit
`jobs work` (einziger Pfad, der `reprise-stems` ins CLI zieht;
Produktivpfad nutzt G, Tests den Fake aus D); Linux-only-Feature `mpris`
mit `playback play-pause|next|previous|status` (zbus direkt, ohne
platform-linux — beschlossene Gate-Ausnahme). Tests nach A-Muster +
Job-Roundtrip gegen Temp-DB mit Fake-Backend-Worker; Zwei-Worker-Lease
(CLI-Worker + simulierter App-Worker: genau ein Claimer pro Job, kein
Doppel-Render); Feature-Matrix (`default`/`worker`/`mpris`) baut,
`cargo tree`-Probe: Default bleibt nur-Core. Done: Gates; ≥ 18 Tests.

**H2 — MCP: Instrumental-Fläche** (nach D + B; parallel zu H1).
Ownership: `crates/reprise-mcp/**` (in dieser Welle exklusiv H2).
Inhalt: `music_create_instrumental` (Capability `ai:create`; registriert
Jobs, kehrt sofort mit Job-/Batch-IDs zurück; `save`-Default `true`,
`save=false` staged) und `music_get_job_status` (read-only); ehrliche
Antwort, wenn kein Worker läuft (Jobs bleiben `queued`; Hinweis auf App
oder `reprise-cli jobs work`). Tests nach B-Muster (Fixtures, Capability
off ⇒ verweigert, D19-Leak-Negativmatrix) + Job-Roundtrip gegen Temp-DB
mit Fake-Worker. Done: Gates; ≥ 15 Tests.

**P3b — Feature-Abnahme** (seriell): echtes Backend in der App verdrahten
(kleiner isolierter Commit), End-to-End-Smoke headless (MCP erzeugt Job,
Worker mit Fake/Echt-Backend rendert, Track erscheint live), Gates,
Review.

## 5. Teststrategie

1. **Core headless, ein Prozess** (die verlässliche Ebene): Outbox-Atomik,
   Ordnung, Prune, Guard, Job-Zustandsmaschine (Lease/Reclaim/Cancel/
   Batch, injizierte Uhr), Provenance-Roundtrips, Tag-Schema-Roundtrip,
   Registrierungs-Transaktion inkl. Rollback, Pfad-Guard des
   Instrumental-Ordners, Notifier mit zwei Connections in einem Prozess
   (weckt identisch wie fremde Prozesse).
2. **Cross-Process display-frei:** Integrationstests spawnen die echten
   Binaries (`CARGO_BIN_EXE_*`) gegen Temp-DBs — CLI-/MCP-Roundtrips,
   stdio-Fixtures, Busy unter gehaltener Transaktion, Job-Anlage
   CLI→DB→Event, Zwei-Worker-Lease-Koordination (genau ein Claimer).
   Großzügige, benannte Timeouts.
3. **GTK bewusst minimal:** wenige isolierte Displaytests (Live-Refresh,
   Batch-Fortschritt, Warte-Regel, Filterzählung), nur über
   `scripts/check-display-tests.sh` (ein Test = ein Prozess). Der
   Default-Workspace-Lauf bleibt frei von neuen Display-Abhängigkeiten;
   auf die bekannte MainContext-Flakiness wird nichts gebaut.
4. **ML ohne Realdaten:** nur generierte/lizenzgeklärte Kurz-Fixtures;
   niemals `~/.local/share/reprise/reprise.db` oder `/home/marvin/Music`;
   Temp-XDG in jedem Kommando (AGENTS.md-Rezept). Performance-Zahlen sind
   Same-Host-Evidenz im Release-Profil, keine CI-Schwellen
   (TESTING.md-Konvention).

## 6. Risiken und Nicht-Ziele

Risiken:

- **Busy-Fenster bei langem Scan** (eine große Transaktion): externe Writes
  warten > 5 s. Mitigation: Fassaden-Retry mit Jitter für CLI/MCP, klare
  Fehlertexte; Scan-Transaktions-Split bewusst NICHT hier
  (Regressionsrisiko).
- **Versionsdrift getrennter Binaries:** fail-closed via `SchemaTooNew`
  (Beschluss 8), Meldung nennt die Richtung.
- **Zwei Worker-Hosts** (App + `reprise-cli jobs work`, Beschluss 3):
  Doppel-Claim wird durch Lease/`claimed_by`/Heartbeat verhindert —
  genau ein Claimer pro Job, Reclaim nur nach Lease-Ablauf; explizit
  getestet in H1 (Zwei-Worker-Test).
- **Echtzeitfaktor der Separation** unbewiesen bis zum Spike; unter ~1×
  Echtzeit bedeutet die Warte-Regel Minuten je Track — akzeptiert, aber
  gemessen statt behauptet.
- **Modell-Lizenz/-Download:** Gewichte können das LICENSING-Gate reißen
  (⇒ Feature blockiert); Download braucht Checksum + Offline-Fehlerpfad;
  Flatpak: Modell nie im Bundle, Runtime muss offline baubar sein —
  Spike-Prüfpunkt.
- **Schreiben in die Library-Root** (Instrumental-Ordner): einziger Ort, an
  dem die App Audiodateien erzeugt. Absicherungen (Beschluss 13): nur
  nach expliziter User-Aktion, nur unterhalb des dedizierten
  Unterordners (Pfad-Guard + Test), atomares Move, idempotenter Rescan.
  Watcher-Loop (eigene Datei löst Scan aus) ist harmlos, wird aber
  getestet (Scan sieht fertige, bereits registrierte Datei ⇒ No-op).
- **notify-Grenzfälle** (Netz-FS, Watch-Limits): Degradation auf Polling
  wie beim Library-Watcher; WAL auf Netz-FS bleibt Nicht-Support.
- **MCP-SDK-Churn** (Tier 2; Revision `2026-07-28` zuletzt RC): Pinning +
  Fixtures; Revisions-Update als bewusster Einzel-Commit.
- **Flatpak-Pfaddivergenz:** sandboxed App nutzt `~/.var/app/…`, Host-CLI
  `~/.local/share/…`. v1 dokumentiert das (+ `--db`);
  Discovery-Reihenfolge ist Release-Arbeit.
- **Event-/Refresh-Storm** bei Massen-Writes: Sammel-Events (Scan = 1),
  Debounce, Koaleszierung, Progress-Drossel (≤ 2 Writes/s);
  Abnahmekriterium in C/F misst „ein Refresh pro Batch“.
- **Staging-Speicherkosten (Beschluss 15):** Unentschiedene Renders
  bleiben erhalten — auch über Neustarts — und kosten Platz im Data-Dir
  (~20–60 MB FLAC pro Track). Die Kosten sind in der
  Konvertierungs-Playlist sichtbar; Aufräumen ist die
  Speichern/Verwerfen-Entscheidung, kein stiller Reaper.
- **Doppelte Titel in Album-Ansichten** durch Instrumental-Fassungen im
  selben Album: per Beschluss 14 bewusst akzeptiert — Album-Tag bleibt
  unverändert, Badge + Titel-Suffix disambiguieren.

Nicht-Ziele:

- Kein Core-Daemon, kein eigenes IPC-Protokoll, kein HTTP-/Remote-/
  OAuth-MCP.
- Im MCP keine Playback-Transport-/Queue-/Tag-/Delete-Tools; Playback
  (via MPRIS) und playlist rename/delete sind beschlossene Flächen
  **nur im CLI** (Beschlüsse 2, 3).
- Keine „Agent Access“-Preferences-Seite in diesem Plan (benannter
  Folge-Task nach Paket F; bis dahin wirken die Settings-Keys —
  Beschluss 7).
- Kein neuer Frontend-Code für KDE/Windows/Android/iOS, kein UniFFI-Crate,
  keine Runtime-Extraktion aus `reprise-gnome` (nur dokumentierte
  Richtung).
- **Keine Genre-Remixes** (gestrichen 2026-07-21, Qualität); **kein
  DSP-Center-Cancel-„Quick-Karaoke“** (unter der Qualitätsschwelle);
  **kein globaler Instrumental-Schalter und kein rollierendes
  Render-Fenster mit Eviction** (Modell verworfen zugunsten expliziter
  Fassungen; das Staging aus 2.4 ist entscheidungsgebunden, keine
  Eviction-Maschinerie); **kein progressives Abspielen halbfertiger
  Renders** (Sektion 8); **nur Instrumental-Ausgabe, keine
  Acapella-/4-Stem-Ablage** (Beschluss 19).
- **Keine KI-Musik-Generierung** — Sektion 8, nichts dafür gebaut.
- Mix-Planer (Stufe 1B) und Klangprofil-MCP-Tools (M2–M5) werden hier
  weder implementiert noch blockiert.

## 7. Beschlüsse (gegrillt 2026-07-21)

Alle 19 offenen Fragen des Entwurfs sind entschieden. Die Nummerierung
entspricht den Fragen-Nummern des Entwurfs; die Beschlüsse sind oben in
den Fließtext eingearbeitet — diese Liste ist die kompakte Referenz.

1. **Prozessmodell:** (i) Embedded Core + WAL + Events. Kein Daemon.
   MPRIS bleibt die Playback-IPC.
2. **MCP-Write:** direktes `music_create_playlist` jetzt; der Draft-Weg
   der Spec koexistiert später unter derselben Capability
   `playlist:create`; im audio-character-Spec-Dokument als dokumentierte
   Supersession von D17 vermerkt (benannter Task in Paket I).
   Überschreiben/Löschen via Agent bleibt ausgeschlossen.
3. **CLI-Umfang v1 maximal** (bewusste User-Abweichung von der
   Plan-Empfehlung): Basis (playlist list/show/create, search, library
   summary, events tail, `--json`, `--db`) **plus** `scan` **plus**
   Playback via MPRIS (Linux-only-Feature `mpris`, zbus direkt im CLI,
   weiterhin ohne platform-linux — die vorgesehene Gate-Ausnahme gilt)
   **plus** `playlist delete/rename` (delete verlangt `--yes`) **plus**
   Standalone-Worker `reprise-cli jobs work` hinter Cargo-Feature
   `worker` — der einzige Pfad, der `reprise-stems` ins CLI zieht; das
   Basis-CLI bleibt nur-Core. Pakete A und H entsprechend größer
   geschnitten (A: scan + delete/rename; H: worker + playback — hier in
   H1/H2 geteilt, Sektion 4).
4. **CLI-Name:** `reprise-cli`.
5. **Wake-up:** notify auf DB/WAL + 250 ms Debounce + data_version-Check;
   Degradation auf 2-s-Polling. Zahlen als benannte Konstanten.
6. **Externe Änderungen:** still aktualisieren; Selektion/Scroll
   erhalten, kein Fokus-Diebstahl — als `[geplant]`-UX-Regeln in
   Paket C.
7. **Capabilities:** `library:read`, `playlist:create`, `ai:create`;
   fail-closed off; Entzug wirkt pro Aufruf sofort, neue Freigaben nach
   Server-Neustart (Spec-Semantik übernommen). Verwaltung: eigene
   Preferences-Unterseite „Agent Access“ als benannter Folge-Task nach
   Paket F — nicht Teil dieses Plans; bis dahin wirken die
   Settings-Keys.
8. **Schema-Guard:** fail-closed `SchemaTooNew` (P0).
9. **Lizenzen:** `reprise-cli`, `reprise-mcp`, `reprise-stems` alle MIT.
10. **audio-character-Plan:** M1 vorziehen; dort **nur** den M1-Absatz
    als „superseded by multi-frontend-core“ markieren (benannter Task in
    Paket I); M2–M5/1B bleiben unberührt.
11. **ML:** Spike (Paket E) entscheidet candle vs ort faktenbasiert;
    libtorch und Python-Subprozess verworfen. Gewichte:
    First-Use-Download mit Checksum + Lizenznotiz + Lizenz-Gate; Bündeln
    verworfen; Flatpak-Add-on allenfalls später.
12. **Cross-Target-Check:** jetzt in Paket I (`cargo check`
    `x86_64-pc-windows-msvc` + `aarch64-linux-android` in CI).
13. **Ordner:** `<library_root>/Reprise Instrumentals/<Artist>/<Titel>
    (Instrumental).flac`; konfigurierbar; Pfad-Guard + Test;
    Rescan-Rekonstruktion aus eingebetteten Tags; Quellreferenz textuell
    + optional MusicBrainz-ID (keine App-internen IDs in Tags).
14. **Tags:** Titel-Suffix „(Instrumental)“, Album-Tag unverändert
    (Album zeigt beide Fassungen; Badge + Suffix disambiguieren);
    Quell-Link DB primär + Tag-Referenz.
15. **Staging:** Renders bleiben bis zur User-Entscheidung erhalten,
    auch über Neustarts (Plattenkosten sichtbar in der
    Konvertierungs-Playlist, kein stiller Reaper); die Playlist-Zeile
    wechselt nach dem Speichern auf den promoteten Titel und bleibt bis
    zum Aufräumen; „Playlist leeren“ warnt bei Unentschiedenen; Drag
    eines bereits Konvertierten gibt einen Hinweis statt Doppel-Job;
    MCP/CLI-Default `save=true`, `--stage`/`save=false` verfügbar.
16. **Duplikate/Löschen:** Skip + Verweis auf Bestehendes
    (UNIQUE-Absicherung; späteres `--force` denkbar, nicht v1); Original
    gelöscht ⇒ Fassung bleibt eigenständig, Quell-Verweis wird reiner
    Provenienz-Text; Instrumental gelöscht ⇒ normaler Delete, jederzeit
    neu erzeugbar.
17. **Filter:** KI-Titel sichtbar, Filter opt-in; Filterzustand sticky
    über Sessions wie andere View-Zustände; Umsetzung nach ux-rules
    Sektion K (FIL-1a Sichtbarkeit, FIL-2 Zählung); keine
    Shuffle-/Auto-Queue-Sonderregel in v1 (Nachfüllung folgt der
    sichtbaren Ansicht); eine Langform-Ausschlussregel entsteht erst,
    falls Generierung real wird — dann als `[geplant]`-Regel.
18. **Fortschritt:** nur Aggregatbalken + Zeilenzustände in der
    Konvertierungs-Playlist; kein Sidebar-/Statusleisten-Slot
    (android-sync-V2-Bottom-Slot nicht anfassen), kein Toast.
19. **Stems:** nur Instrumental-Ausgabe.

## 8. Später / Ideen-Parkplatz — nichts davon wird gebaut, nichts verbaut

- **KI-Musik-Generierung** (Langform, z. B. zweistündige
  Meditationsmusik) als spätere Job-Art derselben Pipeline
  (`ai_jobs.kind`, optionaler Quelltrack, Prompt als Provenienz);
  realistisch external-service-gestützt.
- **Remote-Quellen + Discovery** (User-Vision 2026-07-21):
  YouTube-Audio als Wiedergabequelle für Titel, die nicht lokal
  vorliegen; ähnliche Interpreten (lokal seit PR #23 vorhanden) auf
  nicht-lokale Vorschläge ausweiten; Neuerscheinungen (die
  `new_releases`-Tabelle existiert) direkt anspielbar machen. Zwingende
  Rechtsabwägung in einem **eigenen, separat zu grillenden Plan**:
  offizieller YouTube-Embed-Player (erlaubt; sichtbarer Player, Werbung,
  kein reines Hintergrund-Audio) versus Stream-Extraktion (verletzt
  YouTube-ToS; Präzedenz Spotube; für Flathub ein bewusstes
  Verteilungsrisiko). Die Spotify-API erlaubt kein
  Drittanbieter-Playback. Die Nähte dieses Plans stehen dem nicht im Weg
  (optionale Quelle in `provenance`, ID-basierte MCP-Antworten,
  entitäts-generische Events) — mehr wird dafür nicht getan.
- **Progressiver Frühstart** halbfertiger Renders (losspielen, sobald der
  Render dem Playhead sicher voraus ist).
- **D-Bus-Ping als Latenz-Optimierer** im Platform-Layer, zusätzlich zum
  notify-Weckruf; ebenso ein späterer `org.reprise.Reprise1`-Service als
  App-hosted-Erweiterungspunkt.
- **Multi-Root-Scan-Support** (erst nötig, wenn der Instrumental-Ordner
  die Library-Root verlassen soll).
- **`--force`-Re-Render** bestehender Fassungen (Beschluss 16: nicht v1).
- **Flatpak-„Modell-Add-on“-Paket** für Gewichte (Beschluss 11:
  allenfalls später).
- **Langform-Ausschlussregel** für Shuffle/Auto-Queue, falls generierte
  Langform-Titel real werden (Beschluss 17; dann als `[geplant]`-Regel).
