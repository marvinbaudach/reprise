---
slug: architecture-consolidation
worktree: —
branch: claude/project-review-refactoring-qxv9gr
phase: review
codex_session:
created: 2026-07-31
base: 577765b (origin/dev)
---
# Projekt-Review & Konsolidierungsplan vor der Testfreigabe

Auftrag: Gesamtreview nach vielen Features, Spec- und Design-Änderungen. Ziele
des Reviews: (a) saubere Architektur, damit mehrere Apps auf demselben Kern
bauen, (b) saubere Fehlerbehandlung und Logging fürs Debugging, (c) Suche nach
Doppelentwicklung — besonders Radio, Podcast, YouTube und normale Playlisten,
(d) Klärung, ob Playlist-Filter und Interpretenseiten-Pillen sauber getrennt
sind, (e) Performance-Potenziale, (f) Abwärtskompatibilität für eine baldige
Testfreigabe.

Dieses Dokument ist Befund **und** Plan. Es ersetzt keine bestehende Regel:
`docs/ux-rules.md` bleibt der UX-Vertrag, `docs/plans/multi-frontend-core.md`
die Architekturgrundlage. Es benennt, was von diesen Plänen unvollendet blieb,
und was seither ungeplant nebeneinander gewachsen ist.

---

## 0. Kurzurteil

| Bereich | Urteil | Nächster Schritt |
| --- | --- | --- |
| Kern-Wiederverwendbarkeit | **gut** — `Db`-Handle sitzt, Purity ist mechanisch geprüft | `rusqlite::Error` aus der öffentlichen API entfernen |
| Zweite Laufzeit (`reprise-runtime`) | **kritisch** — ~14.500 Zeilen gebaut, verpackt, von keiner Oberfläche benutzt | Entscheidung erzwingen: cutover oder ausbauen |
| Fehlerbehandlung | **sehr gut** im Kern, **eine harte Lücke** beim Start | `expect` in `main.rs` durch Fehlerdialog ersetzen |
| Logging | **ausreichend zum Entwickeln, zu dünn für Tester** | Log-Datei + „Diagnose kopieren" |
| Doppelentwicklung Quellen | **real, aber begrenzt und benannt** | zugesagten Konsolidierungs-Task einlösen |
| Filter vs. Ort | **seit `c565671` sauber** | eine Wahrheit für „hat Sidebar-Zeile" (§5.3) |
| Performance | **eine messbar teure Stelle** (Default-Sortierung ohne Index) | ein Index, eine Migration |
| Sicherheit | **überdurchschnittlich** — keine Injektion, keine Traversierung, Bomben abgedeckt | `--` vor der yt-dlp-URL, Bild-`Limits` |
| Stabilität | **erarbeitet panikarm — aber eine Panik ist ein stiller Abbruch** | `panic::set_hook` + Absturzmarker |
| Abwärtskompatibilität | **Schema ja, Toolchain nein** | MSRV/Toolchain-Befund §9.3 |

**Freigabeempfehlung:** Die App ist inhaltlich testreif. Die Punkte aus §10,
Welle 0 sollten vor dem Öffnen erledigt sein. Sie sind zusammen klein (geschätzt
1–2 Arbeitstage) und fast alle sind Dinge, die ein Tester sonst als „stürzt ab"
oder „ich kann nichts berichten" zurückmeldet.

Der rote Faden durch die drei kritischen Befunde ist derselbe: **das Produkt ist
gut gebaut, aber es kann nicht über sich selbst berichten.** Ein Absturz ist
still (§8.2), ein Startfehler ist eine Panik (§3.2), und die 793
`tracing`-Aufrufe des GTK-Crates erreichen niemanden (§3.3). Genau diese drei
Punkte entscheiden, ob eine Testrunde Erkenntnisse liefert oder nur Frust.

---

## 1. Messgrundlage

Stand `577765b` (`origin/dev`, 2026-07-31). Alle Zahlen selbst gemessen, nicht
aus Docs übernommen.

| Crate | Dateien | Zeilen |
| --- | ---: | ---: |
| `reprise-gnome` | 528 | 140.678 |
| `reprise-core` | 381 | 108.043 |
| `reprise-mcp` | 41 | 12.009 |
| `reprise-platform-linux` | 38 | 11.607 |
| `reprise-runtime` | 35 | 9.337 |
| `reprise-cli` | 37 | 5.181 |
| `reprise-stems` | 11 | 2.755 |
| `reprise-runtime-client` | 5 | 1.879 |
| `reprise-runtime-protocol` | 12 | 1.773 |
| **Summe** | **1.088** | **293.262** |

- Testanteil rund **24 %** in dedizierten Testdateien; über 4.200
  `#[test]`-Funktionen.
- `docs/ux-rules.md`: rund 3.950 Zeilen, **313** `[active]`-Marker.
- Schema-Version **50**, 18 nummerierte Migrationsschritte in `db.rs` plus
  ausgelagerte Migrationsmodule; eigene Migrationstestdateien je Bereich.
- Gate-Skripte: 30 unter `scripts/`, davon 17 `check-*`.

Das ist ein reifes, stark abgesichertes Repository. Die Befunde unten sind
Konsolidierungsarbeit, keine Sanierung.

**Hinweis zur Basis:** `origin/main` steht bei `de4138a`, `origin/dev` zwei
Commits weiter bei `577765b` (`#189` Lyrics/Cover-Robustheit, `#193`
Reveal-Verhalten der Quellenlisten). Gemessen und geprüft wurde gegen `dev` —
`main` ist darin vollständig enthalten. Wo die beiden jüngsten Commits einen
Befund verändern, steht es an der jeweiligen Stelle; §1.1 fasst es zusammen.

### 1.1 Was die zwei jüngsten Commits am Befund ändern

- **`#193` bestätigt die Konsolidierungsrichtung.** Der Commit legt
  `crates/reprise-gnome/src/ui/source_reveal.rs` an — eine *geteilte*,
  GTK-freie Entscheidung „wann bewegt sich der Viewport", mit der ausdrücklichen
  Begründung, dass Podcasts, YouTube und Radio „nicht in drei Antworten
  auseinanderdriften" dürfen, während das *Wie* je Ansicht bleibt. Das ist
  exakt das Muster, das §4.2 und §4.3 für Filterleiste und Add-Dialog
  vorschlagen. Der Weg ist also nicht neu, sondern schon eingeschlagen.
- **`#189` verschärft Befund D3.** Der Lyrics-Pfad wurde in ein `lyrics/`-Modul
  zerlegt und um **zwei weitere** eigene `ureq`-Agenten ergänzt (`lrclib.rs`,
  `netease.rs`). Damit stehen im Kern jetzt **16** statt 13
  HTTP-Boundary-Konstruktionen. Die Duplikation wächst weiter, solange die
  gemeinsame Boundary fehlt.
- **`#189` liefert zugleich den besten Baustein dafür.**
  `lyrics/breaker.rs` ist ein **host-basierter** Circuit Breaker (3 Fehler →
  5 Minuten offen, `LazyLock<Breaker>` über eine Host-Map). Das ist die
  richtige Schlüsselung — pro Host, nicht pro Modul — und damit der natürliche
  Kern des in §4.4 vorgeschlagenen `SourceClient`. Die Empfehlung lautet
  deshalb: nicht neu erfinden, sondern `breaker.rs` herausheben und alle
  Quellen daran anschließen.
- **`#189` öffnet einen neuen Schreibpfad in die Musiksammlung**
  (`cover_writeback.rs`, `lyrics/sidecar_write.rs`, `writeback_publish.rs`):
  Reprise schreibt jetzt `cover.<ext>` und `.lrc` neben vorhandene Titel.
  `AGENTS.md` wurde im selben Commit um die genaue Regel ergänzt (nur aus
  Trackpfaden abgeleitet, nie eine bestehende Datei überschreiben, exakt ein
  Aufräummuster für eigene Temporärdateien). Sicherheitsseitig sauber gelöst —
  §7.1 bewertet es; für die Testfreigabe ist es der Pfad mit dem höchsten
  „fasst fremde Dateien an"-Risiko und gehört auf die manuelle QA-Liste.
- **Unverändert offen:** `AGENTS.md` sagt weiterhin „Three-crate Cargo
  workspace" (§2.5) und trägt weiterhin den Abschnitt „Not released yet — no
  backwards compatibility" (§9.1).

---

## 2. Architektur — trägt der Kern mehrere Apps?

### 2.1 Was bereits trägt

Vier Dinge sind richtig gelöst und sollten nicht angefasst werden:

1. **`Db`-Handle statt `Connection`** (ADR 002, gelandet in `#173`).
   `Db::conn()` ist `pub(crate)`; keine einzige öffentliche Kernfunktion nimmt
   noch `&Connection`. Damit ist die Grenze ein Typ und keine Konvention mehr,
   und die 575 `borrow()`-Stellen der häufigsten Panik-Klasse sind ersatzlos
   weg. Das ist die wichtigste Einzelvoraussetzung für eine zweite App — sie
   ist erfüllt.
2. **Mechanisch geprüfte Abhängigkeitsrichtung.**
   `scripts/check-architecture.sh` prüft je Crate mit `cargo tree --target all
   -e normal`, dass keine GTK/GLib/GStreamer/zbus-Familie in `reprise-cli`,
   `reprise-mcp`, `reprise-stems`, `reprise-runtime` landet, und dass kein
   Fremd-Workspace-Edge entsteht. Der Probe-Wrapper `run_dependency_probe`
   schließt **fail-closed** — ein kaputtes `cargo tree` bricht das Gate ab,
   statt still zu bestehen. Das ist besser als die meisten Projekte es machen.
3. **Kein SQL außerhalb des Kerns**, für GTK *und* die headless-Oberflächen
   getrennt geprüft (mehrzeiliges `rg -U` fängt umgebrochene Statements).
4. **`change_log`-Outbox + `Notifier`.** Prozessübergreifende Sichtbarkeit von
   Fremdänderungen ohne Daemon, mit Degradation auf 2-Sekunden-Polling. Genau
   die richtige Wahl für „mehrere Prozesse auf einer SQLite".

### 2.2 Befund A1 (kritisch) — die zweite Laufzeit ist gebaut, aber nirgends verdrahtet

`docs/plans/multi-frontend-core.md` §9.1 zieht eine Linie: alles in SQLite
bleibt eingebettet, alles *nicht* in SQLite (Audio-Pipeline, In-Memory-Queue,
Gerätelauf, Jobfortschritt) bekommt genau einen Besitzer — `reprise-runtime`.
Dieser Besitzer existiert vollständig:

