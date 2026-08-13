# My Stats: echte Künstlerbilder und eine aufklappbare Interpretenrangliste

Date: 2026-08-13
Status: design approved, not yet implemented
Baseline: `origin/dev` @ 79d3a51528

## Problem

In *My Stats* trägt die Bandreihe fünf Bilder: die Hero-Karte des
Spitzenreiters und vier Kacheln für Rang 2–5. Auf einer echten Bibliothek
zeigen die vorderen Ränge entweder ein Bild, das nicht zur Band passt, oder gar
keines — und hinter Rang 5 hört die Rangliste auf, während die Songkarte
daneben 25 Titel anbietet.

### Das Bild ist ein zufälliges Album, nicht die Band

`ranked_groups` wählt den Repräsentanten eines Interpreten als
**alphabetisch kleinsten Dateipfad**: `MIN(le.path)` je Aggregatzeile
(`stats_screen.rs:230`), danach in Rust noch einmal `if row.path < *path`
(`stats_screen.rs:499`). Welches Album die Karte zeigt, entscheidet damit die
Sortierung von Ordnernamen. Gemessen an der Bibliothek des Nutzers
(2026, Top 3 nach Abspielungen):

| # | Interpret | gewähltes Album |
|---|---|---|
| 1 | Lorna Shore | *I Feel The Everblack Festering Within Me (2025)* |
| 2 | Falling in Reverse | *Popular Monster (2024)* |
| 3 | The Devil Wears Prada | *Color Decay (Deluxe) (2023)* |

Das widerspricht der geschriebenen Regel: STATS-13 verlangt „the album cover of
their **most-played track**". Der Code hat diese Regel nie erfüllt.

### Es gibt bereits Porträts — ohne Oberfläche

`crates/reprise-core/src/artist_portrait/` holt Künstlerporträts (Deezer) und
legt sie unter `~/.cache/reprise/artist-portraits` ab; auf der Maschine des
Nutzers liegen **162** davon. Verbraucher hat das Modul keinen mehr:
`artist_portrait_worker.rs:3` sagt es selbst — die frühere Artists-Rasteransicht
besaß die Anfragen, geblieben ist nur der Schalter in den Einstellungen,
„until another visible portrait surface is introduced". Die Bandkarten sind
diese Fläche.

### Die Rangliste endet bei Rang 5

`SPOTLIGHT_ALSO_LIMIT = 4` (`stats_snapshot.rs:17`) kappt `spotlight.also`, und
`stats_bands_row.rs:91` liest ausschließlich daraus. Die Daten wären da:
`StatsSnapshot.top_artists` (`stats_snapshot.rs:119`) trägt **alle** Interpreten
des Zeitraums, ungekürzt — dieselbe Quelle, aus der die Songkarte ihre
Fortsetzung speist.

## Entscheidungen

Vom Nutzer bestätigt, bindend:

1. **Porträt zuerst, Album als Rückfall.** Nicht nur ein besseres Album wählen
   und nicht nur Porträts zeigen — die Kette deckt beide Fehlerbilder ab
   (falsches Bild, kein Bild).
2. **Aufklappen wie bei den Songs**, kein Pfeil-Blättern und kein Karussell:
   dasselbe Bedienmuster, das die Songkarte schon trägt.
3. **Zweispaltige Zeilen mit rundem Porträt** für Rang 6–20.
4. **Modul an → nachladen, Modul aus → nur Cover.** Netzabfragen bleiben
   abschaltbar; die Einstellung bekommt ihre sichtbare Wirkung zurück.
5. **Der Umschalter „by plays / by time" kommt mit** — die Bandreihe wird
   vollständig wie die Songkarte bedient.

## Entwurf

### 1 · Die Bildkette

Eine gemeinsame Auflösung für Hero-Karte, Kacheln und Listenzeilen, neu in
`crates/reprise-gnome/src/ui/stats/stats_artist_image.rs`:

1. **Porträt aus dem Zwischenspeicher** — `artist_portrait::load_cached(name)`.
2. **Fehlt es und ist das Modul aktiv** — Anfrage an den Porträt-Worker, das
   Bild trifft asynchron ein und wird eingetauscht. Höchstens die 20 gezeigten
   Ränge, gestaffelt; der vorhandene Negativ-Marker (`.notfound`, 7 Tage)
   verhindert Wiederholungen. Ein Generationszähler wie im `CoverLoader`
   verwirft Antworten, die nach einem Perioden- oder Sortierwechsel eintreffen.
3. **Album-Cover** — vom **meistgehörten** Album des Zeitraums (siehe 3),
   über den bestehenden `CoverLoader`.
4. **Initialen** — wie heute, nie eine leere Fläche.

Porträts laufen durch dieselbe Thumbnail-Pipeline wie Cover
(`CoverSource::FolderImage(pfad)` → `cover::thumbnail`), also `Portrait` (192)
für Karte und Kacheln, `List` (48) für die Zeilen. Kein 500-px-JPEG hinter
einem 32-px-Kreis.

`ArtistPortraitRuntime` (`ui/now_playing/artist_portrait_worker.rs`) bekommt
seine Warteschlange zurück, gebaut nach dem Muster von
`ui/cover/cover_download_worker.rs`: Hintergrund-Thread, Ergebnis über einen
glib-Kanal in den Hauptthread. Anfragen entstehen nur, wenn
`runtime.enabled` gesetzt ist; der Kommentar in Zeile 3–5 entfällt.

### 2 · Die Rangliste

Die Bandreihe wird zur Karte mit dem Aufbau der Songkarte:

* **Umschalter** `by plays / by time` (`adw::ToggleGroup`) rechts über der
  Reihe, auf der Linie des Songkarten-Umschalters. Kein zweiter Titel: der
  Kicker „MOST PLAYED BAND" bleibt im Hero-Bild.
* Er ordnet **die ganze Reihe** um — Hero, Kacheln und aufgeklappte Liste. Der
  Spitzenreiter kann dabei wechseln; in der Beispielbibliothek wechseln
  jedenfalls die Kacheln, weil die heutige Reihung der Hörzeit folgt: The
  Browning kommt dort auf 29 Abspielungen und steht trotzdem nicht unter den
  fünf gezeigten. Der Anteil „N % of your artist listening"
  wird für den jeweiligen Spitzenreiter neu gerechnet — Nenner bleibt die
  Summe der Hörzeit aller Interpreten, wie in `spotlight()`.
* **`Show more top artists`** klappt Rang 6–20 auf, zweispaltig, je Zeile:
  Rang · rundes 32-px-Porträt · Name · Balken relativ zu Rang 1 · Metrik nach
  Umschalterstellung. Klick öffnet den Interpreten wie auf den Kacheln. Die
  Schaltfläche erscheint nur, wenn es mehr als fünf Interpreten gibt, und
  wechselt beim Öffnen auf `Hide more top artists`.
* Die Fortsetzung wächst **in derselben Karte**, wie STATS-22 es für die Songs
  festhält — keine zweite Sektion unter der Seite.

Neue Dateien: `stats_bands_card.rs` (Kopf, Umschalter, Aufklapper, Revealer)
und `stats_bands_more.rs` (die zweispaltigen Zeilen). `stats_bands_row.rs`
bleibt das 2:1:1:1:1-Raster und liest künftig die sortierte Interpretenliste
statt `spotlight.also`.

### 3 · Datenseite (`reprise-core`)

* `artist_rows` (`stats_screen.rs:219`) gruppiert zusätzlich nach `le.album`.
  Die Faltung in `fold_groups` summiert unverändert je Schlüssel — feinere
  Zeilen ändern die Summen nicht, nur die Auswahl des Repräsentanten.
