---
slug: table-sorting-and-hideable-link-columns-b
worktree: /home/marvin/Projects/reprise-table-sorting-and-hideable-link-columns-b
branch: feature/table-sorting-and-hideable-link-columns-b
phase: coded
codex_session:
created: 2026-08-15
---

# Strang B — Concerts

## Zweck

Die Concerts-Tabelle sortiert heute weitgehend falsch: `artist_column` trägt als
einzige Spalte mit ID keinen Sorter, und `apply_sort` verschluckt per Wildcard
jede ID außer `distance` und ordnet dann nach Datum, während der Pfeil auf die
geklickte Spalte zieht. Dieser Strang macht sechs Header echt sortierbar,
stellt die vom Nutzer gewünschte Vorgabeanordnung her, verwirft gespeicherte
Layouts einmalig per Migration und hängt den geteilten Ein-Pfeil-Helfer an.

**Vorbedingung:** Strang A ist gelandet.
`crates/reprise-gnome/src/ui/table_columns/single_sort_indicator.rs` existiert
mit `mark(&gtk4::ColumnView)`, `PRIMARY_SORT_INDICATOR_CLASS` und
`count_primary_indicators`. Ist die Datei nicht da, brich ab und melde es —
schreibe den Helfer **nicht** selbst.

Aufgaben in dieser Reihenfolge: **B-1, B-2, B-3, B-4, B-5.** Je Aufgabe ein
fokussierter Commit.

---

## Dateibesitz

Dir gehören diese Bäume vollständig:

```
crates/reprise-gnome/src/ui/concerts/**
crates/reprise-view/src/columns/concert.rs
crates/reprise-core/src/db_concerts*.rs
crates/reprise-core/src/db.rs
```

Innerhalb dieser Bäume darfst du jede Änderung machen, die die Aufgaben
brauchen — auch an Dateien, die dieser Plan nicht namentlich nennt. Neue
Hilfsfunktionen, verschobene reine Funktionen, angepasste Nachbartests:
erlaubt, solange sie in deinen Bäumen liegen.

## Was dir **nicht** gehört

Fasse außerhalb deiner Bäume **nichts** an:

- `crates/reprise-gnome/src/ui/table_columns/**` — Strang A. Du **rufst** den
  Ein-Pfeil-Helfer, du änderst ihn nicht.
- `crates/reprise-gnome/src/ui/track_list/**`, `.../ui/style/**` — Strang A
- `crates/reprise-gnome/src/ui/releases/**`,
  `crates/reprise-view/src/columns/release.rs`,
  `crates/reprise-core/src/artist_news*.rs` — Strang C, **läuft parallel zu dir**
- `crates/reprise-view/src/columns/key.rs`, `.../layout.rs` — unberührt; `Pin`
  behält seine zwei Zustände
- `crates/reprise-core/src/lib.rs` — wird von keinem Strang angefasst; du legst
  keine neue Datei an, `migrate_v75` lebt im bereits deklarierten `db_concerts`
- `docs/ux-rules.md` — Strang R. Auch wenn dir auffällt, dass eine Regel durch
  deine Arbeit falsch wird: **nicht dort schreiben**, in den Bericht damit.
- `po/POTFILES.in` — unverändert; du brauchst keine neuen Zeichenketten

**Lesen ist immer erlaubt.** Zwei Bezeichner, die du brauchst, liegen außerhalb
deiner Bäume und werden nur **referenziert**, nicht geändert:

- `CONCERTS_COLUMN_LAYOUT_KEY`
  (`crates/reprise-core/src/library/settings_column_keys.rs:9`) — für den
  Literal-Test in B-4
- `crate::ui::table_columns::registry::filler_for` (`registry.rs:430`,
  `pub(in crate::ui)`) — für den Füller-Test in B-4

---

## Die gemeinsame Vergleichsregel (MISSING-LAST)

Bindend für **jede** neue Textsortierung dieses Strangs. Ein Feld, das leer ist
oder nur aus Leerzeichen besteht, wird wie ein fehlender Wert behandelt und
landet **richtungsunabhängig am Ende** — genau wie `compare_optional`
(`concerts_presentation.rs:64-81`) es heute für Datum und Distanz tut.

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

Gilt für **Artist, City, Venue und Source**. `compare_optional` bleibt
**unverändert** — die neuen Schlüssel gehen nicht durch sie, weil ihre Felder
`String` sind und kein `Option`; `compare_text` stellt dieselbe Semantik her.

