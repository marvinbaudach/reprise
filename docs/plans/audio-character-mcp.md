---
slug: audio-character-mcp
worktree: /tmp/reprise-audio-character-mcp
branch: feat/audio-character-mcp
base: 35045a33
phase: ready-for-review
created: 2026-07-19
---

# Klangprofil und agentenfähige Playlistplanung — Implementierungsplan

Dieser Plan setzt
[`docs/superpowers/specs/2026-07-19-audio-character-mcp-design.md`](../superpowers/specs/2026-07-19-audio-character-mcp-design.md)
um. Die Spezifikation ist bindend. Bei Widerspruch gewinnt anschließend
`docs/ux-rules.md`, dann die Spezifikation, dann dieser Plan.

Die Stufen sind absichtlich getrennt:

- **Stufe 1A** ist der nächste ausführbare Abschnitt und endet mit einer
  gemeinsamen Review.
- **Stufe 1B** (nativer Mix-Planer) beginnt nicht automatisch. Sie benötigt
  nach Stufe 1A eine explizite User-Anweisung.
- **Stufe 2** (MCP) beginnt nicht automatisch. Sie benötigt nach Stufe 1B eine
  explizite User-Anweisung.
- **Stufe 3** (semantische Atmosphäre/Embeddings) ist kein geplanter
  Implementierungsabschnitt, sondern ein späteres Forschungsziel.

Kein Task greift auf reale Musik, reale Reprise-Datenbank, Live-Desktop,
Accounts oder Netzservices zu. Audiointegration verwendet ausschließlich
versionierte redistributable Fixtures.

## Ausführungsregeln

Für jeden Task gilt strikt:

1. Plan, Spezifikation, betroffene UX-Regeln und aktuellen Git-Stand lesen.
2. Failing Test schreiben und den erwarteten roten Lauf beobachten.
3. Kleinste korrekte Implementation schreiben.
4. Fokustests grün ausführen.
5. Alle Pflicht-Gates aus `AGENTS.md` ausführen.
6. Core-Purity nach jeder Core-Änderung beweisen.
7. Bearbeitete/erzeugte Code-Dateien unter 800 Zeilen halten.
8. Diff adversarial gegen Spezifikation und Task prüfen; Findings beheben.
9. Exakt mit der angegebenen Message committen.
10. `.superpowers/sdd/progress.md` mit Commit und Basis aktualisieren.
11. Ohne Reviewpause zum nächsten Task derselben Stufe weitergehen.

Die Schema-Versionsnummer wird zu Beginn von Task 2 aus dem dann aktuellen
`main` ermittelt. Der Plan nennt bewusst nicht „v18", weil parallele Branches
die Nummer belegen können.

## Stufe 1A — Klangprofil-Grundlage

### Task 1 — UX-Vertrag und Test-/Lizenz-Gates

**Commit:** `docs(ux-rules): define local audio character analysis`

**Ziel:** Die neue sichtbare und rechenintensive Funktion ist normativ
beschrieben, bevor Produktionscode existiert.

**Änderungen:**

- `docs/ux-rules.md`: neue append-only Sektion mit `[geplant]`-Regeln für
  lokale/opt-in Analyse, vier Dimensionen, Coverage/Unsicherheit,
  Background-Steuerung und den Now-Playing-Klangprofil-Tab.
- Regel-IDs aus dem dann aktuellen Dokument ableiten, nicht vorab erfinden.
- `scripts/check-ux-traceability.sh`: keine Logikänderung außer ein neuer
  Präfix würde wider Erwarten nicht automatisch erkannt; dafür zuerst eine
  rote Negativprobe.
- `LICENSING.md`: festhalten, dass Audioanalyse im MIT-Enginepfad keine
  AGPL-/Non-Commercial-Modelle einbinden darf.
- `TESTING.md`: Fixture-Herkunft und Audioanalyse-Benchmark als künftige Gates
  dokumentieren.

**Roter Beleg:** Negativfixture mit einer fälschlich `[aktiv]` gesetzten neuen
Regel ohne regelbenannten Test muss am Traceability-Gate scheitern.

**Abnahme:** Regeln bleiben `[geplant]`; kein Produktcode oder UI-Schalter wird
vorgetäuscht.

### Task 2 — Versionierte Persistenz und Staleness

**Commit:** `feat(core): persist versioned audio character analysis`

