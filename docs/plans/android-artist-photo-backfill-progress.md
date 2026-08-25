---
slug: android-artist-photo-backfill-progress
worktree: /home/marvin/Projects/reprise-android-artist-photo-backfill-progress
branch: feature/android-artist-photo-backfill-progress
phase: planned
codex_session:
created: 2026-08-25
---
# Künstlerfotos nachladen — und den Lauf sichtbar machen (Android)

Der Entwurf ist `docs/design/android-download-progress.design.html`, Variante
**1c „Leiste mit Balken"**, Runde 2. Die Zahlen und Farben dort sind bindend;
die Hex-Werte in diesem Plan benennen, welches vorhandene Theme-Token gemeint
ist — sie sind **keine** Anweisung, neue Werte ins Theme zu schreiben.

## Warum das mehr ist als eine Fortschrittsanzeige

Die Aufgabe hieß „Fortschrittsanzeige". Sie ist es nicht: der Vorgang, den die
Leiste anzeigen soll, existiert nirgends.

| Befund | Beleg |
|---|---|
| Kein Bulk-Lauf über Künstler, weder Android noch Desktop | `SignatureTask::ALL` kennt nur `Spectrogram` und `CoverDownload` — `crates/reprise-core/src/library/startup_tasks.rs:21-27` |
| Der Toggle setzt **nur ein Flag** | `crates/reprise-android-ffi/src/online_sources.rs:16-26` — `set_enabled` + `modules::set_enabled(ARTWORK_MODULE)`, sonst nichts |
| Fotos kommen nur on-demand pro sichtbarer Zeile | `android/…/ArtistCover.kt:20-48`; `artist_portrait_worker.rs:19-29` ist eine `VecDeque` ohne Kenntnis einer Gesamtzahl |
| Die Desktop-„Background activity" gibt es auch dort noch nicht | Der String kommt im Quellcode nicht vor. Sichtbar ist der `ScanChip` (`crates/reprise-gnome/src/ui/scan/scan_chip.rs`), gespeist von Lyrics- und Album-Cover-Batch — Porträts speisen ihn nicht |
| **Die FFI verschluckt den Transportfehler** | `crates/reprise-android-ffi/src/artist_portrait.rs:60-64` — `Err(error) => Ok(None)`. „Kein Netz" und „Deezer kennt den Künstler nicht" kommen identisch an |
| Kein Netz-Beobachter in der App | kein `ConnectivityManager`, kein `NetworkCallback` in `android/app/src/main` |
| **Kein automatischer Scan auf Android** | `LibrarySession.restore` (`LibrarySession.kt:93-100`) liest nur Tree-URI und DB, ruft nie `port.scan`. `MainActivity.onResume` (`:424`) tut nichts. Kein WorkManager/JobScheduler/AlarmManager/FileObserver. `TimeWindowTask::LibraryScan` wird von `reprise-android-ffi` nicht benutzt |
| Es gibt aber schon einen Negativ-Cache | `crates/reprise-core/src/artist_portrait/cache.rs:2` — `<key>.notfound`-Marker, Haltbarkeit 7 Tage gegen 30 Tage für Treffer (`cache.rs:34-45`, Test `:135`) |
| Scan-Fortschritt ist gepusht und ungedrosselt | `ScanProgressListener` → `crates/reprise-android-ffi/src/lib.rs:170`, ein Callback **pro Datei** (`library/scanner_progress.rs:79-90`) |
| Scan-Zustand überlebt keine Rotation | `LibraryScreen.kt:43` `remember`, kein `rememberSaveable`; `AndroidManifest.xml:44-45` deckt nur `uiMode` ab. Ein `MobileSurfaceViewModel` existiert (`MainActivity.kt:212`) |

## Was im Grill entschieden wurde

| Zweig | Entscheidung |
|---|---|
| Zähler | **Der Entwurf gewinnt gegen den Prosatext der Spezifikation.** `done` sind Künstler mit Foto, `failed` das lila Reststück, `done + failed = total` |
| Zweiter Lauf | Der `.notfound`-Marker zählt. Nach einem vollen Lauf ist die Arbeitsliste leer: keine Leiste, keine Anfrage |
| Auto-Scan | Still bei `onResume`, Mindestabstand 5 Minuten. Der Vollbild-Scan bleibt für Ordnerwahl und „Rescan" von Hand |
| Sichtbarkeit | Über dem Pager, also auf Titles, Artists **und** Queue |
| Fortsetzen | Beim App-Start, wenn der Toggle an und die Arbeitsliste nicht leer ist |
| Offline | Aus aufeinanderfolgenden Transportfehlern abgeleitet, kein `ConnectivityManager` |
| Schnitt | Kein Schnitt, ein Strang — Begründung in `## Parallelität` |

