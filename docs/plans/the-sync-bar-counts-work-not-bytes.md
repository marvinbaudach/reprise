---
slug: the-sync-bar-counts-work-not-bytes
worktree: /home/marvin/Projects/reprise-the-sync-bar-counts-work-not-bytes
branch: feature/the-sync-bar-counts-work-not-bytes
phase: coded
codex_session:
created: 2026-08-29
---
# Der Sync-Balken soll Arbeit zählen, nicht Bytes

Der Fortschrittsbalken des Gerätesyncs steht bei 94 %, daneben „11 of 155
files", die Restzeit sagt „1 s left" — und danach läuft der Sync noch rund vier
Minuten. Am Ende erscheint „No space left on device", obwohl das Handy 182 GiB
frei hat, und der Lauf verwirft die Bestätigung für vier bereits geschriebene
Playlists. Die Anzeigen sind nicht ungenau, sie sind falsch.

Alle vier Defekte haben dieselbe Wurzel: **zwei Schreiber auf einen
Fortschrittszustand, ohne gemeinsamen Nenner.**

## Belege

Am laufenden Sync gemessen, zwei Abfragen von `music_get_device_sync_state` im
Abstand von ~90 s:

| | Poll 1 | Poll 2 |
|---|---|---|
| `bytes_done` | 265.506.989 | 267.992.201 |
| `bytes_total` | 274.083.101 | 274.083.101 |
| `bytes_per_second` | 22.413.788 | **22.413.788** |
| `current_track` | `…Seeing Red.reprise-analysis` | `…Shinjuku Masterlord.reprise-analysis` |

Effektiv **27 kB/s**, gemeldet 22,4 MB/s — die Rate ist auf die Byte genau
eingefroren. Beide Dateien sind Analyse-Sidecars, keine Audiodateien.
Restarbeit zu diesem Zeitpunkt: 6,09 MB von 274,08 MB = **2,2 % des Balkens für
144 Sidecars und 4 Playlist-Writes**, faktisch noch ~225 s.

Nach dem Abbruch: `files_to_copy: 0`, `managed_tracks` von 754 auf 799 — die
Musik war angekommen. Aber `playlist_writes: 4` wieder offen und beide neuen
Playlists `last_synced_at: null`.

## Ist-Zustand

### Die tatsächliche Reihenfolge eines Laufs

`run_planned_sync` (`crates/reprise-gnome/src/ui/device_sync/device_sync_planned.rs:166`)
treibt die Zustandsmaschine bis `Effect::Finished`. **Danach** laufen zwei
weitere Phasen außerhalb der Maschine:

1. Maschine: Transfers → Playlist-Writes → Playlist-Removals → Removals
   (`machine.rs:422`, `:474`, `:517`, `:563`)
2. `run_analysis_phase` (`device_sync_planned.rs:176`, `:200-243`) — die
   Analyse-Sidecars
3. `write_track_metadata_list` (`device_sync_planned.rs:181`) — nur bei
   `SyncInitiator::Listener`
4. `finish_sync` (`device_sync_planned.rs:301`)

`machine.rs` enthält kein einziges Vorkommen von `analysis_writes`. Die
Maschine ist die Autorität über Phase und Fortschritt — außer für die zwei
Phasen am Ende, und genau dort bricht die Anzeige.

### Der Balken misst Bytes und ist nicht monoton

`fraction()` (`device_sync_dock.rs:308-317`) nimmt `bytes_done / bytes_total`,
sobald `bytes_total > 0`. Der Dateizähler `done/total` ist nur der Fallback für
Pläne ganz ohne Bytes. Der Balken ist also immer byte-gewichtet, der Text
daneben immer dateigewichtet, und beide teilen sich keinen Nenner.

Jede Phase reicht ihren eigenen `bytes_done`-Wert an `syncing_phase()`:

| Phase | übergibt | Wirkung |
|---|---|---|
| Transfers (`machine.rs:433`, `:457`) | `completed_bytes` | 0 → ~94 % |
| Playlist-Writes (`machine.rs:511`) | `plan.transfer_bytes` | fest **100 %** |
| Removals (`machine.rs:577`) | `0` | zurück auf **0 %** |
| Sidecars (`device_sync_planned.rs:213-219`) | `transfer_bytes − analysis_bytes` | **94 %** → 100 % |

