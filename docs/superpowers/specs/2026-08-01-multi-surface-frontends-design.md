# Drei Oberflächen auf einem Rust-Kern — Design

Status: bereit zur gemeinsamen Review

Branch: `feature/multi-surface-frontends`

Basis: `b232cee2f5` (`origin/dev`, 2026-08-01)

## 1. Ziel

Reprise soll neben der nativen GTK4-App auf Android, KDE, macOS und Windows
laufen — gepflegt von einem Einzelentwickler. **Android hat Priorität**, wird
auf F-Droid gratis und im Play Store bezahlt veröffentlicht (B11) und ist ein
**eigenständiger Player ohne Desktop-Voraussetzung** (B12); die
Desktop-Oberfläche folgt danach.

Die Produktpositionierung ist dabei zweistufig: die App muss als lokaler
Musikplayer allein bestehen, weil sie sonst niemanden hat, dem eine spätere
Verzahnung mit dem Desktop nützt. Die Verzahnung ist das
Differenzierungsmerkmal, nicht die Eintrittskarte.

Der Engpass ist nicht der Port, sondern die Dauerlast: **48 % aller Commits
der letzten 90 Tage fassen `reprise-gnome` an** (1030 von 2107, ohne Merges),
mit +238.863/−94.921 Zeilen Bewegung bei 140k LOC Frontend-Größe.

Der Hebel dagegen ist **nicht**, die Zahl der Oberflächen zu minimieren — das
hat sich als falsches Ziel erwiesen (siehe B1). Der Hebel ist, die **teure**
Schicht genau einmal zu haben:

- **Geteilt (Rust, ~64k LOC):** Formatierung, Filterung, Sortierung,
  Zustandsmaschinen, Navigationshistorie, Strings — als neue Crate
  `reprise-view`.
- **Dupliziert (Widget-Ebene):** GTK-Widgets (55k, bestehend),
  Compose-Composables (8–15k, neu), Web-Komponenten (15–25k, neu).

Statt 64k LOC Logik dreimal zu bauen, wird sie einmal gebaut und dreimal
konsumiert. Der neu zu schreibende Widget-Code liegt bei 25–40k LOC.

Zielbild:

```text
reprise-gnome (GPL)     reprise-android (Compose)   reprise-app (Tauri 2)
  GTK4 + libadwaita       Kotlin, rendert nur         Web-UI, rendert nur
  Vollprodukt, GNOME      Android — PRIORITÄT         KDE · macOS · Windows
  Linux                   Mobil-Zuschnitt             Desktop-Zuschnitt
        |                         |                          |
        |                    UniFFI/JNI                 Tauri-IPC
        +-------------------------+--------------------------+
                                  |
                        reprise-view (MIT)   <- NEU
           ViewModels, Formatierung, Filter, Sortierung,
           Zustandsmaschinen, Navigationshistorie, Strings
                                  |
        +-------------------------+--------------------------+
        |                                                    |
reprise-runtime-client (MIT)                        reprise-core (MIT)
  Transport abstrahiert                             Bibliothek, Queries, DB
   +------+------+
   |             |
 D-Bus      In-Process
 (Linux)    (Android, Windows, macOS)
        |
  reprise-runtime (MIT)  — heute schon transportfrei
        |
  reprise-platform-{linux, android, windows, macos}
```

## 2. Aktueller Stand — gemessene Inventur

Alle Zahlen gegen `origin/dev` `cf54c55e80`. Die Kopplungsmessung ist eine
Heuristik (Zählung von `gtk::`/`adw::`/`glib::`/`gio::`/`gdk::`/`pango::`/
`gsk::`/`graphene::`-Pfaden je Produktivdatei) und daher auf ±10 % genau,
nicht exakt.

| Crate | LOC | Portabilität |
| --- | --- | --- |
| `reprise-core` | 108.071 | dependency-pur, per `cargo tree` in `check-architecture.sh` erzwungen |
| `reprise-runtime` | 9.337 | **transportfrei** — Deps: core, protocol, rusqlite, tracing |
| `reprise-runtime-protocol` | 1.773 | Command/Snapshot über `serde` + `zvariant` |
| `reprise-runtime-client` | 1.879 | an `zbus` gebunden — die einzige Transport-Fessel |
| `reprise-platform-linux` | 11.607 | GStreamer, MPRIS, MTP, Trash, D-Bus-Aktivierung |
| `reprise-gnome` | 139.975 | 119.613 Produktiv / 20.097 Test in `src/`, 265 in `build.rs` + `examples/` |

Aufschlüsselung von `reprise-gnome` nach Toolkit-Kopplung (nur Produktivcode):

| Kopplung | Dateien | LOC | Bedeutung |
| --- | --- | --- | --- |
| keine Referenz | 199 | ~29.272 | fertige portable Logik, sitzt in der falschen Crate |
| 1–5 Referenzen | 121 | 34.690 | verhandelbare Mitte, mit Arbeit entkoppelbar |
| 6–30 Referenzen | 119 | 49.729 | echter Widget-Code |
| > 30 Referenzen | 10 | 5.922 | echter Widget-Code |

Weitere Bestandsgrößen: `docs/ux-rules.md` mit 4.057 Zeilen und ~396
Regel-IDs; ~4.100 `#[test]`-Funktionen im Workspace; ~5.427 LOC übersetzbare
Texte in `strings_*`/`*_strings`/`*_copy`-Dateien; ~1.265 LOC CSS/Theme in
125 Dateien mit GTK-CSS-APIs.

### 2.1 Was günstig vorgefunden wird

