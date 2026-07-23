# Klangprofil und agentenfähige Playlistplanung — Design

Status: bereit zur gemeinsamen Review

Branch: `feat/audio-character-mcp`

Basis: `35045a33` (`main`, 2026-07-19)

## 1. Ziel

Reprise analysiert Musikdateien ausschließlich lokal und bildet aus messbaren
Audiosignalen ein erklärbares **Klangprofil**. Dieses Profil liefert dem User
interessante, vorsichtig formulierte Informationen und speist einen einzigen,
deterministischen Mix-Planer. Derselbe Planer wird von der nativen GTK-Oberfläche
und später von einem lokalen MCP-Server verwendet.

Der zentrale Produktfluss lautet:

```text
Audiodatei (read-only)
    -> versionierte Audio evidence
    -> versioniertes Sound profile
    -> strukturierte Mix intent
    -> unveränderlicher Mix draft + Selection reasons
    -> explizite Draft approval
    -> manuelle Playlist
```

Die Funktion behauptet nicht, eine objektive Emotion des Titels oder des Users
zu kennen. In der ersten Stufe verwendet die UI deshalb „Audio Character" /
„Klangprofil" und messnahe Dimensionen, nicht „happy", „sad" oder eine einzelne
Mood-Kategorie.

## 2. Aktueller Reprise-Stand

Der Plan baut auf vorhandenem, geprüftem Verhalten auf:

- `reprise-core::waveform::WaveformBackend` ist bereits die plattformneutrale
  Naht für Waveform-Extraktion.
- `reprise-platform-linux::waveform::GstreamerWaveformBackend` dekodiert Audio
  heute über `gst-launch-1.0`, sammelt das vollständige 8-kHz-Monosignal im
  Speicher und berechnet normalisierte RMS-Peaks.
- Ein Startup-/Post-Scan-Backfill startet bis zu vier Waveform-Worker. Fehler
  werden nur gezählt; ein dauerhafter Zustand, Pause, Abbruch oder Backoff
  existiert nicht.
- `tracks.waveform_peaks` speichert 1.000 `u8`-Peaks direkt am Track.
- Die Smart-Playlist-Regeln sind eine validierte Feld-Whitelist, ausschließlich
  per `AND` verbunden. Sie können Bereiche ausdrücken, aber weder Alternativen
  noch Ähnlichkeitsdistanz, Diversität oder einen Spannungsbogen.
- `playlists::create_with_tracks` erzeugt eine manuelle Playlist samt Tracks
  bereits atomar. Diese Operation ist die spätere Persistenzsenke eines
  freigegebenen Mix-Entwurfs.
- Die README plant einen schmalen MCP-Adapter über Core-Verträge bereits ein:
  explizite Capabilities, standardmäßig read-only, keine Pfad- oder
  Credential-Leaks.
- `My Stats` rechnet ausschließlich aus lokalen `listen_events`. Künftige
  Klangprofil-Aussagen müssen dieselbe Ereignismenge verwenden und ihre
  Analyseabdeckung nennen.

## 3. Verbindliche Produktentscheidungen

### D1 — „Klangprofil" ist der Produktbegriff

Die deutsche UI verwendet „Klangprofil", die englische UI „Audio Character".
„Atmosphäre" darf als vorsichtige Interpretation erscheinen, aber nicht als
gespeicherte Wahrheit. „Mood" ist weder Tabellen- noch Haupt-UI-Begriff.

### D2 — Kontinuierliche Dimensionen statt exklusiver Labels

Stufe 1A stellt genau diese normalisierten Dimensionen (`0.0..=1.0`) bereit:

- **Intensity** — ruhig bis intensiv;
- **Brightness** — dunkel bis hell;
- **Dynamicity** — gleichmäßig/komprimiert bis stark dynamisch;
- **Rhythmicity** — fließend bis stark puls-/onset-geprägt.

Zusätzlich stehen messnahe Werte zur Verfügung:

- Tempo in BPM plus Tempo-Konfidenz;
- Lautheit beziehungsweise Energieverteilung;
- Dynamikspanne;
- spektraler Schwerpunkt und Roll-off;
- spektraler Fluss und Onset-Rate.

