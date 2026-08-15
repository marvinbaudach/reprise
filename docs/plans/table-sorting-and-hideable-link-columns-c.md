---
slug: table-sorting-and-hideable-link-columns-c
worktree: /home/marvin/Projects/reprise-table-sorting-and-hideable-link-columns-c
branch: feature/table-sorting-and-hideable-link-columns-c
phase: reviewed
codex_session:
created: 2026-08-15
---

# Strang C — Releases

## Zweck

Die Releases-Tabelle sortiert heute gar nicht: `wire_sorting`
(`releases_view.rs:663-683`) verbindet **nur** `primary_sort_order_notify` und
liest `primary_sort_column()` überhaupt nie; ein Klick auf „Artist" oder
„Release" wechselt die Spalte, nicht die Richtung — das verbundene Signal feuert
also nicht, und die Zeilen bleiben stehen, während der Pfeil umzieht. Selbst
wenn es feuerte, gäbe es nichts zu holen: `artist_news_view::sort_rows` nimmt
ausschließlich eine Richtung und ordnet immer nach `first_release_date`.

Zusätzlich sind `Status` und `Buy` als abschließende Pins fest verdrahtet und
lassen sich weder ausblenden noch verschieben noch im Editor sehen.

Dieser Strang gibt `sort_rows` einen Sortierschlüssel, repariert `wire_sorting`,
entpinnt Status und Buy und hängt den geteilten Ein-Pfeil-Helfer an.

**Vorbedingung:** Strang A ist gelandet.
`crates/reprise-gnome/src/ui/table_columns/single_sort_indicator.rs` existiert
mit `mark(&gtk4::ColumnView)`, `PRIMARY_SORT_INDICATOR_CLASS` und
`count_primary_indicators`. Ist die Datei nicht da, brich ab und melde es —
schreibe den Helfer **nicht** selbst.

Aufgaben in dieser Reihenfolge: **C-1, C-2, C-3, C-4.** Je Aufgabe ein
fokussierter Commit — mit **einer** harten Ausnahme, siehe C-3.

---

## Dateibesitz

Dir gehören diese Bäume vollständig:

```
crates/reprise-gnome/src/ui/releases/**
crates/reprise-view/src/columns/release.rs
crates/reprise-core/src/artist_news*.rs
```

Innerhalb dieser Bäume darfst du jede Änderung machen, die die Aufgaben
brauchen — auch an Dateien, die dieser Plan nicht namentlich nennt.

## Was dir **nicht** gehört

Fasse außerhalb deiner Bäume **nichts** an:

- `crates/reprise-gnome/src/ui/table_columns/**` — Strang A. Du **rufst** den
  Ein-Pfeil-Helfer, du änderst ihn nicht. Insbesondere `registry.rs` bleibt
  unberührt, auch `sort_fallback` und dessen Bestandstests (siehe RF-C4).
- `crates/reprise-gnome/src/ui/track_list/**`, `.../ui/style/**` — Strang A
- `crates/reprise-gnome/src/ui/concerts/**`,
  `crates/reprise-view/src/columns/concert.rs`,
  `crates/reprise-core/src/db*.rs` — Strang B, **läuft parallel zu dir**
- `crates/reprise-view/src/columns/key.rs`, `.../layout.rs` — unberührt; `Pin`
  behält seine zwei Zustände und seine Bedeutung. Es wird **kein** dritter
  Pin-Zustand eingeführt.
- `crates/reprise-core/src/lib.rs` — du legst keine neue Datei an
- `docs/ux-rules.md` — Strang R. Du schreibst den Test `nr_39_…`, aber **nicht**
  die Regel NR-39. Wenn dir auffällt, dass eine Regel durch deine Arbeit falsch
  wird: in den Bericht damit, nicht in die Datei.
- `crates/reprise-gnome/src/ui/strings_releases.rs` — im Alleinbesitz einer
  parallel laufenden fremden Arbeit. Du brauchst dort **nichts**:
  `RELEASES_STATUS` und `RELEASES_LINK` existieren und werden von
  `releases_column_layout.rs::label()` bereits verwendet. `po/POTFILES.in`
  bleibt ebenfalls unverändert.

---

## Kollision mit laufender fremder Arbeit

`crates/reprise-gnome/src/ui/releases/releases_view.rs` steht im Alleinbesitz
von Strang 2 der noch nicht gelandeten Arbeit `updates-concerts-releases-rework`
(`docs/plans/updates-concerts-releases-rework-2.md`). Der Nutzer hat entschieden,
trotzdem jetzt zu bauen.

