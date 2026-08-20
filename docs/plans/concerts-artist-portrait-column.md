---
slug: concerts-artist-portrait-column
worktree: /home/marvin/Projects/reprise-concerts-artist-portrait-column
branch: feature/concerts-artist-portrait-column
phase: refactored
codex_session:
created: 2026-08-20
---
# Ein Künstlerbild führt die Konzertzeile an

> Alle Zeilennummern gegen `origin/dev` @ `799942cdc4` (20.08.2026). Jede hier
> angefasste **Code**-Datei ist dort identisch mit dem lokalen Arbeitsbaum
> (geprüft per `git diff --quiet origin/dev -- <pfad>`). Einzige Ausnahme:
> `docs/ux-rules.md` trägt lokal eine fremde, unveröffentlichte Änderung
> (SEARCH-16 der Centering-Arbeit) — der Worktree dieses Plans schneidet von
> `origin/dev` ab und sieht sie nicht.

## 0. Auftrag

Die Concerts-Tabelle bekommt eine führende Bildspalte mit dem Porträt des
Künstlers — dasselbe Muster, das die Releases-Tabelle seit NR-33 mit ihrem
Cover fährt. Zugleich wächst die Kachel in **beiden** Tabellen von 40 auf
56 px, weil 40 px das Bild zur Briefmarke macht.

### Die Beschlüsse

Alle sechs sind entschieden — der Plan setzt sie um, er verhandelt sie nicht
neu. Die ersten beiden kamen vom Auftraggeber, die übrigen vier aus dem Grill
des Entwurfs:

1. **56 px, in beiden Ansichten.** `widths::COVER` 40 → 56.
2. **Das Bild wird bei Bedarf nachgeladen**, nicht nur aus dem Cache gezeigt.
   Bis es da ist, steht die Initialen-Kachel auf der Akzentfarbe (NR-2).
3. **Die Spalte entsteht in `append_columns()`**, nicht davor in
   `ConcertsView::new`. Grund in 2.1.
4. **Kein Kachel-Refactor.** `LazyReleaseCover` wird um einen Künstlerschlüssel
   erweitert, statt die Kachel in ein geteiltes Modul zu heben. Grund in 2.2.
5. **Jede sichtbare Zeile darf abrufen**, auch die `is_similar`-Empfehlungen.
   Deezer sieht damit auch Namen außerhalb der Bibliothek; gedeckt ist das vom
   Artwork-Schalter aus NET-1a, der alles zusammen abschaltet.
6. **Eckige Kachel, 4 px Radius, wie das Release-Cover** — nicht rund wie der
   `adw::Avatar` in My Stats. Das Updates-Popover bleibt bei 44 px und bei
   seinem Nur-Cache-Verhalten; sein Cache füllt sich künftig über die Tabelle.

## 1. Ausgangslage (verifiziert)

### 1.1 Die Kachel, die es schon gibt

`ui/updates/release_cover.rs` (336 Z.) baut `LazyReleaseCover`: ein
`gtk4::Overlay` mit `set_size_request(edge, edge)`, darin eine `DrawingArea`
in der Akzentfarbe, ein Initialen-`Label`, ein `gtk4::Picture`
(`ContentFit::Cover`, anfangs unsichtbar), eine Haarlinien-`DrawingArea` und
**zwei unsichtbare Zustands-Labels** (`reprise-release-cover-mbid`,
`reprise-release-cover-started`). Die Labels sind der Kunstgriff, der das
Modul ohne `unsafe` qdata und ohne eigene GObject-Unterklasse recyclingfest
macht: `from_widget()` rekonstruiert den Wrapper allein aus den CSS-Klassen
der Kinder, also findet auch ein während `bind` neu gebauter Wrapper denselben
Per-Zellen-Zustand.

Drei Aufrufer hängen daran, alle drei bleiben unangetastet:

| Aufrufer | Einstieg | Kante |
|---|---|---|
| `ui/releases/releases_columns.rs:40` `cover_column()` | `new_unbound` + `set_release` | `widths::COVER` |
| `ui/updates/release_row.rs:19` | `new` | `COVER_EDGE = 44` |
| `ui/updates/concerts_section.rs:218` | `new_cached_artist` | `COVER_EDGE = 44` |

