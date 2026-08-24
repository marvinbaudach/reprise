---
slug: releases-missing-cover-shows-the-band
worktree: /home/marvin/Projects/reprise-releases-missing-cover-shows-the-band
branch: feature/releases-missing-cover-shows-the-band
phase: shipped
codex_session:
created: 2026-08-24
---
# Releases: Wo kein Cover ist, steht die Band

Beschluss des Nutzers vom 24.08.2026, wörtlich:

> „die leeren album cover sind nicht hübsch. die initialien sind nicht
> zentriert. gibt es da nicht ein band foto?"

Acht Entscheidungen desselben Tages, drei aus der Rückfrage, fünf aus dem
Grill:

1. **Bandfoto mit Nachladen** — erst der Cache, bei Miss ein Request über
   dieselbe `ArtistPortraitRuntime`, die Concerts und My Stats benutzen.
2. **Gedämpfte Karte** statt Akzentfläche, wenn weder Cover noch Foto da ist.
3. **Keine Kennzeichnung** — das Porträt steht wie ein Cover in der Zelle.
4. **Kein Bildtausch** — nie ersetzt ein Bild ein anderes, es füllt sich nur
   der Platzhalter.
5. **Der Negativ-Marker wird gelesen**, damit eine Zeile, für die feststeht,
   dass es kein Cover gibt, ihr Foto sofort zeigt statt auf ein Urteil zu
   warten, das längst gefallen ist.
6. **Ein Tile für alle Flächen** — Releases, Concerts und der Updates-Popover
   teilen weiter genau ein Aussehen, samt neuer Geometrie.
7. **Ein PR** mit getrennten Commits, kein Zweischritt.
8. **Die Abnahme zählt Silhouetten** — sonst weiß niemand, ob die Kette in
   diesem Bestand Fotos oder graue Flächen produziert.

Gelesen gegen `origin/dev` @ `653568247e`. Jede Zeilenangabe stammt aus diesem
Stand. **Der Haupt-Checkout hängt 47 Commits zurück** und kennt
`releases_cover_column.rs` noch gar nicht — wer dort prüft, prüft das Falsche.

## Der Kern in einem Satz

Die Zelle hat heute zwei Zustände, Cover oder Initialen, und der zweite ist
sowohl hässlich als auch schief; sie bekommt einen dritten (das Porträt), und
der Platzhalter wird von einem Widget-Stapel zu **einer** gezeichneten Fläche —
womit die Zentrierung keine Alignment-Frage mehr ist, sondern eine Rechnung im
selben Koordinatensystem wie der Grund.

## Befund (gemessen)

### Die Schieflage ist echt

Aus dem Screenshot des Nutzers, 1:1-Pixel, Initialen-Ink gegen Tile-Rechteck:

| Tile | links | rechts | oben | unten |
| --- | --- | --- | --- | --- |
| MC (Mental Cruelty) | 16 px | 9 px | 23 px | 24 px |
| RO (Ritual of Despair) | 18 px | 10 px | 24 px | 24 px |

Also rund 4 px nach rechts, vertikal sauber. Das Tile misst dabei 57×63 px —
**kein Quadrat**: es füllt die Zeilenhöhe, obwohl `widths::COVER`
(`table_column_widths.rs:27`) als „a square release-cover" dokumentiert ist.
Echte Cover werden dadurch oben und unten beschnitten.

### Der Bauplan allein erklärt sie nicht

Drei headless nachgebaute Fassungen des Widget-Baums (GTK 4.22.4, X11/Xvfb)
zeigen das Label **exakt mittig**:

| Nachbau | Ergebnis |
| --- | --- |
| Overlay + DrawingArea + zentriertes `title-3`-Label in einer `ColumnView`-Zelle | Label bei x=11, Soll 11.0 |
| dito, plus unsichtbare State-Labels, Picture, Hairline, Zeile höher als 56 | Label bei x=11, Soll 11.0 |
| dito, plus `source_context_surface`-Wrapper (`cell { padding: 0 }`, Surface `8px 6px`, `hexpand` auf dem Kind) | Ink-Abstände 12/12 |