Valenz, Fröhlichkeit, Traurigkeit, Aggressivität, Akustikanteil,
Instrumentalität und freie Atmosphärenwörter gehören nicht zu Stufe 1A. Sie
benötigen ein separat lizenziertes und evaluiertes semantisches Modell.

### D3 — Messung und Projektion sind getrennt versioniert

Audio evidence bewahrt die stabilen Messwerte. Das Sound profile ist eine
deterministische Projektion daraus. Ändert sich nur die Normalisierung oder
Gewichtung, kann Reprise das Profil ohne erneutes Dekodieren berechnen. Ändert
sich der Extraktor, wird neu dekodiert.

Jedes Ergebnis nennt mindestens:

- `extractor_version`;
- `profile_version`;
- Quellidentität aus Track-ID, `file_mtime` und `file_size`;
- `analyzed_at`;
- Konfidenz beziehungsweise Verfügbarkeit je unsicherem Messwert.

Ein Pfadwechsel allein invalidiert die Analyse nicht. Eine veränderte
Dateigröße, MTime oder Extraktorversion tut es. Fehlende Tracks behalten ihren
Cache, erscheinen aber nie als Mix-Kandidaten.

### D4 — Lokal, read-only und ohne Modell-Download

Stufe 1A liest Audiodateien, schreibt sie nie und benötigt kein Netz. Sie lädt
kein Modell nach und übermittelt weder Audio noch Merkmale. Die Funktion ist
eine lokale Library-Einstellung, kein Eintrag der für externe Integrationen
reservierten Plugins-Seite.

### D5 — Analyse ist explizit aktiviert und beherrschbar

Eine neue Installation analysiert nicht ungefragt die gesamte Bibliothek. Die
Einstellung „Analyze audio locally" aktiviert die Funktion. Danach gilt:

- genau ein Analyse-Worker standardmäßig;
- Fortschritt mit fertig/gesamt/fehlgeschlagen;
- Pause, Fortsetzen und Abbrechen;
- Fortsetzung nach Neustart;
- bestehende Wiedergabe, Scan und Android-Transcoding bleiben reaktionsfähig;
- keine Analyse innerhalb der atomaren Scan-Transaktion;
- Fehler werden typisiert persistiert und nicht bei jedem Start endlos neu
  versucht;
- eine bewusste Aktion „Retry failed" setzt den Backoff zurück;
- Deaktivieren stoppt neue Arbeit, löscht aber vorhandene Profile nicht.

Der Schalter betrifft ausschließlich Audio-Evidenz und Klangprofil. Die schon
ausgelieferte Waveform bleibt eine bedingungslose Playerfunktion: Fehlende Peaks
werden auch bei ausgeschaltetem Klangprofil weiter erzeugt. Ist das Klangprofil
aktiv und fehlen beide Ergebnisse, koordiniert der Worker einen gemeinsamen
Decode-Durchlauf statt zwei konkurrierende Backfills zu starten.

### D6 — Streaming statt vollständigem PCM im Speicher

Der Linux-Adapter dekodiert PCM in begrenzten Blöcken über eine native
GStreamer-Pipeline. Ein vollständiger Titel darf nicht als `Vec<i16>` im
Speicher liegen. Die bestehenden Waveform-Peaks und die neue Audio evidence
werden im Hintergrund in einem Decode-Durchlauf erzeugt.

Die vorhandene On-Demand-Waveform-Funktion bleibt als eigenständige Fähigkeit
erhalten. Der Linux-Adapter darf intern denselben begrenzten Decoder verwenden;
ein Fehler des Klangprofils darf die Waveform der Playerbar nicht verhindern.
Ein deaktiviertes Klangprofil darf die Waveform ebenfalls nicht verhindern.

### D7 — Ein tiefer Core-Planer gehört UI und Agenten gemeinsam

Die externe Naht des neuen Core-Moduls besteht aus wenigen Operationen:

