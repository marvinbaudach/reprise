---
slug: android-artist-photos-ui
worktree: /home/marvin/Projects/reprise-android-artist-photos-ui
branch: feature/android-artist-photos-ui
phase: planned
codex_session:
created: 2026-08-14
---
# Android: Künstlerfotos — Strang `ui` (PR 2): Oberfläche

Strangdatei zu `docs/plans/android-artist-photos.md`. Dort stehen die Bindung an
die Spec, die Regeln für den Umsetzer, die fünf selbstgefällten Entscheidungen,
die Testkommandos und die Merge-Ordnung. Diese Datei wiederholt sie nicht und
enthält nur die Aufgaben.

## Vorbedingung — nicht verhandelbar

**PR 1 (`docs/plans/android-artist-photos-core.md`) MUSS gelandet sein, bevor
hier irgendetwas beginnt.** Nicht „fast fertig", nicht „im Review" — gelandet
auf `dev`, und die Android-Gates auf `dev` danach grün. Der Grund: die
UniFFI-Bindings unter `android/app/src/main/java/uniffi/` sind generiert und
gitignoriert (`scripts/check-android-suite.sh:146-151`). Ohne den gelandeten
Rust-Teil kennt Kotlin `artistPortraitCached`, `artistPortraitFetch` und
`onlineSourcesEnabled` nicht und übersetzt schlicht nicht. Ein Worktree, der
parallel zu `core` läuft, misst nichts — er erzeugt die Bindings aus einem
anderen Rust-Stand.

**Erster Handgriff in diesem Strang, vor Aufgabe 1:**

```sh
git fetch origin dev && git rebase origin/dev      # muss core enthalten
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/check-android-suite.sh > $LOG/android-baseline.log 2>&1
grep -E '^suites=' $LOG/android-baseline.log       # failures=0 errors=0 verdict=fresh
```

Das erzeugt die Bindings neu und beweist zugleich, dass die Ausgangslage grün
ist. Ein rotes Baseline-Log ist kein Grund weiterzumachen, sondern ein Grund
nachzusehen, wer es rot gemacht hat.

`$LOG` ist ein Arbeitsverzeichnis außerhalb des Repos, etwa
`/tmp/reprise-artist-photos`.

---

## Aufgabe 1 — `AndroidArtworkSize::ArtistDetail`

**Dateien:** `crates/reprise-android-ffi/src/library_types.rs:59-73`,
`crates/reprise-android-ffi/src/artist_portrait.rs` (Testblock).

Diese Aufgabe ist Rust, obwohl sie im Kotlin-Strang liegt. Sie steht hier, weil
ein neuer Enum-Zweig die drei `when`-Blöcke in `TrackCover.kt` unvollständig
macht und damit den Android-Build rot — in PR 1 hätte das `dev` zwischen den
Landungen rot gefärbt (siehe Mutterplan). Hier landet der Zweig zusammen mit
seinen `when`-Zweigen aus Aufgabe 2.