Keiner dieser Nachbauten lud die App-CSS. Übrig bleiben als Verdächtige die
Stylesheets der App und die Schrift der echten Sitzung. **Die Ursache ist
offen — und der Umbau unten braucht sie nicht:** wer Grund und Glyphen in
denselben Cairo-Kontext malt, kann sie nicht mehr gegeneinander verschieben.
Commit 1 klärt sie trotzdem, weil ein reproduzierter Fehler der bessere
Regressionsschutz ist als ein umgangener — mit Stoppregel, siehe dort.

### Wo das Tile herkommt

| Stelle | Datei @ `653568247e` | Verhalten |
| --- | --- | --- |
| Das Tile | `crates/reprise-gnome/src/ui/updates/release_cover.rs:51-120` | `GtkOverlay`: `DrawingArea` füllt die Zelle mit `accent_rgba()`, darüber ein `title-3`-Label mit Initialen, darüber `GtkPicture`, darüber eine Hairline in Weiß 22 % |
| Zustand pro Zelle | `release_cover.rs:104-110` | drei unsichtbare Labels (`mbid`, `artist`, `started`) — bewusst, weil GTK sie mit der recycelten Zelle besitzt |
| Bindung Releases | `crates/reprise-gnome/src/ui/releases/releases_cover_column.rs:19-47` | `set_release(mbid, artist)`, Cover-Abruf beim `map` |
| Bindung Concerts | `crates/reprise-gnome/src/ui/concerts/concerts_artist_cover.rs:175-224` | `set_artist_key(artist)` + `image.show(&tile)` |
| Cover-Abruf | `crates/reprise-core/src/cover_download.rs:151-178` | `Image(path)` oder `Fallback`; `negative_marker_blocks` (`:138-145`) hält einen Negativ-Marker 7 Tage für bindend |
| CSS | `crates/reprise-gnome/src/ui/updates/css.rs:150-154` | `.new-release-cover` — Radius 4 px, Mindestmaß 44 px |
| Regel | `docs/ux-rules.md:2273` (NR-2) | „immediately shows an equally sized tile made of the effective accent color from STYLE-8 plus initials" |

### Das Bandfoto gibt es längst

`reprise_core::artist_portrait` liefert Porträts, `ArtistPortraitRuntime`
(`ui/now_playing/artist_portrait_worker.rs:32-130`) kapselt Gate und Drossel.
Zwei Flächen benutzen das bereits:

- **My Stats** (STATS-23): Porträt → Albumcover → Initialen,
  `ui/stats/stats_artist_image.rs:1-60`.
- **Concerts** (CONC-17a): `concerts_artist_cover.rs:65-137` — Cache-Lookup im
  Worker, bei Miss `runtime.request_while(…)`, mit Generationsschutz gegen die
  recycelte Zelle; verdrahtet in `ui/window/window_content_pages.rs:91-101`.

Nur die Releases-Tabelle fragt nie danach: `set_release` schreibt Initialen und
das war es (`release_cover.rs:132-160`).

**Risiko, das im Plan stehen bleibt:** Deezer liefert für unbekannte Bands
graue Silhouetten aus, die nicht zuverlässig an der Bild-ID erkennbar sind
(bekannte IDs: `MISSING_IMAGE_IDENTIFIERS` in
`crates/reprise-core/src/artist_portrait/deezer.rs`). Genau dieses Feld —
Underground-Metal — trifft das überdurchschnittlich oft. Inhaltsbasierte
Erkennung ist im Produktivcode verboten; Entscheidung 8 macht daraus einen
Messschritt in der Abnahme.

## Die Zustandsfolge in der Zelle

Beim Binden, in dieser Reihenfolge:

1. **Cover auf Platte** → sofort zeigen. Kein Porträt wird angefragt.
2. **Frischer Negativ-Marker** → für diese Veröffentlichung kommt nie ein
   Cover. Gecachtes Porträt sofort zeigen; bei Cache-Miss Tile zeigen und die
   Porträt-Kette starten.
3. **Nichts bekannt** → Tile zeigen, Cover-Abruf starten (unverändert, nur für
   sichtbare Zellen). `Image` → Cover einsetzen. `Fallback` → jetzt die
   Porträt-Kette.

Der Tausch-Verzicht (Entscheidung 4) lebt in Fall 3: solange offen ist, ob ein
Cover kommt, wird kein Foto gezeigt. Fall 2 hat kein Tauschrisiko, weil das
Urteil schon vorliegt — deshalb darf er sofort. Ab dem zweiten Start ist Fall 2
der Normalfall für alles, was die Cover Art Archive nicht kennt.