| Bestandteil | Zeilen | Zustand |
| --- | ---: | --- |
| `reprise-runtime` (Reducer + Ports + Fakes) | 9.337 | fertig, getestet |
| `reprise-runtime-protocol` (Wire-Vertrag) | 1.773 | fertig, versioniert |
| `reprise-runtime-client` (Transport + Mirror) | 1.879 | fertig, getestet |
| `platform-linux/src/runtime_service/` (D-Bus, Lease) | 1.580 | fertig |
| `crates/reprise-gnome/src/ui/runtime/` (GTK-Sitzung) | 809 | **`#![allow(dead_code)]`** |
| **Summe** | **≈ 15.400** | |

Dazu ausgeliefert: `data/org.reprise.Reprise1.service.in`,
`data/reprise-runtime.service.in`, ein eigenes Meson-Target, und
`scripts/check-runtime-service-install.sh`, das die Installation beider
Artefakte in zwei Präfixen prüft.

Verdrahtet ist davon **nichts**. `crates/reprise-gnome/src/ui/runtime/mod.rs`
sagt es selbst: *„This module is not that migration — nothing here is wired
into `PlayerController` or the window yet, on purpose."* `reprise_runtime_client`
wird ausschließlich von diesem toten Modul und von Tests in
`reprise-platform-linux` benutzt. `reprise-mcp` und `reprise-cli` steuern die
Wiedergabe weiterhin über MPRIS (`org.mpris.MediaPlayer2.reprise` +
`org.reprise.Player1`), nicht über `org.reprise.Reprise1`.

Gleichzeitig lebt der produktive Zustand weiter in
`crates/reprise-gnome/src/ui/playback/` (6.916 Zeilen; `player_controller.rs`
792, `queue_transport.rs` 753, `up_next_transport.rs` 639).

**Warum das ein Architekturbefund ist und nicht nur unfertige Arbeit:**

- Es sind heute **zwei Kommandoflächen für dieselbe Fachlichkeit**. Beide
  wrappen dieselben Kerntypen (`reprise_core::queue::Queue`,
  `up_next::UpNextQueue`) — das ist gut —, aber die *Bindung* zwischen beiden
  ist zweimal geschrieben. `crates/reprise-runtime/src/transport_parity_tests.rs`
  sagt das wörtlich: *„what lived in the controller was the binding between the
  two, and that is what these tests pin."* Jede künftige Queue-Regel muss an
  zwei Stellen implementiert und über Paritätstests zusammengehalten werden.
- Es sind **zwei Steuerebenen ausgeliefert**. Ein Agent kann `reprise-runtime`
  über Bus-Aktivierung starten, während die GTK-App läuft. Dann besitzen zwei
  Prozesse eine Wiedergabe. Der Single-Owner-Lease
  (`runtime_service/lease.rs`) schützt die *Runtime* vor sich selbst, nicht vor
  der GTK-App, die den Lease gar nicht nimmt.
- Für die Testfreigabe heißt das: ein Dienst mit systemd-Unit und D-Bus-Namen
  geht mit aus, den kein Produktpfad benutzt. Das ist Angriffsfläche und
  Supportlast ohne Gegenwert.

**Empfehlung — eine von zwei, nicht beide:**

