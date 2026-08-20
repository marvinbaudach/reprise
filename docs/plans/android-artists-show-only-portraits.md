---
slug: android-artists-show-only-portraits
worktree: /home/marvin/Projects/reprise-android-artists-show-only-portraits
branch: feature/android-artists-show-only-portraits
phase: reviewed
codex_session:
created: 2026-08-20
related: android-artist-portrait-before-album-cover, android-artist-photos
---

# Android: Beim Interpreten steht ein Interpretenbild oder gar keins

Beschluss des Nutzers vom 20.08.2026, wörtlich:

> „es darf kein wechsel des bildes kommen. bei artisten nur artist-bilder.
> ansonsten keins"
>
> „kann man nicht beim scannen im hintergrund noch die album- und
> artist-covers ziehen"

Der Befund, aus dem das kommt, steht in
`docs/plans/android-artist-portrait-before-album-cover.md`. Dieser Plan ersetzt
dessen offene Lösungsrichtungen.

Gelesen gegen `origin/dev` @ `afb839069e`. Jede Zeilenangabe stammt aus diesem
Stand. Der lokale Hauptcheckout hängt hinterher — wer dort prüft, prüft das
Falsche.

## Der Kern in einem Satz

**Der Bildwechsel verschwindet nicht dadurch, dass wir schneller laden, sondern
dadurch, dass es nichts mehr zu wechseln gibt.** Fällt der Album-Cover-Rückfall
im Interpretenkontext weg, hat eine Interpretenfläche nur noch zwei Zustände:
den erzeugten Avatar (ein Bild von nichts) und das Porträt. Ein Avatar, der zum
Porträt wird, ist kein Bildtausch — ein Album-Cover, das zum Porträt wird, ist
einer. Deshalb ist **PR 1 der eigentliche Fix**, und der Vorlauf aus PR 2/3
macht nur die Avatar-Phase kurz.

Daraus folgt auch, was *nicht* gebaut wird: kein Einfrieren der Zeile, kein
Unterdrücken des Nachladens. Beides wäre nötig, wenn wir den Album-Cover-
Rückfall behielten, und beides würde die Erstanzeige nach einem frischen Scan
kaputtmachen — der In-Memory-`ArtworkCache` ist beim App-Start leer, die erste
Auflösung *muss* also nachziehen dürfen.

## Was heute passiert (gemessen)

| Stelle | Datei @ `afb839069e` | Verhalten |
| --- | --- | --- |
| Listenzeile | `android/app/src/main/java/de/reprise/spike/BrowseTabs.kt:601-606` | `allowFetch = false`, `AndroidArtworkSize.LIST` — die Liste holt nie |
| Detailkopf | `.../BrowseTabs.kt:333-338` | `allowFetch = true`, `AndroidArtworkSize.ARTIST_DETAIL` — holt beim Öffnen |
| Auflösung | `.../TrackCover.kt:149-177` (`resolveVisual`) | Zeile **167**: `?: resolve(request.trackUri, request.size)?.let(decode)` — **hier** kommt das Album-Cover in die Interpretenfläche |
| Tausch | `.../TrackCover.kt:163-164`, `:307-308` | `refreshesArtistPortrait() = kind == ARTIST && allowFetch` → `cache.invalidateArtistArtwork` ersetzt das gecachte Bild |
| Schlüssel | `.../ArtworkCache.kt:9-20`, `:123-129` | `kind` steckt im Schlüssel *und* in `matchesIdentity` — eine ARTIST-Anfrage kann sich also **kein** TRACK-Bild borgen. Das gecachte Album-Cover unter `kind = ARTIST` entsteht ausschließlich über Zeile 167. |
| Gate | `crates/reprise-android-ffi/src/artist_portrait.rs:43-57` | `network_allowed_or_off(&reader, ARTWORK_MODULE)`, sitzt schon richtig — nur in Rust |
| Drossel | `crates/reprise-core/src/artist_portrait/deezer.rs:12` | `MIN_REQUEST_INTERVAL = 300 ms`, im Modul selbst. **Ein Vorlauf braucht keine eigene Drossel.** |
| TTL | `crates/reprise-core/src/artist_portrait/cache.rs:11-12` | positiv 30 Tage, negativ 7 Tage, `.notfound`-Marker. **Ein zweiter Durchlauf hämmert nicht nach.** |

Zwei Befunde, die den Zuschnitt bestimmen:

1. **Auf Android gibt es überhaupt keinen Album-Cover-Download.**
   `crates/reprise-core/src/cover_download.rs` (MusicBrainz → Cover Art Archive)
   hat genau einen Aufrufer: `crates/reprise-gnome/src/ui/cover/cover_download_worker.rs`.
   In `crates/reprise-android-ffi/` kommt `cover_download` nicht vor;
   Album-Artwork stammt dort ausschließlich aus Datei/Tag (`lib.rs:281
   track_artwork`). „Beim Scannen auch Album-Cover ziehen" ist auf dem Handy
   also kein Vorziehen eines vorhandenen Wegs, sondern ein neuer Strang. Er
   steht **nicht** in diesem Plan (siehe [Nicht in diesem Plan](#nicht-in-diesem-plan)).
2. **Kein offener Branch fasst `android/`, `reprise-android-ffi` oder
   `artist_portrait/` an** (geprüft über alle `refs/remotes/origin` vom
   20.08.2026). Das Rebase-Kopfrisiko des Mutterplans besteht diesmal nicht.

## Regeln für den Umsetzer — zuerst lesen

**Pro Aufgabe zuerst der Test, dann der Code.** Jede Aufgabe nennt die
Testnamen. Ein Test, der beim ersten Lauf grün ist, hat nichts gemessen — er
muss rot sein, bevor der Code entsteht, und das Rot gehört ins Protokoll.

**Kein Gerät, kein `adb`, kein Emulator** — außer in der letzten Aufgabe.

**Harte Umgebungsfakten** (gemessen, nicht verhandelbar):

- Die Android-Suite braucht **JDK 21** (`JAVA_HOME=/usr/lib/jvm/java-21-openjdk`
  vor *jedem* Gradle-Aufruf). Der Systemstandard JDK 26 killt Robolectric.
- Die FFI-Tests hängen an der `readdir`-Reihenfolge; sie laufen mit
  `TMPDIR=/tmp` grün.
- `BUILD SUCCESSFUL` ist kein Beweis. Gradle meldet `:app:testDebugUnitTest`
  als up-to-date und führt nichts aus. Das Urteil steht in
  `android/app/build/test-results/testDebugUnitTest/*.xml`;
  `scripts/check-android-suite.sh:40-76` vergleicht deren `mtime` mit dem Startzeitpunkt des Laufs und endet mit 2, wenn sie älter sind.
- Nach jeder Änderung an `crates/reprise-android-ffi/**` müssen die
  UniFFI-Bindings unter `android/app/src/main/java/uniffi/` neu erzeugt werden
  (`scripts/check-android-suite.sh:146-151`), sonst kennt Kotlin die neue
  Methode nicht. **Daraus folgt die Reihenfolge PR 2 vor PR 3.**
- Lange Läufe in eine Datei umleiten, nicht auf die Konsole. Unten steht `$LOG`
  für ein Arbeitsverzeichnis außerhalb des Repos, etwa
  `/tmp/reprise-artist-portraits`.

## Entscheidungen, die dieser Plan fällt

1. **Der Rückfall verschwindet nur für `kind == ARTIST`.** `TrackCover.kt:167`
   bleibt für Tracks und Alben unverändert; die Verzweigung kommt davor. Kein
   Umbau von `resolveVisual`, ein `if`.
2. **Der Avatar ist der Nichtzustand, nicht ein Bild.** `generatedVisual`
   (`TrackCover.kt:179-193`) erzeugt ihn bereits aus Name + Größe
   (`fallbackSizePx()` liefert für `ARTIST_DETAIL` 640 px). Es wird kein neues
   Platzhalter-Design gebaut. Wenn der Nutzer den Avatar später anders haben
   will, ist das ein eigener Vorgang.
3. **Nachladen bleibt erlaubt.** Avatar → Porträt ist die eine zulässige
   Änderung; sie ist der Grund, warum überhaupt je ein Porträt erscheint.
   `refreshesArtistPortrait()` und `invalidateArtistArtwork` bleiben, können
   nach PR 1 aber nur noch einen Avatar ersetzen, nie ein fremdes Album.
   *Falls der Nutzer auch das nicht will, ist die Gegenrichtung eine Zeile in
   `rememberArtistArtworkVisual` — dann zeigt eine Fläche bis zum nächsten
   Betreten den Avatar. Nicht ohne ausdrückliche Ansage bauen.*
4. **Die Frage „wer braucht noch ein Porträt" wird genau einmal beantwortet, in
   Rust.** Der Vorlauf fragt Rust nach der Namensliste; Kotlin leitet nie selbst
   aus Dateien oder Zuständen ab, welcher Interpret dran ist. Dieselbe
   Entscheidung an zwei Stellen ist der Fehler, den dieses Projekt schon einmal
   bezahlt hat.
5. **Die Frischeprüfung wird aus `load_or_fetch_with` herausgezogen, nicht
   nachgebaut.** Heute steht sie in `artist_portrait/mod.rs:115-124`. Sie wird
   zu einer Funktion in `cache.rs`, die *beide* Aufrufer benutzen. Ein zweiter
   TTL-Vergleich neben dem ersten wäre genau die Drift, die dieser Plan
   vermeidet.
6. **Bei ausgeschaltetem Online-Schalter gibt die Namensliste leer zurück.** So
   bleibt die Schleife in Kotlin trivial und das Gate bleibt an einer Stelle.
   Der Vorlauf läuft dann null Mal statt 151 Mal ins `None`.
7. **Kein WorkManager.** Das Projekt hat ihn nicht (geprüft: kein
   `androidx.work` im gesamten `android/`-Baum), und der Scan läuft heute auf
   einem nackten `Thread` (`MainActivity.kt:497-513`). Der Vorlauf bekommt einen
   Einzelthread-Executor nach demselben Muster wie
   `TrackCover`s Ladeschlange. Bedingungen wie „nur im WLAN" gehören in einen
   eigenen Vorgang, zusammen mit dem Album-Download.

## PR 1 (`ui`) — Beim Interpreten nie ein Album-Cover

Fasst nur `android/` an, kein Rust, keine Bindings. **Ships den eigentlichen
Fix; kann allein landen.**

### Aufgabe 1.1 — Die Auflösung fällt für Interpreten auf den Avatar zurück

*Test zuerst.* In `android/app/src/test/java/de/reprise/spike/ArtistArtworkTest.kt`
wird `anArtistWithoutAPortraitFallsBackToTheAlbumCover:129` zu
`anArtistWithoutAPortraitFallsBackToTheGeneratedAvatar`: dieselbe Vorbereitung,
aber die Erwartung kehrt sich um — der Auflöser darf `resolve(trackUri, …)` für
eine ARTIST-Anfrage **gar nicht erst aufrufen**. Zählen, nicht nur das Bild
vergleichen: ein Zähler auf dem `resolve`-Doppelgänger, der 0 bleiben muss.

*Dann der Code.* In `TrackCover.kt:167` den Rückfall verzweigen: ist
`request.kind == ArtworkKind.ARTIST`, entfällt `resolve(request.trackUri, …)`
und es geht direkt in `generatedVisual(request, resolved = true)`.

### Aufgabe 1.2 — Die Oberfläche zeigt es auch so

*Test zuerst.* In `ArtistPortraitSurfaceTest.kt`:

- `anArtistWithoutAPortraitShowsTheAlbumCover:77` → `…ShowsTheGeneratedAvatar`
- `aClosedSwitchLeavesTheDetailHeadOnTheAlbumCover:249` →
  `aClosedSwitchLeavesTheDetailHeadOnTheGeneratedAvatar`
- `aPortraitFetchedInDetailReplacesTheRowsCachedAlbumCover:161` →
  `aPortraitFetchedInDetailReplacesTheRowsAvatar` — der Tausch bleibt erlaubt,
  aber was ersetzt wird, ist jetzt der Avatar.

**Diese drei Umbenennungen sind Pflicht, nicht Kosmetik.** Ein Test, der weiter
`ShowsTheAlbumCover` heißt, schützt die verworfene Regel und wird beim nächsten
Lesen für die geltende gehalten.

*Neu dazu:* `anArtistSurfaceNeverShowsATrackVisual` — eine ARTIST-Anfrage und
eine TRACK-Anfrage auf **derselben** `representativeUri`; das Bild der einen
darf nie im Slot der anderen landen. Das pinnt `ArtworkCache.kt:123-124`
(`kind` in `matchesIdentity`) fest, das heute nur zufällig mitschützt.

*Bleibt grün, ohne Änderung:* `scrollingTheArtistListNeverFetches:135`,
`aRowResolutionNeverCallsTheFetcher:69`,
`openingAnArtistFetchesExactlyOnceForThatArtist:202`,
`theArtistRowShowsACachedPortrait:48`. Wenn eine davon rot wird, ist der Eingriff
zu groß geraten.

### Abnahme PR 1

```
JAVA_HOME=/usr/lib/jvm/java-21-openjdk TMPDIR=/tmp \
  scripts/check-android-suite.sh > $LOG/pr1.log 2>&1
```
Urteil aus `android/app/build/test-results/testDebugUnitTest/*.xml`, nicht aus
der letzten Zeile.

## PR 2 (`core`) — Rust weiß, wer noch ein Porträt braucht

Fasst nur `crates/` an.

### Aufgabe 2.1 — Die Frischeprüfung bekommt einen Namen

*Test zuerst.* In `crates/reprise-core/src/artist_portrait/` ein Test, der für
vier Lagen die Antwort festhält: frisches Bild → kein Bedarf; frischer
`.notfound`-Marker → kein Bedarf; abgelaufener Marker → Bedarf; nichts da →
Bedarf. Grenzwerte über `now` steuern, nie über echte Dateizeiten warten.

*Dann der Code.* `cache::needs_fetch(dir, name, now) -> bool` (oder ein
`CacheVerdict`-Enum, wenn der stale-Pfad sauberer wird) aus
`mod.rs:115-125` herausziehen und **`load_or_fetch_with` auf dieselbe Funktion
umstellen**. Ein Mutationsnachweis gehört dazu: wer die neue Funktion
verfälscht, muss *beide* Aufrufer rot sehen — sonst hängt einer noch am alten
Code.

### Aufgabe 2.2 — Die Brücke gibt die Namen heraus

*Test zuerst.* In `crates/reprise-android-ffi/src/artist_portrait.rs` (Testblock
am Dateiende, Muster der bestehenden Tests dort):

- `artists_missing_portraits_skips_those_already_cached`
- `artists_missing_portraits_returns_nothing_when_the_switch_is_off`
- `artists_missing_portraits_respects_its_limit`
- `artists_missing_portraits_never_touches_the_network` — der
  Porträt-Doppelgänger darf 0-mal gerufen werden

*Dann der Code.* Neue `#[uniffi::export]`-Methode auf `MusicLibrary`, in
derselben Datei, mit eigenem `impl`-Block:

```rust
pub fn artists_missing_portraits(&self, limit: u32) -> Result<Vec<String>, LibraryError>
```

- liest das Gate zuerst (`network_allowed_or_off(&reader, ARTWORK_MODULE)`) und
  gibt bei „aus" ein leeres `Vec` zurück (Entscheidung 6),
- holt die Interpretennamen über `queries::query_artists(&reader, "", window)`
  in Bibliotheksreihenfolge — dieselbe Reihenfolge, die die Liste zeigt, damit
  der Vorlauf oben anfängt, wo der Nutzer hinsieht,
- filtert mit `cache::needs_fetch` gegen `self.portrait_dir()`,
- schneidet bei `limit` ab.

`Result`, nicht `Option`: die Methode fasst die Datenbank an, und ein
vergifteter Handle darf nicht als „niemand braucht was" durchgehen — dieselbe
Trennung, die `lib.rs:260-275` für `track_artwork` begründet.

### Abnahme PR 2

```
TMPDIR=/tmp cargo test -p reprise-core -p reprise-android-ffi \
  > $LOG/pr2.log 2>&1; grep -c '^test result: FAILED' $LOG/pr2.log
```

## PR 3 (`ui`) — Der Vorlauf füllt den Zwischenspeicher nach dem Scan

Braucht die Bindings aus PR 2. **Beginnt mit einer Binding-Erzeugung.**

### Aufgabe 3.1 — Eine Warteschlange, die man anhalten kann

*Test zuerst.* Neue Datei
`android/app/src/test/java/de/reprise/spike/ArtistPortraitPrefetchTest.kt`,
gegen einen `LibrarySessionPort`-Doppelgänger (Muster:
`BrowseSurfaceTest.kt:638-640`):

- `thePrefetchFetchesEveryNameTheBridgeReports`
- `thePrefetchFetchesEachArtistOnlyOnce`
- `anEmptyListEndsThePrefetchWithoutFetching` (deckt den ausgeschalteten
  Schalter mit ab — Rust liefert dann leer)
- `shutdownStopsThePrefetchBeforeTheNextFetch`
- `aFailingFetchDoesNotStopTheRest`

*Dann der Code.* `ArtistPortraitPrefetch.kt`: ein Einzelthread-Executor, eine
`@Volatile`-Stoppflagge, eine Schleife, die Namen in Blöcken (`limit = 32`)
abholt und je Name `artistPortraitFetched(name, AndroidArtworkSize.LIST)` ruft,
bis die Brücke leer antwortet. **Keine eigene Drossel** — `deezer.rs:12` bremst
schon, und eine zweite Bremse an anderer Stelle wäre wieder dieselbe
Entscheidung an zwei Orten.

Die Größe ist `LIST`, weil die Liste der Ort ist, an dem der Avatar sonst
stehenbleibt; `ARTIST_DETAIL` wird beim Öffnen aus derselben Datei verkleinert
(`reduced_portrait_path`), kostet also keinen zweiten Netzzugriff.

### Aufgabe 3.2 — Angehängt an Scan und Start

*Test zuerst.* In `LibraryScreenStateTest.kt` (dort liegt der Port-Doppelgänger
schon, `:473-478`):

- `aFinishedScanStartsThePrefetch`
- `thePrefetchNeverRunsInsideTheScan` — der Vorlauf darf erst nach
  `port.scan(...)` beginnen; der Scan hält den Writer, und der Vorlauf soll den
  Rückweg in die Bibliotheksansicht nicht verzögern
- `restoringAnExistingLibraryStartsThePrefetch` — für Bibliotheken, die vor
  diesem Plan gescannt wurden

*Dann der Code.* Start in `LibrarySession.scanTree` **nach** dem Leeren von
`artworkPaths` (`LibrarySession.kt:236-242`) und im Wiederherstellungspfad;
`shutdown()` neben `artworkDelegate.value.shutdown()` in
`MainActivity.onDestroy` (`MainActivity.kt:468-475`).

### Aufgabe 3.3 — Am Gerät nachsehen

Die einzige Aufgabe mit Emulator/Gerät. Sie erledigt zugleich die offene
Sichtprüfung aus `docs/plans/android-artist-photos-task10.HANDOFF.md`.

Aufzunehmen, mit Bild:

1. Frisch gescannte Bibliothek, Interpretenliste sofort nach dem Scan: Avatare,
   **kein einziges Album-Cover**.
2. Dieselbe Liste eine Minute später: Porträts, ohne dass jemand einen
   Interpreten geöffnet hat.
3. Drei bis vier Interpreten öffnen und zurück: der Detailkopf zeigt dasselbe
   Bild wie die Zeile, vorher wie nachher.
4. Online-Schalter aus, App neu starten: Avatare, keine Netzanfrage, kein
   Album-Cover.

**Punkt 3 beantwortet zugleich die offene Frage (a)/(b) aus dem Vorgängerplan:**
zeigt der Kopf ein *fremdes Gesicht* statt eines Avatars oder des richtigen
Porträts, liegt der Fehler in der Deezer-Auswahl (Memory
*reprise-deezer-portrait-placeholders*), nicht in der Reihenfolge — dann ist ein
eigener Vorgang fällig, und der Vorlauf aus PR 3 macht ihn dringender, weil er
ein falsches Gesicht bibliotheksweit einlagert statt einzeln.

### Abnahme PR 3

Wie PR 1, plus die Aufnahmen aus 3.3 im Übergabebericht.

## Nicht in diesem Plan

- **Album-Cover-Download auf Android.** Existiert dort nicht (Befund 1). Er
  wäre: `cover_download` an die Brücke, MusicBrainz-Drossel (1 Anfrage/s,
  eigener User-Agent), CAA-Bilder sind groß — das gehört hinter einen eigenen
  Schalter mit Netz-/Akkubedingungen und in einen eigenen Plan. Der Bug, um den
  es hier geht, wird davon nicht berührt: nach PR 1 taucht im Interpretenkontext
  ohnehin kein Album-Cover mehr auf.
- **Der Desktop.** Dort holt die Interpretenansicht beim Ansehen; ob sie
  denselben Vorlauf bekommt, ist eine eigene Frage.
- **Ein neues Platzhalter-Design** (Entscheidung 2).
- **Bedingungen wie „nur im WLAN"** (Entscheidung 7).