- **`reprise-runtime` ist bereits transportfrei.** Die Laufzeitlogik (Player,
  Queue, Jobs, Geräteläufe) linkt kein D-Bus; nur die Serving-Schleife liegt
  in `reprise-platform-linux`. Auf Android, Windows und macOS lässt sich
  dieselbe Runtime in-process einbetten, ohne sie anzufassen.
- **Der Protokollvertrag ist bereits Command + Snapshot über `serde`.**
- **`check-frontend-thinness.sh` existiert** und pinnt bereits Budgets für
  direkte DB-, Dateisystem-, Thread- und Worker-Zugriffe im Frontend.
- **`docs/ux-rules.md` ist eine geschriebene Abnahmespezifikation.**
- **Der Dependency-Baum ist F-Droid-tauglich.** `rusqlite` (bundled), `lofty`,
  `image`, `ureq` (rustls), `notify`, `walkdir` sind allesamt freie
  Rust-Crates ohne proprietäre Blobs. Media3/ExoPlayer und Jetpack Compose
  sind Apache-2.0 und brauchen keine Google Play Services.

### 2.2 Wo die Nähte lecken

1. **~29k LOC portable Präsentationslogik sitzen in der GPL-Frontend-Crate**
   (`queue_transport.rs`, `podcasts_presentation.rs`, `nav_history.rs`, alle
   `strings_*`). Für andere Oberflächen heute unerreichbar.
2. **`reprise-runtime-client` ist an `zbus` gebunden.** Android, Windows und
   macOS haben keinen D-Bus.
3. **Das Protokoll liefert Rohzustand, keine darstellungsfertigen Werte.**
4. **Die UX-Regeln kennen keinen Geltungsbereich.**
5. **Ein großer Teil der Frontend-Tests braucht ein Display** und ist im Rudel
   flaky (siehe `check-display-tests.sh`).
6. **Der Core spricht mit nicht-freien Netzdiensten.** Last.fm (Scrobbling),
   Cover- und Artist-News-Quellen. Für F-Droid ist das kein Ausschlussgrund,
   aber es bestimmt die Anti-Feature-Kennzeichnung (B11).

## 3. Beschlüsse

Gegrillt und beschlossen am 2026-08-01.

**B1 — Drei Oberflächen, nicht eine und nicht fünf.** `reprise-gnome` bleibt
das Vollprodukt für GNOME/Linux. Eine native Compose-App bedient Android.
Eine Tauri-2-App bedient KDE, macOS und Windows. Verworfen: je ein natives
Frontend pro Plattform (nicht tragbar); eine einzige Web-App für alles (siehe
B6); GTK auf Plasma nur zivilisieren (löst die anderen Plattformen nicht).

Die ursprüngliche Fassung dieses Designs zielte auf **zwei** Oberflächen und
begründete das mit der Dauerlast. Das war ein Fehlschluss: der teure Teil ist
die Logik, nicht die Widget-Ebene, und die Logik wird durch `reprise-view`
ohnehin nur einmal gebaut.

**B2 — Die neuen Oberflächen sind kleinere Produkte, keine Kopien.** Weder
Android- noch Tauri-App streben Funktionsparität an. Zwei Zuschnitte, 3.1.

**B3 — Desktop: Tauri 2 mit Web-UI für KDE, macOS und Windows.** Gründe: (a)
der bestehende `serde`-Snapshot-Vertrag passt direkt auf Tauri-IPC; (b)
Agenten erzeugen Web-UI deutlich schneller als Slint oder QML, was bei diesem
Arbeitsmodell der ausschlaggebende Multiplikator ist; (c) höchste
Design-Decke. Verworfen: Rust-UI-Toolkit (Slint/Dioxus) — jedes Listen-,
Drag&Drop- und Kontextmenü-Verhalten wäre Eigenbau; Qt/QML — dritte Sprache,
auf Windows weder nativ noch modern.

**B4 — Geteilte Präsentationsschicht `reprise-view`, MIT.** Konsistent mit
`reprise-core`, `-runtime` und `-platform-linux`; hält den in `LICENSING.md`
vorgesehenen Pfad für fremde oder proprietäre Frontends offen. Der Autor hält
das Copyright an allen bewegten Zeilen allein. Die übersetzbaren Texte ziehen
mit um; siehe offener Punkt O2.

**B5 — Migration vollständig, aber nach Bedarf geschnitten.** `reprise-view`
wird nicht opportunistisch „bei Berührung" gefüllt, sondern paketweise und je
Ausschnitt vollständig. Die **Reihenfolge** der Ausschnitte richtet sich nach
der Produktpriorität: zuerst alles, was der mobile Zuschnitt braucht (P1a),
danach der Rest (P1b).

> **Revision 2026-08-01, nach Festlegung der Android-Priorität.** Die
> ursprüngliche Fassung lautete „erst `reprise-gnome` vollständig umbauen,
> dann neue Oberflächen". Das hätte die Android-App um Monate verzögert. Der
> Unterschied zum verworfenen „bei Berührung migrieren" bleibt gewahrt: dort
> wäre die Migration opportunistisch und ungeplant gewesen, hier ist sie
> geplant und für einen definierten Ausschnitt vollständig. Das GTK-Frontend
> wird in P1a auf den migrierten Ausschnitt mitgezogen — es bleibt zu keinem
> Zeitpunkt auf einer Altfassung sitzen.

**B6 — Android wird nativ (Jetpack Compose), nicht Tauri.** Drei Gründe:

1. **Der Sharing-Vorteil war eine Illusion.** Zwischen Web-Desktop und
   Web-Mobil teilen sich Design-Tokens, Icons und ein paar Primitive — also
   die billige Hälfte. Layout, Navigation und Interaktionsmodell sind
   verschieden (Pointer + Tastatur + dichte Tabellen gegen Touch + Gesten).
2. **Der WebView-Preis fällt auf dem heißen Pfad an.** Scroll-Performance und
   Touch-Physik in einer langen Titelliste sind die Kerninteraktion eines
   Musikplayers, nicht ein Randfall.
3. **Der Hybrid wäre die komplizierteste Variante gewesen.** Tauri auf Android
   hieße WebView ↔ Tauri-IPC ↔ Rust-Runtime ↔ JNI ↔ Kotlin-Foreground-Service
   ↔ Media3 — vier Grenzen. Nativ sind es zwei: Compose ↔ UniFFI ↔ Rust ↔
   Media3.

Entscheidend ist, dass dabei **nichts von der Vorbereitung verloren geht**:
`reprise-view` ist Rust und kompiliert nach Android. Die Compose-App bindet
dieselbe Präsentationsschicht ein, die GTK konsumiert. Etabliertes Muster; das
Matrix Rust SDK fährt einen Rust-Logikkern unter nativen mobilen Oberflächen.

**B7 — Android-Playback läuft in einem `MediaSessionService`.** Eine Musik-App
muss bei ausgeschaltetem Bildschirm weiterspielen. Media3/ExoPlayer in einem
Foreground-Service ist der Weg; der Service ist zugleich der **Wirt der
eingebetteten Runtime** — dieselbe Rolle, die auf Linux die D-Bus-Aktivierung
spielt. Durch B6 ist das keine Sonderkonstruktion, sondern die natürliche
Architektur der App.

**B8 — Kein Kotlin Multiplatform, kein Compose Multiplatform.** KMP teilt
Kotlin-Code über Plattformen; die geteilte Logik dieses Projekts ist aber Rust
(`reprise-view`). KMP würde eine zweite, leere oder duplizierende Schicht
darüberlegen. Für ein einzelnes Kotlin-Ziel ist es definitionsgemäß Overhead.
Es würde erst richtig, wenn iOS käme — und iOS ist Nicht-Ziel (Sektion 6).

**B9 — Der heiße Fenster-Query-Pfad bleibt Funktionsaufruf.** Die gefensterten
200-Zeilen-Queries gehen nicht über IPC. Das bestätigt Beschluss 1 aus
`docs/plans/multi-frontend-core.md` §2.1 und ändert ihn nicht.

**B10 — Der Android-Spike läuft vor allem anderen Code.** Media3/JNI,
Foreground-Service als Runtime-Wirt, UniFFI über `reprise-view`, SAF gegen den
pfadbasierten Scanner und die F-Droid-Baubarkeit sind fünf gekoppelte
Unbekannte auf der Plattform, auf der die wenigste Erfahrung vorliegt.

**B11 — Veröffentlichungsziel Android ist F-Droid, Google Play als zweiter,
kostenpflichtiger Kanal.** Gleiche Quellen, zwei Listings: F-Droid gratis,
Play bezahlt. Das monetarisiert die Bequemlichkeit, ohne die Identität zu
kosten. **Keine Werbung** (siehe Nicht-Ziele). Daraus folgen fünf bindende
Randbedingungen, die jede spätere Entscheidung überstimmen:

1. **Keine proprietären Abhängigkeiten.** Keine Google Play Services, keine
   Firebase, keine Closed-Source-SDKs. Media3/ExoPlayer und Compose erfüllen
   das (Apache-2.0); der Rust-Baum ebenfalls (2.1).
2. **F-Droid baut selbst aus den Quellen — und kann es.** Beantwortet am
   2026-08-01, Befund in `docs/research/android-spike-2026-08.md`:
   **TRÄGT MIT AUFLAGEN.** Delta Chat (`com.b44t.messenger`) baut eine
   Rust-Kernbibliothek über das NDK aus einem produktiven Rezept, zuletzt
   veröffentlicht am 2026-07-31. `rustup` ist erlaubt, Netzzugang während
   des Builds vorhanden (also kein `cargo vendor` nötig), das Timeout ist
   konfigurierbar. Auflagen: ein Build-Eintrag **je ABI** mit eigenem
   `versionCode` (F-Droid kennt keine App Bundles), großzügiges `timeout:`,
   NDK r27 als Rückfallebene neben der aktuellen Version, und die
   Erstaufnahme-Pipeline früh mit den Maintainern klären. Die
   Abbruchbedingung aus B10 ist damit aufgehoben.
3. **Nicht-freie Netzdienste werden gekennzeichnet, nicht versteckt.**
   Last.fm und die Cover-/News-Quellen führen zu einem `NonFreeNet`-
   Anti-Feature. Kein Ausschlussgrund; MusicBrainz ist frei und
   unproblematisch. Der mobile Zuschnitt (3.1) hält diese Fläche ohnehin
   klein.
4. **Lizenz der Android-App: GPL-3.0-or-later**, wie `reprise-gnome`. F-Droid
   ist damit uneingeschränkt kompatibel. Die oft zitierte GPL-3-Reibung mit
   Play betrifft das Weiterverbreiten *fremden* GPL-3-Codes; als
   Alleinurheber ist der Autor davon nicht betroffen.