- Die Bereiche sind **semantisch disjunkt**: jener Strang baut `build_footer()`
  (`:421`) und `apply_footer()` (`:476`) auf ein gemeinsames `feed_footer.rs`
  um; du fasst nur `wire_sorting` (`:663-683`, am Dateiende) und die Zeile
  `:233` an.
- Der Konflikt ist **positionell, nicht inhaltlich**. Bei einem Merge-Konflikt:
  **beide Seiten übernehmen.** `wire_sorting` ist eine geschlossene Funktion am
  Dateiende — als Ganzes aus diesem Zweig übernehmen, der Rest der Datei aus dem
  fremden Strang.
- Prüfung **nach** der Auflösung, nicht vorher: `wire_sorting` verbindet beide
  `notify`-Signale, `sort_key_for_id` wird gerufen, die Zeile mit
  `sort_by_column(Some(&date_column), Descending)` steht weiterhin **nach**
  `wire_sorting`, und `single_sort_indicator::mark` steht **vor** ihr.
- Halte deinen Eingriff in dieser Datei so klein wie möglich. Keine
  Aufräumarbeiten, keine Umformatierung, keine Umsortierung von Funktionen.

---

## Die gemeinsame Vergleichsregel (MISSING-LAST)

Bindend für **jede** neue Textsortierung dieses Strangs. Ein Feld, das leer ist
oder nur aus Leerzeichen besteht, wird wie ein fehlender Wert behandelt und
landet **richtungsunabhängig am Ende** — dieselbe Semantik, die
`compare_release_dates` für fehlende Daten schon hat (`Some` vor `None`,
unabhängig von der Richtung).

```rust
/// Leerer oder reiner Leerzeichen-Wert = fehlender Wert.
fn present(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn compare_text(left: &str, right: &str, direction: ReleaseSortDirection) -> Ordering {
    match (present(left), present(right)) {
        (Some(left), Some(right)) => {
            let ordering = left
                .to_lowercase()
                .cmp(&right.to_lowercase())
                .then_with(|| left.cmp(right));
            match direction {
                ReleaseSortDirection::Ascending => ordering,
                ReleaseSortDirection::Descending => ordering.reverse(),
            }
        }
        // Richtungsunabhängig: fehlende Werte stehen in beiden Richtungen
        // hinten, wie compare_release_dates es für fehlende Daten tut.
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
```

Gilt für **Title, Artist und Type**.

---

## Aufgabe C-1 — Sortierschlüssel im Kern

**Ziel:** `artist_news_view::sort_rows` nimmt einen Schlüssel entgegen; das
Datumsverhalten bleibt bitgleich.

**Bereich:** `crates/reprise-core/src/artist_news_view.rs`,
`crates/reprise-core/src/artist_news.rs` (Re-Export),
`crates/reprise-core/src/artist_news_view_tests.rs`,
`crates/reprise-gnome/src/ui/releases/releases_view.rs` (eine Zeile).

### Vorgehen

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseSortKey { Date, Title, Artist, Type }

