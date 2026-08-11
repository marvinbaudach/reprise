# Android: Queue-Reiter, Suchfokus und Kontextmenüs — Design

Status: bereit zur Review

Basis: `75a24b35a9` (`origin/dev`, 2026-08-10)

## 1. Ziel

Vier Lücken in der Android-App schließen, die beim täglichen Gebrauch stören:

1. Die Suche öffnet sich ohne Fokus — man muss erst ins Feld tippen, bevor man
   tippen kann. Und sie lässt sich nach Aussage des Nutzers nicht wieder
   schließen.
2. Die Warteschlange ist nur über einen Umschalter innerhalb von Now Playing
   erreichbar und damit praktisch versteckt.
3. Es gibt keinerlei Kontextmenü: kein Einreihen, kein Löschen, nichts.
4. Auch die Player-Ansicht bietet keine Möglichkeit, den laufenden Titel
   loszuwerden.

## 2. Ausgangslage (gemessen auf `origin/dev`)

- `BrowseTab` (`BrowseScreen.kt:51`) hat vier Einträge: `TITLES`, `ARTISTS`,
  `ALBUMS`, `FAVOURITES`. Sie werden als `NavigationBar` plus `HorizontalPager`
  gerendert; `libraryDestinations` (`LibraryFramePolicy.kt:37`) leitet sich
  direkt aus `BrowseTab.entries` ab.
- Die Warteschlange existiert bereits als `NowPlayingQueuePage`
  (`NowPlayingQueue.kt`) mit Abspielen, Verschieben und Entfernen. Sie ersetzt
  im Now-Playing-Sheet das Cover, sobald `nowPlayingQueueVisible` gesetzt ist —
  aufgerufen an zwei Stellen (`NowPlayingSheet.kt:145`, `NowPlayingScene.kt:342`).
- `TitleSearchField` (`BrowseTabs.kt:62`) ist ein schlichtes `OutlinedTextField`
  ohne `FocusRequester` und ohne nachgestelltes Icon. Ein Schließen existiert im
  Code sehr wohl: das Lupen-Icon in der Zusammenfassungszeile wechselt zu
  `close` (`LibraryFrame.kt:85–90`).
- Kein einziges `combinedClickable` oder `onLongClick` in der gesamten App.
- Die Android-Warteschlange **ist** bereits `reprise_core::queue::Queue` — der
  Kern hält den Zustand, Media3 ist reines Backend
  (`playback_session/queue_boundary.rs`). Der Kern kennt kein Einfügen an
  Position, nur `append_tracks`.
- `reprise_core::library::trash_tracks_with` erledigt plattformunabhängig die
  Buchführung beim Löschen: der Aufrufer injiziert die Plattform-Löschaktion,
  der Kern prüft den registrierten Pfad und räumt die Datenbank.
- `Queue::remove_ids` rückt den Playhead auf den nächsten überlebenden Titel in
  Abspielreihenfolge vor, wenn der aktuelle Titel entfernt wird.

## 3. Beschlossene Entscheidungen

| # | Frage | Beschluss |
|---|-------|-----------|
| 1 | Was heißt „Löschen"? | Die Datei wird tatsächlich vom Gerät gelöscht. SAF kennt keinen Papierkorb — deshalb immer mit Bestätigungsdialog. |
| 2 | Queue-Reiter vs. Sheet-Umschalter | Der Reiter wird der einzige Ort. Der Umschalter in Now Playing entfällt. |
| 3 | Reichweite des Kontextmenüs | Titel, Favoriten und Alben. Künstler bleibt vorerst außen vor. |
| 4 | Queue-Zeilen | Bekommen dasselbe Long-Press-Menü, mit auf die Queue zugeschnittenen Einträgen. |
| 5 | Einreihen bei leerer Queue | Reiht nur ein. Die Wiedergabe startet nie von selbst. |
| 6 | Ort der Einreih-Logik | Neue Kernmethode in `queue.rs` (Ansatz A). Die Konvergenz von Android auf `up_next::UpNext` ist ein eigenes Vorhaben und **nicht** Teil dieser Arbeit. |
| 7 | Löschen des laufenden Titels | Sprung zum nächsten Titel, die Wiedergabe läuft weiter. |
| 8 | Umfang des Now-Playing-Menüs | Genau ein Eintrag: „Vom Gerät löschen…". |

## 4. Oberfläche und Verhalten

### 4.1 Suche