**Änderung:** Dritter Zweig `ArtistDetail` im `uniffi::Enum`, in
`thumbnail_size()` auf `ThumbnailSize::MobilePortrait` abgebildet. Der
Doc-Kommentar bei `library_types.rs:59` („Die zwei gemessenen Android-Slots")
wird auf drei korrigiert.

**Test zuerst** (neu: `crates/reprise-android-ffi/src/artist_portrait_tests.rs`,
eingehängt per `#[cfg(test)] #[path = ...] mod` in `artist_portrait.rs`, oder
zunächst als `#[cfg(test)] mod tests` in `library_types.rs`):

- `android_artwork_sizes_map_to_the_three_measured_rungs` — `List → MobileList`
  (168), `NowPlaying → MobileFull` (1092), `ArtistDetail → MobilePortrait` (640),
  über `pixels()` verglichen, nicht über den Enum-Namen.
- `a_portrait_is_never_requested_at_the_now_playing_rung` — **der aus PR 1
  verschobene Test**, wörtlich aus dessen Aufgabe 5 übernommen: derselbe Aufbau
  wie `net_1a_an_open_gate_calls_the_fetcher_once_and_returns_the_reduced_file`,
  aber mit `AndroidArtworkSize::ArtistDetail`; Pfad endet auf `-640.png`, und
  kein Pfad aus beiden Bildmethoden endet je auf `-1092.png`. Er konnte in PR 1
  nicht existieren, weil es `ArtistDetail` dort noch nicht gab.

**Fertig, wenn:**
`TMPDIR=/tmp cargo test --locked -p reprise-android-ffi > $LOG/ffi.log 2>&1`
grün ist und beide Namen im Log stehen.

**Bekannte Folge:** Ab hier ist die Kotlin-Übersetzung rot — `TrackCover.kt:72`,
`:111` und `:285` sind `when`-Blöcke über `AndroidArtworkSize` und damit
unvollständig. Aufgabe 2 schließt das. Wer zwischendurch die Android-Suite
fahren will, macht Aufgabe 2 vorher. Danach einmal die Bindings erzeugen (die
zwei `cargo`-Zeilen aus `scripts/check-android-suite.sh:145-151`), damit Kotlin
den neuen Zweig kennt.

---

## Aufgabe 2 — Kotlin: der zweite Auflösungsweg

**Dateien:**
`android/app/src/main/java/de/reprise/spike/ArtworkRequestGate.kt:11-16`,
`.../ArtworkCache.kt:9-18,77-83`,
`.../TrackCover.kt:39-49,72-75,111-114,143-155,285-288`,
neu `.../ArtistCover.kt`.

**Änderung — bewusst additiv, kein Umbau:**

- `ArtworkRequestGate.kt`: neues `internal enum class ArtworkKind { TRACK, ARTIST }`;
  `ArtworkRequest` bekommt `val kind: ArtworkKind = ArtworkKind.TRACK`,
  `val artistName: String = ""` und `val allowFetch: Boolean = false`. Alle drei
  mit Vorgabe, damit jede bestehende Aufrufstelle unverändert übersetzt.
  `begin(...)` bekommt dieselben drei Parameter mit denselben Vorgaben.
- `ArtworkCache.kt`: `TrackArtworkKey` und `GeneratedArtworkKey` bekommen
  `kind`; `trackKey()` und `generatedKey()` reichen `kind` durch. Damit kann ein
  Interpret, der heißt wie eine `trackUri`, deren Bild nicht mehr erben.
- `TrackCover.kt`: zwei neue Konstruktorparameter mit Vorgabe
  `{ _, _ -> null }`:
  `resolveArtistPortraitCached: (String, AndroidArtworkSize) -> String?` und
  `resolveArtistPortraitFetched: (String, AndroidArtworkSize) -> String?`.
  `resolveVisual` (`:143-155`) verzweigt auf `request.kind`: bei `ARTIST` erst
  der Porträtpfad (`allowFetch` entscheidet, welche der beiden Funktionen), bei
  `null` der bestehende `resolve(request.trackUri, request.size)` mit der
  `representativeUri`, bei `null` weiter in `generatedVisual`. Die drei
  `when`-Blöcke (`:72-75`, `:111-114`, `:285-288`) bekommen je einen Zweig
  `ArtistDetail` — Lane `fullSizeWorker`, Fallbackgröße 640.
- Neu `ArtistCover.kt`: `rememberArtistArtworkVisual(name, representativeUri,
  artworkSize, allowFetch)` als Zwilling von
  `rememberTrackArtworkVisual` (`TrackCover.kt:236-257`) — dieselbe
  `DisposableEffect`/`gate.invalidate`-Mechanik ist der von der Spec verlangte
  Generationszähler, sie muss nicht neu erfunden werden. Dazu
  `ArtistAvatar(visual, sizeDp)` (runde Form über `ArtworkCover(..., shape =
  CircleShape)`) und `ArtistPortraitHeader(visual, artist)`.

**Test zuerst:**

- `ArtworkCacheTest.kt`: `anArtistNamedLikeATrackUriDoesNotInheritItsCover` —
  gleicher Schlüsseltext, einmal `TRACK`, einmal `ARTIST`, gleiche Größe; das
  zweite `artwork(...)` gibt `null`.
- neu `ArtistArtworkTest.kt` (Muster: `ArtworkCompositionTest.kt`, jeweils mit
  **eigener** `ArtworkCache()` — die Vorgabe `SharedArtworkCache`
  (`ArtworkCache.kt:92`) ist prozessweit und blutet zwischen Robolectric-Tests):
  - `aRowResolutionNeverCallsTheFetcher` — `allowFetch = false`, der
    Fetch-Zähler bleibt 0, der Cached-Zähler wird 1.
  - `aDetailResolutionCallsTheFetcherExactlyOnce`
  - `anArtistWithoutAPortraitFallsBackToTheAlbumCover` — Porträt `null`,
    `resolve` liefert einen Pfad, `decode` ein bekanntes Bitmap; genau dieses
    kommt an.
  - `anArtistWithoutEitherGetsTheGeneratedCover` — beide `null`,
    `fallback`-Lambda wird mit dem Interpretennamen gerufen.

**Fertig, wenn:**
`JAVA_HOME=/usr/lib/jvm/java-21-openjdk android/gradlew --project-dir android
:app:testDebugUnitTest --tests 'de.reprise.spike.ArtistArtworkTest' --tests
'de.reprise.spike.ArtworkCacheTest' > $LOG/android-2.log 2>&1` grün **und** die
frisch geschriebenen XML-Dateien unter
`android/app/build/test-results/testDebugUnitTest/` zeigen die erwarteten
`tests=`/`failures=` — die Konsolenzeile allein zählt hier nicht.

---

## Aufgabe 3 — Kotlin: die Brücke durchreichen

**Dateien:** `.../LibrarySession.kt:13-62,196-214`,
`.../AndroidLibrarySessionPort.kt:119-120`, `.../MainActivity.kt:74`,
`.../MainActivitySurface.kt:11-47`, sowie die zwei Doppelgänger
`android/app/src/test/java/de/reprise/spike/BrowseSurfaceTest.kt:537` und
`.../LibraryScreenStateTest.kt:335-340`.

**Änderung:**

- `LibrarySessionPort` (`LibrarySession.kt:13-62`) bekommt
  `fun artistPortraitCached(name: String, size: AndroidArtworkSize): String?` und
  `fun artistPortraitFetched(name: String, size: AndroidArtworkSize): String?`
  — **ohne** Standardimplementierung, damit beide Doppelgänger die Entscheidung
  sichtbar treffen müssen.
- `AndroidLibrarySessionPort` reicht auf `library.artistPortraitCached(...)` bzw.
  `library.artistPortraitFetch(...)` durch; die zweite ist die einzige Stelle
  mit `Result` — Fehler werden mit `Log.w` gemeldet und als `null` beantwortet,
  wie es `artworkFor` mit seinen Fehlern hält.
- `LibrarySession` reicht beide weiter, ohne `artworkPaths` zu benutzen (siehe
  Entscheidung 4 im Mutterplan).
- `MainActivity.kt:74`: `TrackArtwork(resolve = session::artworkFor,
  resolveArtistPortraitCached = session::artistPortraitCached,
  resolveArtistPortraitFetched = session::artistPortraitFetched)`.
- `MainActivitySurfaceDependencies` bekommt
  `val onlineSourcesEnabled: () -> Boolean = { false }` und
  `val setOnlineSourcesEnabled: (Boolean) -> Unit = {}` — **mit** Vorgabe, weil
  drei Testdateien diese Datenklasse bauen
  (`BrowseDestinationMigrationTest.kt`, `MainActivityPlayViewStabilityTest.kt`,
  `MainActivityConfigurationTest.kt`) und sonst alle drei bricht.
  `productionSurface()` (`MainActivity.kt:274-309`) füllt sie über `library`,
  eingepackt in `runCatching` mit `Log.e`, wie `restoreStoredDestination`
  (`MainActivity.kt:311-317`).

**Test zuerst:**

- `LibraryScreenStateTest.kt`: `portraitLookupsAreNotMemoisedBySession` — zwei
  Aufrufe von `session.artistPortraitCached("Low", LIST)` erzeugen zwei
  Port-Aufrufe. Das hält Entscheidung 4 fest, damit sie niemand aus Versehen
  „optimiert".

**Fertig, wenn:** die volle Suite läuft (siehe Testkommandos im Mutterplan) und
die Testanzahl gegenüber Aufgabe 2 um genau die neuen Tests gestiegen ist.

---

## Aufgabe 4 — Zeile: runder Avatar in `ArtistRow`

**Dateien:** `.../BrowseTabs.kt:589-599`.

**Änderung:** `ArtistRow` bekommt `leadingContent = { ArtistAvatar(...) }` mit
40 dp und der Kette aus Aufgabe 2:
`rememberArtistArtworkVisual(name = artist.name, representativeUri =
artist.representativeUri, artworkSize = AndroidArtworkSize.LIST, allowFetch =
false)`. Die Zeile bleibt sonst, wie sie ist; `ArtistRow` wird von
`ArtistRows:582` (Liste), `ArtistRows:564` (Raster) und
`ArtistSearchSections:428` (Suche) benutzt und erbt den Avatar überall.

**Test zuerst** (neu: `ArtistPortraitSurfaceTest.kt`):

- `theArtistRowShowsACachedPortrait` — Fake-`TrackArtwork` liefert für den
  Porträtweg ein eindeutig gefärbtes Bitmap; das Bild in der Zeile ist dieses.
- `anArtistWithoutAPortraitShowsTheAlbumCover`
- `anArtistWithoutEitherShowsTheGeneratedCover`
- `scrollingTheArtistListNeverFetches` — 200 Zeilen, `performScrollToIndex(180)`
  auf `library-artists-list` (dasselbe Muster wie
  `MainActivityConfigurationTest.kt:160-162`), Fetch-Zähler bleibt 0. Das ist
  die Regel, die den ganzen Plan trägt.

**Fertig, wenn:** volle Suite grün, alle vier Namen im XML.

---

## Aufgabe 5 — Detailseite: Kopf über den Alben

**Dateien:** `.../BrowseTabs.kt:244-272` (beide Aufrufstellen),
`.../BrowseTabs.kt:314-383` (`ArtistDetailSections`).

**Der Kopf zeigt Bild und Kennzahlen, keinen Namen.** Die Zurück-Zeile
(`BrowseTabs.kt:214-223`) bleibt der stehende Titel der Seite, und der Play-Knopf
(`BrowseTabs.kt:231-242`) bleibt, wo er ist — er wandert **nicht** in die
`LazyColumn`. Reihenfolge auf dem Schirm, von oben:

1. Zurück-Zeile mit Pfeil und Interpretennamen — steht fest, scrollt nicht weg.
2. `ListPlayButton` — steht fest.
3. **Neu:** Bild 210 dp und Kennzahlen — scrollt mit weg.
4. Sektion „Albums".

Damit steht der Name genau einmal auf dem Schirm, und zwar an der Stelle, die
ihn auch beim Scrollen behält. Die Frage, ob die Zurück-Zeile den Namen abgibt,
ist entschieden: nein.

**Änderung:**

- `ArtistDetailSections` bekommt einen Parameter `artist: LibraryArtist`; beide
  Aufrufe (`:245` und `:259`) reichen `selectedArtist.artist` durch.
- Der Zustand wird **außerhalb** der `LazyColumn` geholt:
  `val head = rememberArtistArtworkVisual(artist.name, artist.representativeUri,
  AndroidArtworkSize.ArtistDetail, allowFetch = true)` im Rumpf von
  `ArtistDetailSections`, vor `LazyColumn` (`:339`). Damit gibt es genau eine
  Anfrage pro geöffneter Seite, und nicht eine weitere, sobald der Kopf aus dem
  Sichtfeld scrollt und wiederkommt.
- Neues erstes `item(key = "artist-portrait-head")` vor
  `"artist-albums-heading"` (`:344`): `ArtistPortraitHeader(head, artist)` mit
  210 dp Bild, darunter `artist.details()` (`LibraryText.kt:22`, also
  „X albums • Y tracks"). **Kein Name.**
- `itemCount` (`:331-334`) bekommt `+ 1`. Sonst zeigt der wiederhergestellte
  Ankerwert (`surfaceState.scrollPosition(key).within(itemCount)`, `:335`) auf
  eine Zeile zu wenig.
- Kein Klickziel, kein Vollbild, kein Zoom.

**Test zuerst** (in `ArtistPortraitSurfaceTest.kt`):

- `openingAnArtistFetchesExactlyOnceForThatArtist` — Zähler 1, und der
  übergebene Name ist der geöffnete Interpret.
- `theDetailHeadDoesNotFetchAgainWhenItScrollsOutAndBack` —
  `performScrollToIndex` über den Kopf hinaus und zurück, Zähler bleibt 1.
- `aClosedSwitchLeavesTheDetailHeadOnTheAlbumCover` — der
  `LibrarySessionPort`-Doppelgänger bildet den Vertrag der Brücke nach: bei
  geschlossenem Gate gibt `artistPortraitFetched` `null` zurück, **ohne** seinen
  eigenen Beschaffer-Zähler zu erhöhen. Erwartet: das Album-Cover ist zu sehen,
  der Beschaffer-Zähler ist 0. Dass die Brücke sich wirklich so verhält, hat
  `core`-Aufgabe 5 bewiesen — hier wird nicht ein zweites Mal dieselbe
  Entscheidung getroffen.
- `reopeningTheSameArtistDoesNotReachTheNetworkTwice` — zweimal öffnen; der
  Doppelgänger zählt zwei *Aufrufe*, aber sein dahinterliegender
  Beschaffer-Zähler bleibt bei 1, weil der zweite Aufruf aus dem
  Zwischenspeicher beantwortet wird.
- `theDetailHeadShowsTheCountsAndNotTheName` — der Interpretenname steht genau
  einmal im Baum (Zurück-Zeile), `artist.details()` ist sichtbar. Ohne diesen
  Test wandert der Name irgendwann still zurück unter das Bild.

**Zusätzlich prüfen (nicht neu schreiben):** `ArtistDetailSurfaceTest.kt`
bleibt unverändert grün — insbesondere
`artistPageListsTheArtistsAlbums` und `artistAlbumsRenderNewestFirstWith
AlphabeticalTies`, die über Textpositionen urteilen und durch das neue erste
Element verschoben werden.

---

## Aufgabe 6 — Einstellungsseite als Bauteil

**Dateien:** neu `.../settings/OnlineSourcesSettingsPage.kt`, neu
`.../settings/SettingsControls.kt`, `.../PlaybackSettingsScreen.kt:244-276`.

**Änderung:**

- `SettingsSwitchRow` (heute `private` in `PlaybackSettingsScreen.kt:244`, mit
  seinem tragenden Kommentar über die Trefferfläche) wandert wortgleich als
  `internal` nach `settings/SettingsControls.kt`; `PlaybackSettingsScreen.kt`
  importiert sie. Keine zweite Kopie.
- `OnlineSourcesSettingsPage(enabled, setEnabled, back)` nach dem Muster von
  `AppearanceSettingsPage.kt:22-49`: `SettingsTopAppBar("Online sources", "Back
  to Settings", back)`, `SettingsSectionTitle("Artist photos")`, ein
  `SettingsSwitchRow("Download artist photos", ...)`, darunter ein Absatz, der
  drei Dinge sagt: dass jeder angezeigte Interpretenname an Deezer geht, dass
  die App sonst nichts ins Netz schickt, und dass ohne den Schalter die
  Album-Cover bleiben. `testTag("settings-page-online-sources")`.

**Test zuerst** (neu: `OnlineSourcesSettingsPageTest.kt`):

- `theSwitchStartsFromTheSuppliedState`
- `togglingTheSwitchReportsTheNewValueOnce`
- `thePageNamesDeezerAndSaysWhatLeavesTheDevice` — prüft, dass „Deezer" im
  sichtbaren Text steht. Ohne diesen Test kann der erklärende Absatz still
  verschwinden.

**Fertig, wenn:**
`JAVA_HOME=/usr/lib/jvm/java-21-openjdk android/gradlew --project-dir android
:app:testDebugUnitTest --tests 'de.reprise.spike.OnlineSourcesSettingsPageTest'`
grün, XML frisch.

---

## Aufgabe 7 — Die Seite verdrahten

**Dateien:** `.../settings/SettingsOverview.kt:25-75`,
`.../settings/SettingsNavigation.kt:19-104`, `.../BrowseScreen.kt:690-710`,
`.../MainActivity.kt:235-266,274-309`, `.../MainActivitySurface.kt`,
`android/app/src/test/java/de/reprise/spike/MainActivitySettingsNavigationTest.kt:52-66`.

**Änderung:** `SettingsRoute` bekommt `ONLINE_SOURCES("online-sources")`; die
Liste in `SettingsOverview:50-75` eine fünfte `SettingsSection` (Symbol `cloud`,
Titel „Online sources", Untertitel „On"/„Off" aus dem Zustand);
`SettingsNavigation` einen `composable`-Block und zwei neue Parameter;
`BrowseScreen:690` reicht sie durch; `MainActivity` hält den Wert in einem
`mutableStateOf` neben `themeSelection` und schreibt ihn über
`surface.setOnlineSourcesEnabled`.

**Test zuerst:** `MainActivitySettingsNavigationTest.kt` —
`overviewListsExactlyTheFourSectionsThatExist:52` wird zu
`overviewListsExactlyTheFiveSectionsThatExist`: `assertCountEquals(5)`, alle
fünf Zeilen 72 dp, `onNodeWithText("Online sources").assertIsDisplayed()`. Dazu
ein neuer Test `theOnlineSourcesPageOpensAndBackReturnsToTheOverview` nach dem
Muster von `pageBackReturnsToOverviewAndOnlyOverviewBackClosesTheOverlay:69`.

**Fertig, wenn:** volle Suite grün.

---

## Aufgabe 8 — Die INTERNET-Berechtigung

**Dateien:** `android/app/src/main/AndroidManifest.xml:6-14`.

**Änderung:** `<uses-permission android:name="android.permission.INTERNET" />`
mit einem Kommentar in derselben Form wie der über `POST_NOTIFICATIONS`: dass
die Wurzelzertifikate über `rustls` + `webpki-roots` einkompiliert sind, der
Android-Zertifikatsspeicher also nicht gebraucht wird, und dass ohne den
Schalter aus Aufgabe 6/7 kein Byte fließt.

**Vorbereitungsschritt — das Orakel wird gemessen, nicht geraten.** Es gibt kein
gebautes Merged-Manifest zum Nachschlagen, und `androidx.media3:media3-exoplayer`
sowie `media3-session` (`android/app/build.gradle.kts:92-93`) können eigene
`uses-permission`-Einträge beisteuern. Ein Gleichheitsvergleich gegen eine
geratene Zahl von vier wäre sofort rot, ohne dass irgendetwas falsch ist. Also:
den Test einmal mit einer bewusst fehlschlagenden Zusicherung fahren, deren
Meldung die tatsächlich zusammengeführte Menge ausdruckt, diese Menge als
Konstante im Test einpinnen und die Herkunft der fremden Einträge in einem
Kommentar festhalten.

**Test zuerst** (neu: `ManifestPermissionsTest.kt`, Muster
`ApplicationIdentityTest.kt:29-38`):

- `theAppRequestsInternetAndNothingUnexpected` — über
  `packageManager.getPackageInfo(packageName, PackageManager.GET_PERMISSIONS)
  .requestedPermissions` zwei Zusicherungen:
  1. `INTERNET` **muss** enthalten sein — das ist die Aussage dieser Aufgabe,
     und sie darf nicht still verschwinden.
  2. Die Menge stimmt mit der eingepinnten Konstante überein. Die
     Fehlermeldung nennt die Differenz in beide Richtungen, damit eine neu
     hinzugekommene Berechtigung sofort mit Namen auffällt statt nur als Zahl.
     Kommt eine Abhängigkeit mit einer weiteren Berechtigung dazu, ist das
     Rot ein Hinweis und eine bewusste Entscheidung, kein Fehler — die
     Konstante wird dann mit Begründung nachgezogen.

**Fertig, wenn:** volle Suite grün. Diese Änderung gehört in die
Freigabemeldung, nicht nur in den Diff.

---

## Aufgabe 9 — Volles Gate

**Dateien:** keine.

**Fertig, wenn** alle Läufe aus den Testkommandos des Mutterplans grün sind, und
die Zeile `suites=… tests=… failures=0 errors=0 … verdict=fresh` aus
`scripts/check-android-suite.sh` im Protokoll steht. Die Untergrenze
`ANDROID_TEST_FLOOR=334` (`check-android-suite.sh:9`) steigt durch diesen Plan
nur an; sie muss nicht angefasst werden.

Dazu die Formalitäten: `cargo fmt --all`, `cargo clippy --all-targets
--all-features -- -D warnings`.

Die volle Android-Suite steht hier und nicht in einer Post-Merge-Liste: dieser
Strang besitzt alle Android-Dateien, die sie anfasst, und `core` ist zu diesem
Zeitpunkt bereits gelandet.

---

## Aufgabe 10 — Sichtprüfung am Gerät

**Dateien:** keine.

**Vorbereitungsschritt — ohne ihn ist die erste Aufnahme unmöglich.** Die Liste
holt nie selbst (`allowFetch = false`, Aufgabe 4); ein Porträt landet
ausschließlich dadurch im Zwischenspeicher, dass jemand den Interpreten geöffnet
hat. Auf einer frischen Installation zeigt die Liste deshalb **nur**
Album-Cover und Fallback-Farben — die verlangte Aufnahme mit allen drei Zuständen
nebeneinander ist dort nicht zu bekommen. Also: erst drei bis vier Interpreten
öffnen und wieder zurück, dann die Aufnahme machen. Dieselbe Falle hat schon den
Desktop-Lauf gekostet („Rang 6–20 existieren erst nach dem Klick").

Zu belegen mit Aufnahmen, nicht mit einem Bericht:

- **Zeile:** Screenshot der Interpretenliste mit mindestens einem geladenen
  Porträt, einem Album-Cover-Rückfall und einer Fallback-Farbe in derselben
  Aufnahme. Der Avatar ist rund und liegt auf der Textgrundlinie.
- **Detailkopf:** zwei Screenshots derselben Seite — einer unmittelbar nach dem
  Öffnen (Album-Cover), einer nach dem Eintauschen des Porträts. Die Position
  der Sektion „Albums" ist in beiden identisch; das ist der Nachweis, dass
  nichts springt. Auf beiden steht der Interpretenname genau einmal, nämlich in
  der Zurück-Zeile.
- **Scrollen:** eine Bildschirmaufnahme des Durchlaufs durch die
  Interpretenliste. Framezeiten nur aus einem **Release**-Build auf dem echten
  Gerät (`adb shell dumpsys gfxinfo org.reprise framestats`) — ein Debug-Build
  und der Emulator können diese Frage nicht beantworten, das ist gemessen.

**Messpunkt, kein Kopfrisiko:** `thumbnail_with_source` liest die Quelldatei ganz
ein und hasht sie, bevor es den Cache-Treffer feststellt (`cover.rs:222-228`).
Für ein 1000 × 1000-Porträt von Deezer sind das pro neu gebundener Zeile ein paar
hundert Kilobyte. Das läuft auf den Einzelthread-Executoren
`reprise-artwork-list` / `reprise-artwork-full` (`TrackCover.kt:44-45`), nie auf
dem Hauptthread — es kann also keinen Frame reißen, sondern nur seine eigene Lane
verstopfen und Bilder verspätet liefern. Wenn Porträts beim Scrollen sichtbar
nachhinken, ist das der erste Verdächtige; wenn die Liste selbst stockt, ist es
nicht dieser Pfad.
