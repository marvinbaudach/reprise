---
slug: table-sorting-and-hideable-link-columns-a
worktree: /home/marvin/Projects/reprise-table-sorting-and-hideable-link-columns-a
branch: feature/table-sorting-and-hideable-link-columns-a
phase: planned
codex_session:
created: 2026-08-15
---

# Strang A — Der geteilte Ein-Pfeil-Helfer

## Zweck

Der bereits ausgelieferte, display-getestete Ein-Pfeil-Mechanismus der
Musikbibliothek wird zu einem geteilten Helfer, den Concerts und Releases später
ebenfalls rufen können. **Reiner Extraktions-Refactor: für die Musikbibliothek
ändert sich nichts.** Dieser Strang ändert keine Ansicht — Concerts und Releases
bekommen ihren Aufruf erst in Strang B bzw. C.

Dieser Strang ist Vorbedingung für B und C. Er muss vor beiden landen.

## Warum keine Messung

Die Spec verlangte ursprünglich, das GTK-Verhalten erst empirisch zu messen, und
schlug einen Stapel-Reset über `sort_by_column(None, …)` mit
`Cell<bool>`-Reentranzschloss vor. **Das ist verworfen.** Der Weg, der hier
extrahiert wird, kommt GTKs Sortierstapel gar nicht in die Quere: er blendet die
Indikatoren aller Nicht-Primärspalten per CSS aus und lässt den Sorter
unangetastet. Damit stellt sich die Frage, die die Sonde beantworten sollte,
nicht mehr.

Der Beleg existiert bereits, und zwar als **Pixelmessung**:
`inactive_sort_columns_render_no_arrow` (`track_list_header_style.rs:193`)
rendert beide Indikatoren in eine Textur, zählt Pixel mit Alpha ≠ 0 und verlangt,
dass genau einer gezeichnet wird. Dazu
`style_10_hiding_the_sorted_column_keeps_a_visible_sort_indicator`
(`table_columns/registry.rs:612`) mit `count_primary_indicators`.
**Niemand holt eine Messung nach.** Was zu messen war, ist gemessen und steht
als Pixeltest im Repo; dieser Strang darf ihn nur nicht zerbrechen.

---

## Dateibesitz

Dir gehören diese Bäume vollständig:

```
crates/reprise-gnome/src/ui/table_columns/**
crates/reprise-gnome/src/ui/track_list/**
crates/reprise-gnome/src/ui/style/**
```

dazu die Modul- und Re-Export-Zeilen in `crates/reprise-gnome/src/ui/mod.rs`,
falls die neue Datei dort sichtbar gemacht werden muss.

Innerhalb dieser Bäume darfst du jede Änderung machen, die die Aufgabe braucht —
auch an Dateien, die dieser Plan nicht namentlich nennt. Neue Dateien, neue
Hilfsfunktionen, angepasste Nachbartests: erlaubt, solange sie in deinen Bäumen
liegen.

## Was dir **nicht** gehört

Fasse außerhalb deiner Bäume **nichts** an. Namentlich verboten, weil dort
parallel gearbeitet wird oder ein anderer Strang zuständig ist:

- `crates/reprise-gnome/src/ui/concerts/**` — Strang B
- `crates/reprise-gnome/src/ui/releases/**` — Strang C
- `crates/reprise-view/**` — Strang B und C
- `crates/reprise-core/**` — Strang B und C
- `docs/ux-rules.md` — Strang R
- alles übrige im Repo

Wenn du glaubst, eine fremde Datei ändern zu müssen, ist das ein Befund für den
Bericht, keine Änderung.

---

## Aufgabe A-1 — `single_sort_indicator` extrahieren

**Ziel:** Genau ein sichtbarer Sortierindikator, aus **einer** Quelle, für jede
Tabelle, die den Helfer ruft. Für die Musikbibliothek bitgleiches Verhalten.

### Ausgangslage im Code

`crates/reprise-gnome/src/ui/track_list/track_list_header_style.rs` enthält
heute vier Dinge, von denen nur die ersten beiden track-list-spezifisch sind:

1. die Header-Textfarbe (`> header label { color: @reprise_secondary_fg_color; }`)
   und die Hairline-Regeln — **bleiben, wo sie sind**;