Der dritte ist der Beleg, dass Konzertzeile und Künstlerporträt hier schon
zusammenfinden — allerdings **nur aus dem Cache**: `new_cached_artist(artist,
edge, allowed)` ruft `artist_portrait::load_cached` und startet nie eine
Anfrage.

### 1.2 Die Spalte, die es zu kopieren gilt

`ui/releases/releases_columns.rs:40-82`: Factory mit `setup`/`bind`/`unbind`,
`ColumnViewColumn` **ohne `id`**, `resizable(false)`,
`widths::pin(&column, widths::COVER)`, als erste Spalte angehängt. Die
Id-Losigkeit ist Pflicht, nicht Stil: `ui/table_columns/registry.rs:74-77`
bricht mit „pinned column `{id}` must not expose an editable id" ab, wenn eine
gepinnte Spalte doch eine trägt. Umgekehrt ordnet
`bind_view_column_keys` (`registry.rs:49-95`) jede **unbenannte** Spalte vor
der ersten benannten den Leading-Pins zu — genau der Aufbau, den diese Arbeit
herstellt.

`reprise-view/src/columns/release.rs`: `ReleaseColumn::Cover` steht in `ALL`,
**nicht** in `DEFAULT_VISIBLE`, `pin()` liefert `Some(Pin::Leading)`.

### 1.3 Belegt: keine Migration nötig

`layout::parse` (`reprise-view/src/columns/layout.rs:69-75`) reicht **jedes**
gelesene Layout durch `normalize` (`:29-54`). Dort werden zuerst alle Pins in
den Sichtbarkeitssatz geschrieben (`:30-34`), dann die führenden Pins an den
Anfang der Reihenfolge gesetzt (`:35-39`), und erst danach das freie Band in
der Reihenfolge des Nutzers. Ein Concerts-Layout, das vor dieser Spalte
gespeichert wurde, kennt den Namen `cover` nicht — und bekommt ihn beim Lesen
vorangestellt und sichtbar. Die vorhandenen Tests dazu:
`normalize_forces_every_pin_visible` (`:148`) und
`a_pin_can_be_neither_hidden_nor_moved` (`:177`).

**Es braucht also keine Migration v76.** T2 sichert das mit einem eigenen Test
ab; siehe auch 4.3 für den Fall, dass er wider Erwarten rot ist.

### 1.4 Belegt: die 56 px kommen bei Releases wirklich an

Eine seit Monaten gespeicherte Spaltenbreite könnte die neue Konstante
aushebeln — tut sie aber nicht. `width_persistence::restore_stored_widths`
(`ui/table_columns/width_persistence.rs:41-64`) schreibt eine gespeicherte
Breite nur zurück, wenn `key.pin().is_none()`; `save_widths_now` (`:66-77`)
und der Listener-Schnappschuss (`:80-90`) filtern Pins ebenso heraus. Die
Cover-Spalte ist ein Leading-Pin, ihre Breite kommt also immer aus
`widths::COVER`.

### 1.5 Woher das Künstlerbild kommt

`reprise_core::artist_portrait` holt bei Deezer, bevorzugt `picture_xl`
(**1000×1000**), sonst `picture_big` (500×500); Plattencache unter
`~/.cache/reprise/artist-portraits`, 30 Tage TTL für Treffer,
`.notfound`-Marker mit 7 Tagen für Fehlschläge. Deezers Platzhalterbilder
werden schon heute erkannt und übersprungen
(`artist_portrait/deezer.rs`, `defensive_fallback_skips_placeholder_xl_for_real_big`).

`ui/now_playing/artist_portrait_worker.rs::ArtistPortraitRuntime` ist der
**einzige zulässige Weg** dorthin. Er hält das NET-1a-Gate
(`network_allowed_or_off(conn, &modules::ARTWORK_MODULE)`, neu ausgewertet in
`recompute_enabled`), `request_would_run()`, `request_while(name,
still_visible, on_ready)` und deckelt auf `MAX_IN_FLIGHT = 3`. Erzeugt wird er
in `ui/window/window_runtime_setup.rs:59` als Feld `artist_portrait` der
`WindowRuntimes`.