### Der Widerspruch beim Zähler, ausgeschrieben

Die Spezifikation sagt „Zähler zählt abgeschlossene Anfragen (Erfolg +
Fehlschlag)". Der Entwurf zeigt `397 / 412` **plus** „15 without a photo", und
397 + 15 = 412. Beides zugleich geht nicht.

Es gilt der Entwurf: der Zähler zählt Künstler, die ein Foto bekommen haben.
Damit sind Zustand 3 (`412 / 412`, ganz türkis) und Zustand 4 (`397 / 412`,
türkis + lila Rest) dieselbe Darstellung mit unterschiedlich viel Lila, und die
Vorlese-Ansage „Artist photos, 128 of 412 downloaded" stimmt wörtlich.

Der Balken ist zu `(done + failed) / total` gefüllt, davon `done / total`
türkis, der Rest lila. Während des Laufs hängt der Zähler deshalb sichtbar
hinter dem Balken zurück, sobald es Fehlschläge gibt — das ist gewollt und
richtig. „Keine Sprünge zurück" gilt, weil beide Zahlen nur wachsen.

---

## Teil 1 — Der Lauf im Rust-Kern

Neues Modul `crates/reprise-core/src/artist_portrait/backfill.rs`. Es liegt
bewusst **in** `reprise-core/src/artist_portrait/`, weil es `cache::negative_marker_path`
und `cache::is_fresh` braucht, die dort `pub(crate)` sind.

### 1. Arbeitsliste und Gesamtzahl

`query_artists` (`crates/reprise-core/src/queries/library_views.rs:267`) liefert
die Namen. Ein Künstler gehört in die Arbeitsliste, wenn **beides** zutrifft:

- er hat kein frisches Porträt im Cache (`cache::portrait_path_in`), **und**
- er hat keinen frischen `.notfound`-Marker (`cache::negative_marker_path` plus
  die Frist aus `cache::is_fresh`, 7 Tage).

`total` ist die Länge dieser Liste. Ist sie leer, startet **kein** Lauf und es
erscheint keine Leiste — das ist der Normalfall bei jedem Öffnen der App,
nachdem einmal alles geholt wurde.

Solange die Liste ermittelt wird, ist der Zustand `Preparing` und `total` ist
`0`. Das ist Zustand 1 des Entwurfs. Der Zustand muss **vor** der Ermittlung
gesetzt und gemeldet werden, nicht danach — sonst erscheint die Leiste erst mit
der ersten Antwort statt sofort beim Einschalten, und genau das fordert die
Spezifikation ausdrücklich.

### 2. Zustand und Fortschrittstyp

```rust
pub enum PortraitBackfillState { Preparing, Running, Paused, Complete }

pub struct PortraitBackfillProgress {
    pub run_id: u64,      // wächst mit jedem Start
    pub state: PortraitBackfillState,
    pub done: u32,        // Künstler mit Foto
    pub failed: u32,      // Anfrage beendet, kein Foto
    pub total: u32,       // 0 solange Preparing
}
```

Die Benennung lehnt sich an `lyrics::BatchProgress`
(`crates/reprise-core/src/lyrics/batch.rs:11-27`) an, statt ein drittes
Vokabular für dieselbe Sache zu erfinden.

`run_id` ist nicht kosmetisch. „Nach dem Schließen erscheint sie im selben Lauf
nicht wieder" ist ohne Laufkennung nicht definierbar; die UI merkt sich die
weggeklickte `run_id`. Ein Neustart der App ist damit ein neuer Lauf und die
Leiste erscheint wieder — das ist gewollt.

### 3. Drosselung

Der Scan-Reporter drosselt nicht und ruft pro Datei bis in die Compose-UI durch.
Für diesen Lauf wird gedrosselt, und zwar **im Kern**, damit die FFI-Grenze nicht
412-mal in schneller Folge überquert wird.