2. `TRACK_LIST_CLASS = "reprise-track-list"` — bleibt;
3. `PRIMARY_SORT_INDICATOR_CLASS`, die Regel
   `sort-indicator:not(.…) { opacity: 0; }`, `find_sort_indicator`,
   `sync_primary_sort_indicator` und die drei Signalverbindungen in `mark()` —
   **ziehen um**.

Produktive Aufrufer heute:

- `track_list_builder.rs:49` → `track_list_header_style::mark(&column_view)`
- `track_list_sort.rs:56` → `track_list_header_style::sync_primary_sort_indicator(view)`
  (direkter Aufruf im selben Zug, in dem sortiert wird — dadurch verschwindet der
  alte Pfeil **sofort** statt einen Frame später)

Testaufrufer:

- `table_columns/registry.rs:646` → `track_list_header_style::mark(&view)`
- `table_columns/registry.rs:622-630` → lokaler `count_primary_indicators`, der
  das Klassenliteral `"reprise-primary-sort-indicator"` benutzt

Zusätzlich liest `style/theme.rs:530` (`contrast_3_secondary_surfaces_use_verified_level`)
`track_list_header_style::css()` und sucht darin den Selektor `> header label`.

### Vorgehen

1. **Neue Datei** `crates/reprise-gnome/src/ui/table_columns/single_sort_indicator.rs`:

   ```rust
   //! Exactly one visible sort indicator, for any GtkColumnView.
   //!
   //! GTK's ColumnViewSorter keeps a multi-column sort stack and renders a
   //! directional arrow for every column on it, while every table in this app
   //! reads only `primary_sort_column`. The secondary arrows therefore claim an
   //! order nobody establishes. GTK also updates its own ascending/descending
   //! classes one frame after a new primary column is selected, briefly leaving
   //! both the old and the new arrow visible.
   //!
   //! An app-owned class on exactly the primary column's indicator, plus one CSS
   //! rule that hides every indicator without it, solves both: the inactive
   //! arrows never paint, their width stays reserved, and headers do not shift.
   //! GTK's sorter is left untouched.

   const SINGLE_SORT_CLASS: &str = "reprise-single-sort";
   pub(in crate::ui) const PRIMARY_SORT_INDICATOR_CLASS: &str =
       "reprise-primary-sort-indicator";

   pub(in crate::ui) fn css() -> String { /* genau eine Regel, s. u. */ }
   pub(in crate::ui) fn mark(view: &gtk4::ColumnView) { /* aus mark() übernommen */ }
   pub(in crate::ui) fn sync_primary_sort_indicator(view: &gtk4::ColumnView) { /* unverändert */ }
   fn find_sort_indicator(widget: &gtk4::Widget) -> Option<gtk4::Widget> { /* unverändert */ }
   ```

   `css()` liefert **genau eine** Regel — die restliche Track-List-CSS
   (Header-Textfarbe, Zellen-Hairlines) bleibt in `track_list_header_style` und
   wird ausdrücklich **nicht** auf andere Tabellen übertragen:

   ```css
   .reprise-single-sort sort-indicator:not(.reprise-primary-sort-indicator) { opacity: 0; }
   ```

   `mark()` setzt `SINGLE_SORT_CLASS` und verbindet dieselben drei Signale, die
   `track_list_header_style::mark` heute verbindet: `connect_map`,
   `view.columns().connect_items_changed`, und —
   sofern `view.sorter().and_downcast::<gtk4::ColumnViewSorter>()` gelingt —
   `connect_primary_sort_column_notify`. Die `WeakRef`-Aufhängung
   (`view.downgrade()` je Closure) wird **wörtlich übernommen**; sie ist der
   Grund, warum die Signale den View nicht am Leben halten.

   `sync_primary_sort_indicator` und `find_sort_indicator` wandern unverändert
   mit, inklusive des Doc-Kommentars über die gleiche Reihenfolge von
   Header-Kindern und `view.columns()` (Sichtbarkeit eingeschlossen).

   Dazu ein **Zählhelfer für die Tests der anderen Stränge**, damit B und C ihn
   nicht kopieren müssen:

   ```rust
   #[cfg(test)]
   pub(in crate::ui) fn count_primary_indicators(widget: &gtk4::Widget) -> usize
   ```

   Zählt rekursiv Widgets mit `css_name() == "sort-indicator"` **und**
   `has_css_class(PRIMARY_SORT_INDICATOR_CLASS)`; Vorlage ist der lokale Helfer
   in `registry.rs:622-630`, nur mit der Konstante statt dem Literal.