`ui/stats/stats_artist_image.rs` ist das Vorbild für den Ablauf **und** für
die Testbarkeit: `StatsArtistImage::for_test(cover_loader, cached_portrait)`
injiziert den Cache-Auflöser, damit kein Test das echte XDG-Verzeichnis liest.

### 1.6 Der Speicherfallstrick

`LazyReleaseCover` hängt sein Bild per `picture.set_filename()` ein. Bei
Releases ist das harmlos: die Quelle ist `/release-group/{mbid}/front-250`
(`cover_download.rs:123-126`), also 250×250 ≈ 250 KB dekodiert.

Für ein Porträt gilt das **nicht**. 1000×1000 RGBA sind rund 4 MB pro Bild,
eine Konzertliste zeigt leicht 40–50 Zeilen gleichzeitig: 160–200 MB
Bilddaten für Kacheln von 56 px.

Der Ausweg liegt fertig herum. `ui/cover/cover_loader.rs::CoverLoader`
(573 Z.) skaliert beim Dekodieren auf eine `ThumbnailSize` und hält die
Ergebnisse in einem LRU-Cache mit dem Schlüssel `"{pfad}|{pixel}"` — bei einer
Tour mit zehn Terminen desselben Künstlers wird das Bild also **einmal**
dekodiert. `load_image_into_picture(picture, image_path, size, token, current,
on_loaded)` (`:195`) ist die passende Methode, `ThumbnailSize::Portrait`
(192 px) die passende Stufe: sie deckt 56 px auch auf einem 2×-HiDPI-Schirm
(112 px) mit Reserve und kostet 147 KB statt 4 MB.

Den produktiven `CoverLoader` gibt `track_list.shared_cover_loader()` heraus —
so bekommt ihn heute schon die Stats-Ansicht (`ui/window/window.rs:244`).

### 1.7 Zeilenhöhe und CSS

Weder `ui/concerts/css.rs` noch `ui/releases/css.rs` setzt eine Zeilenhöhe.
Die Höhe folgt dem höchsten Kind, und das ist die Kachel mit ihrem
`set_size_request(edge, edge)`. **Die Zeilen beider Tabellen wachsen also von
~40 auf ~56 px.** Beabsichtigt — aber es ist die Nebenwirkung, die man beim
Betrachten zuerst sieht, und sie gehört deshalb in die Abnahme.

`.new-release-cover { border-radius: 4px; min-width: 44px; min-height: 44px }`
(`ui/updates/css.rs:150-154`) ist ein Minimum, kein Fixum: 56 px überschreiben
es widerspruchsfrei, und der Radius von 4 px erfüllt Beschluss 6 ohne eine
Zeile neues CSS. Das Popover behält seine 44 px, weil `COVER_EDGE` dort eine
eigene Konstante ist und nicht angefasst wird.

## 2. Die Aufgaben

### T1 — `LazyReleaseCover` bekommt einen Künstlerschlüssel

In `ui/updates/release_cover.rs`, additiv (~25 Zeilen). Die bestehende
Oberfläche — `new`, `new_unbound`, `new_cached_artist`, `from_widget`,
`set_release`, `widget` — bleibt **wortgleich**, damit die drei Aufrufer aus
1.1 unverändert bleiben.

Neu:

```rust
const ARTIST_CLASS: &str = "reprise-release-cover-artist";

/// Binds this cell to an artist instead of a release: initials tile,
/// no image, and the artist as the cell's key. The MBID label stays
/// empty, so neither `set_release` nor the map handler can ever start
/// a release-cover fetch from a concert cell.
pub(in crate::ui) fn set_artist_key(&self, artist: &str)
pub(in crate::ui) fn artist_key(&self) -> String
pub(in crate::ui) fn show_paintable(&self, paintable: Option<&gtk4::gdk::Paintable>)
```