```rust
plan_mix(conn, intent) -> MixDraft
approve_mix_draft(conn, draft_id, playlist_name) -> PlaylistCommit
```

SQL, Skalennormalisierung, Kandidatenauswahl, Distanz, Diversität,
Dauerfüllung, Draft-Persistenz und Konfliktprüfung bleiben Implementation des
Moduls. GTK und MCP dürfen diese Regeln nicht nachbauen.

### D8 — Der Agent übersetzt Sprache, Reprise entscheidet Musik

Reprise enthält in den Stufen 1A, 1B und 2 kein LLM und parst keine natürliche
Sprache.
Ein Agent übersetzt beispielsweise „ruhige, dunkle Musik für eine nächtliche
Zugfahrt" in eine strukturierte Mix intent. Reprise validiert diese Absicht
und liefert eine deterministische Auswahl.

Das bewahrt Testbarkeit und macht dasselbe Ergebnis aus GTK-Slidern, einem
MCP-Client oder einer künftigen nativen Plattform möglich.

### D9 — Harte Bedingungen und weiche Wünsche bleiben verschieden

Eine Mix intent enthält:

**Harte Bedingungen**

- optionale Quellmenge (gesamte Bibliothek, Playlist, Artist, Album oder
  explizite Trackmenge);
- gewünschte maximale Anzahl beziehungsweise Zieldauer;
- erforderliche aktuelle Analyse;
- minimale Analysekonfidenz;
- ausgeschlossene Track-, Artist- oder Album-Identitäten;
- nur vorhandene, nicht entfernte Tracks.

**Weiche Wünsche**

- Ziel und Gewicht je Klangprofil-Dimension;
- Tempoziel und Gewicht;
- Familiarity (`familiar`, `balanced`, `discover`);
- Variety (`cohesive`, `balanced`, `wide`);
- optionaler Energieverlauf (`flat`, `rise`, `fall`, `arc`).

Unbekannte Felder, Werte außerhalb ihrer Bereiche und widersprüchliche harte
Bedingungen sind Fehler. Sie werden nie still normalisiert.

### D10 — Der Mix-Entwurf ist erklärbar, aber enthält keine Chain of Thought

Ein Mix draft enthält:

- eine stabile `draft_id`;
- die normalisierte Mix intent;
- Quellsnapshot, Extraktor- und Profilversion;
- geordnete Track-IDs mit anzeigbaren Metadaten, nie Dateipfade;
- Gesamtanzahl und Gesamtdauer;
- Analyseabdeckung der betrachteten Population;
- strukturierte Selection reasons pro Track;
- Warnungen wie „only 43 of 80 eligible tracks are analyzed";
- Ablaufzeit und Status `current` oder `stale`.

Selection reasons sind kurze Daten wie `brightness_match`, `tempo_match`,
`artist_gap` oder `duration_fit`, keine freien internen Gedankengänge.

### D11 — Auswahl ist deterministisch und divers

Bei identischem Kandidaten-/Quellsnapshot, identischer Mix intent und
identischem Seed entsteht dieselbe Reihenfolge. Der Planer:

1. wendet harte Bedingungen in SQL an;
2. berechnet die gewichtete Distanz zum Zielprofil;
3. sortiert stabil nach Distanz, dann Track-ID;
4. wählt greedily mit einer Diversitätsstrafe;
5. verhindert Track-Duplikate;
6. hält standardmäßig mindestens vier Positionen Abstand zwischen demselben
   Artist, sofern die Kandidatenmenge das erlaubt;
7. füllt die Zieldauer bis zur kleineren Abweichung; eine Überschreitung ist
   höchstens die Dauer eines letzten Tracks;
8. wendet den gewünschten Energieverlauf erst auf die ausgewählte Menge an,
   ohne die Mitgliedschaft nachträglich zu verändern.

Kann eine harte Bedingung nicht erfüllt werden, entsteht kein Draft. Sind nur
weiche Wünsche oder die Dauer nicht vollständig erfüllbar, entsteht ein
partieller Draft mit maschinenlesbarer Warnung.

### D12 — Unanalysierte Tracks werden nicht erfunden