* `ranked_groups` (`stats_screen.rs:480`) wählt den Pfad der Zeile mit den
  **meisten Abspielungen**; Gleichstand → mehr Hörzeit → Pfad alphabetisch,
  damit die Auswahl deterministisch bleibt und Tests nicht flackern.
* `RankedGroup` trägt bis zu **drei** Cover-Kandidaten (die drei meistgehörten
  Alben). Die Oberfläche geht sie der Reihe nach durch, bis eines eine Grafik
  hat; `representative_track_path` bleibt als erster Kandidat erhalten, damit
  `GenreSegment` und `TopAlbum` unverändert weiterlaufen. Ohne die Kandidaten
  bliebe ein Interpret bildlos, dessen meistgehörtes Album kein Cover trägt,
  obwohl das zweite eines hat.
* `StatsSnapshot::top_artists_sorted(SortBy)` analog zu `top_tracks_sorted`
  (`stats_snapshot.rs:130`), plus die Anteilsrechnung für den jeweiligen
  Spitzenreiter.
* `SPOTLIGHT_ALSO_LIMIT` bleibt, wo es ist: `spotlight` behält seine Bedeutung
  für die übrigen Verbraucher; die Bandreihe liest `top_artists`.

### 4 · Regelwerk

Neue Regel **STATS-23** [active] [gtk] in `docs/ux-rules.md`, die **STATS-13**
ersetzt (dieses wird `[replaced by STATS-23]`). Sie hält fest:

* die Bildkette Porträt → meistgehörtes Album (bis zu drei Kandidaten) →
  Initialen, und dass das Nachladen an der Erweiterung hängt;
* dass der Umschalter die ganze Reihe ordnet, Spitzenreiter eingeschlossen;
* dass die Fortsetzung Rang 6–20 in derselben Karte zeigt und nur erscheint,
  wenn es mehr als fünf gibt;
* dass Zeilen wie Kacheln antworten: Zeigercursor, Hover-Wäsche, Fokusring,
  Klick und Enter öffnen den Interpreten (BTN-1/BTN-4).

`scripts/check-ux-traceability.sh` verlangt daraufhin mindestens einen Test
mit `stats_23_` im Namen und verbietet weitere Verweise auf STATS-13 — die
bestehenden `stats_13_*`-Tests werden umbenannt.

## Tests

**Kern (`reprise-core`)**

* Repräsentant ist das meistgehörte Album, nicht der alphabetisch erste Pfad.
* Gleichstand löst deterministisch auf (Hörzeit, dann Pfad).
* Kandidatenliste steht in Abspielreihenfolge und hält höchstens drei.
* Feinere Gruppierung nach Album ändert Abspielungen und Hörzeit je Interpret
  nicht (Regressionsschutz für die Faltung).
* `top_artists_sorted` liefert beide Richtungen stabil.

**Oberfläche (`reprise-gnome`, `--bin reprise`)**

* `stats_23_*`: Bandkarte zieht das Porträt dem Album-Cover vor, wenn eines im
  Zwischenspeicher liegt.
* Rückfallkette: kein Porträt → erstes Album mit Grafik → Initialen.
* Bei ausgeschaltetem Modul entsteht keine Porträtanfrage.
* Aufklapper erscheint erst ab sechs Interpreten, Beschriftung wechselt,
  Zeilenzahl stimmt.
* Umschalter ordnet Hero, Kacheln und Liste gemeinsam um und rechnet den
  Anteil neu.
* Klick auf eine Fortsetzungszeile öffnet denselben Interpreten wie die Kachel.

## Nicht im Umfang

* Die Genre-Karte bekommt keine Porträts; sie behält ihre eigene Auswahlregel
  für den repräsentativen Track und bleibt von STATS-23 unberührt.
* Kein neuer Anbieter für Porträts, keine Änderung an der Deezer-Abfrage, an
  den Zwischenspeicherfristen oder am Einstellungstext.
* Die Android-Oberfläche bleibt unberührt.
* Kein Umbau der Songkarte; sie ist hier nur das Vorbild.