`TitleSearchField` bekommt einen `FocusRequester`. Beim Öffnen fordern Fokus und
Tastatur in einem `LaunchedEffect` an, sodass man sofort tippt.

Geschlossen wird die Suche auf drei Wegen — bewusst mehrfach, damit es
unabhängig davon greift, warum das vorhandene Schließen beim Nutzer nicht
ankommt:

- ein nachgestelltes Icon im Feld selbst: bei vorhandenem Text `clear` (leert
  nur den Text), bei leerem Feld `close` (schließt die Suche),
- die System-Zurück-Taste über einen `BackHandler`, der nur greift, solange die
  Suche offen ist,
- das bestehende Toggle in der Zusammenfassungszeile bleibt unverändert.

Schließen räumt Suchtext und Tastatur weg; das erledigt `toggleSearch` bereits.
Die IME-Aktion „Suchen" klappt nur die Tastatur ein und lässt die Suche offen.

**Vorgeschalteter Diagnoseschritt.** Die Ligatur `close` steckt nachweislich im
mitgelieferten Font (`material_symbols_rounded.ttf`), und der Code schaltet
korrekt um. Warum der Nutzer trotzdem kein X sieht, ist unbewiesen. Erster
Umsetzungsschritt ist deshalb eine Reproduktion am Emulator mit Screenshot,
bevor an dieser Stelle etwas geändert wird. Sollte sich dabei eine andere
Ursache zeigen (verdeckte Zeile, Rendern der Ligatur, veralteter Build), wird
sie behoben statt umgangen.

### 4.2 Warteschlange als fünfter Reiter

`BrowseTab` bekommt einen fünften Eintrag `QUEUE`. Er zeigt die vorhandene
`NowPlayingQueuePage`, aus dem Sheet herausgelöst. Der Zustand
`nowPlayingQueueVisible` und beide Aufrufstellen entfallen; Now Playing zeigt
wieder ausschließlich das Cover.

Das gespeicherte Startziel (`AndroidLibraryDestinationChoice` im FFI) kennt
weiterhin nur die vier Bibliotheksziele. Queue kommt dort **nicht** hinein — in
eine leere Warteschlange zu starten wäre ein schlechter Empfang nach einem
Neustart. Konkret: `selectDestination(BrowseTab.QUEUE)` lässt das gespeicherte
Ziel unverändert, während der Pager normal umschaltet.