- **(A) Cutover ziehen.** `PlayerController` wird Client von `RuntimeSession`;
  `queue_transport`/`up_next_transport` werden zu Kommandos + Snapshot-Rendering.
  Ergebnis: eine Fachlichkeit, ein Besitzer, MCP/CLI verlieren die
  MPRIS-Krücke. Aufwand groß (Schätzung: 3–5 Pakete auf Ebene der
  „episodes-as-queue-citizens"-Pakete), Risiko hoch, Nutzen groß und dauerhaft.
- **(B) Zurückstellen und ausbauen.** `ui/runtime/` löschen, Runtime-Crates auf
  einem Branch parken, die beiden `.service`-Dateien und das Meson-Target aus
  der Auslieferung nehmen, `check-runtime-service-install.sh` mitnehmen.
  Aufwand klein, Risiko klein, gewinnt die ~15.000 Zeilen Pflegelast zurück.

**Nicht empfohlen: den Zustand über die Testfreigabe hinweg lassen.** Der
Schwebezustand ist die einzige Variante, die alle Kosten beider Optionen trägt.
Für eine baldige Freigabe ist **(B) mit dokumentierter Wiederaufnahme** der
ehrlichere Weg; **(A)** ist die richtige Antwort auf „mehrere Apps auf demselben
Kern", aber nicht in denselben Wochen wie eine erste Testrunde.

### 2.3 Befund A2 — `rusqlite::Error` ist der Fehlertyp der öffentlichen Kern-API

858 öffentliche Kernsignaturen geben `Result<_, rusqlite::Error>` zurück. Folge:
**jede** Oberfläche muss `rusqlite` als direkte Abhängigkeit führen, nur um
Kernfehler benennen zu können. Die Manifeste sagen es selbst — `reprise-mcp`:
*„`rusqlite` is a direct dependency only because the core facades surface
`rusqlite::Error` in their signatures."*

Das ist die letzte verbliebene Persistenz-Leckage nach ADR 002. Für eine
zweite App (KDE/Qt, Android, iOS) bedeutet sie: das Frontend kompiliert SQLite
mit, obwohl es nie SQL sieht, und muss auf einen Fremdfehlertyp matchen, dessen
Varianten nichts über die Fachlichkeit sagen.

**Empfehlung:** ein `reprise_core::CoreError` (thiserror) mit den wenigen
Klassen, die Aufrufer wirklich unterscheiden — `NotFound`, `Conflict`,
`Busy` (SQLITE_BUSY, für Retry-Entscheidungen), `Invalid`, `Backend(String)`.
`rusqlite::Error` wird `#[from]` gefaltet und nie durchgereicht. Umstellung
mechanisch und schrittweise möglich (Modul für Modul, `From`-Impl trägt die
Zwischenstände). Erst danach kann `rusqlite` aus `reprise-cli`/`reprise-mcp`
verschwinden — was das Gate in `check-architecture.sh` dann sogar prüfen kann.

### 2.4 Befund A3 — die Kompositionswurzel kennt jede Ansicht

`crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs` definiert
`RuntimeWiring` mit **über 40 Feldern**: jede Ansicht, jeder Runtime, jedes
Widget der Fensterdekoration. `window.rs` selbst ist diszipliniert klein (597
Zeilen, Gate bei 600), aber die Verdrahtung ist nur verschoben, nicht aufgelöst.

Für „mehrere Apps" ist das die praktische Bremse: keine Ansicht lässt sich
einzeln in einer anderen Schale hochziehen, weil ihre Verdrahtung nur als
Gesamtpaket existiert. Für die *heutige* App ist es tragbar — deshalb steht
dieser Punkt in Welle 2, nicht in Welle 0.

**Empfehlung:** nicht das Struct aufteilen (das verschiebt nur wieder), sondern
pro Ansicht ein schmales `…Ports`-Struct einführen, das genau die Kollaborateure
nennt, die sie braucht. `RuntimeWiring` baut diese Ports und übergibt sie; die
Ansicht kennt `RuntimeWiring` nicht mehr. Das ist inkrementell pro Ansicht
machbar und jede Etappe kompiliert.

### 2.5 Befund A4 — `AGENTS.md` beschreibt das Projekt von vor drei Crates

`AGENTS.md` sagt „Three-crate Cargo workspace" und listet drei Crates. Es sind
neun. Die Roadmap dort endet bei „GUI-A2 (Cover-Download)"; tatsächlich sind
Podcasts, YouTube, Radio, Concerts, New Releases, Device-Sync, Library Doctor,
My Stats, Stems und die Runtime gelandet. Der Abschnitt „Not released yet — no
backwards compatibility" ist die Regel, die mit der Testfreigabe **kippt**
(§9.1).

Das ist kein Kosmetikpunkt: `AGENTS.md` ist laut eigener Ansage das erste, was
ein Agent liest. Ein falsches Weltbild an dieser Stelle erzeugt genau die Art
von Fehlentscheidung, die dieses Review sucht.

---

## 3. Fehlerbehandlung und Logging

### 3.1 Was trägt

- **Panikfreiheit ist praktisch erreicht.** In Produktionscode (ohne Testdateien
  und ohne `#[cfg(test)]`-Blöcke) stehen: `reprise-core` 1 `unwrap`,
  `reprise-gnome` ~20 verteilt auf Einzelstellen, `reprise-runtime`/
  `-client`/`-protocol` **0**. Das ist für 287k Zeilen außergewöhnlich.
- **`reprise_core::source_error`** ist eine vorbildliche Fehlerprojektion:
  `Display` trägt nur sichere Sätze, technische Nutzlast ausschließlich über
  `details()`, und es gibt Tests, die beweisen, dass weder `Display` noch
  `Debug` Host, Token oder Statuscode ausplaudern. Dazu eine geteilte
  Präsentationsentscheidung (Banner vs. Vollfläche, Aktionen je Quelle,
  Sammelmeldung ab drei Fehlern).
- 54 `thiserror`-Enums mit sprechenden Meldungen.

### 3.2 Befund E1 (Freigabe-Blocker) — Panik beim Datenbankstart

`crates/reprise-gnome/src/main.rs`:

```rust
let conn = db::Db::open_migrated(Some(&path)).expect("failed to open or migrate database");
```

Das ist der einzige Weg in die App. `DbError` kennt vier Fälle, und drei davon
sind für einen Tester realistisch:

- `SchemaTooNew` — Tester hat einen neueren Build ausprobiert und geht zurück.
  **Das ist der Downgrade-Fall, den `db.rs` bewusst erkennt** — und die GUI
  wirft ihn weg.
- `Io` — Platte voll, `~/.local/share` nicht schreibbar, Home auf Netzlaufwerk.
- `Sqlite` — beschädigte Datei nach hartem Ausschalten.

In allen drei Fällen: Prozess bricht mit Panikmeldung auf stderr ab, kein
Fenster, keine Meldung. Der Tester meldet „startet nicht".

**Empfehlung:** `open_migrated` behandeln und im Fehlerfall einen
`adw::AlertDialog` (bzw. bei fehlender GTK-Initialisierung eine
`g_printerr`-Meldung + Exitcode) zeigen, der je Fall etwas Handhabbares
anbietet: bei `SchemaTooNew` „Diese Bibliothek wurde mit einer neueren Version
angelegt" + Pfad, bei `Io`/`Sqlite` Pfad + „Diagnose kopieren". Klein, aber es
entscheidet, ob eine Testrunde brauchbare Rückmeldungen liefert.

### 3.3 Befund E2 (Freigabe-Blocker) — Logging erreicht den Tester nicht

`init_logging()` schreibt ausschließlich nach **stderr**, Filter über
`REPRISE_LOG`, Default `info,lofty=error`. Es gibt:

- keine Logdatei,
- keine Rotation,
- keine In-App-Ausleitung („Diagnose kopieren", „Logordner öffnen"),
- keinen Hinweis in `README.md`, wie man Logs bekommt.

Unter Flatpak landet stderr im Journal; ein Tester, der die App aus der
Übersicht startet, hat keinen sichtbaren Weg dorthin. 793 `tracing`-Aufrufe im
GTK-Crate sind also vorhanden und praktisch unerreichbar.

Zwei kleinere Punkte derselben Familie:

- **Kein Korrelationsbegriff.** 0 `tracing::trace!`, keine Spans. Ein
  Podcast-Refresh, der über Worker-Thread, HTTP-Boundary, Store und View läuft,
  hinterlässt Zeilen ohne gemeinsamen Faden. `#[tracing::instrument]` an den
  wenigen Einstiegspunkten (Scan, Refresh je Quelle, Device-Run, Job) würde
  das lösen, ohne Aufrufstellen anzufassen.
- **Verteilung schief.** 559 `warn!` gegen 138 `info!` und 105 `debug!`. Wenn
  `warn` die Standardablage für „unerwartet, aber egal" ist, verliert es seine
  Bedeutung als Signal. Beim Durchsehen der Logausleitung lohnt eine Runde
  Reklassifizierung.

**Empfehlung:** `tracing_subscriber` um einen zweiten Layer auf
`$XDG_STATE_HOME/reprise/reprise.log` ergänzen (eine Datei, beim Start rotiert,
Größe gedeckelt), plus einen Menüeintrag „Diagnose kopieren", der die letzten N
KB Log + Version + Schema-Version + aktive Module in die Zwischenablage legt.
Die Redaktionsregel steht schon: technische Nutzlast wie in `SourceError`
behandeln, keine Pfade aus dem Musikordner ins Log.

### 3.4 Befund E3 — 54 Fehlertypen ohne gemeinsame Ebene

Die Enums sind einzeln sauber, aber es gibt keine gemeinsame Achse. Konkret
fehlen zwei Fragen, die jede Oberfläche stellt und heute je Enum neu beantwortet
bekommt: *Ist das für den Nutzer sichtbar oder Diagnose?* und *Ist Wiederholen
sinnvoll?*

`source_error` beantwortet beides — aber nur für Netzquellen. `PodcastError`,
`RadioError`, `ConcertError`, `ProviderError`, `NewsError`, `LyricsError`,
`FetchError`, `PortraitError`, `RemoteStatsError` bilden jeweils eigene
Varianten für dieselben HTTP-Zustände (Timeout, Transport, Status, RateLimited,
Parse) und werden dann nach `SourceErrorKind` gefaltet.

**Empfehlung (klein, hoher Ertrag):** einen `SourceTransportError` als
gemeinsamen Rückgabetyp der HTTP-Boundary einführen (siehe §4.4) und die
domänenspezifischen Enums nur noch die *fachlichen* Fälle tragen lassen. Das
löst Befund D3 und E3 in einem Zug.

### 3.5 Befund E4 — verschluckte Fehler

~330 `let _ = …` / `.ok();` in Produktionscode über den Workspace. Stichproben
zeigen überwiegend legitime Fälle (Best-Effort-Cleanup, GTK-Rückgabewerte). Es
ist kein Gate-Thema, aber ein guter Kandidat für eine einmalige Durchsicht der
Kern- und Worker-Pfade mit der Frage: „Würde ich diesen Fehler im Bugreport
sehen wollen?" — und falls ja, `tracing::debug!` statt Schweigen.

---

## 4. Doppelentwicklung: Radio, Podcast, YouTube, Playlisten

Kurzantwort: **Ja, aber weniger und gezielter, als die Fragestellung
befürchtet.** Die großen Achsen sind richtig geteilt; dupliziert ist die
Schale drumherum.

### 4.1 Was bereits richtig geteilt ist — nicht anfassen

- **YouTube ist kein zweites Podcast-System.** Ein `PodcastKind { Rss, Youtube }`
  in einem Store, einer Pipeline, einem Datenmodell (`SubscriptionRow`,
  `EpisodeRow`); YouTube unterscheidet sich nur im Fetcher (`YoutubeFetcher`
  gegen `FeedFetcher`) und in den Projektionen (`podcasts/youtube.rs`, 245
  Zeilen). Die GTK-Seite teilt sich sogar den Typ: `RuntimeWiring` hält
  `podcasts_view` **und** `youtube_view` beide als `Rc<PodcastsView>`. Das ist
  vorbildlich gelöst.
- **Ein Track-List-Modell für alle lokalen Quellen.** `ViewSource` hat 17
  Varianten; Library, RecentlyAdded, Playlist, Smart, Queue, Missing, Album,
  Artist, Genre laufen alle über ein `TrackListModel` und eine `ColumnView`.
  Es gibt kein zweites Playlist-Widget.
- **Geteilte Quellen-Oberflächen:** `source_error.rs` (Fehlerpräsentation),
  `source_empty_state.rs` (Leerzustand für Podcasts/YouTube/Radio),
  `source_error_banner.rs`, `source_context_surface.rs` (die Zellen-Trefferfläche
  für Kontextmenüs), `source_add_action.rs`, `one_shot_task.rs`.
- **Eine Queue-Engine.** GTK-Controller und Runtime wrappen dieselben
  `Queue`/`UpNextQueue` aus dem Kern (die *Bindung* ist doppelt — §2.2, das ist
  ein Runtime-Befund, kein Quellen-Befund).

### 4.2 Befund D1 — fünf Filterleisten

| Datei | Zeilen |
| --- | ---: |
| `ui/browse/browse_bar.rs` (+ `_chips`, `_chooser`, `_count`, `_strings`) | 692 (+638) |
| `ui/concerts/concerts_filter_bar.rs` | 574 |
| `ui/releases/releases_filter_bar.rs` | 426 |
| `ui/radio/radio_filter_bar.rs` | 416 |
| `ui/podcasts/podcasts_filter_bar.rs` | 313 |

Geteilt wird davon exakt **eine CSS-Klasse** (`browse_bar::CHIP_CSS_CLASS`).
Alles andere ist fünfmal geschrieben: eigenes Facetten-Enum, eigenes
`remove_filter`, eigener Popover mit Facetten-/Werte-Seite, eigene
Ergebniszeile, eigene Persistenz-Keys. Die Kopiergrade sind bis in die
Konstanten sichtbar:

- `const FILTER_BAR_MIN_HEIGHT: i32 = 34;` — **5×** identisch.
- `const FACET_PAGE: &str = "facets"; const VALUE_PAGE: &str = "values";` — **3×**.

Konsequenz: Die Filter-Regeln aus `docs/ux-rules.md` Abschnitt K gelten faktisch
nur für `browse_bar`. Die gerade beschlossene Trennung Ort/Filter (§5) ist in
den vier anderen Leisten nicht abgebildet, weil sie deren Code nie erreicht hat.

**Empfehlung:** eine generische `FilterBar<F: FilterModel>` in `ui/browse/`, die
Geometrie, Chip-Aufbau, Popover-Navigation, „Alle löschen" und die Zählzeile
besitzt. Je Quelle bleibt ein kleines `FilterModel`-Impl (Facetten, Labels,
Werte, Persistenz-Key) — realistisch 60–120 Zeilen statt 300–570. Erwartete
Netto-Reduktion: ~1.200 Zeilen, und Abschnitt K wird zum ersten Mal für alle
Quellen wahr.

### 4.3 Befund D2 — zwei „Suche-oder-URL"-Dialoge

`ui/podcasts/add_dialog.rs` (754) + `add_dialog_input.rs` (430) +
`add_dialog_results.rs` (95) gegen `ui/radio/add_dialog.rs` (788) +
`radio_add_input.rs` (18) + `station_preview.rs` (79). Beide haben:

- dieselbe Phasenmaschine — `Idle → Searching → Results → Previewing → Preview
  → Error`,
- dasselbe `classify_input` → `AddInput` (Suchbegriff vs. URL),
- denselben Generationszähler gegen veraltete Ergebnisse,
- dieselbe `one_shot_task` + `source_add_action`-Verdrahtung,
- dieselbe Connectivity-Abfrage vor dem Absenden.

`docs/plans/podcasts-radio.md` §9.3 hat „`add_dialog.rs` je Feature" bewusst so
geplant. Nach der Landung ist der Beleg da, dass die Gemeinsamkeit größer ist
als angenommen.

**Empfehlung:** `ui/source_add_dialog.rs` mit der Phasenmaschine und der
Ergebnisliste; je Quelle ein Trait mit `classify_input`, `search`, `preview`,
`commit` und den Copy-Identitäten. Mittlere Größe, klarer Ertrag, geringes
Risiko (der Dialog hat eigene Tests auf beiden Seiten, die als Netz dienen).

### 4.4 Befund D3 — 16 HTTP-Boundaries, und der zugesagte Konsolidierungs-Task

`ureq::Agent::config_builder()` wird an **16** Stellen im Kern konstruiert:
`artist_portrait/deezer.rs`, `concerts/http.rs`, `cover_download.rs`,
`library/lastfm_stats.rs`, `library/library_doctor/remote/network.rs`,
`library/listenbrainz.rs`, `lyrics/lrclib.rs`, `lyrics/netease.rs`,
`musicbrainz.rs`, `podcasts/http.rs` (2×), `podcasts/source_artwork.rs`,
`radio/http.rs` (2×), `scrobbling.rs`, `scrobbling/lastfm.rs`. Es waren dreizehn
vor zwei Commits — die Zahl wächst, solange die gemeinsame Boundary fehlt
(§1.1).

`podcasts/http.rs` und `radio/http.rs` sind strukturell fast Zeile für Zeile
identisch: gleicher `static LAST_REQUEST: Mutex<Option<Instant>>`, gleiches
`MIN_REQUEST_INTERVAL`, gleicher `FIXTURE_DIR_ENV`-Mechanismus mit
`thread_local`-Override, gleiche `classify_transport`-Faltung nach
`SourceErrorKind`.

Die Ratenbegrenzung ist **fünfmal getrennt** implementiert (`radio`, `podcasts`,
`concerts`, `musicbrainz`, `artist_portrait/deezer`), jede mit eigenem
prozessweitem Mutex. Es gibt damit *kein* gemeinsames Anfragebudget: fünf
Quellen dürfen parallel je einmal pro Sekunde feuern. Für „Netzwerk aus",
„gemessene Verbindung" und Backoff bedeutet das fünf Orte, an denen eine
Richtlinie eingehalten werden muss.

Fünf getrennte Fixture-Verzeichnis-Variablen (`REPRISE_RADIO_FIXTURE_DIR`,
`REPRISE_PODCASTS_FIXTURE_DIR`, `REPRISE_CONCERTS_FIXTURE_DIR`,
`REPRISE_MUSICBRAINZ_FIXTURE_DIR`, `REPRISE_LRCLIB_FIXTURE_DIR`) sind die
Testseite desselben Musters.

**Das ist bereits beschlossene Arbeit.** `docs/plans/podcasts-radio.md`, Zeile
der Grill-Beschlüsse: *„Boundary-Klone bestätigt + fester Konsolidierungs-Task
nach Landung beider Features."* Beide sind gelandet. Der Task ist fällig.

**Empfehlung:** `reprise_core::net` mit
- einem `SourceClient { agent, user_agent, timeout, rate: &'static RateLimiter }`,
- **einem** Ratenbegrenzer mit Budget pro Host statt pro Modul,
- einer `SourceTransportError`-Faltung (löst zugleich E3),
- **einer** Fixture-Variable `REPRISE_HTTP_FIXTURE_DIR` mit Unterordner je
  Provider.

Kern-only, GUI-frei, testbar ohne Netz — genau die Sorte Arbeit, die dem
Mehr-App-Ziel direkt dient, weil eine zweite App diese Richtlinien sonst
neu bauen müsste.

### 4.5 Befund D4 — fünf parallele Tabellenseiten

`track_list`, `podcasts`, `radio`, `releases`, `concerts` haben jeweils
`*_view.rs`, `*_columns.rs`, `*_model.rs`, `*_presentation.rs`,
`*_empty_state.rs`, `*_failure_ui.rs`, `*_filter_bar.rs`, `css.rs` — dieselbe
Dateigrammatik, fünfmal ausgeschrieben. Vier eigene `ColumnView`-Konstruktionen
plus vier eigene `SignalListItemFactory`-Sätze.

Das ist ein bewusst gewachsenes Muster und funktioniert. Ich empfehle hier
**keine große Vereinheitlichung**: die Zeilenformen unterscheiden sich real
(Track-Zeile mit Rating und Cover; Episodenzeile mit Downloadzustand;
Stationszeile mit Favoritenstern; Release-Zeile mit Cover und Affiliate-Link).
Eine gemeinsame `SourceTablePage`-Abstraktion würde vor allem Konfiguration
statt Code erzeugen.

Was sich lohnt, ist der schmale Teil: Filterleiste (D1), Leerzustand (bereits
geteilt), Fehlerbanner (bereits geteilt), Kontextflächen (bereits geteilt) —
also genau das, was nicht die Zeilenform ist.

### 4.6 Befund D5 — zwei Queue-Kommandoflächen

Siehe §2.2. Wird durch die Runtime-Entscheidung mit erledigt und ist kein
eigener Task.

---

## 5. Filter auf Playlisten vs. Filterpillen auf Interpretenseiten

### 5.1 Antwort

**Ja, die Trennung ist sauber — seit `c565671` (heute, 2026-07-31).** Vorher war
sie es nicht, und die Frage trifft genau die Stelle, an der es klemmte.

Das Modell steht jetzt in
`crates/reprise-gnome/src/ui/browse/filter_restriction.rs` als reine, GTK-freie
Entscheidungsschicht:

| | **Ort** (place) | **Filter** |
| --- | --- | --- |
| Bedeutung | wo man ist | was innerhalb davon zurückgehalten wird |
| Betreten/Verlassen | Navigation, History-Push | Zustandsänderung am selben Ort |
| Angezeigt durch | Sidebar-Zeile — oder, wenn keine existiert, die **Ortspille** | Chips + „Alle löschen" |
| Gilt für | Artist, Album, Genre | Suche, Facetten, „KI-Musik ausblenden" |

Die drei tragenden Funktionen:

- `has_place_pill(source)` — wahr **nur** für `Artist`/`Album`/`Genre`, also
  genau die Orte, die man aus der Trackliste heraus betritt und die keine
  Sidebar-Zeile haben.
- `is_restricted(search, browse, exclude_ai)` — ein Ort ist **nie** eine
  Einschränkung. Nur Suche, Facetten und der KI-Ausschluss schränken ein.
- `row_visible(is_track_source, restricted, has_place_pill, preference_visible)`
  — die Zeile erscheint, wenn eingeschränkt **oder** eine Ortspille fällig ist
  **oder** die Einstellung sie will.

Das Wegklick-Verhalten ist damit strukturell verschieden, nicht nur optisch:

- **Playlist mit Filter** → Chip trägt ein `×`. Klick entfernt den Filter,
  der Ort bleibt die Playlist. Die Zählung bleibt „X von Y" relativ zur
  Playlist.
- **Interpretenseite** → Ortspille, **ohne** `×`, mit vorangestelltem `‹`, ganze
  Pille ist Klickziel, Tooltip und Accessible-Label nennen ein Ziel
  („Interpretenseite verlassen"), nicht eine Entfernung. Klick ist eine
  NAV-2-Navigation mit History-Push. Die Wiedergabe bleibt unberührt (PLAY-8).
- **Interpretenseite mit Filter** → beide Zonen nebeneinander, durch einen
  Separator getrennt; Zählung „2 von 3 Titeln" relativ zum **Ort**, nie zur
  Bibliothek.

Das ist gut gebaut: die Entscheidung liegt in reinen Funktionen mit
regelbenannten Tests (`fil_1c_places_carry_a_pill_without_restricting`,
`fil_1c_sidebar_places_carry_no_pill`, `fil_8_recently_added_is_a_sidebar_place_without_a_pill`,
`fil_2_row_shows_for_a_place_pill_without_any_filter`), nicht in Widget-Code.

### 5.2 Wie es vorher falsch war — für den Rückblick

Die Design-Datei `docs/superpowers/specs/2026-07-31-place-pill-vs-filter-pill-design.md`
hält die Messung fest: Interpretenseite zeigte `FILTER`, Pille `Alpha Artist ×`,
Zähler `3 of 9 tracks` — optisch nicht von einem Facetten-Chip zu unterscheiden,
aber das `×` verließ den Ort statt einen Filter zu entfernen. Gleiche Form,
gleiche Überschrift, gleiche Zählvokabel; andere Bedeutung, andere Geste,
andere Folge. Genau der Verdacht aus der Fragestellung.

### 5.3 Restrisiken

1. **Die vier anderen Filterleisten kennen die Unterscheidung nicht** (§4.2).
   Podcasts, Radio, Releases, Concerts bauen ihre Chips selbst. Sie haben heute
   keine Orte im Sinne von Artist/Album/Genre, also ist es kein aktiver Fehler —
   aber `youtube_channel_detail.rs` (629 Zeilen) *ist* ein Ort innerhalb einer
   Quelle. Ob dessen Rücksprung derselben Grammatik folgt, sollte gegen FIL-1c
   geprüft werden.
2. **Der Nachbar-Bug ist gefixt — geprüft, nicht angenommen.** Die Design-Datei
   nennt ihn ausdrücklich: jede Queue-Mutation löste einen Sidebar-Refresh aus,
   und `resolve_select_source` fiel auf Library zurück, weil Artist/Album/Genre
   keine Sidebar-Zeile haben — die Interpretenseite sprang also beim
   Doppelklick auf einen Track weg. Der Guard ist auf `dev` **und** auf `main`
   vorhanden: `sidebar/sidebar.rs:466` definiert `has_sidebar_row`,
   `sidebar_rebuild.rs:370` benutzt es, `sidebar_tests.rs:663/674` deckt beide
   Seiten ab. Damit ist kein Restrisiko mehr offen; der Punkt bleibt hier nur
   als Beleg stehen.
3. **Zwei Wahrheiten für „hat eine Sidebar-Zeile" — und die bleibt offen.**
   `has_place_pill()` (`browse/filter_restriction.rs`) und `has_sidebar_row()`
   (`sidebar/sidebar.rs`) ziehen dieselbe Unterscheidung in zwei getrennten
   `matches!`-Ausdrücken in zwei Modulen. Heute sind sie einig; nichts hält sie
   dabei. Der nächste Ort, der hinzukommt, wird in genau einem der beiden
   ergänzt. **Eine Funktion, zwei Aufrufer** — das ist der billigste Fix im
   ganzen Dokument.

---

## 6. Performance

### 6.1 Messung — Standardsortierung läuft ohne Index

Nachgestellt mit dem echten Tabellen- und Indexstand aus `db.rs` und den echten
Query-Strings aus `queries/clauses.rs`, 100.000 Zeilen, `ANALYZE` gelaufen,
SQLite 3.45.1. `EXPLAIN QUERY PLAN` je Sortierfeld:

| Sortierung | Plan |
| --- | --- |
| `artist` (**Standardansicht**) | `SCAN tracks` + `USE TEMP B-TREE FOR ORDER BY` |
| `title` | `SCAN tracks USING INDEX idx_tracks_present_title_nocase` |
| `album` | `SCAN tracks USING INDEX idx_tracks_present_album_order` |
| `genre`, `year`, `added_at`, `rating`, `play_count`, `duration_ms` | `SCAN` + Temp-B-Tree |

Nur `title` und `album` haben passende partielle NOCASE-Indizes. Die
Standardsortierung `artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no`
hat keinen — `idx_tracks_artist ON tracks(artist)` ist weder NOCASE noch
partiell und kann die Ordnung nicht liefern.

Laufzeiten desselben Fensters (LIMIT 200), Median aus 9 Läufen:

| Fall | offset 0 | offset 50.000 | offset 99.800 |
| --- | ---: | ---: | ---: |
| `artist`, ohne Index (**heute**) | 14,9 ms | **312 ms** | **380 ms** |
| `artist`, mit Kandidatenindex | 0,44 ms | 1,95 ms | 3,37 ms |

Der Kandidat:

```sql
CREATE INDEX idx_tracks_present_artist_order
ON tracks(artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no)
WHERE missing_since IS NULL AND removed_at IS NULL;
```

Das ist die mit Abstand größte Einzelwirkung im ganzen Review: **eine Migration,
eine Indexzeile, Faktor 30–100 auf dem meistbenutzten Pfad der App.**

Einschränkung, ehrlich benannt: in-memory-DB, synthetische Daten, Python-SQLite.
Absolutwerte auf echter Hardware und echter Datei werden abweichen; das
Planverhalten und das Verhältnis nicht.

### 6.2 Warum es sich als Ruckeln zeigt

`TrackListModel::item()` führt die Fensterabfrage **synchron auf dem GTK-Thread**
aus, wenn der Cache verfehlt (dokumentiert in `track_list_model.rs`, bewusst so
entschieden). Bei 0,4 ms ist das richtig. Bei 312 ms ist es ein sichtbarer
Frame-Aussetzer beim Scrollen. Der Cache (8 Fenster à 200 Zeilen) hilft nur bei
Rückwärtsbewegung im selben Bereich.

Nach dem Index gilt wieder, was die Entscheidung ursprünglich rechtfertigte.

### 6.3 Weitere Befunde, nach Ertrag sortiert

1. **Suche ohne Volltextindex.** `filter_clause` baut
   `title LIKE '%x%' OR artist LIKE … OR album LIKE … OR genre LIKE …` — kein
   Index kann das bedienen. Gemessen: 28 ms Fenster + 29 ms `COUNT(*)` je
   Tastendruck bei 100k Titeln, also ~57 ms auf dem UI-Thread. Es gibt einen
   200-ms-Debounce (`window.rs`), das rettet die Eingabe, aber jedes Zeichen
   kostet weiterhin zwei volle Scans.
   **Empfehlung, gestaffelt:** (a) kurzfristig `COUNT(*)` und Fenster nicht
   getrennt zählen lassen, wo die Gesamtzahl nur für „X von Y" gebraucht wird —
   ein `LIMIT`-basiertes „mehr als N" reicht für die Anzeige oberhalb einer
   Schwelle; (b) mittelfristig eine FTS5-Contentless-Tabelle über
   `(title, artist, album, genre)`, per Trigger gepflegt. FTS5 ist in
   `rusqlite`s `bundled` enthalten, also keine neue Abhängigkeit.
2. **`OFFSET`-Paginierung.** Auch mit Index ist `OFFSET 99.800` linear (3,4 ms
   gegen 0,4 ms). Für 100k tragbar, für „beliebig große Bibliothek" nicht.
   Keyset-Paginierung wäre die saubere Antwort, ist aber ein größerer Eingriff
   (die Sortier-Whitelist müsste stabile Tiebreaker garantieren). **Nach dem
   Index neu bewerten, nicht vorher.**
3. **Fehlende Indizes für die übrigen Sortierfelder.** `genre`, `year`,
   `added_at`, `rating`, `play_count` laufen alle über Temp-B-Tree.
   `added_at DESC` ist der Standard für „Zuletzt hinzugefügt" und lohnt
   wahrscheinlich als zweiter Index; die restlichen erst nach Messung, welche
   Spalten Nutzer wirklich sortieren — jeder Index kostet Schreiblast beim Scan.
4. **`ANALYZE` läuft nie.** Weder beim Öffnen noch nach einem Scan. SQLite
   plant dann nach Heuristik. Nach großen Scans einmal `PRAGMA optimize`
   (billig, no-op wenn nichts zu tun) wäre die konventionelle Antwort.
5. **Fünf unabhängige Ratenbegrenzer** (§4.4) sind auch ein Performance-Thema:
   beim Start können mehrere Quellen gleichzeitig Netz und CPU belegen, ohne
   dass irgendwo ein Gesamtbudget existiert.

### 6.4 Was bereits gut ist

Nicht übersehen: Fenster-Virtualisierung von Widgets **und** Daten,
gedeckelter Fenstercache, Generationstoken gegen veraltete Async-Ergebnisse,
`REPRISE_PERF_RUNTIME_REPORT` als eingebaute Widget-/Cache-Messung,
`scripts/performance-baseline.sh` mit 10k-/100k-Profilen. Die Infrastruktur, um
diese Verbesserungen zu belegen, ist bereits da — sie sollte für den Index-Fix
auch benutzt werden.

---

## 7. Sicherheit

Reprise ist ein lokaler Desktop-Player, aber es gibt drei echte
Vertrauensgrenzen: **fremde Feeds** (RSS/YouTube/radio-browser liefern Text,
URLs und Bilder von Dritten), **ein Subprozess** (`yt-dlp`), und **die
Agenten-Oberfläche** (MCP/CLI schreiben in dieselbe Datenbank). Das Review hat
sie einzeln durchgesehen.

### 7.1 Was bereits richtig gebaut ist

Diese Punkte sind bemerkenswert sorgfältig gelöst und sollten so bleiben:

- **Keine SQL-Injektion durch Sortierparameter.** `SORT_WHITELIST` in
  `queries/clauses.rs` ist eine Nachschlagetabelle; `sort_field` wird
  ausschließlich als Schlüssel benutzt, nie interpoliert. Unbekannte Werte
  fallen still auf `title` zurück. Alle Nutzereingaben laufen als gebundene
  Parameter.
- **Keine Pfad-Traversierung aus fremden Daten.** Podcast-Downloads landen
  unter `fnv1a_64(feed_url)/fnv1a_64(guid).ext` (`podcasts/downloads.rs`) —
  ein Feed kann seinen Dateinamen also gar nicht wählen. Ein `../..` im GUID
  ist nach dem Hash ein Hexwort.
- **Der neue Schreibpfad in die Musiksammlung ist eng gefasst.** Seit `#189`
  schreibt Reprise `cover.<ext>` und `.lrc` neben vorhandene Titel
  (`cover_writeback.rs`, `lyrics/sidecar_write.rs`, `writeback_publish.rs`).
  Das Ziel wird ausschließlich aus dem *Trackpfad* abgeleitet, nie aus
  Providerdaten; `write_album_cover` prüft zusätzlich, dass die Bytes wirklich
  das behauptete Bildformat sind (`validated_image_extension`), dass die
  Endung in `cover::IMAGE_EXTS` steht, und bricht ab, wenn das Album bereits
  Artwork hat. Eine bestehende Datei wird nie überschrieben. Das ist die
  richtige Konstruktion für die riskanteste Operation der App.
- **Antwortgrößen sind gedeckelt.** `http_body::read_bounded_string` bei 2 MB
  für Feeds und JSON, `cover_download::MAX_IMAGE_BYTES` bei 20 MB,
  `source_artwork::MAX_IMAGE_BYTES` bei 4 MB — jeweils mit `take(N+1)` und
  Prüfung, also kein unbegrenzter Speicherfraß durch einen bösartigen Server.
- **XML-Bomben sind ausgeschlossen.** `podcasts/feed.rs` benutzt `quick-xml`
  mit `check_end_names = true` und **expandiert keine Entities**: eine
  undeklarierte Entity wird verbatim übernommen und geloggt, nie aufgelöst.
  Billion Laughs ist damit strukturell unmöglich, nicht nur unwahrscheinlich.
- **Fehlermeldungen lecken nichts.** `SourceError` trennt sichere Anzeige von
  technischer Nutzlast, und drei Tests beweisen, dass weder `Display` noch
  `Debug` Host, Token, Statuscode oder Pfad ausgeben.
- **`unsafe` ist auf eine Stelle beschränkt** mit begründetem SAFETY-Kommentar
  (`kill(-pgid, SIGKILL)` in `podcasts/ytdlp.rs`); im GTK-Crate ist `unsafe`
  per Gate auf genau eine Allowlist-Datei begrenzt.
- **Bild-IDs werden validiert, nicht vertraut.** `youtube.rs` lässt nur das
  YouTube-ID-Alphabet in die Thumbnail-URL — mit einer Begründung im Code, die
  genau die richtige ist („turns an implicit assumption into a checked one").
- **Der Flatpak-Sandkasten ist eng und mechanisch bewacht.** Kein
  `--filesystem=home`, kein `--device=all`, kein Session-Bus;
  `check-flatpak-device-permissions.sh` lässt außer `xdg-run/gvfsd` gar keine
  `--filesystem=`-Zeile zu und bricht sonst ab.
- **Zugangsdaten liegen im Keyring** (`oo7`), nicht in der Datenbank; die
  gebündelte Ticketmaster-Kennung ist in `RELEASING.md` ausdrücklich als „aus
  einer veröffentlichten Binary extrahierbar" markiert statt als Geheimnis
  behandelt.

### 7.2 Befund S1 — kein `--` vor der URL beim yt-dlp-Aufruf

`ytdlp.rs::list/resolve` und `ytdlp_download.rs` hängen die URL als letztes
Positionsargument an, **ohne** vorangestelltes `--`. yt-dlp wertet Optionen an
jeder Position aus, also entscheidet allein der Inhalt der Zeichenkette, ob sie
als URL oder als Option gelesen wird.

Heute ist das **nicht ausnutzbar**, und zwar aus drei voneinander unabhängigen
Gründen — die aber alle zufällig sind, nicht zugesichert:

1. Nutzer- und Agenteneingaben laufen durch `url_detect::detect`, das nur
   `http`/`https` durchlässt; eine Zeichenkette mit `-` am Anfang parst nicht
   als URL und wird zur Suche.
2. Episoden-URLs entstehen als `format!("https://www.youtube.com/watch?v={id}")`
   — der Präfix ist ein Literal.
3. Suchbegriffe werden zu `ytsearch5:{terms}` und beginnen damit nie mit `-`.

Es gibt aber keine Stelle, die diese Invariante *hält*. Ein künftiger Aufrufer,
der eine gespeicherte `feed_url` direkt weiterreicht (etwa nach einem Import
oder aus einer Migration), hebt sie auf, und der Compiler sagt nichts.

**Empfehlung (klein, defensiv):** in `run()` und im Download-Pfad ein `--`
unmittelbar vor dem ersten Positionsargument einfügen, plus eine
Debug-Assertion, dass die URL mit `http://` oder `https://` beginnt. Zwei
Zeilen, und die Invariante steht im Code statt in drei getrennten Zufällen.

### 7.3 Befund S2 — `--cookies-from-browser` gibt Cookies an einen fremden Prozess

Ist eine Browser-Sitzung konfiguriert, hängt jeder yt-dlp-Aufruf
`--cookies-from-browser <browser>` an — yt-dlp liest dann die Cookie-Datenbank
des Browsers und schickt die Cookies an YouTube. Das ist die vom Feature
gewollte Funktion (POD-22, „YouTube braucht einen angemeldeten Browser"), und
`resolve_browser_session` beschränkt den Wert auf unterstützte Browser
(`config.rs`-Test `pod_22_browser_session_round_trips_only_supported_browsers`).

Trotzdem ist es die weitreichendste Berechtigung, die die App überhaupt
ausübt: der Zugriff auf die Anmeldedaten eines anderen Programms. Zwei
Beobachtungen:

- **Unter Flatpak funktioniert es ohnehin nicht** — der Sandkasten sieht das
  Browserprofil nicht. Das ist gut, sollte aber als Verhalten *erklärt* werden,
  sonst wird es als Bug gemeldet.
- **`REPRISE_YTDLP_COOKIES_FROM_BROWSER`** (`ytdlp_discovery.rs`) umgeht die
  Einstellung per Umgebungsvariable. Für Entwicklung sinnvoll; in einem
  Release-Build sollte diese Variable ignoriert werden, damit die einzige
  Quelle für diese Entscheidung die sichtbare Einstellung bleibt.

**Empfehlung:** Copy im Plugin-Bereich, die benennt, was der Schalter tut („liest
die YouTube-Cookies deines Browsers"), und den Umgebungs-Override auf
Debug-Builds beschränken.

### 7.4 Befund S3 — Weiterleitungen ohne Zielprüfung (SSRF, geringe Schwere)

`ureq` folgt Weiterleitungen (Standard: bis zu 10). Es gibt keine Prüfung des
Weiterleitungsziels — ein bösartiger Feed kann also auf `http://127.0.0.1:…`
oder eine Adresse im lokalen Netz zeigen, und Reprise holt sie ab.

Schwere ist gering: die Antwort wird als Feed/JSON/Bild geparst, das Ergebnis
erreicht den Angreifer nicht zurück, und ein Desktop-Client ist kein
interessanter SSRF-Pivot. Erwähnenswert ist es trotzdem, weil ein Nutzer eine
URL abonniert und nicht erwartet, dass sein Rechner dadurch sein eigenes
Netzwerk abklopft.

**Empfehlung:** kein eigener Resolver-Umbau. Eine kleine Prüfung im gemeinsamen
`SourceClient` (Welle 2, §4.4) reicht: Weiterleitungen auf Loopback-,
Link-Local- und private Adressbereiche ablehnen und als
`SourceErrorKind::Unreachable` melden. Genau dafür ist die gemeinsame
HTTP-Boundary die richtige Stelle — heute müsste man es fünfmal einbauen.

### 7.5 Befund S4 — die Agenten-Oberfläche schreibt in dieselbe Datenbank

`reprise-mcp` ist als „read-only Ressourcen + capability-gated create tools"
entworfen und hält das auch: `PlayTrackIds`, `QueueAddNext`, `QueueAddLast`
bleiben track-only und validieren gegen vorhandene IDs; es gibt keine
Lösch-, Tag- oder Playback-Werkzeuge über den vorgesehenen Umfang hinaus.
`capability.rs` ist die Torkontrolle.

Zwei Dinge, die vor einer Freigabe mit aktivem MCP klar sein sollten:

- **Die Capability-Erteilung ist nicht sichtbar in der App.** Ein Nutzer, der
  den MCP-Server einrichtet, sieht in Reprise selbst nicht, welche Klassen von
  Operationen ein Agent gerade darf.
- **`source_actions.rs` nimmt URLs von Agenten** und legt Abos an. Der Pfad
  geht durch `url_detect`, ist also so eng wie der GUI-Pfad — aber ein Agent
  kann damit unbeaufsichtigt Netzverbindungen zu selbstgewählten Hosts
  auslösen. Das ist gewollt (deshalb capability-gated), gehört aber in die
  Testrunden-Dokumentation, nicht in die Entdeckung durch einen Tester.

**Empfehlung für die Testrunde:** MCP standardmäßig aus, und wenn an, mit einer
sichtbaren Zeile in den Einstellungen, welche Capabilities erteilt sind.

### 7.6 Was noch geprüft gehört, hier aber nicht abschließend bewertbar

- `cargo audit` läuft im Gate mit genau einer akzeptierten Advisory
  (RUSTSEC-2024-0436, `paste` über `lofty`). Die Regel „eine neue Advisory =
  STOP" ist die richtige; für ein Release sollte zusätzlich `cargo deny`
  (Lizenzen + Duplikate) einmal laufen, weil `LICENSING.md` Aussagen macht, die
  heute niemand mechanisch prüft.
- Die `image`-Dekodierung nutzt `image::load_from_memory` ohne explizite
  `Limits`. Die Byte-Obergrenze davor deckt den einfachen Fall ab; eine
  Dekompressionsbombe (kleines PNG, riesige Pixelfläche) ist damit **nicht**
  abgedeckt. `image::Limits` mit `max_alloc` und maximaler Kantenlänge zu
  setzen ist eine Zeile pro Dekodierstelle und schließt die Lücke.

---

## 8. Stabilität

Die Frage „stürzt die App im Feld ab?" hat hier eine ungewöhnlich gute und eine
ungewöhnlich schlechte Antwort — beide messbar.

### 8.1 Die gute Hälfte: Panikfreiheit ist erarbeitet, nicht behauptet

- Produktionscode enthält praktisch keine `unwrap`/`expect` (§3.1): Kern **1**,
  Runtime-Crates **0**, GTK ~20 an eng begründeten Einzelstellen.
- `TrackListModel` degradiert bei jedem Datenbankfehler zu `None`/`0` und
  loggt, statt zu panicken — ausdrücklich dokumentiert: *„a broken DB
  connection must never crash the UI thread."*
- Generationstoken verhindern, dass verspätete Cover, Metadaten, Lyrics oder
  Fortschrittswerte in eine recycelte Zeile schreiben.
- `one_shot_task` benennt jeden Worker-Thread (auffindbar im Backtrace) und ist
  abbruchsicher: ein weggeworfener Empfänger verwirft nur das Ergebnis.
- Die `Db`-Handle-Migration hat **575** `RefCell`-Borrows auf dem Datenbankpfad
  ersatzlos entfernt — die häufigste Panik-Klasse des Projekts ist dort
  strukturell ausgeschlossen.
- Der Ledger zeigt, dass diese Klasse aktiv gejagt wird (Task 0.4: ein
  reentranter Subscriber-Borrow im Podcast-Runtime, mit rot-grün-Regression
  geschlossen).

### 8.2 Befund T1 (kritisch) — eine Panik ist ein stiller Abbruch

Trotzdem stehen im GTK-Crate weiterhin **1.633** `borrow()`/`borrow_mut()` über
rund 160 `Rc<RefCell<…>>`-Zellen. `AGENTS.md` nennt genau das „the #1 recurring
panic class". Was passiert, wenn eine davon zuschlägt:

1. Der `BorrowMutError` paniert in einem GTK-Callback.
2. Der Callback wird über die C-Grenze aufgerufen; ein Unwind über `extern "C"`
   ist in heutigem Rust ein **Abbruch**, kein Fehlerpfad. Der Prozess ist weg.
3. Die Panikmeldung geht nach **stderr** — und dort erreicht sie den Tester
   nicht (§3.3).
4. Es gibt **keinen `panic::set_hook`** irgendwo im Workspace.

Damit ist der schlimmste Fehlerfall zugleich der am schlechtesten
diagnostizierbare: Fenster verschwindet, keine Meldung, kein Artefakt, kein
Bugreport, der mehr sagt als „war plötzlich weg".

**Empfehlung (gehört in Welle 0, zusammen mit 0.2):** ein
`std::panic::set_hook`, der Panik-Nachricht, Ort und Backtrace in die Logdatei
schreibt, bevor der Prozess endet, plus eine Markerdatei
(`$XDG_STATE_HOME/reprise/last-crash`), die beim nächsten Start eine Zeile
anbietet: „Reprise wurde beim letzten Mal unerwartet beendet — Diagnose
kopieren?". Das ist wenig Code und verwandelt die schlimmste Fehlerklasse von
unsichtbar in berichtbar. `RUST_BACKTRACE=1` sollte der Hook selbst setzen, weil
ein Tester es nie setzt.

### 8.3 Befund T2 — ein gestorbener Worker wird nicht überall bemerkt

`one_shot_task` liefert sein Ergebnis über einen Kanal. Panik der Aufgabe →
Sender fällt → Empfänger bekommt `Err(RecvError)`. Die Aufrufer behandeln das
uneinheitlich:

- **Vorbildlich:** `tag_edit/tag_edit_flow.rs` hat einen eigenen `Err`-Arm,
  loggt „worker channel closed unexpectedly", reaktiviert die Dialogknöpfe und
  zeigt eine Meldung.
- **Lücke:** `delete_tracks.rs` macht
  `let Ok(result) = receiver.recv().await else { return; };` — der Dialog
  bleibt, wie er ist, ohne Meldung und ohne Log.

**Empfehlung:** die Konvention in `one_shot_task` verankern, statt sie je
Aufrufer zu wiederholen — etwa ein `recv_or_fault(&receiver, "delete tracks")`,
das im Fehlerfall loggt und einen typisierten Fehlgrund zurückgibt. Danach ist
„Worker gestorben" ein Zustand mit Namen und nicht ein `return`.

### 8.4 Befund T3 — Startpfad ohne Rückfallebene

Zusammengefasst aus §3.2, hier unter dem Stabilitätsaspekt: die App hat
**genau einen** Weg zu starten, und der endet bei Problemen in einer Panik.
Kein Fehlerdialog, kein Read-only-Modus, kein „Bibliothek an anderem Ort
öffnen". Für eine Testrunde auf fremden Rechnern — anderes Dateisystem,
volle Platte, Home auf NFS, ältere Datei nach Downgrade — ist das die
wahrscheinlichste Absturzquelle überhaupt, und sie ist gleichzeitig die
billigste zu beheben.

### 8.5 Befund T4 — was Stabilität heute *nicht* gefährdet

Damit die Liste ehrlich bleibt, auch die geprüften Nicht-Befunde:

- **Threading:** wenige, benannte Threads; GStreamer-Ereignisse überqueren die
  Threadgrenze ausschließlich als `Send`-Daten über einen `async-channel`, den
  eine einzige langlebige Schleife auf dem Hauptkontext leert. Der Drain hält
  nur ein `Weak`, kann den Controller also nicht am Leben halten.
- **Datenbank-Nebenläufigkeit:** WAL, `busy_timeout` 5 s als benannte
  Konstante, Worker öffnen eigene Handles statt eine Connection zu teilen,
  Migrationen laufen transaktional zusammen mit dem `user_version`-Bump.
- **Subprozess-Aufräumen:** yt-dlp läuft in einer eigenen Prozessgruppe mit
  Deadline; Timeout und Fehlerpfad killen die ganze Gruppe, kein verwaister
  Downloader.
- **Bekannte Fremdfehler sind dokumentiert** statt umschifft
  (`docs/upstream/`), inklusive Repro-Skripten.

---

## 9. Abwärtskompatibilität und Testfreigabe

### 9.1 Die Regel kippt mit der Freigabe

`AGENTS.md` sagt heute:

> **Not released yet — no backwards compatibility.** Reprise has **not** shipped
> and there are **no existing installations**.

Ab dem Tag, an dem der erste Tester installiert, ist dieser Satz falsch, und die
darauf gebaute Erlaubnis („wo sauberes und abwärtskompatibles Datenmodell
kollidieren, nimm das saubere und lösche die alte Form") wird zu einem
Datenverlustrisiko in fremden Bibliotheken.

**Empfehlung, im selben Commit wie die Freigabe:** Abschnitt ersetzen durch eine
Regel mit Stichtag, etwa: *ab Schema 50 / Version 0.1.1 gilt: Migrationen sind
vorwärtsgerichtet und verlustfrei; ein Feld darf entfallen, sobald eine
Migration seinen Inhalt überführt hat; Settings-Keys werden migriert, nicht
verworfen.* Ohne diese Änderung wird die nächste „saubere Umstellung"
regelkonform Testerdaten löschen.

### 9.2 Was schon abwärtskompatibel ist — gut

- **Schema 50, vorwärtsgerichtete Migrationen**, jeder Schritt in einer
  Transaktion zusammen mit dem `user_version`-Bump (der Kommentar in `db.rs`
  erklärt, warum das nach einem Crash-Fall so gebaut wurde).
- **`SchemaTooNew` wird erkannt** — Downgrade wird nicht stillschweigend auf
  einer neueren Datei gefahren. (Die GUI wirft die Information weg — §3.2.)
- **`db_grandfather.rs`** ist bereits ein echter Kompatibilitätsmechanismus:
  bestehende Datenbanken behalten Netzfunktionen, die sie vor der Einführung des
  Modul-Gates schon hatten, entschieden anhand von Belegen in den Daten (Abos,
  Radio-Favoriten, Downloads, Cover-Cache), nicht anhand einer Pauschale.
- **Protokoll-Kompatibilitätstest** im Runtime-Protokoll: ein älteres Dictionary
  ohne typisierte Felder wird weiterhin dekodiert (Ledger, Paket 5).
- **Eigene Migrationstestdateien** je Bereich (`db_recent_migration_tests.rs`,
  `db_podcasts_radio_migration_tests.rs`, `db_network_migration_tests.rs`, …).

### 9.3 Befund K1 (Freigabe-Blocker) — die deklarierte MSRV ist nicht erreichbar

Reproduziert in dieser Umgebung mit `cargo build -p reprise-core --locked`:

```
Compiling libsqlite3-sys v0.38.1
error[E0658]: use of unstable library feature `cfg_select`
  --> libsqlite3-sys-0.38.1/build.rs:110:9
```

Also: der **gepinnte** Abhängigkeitsgraph baut mit `rustc 1.94.1` nicht, während
jedes Workspace-Manifest `rust-version = "1.92"` deklariert.
`scripts/tests/msrv.sh` kann das nicht fangen — es liest ausschließlich
`cargo metadata` und prüft, dass das *Feld* überall `1.92` sagt. Es baut nie mit
dieser Toolchain.

Warum das für die Freigabe zählt: `org.reprise.Reprise.yml` baut mit
`org.freedesktop.Sdk.Extension.rust-stable` unter GNOME-Runtime 50 und
`CARGO_NET_OFFLINE=true` gegen `flatpak/cargo-sources.json` — also exakt gegen
diese gepinnten Versionen. Ob die rustc-Version dieser SDK-Extension neu genug
ist, entscheidet, ob das Flatpak überhaupt baut. Die CI baut auf Arch-Rolling
und beantwortet die Frage nicht.

**Empfehlung:**
1. Die tatsächlich nötige rustc-Version bestimmen (`cargo build --locked` mit
   der SDK-Toolchain oder in einem `flatpak-builder`-Lauf).
2. `rust-version` im Workspace darauf setzen — oder `rusqlite`/`libsqlite3-sys`
   auf eine Version zurücknehmen, die 1.92 hält.
3. `scripts/tests/msrv.sh` um einen echten Build mit der deklarierten Toolchain
   ergänzen, sonst misst der Check weiterhin etwas anderes als sein Name sagt.
4. Ein `rust-toolchain.toml` erwägen, damit Entwickler und CI dieselbe Toolchain
   sehen.

### 9.4 Befund K2 — `check-stem-runtime-packaging` ist rot auf der Basis

Der Ledger hält es fest: *„the extra release-only
`scripts/check-stem-runtime-packaging.sh` probe remains red on the unchanged
base because `build-aux/meson-cargo-build.sh` lacks the two ONNX runtime
environment markers the check requires."* Dieser Check ist Teil von
`scripts/check-release.sh`, nicht von `check-merge-readiness.sh` — er ist also
korrekt nicht als Merge-Blocker aufgetreten, wird aber jede Release-Prüfung
stoppen.

**Empfehlung:** vor der Freigabe entweder reparieren oder das Stem-Feature für
die Testrunde ausschalten (`-Dstem_backend=false`) und den Check entsprechend
gaten. Für eine erste Testrunde ist Letzteres die kleinere Wette — ein
experimentelles ML-Feature erzeugt Supportlast, die vom eigentlichen Testziel
ablenkt.

### 9.5 Befund K3 — der Flatpak-Sandkasten ist streng, und das ist der erste Stolperstein

`finish-args` enthält **kein** `--filesystem=home` und kein `--filesystem=xdg-music`;
`check-flatpak-device-permissions.sh` verbietet jede `--filesystem=`-Zeile außer
`xdg-run/gvfsd`. Bibliothekszugriff läuft also ausschließlich über den
Portal-Ordnerdialog. Das ist eine gute, bewusste Entscheidung — und zugleich das
Erste, was jeder Tester berührt.

**Empfehlung:** genau diesen Pfad vor der Freigabe manuell verifizieren, in
einem echten Flatpak, nicht in einem Dev-Build: Ordner wählen → scannen →
**App beenden** → neu starten → sind die Titel noch spielbar? Wenn die
Portalberechtigung nicht persistent gewährt ist, ist die App nach dem ersten
Neustart leer, und das ist der Bugreport, der eine ganze Testrunde dominiert.
`RELEASING.md` §„Manual GNOME QA" führt den Schritt bereits — er ist der
wichtigste der Liste.

### 9.6 Weitere Freigabe-Beobachtungen

- **`reprise-runtime` geht mit aus** (Meson-Target, zwei `.service`-Dateien) und
  wird von nichts benutzt (§2.2). Für eine Testrunde: nicht ausliefern.
- **`AGENTS.md`** ist inhaltlich veraltet (§2.5).
- **`docs/ux-rules.md`** hat zwei Abschnitte mit dem Buchstaben `T`
  (Zeile 1921 „T. Accessibility & Keyboard", Zeile 1995 „T. Network features
  opt-in") und keinen Abschnitt `AC`. Bei 307 aktiven Regeln, die per
  `check-ux-traceability.sh` auf Tests abgebildet werden, ist eine doppelte
  Sektionskennung eine Falle für die nächste Regelnummer.

---

## 10. Refactoring-Plan

> **Ausführung:** `docs/plans/consolidation-implementation.md` schreibt diese
> Wellen task-genau aus — roter Test, Dateien, Gate, Commit-Titel je Task für
> Welle 0 und 1, Paketebene mit Datei-Ownership für Welle 2 bis 5. Dieser
> Abschnitt bleibt die Übersicht; dort steht, wie es gebaut wird.

Priorisiert nach *Ertrag pro Risiko*, in Wellen, die einzeln landbar sind. Jede
Welle ist ein eigener Branch mit squashed PR gegen `dev`, nach der Methode aus
`AGENTS.md` (Test zuerst, volle Gate-Batterie, Ledger-Zeile).

### Welle 0 — Freigabe-Blocker (vor dem Öffnen der Testrunde)

| # | Task | Aufwand | Risiko |
| --- | --- | --- | --- |
| 0.1 | `main.rs`: `open_migrated` behandeln statt `expect`; Fehlerdialog je `DbError`-Fall (§3.2) | S | niedrig |
| 0.2 | Logdatei unter `$XDG_STATE_HOME/reprise/` + „Diagnose kopieren" im Hauptmenü (§3.3) | M | niedrig |
| 0.3 | MSRV/Toolchain klären, `msrv.sh` zu einem echten Build machen (§9.3) | S–M | niedrig |
| 0.4 | `AGENTS.md`: Crate-Liste, Roadmap, und **die Kompatibilitätsregel** auf den Stichtag umstellen (§9.1, §2.5) | S | niedrig |
| 0.5 | Entscheidung Runtime: ausliefern oder ausbauen — für die Testrunde: nicht ausliefern (§2.2) | S (Variante B) | niedrig |
| 0.6 | Flatpak-Portalpfad manuell verifizieren: Ordner wählen → scannen → Neustart → spielbar? (§9.5) | S | — |
| 0.7 | `check-stem-runtime-packaging` reparieren **oder** Stem-Feature für die Testrunde aus (§9.4) | S | niedrig |
| 0.8 | `std::panic::set_hook`: Panik + Backtrace in die Logdatei, Absturzmarker, beim nächsten Start „Diagnose kopieren?" (§8.2) | S | niedrig |
| 0.9 | Cover-/Lyrics-Writeback in die manuelle QA aufnehmen: schreibt nur wo erlaubt, überschreibt nie, Temp-Sweep greift (§7.1, §1.1) | S | — |
| 0.10 | MCP für die Testrunde standardmäßig aus; erteilte Capabilities in den Einstellungen sichtbar (§7.5) | S | niedrig |

Zusammen realistisch 1–2 Arbeitstage. Danach ist eine Testrunde belastbar:
Abstürze werden zu Meldungen, Meldungen kommen mit Logs, und was ausgeliefert
wird, wird auch benutzt.

**Reihenfolge innerhalb der Welle:** 0.2 und 0.8 zuerst und zusammen — sie
teilen sich dieselbe Logdatei, und ohne sie ist jede spätere Testrückmeldung
blind. Danach 0.1 (der Startpfad ist die wahrscheinlichste Absturzquelle
überhaupt), dann der Rest in beliebiger Folge.

### Welle 1 — Performance, mit Beleg

| # | Task | Aufwand | Risiko |
| --- | --- | --- | --- |
| 1.1 | Migration 51: `idx_tracks_present_artist_order` (§6.1) | S | niedrig |
| 1.2 | `scripts/performance-query-compare.sh` vorher/nachher für 10k und 100k festhalten | S | — |
| 1.3 | `PRAGMA optimize` nach großem Scan (§6.3.4) | S | niedrig |
| 1.4 | `added_at`-Index prüfen und ggf. mitnehmen | S | niedrig |

Welle 1 ist bewusst klein und vor Welle 2 einsortiert: sie ist die einzige
Änderung mit sofortiger, für Tester spürbarer Wirkung.

### Welle 2 — Quellen-Grammatik konsolidieren

| # | Task | Aufwand | Risiko |
| --- | --- | --- | --- |
| 2.1 | `reprise_core::net`: ein `SourceClient`, **ein** Ratenbegrenzer, eine Fixture-Variable, `SourceTransportError` (§4.4, §3.4) — der 2026-07-26 zugesagte Konsolidierungs-Task | L | mittel |
| 2.2 | Generische `FilterBar<F: FilterModel>`; die vier Quellen-Leisten darauf umstellen (§4.2) | L | mittel |
| 2.3 | `ui/source_add_dialog.rs`: eine Phasenmaschine für Podcasts und Radio (§4.3) | M | mittel |
| 2.4 | `has_place_pill()` / `has_sidebar_row()` zu einer Wahrheit zusammenführen (§5.3.3) | S | niedrig |
| 2.5 | `youtube_channel_detail` gegen FIL-1c prüfen (§5.3.1) | S | niedrig |
| 2.6 | `--` vor jedes yt-dlp-Positionsargument + Debug-Assertion auf `http(s)://` (§7.2) | S | niedrig |
| 2.7 | `image::Limits` (max. Kantenlänge, `max_alloc`) an jeder Dekodierstelle (§7.6) | S | niedrig |
| 2.8 | `recv_or_fault` in `one_shot_task`: „Worker gestorben" bekommt einen Namen (§8.3) | S | niedrig |

Die drei Sicherheits-/Stabilitätspunkte hängen an Welle 2, weil 2.1 die
gemeinsame HTTP-Boundary schafft — dort gehören auch die Weiterleitungsprüfung
(§7.4) und der aus `lyrics/breaker.rs` herausgehobene Circuit Breaker hin. Vor
2.1 müsste man jede dieser Regeln fünf- bis sechzehnmal einbauen.

Erwartete Netto-Reduktion Welle 2: grob 1.500–2.000 Zeilen, bei gleichzeitig
*strengeren* Garantien (ein Netzbudget statt fünf, Abschnitt K für alle
Quellen).

### Welle 3 — Kern-API für die zweite App

| # | Task | Aufwand | Risiko |
| --- | --- | --- | --- |
| 3.1 | `CoreError` einführen, `rusqlite::Error` aus öffentlichen Signaturen entfernen (§2.3) | L | mittel |
| 3.2 | `rusqlite` aus `reprise-cli`/`reprise-mcp` entfernen; Gate ergänzen | S | niedrig |
| 3.3 | Parameterobjekte statt der `query_track_window*`-Überladungen (7× `too_many_arguments` allein in `queries/mod.rs`) | M | niedrig |
| 3.4 | Pro Ansicht ein `…Ports`-Struct; `RuntimeWiring` verliert seine Rolle als Universalkontext (§2.4) | L | mittel |

Erst nach Welle 3 ist die Aussage „ein zweites Frontend erbt die Fachlichkeit"
mehr als eine Behauptung: dann kann eine zweite App den Kern nehmen, ohne SQLite
zu übersetzen und ohne die GTK-Verdrahtung nachzubauen.

### Welle 4 — die Runtime-Entscheidung ausführen

Nur wenn Welle 0.5 auf **(A) Cutover** entschieden wurde. Dann in Paketen wie
bei „episodes as queue citizens": Ports verdrahten → `PlayerController` auf
Snapshots umstellen → Queue-Kommandos umleiten → MPRIS-Spiegel vom Runtime
speisen → MCP/CLI von MPRIS auf `org.reprise.Reprise1` umstellen →
`transport_parity_tests` von Netz zu Vertrag befördern.

### Welle 5 — nur nach Messung

`OFFSET` → Keyset-Paginierung (§6.3.2) und FTS5-Suche (§6.3.1). Beide sind echte
Verbesserungen, aber beide sind erst nach Welle 1 ehrlich zu bewerten: der Index
verschiebt die Grenze so weit, dass die restlichen Kosten möglicherweise unter
der Wahrnehmungsschwelle liegen.

---

## 11. Ausdrücklich **nicht** empfohlen

Damit spätere Sessions nicht in dieselben Versuchungen laufen:

- **Keine `SourceTablePage`-Universalabstraktion** über Trackliste, Podcasts,
  Radio, Releases, Concerts (§4.5). Die Zeilenformen sind real verschieden;
  eine gemeinsame Tabelle würde Code durch Konfiguration ersetzen und
  Sonderfälle in Flags verwandeln. Nur der Rahmen (Filterleiste, Leerzustand,
  Fehlerbanner) gehört geteilt — und drei von vier sind es bereits.
- **Kein Zusammenlegen von Radio und Podcasts im Kern.** Stationen sind keine
  Episoden: kein Feed, keine GUID, keine Resume-Position, kein Download. Die
  Gemeinsamkeit liegt in der HTTP-Boundary und der UI-Schale, nicht im
  Datenmodell.
- **Keine Aufteilung von `reprise-core` in mehrere Crates.** 104k Zeilen sind
  viel, aber die Modulgrenzen sind sauber, die Purity ist mechanisch geprüft,
  und mehrere Crates würden vor allem Feature-Flag-Kombinatorik erzeugen.
- **Keine Umstellung auf async/tokio.** Blockierendes HTTP auf Worker-Threads
  plus `async-channel` an der GTK-Grenze ist für dieses Programm die einfachere
  und bereits bewährte Lösung. `tokio` lebt korrekt nur in `reprise-mcp`, weil
  das SDK es erzwingt.
- **Kein Absenken von Gates, um Wellen schneller zu landen.** Die
  800-Zeilen-Grenze, die Orchestrator-Deckel und die
  Frontend-Thinness-Budgets sind die Gründe, warum dieses Repo bei 287k Zeilen
  noch navigierbar ist.

---

## 12. Vorgeschlagene Gate-Ergänzungen

Jede dieser Zeilen macht einen Befund oben unwiederholbar:

1. **`msrv.sh` baut wirklich** mit der deklarierten Toolchain (§9.3).
2. **Kein `expect`/`unwrap` in `main.rs`** — ein `rg`-Verbot in
   `check-architecture.sh`, drei Zeilen (§3.2).
3. **Duplizierte UI-Konstanten** — `FILTER_BAR_MIN_HEIGHT`, `FACET_PAGE`,
   `VALUE_PAGE` dürfen genau einmal definiert sein (§4.2).
4. **Ein `ureq::Agent::config_builder()`-Budget** in `reprise-core`, gedeckelt
   auf die aktuelle Zahl und nur senkbar — nach dem Vorbild von
   `check-frontend-thinness.sh` (§4.4).
5. **Sektionsbuchstaben in `ux-rules.md` sind eindeutig** — eine Zeile in
   `check-ux-traceability.sh` (§9.6).
6. **Wenn die Runtime ausgeliefert wird, benutzt sie auch jemand**: ein Check,
   dass `reprise_runtime_client` außerhalb von Tests referenziert wird, sobald
   `data/*.service.in` installiert werden (§2.2).
7. **Jedes yt-dlp-Positionsargument steht hinter `--`** — ein `rg`-Verbot auf
   `Command::new(&self.binary)`-Pfaden ohne Separator (§7.2).
8. **`cargo deny` im Release-Gate** für Lizenzen und Duplikate, weil
   `LICENSING.md` Aussagen macht, die heute niemand mechanisch prüft (§7.6).

---

## 13. Antworten auf die gestellten Fragen, kompakt

**„Saubere Architektur, um mehrere Apps auf demselben Kern zu bauen?"**
Die Grundlage steht (Db-Handle, geprüfte Purity, kein SQL außerhalb des Kerns).
Drei Dinge fehlen: `rusqlite::Error` aus der öffentlichen API (§2.3), eine
Entscheidung über die zweite Laufzeit (§2.2), und ansichtsweise Ports statt
einer 40-Feld-Kompositionswurzel (§2.4).

**„Saubere Fehlerbehandlung und Logging zum Debuggen?"**
Fehlerbehandlung: sehr gut, mit **einer** harten Lücke — die App paniert beim
Datenbankstart (§3.2). Logging: zum Entwickeln brauchbar, für Tester unbrauchbar,
weil es nur nach stderr geht (§3.3).

**„Haben wir Dinge doppelt entwickelt — Radio, Podcast, YouTube, Playlisten?"**
YouTube und Podcasts: nein, vorbildlich geteilt. Playlisten: nein, ein Modell für
alle lokalen Quellen. Doppelt sind die *Schalen*: fünf Filterleisten (§4.2), zwei
Add-Dialoge (§4.3), dreizehn HTTP-Boundaries mit fünf Ratenbegrenzern (§4.4).
Für Letzteres existiert bereits ein zugesagter, fälliger Konsolidierungs-Task.
Die größte Doppelentwicklung liegt woanders: zwei Kommandoflächen für Wiedergabe
und Queue (§2.2).

**„Sind Playlist-Filter und Interpretenseiten-Pillen sauber zu unterscheiden?"**
Seit `c565671` ja — Ort und Filter haben getrennte Form, Position, Geste und
Zählvokabel, und die Entscheidung liegt in reinen, regelbenannten Funktionen.
Drei Restpunkte in §5.3, davon einer wichtig: der Sidebar-Refresh-Fix muss auf
`dev` sein, sonst verschwindet die Ortspille beim ersten Abspielen.

**„Performance-Optimierungen?"**
Eine sticht heraus: die Standardsortierung der Bibliothek hat keinen Index und
kostet gemessen bis zu 380 ms je Fenster bei 100k Titeln — auf dem UI-Thread.
Ein Index bringt das auf 3,4 ms (§6.1). Danach: Suche ohne FTS und
OFFSET-Paginierung, beide erst nach dieser Messung neu bewerten.

**„Wie steht es um die Sicherheit?"**
Überdurchschnittlich. Keine SQL-Injektion (Whitelist statt Interpolation), keine
Pfad-Traversierung (Downloads unter Hashwerten), Antwortgrößen gedeckelt,
XML-Bomben strukturell ausgeschlossen, Fehlermeldungen mit getesteter
Redaktionsgrenze, `unsafe` auf eine begründete Stelle beschränkt, Flatpak-Sandkasten
eng und mechanisch bewacht. Drei defensive Lücken: kein `--` vor der yt-dlp-URL
(§7.2, heute nur durch Zufall nicht ausnutzbar), Weiterleitungen ohne Zielprüfung
(§7.4, geringe Schwere), und Bilddekodierung ohne `Limits` (§7.6). Alle drei sind
klein und gehören in Welle 2, wo die gemeinsame HTTP-Boundary entsteht.

**„Wie stabil ist die App?"**
Die Panikfreiheit ist erarbeitet, nicht behauptet: praktisch keine `unwrap` im
Produktionscode, Generationstoken gegen veraltete Async-Ergebnisse, benannte
Worker-Threads, und die `Db`-Migration hat 575 Borrows der häufigsten
Panik-Klasse ersatzlos entfernt. Das Problem ist nicht die Häufigkeit von
Abstürzen, sondern ihre **Unsichtbarkeit**: 1.633 verbleibende `RefCell`-Borrows,
eine Panik im GTK-Callback ist ein Prozessabbruch, es gibt keinen `panic::set_hook`,
und die Meldung geht nach stderr, wo kein Tester sie sieht (§8.2). Ein Absturz
hinterlässt heute nichts. Das ist der wichtigste kleine Fix im ganzen Dokument.

**„Ist es abwärtskompatibel?"**
Datenmodell: ja — vorwärtsgerichtete, transaktionale Migrationen bis Schema 50,
Downgrade wird erkannt, Grandfathering existiert. Aber die Projektregel sagt
heute noch ausdrücklich „keine Abwärtskompatibilität nötig", und die muss mit der
Freigabe kippen (§9.1), sonst löscht die nächste regelkonforme „saubere
Umstellung" Testerdaten. Nicht kompatibel ist derzeit die **Build**-Seite: die
deklarierte MSRV 1.92 ist mit dem gepinnten Abhängigkeitsgraphen nicht
erreichbar (§9.3).