Ein Klangprofil-Mix enthält nur Tracks mit aktueller Analyse. Reprise füllt
keine Lücke still mit Genre, Rating oder Zufallstiteln. Der User oder Agent
kann die Analyse starten oder eine explizit andere, metadatenbasierte Auswahl
anfordern; das ist eine andere Mix intent.

### D13 — Entwurf und Persistenz sind getrennte Vorgänge

`plan_mix` verändert keine Playlist, Queue, Wiedergabe oder Datei. Eine
Draft approval erzeugt genau eine neue manuelle Playlist atomar. Sie verändert
keine bestehende Playlist.

Approval ist idempotent: derselbe Draft plus derselbe Idempotency-Key liefert
dasselbe Ergebnis. Ein abgelaufener, bereits freigegebener oder in seinen
ausgewählten Tracks beziehungsweise harten Bedingungen veralteter Draft wird
nicht still neu geplant. Eine neue Analyse oder Metadatenänderung an einem
unbeteiligten Track macht ihn ausdrücklich nicht stale. Der Aufrufer muss nur
bei einer für genau diesen Draft relevanten Änderung neu planen.

### D14 — GTK zeigt immer den Draft vor dem Speichern

Der native Mix-Builder bietet Profilziele, Dauer, Variety, Familiarity und
Energieverlauf an. Vor „Save as Playlist" zeigt er Trackfolge, Dauer,
Analyseabdeckung und Warnungen. Das Speichern verwendet ausschließlich die
gezeigte `draft_id`; es führt keine zweite unsichtbare Planung aus.

### D15 — My Stats nennt Population und Coverage

Klangprofil-Statistiken verwenden `listen_events` derselben Periode wie der
restliche Screen. Ein Insight erscheint nur bei mindestens 20 analysierten
Plays und mindestens 70 Prozent Analyseabdeckung der Plays mit noch
vorhandenem Track. Jeder Insight nennt „based on N analyzed plays". Unterhalb
der Schwellen erscheint keine scheinpräzise Aussage.

### D16 — MCP ist ein eigener, schmaler Adapter

Stufe 2 ergänzt `crates/reprise-mcp` als lokales Binary. Es hängt von
`reprise-core`, nicht von GTK oder `reprise-platform-linux`, ab. Der Adapter
öffnet die lokale Datenbank über den normalen Core-Pfad und übersetzt
MCP-Schemas in Core-Typen. Musiklogik, SQL-Fragmente und Playlist-Transaktionen
gehören nicht in den Adapter.

Initial wird nur lokales `stdio` unterstützt. HTTP, Remote-Zugriff, OAuth,
Server-Sent Events und ein dauerhaft lauschender Socket sind außerhalb von
Stufe 2. Damit verlässt die Bibliothek den Host nicht, und der Server braucht
keine Netzwerk-Credentials.

Die Implementierung zielt auf die bei Baubeginn aktuelle stabile MCP-Revision.
Stand der Designentscheidung ist `2025-11-25`; die angekündigte
`2026-07-28`-Revision ist am 2026-07-19 noch Release Candidate und deshalb nur
ein Kompatibilitäts-Watchpoint. Der offizielle Rust-SDK ist Tier 2; seine
Version wird gepinnt und mit Schema-/Protokoll-Fixtures zusätzlich abgesichert.

### D17 — MCP-Primitive bleiben klein

Stufe 2 exponiert:

**Resources**

- `reprise://library/summary` — Track-/Artist-/Albumzahl, Gesamtdauer und
  Analyseabdeckung;
- `reprise://audio-character/vocabulary` — Dimensionen, Wertebereiche und
  Semantik der Mix intent;
- `reprise://playlists` — Namen, IDs und Trackzahlen ohne Pfade.

**Read-only tools**

- `music_search_tracks` — begrenzte, paginierte Metadatensuche;
- `music_get_sound_profiles` — Profile für höchstens 100 explizite Track-IDs;
- `music_get_mix_draft` — liest einen vorhandenen Draft erneut.

**Planning tool**