5. **Speicherzugriff ausschließlich über SAF.** `MANAGE_EXTERNAL_STORAGE`
   wird von Play im Wesentlichen nur Dateimanagern genehmigt, und MediaStore
   allein reicht nicht: Reprise braucht die Geschwisterdateien neben den
   Titeln (`cover.jpg`, LRC-Sidecars, M3U) sowie Schreibzugriff für
   Tag-Writeback. Ein per `ACTION_OPEN_DOCUMENT_TREE` gewählter Baum mit
   persistierter Berechtigung liefert genau das. Diese Disziplin ist
   Voraussetzung für den Play-Kanal, nicht Vorsorge.

**B12 — Die Android-App ist eigenständig lauffähig.** Sie ist ein
vollwertiger lokaler Musikplayer und setzt keine Desktop-Installation
voraus. Das ist möglich, weil `reprise-core` bereits ein vollständiger
Bibliotheksverwalter ist — Scanner, Datenbank, Playlists, Queue, Queries —
und die App ihn einbettet.

Konsequenz für den Zuschnitt: Ersteinrichtung, Playlist-Bearbeitung und
Grundeinstellungen gehören in den Mobil-Zuschnitt (3.1) und damit in P1a.

Klarstellung zur Verzahnung mit dem Desktop: Heute existiert **Geräte-Sync
über MTP** — der Desktop schiebt Dateien aufs Telefon. Eine echte
Verzahnung (geteilte Playlists, Hörstand, Statistiken) ist eine eigene,
spätere Funktion und **nicht Teil von P7**. Für v1 gilt: eigenständiger
Player, der denselben Kern hat wie der Desktop.

**B13 — Podcasts und Radio verlassen `reprise-gnome` nie.** Ihre
Präsentationslogik wandert **nicht** nach `reprise-view`, auch nicht in P1b.
Beide Oberflächen bieten sie nicht an (3.1), also hätte die geteilte Crate
für diesen Code keinen zweiten Konsumenten — sie zu migrieren wäre Arbeit
ohne Gegenwert und würde die Crate aufblähen, die B4 klein halten soll.

Gemessen (`origin/dev`, 2026-08-01): 58 Dateien mit 14.902 LOC unter
`ui/podcasts/**`, `ui/radio/**` und `strings_podcasts.rs`, davon **33
Dateien mit 5.593 LOC toolkit-frei** — genau der Anteil, der sonst
mitgezogen worden wäre.

**YouTube ist davon ausgenommen, weil es kein Subsystem ist.** Es ist eine
Variante im Kern-Datenmodell (`ViewSource::Youtube`), ein persistiertes
Feature-Flag (`modules::YOUTUBE_MODULE`) und ein `BrowserPlace::Youtube` in
`nav_history.rs` — einer Datei, die im Mobil-Zuschnitt liegt und migrieren
muss. Die neuen Oberflächen lassen das Modul aus und zeigen keine
YouTube-Ansichten; die Enum-Varianten reisen im geteilten Code mit. Das ist
ein Ausschluss auf der Oberfläche, nicht im Code, und kostet eine Handvoll
Match-Arme.

**Rückwirkung auf S1:** Der Spike-Task zu UniFFI (Frage 3) hatte
`podcasts_presentation.rs` als Prüfobjekt gewählt, weil er verschachtelte
Strukturen, ein Enum und `BTreeMap` enthält. Diese Datei migriert nun nie.
Prüfobjekt ist stattdessen `ui/track_list/queue_sections.rs` mit
`QueueViewModel`, `QueueSection`, `QueueSectionKind` und
`VirtualContextTail` — dieselben FFI-kritischen Formen, aber im
Mobil-Zuschnitt und damit tatsächlich grenzüberschreitend.