2. **`table_columns/mod.rs`**: `pub(in crate::ui) mod single_sort_indicator;` in
   die bestehende Modulliste (alphabetisch zwischen `registry` und
   `width_persistence`).

3. **`style/mod.rs`**: `super::table_columns::single_sort_indicator::css()` in
   die Liste in `app_css()` (`style/mod.rs:101`) aufnehmen. Ohne diesen Schritt
   existiert die Klasse, aber keine Regel — und der Fehler wäre unsichtbar, weil
   die Indikatoren dann einfach alle sichtbar bleiben.

4. **`track_list_header_style.rs`** gibt ab:
   - `css()` verliert die `sort-indicator`-Regel und den Verweis auf
     `PRIMARY_SORT_INDICATOR_CLASS`. Der Doc-Kommentar über die verzögerten
     `ascending`/`descending`-Klassen wandert mit an den neuen Ort; hier bleibt
     ein Einzeiler, der auf `single_sort_indicator` verweist.
   - `PRIMARY_SORT_INDICATOR_CLASS`, `find_sort_indicator` und
     `sync_primary_sort_indicator` verschwinden hier ganz.
   - `mark()` setzt weiterhin selbst `TRACK_LIST_CLASS` und ruft danach
     `crate::ui::table_columns::single_sort_indicator::mark(view)`. Es verbindet
     die drei Signale **nicht mehr selbst** — sonst liefe die Synchronisierung
     zweimal je Ereignis.

5. **`track_list_sort.rs:56`** ruft `sync_primary_sort_indicator` künftig unter
   dem neuen Pfad. Der Aufruf selbst bleibt, an derselben Stelle: er ist der
   Grund, warum der Pfeilwechsel in der Musikbibliothek ohne Frame-Verzögerung
   sichtbar ist.

6. **`registry.rs`**: der lokale `count_primary_indicators` im Test weicht dem
   geteilten Helfer; `registry.rs:646` ruft `single_sort_indicator::mark(&view)`
   statt den Umweg über `track_list_header_style::mark`. Der Test misst damit
   direkt den geteilten Mechanismus.

### Nachweis (Unit)

```
cargo test -p reprise-gnome single_sort_indicator > $SCRATCH/a1-unit.log 2>&1
cargo test -p reprise-gnome track_list_header_style >> $SCRATCH/a1-unit.log 2>&1
cargo test -p reprise-gnome style >> $SCRATCH/a1-unit.log 2>&1
```

Neu, in `single_sort_indicator.rs`:

- `single_sort_css_hides_every_non_primary_indicator` — prüft den erzeugten
  CSS-String (Muster: `header_style_is_subtle_and_scoped_away_from_song_cells`):
  enthält `.reprise-single-sort sort-indicator:not(.reprise-primary-sort-indicator)`,
  enthält `opacity: 0`, enthält **nicht** `sort-indicator.unsorted`, enthält
  **nicht** `reprise-track-list` (Beleg, dass die Regel neutral ist).

Angepasst, in `track_list_header_style.rs`:

- `header_style_is_subtle_and_scoped_away_from_song_cells` — die beiden
  Indikator-Assertions wandern in den neuen Test; hier kommt
  `assert!(!css.contains("sort-indicator"))` dazu, das den Umzug festnagelt. Die
  Assertions über `> header label`, `@reprise_secondary_fg_color` und
  `!contains("reprise-track-cell")` bleiben **wörtlich stehen**.

Unverändert grün, ohne Anfassen:

- `contrast_3_secondary_surfaces_use_verified_level` (`style/theme.rs:515`) —
  liest `track_list_header_style::css()` und sucht `> header label`. Wird rot,
  wenn die Extraktion zu viel mitnimmt.
- der CSS-Parsetest in `style/mod.rs:66` (`css_parse_errors(&app_css())`) —
  fängt einen Syntaxfehler in der neuen Regel.

### Nachweis (Display)

```
scripts/check-display-tests.sh > $SCRATCH/a1-display.log 2>&1
```

Neu, in `single_sort_indicator.rs`:

- `marking_a_column_view_scopes_the_single_sort_rule_to_it` — `mark(&view)`;
  `view.has_css_class("reprise-single-sort")` ist wahr, ein unbeteiligter
  `ColumnView` trägt sie nicht. (Vorlage:
  `marking_targets_only_the_track_table_root`.)
- `the_shared_helper_leaves_one_indicator_after_two_sorts` — bare
  `gtk4::ColumnView`, zwei Spalten mit je einem
  `CustomSorter::new(|_, _| gtk4::Ordering::Equal)`, `SortListModel` über
  `view.sorter()`, `NoSelection`, `gtk4::Window` mit `present()`, dann
  `while gtk4::glib::MainContext::default().iteration(false) {}`; `mark(&view)`;
  zwei `view.sort_by_column(Some(&…), Ascending)` auf **verschiedene** Spalten;
  danach `count_primary_indicators(view.upcast_ref()) == 1`. Vorlage für den
  Aufbau: `registry.rs:610-684`.

Unverändert grün, ohne Anfassen — das ist der eigentliche Nachweis, dass der
Refactor nichts kostet:

- `inactive_sort_columns_render_no_arrow` (`track_list_header_style.rs:193`) —
  **die Pixelmessung.** Sie ist der harte Beleg; bleibt sie grün, ist der Umzug
  wirkungsgleich.
- `marking_targets_only_the_track_table_root` — prüft `TRACK_LIST_CLASS`. **Nicht**
  auf die neue Klasse umschreiben; die Track-List-Wurzel behält beide.
- `mapped_column_title_uses_the_subtle_foreground_alpha`
- `style_10_hiding_the_sorted_column_keeps_a_visible_sort_indicator` (`registry.rs:612`)
- `column_headers_update_sort_state_and_reload_once`,
  `sorting_a_new_column_replaces_the_previous_sort_key`

---

## Randfälle

**RF-A1 — Die Track-List-Wurzel trägt jetzt zwei Klassen.**
`track_list_header_style::mark` setzt `reprise-track-list` (für Textfarbe und
Hairlines) **und** delegiert an `single_sort_indicator::mark`, das
`reprise-single-sort` setzt. Beides ist nötig; keine der beiden Klassen darf die
andere ersetzen. `marking_targets_only_the_track_table_root` prüft weiterhin die
erste.

**RF-A2 — `sync_primary_sort_indicator` muss von außen erreichbar bleiben.**
`track_list_sort.rs:56` ruft sie direkt, damit der Pfeilwechsel im selben Aufruf
sichtbar wird statt einen Frame später. Sie zu privatisieren bricht nicht die
Signale, sondern das Timing — und das fängt nur die Pixelmessung. Sichtbarkeit
`pub(in crate::ui)`.

**RF-A3 — Doppelte Signalverbindung.** Verbindet `track_list_header_style::mark`
die drei Signale weiterhin selbst **und** delegiert, läuft die Synchronisierung
zweimal je Ereignis. Kein sichtbarer Fehler, aber vermeidbare Arbeit im
Header-Pfad und eine Falle für den nächsten Leser. Genau eine Stelle verbindet.

**RF-A4 — Die Header-Label-Regel bleibt, wo sie ist.** Zieht die Extraktion
`> header label { color: @reprise_secondary_fg_color; }` versehentlich mit, wird
`contrast_3_secondary_surfaces_use_verified_level` rot — und Concerts/Releases
bekämen ungefragt die Track-List-Kopfzeilenoptik. Nur die
`sort-indicator`-Regel zieht um.

**RF-A5 — Nach A allein ändert sich in keiner Ansicht etwas.** Concerts und
Releases rufen den Helfer erst in B-5 bzw. C-4. Ein Lauf nach A zeigt in beiden
Tabellen weiterhin mehrere Pfeile. Das ist **beabsichtigt** und kein Zeichen
eines unfertigen Strangs.

**RF-A6 — `view.sorter()` kann fehlschlagen.** `mark()` verbindet
`connect_primary_sort_column_notify` nur, wenn der Downcast auf
`gtk4::ColumnViewSorter` gelingt; die anderen beiden Signale werden immer
verbunden. Dieses `if let` wörtlich übernehmen — es ist der Grund, warum `mark`
auf einem `ColumnView` ohne Modell nicht panickt, und genau das tut der Test
`marking_a_column_view_scopes_the_single_sort_rule_to_it`.

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