- `music_plan_playlist` — erzeugt und persistiert einen Mix draft, aber keine
  Playlist, Queue-, Wiedergabe- oder Dateiveränderung. Die begrenzte
  Draft-Persistenz wird im Toolvertrag ausdrücklich als Seiteneffekt genannt.

**Write tool**

- `music_create_playlist_from_draft` — atomare Approval eines Drafts.

Es gibt in Stufe 2 kein beliebiges SQL, keine Dateipfade, Lyrics,
Credential-Werte, Tagschreiboperation, Library-Löschung, Papierkorbaktion,
Queue-Mutation oder Playback-Steuerung. Prompts werden zunächst nicht
exponiert; Agenten können die JSON-Schemas unmittelbar verwenden.

> **Ergänzung (2026-07-21, multi-frontend-core-Plan):** Direktes
> `music_create_playlist` (Name plus explizite Track-IDs) ist jetzt unter
> derselben Capability `playlist:create` erlaubt; der hier beschriebene
> Draft-Weg (`music_create_playlist_from_draft`) bleibt ein später
> koexistierender Pfad. Überschreiben und Löschen via Agent bleiben
> ausgeschlossen.

### D18 — Capabilities sind fail-closed

Der MCP-Server startet standardmäßig mit `library:read` und `mix:plan`.
`playlist:create` ist aus. Der User aktiviert sie explizit in Reprise und
startet den MCP-Prozess anschließend neu; eine Umgebungsvariable darf die
gespeicherte Ablehnung nicht überstimmen.

Jedes Tool prüft seine Capability im Core-nahen Adapter vor dem Datenzugriff.
Ein unbekannter Capability-Wert bedeutet „verweigert". Die Write-Capability
erlaubt ausschließlich das Erzeugen einer neuen manuellen Playlist aus einem
gültigen Draft. Sie erlaubt weder Überschreiben noch Löschen.

### D19 — Keine Pfad- oder Verlaufslecks

MCP-Ergebnisse enthalten opaque Track-IDs und die Metadaten Titel, Artist,
Album, Dauer, Jahr, Genre, Rating und Klangprofil. Sie enthalten niemals:

- Audio- oder Cover-Dateipfade;
- XDG-, Cache- oder Datenbankpfade;
- Lyrics;
- Geräte-Seriennummern oder MTP-Pfade;
- Credentials, Tokens oder Settings-Werte;
- rohe Hörereignisse oder genaue Hörzeitpunkte.

Ein späterer Zugriff auf aggregierte Hörpräferenzen benötigt eine eigene,
standardmäßig deaktivierte Capability. Stufe 2 bietet ihn nicht.

### D20 — MCP bleibt stateless, Drafts sind langlebige Core-Daten

Mix drafts werden mit begrenzter Lebensdauer in SQLite gespeichert, nicht in
einer MCP-Session oder im Prozessspeicher. Damit überlebt ein Draft einen
Client-Neustart und der Transport kann stateless bleiben. Abgelaufene Drafts
werden beim Zugriff beziehungsweise in einer begrenzten Wartungsoperation
bereinigt; keine ungebundene Startup-Schleife läuft über die gesamte Tabelle.

## 4. Architektur

```text
reprise-gnome                         reprise-mcp (stdio)
  Mix Builder                            MCP resources/tools
       |                                      |
       +--------------+-----------------------+
                      |
          reprise-core::sound_profile
          reprise-core::mix_planner
            - storage and staleness
            - projection and coverage
            - candidate queries
            - deterministic planning
            - durable drafts
            - atomic approval
                      |
                 SQLite DB

reprise-gnome background runtime
                      |
       core AudioAnalysisBackend seam
                      |
reprise-platform-linux GStreamer adapter
  bounded PCM decode + evidence extraction
```

### Modulgrenzen

`sound_profile` ist ein tiefes Core-Modul. Seine öffentliche Fläche umfasst
Profile, Coverage, Pending-Work und persistierte Ergebnisse; Tabellenform,
SQL und Projektionsgewichte bleiben intern.