## Der PR (`gnome` + ein Helfer in `core`)

Ein Branch, vier Commits in dieser Reihenfolge. Die genannten Dateien sind
Startpunkt, kein Zaun: reicht eine Liste nicht, um den Vertrag zu erfüllen,
gilt der Vertrag — anhalten nur, wenn *er* falsch ist.

### Commit 1 — Die Schieflage reproduzieren

*Test zuerst.* Neuer Display-Test neben den bestehenden in `release_cover.rs`
(`#[ignore = "requires a display; run via xvfb-run"]`), der die App-CSS lädt
wie `concerts_visual_tests.rs:44-52`
(`style::install_css_string_for_test(theme_css + app_css_for_test)`), das Tile
über `source_context_surface::wrap` in eine echte `ColumnView`-Zelle hängt,
`set_release("…", "Mental Cruelty")` bindet und misst: Ink-Kasten der Initialen
(`layout.pixel_extents()` plus Position des Labels im Overlay) gegen den Kasten
des Grund-`DrawingArea`. Urteil `|links − rechts| ≤ 1`.

**Stoppregel.** Ist der Test rot, ist die Ursache gefunden — im Commit-Text
festhalten, welcher CSS-Anteil sie trägt. Ist er grün, liegt die Ursache
außerhalb dessen, was ein Test greifen kann (Sitzungsschrift, Skalierung):
genau das festhalten und **sofort** zu Commit 2 weitergehen. Kein Weitersuchen,
der Umbau nimmt beiden Fällen die Grundlage.

### Commit 2 — Ein `DrawingArea` malt Grund und Initialen

