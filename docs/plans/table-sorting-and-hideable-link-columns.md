---
slug: table-sorting-and-hideable-link-columns
worktree:
branch:
phase: shipped
codex_session:
created: 2026-08-15
strands: a,b,c,r
merge_order: a,b,c,r
spec: docs/superpowers/specs/2026-08-14-table-sorting-and-hideable-link-columns-design.md
---

# Sortierung, die sortiert — und ausblendbare Link-Spalten (Mutterplan)

Dieser Plan beansprucht **keinen** Zweig: `worktree:` und `branch:` bleiben leer.
Gebaut wird in vier Strangdateien, jede mit eigenem Worktree:

| Strang | Datei | Inhalt |
|---|---|---|
| A | `table-sorting-and-hideable-link-columns-a.md` | Vorlauf: geteilter Ein-Pfeil-Helfer |
| B | `table-sorting-and-hideable-link-columns-b.md` | Concerts (B-1…B-5) |
| C | `table-sorting-and-hideable-link-columns-c.md` | Releases (C-1…C-4) |
| R | `table-sorting-and-hideable-link-columns-r.md` | Abschluss: UX-Regeln, Gesamtlauf |

Codex bekommt immer nur **eine** Strangdatei. Jede ist deshalb allein lesbar und
wiederholt die Testdisziplin vollständig. Dieser Mutterplan ist für den
Menschen, der die Stränge startet und zusammenführt.

---

## Ausgangslage in Kurzform

Die Spec beschreibt das WAS im Detail. Hier nur, was zum Schneiden nötig ist:

- **Concerts sortiert falsch.** `artist_column` (`concerts_columns.rs:78-137`)
  setzt als einzige Spalte mit ID keinen Sorter — der Header ist tot.
  `apply_sort` (`concerts_view.rs:756-772`) kennt nur `"distance"` und fällt für
  alles andere per Wildcard auf Datum; `city` und `venue` sortieren also nach
  Datum, während der Pfeil auf ihre Spalte zieht. `ConcertSortKey`
  (`concerts_presentation.rs:11-14`) hat nur `Date` und `Distance`.
- **Releases sortiert gar nicht.** `wire_sorting` (`releases_view.rs:663-683`)
  verbindet nur `primary_sort_order_notify` und liest `primary_sort_column()`
  nie. `artist_news_view::sort_rows` nimmt ausschließlich eine Richtung und
  ordnet immer nach `first_release_date`.
- **Mehrere Pfeile.** `GtkColumnViewSorter` führt einen Sortierstapel; ein Klick
  legt die alte Spalte auf Rang 2 statt sie zu ersetzen, GTK zeichnet die
  Nachrangpfeile mit. Beide Ansichten lesen nur `primary_sort_column`.
- **Releases' Link- und Status-Spalte sind gepinnt.** `ReleaseColumn::pin()`
  (`release.rs:61-67`) pinnt `Cover` führend sowie `Status` und `Buy`
  abschließend; `layout::normalize` erzwingt für jeden Pin Sichtbarkeit und
  `EditorModel::columns` blendet Pins aus dem Editor aus.
- **Concerts' Vorgabelayout** zeigt heute sechs Spalten in der Reihenfolge
  `Date, Artist, City, Venue, Distance, Tickets` (Source aus).

---

## Die fünf Beschlüsse

Diese fünf Punkte stammen aus dem Grilling des Entwurfs. Sie **überschreiben**
Entwurf und Spec, wo sie sich widersprechen.

### 1 — Ein Pfeil ohne Messung

Die Spec nennt Teil 2 „der einzige, dessen GTK-Verhalten ich nicht gemessen
habe" und verlangt eine empirische Sonde vor der Umsetzung. **Das ist erledigt,
bevor es begonnen hat.** Im Repo liegt bereits ein ausgelieferter,
display-getesteter Mechanismus für genau dieses Problem, nur auf die
Musikbibliothek verdrahtet:

`crates/reprise-gnome/src/ui/track_list/track_list_header_style.rs` —
`PRIMARY_SORT_INDICATOR_CLASS`, `mark()`, `sync_primary_sort_indicator()`, plus
die CSS-Regel `sort-indicator:not(.reprise-primary-sort-indicator) { opacity: 0 }`.
Sein Doc-Kommentar beschreibt wörtlich das Symptom der Spec („briefly leaving
both the old and new arrows visible").

Der Beleg existiert ebenfalls schon, und zwar als **Pixelmessung**:
`inactive_sort_columns_render_no_arrow` (`track_list_header_style.rs:193`)
rendert beide Indikatoren in eine Textur, zählt Pixel mit Alpha ≠ 0 und
verlangt, dass genau einer gezeichnet wird. Dazu kommt
`style_10_hiding_the_sorted_column_keeps_a_visible_sort_indicator`
(`table_columns/registry.rs:612`) mit seinem Zählhelfer `count_primary_indicators`.

**Folge:** Variante S (Stapel-Reset über `sort_by_column(None, …)`) und
**Aufgabe 0 des Entwurfs entfallen ersatzlos.** Es gibt kein Reentranzschloss,
keinen Zweischritt, keine dreifach laufende Sortierung, keine Sonde. Strang A
ist ein reiner Extraktions-Refactor: der Mechanismus zieht nach
`ui/table_columns/single_sort_indicator.rs` und wird von Concerts **und**
Releases gerufen. Für die Musikbibliothek ändert sich das Verhalten nicht.

**Niemand holt die Messung nach.** Die Sonde aus dem Entwurf hätte GTKs
Rohverhalten beim Stapel-Reset gemessen — eine Frage, die nur die verworfene
Variante S gestellt hat. Der gewählte Weg kommt GTKs Stapel gar nicht in die
Quere: er blendet die Indikatoren aller Nicht-Primärspalten per CSS aus und
lässt den Sorter unangetastet. Was zu messen war, ist gemessen, und der
Messstand steht als Pixeltest im Repo.

### 2 — Der Schnitt bleibt `A → {B, C} → R`

Vier Landungen, drei Worktrees (R braucht keinen eigenen Code-Zweig, nur einen
für `docs/ux-rules.md`). A ist nach Beschluss 1 nur noch **eine** Aufgabe.

### 3 — `Source` wird sortierbar

Sortierbar in Concerts: **Date, Artist, City, Venue, Distance, Source.**
`Tickets` bekommt weiterhin **keinen** Sorter — die Zelle ist ein Knopf, dessen
Beschriftung fehlen kann, es gibt dort nichts zu ordnen.

`ConcertSortKey` bekommt damit gegenüber der Spec **vier** neue Varianten:
`Artist`, `City`, `Venue`, `Source`. `source_column`
(`concerts_status_cells.rs:138`) rendert `source_name(&row)` als gewöhnliches
Label und trägt bereits `.id(ConcertColumn::Source.as_str())`; es fehlt nur der
Sorter. Sortiert wird nach demselben `source_name(&row)`, das die Zelle zeigt —
also `ticket_source` mit Rückfall auf `provider`, nicht das rohe Feld.

### 4 — Leerwerte zählen als fehlend (MISSING-LAST)

Für **jede** neue Textsortierung in **beiden** Tabellen gilt eine gemeinsame
Vergleichsregel. Ein Feld, das leer ist oder nur aus Leerzeichen besteht, wird
wie `None` behandelt und landet **richtungsunabhängig am Ende** — genau wie
`compare_optional` (`concerts_presentation.rs:64-81`) es heute für Datum und
Distanz tut.

```rust
/// Leerer oder reiner Leerzeichen-Wert = fehlender Wert.
fn present(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn compare_text(left: &str, right: &str, direction: SortDirection) -> Ordering {
    match (present(left), present(right)) {
        (Some(left), Some(right)) => {
            let ordering = left
                .to_lowercase()
                .cmp(&right.to_lowercase())
                .then_with(|| left.cmp(right));
            match direction {
                SortDirection::Ascending => ordering,
                SortDirection::Descending => ordering.reverse(),
            }
        }
        // Richtungsunabhängig: fehlende Werte stehen in beiden Richtungen
        // hinten, wie compare_optional es für Datum und Distanz tut.
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
```

Gilt für Concerts (Artist, City, Venue, Source) und Releases (Title, Artist,
Type). Jede Tabelle bekommt **einen** Test, der beide Richtungen prüft.

**Warum dupliziert und nicht geteilt:** Concerts' Sortierung lebt in
`reprise-gnome`, Releases' in `reprise-core`. Ein geteilter Helfer müsste in
`reprise-core` liegen und wäre damit eine Datei, die B und C gemeinsam
anfassen — genau die Kollision, die der Schnitt vermeidet. Die Funktion ist
zwölf Zeilen; der Preis der Duplikation ist kleiner als der eines vierten
Strangs. Der **Wortlaut** oben ist normativ: beide Stränge schreiben dieselbe
Semantik, auch wenn sie zwei Funktionen schreiben.

`compare_optional` bleibt **unverändert**. Der Entwurf behauptete an dieser
Stelle noch das Gegenteil („ein leerer String ist ein gültiger Wert und sortiert
bei Ascending vorn") — das ist durch diesen Beschluss überholt. Randfall RF-1
des Entwurfs ist damit hinfällig.

### 5 — Ausdrücklich nicht angefasst

- **Der bevorzugte Füller bleibt `ConcertColumn::Venue`**
  (`concerts_column_layout.rs:26`, `width_persistence::wire`). Grund:
  `concerts_location_columns.rs` schaltet `venue.set_expand()` bewusst um, wenn
  die Distanz-Spalte mangels Standort verschwindet. Ihn auf Artist zu ziehen
  änderte das Verhalten für jeden, der Venue eingeblendet lässt — und das steht
  in keiner Spec. Der Rückfallpfad `filler_for` macht Artist ohnehin zum Füller,
  sobald Venue per neuem Default aus ist. Das wird **per Test festgenagelt**
  (B-4), nicht per Konfiguration erzwungen.
- **Die `[replaced by <ID>]`-Konvention** für STYLE-10 (etabliert,
  `docs/ux-rules.md:20`). Kein Löschen, kein Umschreiben am Ort.
- **Die Migration löscht nur `ui.column_layout.concerts`**, nicht
  `ui.column_widths.concerts`. Breiten sind von Reihenfolge und Sichtbarkeit
  unabhängig.

---

## Der Schnitt

### Dateibesitz

Besitz ist als **Glob-Baum** formuliert, nicht als Dateiliste. Innerhalb seiner
Bäume darf ein Strang jede Änderung machen, die seine Aufgabe braucht — auch an
Dateien, die dieser Plan nicht namentlich nennt. Die Grenze ist ein **Verbot
fremder Bäume**, keine Erlaubnisliste.

| Strang | Bäume |
|---|---|
| **A** | `crates/reprise-gnome/src/ui/table_columns/**`<br>`crates/reprise-gnome/src/ui/track_list/**`<br>`crates/reprise-gnome/src/ui/style/**`<br>dazu die Modul-/Re-Export-Zeilen in `crates/reprise-gnome/src/ui/mod.rs` |
| **B** | `crates/reprise-gnome/src/ui/concerts/**`<br>`crates/reprise-view/src/columns/concert.rs`<br>`crates/reprise-core/src/db_concerts*.rs`<br>`crates/reprise-core/src/db.rs` |
| **C** | `crates/reprise-gnome/src/ui/releases/**`<br>`crates/reprise-view/src/columns/release.rs`<br>`crates/reprise-core/src/artist_news*.rs` |
| **R** | `docs/ux-rules.md`<br>`docs/plans/table-sorting-and-hideable-link-columns*.md` |

Überschneidungsfrei geprüft:

- `crates/reprise-core/src/lib.rs` wird von **keinem** Strang angefasst. B legt
  keine neue Datei an (`migrate_v75` lebt im bereits deklarierten
  `db_concerts`), C ändert nur bestehende Module.
- `crates/reprise-gnome/src/ui/mod.rs` gehört allein A. B und C legen keine
  neuen Module an, die dort deklariert werden müssten — beide arbeiten in
  bestehenden Dateien ihrer Unterbäume, deren `mod.rs` ihnen gehört.
- `crates/reprise-view/src/columns/key.rs` und `layout.rs` bleiben unberührt:
  `Pin` behält seine zwei Zustände und seine Bedeutung.
- `po/POTFILES.in` bleibt unverändert. Für die neuen Releases-Spaltentitel sind
  keine neuen Zeichenketten nötig — `RELEASES_STATUS` und `RELEASES_LINK`
  existieren und werden von `releases_column_layout.rs::label()` bereits
  verwendet.

### Parallelität

```
A  →  { B , C }  →  R
```

**Nur B und C laufen gleichzeitig.** A ist Vorstufe, R ist Nachstufe. Es sind
also **höchstens zwei Codex-Läufe gleichzeitig** auf der Maschine — bewusst so
geschnitten, damit parallele Läufe die Maschine nicht überfahren.

- **A vor B und C**, weil beide seine Funktion aufrufen (B-5, C-4). Startet man
  B oder C vorher, gibt es den Aufruf nicht, den ihre letzte Aufgabe schreiben
  soll.
- **B und C sind untereinander disjunkt**: kein gemeinsamer Pfad, keine
  gemeinsame Datei. Sie dürfen ohne Absprache parallel laufen.
- **R nach beiden**, weil `docs/ux-rules.md` der klassische Ort ist, an dem zwei
  Stränge denselben Abschnitt schreiben und der Merge Regeltext verliert. Die
  gesamte Regelarbeit liegt deshalb in **einer** späten Aufgabe.

### Wenn nicht parallel gefahren wird

Lineare Reihenfolge: **A-1, B-1, B-2, B-3, B-4, B-5, C-1, C-2, C-3, C-4, R-1,
R-2** — und alle Post-Merge-Querprüfungen wandern in R-2.

---

## Post-Merge-Querprüfungen

Jede dieser Prüfungen liest oder ändert eine Datei, die der jeweilige Strang
**nicht** besitzt. Sie gehören ausdrücklich **nicht** in die Aufgaben von B oder
C: kein Strang kann über seine Besitzgrenze hinweg verifizieren.

1. **`bind_view_column_keys` panickt nicht** (`table_columns/registry.rs`,
   Besitz A). Nach C-3 müssen alle sieben Releases-Spalten binden: Cover ohne ID
   als führender Pin, die anderen sechs mit ID. Prüfung: irgendein
   Releases-Display-Test, der die Ansicht **baut** — z. B. `nr_39_…` aus C-3 —
   nach dem Merge erneut fahren. Vor dem Merge kann C das nur gegen seinen
   eigenen Zweig zeigen, nicht gegen das zusammengeführte Ergebnis.
2. **`sort_fallback` bleibt korrekt** (`registry.rs`, Besitz A). Die
   Bestandstests `hiding_primary_sort_chooses_first_visible_sortable_free_column`
   und `style_10_hiding_the_sorted_column_keeps_a_visible_sort_indicator` müssen
   grün sein, mit dem neuen `ReleaseColumn`-Pinning **und** dem extrahierten
   Ein-Pfeil-Helfer gleichzeitig geladen.
3. **Die Musikbibliothek hat keinen Kollateralschaden** (`track_list/**`,
   Besitz A). Strang A gibt `track_list_header_style` seine Konstanten und
   Funktionen ab; nach dem Merge müssen weiterhin grün sein:
   `inactive_sort_columns_render_no_arrow` (die Pixelmessung),
   `marking_targets_only_the_track_table_root`,
   `mapped_column_title_uses_the_subtle_foreground_alpha`,
   `header_style_is_subtle_and_scoped_away_from_song_cells`,
   `column_headers_update_sort_state_and_reload_once`,
   `sorting_a_new_column_replaces_the_previous_sort_key`,
   `contrast_3_secondary_surfaces_use_verified_level` (`style/theme.rs:515`,
   liest `track_list_header_style::css()` und dessen `> header label`-Selektor —
   der bleibt dort, aber der Test bricht, wenn die Extraktion zu viel mitnimmt).
4. **Ein Pfeil in beiden Tabellen zugleich** —
   `two_concert_sorts_leave_one_indicator` (B-5) und
   `two_release_sorts_leave_one_indicator` (C-4) in **einem** Lauf. Jeder Strang
   kann nur seinen eigenen zeigen.
5. **Die UX-Regeln** (`docs/ux-rules.md`, Besitz R). Aufgabe R-1 vollständig,
   danach `scripts/check-display-tests.sh --rule-named`. Jede in R-1 genannte
   Regel-ID muss einen gleichnamigen Test finden.
6. **Voller Lauf** — Aufgabe R-2, alle drei Pakete plus die gesamte
   Display-Suite. Die Teilläufe der Stränge sind **kein** Ersatz: eine grüne
   Bilanzzeile aus einem Lauf, in dem eine Suite gar nicht startete, ist in
   diesem Repo schon einmal als Beleg durchgegangen.

---

## Kollision mit laufender Arbeit

Zwei Stränge von `updates-concerts-releases-rework` sind noch nicht in `dev`.

**Die Datei, die den Konflikt trägt:**
`crates/reprise-gnome/src/ui/releases/releases_view.rs`. Sie steht im
Alleinbesitz von Strang 2 jener Arbeit
(`docs/plans/updates-concerts-releases-rework-2.md`, Abschnitt „Dateibesitz").
Dieser Plan ändert dort `wire_sorting` (C-2) und fügt eine Zeile für den
Ein-Pfeil-Helfer ein (C-4). Der Nutzer hat entschieden, trotzdem jetzt zu bauen.

**Wie der Konflikt aufzulösen ist:**

- Die Bereiche sind **semantisch disjunkt**. Strang 2 baut `build_footer()`
  (`:421`) und `apply_footer()` (`:476`) auf das gemeinsame `feed_footer.rs` um
  und lässt die Datei dabei schrumpfen. Dieser Plan fasst nur `wire_sorting`
  (`:663-683`, am Dateiende) und die Zeile `:233` an.
- Der Konflikt ist deshalb **positionell, nicht inhaltlich**: git meldet ihn,
  weil sich Zeilennummern verschieben, nicht weil zwei Änderungen dieselbe
  Aussage treffen. Auflösung: **beide Seiten übernehmen.** `wire_sorting` ist
  eine geschlossene Funktion am Dateiende — sie wird als Ganzes aus diesem
  Zweig übernommen, der Rest der Datei aus Strang 2.
- Prüfung **nach** der Auflösung, nicht vorher: `wire_sorting` verbindet beide
  `notify`-Signale, `sort_key_for_id` wird gerufen, die Zeile mit
  `sort_by_column(Some(&date_column), Descending)` steht weiterhin **nach**
  `wire_sorting`, und der `single_sort_indicator::mark`-Aufruf steht **vor**
  ihr. Danach `cargo test -p reprise-gnome releases` und die
  Releases-Display-Tests.
- Wer zuerst landet, gewinnt die Basis; die zweite Seite rebased. Da Strang 2
  die Datei umbaut, ist es billiger, wenn **dieser** Plan zuerst landet. Ist das
  nicht möglich, trägt die Auflösung dieser Plan.

**Frei und unstrittig:** `crates/reprise-gnome/src/ui/concerts/**` und
`crates/reprise-core/**` (Strang 1 jener Arbeit ist bereits in `dev`),
`crates/reprise-gnome/src/ui/releases/releases_columns.rs` (hat sich Strang 2
ausdrücklich gesperrt), `crates/reprise-view/**`,
`crates/reprise-gnome/src/ui/table_columns/**`,
`crates/reprise-gnome/src/ui/track_list/**`.

**Achtung bei `docs/ux-rules.md`:** Strang 2 jener Arbeit besitzt dort Abschnitt
R (NR-34…NR-38, NR-21a und Statusmarker auf NR-5b/10a/21/22/23) plus **eine**
Zeile in Abschnitt AE (Statusmarker auf CONC-7). Dieser Plan schreibt in
Abschnitt R (NR-30, NR-33, neu **NR-39**), Abschnitt AE (neu **CONC-17**) und
Abschnitt S (STYLE-10 → **STYLE-13**). Die IDs überschneiden sich nicht —
NR-39 ist genau deshalb gewählt —, aber der Textkonflikt in denselben
Abschnitten ist sicher. Deshalb liegt die gesamte Regelarbeit in R-1.

**Achtung bei `strings_releases.rs`:** ebenfalls im Alleinbesitz von Strang 2.
Dieser Plan braucht dort **nichts** und darf dort **nichts** anfassen.

---

## Randfälle, die mehr als einen Strang betreffen

Strangeigene Randfälle stehen in den Strangdateien. Hier nur die, die über eine
Besitzgrenze hinweg wirken.

**RF-A — Leerwerte und Gleichstand (B und C).** Siehe Beschluss 4. Beide
Tabellen behandeln leere und reine Leerzeichen-Felder wie fehlende Werte und
legen sie richtungsunabhängig ans Ende. Gleiche Textfelder fallen auf das Datum
durch — Concerts aufsteigend, Releases absteigend (siehe RF-B). Je Tabelle ein
Test, der **beide** Richtungen prüft.

**RF-B — Der Gleichstand-Entscheider dreht nicht mit (B und C).** Der
Datums-Entscheider hinter einer Textsortierung läuft immer in derselben
Richtung, unabhängig von der Sortierrichtung: bei Concerts aufsteigend, bei
Releases absteigend (passend zur Vorgabesortierung „neueste zuerst"). Grund: er
stellt Stabilität her, er drückt keine Ordnung aus. Würde er mitdrehen, sprängen
gleichnamige Zeilen beim Richtungswechsel **doppelt**. Das gehört als Kommentar
an beide Stellen.

**RF-C — Der Registry-Rückfall trifft die neuen Spalten nicht (C und A).**
`sort_fallback` (`registry.rs:334`) sucht die Ersatzspalte über
`key.pin().is_none() && visible && sortable`, wobei `sortable` echt an
`column.sorter().is_some()` hängt. `Status` und `Buy` sind nach C-3 zwar frei
und sichtbar, tragen aber keinen Sorter — sie werden korrekt übersprungen.
Der Bestandstest `hiding_primary_sort_chooses_first_visible_sortable_free_column`
(`registry.rs:597`) baut auf `Layout::<ReleaseColumn>::default()` und einem
`|_| true`-Sortierprädikat; er bleibt grün, weil er `Title` versteckt und `Date`
erwartet, und die Reihenfolge sich nicht ändert. **C fasst ihn nicht an** — die
Datei gehört A. Nachweis: Post-Merge-Querprüfung 2.

**RF-D — Distance ruft `sort_by_column` von außen (A und B).**
`LocationColumns::apply` (`concerts_location_columns.rs:106`, `:126`) und
`sort_by_date()` (`:133`) rufen selbst `view.sort_by_column(…)`, wenn die
Standortverfügbarkeit kippt. Mit dem gewählten Mechanismus ist das **folgenlos**:
der Helfer markiert nur den Indikator der jeweils aktuellen Primärspalte und
setzt nie eine Sortierung um. Es gibt kein Reentranzschloss, das ein fremder
Aufruf falsch treffen könnte — das ist der Hauptgewinn aus Beschluss 1.
Nachweis: `losing_the_location_still_falls_back_to_the_date_sort` (B-5).

**RF-E — Ein freier Füller, der nicht füllen kann (C).** Nach C-3 kann der
Nutzer alle Textspalten ausblenden; `filler_for` wählt dann die erste sichtbare
freie Spalte — im Extremfall `Status` oder `Buy`, beide mit fester Breite und
`resizable(false)`. Die Tabelle expandiert dann eine Aktionsspalte. Kein
Absturz, aber hässlich. **Außerhalb des Umfangs dieser Spec**; hier nur benannt,
damit es beim nächsten Bericht nicht als neuer Fehler gilt.

---

## Testdisziplin (Kurzfassung; vollständig in jeder Strangdatei)

**Unit** (kein Display): `cargo test -p <crate> <filter>`, Ausgabe nach
`$SCRATCH/<name>.log`, Auswertung per `grep`. Fallen: `-p reprise-gnome --lib`
findet nichts; `--exact` mit Modulpfad läuft ins Leere; `running 0 tests` endet
ebenfalls mit `test result: ok` — die Zeile `running N tests` gegen die erwartete
Zahl halten.

**Display**: `scripts/check-display-tests.sh` (bzw. `--rule-named`). Neue
display-gebundene Tests tragen zwingend
`#[ignore = "requires a display; run via xvfb-run"]` und beginnen mit
`let _main_context = crate::ui::test_main_context::lock_main_context();` gefolgt
von `gtk4::init().unwrap();`. Ein einzeln roter Display-Test im Rudel ist **kein**
Beleg für einen Fehler — isoliert nachfahren. `dev` hat bekannte rote
Display-Tests.