Ein voller Lauf zeigt also `0 → 94 → 100 → 0 → 94 → 100`. Deshalb wird hier
nicht nachjustiert, sondern das Modell ersetzt.

`bytes_total` ist `plan.transfer_bytes` (`machine.rs:690`), gesetzt an genau
zwei Stellen: `mirror.rs:496` summiert `copy` + `replace`, `mirror.rs:413`
addiert die Sidecar-Bytes. **`playlist_writes` kommt in keiner vor** — die
Playlist-Dateien sind null Prozent des Balkens.

### Der Dateizähler startet mitten im Lauf neu

`run_analysis_phase` meldet `syncing_phase(SyncStep::Copying, index,
writes.len(), …)` (`device_sync_planned.rs:229-238`): `done` fängt wieder bei 0
an, `total` ist jetzt die Zahl der Sidecars, und der Schritt heißt weiterhin
`Copying`. Daher „11 of 155 files" neben 94 % — beide Zahlen stimmen für sich,
gehören aber zu verschiedenen Läufen.

### Die Rate wird in der Sidecar-Phase nicht mehr gemessen

`MtpRateMeter` (`device_sync_rate.rs`) ist ein EWMA über Byte-Deltas
(Gewichtung 3:1, `:3-4`), gefüttert ausschließlich von `mtp_rate.observe()` in
`device_sync_effects.rs:730`, dem CopyProgress-Pfad.
`copy_analysis_sidecar` (`device_sync_effects.rs:425-481`) übergibt als
Progress-Callback `Rc::new(|_, _| {})` — einen No-op. In der gesamten
Sidecar-Phase kommt kein Progress-Event an; der Meter behält seinen letzten
Audio-Wert.

`remaining()` (`device_sync_dock.rs:302-306`) rechnet
`(bytes_total − bytes_done) / bytes_per_second` → 6 MB / 22,4 MB/s →
`div_ceil` → **konstant „1 s left"**, minutenlang.

### „No space left on device" meint das falsche Device

`staging::stage_bytes` (`crates/reprise-core/src/device_sync/staging.rs:41-48`)
schreibt per `std::fs::write` nach `std::env::temp_dir()` (`:29`) — lokal. Der
`io::Error` wird an den Aufrufstellen (`device_sync_effects.rs:63`, `:449`,
`:508`) roh in „could not stage …: {error}" formatiert und landet auf der
Geräteseite neben „Free 182.3 GiB". Vorgefunden: `/tmp` ist ein tmpfs mit
16 GB, gefüllt von einem 6,6-GB-Build-Verzeichnis. Auch jede Transcode-Ausgabe
geht durch dasselbe tmpfs (`device_sync_effects.rs:125`), also durch den RAM.

Laut Doc-Kommentar (`staging.rs:36-39`) bleiben fehlgeschlagene Writes
absichtlich liegen; vorgefunden wurden 24 Reste.

### Ein Fehlschlag am Ende verwirft die Arbeit davor

Der ENOSPC traf `write_track_metadata_list`, den allerletzten Schritt. Der
macht aus dem Outcome ein `Failed` (`device_sync_planned.rs:181-186`), und
`finish_sync` wendet `verified_sources` nur bei `Completed` an (`:318-320`).
Die vier Playlist-Dateien **waren zu dem Zeitpunkt geschrieben** — sie
verloren trotzdem ihren Stempel, und der nächste Lauf plant sie komplett neu.
Genau das zeigt der gemessene Gerätezustand.

## Was gebaut werden soll

### 1. Staging: echter Cache statt tmpfs, eigener Fehlertyp

- `staging::staging_dir()` nach dem Hausmuster aus `cover.rs:196-201` und
  `artist_portrait/cache.rs:30-31`:
  `dirs::cache_dir().unwrap_or_else(std::env::temp_dir)`, Unterordner
  `reprise/device-sync`. Verzeichnis bei Bedarf anlegen. Damit läuft kein
  transcodierter Track mehr durch den RAM.