`audio_analysis` definiert den plattformneutralen Analysevertrag und die
versionierten Ergebniswerte. Der Linux-Adapter ist die Produktionsumsetzung;
Tests verwenden einen deterministischen Fake-Adapter.

`mix_planner` besitzt Mix intent, Mix draft, Selection reasons und Approval.
Es verwendet `sound_profile` und bestehende Playlist-Funktionen intern. Weder
GTK noch MCP sehen seine SQL-Implementation.

`reprise-mcp` ist absichtlich flach: Transport-, Schema- und Capability-
Adapter. Würde man das Crate löschen, bleibt jede Auswahl- und
Persistenzfunktion in Core vollständig nutzbar.

## 5. Persistenzkonzept

Die konkrete Schema-Version wird bei Beginn der Implementierung aus dem dann
aktuellen `main` bestimmt; der heutige Stand ist v17, aber parallel laufende
Branches dürfen die nächste Nummer belegen.

Logisch werden benötigt:

### Track analysis

Eine Zeile pro Track mit:

- Source-Fingerprint (`file_mtime`, `file_size`);
- Extractor-/Profilversion;
- roher Audio evidence;
- normalisiertem Sound profile;
- Konfidenzen;
- Status `ready | failed` plus typisiertem Fehler;
- Analysezeit und Retry-Zustand.

`pending` wird nicht millionenfach materialisiert: Ein vorhandener Track ohne
aktuelle Zeile ist pending. Track-Löschung kaskadiert. Missing/removed bleiben
aus Pending- und Mix-Abfragen ausgeschlossen.

### Mix drafts

Eine Draft-Kopfzeile enthält Intent-JSON in kanonischer Form, Quellsnapshot,
Profilversion, Seed, Ablaufzeit, Status und Diagnostics.
Positionszeilen speichern Track-ID, Position, Score und Selection reasons.

Der Quellsnapshot speichert die Identität/Fingerprints der ausgewählten Tracks
und die harten Quellbedingungen. Approval prüft genau diese Auswahl erneut:
Tracks müssen noch PRESENT sein, ihre Analysefingerprints müssen passen und sie
müssen weiterhin der verlangten Quellmenge angehören. Neue oder geänderte
unbeteiligte Tracks machen den Draft nicht stale. Approval und
Playlist-Erzeugung laufen in einer Transaktion.

## 6. Analysequalität und Kalibrierung

Stufe 1A benötigt einen reproduzierbaren Fixture-Korpus im Repository:

- Stille und sehr leises Signal;
- Sinus in tiefen und hohen Frequenzen;
- Impuls-/Click-Track mit bekannten BPM;
- dynamisch an- und abschwellendes Signal;
- gleichmäßig komprimiertes Signal;
- verrauschtes beziehungsweise breitbandiges Signal;
- kurze reale, redistributable Musikfixtures für Container-/Codec-Integration.

Synthetische Fixtures prüfen exakte mathematische Eigenschaften. Reale
Fixtures prüfen nur robuste Bereiche und Ordnungen, nie subjektive
Atmosphäre. Der Fixture-Ursprung und seine Lizenz werden neben der Datei
dokumentiert.

Release-Benchmarks messen:

- Peak RSS beziehungsweise den nachweisbaren PCM-Pufferdeckel;
- Decode-/Analysezeit pro Audiominute;
- Datenbankgröße pro 10.000 Tracks;
- Pending-Abfrage bei 100.000 Tracks;
- Mixplanung bei 1.000, 10.000 und 100.000 Profilzeilen;
- Ergebnisdeterminismus unabhängig von SQLite-Plan und Thread-Reihenfolge.

Harte Verträge:

- PCM-Speicher ist durch feste Chunk-/FFT-Fenster begrenzt, nicht durch
  Trackdauer;
- Standardparallelität ist 1;
- Mixplanung lädt keine vollständigen PCM-, Waveform- oder Embedding-BLOBs;
- maximal 500 Kandidaten gelangen aus der SQL-Vorauswahl in die greedy
  Diversitätsphase;
- MCP-Antworten sind paginiert beziehungsweise hart begrenzt.

