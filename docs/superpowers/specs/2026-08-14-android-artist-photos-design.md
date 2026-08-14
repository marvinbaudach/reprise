# Android: Künstlerfotos in der Interpretenansicht

Date: 2026-08-14
Status: design approved, not yet implemented
Baseline: `origin/dev` @ 5721ade95e
Branch: `feature/android-artist-photos`

## Problem

Die Interpretenansicht der Handy-App ist eine Textliste. `ArtistRow`
(`android/app/src/main/java/de/reprise/spike/BrowseTabs.kt:590`) ist ein
`ListItem` aus `headlineContent` (Name) und `supportingContent`
(„X albums • Y tracks"), ohne Bildslot. Öffnet man einen Interpreten, beginnt
`ArtistDetailSections` (`BrowseTabs.kt:315`) direkt mit der Sektion „Albums";
über der Liste steht nur eine Zeile aus Zurück-Pfeil und Name
(`BrowseTabs.kt:213`).

Der Beschaffer existiert längst. `crates/reprise-core/src/artist_portrait/`
holt Porträts bei Deezer, legt sie unter einem Dateicache ab und schützt sich
mit Fristen (positiv 30 Tage, negativ 7 Tage über einen `.notfound`-Marker,
`artist_portrait/cache.rs:11-12`). Auf dem Desktop hängen daran zwei Flächen:
`ui/stats/stats_artist_image.rs` liest ausschließlich den Zwischenspeicher, und
`ui/now_playing/artist_portrait_worker.rs` ist der einzige Ort, der tatsächlich
holt. Die Spec vom 13.08. (`2026-08-13-stats-artist-images-and-ranking-design.md`)
schließt ausdrücklich mit „Die Android-Oberfläche bleibt unberührt". Das wird
hier nachgeholt.

Drei Dinge fehlen dafür, und jedes ist eine echte Lücke, keine Verkabelung:

1. **Der Cache-Pfad zeigt auf dem Handy ins Leere.** `cache::cache_dir()`
   (`artist_portrait/cache.rs:16-20`) baut auf `dirs::cache_dir()` mit
   `std::env::temp_dir()` als Rückfall. Auf Android ist weder das eine gesetzt
   noch das andere verlässlich beschreibbar. Die Variante mit explizitem
   Verzeichnis, `load_or_fetch_with` (`artist_portrait/mod.rs:73`), ist
   `pub(crate)`; öffentlich ist nur `load_cached_from` (`mod.rs:65`).
2. **Die App darf nicht ins Netz.** `android/app/src/main/AndroidManifest.xml`
   listet `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PLAYBACK` und
   `POST_NOTIFICATIONS` — keine `INTERNET`. Der HTTP-Stapel ist dagegen längst
   mitgebaut: `reprise-android-ffi` hängt ohne Feature-Schalter an
   `reprise-core`, das `ureq 3.3` zieht, und die Sperrschicht ist
   `rustls` + `webpki-roots` + `ring` (Cargo.lock). Die Wurzelzertifikate sind
   also einkompiliert; der Android-Zertifikatsspeicher wird nicht gebraucht.
3. **Es gibt keinen Ein-Aus-Punkt.** Die Einstellungen der App kennen
   Appearance, Library, Playback und About
   (`android/app/src/main/java/de/reprise/spike/settings/`). Weder das
   Artwork-Modul noch das Online-Quellen-Gate haben dort eine Oberfläche.

## Entscheidungen

Vom Nutzer bestätigt, bindend:

1. **Die Kette des Desktops, nicht eine zweite.** Porträt → Album-Cover des
   Interpreten → Fallback-Farben. Nicht nur Porträts (sonst bleibt die Ansicht
   bei unbekannteren Interpreten leer), nicht nur Cover (dann ist es kein Foto).
2. **Liste und Detailseite**, nicht nur die Zeilen: runder Avatar in
   `ArtistRow`, großes Bild über den Alben der geöffneten Seite. Ein 40-dp-Kreis
   allein rechtfertigt den Deezer-Abruf nicht.
3. **Geholt wird erst beim Öffnen.** Listenzeilen lesen ausschließlich den
   Zwischenspeicher. Ohne diese Trennung erzeugte ein einziger Durchlauf durch
   eine Bibliothek mit mehreren hundert Interpreten ebenso viele Suchanfragen
   und schöbe die halbe Bibliothek als Klartext zu Deezer.
4. **Eine neue Einstellungsseite „Online sources"**, ein Schalter, Voreinstellung
   aus. Das ist die Stelle, an die Lyrics, Radio und Podcasts später ohne Umbau
   dazukommen, und der Ort für den Satz, dass Künstlernamen das Gerät verlassen.
5. **Basis ist `origin/dev`**, nicht der offene Branch
   `feature/android-list-scroll-performance`. Der fasst dieselben drei Dateien
   an (`BrowseTabs.kt`, `TrackCover.kt`, `ArtworkCache.kt`); wer zweiter landet,
   rebast. Unsere Änderung an der Bildpipeline ist additiv — ein zweiter
   Auflösungsweg neben dem bestehenden, kein Umbau des bestehenden.

## Entwurf

### 1 · Kern (`reprise-core`)

Eine einzige Erweiterung in `artist_portrait/mod.rs`:

```rust
pub fn load_or_fetch_in(name: &str, dir: &Path) -> Result<PortraitOutcome, PortraitError>
```

Sie tut, was `load_or_fetch` (`mod.rs:44`) tut, nimmt das Verzeichnis aber
entgegen, statt es aus `cache::cache_dir()` zu ziehen. `load_or_fetch` bleibt
als Bequemlichkeit für den Desktop erhalten und ruft die neue Funktion.

An der Deezer-Abfrage, den Fristen, der Auswahlregel und der Platzhalterliste
`MISSING_IMAGE_IDENTIFIERS` (`artist_portrait/deezer.rs`) ändert sich **nichts**.
Das Handy erbt sie mitsamt dem Verhalten, das der Placeholder-Strang dort
etabliert hat.

### 2 · Brücke (`reprise-android-ffi`)

`MusicLibrary` bekommt schon heute beide Pfade von Kotlin
(`open(app_private_directory, app_cache_directory)`, `lib.rs:81`) und hält den
zweiten als `cache_root` (`lib.rs:91`). Porträts liegen unter
`cache_root/artist-portraits`.

Drei neue Methoden:

* `artist_portrait_cached(name, size) -> Option<String>` — `load_cached_from`
  auf dem Porträtverzeichnis. Fasst das Netz nie an, auch nicht bei
  freigeschaltetem Gate.
* `artist_portrait_fetch(name, size) -> Option<String>` — blockierend, für einen
  Worker-Thread gedacht. **Prüft zuerst**
  `online_sources::network_allowed(conn, &modules::ARTWORK_MODULE)` — dieselbe
  Kombination, die der Desktop-Worker benutzt
  (`artist_portrait_worker.rs:35`; Schlüssel `module.artwork.enabled`,
  `modules.rs:382`). Ohne Freigabe kehrt sie sofort leer zurück, ohne ein Byte
  Netz und ohne Cache-Schreibvorgang.
* `online_sources_enabled()` / `set_online_sources_enabled(bool)` — liest und
  schreibt `online_sources.enabled` und `module.artwork.enabled` gemeinsam. Ein
  Schalter, zwei Schlüssel, damit die Bedeutung auf beiden Geräten dieselbe
  bleibt.

Beide Bildmethoden geben den Pfad einer **verkleinerten** Datei zurück, gewonnen
über denselben Weg wie Cover: `cover::thumbnail_with_source` (`lib.rs:258`) mit
`CoverSource::FolderImage(pfad)`. Kein 500-px-JPEG hinter einem 40-dp-Kreis —
auf dem Handy wiegt das schwerer als auf dem Desktop.

### 3 · Oberfläche (Android)

**Manifest.** `<uses-permission android:name="android.permission.INTERNET" />`.
Das ist eine sichtbare Änderung an der Berechtigungsliste der App und gehört in
die Freigabemeldung, nicht nur in den Diff.

**Bildauflösung.** `TrackCover.kt` trägt heute
`resolve: (String, AndroidArtworkSize) -> String?` und
`decode: (String) -> Bitmap? = BitmapFactory::decodeFile` (`TrackCover.kt:40-41`).
Es dekodiert also ohnehin einen Dateipfad — ein Porträt geht denselben Weg, nur
mit einem anderen `resolve`. `ArtworkCache` und `ArtworkRequestGate` werden
mitbenutzt; ihr Schlüssel muss dafür **Art** (Track/Interpret) mitführen, sonst
kollidiert ein Künstlername mit einer `trackUri`.

**Zeile.** `ArtistRow` (`BrowseTabs.kt:590`) bekommt `leadingContent`: runder
Avatar, Kette

1. `artist_portrait_cached(name)`,
2. Album-Cover über `LibraryArtist.representativeUri`
   (`LibraryScreenState.kt:128`, gespeist aus `browse.rs:58`) auf dem
   bestehenden Track-Weg,
3. `fallbackCoverBitmap` / `androidFallbackCoverColours` (`FallbackCover.kt:14`).

Aus der Liste geht **keine** Anfrage raus. Das ist die Regel, die den Entwurf
trägt, und sie gehört in einen Test, nicht in einen Kommentar.

**Detailseite.** Ein neues erstes `item()` in der `LazyColumn` von
`ArtistDetailSections` (`BrowseTabs.kt:339`), vor der Sektion „Albums": großes
Bild, darunter Name und Kennzahlen, gleiche Kette. Beim Öffnen wird genau **ein**
`artist_portrait_fetch` für den geöffneten Interpreten auf einem Worker
angestoßen — ob daraus eine Netzanfrage wird, entscheidet der Kern: liegt ein
frisches Porträt oder ein gültiger Negativmarker vor, kehrt der Aufruf ohne
Netzverkehr zurück. Ein Generationszähler verwirft Antworten, die nach dem
Wegnavigieren eintreffen — dasselbe Muster, das die Desktop-Spec für den
`CoverLoader` festhält.

**Einstellungen.** Neue Seite `settings/OnlineSourcesSettingsPage.kt`, verdrahtet
in `SettingsNavigation.kt` und `SettingsOverview.kt`. Ein Schalter
„Download artist photos" mit erklärendem Text: dass jeder angezeigte
Interpretenname an Deezer geht, dass die App sonst nichts ins Netz schickt, und
dass ohne den Schalter die Album-Cover bleiben.

### 4 · Regelwerk

`docs/ux-rules.md` beschreibt die GTK-Oberfläche; Android kommt darin nur als
*fremdes Gerät* vor (MTP, Sync-Auswahl). Es entsteht daher **keine** neue
UX-Regel. Die Absicherung sind die Tests unten.

## Tests

**Kern (`reprise-core`)**

* `load_or_fetch_in` schreibt und liest ausschließlich im übergebenen
  Verzeichnis und rührt `dirs::cache_dir()` nicht an.
* Der Negativmarker im übergebenen Verzeichnis verhindert die zweite Anfrage
  innerhalb der Frist.
* `load_or_fetch` liefert nach dem Umbau dieselben Ergebnisse wie zuvor
  (Regressionsschutz für den Desktop).

**Brücke (`reprise-android-ffi`)**

* Bei ausgeschaltetem Gate zählt ein Fake-Beschaffer **null** Aufrufe, und es
  entsteht keine Datei im Porträtverzeichnis.
* `artist_portrait_cached` erzeugt auch bei eingeschaltetem Gate keinen Aufruf.
* Der zurückgegebene Pfad ist die verkleinerte Datei, nicht das Original.

**Oberfläche (Android, Robolectric — JDK 21, `TMPDIR=/tmp`)**

* Zeile zeigt das Porträt, wenn eines im Zwischenspeicher liegt.
* Ohne Porträt zeigt sie das Album-Cover; ohne beides die Fallback-Farben.
* Das Blättern durch die Liste löst **keinen** `artist_portrait_fetch` aus.
* Das Öffnen eines Interpreten löst **genau einen** aus, für diesen Interpreten.
* Beim zweiten Öffnen desselben Interpreten entsteht zwar wieder ein Aufruf,
  aber **keine zweite Netzanfrage** — der Zwischenspeicher beziehungsweise der
  Negativmarker fängt ihn ab (gezählt am Fake-Beschaffer der Brücke).
* Bei ausgeschaltetem Schalter fragt auch die Detailseite nicht.
* Der Cache-Schlüssel unterscheidet Interpret und Track: ein Künstler namens wie
  eine `trackUri` bekommt nicht deren Bild.

**Sichtprüfung am Gerät**

Avatar rund und auf der Textgrundlinie ausgerichtet, Detailkopf ohne Sprung beim
Eintauschen des nachgeladenen Bildes, Scrollen durch die Interpretenliste ohne
sichtbares Stocken. Zu belegen mit Aufnahmen, nicht mit einem Bericht.

## Nicht im Umfang

* Kein Porträt im Now-Playing der Handy-App.
* Keine Rasteransicht für Interpreten; die Liste bleibt eine Liste.
* Kein Stapellauf, der Porträts für alle Interpreten auf einmal holt.
* Keine Änderung an Deezer-Abfrage, Fristen, Auswahlregel oder Platzhalterliste.
* Keine Cover-Übertragung vom Desktop — das ist der eigene Strang
  `feature/device-sync-covers`
  (`2026-08-14-device-sync-covers-design.md`).
* Keine Porträts über den Gerätesync.