**Ziel:** Core kann Audio evidence und Sound profile ohne Decoder speichern,
invalidieren, laden und als Coverage/Pending-Work projizieren.

**Failing Tests:**

- Migration von der live ermittelten Vorgängerversion bewahrt Tracks,
  Waveforms, Playlists und `listen_events`.
- Frische DB und Upgrade-DB besitzen dasselbe Analyse-Schema.
- `save_analysis` round-tripped Evidence, Profil, Versionen, Konfidenzen und
  Fingerprint.
- gleiche MTime/Größe/Version ist aktuell; jede relevante Abweichung ist
  pending.
- Pfad-/Inode-Wechsel bei gleicher MTime/Größe invalidiert nicht.
- missing/removed werden weder pending noch Mix-eligible.
- Track-Löschung kaskadiert die Analyse.
- Coverage zählt Nenner und aktuelle Profile korrekt und unterscheidet
  Library-Tracks von Listen-Events.
- Failure-Kind und Retry-Zustand round-trippen; unknown Kind fällt sicher auf
  `unknown` zurück.

**Implementation:**

- neue fokussierte Core-Module `audio_analysis` und `sound_profile`;
- Datenwerte als endliche Zahlen validieren; NaN/Inf nie persistieren;
- `0.0..=1.0` per Konstruktor erzwingen;
- Pending-/Coverage-Abfragen erhalten passende Partial-/Join-Indizes;

**Adversarial Checks:** manipulierte DB-Werte, negative MTime/Größe,
unbekannte Version, leere Bibliothek, ausschließlich missing Tracks.

### Task 3 — Streaming-Akkumulator und reproduzierbare Fixtures

**Commit:** `feat(core): extract deterministic audio character evidence`

**Ziel:** Pure Core-Mathematik verarbeitet begrenzte PCM-Chunks und erzeugt
Waveform-Rohwerte, Evidence und Profile ohne Plattform- oder Dateizugriff.

**Failing Tests:**

- Chunkgrenzen verändern das Resultat nicht innerhalb definierter Toleranzen.
- Stille, konstantes Signal, tiefer/hoher Sinus, Click-Track, Crescendo und
  Breitbandrauschen erzeugen erwartete Ordnungen.
- 60/90/120/180-BPM-Click-Tracks landen im Toleranzfenster; Half-/Double-Tempo
  wird über Konfidenz beziehungsweise kanonischen Bereich stabil behandelt.
- Null Chunks, letzter Teilchunk, extrem kurze Datei und sehr langer
  synthetischer Stream bleiben panic-/NaN-frei.
- Profile liegen immer in `0..=1` und eine reine Projektionsversionsänderung
  benötigt kein PCM.
- Waveform-Ausgabe bleibt exakt 1.000 `u8`-Peaks und kompatibel mit der
  bestehenden Playerbar.

**Implementation:**

- `AudioEvidenceAccumulator` nimmt mono PCM plus Sample-Rate blockweise an;
- feste FFT-/Hop-Größe und begrenzte Ringpuffer;
- robuste Perzentil-/Histogramm-Aggregation statt vollständiger Samplelisten;
- Projektion mit benannten Konstanten und dokumentierter Kalibrierung;
- Fixture-Generator als Code, binäre reale Fixtures nur mit Lizenznotiz.

**Gate:** deterministischer Memory-/Chunk-Test beweist, dass Speicher nicht mit
Trackdauer wächst.

### Task 4 — Nativer GStreamer-Analyseadapter

**Commit:** `feat(platform): stream audio into the character analyzer`

**Ziel:** Linux dekodiert unterstützte Formate blockweise, ohne
`gst-launch-1.0`-Subprozess oder vollständigen stdout-Puffer.

**Failing Tests:**

- FLAC-/WAV-Fixtures liefern Evidence und 1.000 Peaks.
- fehlende, leere und nicht dekodierbare Datei ergeben typisierte Fehler.
- Abbruch beendet die Pipeline begrenzt und liefert kein partielles Ready-
  Ergebnis.
- Produktionsadapter verarbeitet mehrere Chunks; ein Ein-Chunk-Fake kann den
  Test nicht versehentlich bestehen.
- bestehende Waveform-Tests bleiben grün und Playerbar-Vertrag unverändert.

**Implementation:**