---

## Aufgabe B-1 — Sortierschlüssel und Textvergleich

**Ziel:** `ConcertSortKey` kennt Artist, City, Venue und Source; `sort_rows`
ordnet danach; die Zuordnung ID → Schlüssel ist eine reine, testbare Funktion.

**Bereich:** `crates/reprise-gnome/src/ui/concerts/concerts_presentation.rs`
(display-frei, kein GTK), `concerts_status_cells.rs`.

### Vorgehen

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConcertSortKey { Date, Distance, Artist, City, Venue, Source }

/// Unbekannte IDs — `tickets` oder eine Spalte aus einem neueren Build —
/// liefern `None`; der Aufrufer lässt die Sortierung dann unangetastet.
pub(super) fn sort_key_for_id(id: Option<&str>) -> Option<ConcertSortKey>
```

1. `sort_key_for_id` mappt **jede** ID explizit über `ConcertColumn::as_str()`
   (kein String-Literal): `date`, `distance`, `artist`, `city`, `venue`,
   `source` → `Some`; `tickets`, `None` und alles andere → `None`. **Kein
   Wildcard-Fallback auf `Date`** — der ist genau der Fehler, den dieser Strang
   beseitigt.
2. `compare_text` und `present` nach der oben stehenden Vergleichsregel neu
   anlegen.
3. **`source_name` zieht um.** Sie steht heute in `concerts_status_cells.rs:20`,
   ist aber eine reine Funktion (`ticket_source`, getrimmt und nichtleer, sonst
   `provider`) und wird jetzt auch von der Sortierung gebraucht. Verschiebe sie
   nach `concerts_presentation.rs`; `concerts_status_cells.rs` (Zellenbindung und
   `row_link_presentation`) ruft sie von dort. Beide Dateien gehören dir.
   **Sortiert wird nach genau dem Wert, den die Zelle zeigt** — nicht nach dem
   rohen `ticket_source`, sonst ordnet die Spalte sichtbar anders, als sie
   aussieht.
4. `sort_rows` bekommt vier neue Arme:

   ```rust
   ConcertSortKey::Artist => compare_text(&left.artist_name, &right.artist_name, direction)
       .then_with(|| date_tiebreak(left, right)),
   ConcertSortKey::City   => compare_text(&left.city,  &right.city,  direction).then_with(…),
   ConcertSortKey::Venue  => compare_text(&left.venue, &right.venue, direction).then_with(…),
   ConcertSortKey::Source => compare_text(source_name(left), source_name(right), direction)
       .then_with(…),
   ```

   mit dem Gleichstand-Entscheider

   ```rust
   /// Immer aufsteigend, unabhängig von `direction`: der Entscheider stellt
   /// Stabilität her, er drückt keine Ordnung aus. Würde er mitdrehen, sprängen
   /// gleichnamige Zeilen beim Richtungswechsel doppelt.
   fn date_tiebreak(left: &ConcertRow, right: &ConcertRow) -> Ordering {
       compare_optional(
           NaiveDate::parse_from_str(&left.date_key, "%Y-%m-%d").ok(),
           NaiveDate::parse_from_str(&right.date_key, "%Y-%m-%d").ok(),
           SortDirection::Ascending,
       )
   }
   ```

   Der Kommentar gehört wörtlich an die Stelle.
5. Die Arme `Date` und `Distance` bleiben **bitgleich**.

### Nachweis (Unit)

```
cargo test -p reprise-gnome concerts_presentation > $SCRATCH/b1.log 2>&1
grep -E "^test result|running [0-9]+ tests|FAILED|panicked" $SCRATCH/b1.log
```

Neue Tests:

- `artist_sort_is_case_insensitive_and_falls_back_to_the_date`
- `city_and_venue_reverse_with_the_direction_but_keep_the_date_tiebreak_ascending`
- `source_sorts_by_the_displayed_name_not_the_raw_field` — eine Zeile mit
  `ticket_source: None` und `provider: "ticketmaster"` sortiert dort ein, wo
  „ticketmaster" hingehört, nicht am Ende.
- `a_blank_text_field_sorts_last_in_both_directions` — **der geforderte
  MISSING-LAST-Test.** Mindestens eine Zeile mit `""` und eine mit `"   "`;
  beide stehen bei Ascending **und** bei Descending hinten. Deckt Artist, City,
  Venue und Source in einem Test ab.
- `sort_key_for_id_maps_every_sortable_column_and_rejects_the_rest` — inklusive
  `assert_eq!(sort_key_for_id(Some(ConcertColumn::Tickets.as_str())), None)` und
  `assert_eq!(sort_key_for_id(None), None)`.

Bestandstests unverändert grün:
`sort_keeps_missing_distances_at_the_end_in_both_directions`,
`date_sort_defaults_to_chronological_order_and_invalid_dates_end_last`.

---

## Aufgabe B-2 — Die Header anschließen

**Ziel:** Jeder sortierbare Concerts-Header ordnet tatsächlich seine eigene
Spalte. Sortierbar sind **Date, Artist, City, Venue, Distance, Source**.
`Tickets` bleibt klick-tot, weil es keinen Sorter trägt.

**Bereich:** `concerts_columns.rs`, `concerts_status_cells.rs`, `concerts_view.rs`.

### Vorgehen

1. In `concerts_columns.rs` den Dummy-Sorter einmalig benennen, statt ihn
   mehrfach zu literalisieren:

   ```rust
   /// Attrappe: die eigentliche Ordnung stellt `apply_sort` über das Modell her.
   /// Der Sorter existiert nur, damit GTK den Header klickbar macht und einen
   /// Indikator zeichnet.
   pub(super) fn header_sorter() -> gtk4::CustomSorter {
       gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)
   }
   ```

   `text_column` (`:220-223`) nutzt sie statt des Inline-Sorters.
2. `artist_column` (`:78-137`) baut seine Spalte selbst und vergisst heute den
   Sorter — das ist der tote Header. Nach dem `builder().id(…)`-Block:
   `column.set_sorter(Some(&header_sorter()));`
3. **Neu gegenüber der Spec:** `source_column` (`concerts_status_cells.rs:138`)
   bekommt ebenfalls `set_sorter(Some(&header_sorter()))`. Die Zelle rendert
   `source_name(&row)` als gewöhnliches Label und trägt bereits
   `.id(ConcertColumn::Source.as_str())` — es fehlte nur der Sorter.
4. `ticket_column` bekommt **keinen** Sorter. Die Zelle ist ein Knopf, dessen
   Beschriftung fehlen kann; hier ist nichts zu ordnen. Hier ist nichts zu tun
   außer nichts zu tun — ein Test hält es fest.
5. `apply_sort` (`concerts_view.rs:756-772`) verliert seinen Wildcard:

   ```rust
   let Some(key) = sort_key_for_id(column.id().as_deref()) else {
       return; // unbekannte Spalte: die bestehende Ordnung bleibt stehen
   };
   ```

   Der frühe Rücksprung **ist** die Forderung „unbekannte IDs behalten den
   bisherigen Schlüssel" aus der Spec, ohne dafür neuen Zustand in `Shared` zu
   halten: das Modell wird nicht angefasst, also bleibt die zuletzt hergestellte
   Ordnung sichtbar.

`wire_sorting` verbindet hier bereits **beide** Signale
(`primary_sort_column_notify` und `primary_sort_order_notify`) — daran ändert
sich nichts.

### Nachweis (Display)

```
scripts/check-display-tests.sh > $SCRATCH/b2.log 2>&1
```

Neu in `concerts_view_tests.rs`:

- `every_sortable_concerts_header_orders_its_own_column` — baut die Ansicht,
  legt drei Zeilen mit paarweise verschiedenen Artist/City/Venue/Source/Datum an,
  holt die Spalte per Helfer

  ```rust
  fn column_by_id(view: &gtk4::ColumnView, id: &str) -> gtk4::ColumnViewColumn
  ```

  (über `view.columns()` iterieren, `column.id()` vergleichen), ruft je
  `view.sort_by_column(Some(&column), Ascending)` und liest die Reihenfolge aus
  `shared.model` zurück. Für `artist`, `city`, `venue`, `source`, `date`,
  `distance`. Beachte: `venue` und `source` sind nach B-3 per Vorgabe **nicht
  sichtbar** — die Spalte existiert trotzdem in `view.columns()` und ist
  sortierbar; der Test darf sich nicht auf Sichtbarkeit verlassen.
- `only_the_ticket_header_carries_no_sorter` —
  `assert!(column_by_id(view, "tickets").sorter().is_none())`; Gegenprobe
  `is_some()` für `artist`, `city`, `venue`, `source`, `date`.

---

## Aufgabe B-3 — Neue Reihenfolge und neue Vorgabesichtbarkeit

**Ziel:** `Artist, Date, City, Distance, Tickets` sichtbar; `Venue` und `Source`
aus; Reihenfolge wie im Screenshot des Nutzers.

**Bereich:** `crates/reprise-view/src/columns/concert.rs`.

### Vorgehen

```rust
const ALL: [ConcertColumn; 7] = [Artist, Date, City, Venue, Distance, Tickets, Source];
const DEFAULT_VISIBLE: [ConcertColumn; 5] = [Artist, Date, City, Distance, Tickets];
```

`append_columns` in `concerts_columns.rs` wird **nicht** umsortiert: Concerts hat
keine Pins, jede Spalte trägt eine ID, also bindet `bind_columns_by_id` rein über
die ID und `ColumnRegistry::apply` stellt anschließend die physische Reihenfolge
nach `layout.order` her. Die Anbaureihenfolge ist bedeutungslos; sie zu ändern
wäre Lärm im Diff.

### Nachweis (Unit)

```
cargo test -p reprise-view concert > $SCRATCH/b3.log 2>&1
cargo test -p reprise-gnome concerts_location_columns >> $SCRATCH/b3.log 2>&1
```

- `the_default_concert_layout_shows_status_but_hides_source` wird auf die neue
  Vorgabe umgeschrieben und dabei umbenannt in
  `the_default_concert_layout_leads_with_the_artist_and_hides_venue_and_source`:
  `layout.order == [Artist, Date, City, Venue, Distance, Tickets, Source]`,
  `visible` enthält Artist/Date/City/Distance/Tickets, enthält **nicht** Venue
  und **nicht** Source.
- `concert_columns_round_trip_without_pinning_status_or_source` bleibt
  unverändert.
- Gegenprobe ohne Änderungsbedarf:
  `automatic_distance_visibility_never_changes_the_user_layout`
  (`concerts_location_columns.rs`) prüft `layout.visible.contains(&Distance)` —
  bleibt wahr. Trotzdem mitlaufen lassen.

---

## Aufgabe B-4 — Migration v75 und der Füller-Nebeneffekt

**Ziel:** Der gespeicherte Concerts-Spaltenschlüssel wird einmalig gelöscht,
damit die neue Vorgabe jeden erreicht; die gespeicherten Breiten bleiben stehen.

**Bereich:** `crates/reprise-core/src/db_concerts.rs`,
`crates/reprise-core/src/db.rs`,
`crates/reprise-core/src/db_concerts_migration_tests.rs`,
`crates/reprise-gnome/src/ui/concerts/concerts_column_layout.rs` (nur Test).

### Vorgehen

1. `migrate_v75` in `db_concerts.rs` — **keine neue Datei**: das Modul ist
   bereits deklariert, trägt bereits `migrate_v73` und ist thematisch der Ort.
   Muster wörtlich von `db_releases_view_scope::migrate_v62`:

   ```rust
   const SCHEMA_V75: &str = r#"
   DELETE FROM settings WHERE key = 'ui.column_layout.concerts';
   "#;

   pub(crate) fn migrate_v75(conn: &Connection) -> Result<(), rusqlite::Error> {
       let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
       if version >= 75 { return Ok(()); }
       let transaction = conn.unchecked_transaction()?;
       transaction.execute_batch(SCHEMA_V75)?;
       transaction.pragma_update(None, "user_version", 75)?;
       transaction.commit()
   }
   ```

   Der Schlüssel steht als **Literal** in der SQL, wie in allen bestehenden
   Migrationen: eine Migration darf einer späteren Umbenennung der Konstante
   nicht folgen. Damit eine solche Umbenennung trotzdem laut wird, kommt in
   denselben Commit ein Test, der `CONCERTS_COLUMN_LAYOUT_KEY` gegen das Literal
   hält.
2. `db.rs`: `crate::db_concerts::migrate_v75(conn)?;` als **letzter** Eintrag in
   `migrate_connection` (die Kette endet heute mit
   `db_new_releases_notify::migrate_v74`), und
   `SUPPORTED_SCHEMA_VERSION: i64 = 75` (`db.rs:26`).
3. `ui.column_widths.concerts` wird **nicht** gelöscht. Breiten sind von
   Reihenfolge und Sichtbarkeit unabhängig, und wer eine Spalte breitgezogen
   hat, will sie nicht zurückgesetzt bekommen.

### Nachweis (Unit)

```
cargo test -p reprise-core db_concerts > $SCRATCH/b4.log 2>&1
cargo test -p reprise-core migration >> $SCRATCH/b4.log 2>&1
cargo test -p reprise-gnome concerts_column_layout >> $SCRATCH/b4.log 2>&1
```

- `v75_drops_the_stored_concerts_column_layout_and_keeps_the_widths`
  (in `db_concerts.rs`): Verbindung öffnen, `settings` mit
  `ui.column_layout.concerts` **und** `ui.column_widths.concerts` **und** einem
  dritten `ui.*`-Schlüssel (z. B. `ui.column_layout.releases`) füllen,
  `user_version` auf 74 setzen, `migrate_v75` **zweimal** rufen (Idempotenz wie
  bei v62), danach: Layout-Zeile weg, Breiten-Zeile da, Releases-Zeile da,
  `PRAGMA user_version == 75`.
- `supported_schema_version_is_v74` in `db_concerts_migration_tests.rs` wird auf
  75 gezogen und umbenannt in `supported_schema_version_is_v75`.
- `the_concerts_layout_setting_key_matches_the_frozen_migration_literal`:
  `assert_eq!(CONCERTS_COLUMN_LAYOUT_KEY, "ui.column_layout.concerts")`.

### Nachweis (Unit, Füller-Nebeneffekt)

In `concerts_column_layout.rs`:

- `hiding_venue_by_default_moves_the_filler_to_the_artist_column`:

  ```rust
  assert_eq!(
      crate::ui::table_columns::registry::filler_for(
          &Layout::<ConcertColumn>::default(), ConcertColumn::Venue),
      Some(ConcertColumn::Artist)
  );
  ```

**Der bevorzugte Füller in `width_persistence::wire(…, ConcertColumn::Venue)`
bleibt unverändert.** Ihn auf Artist zu ziehen änderte das Verhalten für jeden,
der Venue wieder einblendet: `concerts_location_columns.rs` schaltet
`venue.set_expand()` bewusst um, wenn die Distanz-Spalte mangels Standort
verschwindet. Der Test nagelt nur fest, was die neue Vorgabe über den
Rückfallpfad `filler_for` auslöst — er erzwingt es nicht per Konfiguration.

---

## Aufgabe B-5 — Concerts bekommt den einen Pfeil

**Ziel:** Der geteilte Helfer aus Strang A hängt an der Concerts-Tabelle.

**Bereich:** `concerts_view.rs` (eine Zeile).

### Vorgehen

```rust
crate::ui::table_columns::single_sort_indicator::mark(&column_view);
```

An der Stelle, an der die Ansicht ihre CSS-Klassen setzt bzw. unmittelbar nach
dem Aufbau des `ColumnView` — jedenfalls **bevor** die Ansicht ihre
Vorgabesortierung anstößt, damit schon der erste Sortierzustand einen einzigen
Indikator zeigt.

**Kein Reentranzschloss, kein Zweischritt, keine Anschlussreihenfolge relativ zu
`wire_sorting`.** Der Helfer markiert nur den Indikator der jeweils aktuellen
Primärspalte und setzt nie eine Sortierung um; er kann deshalb mit keinem
anderen Handler kollidieren. Der Entwurf sah hier ursprünglich eine
Variante mit `sort_by_column(None, …)` und `Cell<bool>` vor — die ist verworfen.

### Zwingend zu bedenken — Distance

`LocationColumns::apply` ruft selbst `view.sort_by_column(…)`, wenn die
Standortverfügbarkeit kippt (`concerts_location_columns.rs:106`, `:126`), und
`sort_by_date()` (`:133`) tut dasselbe. Mit dem gewählten Mechanismus ist das
folgenlos: diese Aufrufe lösen ein normales `notify` aus, der Helfer markiert
den neuen Indikator, fertig. Der Test unten hält es fest.

### Nachweis (Display)

```
scripts/check-display-tests.sh > $SCRATCH/b5.log 2>&1
```

Neu in `concerts_view_tests.rs`:

- `two_concert_sorts_leave_one_indicator` — zwei `sort_by_column` auf
  verschiedene Spalten, danach
  `single_sort_indicator::count_primary_indicators(view.upcast_ref()) == 1`.
  Den Zählhelfer aus Strang A benutzen, **nicht** kopieren.
- `losing_the_location_still_falls_back_to_the_date_sort` — Bestandsverhalten
  gegen den neuen Helfer: nach `sort_by_distance(Descending)` und
  `location_columns.apply(false)` liefert `primary_sort()` weiterhin
  `("date", Ascending)`, und es bleibt genau ein markierter Indikator. Läuft
  neben dem Bestandstest
  `conc_2_location_availability_hides_distance_without_overwriting_user_choice`
  (`concerts_view_tests.rs:146`), der unverändert bleibt.

---

## Randfälle

**RF-B1 — Leerwerte zählen als fehlend.** Siehe die Vergleichsregel oben. Leere
und reine Leerzeichen-Felder stehen in **beiden** Richtungen hinten. Das ist
bewusst dieselbe Semantik wie bei Datum und Distanz und weicht vom Entwurf ab,
der leere Strings noch als gültige Werte vorn einsortieren wollte. Nachweis:
`a_blank_text_field_sorts_last_in_both_directions`.

**RF-B2 — Source sortiert nach dem angezeigten Namen.** `source_name` fällt von
`ticket_source` auf `provider` zurück. Sortierte die Spalte nach dem rohen
`ticket_source`, stünden alle Zeilen ohne Quelle am Ende, obwohl sie sichtbar
einen Provider-Namen tragen — die Spalte würde anders ordnen, als sie aussieht.
Nachweis: `source_sorts_by_the_displayed_name_not_the_raw_field`.

**RF-B3 — Der Gleichstand-Entscheider dreht nicht mit.** Das Datum entscheidet
Gleichstände immer **aufsteigend**, unabhängig von der Sortierrichtung: es
stellt Stabilität her, es drückt keine Ordnung aus. Würde es mitdrehen, sprängen
gleichnamige Zeilen beim Richtungswechsel doppelt. Kommentar an der Stelle,
Nachweis:
`city_and_venue_reverse_with_the_direction_but_keep_the_date_tiebreak_ascending`.

**RF-B4 — Die Distance-Spalte tauscht ihren Sorter.** `LocationColumns::apply`
setzt den Sorter der Distance-Spalte auf `None`, wenn kein Standort da ist,
versteckt die Spalte, und stellt beides beim Zurückkommen wieder her. **Das
bleibt unangetastet.** `header_sorter()` ist nicht der Ort, an dem das
zentralisiert wird.

**RF-B5 — Unbekannte IDs.** `apply_sort` kehrt bei `None` früh zurück und lässt
das Modell in Ruhe. Sichtbar bleibt damit die zuletzt hergestellte Ordnung. Das
ist die Umsetzung von „unbekannte IDs behalten den bisherigen Schlüssel" ohne
neuen Zustand in `Shared`. Der einzige Fall ist heute `tickets`; ein Layout aus
einem neueren Build könnte weitere liefern.

**RF-B6 — Die Migration trifft `filler_for`.** Nach dem Löschen des
Concerts-Schlüssels startet die Ansicht mit `Layout::default()`: Venue ist aus,
also greift `filler_for` und **Artist** wird faktisch Füller, obwohl der
bevorzugte Füller in `width_persistence::wire` weiterhin `Venue` ist. Sichtbar
ist das nur daran, dass Artist die Restbreite schluckt — was der Kommentar in
`concerts_columns.rs:134` ohnehin behauptet. Belegt durch
`hiding_venue_by_default_moves_the_filler_to_the_artist_column`.

**RF-B7 — `sort_fallback` ist nicht betroffen.** Die Sortierung wird nicht
persistiert (ausdrücklich außerhalb des Spec-Umfangs), und die Ansicht startet
immer mit `sort_by_date()`, dessen Spalte in der neuen Vorgabe sichtbar bleibt.

**RF-B8 — Venue und Source sind sortierbar, aber per Vorgabe unsichtbar.** Kein
Widerspruch: die Spalte existiert in `view.columns()` und trägt ihren Sorter,
egal ob sie eingeblendet ist. Wer sie im Kopfzeilen-Popover einblendet, bekommt
sofort einen funktionierenden Header. Display-Tests dürfen sich deshalb nicht
auf Sichtbarkeit verlassen, wenn sie die Spalte über `column_by_id` holen.

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