**B14 — Designrichtung Android: „1a Cover First".** Gewählt am 2026-08-01 aus
`claude.ai/design/p/89a8e3a7-2b40-407f-990c-258502b0b47d` („Reprise Mobile App
Design", drei Richtungen). Material-3-Struktur auf dunklem Grund; der
Visualizer lebt im Scrubber statt das Interface zu übernehmen; die ruhigste
und nativste der drei Varianten.

Tokens, die daraus verbindlich werden (toolkit-unabhängig, später ein
Compose-Theme):

| Rolle | Wert |
| --- | --- |
| Ruhe-Akzent (Reprise-Teal) | `#20B2AA` |
| Playback-Glow (Nocturne-Blurple) | `#9184D9` |
| Grund | `#0B0C12` |
| Fläche | `#101219` |
| Text auf Teal | `#8FDCD7` |
| Schrift | Inter |
| Icons | Phosphor Icons — MIT, erfüllt B11.1 |

**Das Mockup zeichnet v2, nicht v1.** Es zeigt einen Sync-Chip („Wi-Fi
gekoppelt · Sync vor 2 Min"), Sterne-Bewertungen, Favoriten-Herz und
Abspielzähler — nichts davon steht im Mobil-Zuschnitt (3.1). Verbindlich für
v1 ist der Zuschnitt, nicht das Mockup. Zwei Korrekturen sind dabei benannt:

1. **Der Sync-Chip wird zum Bibliotheks-Chip.** Die App kann einen Sync gar
   nicht beobachten: MTP läuft desktopseitig über GVfs (`mtp://` in
   `reprise-platform-linux`), das Telefon ist der passive Teil, und es gibt
   weder Pairing noch Sync-Ereignis. Ehrlich anzeigbar ist Bibliothekszustand
   („1.284 Titel · 12 neu"), abgeleitet aus dem Scanner. Dieselbe Stelle im
   Layout, eine Aussage, die stimmt.
2. **Bewertungen, Favoriten und Abspielzähler bleiben v2**, zusammen mit den
   Statistiken (3.1).

**B15 — Waveform-Peaks reisen mit der Datei; Android rechnet nur im
Notfall.** Die Scrubber-Waveform aus Design 1a ist `tracks.waveform_peaks`
(1000 `u8` je Titel), heute berechnet aus dekodiertem Audio über den
Core-Vertrag `WaveformBackend` — mit genau einer Implementierung
(`reprise-platform-linux`, GStreamer). Eine ganze Bibliothek auf dem Telefon
zu dekodieren ist akkufeindlich und unnötig, weil der Desktop das Ergebnis
schon hat.

Dreistufig:

1. **Der Geräte-Sync schreibt die Peaks aufs Gerät**, gelesen aus
   `tracks.waveform_peaks`. Das ist keine neue Maschinerie: `device_sync`
   schreibt heute schon Nicht-Audio-Artefakte dorthin (M3U-Playlists über
   `PlaylistWrite`). Die Peaks werden dabei **nie in der Musiksammlung
   materialisiert** — siehe die Trägerbegründung unten.
2. **Android liest sie, wenn vorhanden** — dann ist die Waveform gratis und
   `reprise-android` braucht **keine** `WaveformBackend`-Implementierung.
   Das streicht einen Posten aus P4a.
3. **Fehlen sie, rechnet Android nur für den laufenden Titel**, nie als
   Bibliotheks-Durchlauf. Das ist der Fall des eigenständigen Nutzers (B12),
   der nie synchronisiert — er bekommt einen Scrubber, aber keinen
   Akkufresser.

**Träger: Bytes direkt aufs Gerät, wie bei Playlists.** Nicht als Datei in
der Musiksammlung und nicht als Tag im Audio. Die erste Fassung dieses
Beschlusses empfahl „Sidecar wie bei den LRC-Lyrics"; eine Code-Analyse hat
das widerlegt, und die Begründung ist es wert, festgehalten zu werden:

- **`device_sync` hat zwei verschiedene Transportwege, nicht einen.**
  `PlaylistWrite` (`device_sync/mirror.rs:90`, ausgeführt über
  `replace_playlist` in `platform-linux/device_sync.rs:495`) schreibt **Bytes
  aus dem Speicher** aufs Gerät und braucht keine Quelldatei. Das
  LRC-Sidecar (`device_sync/lyrics_sidecar.rs`, ausgeführt in
  `device_sync_effects.rs:322`) **kopiert eine Datei von der Platte**. Peaks
  sind ein DB-Blob und haben keine Datei — für sie passt der erste Weg.
- **Das LRC-Muster taugt hier nicht als Vorbild.** Es schreibt in die
  Sammlung des Nutzers, weil `.lrc` ein universelles Format ist, das *jeder*
  Player lesen kann — das ist die Rechtfertigung. Peaks im Reprise-eigenen
  Byteformat (1000 Buckets, sqrt-normalisiert, `core/waveform.rs`) sind für
  kein anderes Programm lesbar. Man zöge die gesamte Sicherheitsmaschinerie
  von `writeback_publish.rs` (atomarer Hardlink-Publish, `ignore_path` gegen
  Rescan-Stürme, Leftover-Sweep) mit, ohne den Nutzen zu bekommen, für den
  sie gebaut wurde — und erbte das dort bewusst akzeptierte Waisen-Risiko bei
  Umbenennungen.
- **Der Custom-Tag scheitert am Transcode.** `AudioMetadata`
  (`platform-linux/device_transfer.rs:70`) trägt exakt fünf Felder plus
  Cover; der GStreamer-Transcode nach Opus oder MP3 setzt nur diese. Ein
  Peaks-Tag in einer FLAC-Quelle existierte in der transkodierten Kopie
  schlicht nicht mehr — und der Sync transkodiert.

Übernommen wird also die **Verdrahtungsform** von LRC (außerhalb des reinen
Reducers, an den bestehenden Aufrufstellen in `device_sync_effects.rs`) mit
der **Schreibmechanik** von `replace_playlist`. `mirror.rs` und `machine.rs`
bleiben unberührt, es braucht keine DB-Migration und keine neuen
`Effect`-Varianten.

**Zwei Punkte, die vor dem Bauen zu entscheiden sind:**

1. **Peaks entstehen heute nur beim Abspielen**, nicht beim Scan
   (`ui/playback/now_playing_wiring.rs:299`). Die vorbereitete
   Backfill-Abfrage `pending_waveform_tracks` (`core/db.rs:755`) hat **keinen
   Aufrufer**. Ohne einen Backfill-Worker bekommt das Telefon nur Waveforms
   für Titel, die am Desktop schon einmal liefen — eine Produktentscheidung,
   keine Implementierungsfrage.
2. **Eine Datei je Titel oder ein gebündeltes Manifest je Sync-Lauf.** MTP
   hat feste Latenz je Schreibvorgang; 1 KB pro Titel ist datenmäßig nichts,
   aber ein zusätzlicher Aufruf je Titel ist auf manchen Geräten spürbar.

Die Song-Analyse (`track_audio_analysis`) ist hiervon **nicht** betroffen:
sie wurde entfernt (`db_drop_audio_analysis_mix.rs`) und kommt nicht zurück.

### 3.1 Zuschnitte

Zwei Zuschnitte, beide deutlich enger als das GTK-Vollprodukt.

**Mobil (Android — wird zuerst gebaut):** Ersteinrichtung (Musikordner per
SAF wählen, Scan, Fortschritt); Bibliothek durchsuchen und browsen;
Wiedergabe und Transport; Queue; Playlists lesen, **erstellen und
bearbeiten**; Suche; Now Playing mit Cover und Lyrics; Grundeinstellungen
(Bibliothekspfad, Wiedergabe). **Ohne** Statistiken — erster Kandidat für
v2, nicht für v1.

Die ersten drei Flächen folgen aus B12: ein eigenständiger Player braucht
Onboarding, Playlist-Bearbeitung und Einstellungen, eine reine Companion-App
nicht.

**Desktop (Tauri — KDE, macOS, Windows):** wie mobil, zusätzlich
Smart-Playlists lesend, Filter über die Bibliothek und Statistiken lesend.

**In keinem der beiden:** Tag-Editor; Geräte-Sync/MTP; Stem-Separation und
Instrumental-Fassungen; Concerts; Artist News; Library Doctor;
Import-Fehler-Verwaltung; Podcasts und Radio; Einrichtung von Scrobbling.

Scrobbling **selbst** läuft in der Runtime. Auf Android bedeutet das: es ist
vorhanden, sobald die Runtime eingebettet ist, und schlägt daher auf die
`NonFreeNet`-Kennzeichnung durch (B11.3). Seine Einrichtungsoberfläche bleibt
GTK-exklusiv.

Die Zuschnitte sind Entscheidungen über Oberflächen, nicht über Daten: Die
ausgeschlossenen Bereiche bleiben in der Datenbank sichtbar und werden von den
neuen Oberflächen nicht beschädigt.

## 4. Arbeitspakete

Reihenfolge, nach Android-Priorität geordnet:

```text
P0 -> S1 -> P1a -> P3 -> P4a -> P7 -> P8      (Android bis F-Droid)
                     `-> P1b -> P2 -> P4b -> P6   (Desktop danach)
P5 laeuft durchgehend parallel (nur Dokumentation)
```

Dieses Dokument ist eine **Architektur-Spec, kein Ausführungsplan.** Jedes
Paket braucht einen eigenen Implementierungsplan; P1a und P1b mit hoher
Wahrscheinlichkeit mehrere, wellenweise nach Dateibesitz geschnitten.

### P0 — Vorlauf

- Laufende Pläne landen oder werden ausdrücklich ausgeklammert. Stand
  2026-08-01: `podcasts-radio` (planned), `motion-player` (planned),
  `audio-character-mcp` (ready-for-review), `list-views-fixes` (refactored),
  `ux-rules-motion` (reviewed), `podcast-row-click-selection` (coded).
- Dateibesitz für die Umbauwellen in `AGENTS.md` verankern, nicht nur im Plan.
- `reprise-view` als leere Crate anlegen, MIT, mit `cargo tree`-Gate.
- **Play-Entwicklerkonto anlegen — Aufgabe des Autors, nicht eines Agenten.**
  Neue Privatkonten müssen vor der ersten Produktionsveröffentlichung eine
  Testphase mit 20 Testern über 14 Tage durchlaufen. Das ist der einzige
  Punkt dieses Designs mit einer Wartezeit, die sich nicht parallelisieren
  lässt, und er läuft deshalb ab P0 nebenher — nicht erst bei P8.

### S1 — Android-Spike (vor allem anderen Code, siehe B10)

Beweist oder widerlegt sechs Annahmen an einem Wegwerf-Prototyp:

0. Der Rust-Baum baut überhaupt für die Android-Targets — insbesondere
   `rusqlite` mit gebündeltem SQLite über das NDK. Vorbedingung für alles
   Weitere, deshalb zuerst.
1. Media3/ExoPlayer ist über JNI gegen den bestehenden
   `reprise_core::playback`-Vertrag bedienbar.
2. Ein `MediaSessionService` kann die eingebettete Runtime beherbergen und
   über den App-Lebenszyklus am Leben halten.
3. **UniFFI (oder handgeschriebenes JNI) trägt die Typen, die `reprise-view`
   liefern soll** — Listen, Enums, verschachtelte ViewModels — ohne dass die
   Bindung zum Flaschenhals wird.
4. Der pfadbasierte Scanner ist unter dem Storage Access Framework
   betreibbar — und wenn nicht, welche Core-Änderung es kostet.
5. **Die Rust-plus-NDK-Toolchain ist im F-Droid-Buildserver baubar** (B11.2).

Ergebnis ist ein schriftlicher Befund, kein Code, der übernommen wird. Punkt 3
hat Rückwirkung auf den Schnitt von `reprise-view` in P1a. Punkt 5 kann das
gesamte Vorhaben kippen und wird deshalb so früh beantwortet, wie es
technisch geht — direkt hinter Punkt 0, weil sich ohne funktionierenden
lokalen Build nichts über die Baubarkeit bei F-Droid aussagen lässt. Fällt
Punkt 5 negativ aus, entfallen die Punkte 1 bis 4.

### P1a — `reprise-view`: der mobile Ausschnitt

Alles, was der Mobil-Zuschnitt (3.1) braucht, vollständig migriert:
Track-Listen-Präsentation, Queue-Transport, Playlist-Darstellung **und
-Bearbeitung**, Suche und Filter, Now-Playing-Zustand, Lyrics-Zustand,
**Scan- und Einrichtungs-Präsentation**, Einstellungs-Präsentation, die
zugehörigen Strings.
`reprise-gnome` wird im selben Paket auf den migrierten Ausschnitt gezogen
(B5) — es bleibt zu keinem Zeitpunkt auf einer Altfassung sitzen.

### P1b — `reprise-view`: der Rest

Die verbleibende toolkit-freie und leicht gekoppelte Logik aus
`reprise-gnome`, inklusive der Flächen, die nur das GTK-Vollprodukt hat.
Der Widget-Code (55.651 LOC in 129 Dateien) bleibt in `reprise-gnome`.

**Ausgenommen (B13):** Podcasts und Radio bleiben vollständig in
`reprise-gnome` — 5.593 LOC toolkit-freier Code, der ohne zweiten
Konsumenten nichts in einer geteilten Crate zu suchen hat.

**Nebeneffekt beider Pakete:** Jeder Test, der nach `reprise-view` mitwandert,
läuft ohne Display und deterministisch.

### P2 — Protokoll auf ViewModels heben

`reprise-runtime-protocol` bekommt darstellungsfertige Snapshots aus
`reprise-view`. Ausgenommen bleibt der gefensterte Query-Pfad (B9). Damit wird
„keine Fachlogik in TypeScript" durchsetzbar. **Für Android nicht bindend** —
die Compose-App linkt `reprise-view` direkt.

### P3 — Transport entkoppeln

`reprise-runtime-client` bekommt ein Transport-Trait mit zwei
Implementierungen: `zbus` (Linux) und in-process (Android, Windows, macOS).
`reprise-runtime` bleibt unangetastet. Vorbedingung für P7.

### P4a — Plattform-Backend Android

Umsetzung des in S1 bestätigten Weges gegen die bestehenden Core-Verträge
(`playback`, `media_integration`); es entstehen keine neuen Verträge.

**Ohne `WaveformBackend`** im Normalfall — die Peaks reisen mit der Datei
(B15); nur der Notfallpfad für den laufenden Titel braucht Dekodierung, und
die liefert Media3 ohnehin.

**Die Desktop-Hälfte von B15** — die Peaks im Geräte-Sync mitschreiben — ist
Arbeit an `reprise-core`, `reprise-platform-linux` und der
`device_sync`-Verdrahtung im GTK-Frontend, nicht an Android. Sie hängt an
keinem Spike-Befund, kann unabhängig von P7 landen und nützt sofort.
Umfang laut Code-Analyse: eine neue Pfad-Projektion neben
`device_sync/lyrics_sidecar.rs`, eine verallgemeinerte Bytes-Schreibmethode
statt des auf `.m3u8` festgelegten `replace_playlist`, die zugehörige
Trait- und Backend-Ergänzung, und Kopier-/Entfernen-Funktionen an den drei
bestehenden Aufrufstellen. Keine DB-Migration.

### P4b — Plattform-Backends Windows und macOS

- **Windows:** Playback-Backend. GStreamer (buildbar, Packaging-lastig) oder
  Alternative — Spike, siehe O1.
- **macOS:** Playback-Backend (AVFoundation oder GStreamer),
  `MPNowPlayingInfoCenter` statt MPRIS, Code-Signing und Notarisierung.

**P4a + P4b sind zusammen das Paket mit dem größten Risiko.** Sie wachsen mit
jeder Zielplattform linear, während die Oberflächen konstant bleiben.

### P5 — UX-Regeln nach Geltungsbereich markieren

Jede der ~396 Regeln in `docs/ux-rules.md` bekennt sich zu `[alle]`, `[gtk]`,
`[mobil]` oder `[desktop]`. `check-ux-traceability.sh` lernt die Scopes. Läuft
durchgehend parallel; der `[mobil]`-Anteil muss vor P7 fertig sein.

### P6 — Die Tauri-App

Desktop-Zuschnitt nach 3.1. Geschätzt 15–25k LOC Web-UI. Konsumiert
`reprise-view` über Tauri-IPC; kein direkter DB-Zugriff aus TypeScript.
`reprise-app` ist ein Arbeitstitel.

### P7 — Die Android-App

Mobil-Zuschnitt nach 3.1. Geschätzt 8–15k LOC Compose, GPL-3.0-or-later
(B11.4). Konsumiert `reprise-view` über UniFFI/JNI; hostet die Runtime im
`MediaSessionService` (B7).

### P8 — F-Droid-Veröffentlichung

Build-Rezept und Metadaten für `fdroiddata`; ABI-Splits; Deklaration der
Anti-Features nach B11.3; ein Release-Test, der beweist, dass ein sauberer
Klon ohne lokalen Zustand baut. Reproduzierbare Builds sind Kür, nicht Pflicht.

## 5. Verifikation

Jedes Paket ist an ein mechanisches Tor gebunden, nicht an eine Zusage:

- **`check-architecture.sh` erweitern:** `reprise-view` darf kein `gtk4`,
  `libadwaita`, `glib`, `gstreamer` oder `zbus` in den Baum ziehen — gleiche
  `run_dependency_probe`-Prüfung wie für `reprise-core` und `reprise-runtime`.
- **`check-frontend-thinness.sh`:** Die Budgets sinken monoton mit jeder
  Welle. Eine Welle, die kein Budget senkt, hat nichts bewegt.
- **`check-ux-traceability.sh`:** kennt nach P5 die Geltungsbereiche und misst
  je Oberfläche getrennt.
- **Neu:** Die Tests von `reprise-view` laufen ohne `DISPLAY`. Ein Test in
  dieser Crate, der ein Display braucht, ist ein Fehler im Umzug.
- **Neu:** `reprise-view` baut für die Android-Targets (`cargo ndk`), sonst
  blockiert P7, ohne dass es jemand merkt.
- **Neu (P8):** Ein Build aus einem frischen Klon ohne lokalen Zustand, der
  dem F-Droid-Rezept folgt.
- Die Test-Baseline beim Paket-Start ist die Referenz; jedes Paket bleibt
  dagegen grün.

## 6. Nicht-Ziele

- Kein Ersatz des GTK-Frontends. `reprise-gnome` bleibt das Vollprodukt.
- Keine Funktionsparität der neuen Oberflächen (B2).
- **Kein iOS.** Ausdrücklich entschieden (2026-08-01); es ist der einzige
  Grund, der KMP/CMP rechtfertigen würde (B8).
- **Keine Werbung.** Ad-SDKs sind proprietär, schließen F-Droid aus, wären
  in einer GPL-3-App erklärungsbedürftig und tragen bei realistischer
  Nutzerzahl weniger ein als ein bezahlter Play-Kanal (B11). Ausdrücklich
  entschieden 2026-08-01, damit die Frage nicht wiederkehrt.
- **Keine Desktop-Verzahnung in P7.** Geteilte Playlists, Hörstand und
  Statistiken zwischen Telefon und Desktop sind eine eigene, spätere
  Funktion (B12). Der bestehende MTP-Geräte-Sync bleibt unberührt.
- Keine Änderung an Beschluss 1 aus `docs/plans/multi-frontend-core.md`: Daten
  bleiben eingebettetes SQLite mit `change_log`, kein IPC auf dem Lesepfad.
- Kein Umbau des GTK-Widget-Codes über das hinaus, was die Entkopplung in P1a
  und P1b erzwingt.

## 7. Risiken

| Risiko | Wirkung | Gegenmittel |
| --- | --- | --- |
| ~~F-Droid kann die Rust-NDK-Toolchain nicht bauen~~ | **entschärft 2026-08-01** | Präzedenzfall Delta Chat; Befund in `docs/research/android-spike-2026-08.md` |
| Erstaufnahme in `fdroiddata` scheitert an der CI-Pipeline | P8 verzögert sich, Veröffentlichung blockiert | Bei vergleichbaren Projekten real aufgetreten; früh mit den Maintainern klären, nicht erst beim Einreichen |
| Android ist teurer als gedacht (Media3, Service, UniFFI, SAF) | P7 sprengt den Rahmen | S1 vor allem anderen (B10); Befund kann B6 kippen |
| P4a + P4b wachsen mit jeder Plattform | drei Backends statt einem | Zuschnitt (3.1) hält die Backends klein; O1 vor P4b |
| P1a verzögert sich und blockiert P7 | Android rückt weg | Ausschnitt ist nach Mobil-Zuschnitt geschnitten, also der kleinstmögliche für ein auslieferbares Produkt |
| Der Mobil-Zuschnitt wächst weiter | P1a und P7 laufen davon | 3.1 ist bindend; Statistiken sind ausdrücklich v2. Jede Erweiterung ist eine Spec-Änderung |
| SAF trägt den pfadbasierten Scanner nicht | B11.5 fällt, Play-Kanal fällt, ggf. Core-Umbau | S1-Frage 4, vor jedem Aufwand beantwortet |
| `reprise-view` wird zur Müllhalde | neue Kopplung statt weniger | `cargo tree`-Gate, 800-Zeilen-Limit, Zweck je Modul dokumentiert |
| Fachlogik wandert nach TypeScript | die Duplikation kehrt zurück | P2 liefert fertige Werte; IPC-Grenze macht Verstöße sichtbar |
| Drei Oberflächen driften auseinander | UX-Regeln werden ungleich erfüllt | P5 macht den Geltungsbereich je Regel explizit und messbar |
| Plattform-Drift im Zuschnitt | „nur noch dieses eine Feature" je Oberfläche | 3.1 ist bindend; Erweiterung ist eine Spec-Änderung, kein Ticket |

## 8. Offene Punkte

- **O1 — Windows- und macOS-Playback-Backend.** GStreamer oder plattformeigene
  APIs. Eigener Spike vor P4b; nicht Teil von S1.
- **O2 — i18n in den neuen Oberflächen.** Die ~5.427 LOC Texte ziehen nach
  `reprise-view` (B4). Wie Compose und Tauri die `po`-Kataloge konsumieren,
  ist noch nicht entschieden.
- **O3 — Sichtbarkeit externer Änderungen auf Android.** Der
  `notify`-basierte Weckruf aus `docs/plans/multi-frontend-core.md` §2.2 ist
  unter Android-Storage nicht selbstverständlich. Klärung in S1 oder P4a.
- **O4 — APK-Größe.** Rust plus gebündeltes SQLite über vier ABIs. Ob
  ABI-Splits reichen oder eine ABI-Auswahl nötig wird, entscheidet P8.

## 9. Parkplatz — notiert, nicht gebaut

Nichts hiervon ist Teil dieses Designs. Der Zweck der Liste ist, dass diese
Ideen nicht als stille Annahmen in die Pakete sickern.

- **Wi-Fi-Sync zwischen Desktop und Telefon.** Für v1 bleibt es beim
  bestehenden MTP-Weg über Kabel, der desktopseitig vollständig existiert und
  auf der Android-Seite **keine Arbeit** erfordert (B14). Ein
  Wi-Fi-Sync wäre ein eigenes Paket mit Pairing, Übertragungsprotokoll und
  Konfliktauflösung — und erst dann trägt der Sync-Chip aus dem Mockup seine
  ursprüngliche Beschriftung.
- **Echte Desktop-Verzahnung.** Geteilte Playlists, Hörstand und Statistiken
  (B12). Setzt den vorigen Punkt voraus.
- **Statistiken, Bewertungen, Favoriten und Abspielzähler auf Mobil** (B14,
  3.1). Erster Kandidat für v2, sobald v1 steht.
- **iOS.** Nicht-Ziel (B8, Sektion 6); hier nur notiert, weil es die einzige
  Bedingung ist, unter der KMP/CMP richtig würde.