## 7. Fehler- und Randfälle

- **Track ändert sich während der Analyse:** Ergebnis nur speichern, wenn der
  Source-Fingerprint noch übereinstimmt; sonst verwerfen und erneut pending.
- **Track wird missing:** laufende Analyse darf scheitern; kein Profil wird als
  Kandidat ausgegeben.
- **App wird beendet:** der Worker hält keinen GTK-Borrow; Abbruch beendet den
  aktuellen Chunk-/Trackpfad sauber und lässt übrige Tracks pending.
- **Tempo nicht bestimmbar:** BPM bleibt `None`, die übrigen Dimensionen sind
  gültig; kein erfundener Nullwert.
- **Stille:** Intensity und Rhythmicity werden definiert niedrig, Konfidenz
  nennt die begrenzte Aussage; Division durch null ist ausgeschlossen.
- **Zu wenige Kandidaten:** partieller Draft mit Diagnostic, sofern alle harten
  Bedingungen erfüllt sind.
- **Draft stale:** Approval lehnt ab, wenn ausgewählte Tracks oder harte
  Quellbedingungen nicht mehr gelten; kein stilles Replan. Änderungen an
  unbeteiligten Tracks bleiben folgenlos.
- **Playlistname existiert:** Reprise bewahrt die heutige Semantik manueller
  Playlists und darf eine zweite Playlist gleichen Namens erzeugen. Idempotency
  verhindert nur den doppelten Commit desselben Drafts; eine bestehende
  Playlist wird niemals überschrieben.
- **MCP-Client fordert 100.000 IDs:** Schema-/Runtime-Limit lehnt vor SQL ab.
- **Manipuliertes Intent-/Draft-JSON:** typisierte Validierung, kein dynamischer
  SQL-Identifier und keine freie WHERE-Klausel.
- **Capability ändert sich während Prozesslauf:** die gespeicherte Einstellung
  wird pro Write-Aufruf neu gelesen; Entzug wirkt ohne Serverneustart. Neue
  Freigaben werden erst nach Neustart sichtbar, damit kein Client überraschend
  zusätzliche Tools erhält.

## 8. UX-Richtung für die Stufen 1A und 1B

### Settings

Unter Library erscheint „Audio Analysis":

- Toggle „Analyze audio locally";
- Erklärung „Reads your music locally. Nothing is uploaded.";
- Coverage „1,204 of 1,686 tracks analyzed";
- Progress/Fehlerzahl;
- Pause/Resume;
- „Retry failed";
- „Reanalyze library" nur mit Bestätigung, weil es rechenintensiv ist, aber
  keine Userdaten löscht.

### Now-Playing-Panel

Das bestehende rechte Now-Playing-Panel erhält neben „Up Next" und „Lyrics"
einen dritten Tab „Audio Character" für den aktuell geladenen Track. Er zeigt
vier beschriftete Skalen, BPM und eine kurze Zeile „Analyzed locally". Bei
fehlender Analyse zeigt er „Not analyzed" plus die Aktivierungs-/Analyseaktion.
Farbe ist nie der einzige Informationsträger. Eine allgemeine Detailfläche für
beliebige ausgewählte Library-Tracks ist bewusst nicht Teil von Stufe 1A.

### Mix Builder

Der Builder bietet Presets als Startwerte, aber speichert die strukturierte
Absicht:

- „Calm & dark";
- „Bright & energetic";
- „Dynamic focus";
- „Steady pulse".

Slider und Auswahlfelder bleiben nach Presetwahl editierbar. „Preview" erzeugt
einen Mix draft. „Save as Playlist" bleibt bis zu einem gültigen, aktuellen
Draft deaktiviert.

### My Stats

Die erste Stats-Erweiterung ist klein: eine Klangprofil-Zusammenfassung der
gehörten, analysierten Plays plus ein Deep-Link zum Mix Builder. Keine neue
große Chart-Familie und kein semantisches „your mood".

## 9. MCP-Sicherheit und Protokollgrenze