*Test zuerst.* Die Messung aus Commit 1 wird auf Pixel umgestellt: Tile in eine
Textur rendern (`GtkSnapshot` → `GskRenderer::render_texture` →
`Texture::download`), Ink-Kasten der Glyphen über den Farbabstand zum Grund
bestimmen, Urteil `|links − rechts| ≤ 1 && |oben − unten| ≤ 1`. Dazu ein
zweiter Fall mit einem einzelnen breiten Buchstaben („W") — eine Zentrierung
auf die Glyphenbox statt auf die Ink-Extents fällt genau daran auf.

*Dann der Code.* Neue Datei `ui/updates/release_cover_tile.rs` mit der reinen
Zeichenfunktion (Signatur als Vorschlag, nicht als Vertrag):

```rust
pub(in crate::ui) fn draw(
    context: &gtk4::cairo::Context,
    pango: &gtk4::pango::Context,
    width: f64,
    height: f64,
    initials: &str,
    is_dark: bool,
    foreground: gtk4::gdk::RGBA,
)
```

- Der Text zieht als unsichtbares State-Label mit (`INITIALS_CLASS`, wie
  `mbid`/`artist`/`started` es schon tun) — `initials_text()` und die
  Rebind-Tests lesen weiter dasselbe. `set_release`/`set_artist_key` rufen nach
  dem Setzen `queue_draw()`.
- Zentriert wird auf die **Ink-Extents**: `x = (w − ink.width)/2 − ink.x`,
  vertikal analog. Das ist der Unterschied zwischen metrisch und optisch
  mittig.
- Das `title-3`-Label entfällt als sichtbares Widget, die Klasse wandert
  ersatzlos raus.

**Bildsprache** — Farben ausschließlich über `ui/style` (kein Hex im Rust,
`accent::accent_rgba()`, Kontrastrechnung in `style/color_math.rs`):

| Element | dunkel | hell |
| --- | --- | --- |
| Grund, senkrechter Verlauf | `alpha(accent, 0.20)` → `alpha(accent, 0.08)` | `alpha(accent, 0.16)` → `alpha(accent, 0.06)` |
| Hairline | `alpha(fg, 0.14)` | `alpha(fg, 0.10)` |
| Initialen | Akzent, gegen den Tile-Grund auf ≥ 4.5:1 gezogen | dito |
| Ecke | 4 px wie die Cover | dito |
| Schrift | fett, `0.34 × Kante` (bei 56 px → 19 px), Laufweite +0.04 em | dito |

Der Verlauf wird mit Alpha über den Zeilengrund gemalt, nicht über eine
gedachte Fläche — dadurch trägt der helle Modus von selbst hell. Reicht
`accent_text_color` (`style/accent.rs:111`) für die Textfarbe, wird **er**
sichtbar gemacht statt einer zweiten Kontrastrechnung.

**Reichweite (Entscheidung 6):** `new_cached_artist` (`release_cover.rs:37`),
der Updates-Zeilenkopf (`release_row.rs:104`) und die Concerts-Spalte bekommen
dieselbe Optik. Kein Varianten-Flag.

### Commit 3 — Quadrat statt Zeilenhöhe

*Test zuerst.* Ein Display-Test, der Breite und Höhe des Tiles in einer Zeile
misst, die höher als 56 px ist: beide Male 56, vertikal mittig.

*Dann der Code.* `root.set_halign(Center)` + `set_valign(Center)` in
`new_unbound`. Achtung: `source_context_surface::wrap` setzt `hexpand(true)`
auf das Kind (`source_context_surface.rs:63`) — mit `halign: Center` nimmt das
Overlay trotzdem seine natürliche Breite. Betrifft Concerts mit, gehört in die
Abnahme-Screenshots.

### Commit 4 — Die Porträt-Kette

**4a — Der Cover-Zustand bekommt einen Namen (`core`).**

*Test zuerst.* In `cover_download.rs`: drei Lagen, alle über `now` gesteuert,
nie über echte Wartezeiten — Bild vorhanden → `Cached`; frischer Marker →
`KnownMissing`; abgelaufener Marker und nichts vorhanden → `Unknown`.

*Dann der Code.*

```rust
pub enum CoverState { Cached(PathBuf), KnownMissing, Unknown }
pub fn release_group_cover_state(mbid: &str) -> CoverState
```

**`fetch_release_group_cover` muss auf dieselbe Funktion umgestellt werden**
(`:159-168`), sonst steht dieselbe Entscheidung an zwei Stellen und driftet
auseinander. Mutationsnachweis gehört dazu: wer die neue Funktion verfälscht,
muss beide Aufrufer rot sehen.

**4b — Die Kette wandert aus Concerts heraus (`gnome`).**

*Test zuerst.* Die vorhandenen Tests in `concerts_artist_cover.rs` müssen nach
dem Umzug unverändert grün bleiben — sie sind der Beweis, dass der Umzug nichts
am Verhalten dreht. Wer eine davon anfassen muss, hat mehr umgebaut als
verschoben.

*Dann der Code.* `ConcertsArtistImage` wird zu
`ui/artist_portrait_tiles.rs::ArtistPortraitTiles` (Name frei, Ort nicht: die
Datei gehört keiner der beiden Tabellen). Mit umziehen: Cache-Resolver,
Generationsschutz, `request_while`-Pfad. **Nicht** mit umziehen: `cover_column`
— die Spalte bleibt in Concerts. Die Kette bekommt ein eigenes State-Label
(`PORTRAIT_CLASS`), damit ihr Schlüssel nicht mit `started` kollidiert, das den
Cover-Abruf bewacht.

**4c — Die Releases-Zelle benutzt sie.**

*Test zuerst.* Fünf Fälle, alle mit Fake-Resolver — kein Netz, keine echte
XDG-Cache-Lage:

1. Cover auf Platte → der Porträt-Resolver wird **0-mal** gerufen.
2. `KnownMissing` und Porträt im Cache → das Tile zeigt das Bild **beim
   Binden**, ohne dass ein Cover-Abruf gelaufen ist.
3. `Unknown`, Abruf endet in `Fallback`, Porträt im Cache → Bild danach.
4. `Fallback`, kein Porträt, Modul aus
   (`ArtistPortraitRuntime::for_test(false, …)`) → **kein** Request.
5. Zelle wird während eines laufenden Porträt-Abrufs neu gebunden → das Bild
   der alten Zeile landet nie in der neuen (derselbe Schutz wie
   `style_13_releases_cover_rebinds_when_the_row_changes` für Cover).

*Dann der Code.* `set_release` liest `release_group_cover_state`, legt den
Interpretennamen ins `artist`-Label statt es zu leeren, und stößt in Fall 2 die
Kette sofort an; `start_fetch` (`release_cover.rs:236-266`) stößt sie am
`Fallback`-Zweig an.

**4d — Verdrahtung.** `ReleasesView::set_artist_image(loader, runtime)` als
Zwilling zu `ConcertsView::set_artist_image` (`concerts_view.rs:336-342`),
gerufen in `window_content_pages.rs` direkt nach `releases::install` — dort
liegen `cover_loader` und `artist_portrait` schon als Parameter (`:80-83`).

**4e — Die Regeln nachziehen.**

- `docs/ux-rules.md:2273`: **NR-2** → `[replaced by NR-2a]`, neue Regel NR-2a
  mit der Zustandsfolge oben (Cover → Marker → Porträt über das
  NET-1a-Artwork-Gate → Tile), gedämpfter Optik statt Akzentfläche,
  quadratischem Tile und dem Satz, dass nie ein Bild gegen ein anderes
  getauscht wird.
- `docs/ux-rules.md:5502` (**CONC-17a**) verweist auf „the accent-coloured
  initials tile of NR-2" — Verweis auf NR-2a ziehen und die Farbe herausnehmen,
  sonst beschreibt die Concerts-Regel ein Tile, das es nicht mehr gibt.
- Beide Regeln nennen ihre Tests namentlich, wie die Nachbarregeln es tun.

## Abnahme

```
cargo test -p reprise-core -p reprise-gnome > $LOG/suite.log 2>&1
xvfb-run -a cargo test -p reprise-gnome -- --ignored > $LOG/display.log 2>&1
```

`reprise-gnome` ist ein reines Bin-Crate (`[[bin]] name = "reprise"`), hat also
gar kein Lib-Target; `--lib` bricht dort mit „no library targets found" ab.
Ohne den Schalter laufen die Unit-Tests des Bin-Targets — genau das, was die
Repo-Skripte (`check-display-tests.sh`) auch tun.

Urteil aus `grep -c '^test result: FAILED' $LOG/*.log`, nie aus der letzten
Zeile und nie durch eine Pipe.

Dazu drei Messungen, nicht drei Eindrücke:

1. **Zentrierung.** Screenshot der Releases-Ansicht hell *und* dunkel mit
   mindestens zwei Platzhalter-Zeilen; Ink-Abstände nachmessen (Verfahren wie
   im Befund oben). Soll: symmetrisch ± 1 px, Tile quadratisch. Concerts
   bekommt denselben Screenshot-Satz, weil es dieselbe Fläche benutzt.
2. **Wirkung der Kette.** Gegen die echte Bibliothek: Releases öffnen, warten,
   bis die sichtbaren Zeilen entschieden sind, und zählen, wie viele Cover,
   Porträt oder Tile zeigen. Die Zahlen gehören in den PR-Text — sie sind die
   einzige Aussage darüber, ob die Kette in diesem Bestand etwas ausrichtet.
3. **Silhouetten (Entscheidung 8).** Von den gezeigten Porträts messen, wie
   viele Deezers graue Silhouette sind — Flachheitsprüfung der Bytes
   (~200 Farben, RMSE ≈ 0,06) als **Diagnoseschritt**, nicht als Produktivcode.
   Gefundene Bild-IDs wandern in `MISSING_IMAGE_IDENTIFIERS`. Ist die Quote
   hoch, ist das ein eigener Folgeplan, kein Nachbessern in diesem PR.

## Was nicht gebaut wird

- **Kein Albumcover-Rückfall** wie in My Stats: ein fremdes Album als Bild
  einer fehlenden Veröffentlichung ist eine Falschaussage in einer Tabelle, die
  genau über Veröffentlichungen Auskunft gibt.
- **Kein Abzeichen, keine Entsättigung** am Porträt (Entscheidung 3).
- **Kein Tooltip** auf der Zelle: `set_tooltip_text` kostet einen
  Display-Roundtrip pro Zelle, und der Interpretenname steht in der
  Nachbarspalte.
- **Keine inhaltsbasierte Silhouetten-Erkennung im Produktivcode** — verboten,
  siehe Befund; die Messung bleibt manuell.
- **Kein Vorlauf**, der Porträts im Hintergrund für alle 131 Zeilen zieht. Erst
  die Zahlen aus der Abnahme, dann diese Frage.
- **Kein Varianten-Flag** für die alte Akzentfläche (Entscheidung 6).

## Parallelität

**Ein Strang, ein Branch** (Entscheidung 7). Alle vier Commits fassen
`release_cover.rs` an, Commit 4 baut auf der gezeichneten Fläche aus Commit 2
auf. Zwei Worktrees träfen sich an derselben Datei, ohne dass eine Hälfte
früher fertig wäre.