Gemeldet wird, wenn seit der letzten Meldung ≥ 250 ms vergangen sind **oder**
sich `state` geändert hat **oder** der Lauf endet.

Die beiden letzten Bedingungen sind der eigentliche Inhalt der Regel. Eine reine
Zeitdrosselung verschluckt die Schlussmeldung, und der Balken bliebe bei 96 %
stehen, obwohl der Lauf fertig ist.

Eine Meldung mit kleinerem `done` als die zuletzt gemeldete wird nie erzeugt.

### 4. Offline aus Fehlschlägen ableiten

Der Kern sieht den Unterschied noch, den die FFI wegwirft: das
`portrait_fetch`-Closure liefert `Result<PortraitOutcome, _>`.

| Ergebnis | Bedeutung | Zählung |
|---|---|---|
| `Ok(Found)` | Foto da | `done += 1` |
| `Ok(NotFound)` | Deezer kennt den Künstler nicht | `failed += 1`, Marker wird geschrieben |
| `Err(_)` | Transportfehler | **weder noch** — der Künstler geht zurück in die Warteschlange |

Drei aufeinanderfolgende `Err` → Zustand `Paused`, Beschriftung „Waiting for a
connection", Balken hält seinen Stand. Erneut versuchen mit wachsendem Abstand
(5 s, 15 s, 45 s, gedeckelt bei 2 Minuten). Das erste `Ok` in beliebiger Form
setzt den Zähler zurück und den Zustand auf `Running`.

Die dritte Tabellenzeile ist der Kern der Sache: ein Künstler, dessen Anfrage am
fehlenden Netz gescheitert ist, darf **nicht** als „without a photo" gezählt
werden und es darf **kein** `.notfound`-Marker geschrieben werden. Sonst meldet
ein Lauf, der offline begonnen hat, am Ende „12 / 412" und „400 without a
photo" — und die Marker sperren diese 400 Künstler dann sieben Tage lang aus,
obwohl nie jemand gefragt hat. Das ist der teuerste Fehler, den dieser Plan
enthalten könnte.

Ein entzogenes Netzrecht sieht ebenfalls wie ein Transportfehler aus. Der Lauf
pausiert dann dauerhaft — das ist richtig und braucht keine Sonderbehandlung.

### 5. Abbruch und Lebenszyklus

Der Lauf läuft auf einem eigenen, vom Kern gehaltenen Thread — **nicht** auf dem
Kotlin-`Thread` aus `runLibraryAction` (`MainActivity.kt:497-513`), der an die
Activity gebunden ist. Genau das lässt ihn Screen-Wechsel und Rotation
überstehen.

- Nie zwei Läufe gleichzeitig: ein Start, während einer läuft, tut nichts.
- Toggle aus → Lauf bricht ab, die Leiste verschwindet.
- Seriell, eine Anfrage nach der anderen. Der Desktop deckelt bei
  `MAX_IN_FLIGHT = 3` gegen „flooding the blocking pool"; seriell ist die
  einfachere Antwort auf dieselbe Sorge und schont zusätzlich Deezers eigene
  Ratenbegrenzung (`artist_portrait/deezer.rs:2` hat dafür schon eine Drossel).

### 6. Die FFI-Oberfläche

In `crates/reprise-android-ffi/src/artist_portrait.rs`, nach dem Muster von
`ScanProgressListener`:

```rust
#[uniffi::export(callback_interface)]
pub trait ArtistPortraitProgressListener: Send + Sync {
    fn on_progress(&self, update: ArtistPortraitProgressUpdate);
}

impl MusicLibrary {
    pub fn start_artist_portrait_backfill(&self, listener: Box<dyn ArtistPortraitProgressListener>);
    pub fn cancel_artist_portrait_backfill(&self);
    pub fn artist_portrait_backfill_progress(&self) -> ArtistPortraitProgressUpdate;
}
```

Die dritte Funktion ist neben dem Callback nicht überflüssig. Eine Rotation
zerstört die Activity und mit ihr den Listener; bei reinem Push stünde eine neu
gebaute Activity vor einer leeren Leiste bis zur nächsten Meldung — bei einem
pausierten Lauf also bis zu zwei Minuten. Sie holt den Stand sofort ab und meldet
sich dann neu an.