Das dritte unsichtbare Zustands-Label wird wie die zwei bestehenden in
`new_unbound` angelegt und in `from_widget` mit aufgelöst. `from_widget` darf
weiterhin `None` liefern, wenn ein Kind fehlt — bestehende Kacheln ohne das
neue Label gibt es zur Laufzeit nicht, alle entstehen durch `new_unbound`.

Der `started`-Zustand wird für Konzertzellen mitbenutzt: `set_artist_key`
setzt ihn zurück, T3 markiert damit den bereits angestoßenen Abruf.

Die zwei bestehenden `#[ignore]`-Display-Tests des Moduls
(`style_13_releases_cover_rebinds_when_the_row_changes`,
`style_13_releases_cover_fetches_again_when_rebound_without_unmap`) bleiben
unverändert und müssen weiter grün sein.

### T2 — `ConcertColumn::Cover`

`reprise-view/src/columns/concert.rs`: Variante `Cover` in den Enum, als
`"cover"` persistiert, in `ALL` an erster Stelle (`ALL` wächst auf 8),
**nicht** in `DEFAULT_VISIBLE`, `pin()` liefert `Some(Pin::Leading)` für sie
und `None` für alle anderen.

Zwei Tests in derselben Datei, im Stil der Releases-Nachbarn:

- `conc_17a_the_default_concert_layout_leads_with_the_cover` — die
  Vorgabereihenfolge ist `Cover`, `Artist`, `Date`, `City`, `Venue`,
  `Distance`, `Tickets`, `Source`; `Venue` und `Source` bleiben unsichtbar,
  `Cover` ist sichtbar.
- `a_concert_layout_stored_before_the_cover_gains_it_at_the_leading_edge` —
  `layout::parse::<ConcertColumn>("artist,date,city,venue,distance,tickets,source;artist,date,city,distance,tickets")`
  hat danach `Cover` an Position 0 **und** im Sichtbarkeitssatz. Das ist der
  Beleg für 1.3; solange er grün ist, ist keine Migration nötig.

`ui/concerts/concerts_column_layout.rs`: je ein Arm für `Cover` in `label()`
(`strings::COLUMN_COVER`) und `width()` (`widths::COVER`). Beide Bausteine
existieren und werden von Releases genauso benutzt.

### T3 — Die Bildspalte

Neu: `crates/reprise-gnome/src/ui/concerts/concerts_artist_cover.rs`, in
`concerts/mod.rs` eintragen.

**Der Halter.** `ConcertsArtistImage` als `Rc<Self>` — er trägt, was zur
Bauzeit der Spalte noch nicht existiert:

```rust
portrait:   RefCell<Option<Rc<ArtistPortraitRuntime>>>,
loader:     RefCell<Option<Rc<CoverLoader>>>,
cached:     Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>,
generation: Rc<Cell<u64>>,
```

Produktiv löst `cached` über `artist_portrait::load_cached` auf; ein
`for_test(resolver)`-Konstruktor injiziert stattdessen ein Testverzeichnis —
wortgleich zum Vorbild in `stats_artist_image.rs:54-63`. **Kein Test darf je
das echte XDG-Cacheverzeichnis lesen oder Deezer erreichen.**

**Die Spalte.** `cover_column(view: &gtk4::ColumnView, image: &Rc<ConcertsArtistImage>)`:

- `setup`: `LazyReleaseCover::new_unbound(widths::COVER)` als Zellkind, dazu
  ein `connect_map` auf die Kachel → `image.start(&tile)`. Das ist die Rolle,
  die `wire_lazy_fetch` für Releases spielt: eine Zelle, die erst beim
  Scrollen sichtbar wird, startet dort ihren Abruf.
