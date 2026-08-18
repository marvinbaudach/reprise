---
slug: android-artist-portrait-before-album-cover
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Android — der Interpret soll von Anfang an sein Porträt zeigen, kein Album-Cover zum Tauschen

**Nur ein Befund und eine Design-Ansage des Nutzers, kein Plan.** Festgehalten
am 16.08.2026, gemeldet vom Nutzer:

> „bei der android app, wenn ich auf interpret klicke, dann lädt in der
> Interpretansicht oft ein anderes Cover. Wenn ich dann zurück zur Liste der
> Interpreten gehe, wird dieses neue Cover behalten für den Interpreten. Er
> sollte von anfang an ein Interpretenbild laden und nicht erst Cover vom Album
> zeigen die ersetzt werden."

## Wichtig: Das ist heute so gebaut, nicht kaputt

Das beschriebene Verhalten ist die **beschlossene** Mechanik aus
`docs/plans/android-artist-photos*.md` (gelandet: `core` #482 = `8b87ae8ada`,
`ui` #486 = `0b7cf509d9`). Belege auf `origin/dev` — der lokale Hauptcheckout
ist zu alt, `ArtistCover.kt` existiert dort noch gar nicht, also **alles gegen
`origin/dev` prüfen**:

| Stelle | Datei (auf `origin/dev`) | Verhalten |
| --- | --- | --- |
| Listenzeile | `android/app/src/main/java/de/reprise/spike/BrowseTabs.kt:602-611` | `allowFetch = false` — die Liste holt **nie** ein Porträt |
| Detailkopf | `.../BrowseTabs.kt:333-353` | `allowFetch = true`, `AndroidArtworkSize.ARTIST_DETAIL` — holt das Porträt beim Öffnen |
| Auflösung | `.../ArtistCover.kt:22-50` (`rememberArtistArtworkVisual`), `:91-102` (`artistArtworkRequest`) | `allowFetch` entscheidet zwischen *cached* und *fetch* |
| Rückfall | `.../TrackCover.kt:164-166`: `val bitmap = portrait ?: resolve(request.trackUri, request.size)` | ist noch kein Porträt da, wird `artist.representativeUri` aufgelöst — also das Album-Cover |
| Tausch | `.../TrackCover.kt`: `refreshesArtistPortrait() = kind == ArtworkKind.ARTIST && allowFetch` | nur mit `allowFetch` wird das Porträt nachgeladen und ersetzt das Cover |
| Vorgabe | `.../ArtworkRequestGate.kt:23`: `allowFetch: Boolean = false` | netzfrei ist der Standard |

Daraus folgt genau das gemeldete Erlebnis: Liste zeigt Album-Cover → Öffnen
holt das Porträt → Zurück zeigt die Liste das nun zwischengespeicherte Porträt.
Der Übergabebericht `docs/plans/android-artist-photos-task10.HANDOFF.md`
formuliert es unmissverständlich: „Die Liste holt nie selbst (`allowFetch =
false`), ein Porträt landet ausschließlich dadurch im Zwischenspeicher, dass
jemand den Interpreten geöffnet hat."

Der Nutzer verwirft diese Entscheidung. Das ist eine **Design-Änderung**, kein
Bugfix — und sie kollidiert mit dem Grund, aus dem es so gebaut wurde.

## Warum es so gebaut wurde (der Konflikt, den der Umbau lösen muss)

Die Liste ist bewusst netzfrei, damit Scrollen keine Anfragen auslöst. Das ist
mit Tests festgenagelt (Namen aus `docs/plans/android-artist-photos-ui.md`):

- `scrollingTheArtistListNeverFetches` — 200 Zeilen, `performScrollToIndex(180)`,
  Fetch-Zähler muss 0 bleiben
- `aRowResolutionNeverCallsTheFetcher` — `allowFetch = false`
- `anArtistWithoutAPortraitShowsTheAlbumCover` /
  `anArtistWithoutAPortraitFallsBackToTheAlbumCover`
- `openingAnArtistFetchesExactlyOnceForThatArtist`
- `aClosedSwitchLeavesTheDetailHeadOnTheAlbumCover`

„Von Anfang an ein Interpretenbild" heißt bei 151 Interpreten (Desktop-Zahl,
Android-Bibliothek vermutlich ähnlich): entweder je sichtbarer Zeile eine
Netzanfrage beim Scrollen — genau das, was der Scroll-Performance-Strang
verhindern sollte — oder ein **Vorlauf**, der die Porträts einmal im
Hintergrund holt und ablegt, bevor die Liste sie braucht.

Zusätzlich: Das Netz-Gate lebt bewusst nur in Rust (Entscheidung 3 im
Mutterplan). Bei ausgeschaltetem Online-Quellen-Schalter gibt es **kein**
Porträt — der Album-Cover-Rückfall muss für diesen Fall bleiben.

## Erst klären, sonst wird das Falsche gebaut

**Was genau taucht im Detailkopf auf — ein Porträt oder ein anderes
Album-Cover?** Der Nutzer schreibt „ein anderes Cover". Beide Lesarten führen
zu völlig verschiedener Arbeit:

- **(a) Ein echtes Porträt.** Dann ist nur die Reihenfolge das Problem, und die
  Lösungsrichtungen unten greifen.
- **(b) Ein falsches/fremdes Bild.** Dann liegt ein echter Fehler in der
  Porträtauswahl vor — und dafür gibt es einen bekannten Verdächtigen: Deezer
  liefert unter normal aussehenden Bezeichnern seine graue Silhouette aus, und
  die Auswahlregel nimmt bei Namensgleichheit den populärsten Treffer, was
  fremde Gesichter erzeugen kann (Memory *reprise-deezer-portrait-placeholders*,
  `crates/reprise-core/src/artist_portrait/deezer.rs`,
  `MISSING_IMAGE_IDENTIFIERS`). Dass ein solches Bild dann auch die Liste
  „vergiftet", weil es zwischengespeichert wird, wäre die eigentliche Beschwerde.

Nächster Schritt vor jedem Umbau: am Gerät für drei bis vier Interpreten
festhalten, **welches** Bild vor und nach dem Tausch steht.

## Lösungsrichtungen (offen)

1. **Vorlauf statt Nachladen.** Porträts für die sichtbaren/nächsten Zeilen im
   Hintergrund holen (gedrosselt, eine Warteschlange, kein Fetch pro
   Scroll-Frame), sodass die Liste sie schon hat. Die Detailseite tauscht dann
   nichts mehr aus.
2. **Kein Zwischenzustand im Detailkopf.** Solange kein Porträt da ist, die
   Fallback-Farbe/den generierten Avatar zeigen statt eines Album-Covers, das
   später ersetzt wird — dann springt optisch nichts von „Album" auf „Person".
3. **Beides kombinieren** — Vorlauf für die Liste, generierter Avatar als
   Zwischenstand, Album-Cover nur noch bei ausgeschaltetem Schalter.

## Randnotiz

Die Sichtprüfung am Gerät (Aufgabe 10 aus
`docs/plans/android-artist-photos-task10.HANDOFF.md`) steht ohnehin noch offen.
Sie und dieser TODO gehören zusammen: wer die Aufnahmen macht, sieht dabei
genau den Tausch, um den es hier geht.