- Core-`AudioAnalysisBackend` mit einem `analyze(path, cancellation)`-Aufruf;
- Linux-`GstreamerAudioAnalysisBackend` über AppSink/Callbacks und begrenzte
  PCM-Blöcke;
- interner gemeinsamer Decoderpfad für Analyse und On-Demand-Waveform, ohne
  beide öffentlichen Fähigkeiten zu koppeln;
- kein GTK-/GLib-MainContext-Zugriff im Worker.

**Benchmark:** Decode/Analysezeit und Peak-RSS für kurze und lange Fixtures;
Release-Report, kein unkalibrierter Marketingclaim.

### Task 5 — Dauerhafter, abbrechbarer Ein-Worker-Scheduler

**Commit:** `feat(gnome): run controllable local audio analysis`

**Ziel:** Aktivierte Analyse arbeitet nach Scan und beim Start fortsetzbar,
begrenzt und unabhängig von GTK-Borrows.

**Failing Tests:**

- deaktiviert startet kein **Klangprofil**-Work; der bestehende bedingungslose
  Waveform-Backfill bleibt funktionsfähig;
- aktiviert nimmt ausschließlich aktuelle Pending-Tracks;
- genau ein Track wird gleichzeitig analysiert;
- Pause stoppt vor dem nächsten Track, Resume setzt fort, Cancel beendet;
- Fingerprint-Änderung während Arbeit verwirft das Ergebnis;
- Fehlerzustand verhindert Startup-Retry-Schleife;
- „Retry failed" setzt nur Failed-Zeilen zurück;
- ein zweiter Start erzeugt keinen zweiten Worker;
- Scanabschluss signalisiert neue Arbeit, analysiert aber außerhalb der
  Scan-Transaktion;
- fehlen Waveform und Profil bei aktivierter Analyse, entsteht genau ein
  koordinierter Decode; deaktivierte Analyse erzeugt weiterhin Waveforms;
- Shutdown joint beziehungsweise cancelt ohne UI-Hang.

**Implementation:**

- Schedulerzustand in einem fokussierten Runtime-Modul, nicht in `scanner.rs`;
- eigene DB-Verbindung pro Worker;
- Generation/Cancellation-Token;
- progress coalescing zum GTK-Mainloop;
- bestehender Vier-Worker-Waveform-Backfill wird auf den gemeinsamen,
  begrenzten Pfad migriert.

### Task 6 — Settings und Analysefortschritt

**Commit:** `feat(gnome): expose local audio analysis controls`

**Ziel:** User kann lokale Analyse verstehen, aktivieren und steuern.

**Failing Tests:**

- Settings-Toggle ist fresh-install off und round-tripped.
- Aktivierung startet Analyse, Deaktivierung stoppt neue Arbeit und behält
  Profile.
- Coverage, Running, Paused, Failed und Complete haben eindeutige Zustände.
- Retry erscheint nur bei Fehlern; Reanalyze verlangt Bestätigung.
- UI-Strings nennen lokale Verarbeitung und keinen Upload.
- RefCell-Borrows überqueren keine Scheduler-/GTK-Callbacks.
- schmale Breite und Reduced Motion bewahren Bedienbarkeit.

**Implementation:** eigene Preferences-Unterseite unter Library, gettext-
Strings, shared Sidebar-Aktivität nur falls der bestehende Slot semantisch
passt; ansonsten kein ungeplanter neuer globaler Progress-Stack.

**UX-Flip:** Regeln für Opt-in, Steuerung und Progress werden mit ihren
regelbenannten Tests `[aktiv]`.

### Task 7 — Klangprofil im Now-Playing-Panel

**Commit:** `feat(gnome): show audio character in now playing`

**Ziel:** Der User sieht vier Profile, BPM/Konfidenz und Analysezustand ohne
objektive Mood-Behauptung im vorhandenen rechten Panel des geladenen Tracks.

**Failing Tests:**

- Ready zeigt vier beschriftete Skalen und optionale BPM.
- Pending/Disabled/Failed/Stale unterscheiden sich textlich.
- `None`-Tempo zeigt keinen `0 BPM`-Fake.
- Farbe ist redundant zu Label/Wert/Position.
- Screenreader-Namen enthalten Dimension und Wert.
- Trackwechsel verwendet Generation und zeigt nie Profil des vorherigen
  Tracks.