pub fn sort_rows(
    rows: Vec<HistoryEntry>,
    key: ReleaseSortKey,
    direction: ReleaseSortDirection,
) -> Vec<HistoryEntry>
```

- **`Date`-Arm:** der heutige Rumpf **unverändert**
  (`compare_release_dates(…).then_with(title-tiebreak)`).
- **`Title` / `Artist` / `Type`:** `compare_text` nach der Vergleichsregel oben;
  danach — **richtungsunabhängig absteigend** —
  `compare_release_dates(left, right, Descending)` als Gleichstand-Entscheider.
  Absteigend, weil das zur Vorgabesortierung „neueste zuerst" passt. Kommentar
  an die Stelle:

  ```rust
  // Immer absteigend, unabhängig von `direction`: der Entscheider stellt
  // Stabilität her, er drückt keine Ordnung aus. Würde er mitdrehen, sprängen
  // gleichnamige Zeilen beim Richtungswechsel doppelt.
  ```
- **`Type` vergleicht das rohe `release_type`-Feld**, nicht das lokalisierte
  Label aus `releases_presentation::release_type_label` — `reprise-core` kennt
  die Übersetzung nicht. Konsequenz in eine Zeile Kommentar: in einer Sprache,
  in der die Labels anders alphabetisieren als die Rohwerte, weicht die sichtbare
  Reihenfolge ab. Das ist der Preis dafür, dass die Sortierung im Kern und damit
  ohne GTK testbar bleibt.
- Interner Aufrufer `artist_news_view.rs:186` bekommt `ReleaseSortKey::Date`.
- Re-Export in `artist_news.rs:89` (`sort_rows as sort_release_rows`) bleibt
  namensgleich, nur mit neuer Signatur. **Kein Kompatibilitäts-Wrapper** — der
  einzige externe Aufrufer ist `releases_view.rs:681` und wird in derselben
  Änderung auf `ReleaseSortKey::Date` gezogen; C-2 baut ihn dann richtig um.

### Nachweis (Unit)

```
cargo test -p reprise-core artist_news_view > $SCRATCH/c1.log 2>&1
grep -E "^test result|running [0-9]+ tests|FAILED|panicked" $SCRATCH/c1.log
```

- `release_sort_keeps_invalid_dates_last_and_uses_title_tiebreak`
  (`artist_news_view_tests.rs:570`) **unverändert im Verhalten**, nur um das neue
  `ReleaseSortKey::Date`-Argument ergänzt. Ändert sich hier eine Erwartung, ist
  der `Date`-Arm nicht bitgleich geblieben.
- `release_sort_by_title_is_case_insensitive_and_falls_back_to_the_newest_date`
- `release_sort_by_artist_reverses_with_the_direction`
- `release_sort_by_type_orders_the_raw_field`
- `a_blank_release_field_sorts_last_in_both_directions` — **der geforderte
  MISSING-LAST-Test.** Mindestens eine Zeile mit `""` und eine mit `"   "`; beide
  stehen bei Ascending **und** bei Descending hinten. Deckt Title, Artist und
  Type in einem Test ab.

---

## Aufgabe C-2 — `wire_sorting` liest endlich die Spalte

**Ziel:** Ein Klick auf einen Releases-Header ordnet die Zeilen.

**Bereich:** `releases_view.rs`, `releases_presentation.rs`.

### Vorgehen

1. Reine Zuordnung in `releases_presentation.rs` (display-frei, damit sie ohne
   GTK testbar ist und `releases_view.rs` dünn bleibt — die Datei steht unter
   fremdem Umbau, siehe oben):

   ```rust
   pub(super) fn sort_key_for_id(id: Option<&str>) -> Option<ReleaseSortKey>
   ```

   `date`, `title`, `artist`, `type` → `Some`; `cover`, `status`, `buy`, `None`
   und alles andere → `None`. IDs über `ReleaseColumn::as_str()`, **keine
   String-Literale**.
2. `wire_sorting` bekommt die Form, die Concerts schon hat: **beide** Signale
   (`connect_primary_sort_column_notify` **und**
   `connect_primary_sort_order_notify`) auf einen gemeinsamen
   `apply_sort(&shared, &sorter)`, der `primary_sort_column()` liest, per
   `sort_key_for_id` abbildet, bei `None` früh zurückkehrt und sonst
   `artist_news::sort_release_rows(rows, key, direction)` ins Modell schreibt.
3. `column_view.sort_by_column(Some(&date_column), Descending)` (`:233`) bleibt
   stehen und feuert jetzt beide Handler — die Vorgabesortierung ist danach
   dieselbe wie heute (Datum absteigend). Dieser Aufruf steht **nach**
   `wire_sorting`; die Reihenfolge bleibt so.

### Nachweis (Unit)

```
cargo test -p reprise-gnome releases_presentation > $SCRATCH/c2.log 2>&1
```

- `sort_key_for_id_maps_the_four_text_columns_and_rejects_cover_status_and_buy`

### Nachweis (Display)

```
scripts/check-display-tests.sh > $SCRATCH/c2-display.log 2>&1
```

Neu in `releases_view_tests.rs`:

- `every_sortable_releases_header_orders_its_own_column` — baut die Ansicht,
  legt drei Zeilen mit paarweise verschiedenen Title/Artist/Type/Datum an, holt
  die Spalte per Helfer

  ```rust
  fn column_by_id(view: &gtk4::ColumnView, id: &str) -> gtk4::ColumnViewColumn
  ```

  (über `view.columns()` iterieren, `column.id()` vergleichen), ruft je
  `view.sort_by_column(Some(&column), Ascending)` und liest die Reihenfolge aus
  dem Modell zurück. Für `date`, `title`, `artist`, `type`.
- `the_cover_status_and_link_headers_carry_no_sorter` — für `status` und `buy`
  über `column_by_id`; Cover trägt **keine ID** und wird über seine Position in
  `view.columns()` geholt (führender Pin, Index 0).

---

## Aufgabe C-3 — Status und Link verlieren den Pin und bekommen IDs

**Ziel:** Beide Spalten sind ausblendbar, verschiebbar und Teil des Editors.

**Bereich:** `crates/reprise-view/src/columns/release.rs`,
`crates/reprise-gnome/src/ui/releases/releases_columns.rs`.

### Diese beiden Änderungen müssen im **selben Commit** liegen

Entpinnt man zuerst, verlangt `bind_view_column_keys` (`registry.rs:95-99`)
sofort eine Widget-ID von beiden und **panickt** über `bind_columns_by_id`
(`registry.rs:40-45`, `panic!("invalid … column binding: …")`) beim Öffnen der
Ansicht. Gibt man zuerst die IDs, greift der andere Fehlerpfad
(`registry.rs:74-78`, „pinned column must not expose an editable id"). Beide
Zwischenzustände sind harte Abstürze — das ist kein trennbarer Commit.

### Vorgehen

```rust
fn pin(self) -> Option<Pin> {
    match self {
        Self::Cover => Some(Pin::Leading),
        _ => None,
    }
}