Der Reiter benutzt das reguläre Layout statt des fest verdrahteten
`SurfaceLayout.STACKED`, das die Seite heute im Sheet verwendet. Die Zählzeile
(„12 upcoming tracks") bleibt.

### 4.3 Kontextmenü per langem Drücken

Langes Drücken öffnet ein `DropdownMenu` am Berührungspunkt, begleitet von einem
haptischen Impuls (`LocalHapticFeedback`, `HapticFeedbackType.LongPress`).
Kurzes Tippen behält seine bisherige Bedeutung — realisiert über
`combinedClickable(onClick = …, onLongClick = …)`.

| Ort | Einträge |
|-----|----------|
| Titel, Favoriten | Abspielen · Als nächstes · Hinten anhängen · ─ · Vom Gerät löschen… |
| Album | dieselben vier, angewandt auf alle Titel des Albums in Disc-/Titelreihenfolge |
| Queue-Zeile | Jetzt abspielen · Nach oben · Nach unten · Aus der Queue entfernen |
| Now Playing (Überlauf) | Vom Gerät löschen… |

Zur Genauigkeit: „Abspielen" bedeutet dasselbe wie kurzes Tippen — die
Warteschlange wird durch die Auswahl ersetzt und die Wiedergabe beginnt dort
(heutiges `PlaybackSelection`-Verhalten). „Nach oben" und „Nach unten"
verschieben eine Queue-Zeile um genau eine Position. „Vom Gerät löschen…" auf
einem Album löscht alle Titel dieses Albums.

Einreihen startet unter keinen Umständen die Wiedergabe, auch nicht bei leerer
Warteschlange. Jede erfolgreiche Aktion quittiert über die vorhandene
`TransientMessage` („3 Titel eingereiht").

Die Einträge, der Bestätigungsdialog und die Aufrufe liegen in einer neuen Datei
`TrackContextMenu.kt` und werden von allen vier Orten benutzt. Drei getrennte
Menüimplementierungen würden auseinanderdriften.

### 4.4 Now Playing

Beide Aktionszeilen bekommen einen `more_vert`-Überlauf: die des Sheets
(Sleep-Timer, Herz, Einklappen) und die der Szene (Einklappen, Vollbild). Beide
zeigen dasselbe Menü mit einem Eintrag, damit es nicht vom Zufall abhängt, wo
man es findet.

### 4.5 Löschen

Vor jedem Löschen steht ein Bestätigungsdialog, der Namen und Anzahl nennt und
ohne Beschönigung sagt, dass es endgültig ist. Bestätigt der Nutzer, löscht
Kotlin die Dateien über `DocumentsContract.deleteDocument`, während der Kern die
Buchführung übernimmt.

Löscht man den gerade laufenden Titel, rückt die Wiedergabe auf den nächsten
Titel vor. Überlebt kein Titel mehr, hält sie an.

## 5. Schnitt

### 5.1 Kern (`reprise-core`)

Eine neue Methode in `queue.rs`, die beide Einreih-Richtungen abdeckt:

```rust
/// Wohin ein ausdrücklich eingereihter Titel gehört.
pub enum QueuePlacement { Next, Last }

/// Reiht ausdrücklich vom Nutzer gewählte Titel ein und gibt zurück, wie
/// viele aufgenommen wurden. Startet niemals die Wiedergabe.
///
/// Anders als `append_tracks` belebt dies eine erschöpfte Queue wieder:
/// eine ausdrückliche Nutzeraktion darf nicht wirkungslos verpuffen.
pub fn enqueue(&mut self, new_ids: &[i64], placement: QueuePlacement) -> usize
```

Warum eine eigene Methode und nicht `append_tracks`: dessen Vertrag hält eine
erschöpfte Queue ausdrücklich erschöpft („a removal never resurrects a
position"), und der Desktop verlässt sich darauf. Diesen Vertrag zu ändern,
würde dort Verhalten verschieben, das mit dieser Arbeit nichts zu tun hat.

Die Queue trägt `ids` (die Titel), `order` (Indizes in `ids`) und `pos` (Index in
`order`). `enqueue` hängt die neuen Titel an `ids` an und fügt die zugehörigen
Order-Indizes bei `pos + 1` ein (`Next`) oder am Ende an (`Last`). Die Invariante
`order.len() == ids.len()` bleibt erhalten, `note_sequence_changed()` wird
aufgerufen.

Randfälle, die die Methode selbst behandelt:

- **Leere Queue** (`ids` leer): `pos` wird `Some(0)`, ohne dass etwas startet.
  Der erste eingereihte Titel ist damit der aktuelle — siehe 5.2 dazu, wie er
  trotzdem im Reiter sichtbar bleibt.
- **Erschöpfte Queue** (`ids` gefüllt, `pos == None` nach `Repeat::Off` am
  Ende): `pos` rückt auf den ersten neu eingereihten Titel. Ohne das bliebe die
  Aktion unsichtbar, weil `remaining_window` bei `pos == None` grundsätzlich
  leer zurückgibt.
- **Aktiver Shuffle:** eingefügt wird in die *Abspielreihenfolge*, nicht in
  `ids`. Die neuen Titel kommen also direkt als Nächstes, unabhängig davon, wie
  `order` durchmischt ist.
- **Mehrfaches Einreihen mit `Next`:** ein zweiter Aufruf landet wieder direkt
  hinter dem laufenden Titel, also **vor** dem zuvor Eingereihten. Das
  entspricht der Bedeutung von „als Nächstes" und der `prepend`-Semantik des
  Desktops.
- **Leerer Slice:** No-op, Rückgabe `0`.

`trash_tracks_with` bleibt unverändert; Android ist nur sein erster mobiler
Aufrufer.

### 5.2 FFI (`reprise-android-ffi`)

In `playback_session/queue_boundary.rs`:

```rust
pub fn queue_tracks_next(&self, track_ids: Vec<i64>) -> Result<u32, AndroidPlaybackError>
pub fn queue_tracks_last(&self, track_ids: Vec<i64>) -> Result<u32, AndroidPlaybackError>
```

Beide rufen `Queue::enqueue` mit dem jeweiligen `QueuePlacement` und nehmen
**nur IDs**. Die zugehörigen URIs schlägt der FFI selbst in der
Datenbank nach — die Session hält `track_ids` und `uris` parallel
(`playback_session.rs:160`), und ein aus der Oberfläche durchgereichter Pfad
wäre genau der veraltete Wert, an dem Media3 später hängenbliebe. IDs ohne
auflösbaren Pfad werden übersprungen und zählen nicht in die Rückgabe.

Danach wie bei den bestehenden Queue-Operationen: `persist_queue`, `set_next`
nachziehen, `notify`. Gestartet wird nichts — `start_current` bleibt ungerufen.

**Der Fensteranfang der Queue-Ansicht.** `upcoming_tracks` liest heute ab
`pos + 1`. Reiht man in eine leere Queue drei Titel ein, wird der erste zum
aktuellen und wäre damit unsichtbar: im Reiter nicht enthalten, in Now Playing
nicht dargestellt, weil nichts geladen ist. Der FFI kennt keinen
Lade-ohne-Start-Pfad (`start_current` ruft direkt `play_uri`), und einen zu
bauen, wäre für diese Arbeit deutlich zu viel.

Stattdessen gilt: **solange nichts geladen ist (`current_loaded == false`),
beginnt die Queue-Ansicht beim aktuellen Titel statt dahinter.** Das ist auch
sachlich richtig — ein Titel ist erst dann Gegenwart, wenn er läuft. Tippt man
ihn an, startet er über den bestehenden `play_upcoming_track_now`-Weg.

Diese Fallunterscheidung darf **nicht** zweimal getroffen werden. Sie bekommt
eine gemeinsame Hilfsfunktion, die den Startversatz liefert, und
`upcoming_tracks` wie auch `upcoming_order_position` (also alle Editier- und
Abspieloperationen) benutzen ausschließlich diese. Zwei getrennte Rechnungen
würden bei angehaltener Wiedergabe auf verschiedene Zeilen zeigen — genau die
Art Doppelentscheidung, die in diesem Projekt schon zweimal einen hörbaren
Fehler erzeugt hat.

In `lib.rs`:

```rust
pub fn album_track_ids(&self, album: String, album_artist: String)
    -> Result<Vec<i64>, LibraryError>
```

Ungefenstert, in Disc-/Titelreihenfolge. Ein Album wird über genau dieses Paar
identifiziert (wie `list_album_tracks`), und „Album einreihen" darf nicht davon
abhängen, wie viel die Liste gerade geladen hat.

Neu für das Löschen, mit einem uniffi-Callback-Interface:

```rust
#[uniffi::export(callback_interface)]
pub trait TrashAction: Send + Sync {
    /// Löscht die Datei unter `uri`. Gibt bei Misserfolg die Fehlermeldung
    /// zurück, sonst `None`.
    fn trash(&self, uri: String) -> Option<String>;
}

pub fn trash_tracks(&self, track_ids: Vec<i64>, action: Box<dyn TrashAction>)
    -> Result<AndroidTrashReport, LibraryError>
```

Die Pfade holt der FFI aus der Datenbank, reicht sie an `trash_tracks_with`
weiter und gibt dessen `TrashReport` als uniffi-Record zurück. Anschließend
räumt er die erfolgreich gelöschten IDs über `Queue::remove_ids` aus der
Warteschlange; war der laufende Titel dabei, folgt `start_current()`, bei
erschöpfter Queue ein Stopp.

### 5.3 Kotlin

- `PlaybackControls` und `ActivityPlaybackControls`: `queueTracksNext`,
  `queueTracksLast`.
- `LibrarySession` und `AndroidLibrarySessionPort`: `albumTrackIds`,
  `deleteTracks` (letzteres reicht `DocumentsContract.deleteDocument` als
  `TrashAction` hinein).
- `BrowseTab.QUEUE` plus die herausgelöste Queue-Seite.
- Neue Datei `TrackContextMenu.kt`, angebunden in `LibraryTrackRows.kt`, im
  Album-Grid, in der Queue-Seite und in beiden Now-Playing-Aktionszeilen.
- `TitleSearchField`: `FocusRequester`, nachgestelltes Icon, `BackHandler`.

### 5.4 Datenfluss

```
Long-Press → TrackContextMenu → PlaybackControls/LibrarySession
  → AndroidPlaybackSession (FFI)
  → reprise_core::queue::Queue
  → persist_queue + set_next (Media3) + notify
  → PlaybackUiState → Queue-Reiter lädt sein Fenster neu
```

Antworten laufen über `ApplicationLooperDispatch` zurück auf den App-Looper,
wie die bestehenden Queue-Operationen.

## 6. Fehlerbehandlung

- Jede FFI-Operation liefert ein `Result`. Fehler landen in einer sichtbaren
  Zeile (`BrowseErrorLine`) oder einer `TransientMessage` — nie stillschweigend
  verschluckt.
- Einreihen mit null aufgenommenen Titeln meldet das ausdrücklich, statt Erfolg
  vorzutäuschen.
- Löschen meldet Teilerfolge als Teilerfolge: „2 von 12 konnten nicht gelöscht
  werden", mit den Fehlermeldungen aus dem `TrashReport`.
- Die Queue-Bearbeitung bleibt identitätsgeschützt über `expected_track_id`. Die
  neuen Menüeinträge gehen denselben Weg: eine veraltete Ansicht führt zu
  „nichts passiert" samt Neuladen, nicht zum falschen Titel.
- Verweigert SAF die Löschberechtigung für eine Datei, bleibt der
  Datenbankeintrag bestehen — der Kern entfernt nur, was wirklich weg ist.

## 7. Tests

**Kern (Rust).** `enqueue` in beiden Richtungen gegen leere Queue, erschöpfte
Queue (die Position wird wiederbelebt), aktiven Shuffle, mehrfaches Einreihen
mit `Next` hintereinander (das zweite landet vor dem ersten), Duplikate, leeren
Slice; und dass sich `sequence_identity` ändert. Ein Test hält ausdrücklich
fest, dass `append_tracks` sein bisheriges Verhalten bei erschöpfter Queue
behält — sonst verschiebt sich unbemerkt Desktop-Verhalten.

**FFI (Rust).** In `queue_boundary_tests.rs`: eingereihte Titel erscheinen
sofort in `upcoming_tracks`, überleben die Persistenz, `set_next` stimmt danach;
IDs ohne auflösbaren Pfad werden übersprungen; Einreihen startet nichts. Für den
Fensteranfang: bei angehaltener Wiedergabe enthält `upcoming_tracks` den
aktuellen Titel, und `play_upcoming_track_now(0)` startet genau diesen — der
Test deckt damit ab, dass Ansicht und Editieroperationen dieselbe Rechnung
benutzen. Für `trash_tracks` eine absichtlich scheiternde Aktion, die den
Teilerfolg-Pfad abdeckt, sowie der Fall „laufender Titel gelöscht → nächster
läuft".

**Kotlin (Robolectric/Compose).** Fünf Reiter in `MobileBottomTabsTest`; Queue
als Reiter erreichbar und der Sheet-Umschalter verschwunden; Long-Press öffnet
das Menü; Einreihen startet nichts; der Löschdialog erscheint vor dem Löschen
und ein Abbruch löscht nichts; die Suche hat nach dem Öffnen den Fokus; die
Zurück-Taste schließt sie.

**Zwei Fallen, die hier schon getäuscht haben und deshalb hart gegengemessen
werden:** die Robolectric-Suite läuft nur unter **JDK 21** (Systemstandard ist
26; unter 26 bricht sie im Teardown ab und sieht wie ein eigener Fehler aus),
und „BUILD SUCCESSFUL" wird gegen die tatsächliche Suiten- **und** Testanzahl
geprüft, nicht geglaubt.

**Sichtprüfung am Emulator.** Das X-Rätsel aus 4.1, fünf Einträge im
`NavigationBar`, die Menü-Optik und der Löschdialog.

## 8. Risiken

- **Fünf Einträge sind das Maximum** einer Material3-`NavigationBar`. Die
  Beschriftungen müssen kurz bleiben; auf schmalen Geräten ist das Layout am
  Anschlag und wird am Bild geprüft.
- **Der Symbolname für den Queue-Reiter** muss visuell verifiziert werden. Eine
  Ligaturprüfung im Font war nachweislich unvollständig — sie fand `close`, aber
  auch `play_arrow` nicht, das offensichtlich funktioniert. Der Name wird also
  am gerenderten Bild bestätigt, nicht aus der Fonttabelle geschlossen.
- **Endgültiges Löschen ist die scharfe Kante dieser Arbeit.** Der
  Bestätigungsdialog ist kein Beiwerk, sondern Teil der Funktion; er wird
  getestet, einschließlich des Abbruchpfads.

## 9. Ausdrücklich nicht Teil dieser Arbeit

- Die Konvergenz von Android auf `up_next::UpNext` (Ansatz B aus der
  Vorüberlegung). Sie erfüllt QUE-1/QUE-6 wörtlich, reißt aber Persistenz,
  Shuffle, Repeat und die Media3-Kopplung auf — ein eigenes Vorhaben.
- Ein Kontextmenü im Künstler-Reiter.
- Suche über andere Reiter als Titel.
- Queue als speicherbares Startziel.