- Userpfade und interne Versionen erscheinen nicht.
- Up Next/Lyrics/Audio Character teilen NPP-11s adaptiven Switcher; der neue
  Tab bleibt bei icons-only eindeutig beschriftet und per Tastatur erreichbar.
- Ohne geladenen Track zeigt der Tab einen neutralen Leerzustand.

**Implementation:** wiederverwendbare Profilansicht, aber zunächst nur im
Now-Playing-Panel verdrahtet; kein neuer Library-Details-Dialog und keine neue
Kontextmenüaktion.

**UX-Flip:** Now-Playing-/Klangprofil-Regeln werden im selben Commit aktiv.

### Task 7A — Stufe-1A-Abnahme

**Commit:** Kein eigener Commit, sofern die Review keine Findings erzeugt.

Nach Task 7: vollständige Pflicht-Gates, Audiofixture-/Memory-/Performance-
Report, isolierte Now-Playing-/Settings-Displaytests und adversarial
Standards-/Spec-Review. Display-Socket-Blocker werden exakt als
`deferred host check` dokumentiert. Findings erhalten eigene präzise
Fix-Commits.

**STOP:** Danach gemeinsame Review. Stufe 1B beginnt nicht automatisch.

## Stufe 1B — Nativer Mix-Planer (separate Freigabe erforderlich)

### Task 8 — Mix-Vertrag und Sicherheitsgrenzen

**Commit:** `docs(ux-rules): define audio character mix planning`

**Ziel:** Mix-Preview, Determinismus, Coverage, Draft-Approval und spätere
Agenten-Capabilities sind geplant, bevor der Planer Produktionscode erhält.

**Änderungen:** neue `[geplant]`-Regeln in der bestehenden Klangprofil-Sektion;
`TESTING.md` ergänzt Mix-/MCP-Sicherheitsmatrix; Negativprobe beweist, dass ein
vorzeitiger `[aktiv]`-Flip ohne regelbenannten Test scheitert.

### Task 9 — Mix intent und begrenzte Kandidatenabfrage

**Commit:** `feat(core): validate sound-profile mix intents`

**Ziel:** Core besitzt eine kanonische, serialisierbare Mix intent und eine
sichere Kandidatenprojektion.

**Failing Tests:**

- JSON/Typ-Roundtrip ist kanonisch und stabil.
- unbekannte Felder, NaN, Werte außerhalb `0..=1`, Null-/Negativdauer,
  übergroße ID-Listen und widersprüchliche Bedingungen werden abgelehnt.
- Library/Playlist/Artist/Album/Track-ID-Quellen verwenden bestehende
  Gruppierungs-/PRESENT-Semantik.
- unanalysiert/stale/missing/removed sind ausgeschlossen.
- minimale Konfidenz und Exclusions greifen vor Scoring.
- SQL-Vorauswahl liefert höchstens 500 stabile Kandidaten und liest keine
  Waveform-/PCM-BLOBs.

**Implementation:** typisierte Enums/Validated Scalars; keine freie
Feld-/Operator-/SQL-Zeichenkette aus MCP oder GTK.

### Task 10 — Deterministische, diverse Mixplanung

**Commit:** `feat(core): plan deterministic audio-character mixes`

**Ziel:** Ein Pure-/DB-Core-Pfad erzeugt aus Kandidaten einen erklärbaren,
reproduzierbaren Mix draft.

**Failing Tests:**

- identischer Kandidaten-/Quellsnapshot, Intent und Seed ergibt
  byte-identischen Draft;
- gewichtete Profildistanz ordnet erwartbar;
- stabiler Track-ID-Tiebreak;
- keine Duplikate;
- Artist-Abstand vier, wenn erfüllbar, mit Diagnostic wenn nicht;
- Familiarity-/Variety-Modi verändern nur dokumentierte Score-Anteile;
- Duration stoppt bei der kleineren Abweichung und überschreitet höchstens um
  einen letzten Track;
- rise/fall/arc ordnet die gewählte Mitgliedschaft, ersetzt sie nicht;
- harte Unmöglichkeit ist Fehler, weiche Unterfüllung partieller Draft;
- Selection reasons nennen strukturierte Top-Beiträge ohne Freitextlogik;
- leere/kleine/große Kandidatenmengen bleiben deterministisch.

**Performance-Gate:** 100.000 Profilzeilen plus SQL-Vorauswahl und Planung
werden reproduzierbar berichtet; greedy Phase sieht maximal 500 Kandidaten.