Der Lauf geht **nicht** über `artist_portrait_fetch`: diese Funktion wirft das
`Err` weg (Befundtabelle) und prüft das Gate bei jedem Aufruf neu. Der Lauf prüft
das Gate einmal beim Start und sitzt danach direkt auf dem `portrait_fetch`-Closure.

---

## Teil 2 — Wann der Lauf startet

Vier Anlässe. Alle vier prüfen zuerst den Toggle und dann, ob die Arbeitsliste
überhaupt etwas enthält.

### 7. Beim Einschalten des Toggles

`MainActivity.kt:338-347` setzt heute nur das Flag. Beim Einschalten zusätzlich
den Lauf starten, beim Ausschalten abbrechen.

### 8. Nach jedem Scan

`AndroidLibrarySessionPort.scan` (`AndroidLibrarySessionPort.kt:61-68`): ist der
Scan durch, den Lauf anstoßen. Das deckt „Rescan" von Hand, die erstmalige
Ordnerwahl und den stillen Auto-Scan aus Aufgabe 9 gemeinsam ab.

### 9. Automatischer Scan beim Zurückkehren in den Vordergrund

Heute scannt die App nie von selbst; nach dem Synchronisieren muss man „Rescan"
drücken. Neu:

- In `MainActivity.onResume` (`:424`, bisher leer) einen Scan anstoßen, wenn ein
  Ordner konfiguriert ist.
- Mit **Mindestabstand von 5 Minuten**, Zeitstempel des letzten Scans persistent
  in derselben Ablage wie die Tree-URI. Ohne ihn scannt jeder Wechsel zurück aus
  den Einstellungen die ganze Bibliothek erneut.
- Der Scan läuft **still**: kein `ScanningScreen`-Vollbild für diesen Fall. Die
  Liste bleibt stehen und aktualisiert sich, wenn er durch ist. Der bestehende
  Vollbild-Scan bleibt unverändert für Ordnerwahl und „Rescan" von Hand.

### 10. Beim App-Start fortsetzen

Ist der Toggle an und die Arbeitsliste nicht leer, läuft der Lauf beim Start
weiter — unabhängig davon, ob gerade gescannt wird.

Fortsetzen und Neustarten sind dank des Caches dasselbe: die schon geholten
Fotos fallen aus der Arbeitsliste, der Lauf beginnt beim Rest. Ohne diesen
Anlass fällt ein abgebrochener Lauf durch das 5-Minuten-Fenster aus Aufgabe 9:
öffnet man die App innerhalb dieser Frist wieder, wird nicht gescannt, also
startet auch nichts.

---

## Teil 3 — Die Leiste

### 11. Zustand, der Rotation überlebt

In `MobileSurfaceViewModel` (`MainActivity.kt:212`, `viewModel()`), nicht in
`remember`: der zuletzt gemeldete Fortschritt und die weggeklickte `run_id`.
`LibraryScreen.kt:43` ist ausdrücklich **nicht** der richtige Ort — dieser
Zustand ist genau der, der eine Drehung heute nicht übersteht.

Beim Anlegen einmal den Stand abholen (Aufgabe 6), dann Listener anmelden; beim
Abräumen abmelden.

### 12. Das Composable

Neue Datei `android/app/src/main/java/de/reprise/spike/ArtistPhotoProgressBar.kt`.
Nimmt einen reinen Kotlin-Zustandstyp, keinen FFI-Typ, damit es allein testbar
ist.

Die Zahlen des Entwurfs, innerhalb der Karte als dp/sp zu lesen:

| Ding | Wert | Theme-Token |
|---|---|---|
| Karte, Außenabstand (Browse) | horizontal 12dp, unten 8dp | — |
| Karte, Radius | 10dp | — |
| Karte, Fläche | `#272d38` | `colorScheme.surfaceContainerHigh` (`NocturneSurfaceContainer #292B31`) |
| Karte, Rahmen | 1dp `#333b48` | `colorScheme.outlineVariant` (`NocturneOutline #3F424D`) |
| Karte, Innenabstand | 11dp vertikal, 12dp horizontal | — |
| Abstand Kopfzeile → Balken | 8dp | — |
| Beschriftung | 12sp `#d3dae4` | `colorScheme.onSurface` |
| Zähler | 12sp `#5bd6b4` | `colorScheme.primary` (`reprise_teal #4FDBD4`) |
| Schließen `×` | `#8f96a3`, Trefferfläche 48dp | `colorScheme.onSurfaceVariant` |
| Balken | 4dp hoch, Radius voll | — |
| Balken, Spur | `#333b48` | `colorScheme.outlineVariant` |
| Balken, Füllung | `#5bd6b4` | `colorScheme.primary` |
| Balken, Fehlschlag-Rest | `#9184d9` | `colorScheme.tertiary` (`NocturneTertiary #9184D9`) — dieselbe Farbe wie die Favoriten-Herzen |
| Dritte Zeile | 11sp `#8f96a3` | `colorScheme.onSurfaceVariant` |