- Ein eigener Fehlertyp statt des durchgereichten `io::Error`.
  `io::ErrorKind::StorageFull` wird gesondert benannt; die Meldung nennt das
  **lokale Staging-Verzeichnis**, nie „device". Kein neues
  Konfigurationsfeld — ein sinnvoller Default löst das Problem.
- Die Aufrufstellen (`device_sync_effects.rs:63`, `:449`, `:508`) hören auf,
  den os-Text zu formatieren.
- Reste dieses Prozesses am Laufende aufräumen, statt sie laut
  `staging.rs:36-39` liegen zu lassen. Nur eigene (`reprise-sync-<pid>-*`) —
  fremde PIDs gehören einem anderen Lauf.

### 2. Ein Arbeitseinheiten-Ledger im Kern

Ein Lauf bekommt **eine** geordnete Liste von Arbeitseinheiten, aus dem Plan
gebaut: Transfers (`copy` + `replace`), `analysis_writes`, `playlist_writes`,
`playlist_removals`, `remove` und — wenn eingeschaltet — die Metadatenliste.

```
fraction = (fertige Einheiten + Byte-Anteil der laufenden Einheit) / Gesamtzahl
```

Gewicht **1 pro Einheit**. Bewusst grob: eine Audiodatei mit Transcode kostet
mehr als ein Sidecar, aber beide liegen über MTP in derselben Größenordnung —
im Gegensatz zur Byte-Gewichtung, die sie um Faktor 30 auseinanderzieht. Die
Byte-Interpolation innerhalb der laufenden Einheit hält den Balken bei großen
Dateien flüssig. Die Gewichte bestimmen nur die Linearität des Balkens, nicht
die Wahrheit der Restzeit — die kommt aus gemessenem Durchsatz.

**Falle: `PlannedSyncPhase` leitet `PartialEq, Eq` ab** (`machine.rs:36`). Ein
`f64`-Feld bricht `Eq`. Der Bruch ist ganzzahlig zu halten: die Phase trägt
`done`, `total` und die Bytes **der laufenden Einheit**; den Quotienten bildet
erst die Anzeige.

`PlannedSyncPhase::Syncing` (`machine.rs:41-49`) wird also:

```rust
Syncing {
    step: SyncStep,
    done: u32,
    total: u32,
    current_track: String,
    unit_bytes_done: u64,
    unit_bytes_total: u64,
}
```

Das Ledger führt **zusätzlich** die lauf-weiten Byte-Summen weiter
(`bytes_done`/`bytes_total` über alle Einheiten). Sie verschwinden nicht mit
dem Feldwechsel oben: `sidebar_device_card.rs:340` und die Agenten-Oberfläche
lesen sie, und als Byte-Zähler sind sie weiterhin ehrlich. Die Prozentzahl der
Sidebar (`sync_percent`, `device_sync_strings.rs:135-142`) wechselt dagegen auf
Einheiten, damit Sidebar und Dock nie zwei verschiedene Prozentzahlen für
denselben Lauf zeigen — ihre Signatur ändert sich entsprechend.

### 3. Sidecars und Metadatenliste ziehen in die Maschine

`run_analysis_phase` wird aufgelöst. `analysis_writes` und die Metadatenliste
werden reguläre Schritte: je ein `SyncStep`, ein `Effect` und ein `Event`. Die
GNOME-Schicht führt die Effekte aus, wie sie es für `CopyTrack` schon tut — sie
erfindet keine Phasen mehr. Damit verschwindet der zweite Zähler strukturell
statt durch eine korrigierte Zählung, und der angezeigte Schritt sagt endlich,
was gerade passiert.

Die Metadatenliste läuft nur bei `SyncInitiator::Listener` — ein
GNOME-Begriff. Damit die 760 Zeilen `machine_tests.rs` nicht alle angefasst
werden müssen, bleibt `DeviceSyncMachine::new` unverändert (ohne die Einheit)
und bekommt eine zusätzliche Einschaltung im Builder-Stil.