### Task 11 — Dauerhafte Drafts und atomare Approval

**Commit:** `feat(core): approve durable mix drafts atomically`

**Ziel:** Preview und Speicherung verwenden nachweislich dieselbe Auswahl.

**Failing Tests:**

- Draft-Kopf/Positionen/Reasons round-trippen in Reihenfolge.
- Draft speichert Fingerprints der Auswahl, harte Quellbedingungen und
  Profilversion.
- stale/expired/already-approved wird abgelehnt; ein neuer oder geänderter
  unbeteiligter Track bleibt folgenlos.
- Approval revalidiert PRESENT, Fingerprint, Analyse und Quellmitgliedschaft
  nur für die ausgewählte Menge, ohne neu zu scoren.
- Approval erzeugt atomar eine manuelle Playlist mit exakt den Draft-IDs.
- FK-/Insertfehler rollt Playlist und Approval vollständig zurück.
- derselbe Idempotency-Key liefert dasselbe Playlist-Ergebnis.
- anderer Key auf approved Draft erzeugt keine zweite Playlist.
- existierender Name darf nach heutiger manueller Semantik eine zweite
  Playlist ergeben, überschreibt aber nie die vorhandene.
- begrenzte Cleanup-Abfrage löscht nur abgelaufene, nicht freigegebene Drafts.

**Implementation:** `mix_planner` kapselt vorhandenes
`playlists::create_with_tracks`; Aufrufer dürfen keine Trackliste beim Approval
mitsenden.

### Task 12 — Nativer Mix Builder mit wahrer Preview

**Commit:** `feat(gnome): build playlists from audio character drafts`

**Ziel:** GTK erstellt und speichert Mixes über dieselben Core-Interfaces wie
später MCP.

**Failing Tests:**

- Presets setzen editierbare Intent-Werte.
- Invalid/unerfüllbar zeigt präzise Fehler und erzeugt keinen Draft.
- Preview zeigt Reihenfolge, Dauer, Coverage und Diagnostics.
- Save ist ohne/current-stale Draft korrekt enabled/disabled.
- Save sendet nur `draft_id`, Name und Idempotency-Key.
- gespeicherte Playlist entspricht exakt sichtbarer Preview.
- Änderung eines Controls invalidiert den alten Draft.
- Navigation zur neuen Playlist verwendet normalen Sidebar-/History-Pfad.
- narrow layout, Tastatur, Screenreader und Reduced Motion sind abgedeckt.

**UX-Flip:** Draft-before-save und Mix-Builder-Regeln werden aktiv.

### Task 13 — Coverage-ehrliche My-Stats-Projektion und Stufe-1B-Abnahme

**Commit:** `feat(stats): summarize listened audio character`

**Ziel:** My Stats gewinnt eine kleine, ehrliche Klangprofil-Auswertung und
Stufe 1B endet vollständig geprüft.

**Failing Tests:**

- Aggregat joint ausschließlich aktuelle Profile auf `listen_events` der
  gewählten Periode.
- wiederholte Plays gewichten den gehörten Titel entsprechend den Events.
- Schwellen: mindestens 20 analysierte Plays und 70 % Coverage.
- darunter kein Insight; darüber Text plus „based on N analyzed plays".
- period-/timezone-Wechsel folgt bestehenden Stats-Verträgen.
- Deep-Link öffnet Mix Builder mit der angezeigten Profilrichtung.
- keine Paths/Modellbegriffe/objektiven Emotionen im UI.

**Finale Stufe-1B-Gates:**

- `cargo fmt --check`
- strict workspace Clippy
- workspace tests
- `cargo doc --workspace --no-deps` mit denied warnings
- UX-Traceability, Motion-, Architecture-, QA- und File-Size-Gates
- Core-Purity
- `cargo audit` nur mit bestehender erlaubter Ausnahme
- Audiofixture-/Memory-/Performance-Reports
- isolierte GTK-Displaytests; falls Sandbox-Sockets sie verhindern, exakt
  `deferred host check` dokumentieren
- adversarial Standards-/Spec-Review und Fixpass

**Commit nach Review-Fixes:** Falls Findings Produktionsänderungen benötigen,
je kohärentem Fix eigener Commit mit präziser Message; nicht in den
Stufe-1B-Abschlussledger quetschen.

**STOP:** Danach gemeinsame Review. Stufe 2 beginnt nicht automatisch.