Auf „Online sources" dieselbe Karte ohne horizontalen Außenabstand (der Screen
hat schon 16dp), Innenabstand 12dp, Abstand Kopfzeile → Balken 9dp.

**Keine neuen Farb- oder Typografiewerte im Theme.** Wo `labelMedium` bzw.
`labelSmall` nicht auf 12sp/11sp kommen, den vorhandenen Stil nehmen und die
Schriftgröße an der Verwendungsstelle überschreiben.

Die fünf Zustände, Beschriftungen wörtlich:

| Zustand | Beschriftung | Zähler | Balken | Dritte Zeile |
|---|---|---|---|---|
| `Preparing` | `Preparing artist photos` | keiner | unbestimmt | — |
| `Running` | `Downloading artist photos` | `128 / 412` | bestimmt | — |
| `Complete`, `failed == 0` | `Artist photos complete` | `412 / 412` | 100 % türkis | — |
| `Complete`, `failed > 0` | `Artist photos complete` | `397 / 412` | türkis + lila Rest | `15 without a photo` |
| `Paused` | `Waiting for a connection` | letzter Stand | angehalten, **nicht** unbestimmt | — |

Der Entwurf zeigt auf der Settings-Seite in Runde 1 abweichend „Downloading
portraits" und „Continues in the background". Runde 2 und die Vorgabe „identisch
an beiden Stellen" gehen vor: überall dieselben Beschriftungen.

Ausblenden: `Complete` **ohne** Fehlschläge nach 4 Sekunden von selbst; mit
Fehlschlägen bleibt die Leiste bis zum `×`. Das `×` blendet nur aus, der Lauf
läuft weiter und erscheint mit derselben `run_id` nicht wieder.

Läuft nichts, erzeugt das Composable **nichts** — keine leere Höhe, die Liste
rückt nicht.

### 13. Die beiden Einbaustellen

- **Browse:** `BrowseScreen.kt`, in der `Column` ab Zeile 489 zwischen
  `LibrarySummaryActions` (502–508) und dem `HorizontalPager` (514), hinter den
  beiden `BrowseErrorLine`-Zeilen 512/513. Über dem Pager, also auf Titles,
  Artists und Queue gleichermaßen; beim Wischen bleibt sie stehen.
- **Online sources:** `OnlineSourcesSettingsPage.kt`, zwischen dem
  `SettingsSwitchRow` (38–45) und dem Erklärungstext (46–54).

Sichtbare Texte stehen im Kotlin-Code, nicht in `strings.xml` — das ist die
Konvention der App (`R.string` wird nirgends benutzt), hier keine Abweichung.

### 14. Bewegung und Barrierefreiheit

- Ein- und Ausblenden mit `AnimatedVisibility`, `expandVertically` + `fadeIn`
  bzw. `shrinkVertically` + `fadeOut`, 200 ms.
- Balkenbreite über `animateFloatAsState`.
- Balken mit `progressSemantics`, Beschreibung
  `"Artist photos, 128 of 412 downloaded"`.
- `×` mit `contentDescription = "Hide progress"`, Trefferfläche 48dp.
- Keine Benachrichtigung, kein Toast, keine Snackbar.

---

## Verifikation

Rust (`cargo test -p reprise-core -p reprise-android-ffi`), mit eingesetztem
Fetch-Closure statt echtem Netz:

1. `total` zählt weder Künstler mit Foto noch solche mit frischem
   `.notfound`-Marker. **Kontrollarm:** ein Künstler mit abgelaufenem Marker
   *muss* mitgezählt werden — ohne diesen Arm misst der Test nur, dass irgendwas
   gefiltert wird.
2. Nach einem vollständigen Lauf ist die Arbeitsliste des nächsten Laufs leer und
   es wird **keine** Anfrage gestellt (Zähler auf dem eingesetzten Closure).