- `bind`: `tile.set_artist_key(&row.artist_name)`, dann `image.show(&tile)`.
  Leerer Künstlername ⇒ nur die Initialen-Kachel („?"), kein Abruf.
- `unbind`: `tile.set_artist_key("")`.
- Spalte: **ohne `id`**, `resizable(false)`, **kein Sorter**,
  `title(strings::text(strings::COLUMN_COVER))`,
  `widths::pin(&column, widths::COVER)`.

**`show(&tile)` — der Cache-Weg:**

1. Cache-Auflösung off-thread über `gio::spawn_blocking`; `load_cached` prüft
   Dateien und gehört nicht auf den Main-Thread.
2. Zurück im Main-Kontext: trägt die Zelle inzwischen einen anderen
   `artist_key`, passiert nichts.
3. Treffer ⇒ `loader.load_image_into_picture(&sink, &pfad,
   ThumbnailSize::Portrait, …)` in ein **Senken**-`Picture` außerhalb des
   Widgetbaums; im Rückruf erneut `tile.artist_key() == artist` prüfen und
   erst dann `tile.show_paintable(sink.paintable().as_ref())`. Ohne gesetzten
   `CoverLoader` (Tests) bleibt es bei den Initialen.
4. Kein Treffer und die Zelle ist gemappt ⇒ `start(&tile)`.

**`start(&tile)` — der Netzweg.** Er wird auch direkt aus `bind` gerufen, weil
`ColumnView` eine bereits gemappte Zelle ohne zwischenzeitliches
`unmap`/`map` umhängen kann — genau die Falle, die
`style_13_releases_cover_fetches_again_when_rebound_without_unmap` bei
Releases einfängt:

1. Schlüssel leer, oder `started() == schlüssel` ⇒ nichts tun.
2. Kein Runtime gesetzt, oder `!runtime.request_would_run(&schlüssel)` ⇒
   nichts tun. **Das ist das NET-1a-Gate**; `artist_portrait::load_or_fetch`
   wird an keiner Stelle dieses Moduls direkt gerufen.
3. `mark_started(&schlüssel)`, dann `runtime.request_while(schlüssel.clone(),
   move || tile.artist_key() == schlüssel, move |found| …)`. Der
   `still_visible`-Rückruf ist zugleich der Wegwerfschalter für Zeilen, die
   weggescrollt wurden, **bevor** ein Warteschlangenplatz frei wurde — dafür
   ist er in `ArtistPortraitRuntime` gebaut.
4. Im Ergebnis-Rückruf ein drittes Mal den Schlüssel prüfen, dann über
   denselben Senken-Weg wie in `show` anzeigen.

**Zur Generation.** `load_image_into_picture` verlangt `token` und
`current: &Rc<Cell<u64>>`. Die Korrektheit trägt hier **nicht** dieses Paar,
sondern der Schlüsselvergleich — ein `Rc<Cell<u64>>` pro Zelle wäre über
`from_widget` gar nicht rekonstruierbar. Der Halter führt deshalb einen
spaltenweiten Zähler, der nie invalidiert; er bedient nur die Signatur. **Das
gehört als Kommentar an die Stelle**, sonst liest ein Reviewer es zu Recht als
Vergessenes.

Der eigentliche Grund, warum das sicher ist: der Schlüssel ist der
Künstlername, nicht die Zeile. Zwei Zeilen desselben Künstlers — bei einer
Tour der Normalfall — teilen Schlüssel, Cache-Eintrag und Paintable. Ein
zurückkehrender Abruf trifft entweder eine Zelle, die genau dieses Bild will,
oder gar keine.

### T4 — Einbau in `append_columns` und Verdrahtung

`ui/concerts/concerts_columns.rs:241` — `append_columns` bekommt einen vierten
Parameter:

```rust
pub(super) fn append_columns(
    view: &gtk4::ColumnView,
    query: &crate::ui::search_highlight::QuerySource,
    radius_source: &RadiusSource,
    image: &Rc<ConcertsArtistImage>,
) -> SortColumns
```

Die Cover-Spalte wird darin **als erste** angehängt, vor `date`. Damit sieht
der bestehende Breiten-Vertragstest sie automatisch mit — und genau darum
steht sie hier und nicht in `concerts_view.rs`: STYLE-9 existiert wegen
Spalten ohne feste Breite, und die eine neue Spalte, die keiner prüft, wäre
die falsche.

Die vier Aufrufer ziehen nach: `concerts_view.rs:111` (produktiv) und die drei
Tests in `concerts_columns.rs:355`, `:437`, `:458`.

`ui/concerts/concerts_view.rs`: den Halter in `new()` bauen, an
`append_columns` reichen, und nach außen geben:

```rust
pub(in crate::ui) fn set_artist_image(
    &self,
    loader: Rc<CoverLoader>,
    runtime: Rc<ArtistPortraitRuntime>,
)
```

`ui/window/window.rs`: direkt nach `concerts::install(…)` (`:304-308`)
aufrufen. Beide Werte sind dort im Zugriff — `artist_portrait` seit `:245`,
`track_list.shared_cover_loader()` seit `:244`.

**`ConcertsView::new` behält seine Signatur.** Das ist der Grund für den
`RefCell`-Halter: die sechs bestehenden Aufrufer (`concerts/mod.rs:33`,
`date_format_display_tests.rs:65`, vier in `concerts_view_tests.rs`) bleiben
unangetastet, und eine Ansicht ohne gesetzten Halter zeigt Initialen und rührt
kein Netz an — die beste Vorgabe, die ein Test haben kann.

### T5 — 56 px

`ui/table_column_widths.rs:27`: `COVER: i32 = 40` → `56`, Kommentar nachziehen
(„A compact square release-artwork cell" beschreibt jetzt auch Porträts).
Betroffen sind genau die beiden Tabellen. `release_row.rs` und
`concerts_section.rs` haben ihr eigenes `COVER_EDGE = 44` und bleiben, damit
das Updates-Popover nicht mitwächst.

### T6 — Die Regel

`docs/ux-rules.md`: **CONC-17** (`:5360` in `origin/dev`) wird auf
`[replaced by CONC-17a]` gesetzt, ohne Textänderung. Neu, im Wortlaut von
NR-33 und direkt darunter:

> - **CONC-17a** [active] [gtk] — replaces CONC-17. The Concerts table's
>   default columns are `Cover · Artist · Date · City · Distance · Tickets`,
>   with `Venue` and `Source` hidden; this is the default, not a fixed order,
>   and every unpinned column is hideable and movable. The `Cover` column stays
>   pinned at the leading edge, carries no id and no sorter, and shows the
>   artist's portrait for every row including similar-artist recommendations —
>   cached first, otherwise fetched through the artwork gate of NET-1a, with
>   the accent-coloured initials tile of NR-2 standing in until an image
>   resolves and remaining in place when none does. Sorting, filters, counts,
>   activation semantics and the migration-v75 note of CONC-17 are otherwise
>   unchanged.
>   Tests: `conc_17a_the_default_concert_layout_leads_with_the_cover`
>   (`reprise-view/src/columns/concert.rs`),
>   `conc_17a_the_concerts_cover_column_is_pinned_id_less_and_unsorted`,
>   `conc_17a_a_concert_cover_shows_initials_until_a_portrait_resolves` and
>   `conc_17a_a_rebound_concert_cover_never_shows_the_previous_artist`
>   (`ui/concerts/concerts_view_tests.rs`).

NR-2 bleibt unberührt: sie beschreibt die Herkunft der *Release*-Cover, und
die ändert sich nicht.

### T7 — Tests

In `ui/concerts/concerts_view_tests.rs`, benannt wie in der Regel, mit
`#[ignore = "requires a display; run via xvfb-run"]` wie ihre Nachbarn:

- `conc_17a_the_concerts_cover_column_is_pinned_id_less_and_unsorted` — erste
  Spalte, `id()` ist `None`, `sorter()` ist `None`, Titel ist
  `strings::COLUMN_COVER`, `fixed_width()` ist `widths::COVER`. Vorbild:
  `releases_view_tests.rs:330-342`.
- `conc_17a_a_concert_cover_shows_initials_until_a_portrait_resolves` — mit
  einem `for_test`-Auflöser, der `None` liefert: die Kachel zeigt die
  Initialen des Künstlers und **kein** Bild.
- `conc_17a_a_rebound_concert_cover_never_shows_the_previous_artist` — eine
  Zelle auf Künstler A binden, dann auf B: die Initialen wechseln, das
  `Picture` ist unsichtbar. Vorbild:
  `style_13_releases_cover_rebinds_when_the_row_changes`.

Der bestehende `style_9_concert_columns_keep_their_width_when_the_rows_change`
(`concerts_columns.rs`) deckt die neue Spalte durch T4 automatisch ab —
`instability()` verlangt von **jeder** Spalte ein `fixed_width > 0` und genau
einen Filler. Er muss grün bleiben, ohne dass jemand ihn erweitert.

`only_the_ticket_header_carries_no_sorter` (`concerts_view_tests.rs:116`)
prüft ausschließlich Spalten mit `id` und bleibt darum unverändert gültig; die
sorterlose Cover-Spalte ist für ihn unsichtbar.

## 3. Abnahme

1. `cargo test -p reprise-view` grün, insbesondere die zwei Layout-Tests aus T2.
2. `cargo test -p reprise-gnome` grün (ohne Display-Stufe).
3. Display-Stufe grün für die drei neuen Tests **und** die zwei bestehenden
   Cover-Tests aus T1.
4. `cargo clippy --all-targets -- -D warnings` grün.
5. **Sichtprüfung im laufenden Programm** — Screenshot, nicht Behauptung: die
   Concerts-Tabelle zeigt links eine 56-px-Kachel; wo ein Porträt vorliegt,
   steht das Bild, sonst die Akzentkachel mit Initialen. Die Zeilen sind
   sichtbar höher als vorher, in der Releases-Tabelle ebenso.
6. Mit ausgeschaltetem Artwork-Modul (oder ausgeschaltetem globalen
   Online-Schalter) startet **kein** Abruf: die Kacheln bleiben bei den
   Initialen, bereits gecachte Bilder bleiben sichtbar (NET-1).

## 4. Grenzen und Fallstricke

### 4.1 Was dieser Plan bewusst nicht tut

- Das Updates-Popover bleibt bei 44 px und beim Nur-Cache-Verhalten.
- Die Releases-Cover behalten `set_filename` und `front-250`. Der
  Thumbnail-Weg wäre auch dort sauberer, aber 250 px sind kein
  Speicherproblem, und ein Umbau des Releases-Bildpfads gehört nicht in
  diesen Auftrag.
- Keine neue Kachelabstraktion. Beschluss 4.

### 4.2 Risiken

| Risiko | Umgang |
|---|---|
| Die Zeilen werden zu hoch, die Tabelle wirkt luftleer | Sichtprüfung in der Abnahme; 56 ist gesetzt, eine Korrektur wäre ein eigener Auftrag |
| `docs/ux-rules.md` kollidiert beim Landen mit fremder Arbeit an derselben Datei | Der Einschub liegt bei `:5360`, die fremde Änderung (SEARCH-16) an ganz anderer Stelle — ein Rebase löst das ohne Raten |
| Deezer antwortet auf viele neue Namen langsam | `MAX_IN_FLIGHT = 3` deckelt bereits; die `.notfound`-Marker verhindern, dass ein Fehlschlag bei jedem Scrollen wiederholt wird |

### 4.3 Wenn eine Annahme fällt

Ist der zweite Test aus T2 rot, ist die Analyse in 1.3 falsch. Dann **stoppen
und melden** — keine Migration v76 dazuerfinden, ohne dass jemand die
Auswirkung auf gespeicherte Layouts geprüft hat.

## Parallelität

**Kein Schnitt. Ein Strang.**

T1 ist die Voraussetzung für T3, T3 für T4, T4 für T7, und T2 schreibt die
Spaltenidentität, aus der T4s Reihenfolge folgt — eine Kette, kein Fächer. Der
Grill hat den einzigen Kandidaten für einen zweiten Strang, den
Kachel-Refactor, gestrichen (Beschluss 4); übrig bliebe nur T5, eine
Konstante, die sich mit T3 ohnehin dieselbe Sichtprüfung teilt. Zwei Worktrees
für dieselbe Kette bringen keine Wanduhr-Zeit, nur zwei Rebases.

Post-Merge-Kreuzprüfungen: keine, weil es nur einen Strang gibt.