## Stufe 2 — Lokaler MCP-Adapter (separate Freigabe erforderlich)

Vor M1 werden stabile MCP-Revision, offizieller Rust-SDK-Stand, dessen Lizenz
und Conformance-Support live neu geprüft. Planannahme ist stabile Revision
`2025-11-25`, lokales stdio und gepinnter offizieller Tier-2-Rust-SDK. Falls
`2026-07-28` dann final und im SDK vollständig unterstützt ist, wird die
Zielrevision in M1 dokumentiert aktualisiert; die Tool-Domäne ändert sich
nicht.

### M1 — Separates stdio-Crate und read-only Resources

**Commit:** `feat(mcp): expose local read-only library resources`

**Failing Tests:** Workspace-/Lizenzgrenze, JSON-RPC-Handshake,
`resources/list/read`, Pagination, stdout-Protokollreinheit, keine Pfade oder
Settings-Leaks, unbekannte URI und DB-Fehler.

**Implementation:** `crates/reprise-mcp`, Abhängigkeit nur auf Core +
gepinnten offiziellen SDK, stdio, `stderr`-Logging, Library Summary,
Klangprofil-Vokabular und Playlistübersicht.

### M2 — Begrenzte read-only Tools

**Commit:** `feat(mcp): add bounded audio character query tools`

**Failing Tests:** Tool-Discovery und strukturierte Outputs für
`music_search_tracks` und `music_get_sound_profiles`; Limits 100 IDs,
Pagination, validierte Sort-/Filterwerte, PRESENT-Semantik und vollständige
Leak-Negativmatrix.

### M3 — Mixplanung über den gemeinsamen Core

**Commit:** `feat(mcp): plan explainable playlist drafts`

**Failing Tests:** `music_plan_playlist`/`music_get_mix_draft` entsprechen
direkten Core-Ergebnissen byte-/strukturidentisch; `music_get_mix_draft` ist
read-only annotiert, `music_plan_playlist` wegen der dauerhaften Draft-Zeile
bewusst nicht; invalid/stale/partial Diagnostics; keine Mutation an
Playlist/Queue/Playback; Tool-Schema enthält keine freie SQL- oder
Promptfläche.

### M4 — Capability-geschützte Playlist-Erzeugung

**Commit:** `feat(mcp): create playlists from approved mix drafts`

**Failing Tests:** Capability fresh-install off; Tool ist ohne Freigabe nicht
exponiert oder fail-closed (entsprechend stabiler Spec/SDK-Konvention);
Aktivierung, laufender Entzug, stale Draft, Idempotency, atomare Erstellung,
keine Überschreibung/Löschung und korrekte non-destructive/idempotent
Annotations.

GTK-Settings nennt exakt, welche Daten und Operationen der lokale Agentenzugriff
erhält. Keine HTTP-/OAuth-Oberfläche.

### M5 — MCP-Conformance, Packaging und Stufe-2-Abnahme

**Commit:** `test(mcp): gate local agent playlist access`

**Änderungen und Gates:**

- versionierte JSON-RPC-Fixtures und offizieller Inspector/Conformance-Pfad;
- `scripts/check-architecture.sh`: MCP darf nur Core referenzieren;
- `scripts/check-release.sh`: Binary, Lizenzhinweise und stdio-smoke;
- Security-Matrix für alle Resources/Tools und Capabilities;
- 100k-Metadaten-Response-/Pagination-Benchmark;
- README/README.de: Roadmap-Zeile erst jetzt von geplant auf shipped ändern;
- kein Agenten-/LLM-Netztest und keine reale Bibliothek.

Nach Fixpass endet Stufe 2 zur gemeinsamen Review.

## Bewusst nicht geplant

- semantische happy/sad/aggressive/relaxed-Modelle;
- Lyrics-Sentiment;
- CLAP-/Audio-Text-Embeddings;
- Cloudanalyse oder Modell-Download;
- „similar to this" über große Embedding-BLOBs;
- MCP über HTTP/OAuth;
- Playback-, Queue-, Tag-, Delete-, Trash-, Sync- oder History-Tools;
- autonome Änderung bestehender Playlists;
- Lernen aus Userfeedback.

Diese Punkte benötigen jeweils eine neue Spezifikation beziehungsweise
Stufenfreigabe und werden durch diesen Plan nicht implizit autorisiert.