**`machine.rs` steht bei 748 Zeilen**, die Stilregel sagt 800 max. Die zwei
neuen Phasen sprengen das: die Phasenübergänge gehören in ein eigenes Modul
neben `machine.rs`, das Ledger in ein zweites. Beide brauchen ihre
`pub mod`-Zeile in `device_sync.rs:15-30` — das ist die einzige Änderung an
dieser Datei.

### 4. Restzeit aus gemessenem Einheiten-Durchsatz

`MtpRateMeter` bekommt einen zweiten, phasenunabhängigen Messwert: die Dauer je
abgeschlossener Arbeitseinheit, geglättet mit derselben 3:1-Gewichtung wie
heute die Byte-Rate (`device_sync_rate.rs:3-4`).

```
remaining = verbleibende Einheiten × Sekunden je Einheit
```

Dieser Messwert läuft über **alle** Phasen, weil jede Einheit abschließt — er
kann nicht einfrieren, wenn eine Phase keine Byte-Events liefert.

Die Byte-Rate bleibt als Anzeige, meldet aber 0, sobald sie nicht misst.
`rate_and_remaining` (`device_sync_strings.rs:236-257`) lässt sie dann bereits
heute weg; es bleibt die Restzeit allein. Kein neuer String, keine neue
Übersetzung. Zusätzlich bekommt `copy_analysis_sidecar` einen echten
Progress-Callback statt des No-ops.

**Es darf nur eine ETA-Quelle geben.** Das byte-basierte `remaining()`
(`device_sync_dock.rs:302-306`) wird ersetzt, nicht ergänzt — sonst stehen zwei
Restzeiten nebeneinander und die alte gewinnt irgendwo. Das `.max(1)` in
`rate_and_remaining` (`device_sync_strings.rs:242`) hält die Anzeige heute bei
„1 s" fest; mit einer echten Restzeit ist es harmlos, gehört aber überprüft.

### 5. Pro Einheit stempeln

Jede Playlist bekommt ihre verifizierte Zeit, sobald **ihr** Write bestätigt
ist, unabhängig davon, was danach passiert. Kein späterer Schritt kann eine
frühere Bestätigung rückwirkend kassieren — auch keiner, der erst später
ergänzt wird. Das ist die generelle Form des Fixes und passt zum
Arbeitseinheiten-Modell, das ohnehin gebaut wird.

### 6. Die Agenten-Oberfläche behält ihre Bytes und bekommt Einheiten

`progress` in `agent_device_sync.rs:28-32` (und die Spiegelungen in
`device_sync_agent.rs:196`, `mpris/device_sync_control.rs:128`) behält
`bytes_done`/`bytes_total`/`bytes_per_second` — als *Byte*-Zähler sind sie
ehrlich, und `music_get_device_sync_state` ist eine gelesene Schnittstelle.
Dazu kommen `units_done`, `units_total` und die geschätzte Restzeit. Das ist
zugleich der Messpunkt für die Abnahme unten.

## Verifikation

Ein Screenshot beweist hier nichts — die falsche Anzeige sah bisher plausibel
aus. Was zählt:

- **Monotonie als Test, nicht als Behauptung.** Ein Test über einen Plan mit
  Transfers *und* Sidecars *und* Playlist-Writes *und* Playlist-Removals *und*
  Removals, der jede emittierte `PlannedSyncPhase` einsammelt und prüft:
  die Anzeigefraktion fällt nie, erreicht genau einmal 1,0, `total` bleibt über
  den ganzen Lauf konstant, `done` springt nie zurück.
  **Der heutige Code fällt durch diesen Test** (`0 → 94 → 100 → 0 → 94 → 100`)
  — er ist damit die Kontrollprobe, nicht bloß ein neuer grüner Test.
- **Restzeit ohne Byte-Events:** Test, dass der Einheiten-Durchsatz Werte
  liefert, wenn kein einziges `CopyProgress` eintrifft — genau die Lage, in der
  heute „1 s left" einfriert.
- **Pro-Einheit-Stempel:** Test, dass ein Fehlschlag im letzten Schritt die
  verifizierten Zeiten der davor erfolgreichen Playlists stehen lässt.