MCP trennt Resources, Prompts und Tools; Tools sind modellgesteuert. Der
lokale Server exponiert deshalb nur klar annotierte, eng validierte Tools. Der
Client sollte den User bei Writes einbeziehen, Reprise verlässt sich aber nicht
allein darauf: `playlist:create` bleibt serverseitig fail-closed.

Tool-Resultate verwenden strukturierte Ausgabe und zusätzlich eine kurze
Textzusammenfassung für weniger vollständige Clients. Read-only-,
destructive- und idempotent-Hints werden korrekt gesetzt, aber nie als
Sicherheitskontrolle missverstanden.

Der stdio-Prozess schreibt Protokollausgaben ausschließlich nach `stderr`;
`stdout` bleibt MCP. Logs enthalten Toolname, Ergebnisstatus, Dauer und
anonymisierte Zählwerte, nie Trackmetadaten, Pfade, Intents oder Credentials.

## 10. Lizenz- und Modellgrenze

Die MIT-Lizenz von `reprise-core` und `reprise-platform-linux` muss erhalten
bleiben. Stufe 1A verwendet deshalb keine Essentia-Library und verteilt keine
Essentia-Modelle. Essentia eignet sich als nicht eingebundener
Forschungsvergleich, ist selbst AGPLv3; bereitgestellte Modelle tragen je nach
Generation zusätzliche CC-BY-NC-SA/ND-Bedingungen beziehungsweise eine
proprietäre Lizenzoption.

Eine spätere semantische Stufe beginnt nur mit einem eigenen
Lizenz-/Qualitäts-Gate:

- kommerzielle Nutzung und Weitergabe mit MIT-Core/Proprietär-Frontends
  vereinbar;
- Trainingsdaten- und Modellherkunft dokumentiert;
- lokaler CPU-Pfad ohne verpflichtende Cloud;
- Genre-/Kultur-Bias auf einem festgelegten Korpus gemessen;
- Modellversion und Konfidenz im Ergebnis;
- Userkorrekturen getrennt vom Modellergebnis gespeichert.

Ohne bestandenen Gate bleibt Reprise beim erklärbaren Klangprofil.

## 11. Stufengrenzen

### Stufe 1A — Klangprofil-Grundlage

Enthält Analysevertrag, Linux-Adapter, Persistenz, Worker, Settings und den
Audio-Character-Tab des Now-Playing-Panels. Sie ist der nächste ausführbare
Planabschnitt und bereits allein ein vollständiger Usernutzen.

### Stufe 1B — Nativer Mix-Planer

Enthält gemeinsamen Mix-Planer, Draft/Approval, Mix-Builder und eine kleine
My-Stats-Projektion. Sie beginnt erst nach expliziter Freigabe nach der
Stufe-1A-Review.

### Stufe 2 — Lokaler MCP-Adapter

Enthält das separate stdio-Binary, Resources, read-only Tools, Mix-Planung und
capability-geschützte Playlist-Erzeugung. Sie beginnt erst nach expliziter
Freigabe nach der Stufe-1B-Review.

### Stufe 3 — Semantische Atmosphäre und Ähnlichkeit

Optional: lizenziertes Modell für Valenz/Arousal oder Audio-Text-Embeddings,
Userkorrekturen, „similar to this" und freie Atmosphären. Kein Teil des
aktuellen Implementierungsumfangs.

## 12. Primärquellen

- MCP stable specification `2025-11-25`:
  <https://modelcontextprotocol.io/specification/2025-11-25>
- MCP server primitives:
  <https://modelcontextprotocol.io/specification/2025-06-18/server/index>
- Official Rust SDK (Tier 2):
  <https://github.com/modelcontextprotocol/rust-sdk>
- MCP `2026-07-28` release candidate:
  <https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/>
- Essentia licensing:
  <https://essentia.upf.edu/licensing_information.html>
- Essentia model inventory (research comparison only):
  <https://essentia.upf.edu/models.html>
- Audio/lyrics valence-arousal comparison:
  <https://arxiv.org/abs/1809.07276>
- CLAP audio-text representation (deferred research direction):
  <https://arxiv.org/abs/2206.04769>