3. Drosselung: 100 schnelle Schritte erzeugen ≤ 5 Meldungen, **und** die letzte
   Meldung trägt `Complete` mit dem Endstand. Die zweite Hälfte ist die
   eigentliche Prüfung.
4. Kein Rückwärtssprung von `done` über eine ganze Laufserie.
5. Drei `Err` → `Paused`; das erste `Ok` danach → `Running`.
6. **Der teuerste Fehler, direkt geprüft:** ein Künstler, dessen Anfrage mit
   `Err` endete, ist am Ende weder in `done` noch in `failed`, hat **keinen**
   `.notfound`-Marker auf der Platte und wurde erneut versucht.
7. `run_id` wächst bei jedem Start; ein zweiter Start während eines Laufs erzeugt
   keinen zweiten Lauf.
8. Der Lauf sitzt nicht auf `artist_portrait_fetch`: ein Closure, das `Err`
   liefert, erreicht die Zustandsmaschine als `Err` und nicht als `Ok(None)`.

Kotlin (Robolectric, Muster `OnlineSourcesSettingsPageTest.kt`; JDK 21;
`LD_LIBRARY_PATH` **nicht** von Hand setzen — das Skript tut es seit #645, und
von Hand gesetzt entwertet es den Beleg):

9. Je ein Test pro Zustand aus der Tabelle in Aufgabe 12, über `testTag`.
10. Ohne Lauf ist kein Knoten vorhanden — nicht „unsichtbar", sondern nicht da.
11. `×` blendet aus; eine weitere Meldung mit **derselben** `run_id` bringt die
    Leiste nicht zurück, eine mit neuer `run_id` schon.
12. Beide Einbaustellen zeigen dieselbe Leiste mit denselben Beschriftungen.
13. Der Fehlschlag-Zustand zeigt `397 / 412` und `15 without a photo` — die
    Zahlen aus dem Entwurf, nicht `412 / 412`.

Was diese Tests **nicht** zeigen: dass die Verdrahtung zur echten Deezer-Abfrage
steht. Ein eingesetztes Closure ist grün, egal was der echte Weg tut. Dieser
Nachweis gehört an die laufende App gehalten, nicht an die Testsuite — und die
Leiste ist dafür ihr eigenes Messgerät: erscheint sie beim Einschalten des
Toggles sofort und zählt sie hoch, ist der Weg belegt.

---

## Parallelität

Der Schnitt wurde versucht und verworfen.

**Was disjunkt wäre.** Strang A (`crates/reprise-core/src/artist_portrait/**`,
`crates/reprise-android-ffi/src/artist_portrait.rs`, `.../lib.rs`, Aufgaben 1–6)
und Strang B (nur `ArtistPhotoProgressBar.kt` plus Test, Aufgaben 12 und 14
gegen einen strang-eigenen Kotlin-Typ) berühren keine gemeinsame Datei und sind
jeder für sich grün.

**Warum das trotzdem nicht trägt.** Die Verdrahtung — Aufgaben 7–11 und 13,
also ViewModel, `MainActivity`, `BrowseScreen`, Settings-Seite und Auto-Scan —
kann kein dritter Strang sein. Sie braucht die von UniFFI **erzeugten**
Kotlin-Bindungen aus A und das Composable aus B; ein von `dev` abgezweigter
Worktree hat beides nicht, also übersetzt dieser Strang nicht. Die Kopplung
liefe über eine erzeugte Datei, und dort ist eine grüne Disjunktheitsprüfung
kein Beweis: die Dateilisten wären sauber getrennt und der Strang ließe sich
trotzdem nicht allein bauen. Diese Falle ist in diesem Repo schon einmal
gemessen worden.

**Entschieden: ein Strang.** Der Schnitt A|B kauft Wanduhr nur für den kleineren
Teil der Arbeit; die Verdrahtung ist der größte und riskanteste Brocken und
müsste ohnehin auf beide warten — in einem zweiten `/code`-Lauf nach dem Landen,
also mit zwei Landungen, zwei Reviews und einem zweiten Plan. Der Preis
übersteigt den Gewinn.

Keine `strands`- und keine `merge_order`-Schlüssel, keine Strangdateien, keine
nachgelagerten Quervergleiche: es gibt keine Naht, an der etwas verglichen
werden müsste.