- **ENOSPC ohne echtes Vollaufen:** Staging-Verzeichnis im Test auf ein
  winziges Volume zeigen lassen und prüfen, dass die Meldung das lokale
  Verzeichnis nennt und nicht „device".
- **Abnahme am echten Gerät**, mit derselben Methode wie die Diagnose: zwei
  Abfragen von `music_get_device_sync_state` im Abstand von ~90 s während der
  Sidecar-Phase. `bytes_per_second` darf sich nicht mehr auf die Byte gleichen,
  `units_done` muss wachsen, und die gemeldete Restzeit muss in derselben
  Größenordnung liegen wie die tatsächlich verbleibende. Ohne diese Messung
  gilt der Fix als unbelegt — der ursprüngliche Fehler war für Unit-Tests
  unsichtbar und zeigte sich erst live.

## Parallelität

**Nicht schneidbar — ein Strang.**

- Ein Schnitt entlang der Crates (Kern in `reprise-core`, Anzeige in
  `reprise-gnome`) ist keine Parallelität, sondern eine Reihenfolge: die
  GNOME-Schicht kann das Ledger erst benutzen, wenn die Maschine es hat.
- Ein Schnitt entlang der Defekte legt beide Stränge auf
  `crates/reprise-gnome/src/ui/device_sync/device_sync_effects.rs` — der Umbau
  wegen Progress-Callback und der neuen Effekte (`:425`, `:730`), der
  Staging-Fix wegen der Aufrufstellen (`:63`, `:449`, `:508`). Keine disjunkte
  Dateigruppe.

Der Umfang rechtfertigt den Schnitt ohnehin nicht: rund zehn Dateien in einem
Subsystem, deren Änderungen sich gegenseitig bedingen. Zwei Worktrees und ein
Merge kosten mehr, als die Wall-Clock einbringt.

**Dateien dieses Strangs:**

Korrigiert nach dem Code-Lauf. Die ursprüngliche Liste nannte 15 Dateien und
übersah die Konsumenten von `PlannedSyncPhase` und `SyncStep` außerhalb des
Subsystems — `reprise-runtime`, `reprise-view`, `reprise-runtime-protocol`,
`reprise-mcp` und die GNOME-Test-Fixtures. Codex ist an dieser Lücke gestoppt;
der Schnitt selbst war davon nicht betroffen (ein Strang).

Nicht betroffen, obwohl ein Grep sie findet: `crates/reprise-gnome/src/ui/podcasts/**`
hat ein eigenes, lokales `SyncStep` (`podcasts_sync_state.rs:5`).

```
crates/reprise-core/src/agent_device_sync.rs
crates/reprise-core/src/device_sync.rs
crates/reprise-core/src/device_sync/ledger.rs
crates/reprise-core/src/device_sync/machine.rs
crates/reprise-core/src/device_sync/machine_tests.rs
crates/reprise-core/src/device_sync/phase_transitions.rs
crates/reprise-core/src/device_sync/staging.rs
crates/reprise-core/src/device_sync/staging_tests.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_agent.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_content_transfer.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_dock.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_effects.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_fake_backend.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_feedback.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_inflight_tests.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_page_display_tests.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_page_tests.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_planned.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_rate.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_runtime.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_strings.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_types.rs
crates/reprise-gnome/src/ui/sidebar/sidebar_device_card.rs
crates/reprise-gnome/src/ui/sidebar/sidebar_device_card_mirror_tests.rs
crates/reprise-gnome/src/ui/sidebar/sidebar_device_card_status_tests.rs
crates/reprise-gnome/src/ui/sidebar/sidebar_device_card_text.rs
crates/reprise-gnome/src/ui/sidebar/sidebar_device_marking_tests.rs
crates/reprise-mcp/src/device_sync.rs
crates/reprise-mcp/tests/playback_roundtrip.rs
crates/reprise-platform-linux/src/mpris/device_sync_control.rs
crates/reprise-runtime-protocol/src/device_sync.rs
crates/reprise-runtime-protocol/src/lib.rs
crates/reprise-runtime-protocol/tests/schema.rs
crates/reprise-runtime/src/devices.rs
crates/reprise-runtime/src/runtime_tests.rs
crates/reprise-view/src/device_sync.rs
```