const DEFAULT_VISIBLE: [ReleaseColumn; 6] = [Date, Title, Artist, Type, Status, Buy];
```

Der Doc-Kommentar über `pin()` („Releases has no row context menu, so Status and
Buy are the only access to hiding a release and to its purchase link") ist damit
falsch und wird ersetzt: Cover bleibt gepinnt, weil `header_dnd` den führenden
Pin an der fehlenden ID erkennt; Status und Buy sind ausblendbar, und das
Kopfzeilen-Popover holt sie zurück.

`ALL` bleibt **unverändert** (`Cover, Date, Title, Artist, Type, Status, Buy`) —
`normalize` stellt den führenden Pin voran und hängt das freie Band in
`ALL`-Reihenfolge an, also ist die sichtbare Spaltenfolge dieselbe wie heute.

In `releases_columns.rs`:

- `status_column`: `.id(ReleaseColumn::Status.as_str())` am Builder,
  **kein** `set_sorter`.
- `link_column`: `.id(ReleaseColumn::Buy.as_str())`, **kein** `set_sorter`.
- `cover_column` bleibt **ID-los**. `header_dnd::is_pinned_leading`
  (`header_dnd.rs:190-192`) erkennt den führenden Pin genau daran, und
  `header_dnd.rs:542-544` verhindert darüber, dass Cover Drag-Quelle wird. Eine
  ID an Cover bricht beides und löst zusätzlich den Fehlerpfad
  `registry.rs:74-78` aus.
- `widths::pin(&column, …)` bleibt bei beiden stehen: das ist die
  **Breitenfestlegung**, nicht der Layout-Pin. Nicht verwechseln.

### Nachweis (Unit)

```
cargo test -p reprise-view release > $SCRATCH/c3.log 2>&1
cargo test -p reprise-view layout >> $SCRATCH/c3.log 2>&1
```

- `release_columns_round_trip_and_pin_their_fixed_ones` wird umgeschrieben zu
  `only_the_cover_stays_pinned`: `Cover.pin() == Some(Pin::Leading)`,
  `Status.pin() == None`, `Buy.pin() == None`, `Date.pin() == None`; der
  Round-Trip über `parse`/`as_str` bleibt.
- `nr_33_the_default_release_layout_leads_with_the_cover` bleibt in der
  Reihenfolge unverändert und wird um die Sichtbarkeit ergänzt: `visible`
  enthält Status und Buy.
- `a_layout_stored_before_the_unpinning_keeps_status_and_link_visible`:
  `parse::<ReleaseColumn>("cover,date,title,artist,type,status,buy;date,title,artist,type,status,buy")`
  → beide sichtbar.
- `a_layout_from_before_these_columns_existed_leaves_them_hidden`:
  `parse::<ReleaseColumn>("cover,date,title;date,title")` → Status und Buy stehen
  in `order`, aber **nicht** in `visible`. Siehe RF-C2.

### Nachweis (Display)

```
scripts/check-display-tests.sh > $SCRATCH/c3-display.log 2>&1
```

Neu in `releases_view_tests.rs`:

- `nr_39_the_column_editor_lists_status_and_link_and_hides_them` — der
  Editor-Model (`EditorModel::columns`) liefert beide IDs, `set_visible(false)`
  nimmt sie aus der Sicht, `set_visible(true)` bringt sie zurück, und der
  gespeicherte Layout-String enthält den Zustand. Regel-benannt, weil STYLE-10
  „one rule-named display test per table" verlangt.

  **Die Regel NR-39 selbst schreibt Strang R**, nicht du. Du schreibst nur den
  Test unter diesem Namen. `docs/ux-rules.md` bleibt für dich gesperrt.

### Für den Commit-Text

`restore_stored_widths` überspringt gepinnte Keys. Nach dem Entpinnen würden
gespeicherte Breiten für `status` und `buy` angewendet — es gibt aber keine,
weil die Breitenpersistenz sie bisher genauso übersprungen hat und beide Spalten
`resizable(false)` sind. Kein Handlungsbedarf; der Grund gehört in den
Commit-Text, damit die Frage nicht ein zweites Mal gestellt wird.

---

## Aufgabe C-4 — Releases bekommt den einen Pfeil

**Ziel:** Der geteilte Helfer aus Strang A hängt an der Releases-Tabelle.

**Bereich:** `releases_view.rs` (eine Zeile).

### Vorgehen

```rust
crate::ui::table_columns::single_sort_indicator::mark(&column_view);
```

**Nach** `wire_sorting`, **vor** dem Vorgabe-`sort_by_column` in `:233`, damit
schon der erste Sortierzustand einen einzigen Indikator zeigt.

**Kein Reentranzschloss, kein Zweischritt.** Der Helfer markiert nur den
Indikator der jeweils aktuellen Primärspalte und setzt nie eine Sortierung um.
Der Entwurf sah hier ursprünglich eine Variante mit `sort_by_column(None, …)`
und `Cell<bool>` vor — die ist verworfen.

### Nachweis (Display)

- `two_release_sorts_leave_one_indicator` in `releases_view_tests.rs` — zwei
  `sort_by_column` auf verschiedene Spalten, danach
  `single_sort_indicator::count_primary_indicators(view.upcast_ref()) == 1`.
  Den Zählhelfer aus Strang A benutzen, **nicht** kopieren.

---

## Randfälle

**RF-C1 — Leerwerte zählen als fehlend.** Siehe die Vergleichsregel oben. Leere
und reine Leerzeichen-Felder stehen in **beiden** Richtungen hinten — dieselbe
Semantik, die `compare_release_dates` für fehlende Daten schon hat. Nachweis:
`a_blank_release_field_sorts_last_in_both_directions`.

**RF-C2 — Ein sehr altes gespeichertes Layout.** Der unmittelbar vorhergehende
Stand ist unkritisch: `serialize` hat `status` und `buy` in **beide** Listen
geschrieben, weil `normalize` jeden Pin in `visible` zwingt; nach dem Parsen sind
sie sichtbar, es braucht keine Migration. Kritisch ist nur ein Layout, das die
beiden Spalten **nie erwähnt** hat: `parse_ids` überspringt Unbekanntes,
`normalize` hängt sie an `order`, aber nicht an `visible` — der Nutzer startet
ohne Kaufweg und ohne Verstecken-Aktion. **Bewusst akzeptiert**, weil es genau
die Nebenwirkung ist, die der Nutzer angenommen hat, und weil das
Kopfzeilen-Popover beides zurückholt. Es ist dieselbe Regel, die
`normalize_appends_a_column_the_stored_value_never_mentioned` schon für jede
andere Spalte festhält. Durch
`a_layout_from_before_these_columns_existed_leaves_them_hidden` festgenagelt,
damit es nie stillschweigend passiert.

**RF-C3 — Der Gleichstand-Entscheider dreht nicht mit.** Das Datum entscheidet
Gleichstände bei Releases immer **absteigend**, unabhängig von der
Sortierrichtung (passend zur Vorgabe „neueste zuerst"). Kommentar an der Stelle.

**RF-C4 — Der Registry-Rückfall trifft die neuen Spalten nicht.**
`sort_fallback` (`registry.rs:334`) sucht die Ersatzspalte über
`key.pin().is_none() && visible && sortable`, wobei `sortable` echt an
`column.sorter().is_some()` hängt. `Status` und `Buy` sind nach C-3 zwar frei
und sichtbar, tragen aber keinen Sorter — sie werden korrekt übersprungen. Der
Bestandstest `hiding_primary_sort_chooses_first_visible_sortable_free_column`
(`registry.rs:597`) baut auf `Layout::<ReleaseColumn>::default()` und einem
`|_| true`-Sortierprädikat; er bleibt grün, weil er `Title` versteckt und `Date`
erwartet, und die Reihenfolge sich nicht ändert. **Du fasst ihn nicht an** — die
Datei gehört Strang A. Die Querprüfung nach dem Merge übernimmt Strang R.

**RF-C5 — `Type` sortiert nach dem rohen Feld.** In einer Sprache, deren Labels
anders alphabetisieren als die Rohwerte (`album`, `ep`, `single`), weicht die
sichtbare Reihenfolge von der alphabetischen Erwartung ab. Bewusst in Kauf
genommen: die Alternative wäre, die Sortierung aus dem Kern in die GTK-Schicht
zu ziehen und damit ohne Display untestbar zu machen. Eine Zeile Kommentar an
der Stelle.

**RF-C6 — Ein freier Füller, der nicht füllen kann.** Nach C-3 kann der Nutzer
alle Textspalten ausblenden; `filler_for` wählt dann die erste sichtbare freie
Spalte — im Extremfall `Status` oder `Buy`, beide mit fester Breite und
`resizable(false)`. Die Tabelle expandiert dann eine Aktionsspalte. Kein
Absturz, aber hässlich. **Außerhalb des Umfangs dieser Spec** — hier nur benannt,
damit es beim nächsten Bericht nicht als neuer Fehler gilt. Nichts tun.

**RF-C7 — `bind_view_column_keys` kann nur nach dem Merge vollständig geprüft
werden.** Nach C-3 müssen alle sieben Spalten binden: Cover ohne ID als
führender Pin, die anderen sechs mit ID. Dein Zweig kann das gegen sich selbst
zeigen (`nr_39_…` baut die Ansicht), aber nicht gegen das zusammengeführte
Ergebnis. Das ist Aufgabe von Strang R.

---

## Testdisziplin

**Unit** (kein Display, im Rudel unkritisch):

```
cargo test -p reprise-core   <filter>
cargo test -p reprise-view   <filter>
cargo test -p reprise-gnome  <filter>
```

Fallen, die in diesem Repo schon Zeit gekostet haben und hier gelten:

- `-p reprise-gnome --lib` findet **nichts** — das Paket hat kein `lib`-Target
  unter diesem Namen. Immer ohne `--lib` filtern.
- `--exact` in Kombination mit einem Modulpfad läuft ins Leere. Filter als
  Teilstring angeben.
- Die Ergebniszeile allein ist kein Beleg: `running 0 tests` endet ebenfalls mit
  `test result: ok`. Nach jedem Lauf die Zeile `running N tests` prüfen und
  gegen die erwartete Zahl halten.
- Ausgabe nach `$SCRATCH/<name>.log` umleiten und per `grep`/`wc` auswerten,
  nicht ins Terminal spülen. Auswertungsmuster:
  `grep -E "^test result|running [0-9]+ tests|FAILED|panicked" $SCRATCH/<name>.log`

**Display**:

```
scripts/check-display-tests.sh
scripts/check-display-tests.sh --rule-named        # nur die regelbenannten
```

Das Skript zieht alle `#[ignore]`-Tests aus `reprise-gnome`, startet
`dbus-run-session` + `xvfb-run` je Worker mit isolierten XDG-Roots,
`GSK_RENDERER=cairo`, `GDK_BACKEND=x11`, leerem `WAYLAND_DISPLAY`. Neue
display-gebundene Tests tragen deshalb zwingend:

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
```

und beginnen mit
`let _main_context = crate::ui::test_main_context::lock_main_context();`
gefolgt von `gtk4::init().unwrap();`. Ohne den Lock sind sie im Rudel flaky.

Ein einzeln roter Display-Test in einem Rudel-Lauf ist **kein** Beleg für einen
Fehler. Bei Rot: den einzelnen Test isoliert erneut fahren

```
xvfb-run -a cargo test -p reprise-gnome <name> -- --ignored --nocapture
```

und erst dann urteilen. Ebenso gilt: `dev` hat bekannte rote Display-Tests — Rot
in einer fremden Datei ist nicht die Schuld dieser Arbeit.
