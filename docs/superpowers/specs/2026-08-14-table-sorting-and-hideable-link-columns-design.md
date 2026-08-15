---
title: Sortierung, die sortiert — und ausblendbare Link-Spalten
date: 2026-08-14
phase: design
surfaces: [concerts, releases]
---

# Sortierung, die sortiert — und ausblendbare Link-Spalten

## Ausgangslage

Der Nutzer meldet drei Dinge an derselben Stelle: „ich kann hier nicht nach
Artist sortieren", „ich möchte auch Tickets-Links ausblenden können", „mache
diese Spaltenanordnung zum Default". Beim Nachsehen im Code stellt sich die
erste Meldung als Spitze eines größeren Befunds heraus — die Sortierung beider
Tabellen ist weitgehend Attrappe. Der Nutzer fasst es nach Ansicht der
Releases-Tabelle als „ganz kaputt" zusammen. Das trifft es.

Alle Zeilenangaben gegen `origin/dev` @ `7694c636b3`.

## Was heute wirklich passiert

### Concerts

`ConcertColumn` hat auf `dev` sieben Spalten und **keine** Pins mehr:
`Date, Artist, City, Venue, Distance, Tickets, Source`, davon sechs sichtbar
(Source aus). Jede Spalte trägt eine Widget-ID.

| Spalte   | ID | Sorter | Klick sortiert nach |
|----------|----|--------|---------------------|
| Date     | ✓  | ✓      | Datum ✔             |
| Artist   | ✓  | —      | **nichts**          |
| City     | ✓  | ✓      | **Datum**           |
| Venue    | ✓  | ✓      | **Datum**           |
| Distance | ✓  | ✓      | Distanz ✔           |
| Tickets  | ✓  | —      | —                   |
| Source   | ✓  | —      | —                   |

Zwei getrennte Fehler:

1. `artist_column` (`concerts_columns.rs:78-137`) ruft als einzige Spalte mit
   ID niemals `set_sorter`. `text_column` setzt den Sorter für jede Spalte mit
   ID (`concerts_columns.rs:220-223`), `artist_column` baut seine Spalte selbst
   und vergisst ihn. Deshalb ist der Header tot.
2. `apply_sort` (`concerts_view.rs:756-772`) kennt genau eine ID:

   ```rust
   let key = match column.id().as_deref() {
       Some("distance") => ConcertSortKey::Distance,
       _ => ConcertSortKey::Date,
   };
   ```

   Der Wildcard verschluckt `city` und `venue`. Der Pfeil wandert auf die
   geklickte Spalte, sortiert wird nach Datum. `ConcertSortKey`
   (`concerts_presentation.rs:11-14`) hat nur `Date` und `Distance`.

`wire_sorting` verbindet hier immerhin beide Signale
(`primary_sort_column_notify` **und** `primary_sort_order_notify`).

### Releases

`wire_sorting` (`releases_view.rs:663-683`) verbindet **nur**
`primary_sort_order_notify` und liest `primary_sort_column()` überhaupt nicht.
Der Handler ruft immer `artist_news::sort_release_rows(rows, direction)`.

Folge: Ein Klick auf „Artist" oder „Release" wechselt die primäre Spalte, nicht
die Richtung — das verbundene Signal feuert also gar nicht, und die Zeilen
bleiben unverändert stehen, während der Pfeil umzieht. Selbst wenn es feuerte,
gäbe es nichts zu holen: `sort_rows` (`artist_news_view.rs:124-137`) nimmt
ausschließlich eine Richtung und sortiert immer nach `first_release_date` mit
`title` als Gleichstand-Entscheider. Einen Sortierschlüssel gibt es nicht.

### Die mehreren Pfeile

`GtkColumnViewSorter` führt einen Mehrfach-Sortierstapel; ein Header-Klick legt
die bisherige Spalte auf Rang 2 statt sie zu ersetzen. GTK zeichnet die
Nachrang-Pfeile mit. Beide Ansichten lesen nur `primary_sort_column` — die
zusätzlichen Pfeile behaupten eine Ordnung, die niemand herstellt.

### Die Link-Spalten

`ReleaseColumn::pin()` (`release.rs:61-67`) pinnt `Cover` führend sowie
`Status` und `Buy` abschließend. `layout::normalize` (`layout.rs:31-34`)
erzwingt für jeden Pin Sichtbarkeit, `set_visible` (`layout.rs:79`) ignoriert
den Wunsch, einen Pin zu verstecken, und `EditorModel::columns`
(`registry.rs:378`) blendet Pins ganz aus dem Editor aus. Darum lässt sich die
Link-Spalte nicht abschalten.

In Concerts ist genau das bereits geschehen: `ConcertColumn::pin()` liefert für
jede Spalte `None`, Tickets ist dort schon aus- und einblendbar. Die
Meldung des Nutzers stammt aus einem älteren Build.

## Entwurf

Vier Teile. Teil 1 und 2 gehören zusammen (Sortierung), Teil 3 und 4 sind
unabhängig davon.

### Teil 1 — Die Sortierung anschließen

**Concerts.** `ConcertSortKey` wächst um `Artist`, `City`, `Venue`.
`sort_rows` vergleicht diese drei als Text: erst `to_lowercase`-Vergleich,
dann exakter Vergleich, dann Datum aufsteigend als letzter
Gleichstand-Entscheider. Der Datums-Entscheider ist nicht Kosmetik — ohne ihn
ist die Reihenfolge gleichnamiger Künstler von der Zeilenreihenfolge der Quelle
abhängig und springt bei jedem Refresh.

`artist_column` bekommt denselben Dummy-Sorter, den `text_column` setzt.
`apply_sort` mappt jede ID explizit; unbekannte IDs behalten den bisherigen
Schlüssel, statt still auf Datum zu fallen.

Kein Sorter für `Tickets` und `Source` — dort gibt es nichts zu ordnen, und ein
klickbarer Header ohne Wirkung ist genau der Fehler, den dieser Entwurf
beseitigt.

Die Distance-Spalte ist ein Sonderfall: `LocationColumns` tauscht ihren Sorter
je nach Verfügbarkeit des Standorts gegen `None` und zurück
(`concerts_location_columns.rs`). Das bleibt unangetastet.

**Releases.** `artist_news_view::sort_rows` bekommt einen neuen Parameter:

```rust
pub enum ReleaseSortKey { Date, Title, Artist, Type }

pub fn sort_rows(
    rows: Vec<HistoryEntry>,
    key: ReleaseSortKey,
    direction: ReleaseSortDirection,
) -> Vec<HistoryEntry>
```

`Date` behält exakt das heutige Verhalten samt Titel-Entscheider, damit der
bestehende Test `release_sort_keeps_invalid_dates_last_and_uses_title_tiebreak`
unverändert gilt. Die drei neuen Schlüssel vergleichen ihr Textfeld
case-insensitiv, dann exakt, dann Datum absteigend.

`wire_sorting` verbindet zusätzlich `primary_sort_column_notify` und liest die
Spalten-ID — dieselbe Form, die Concerts schon hat. Cover, Status und Buy
bekommen weiterhin keinen Sorter.

### Teil 2 — Ein Pfeil statt drei

Im `primary_sort_column_notify`-Handler beider Ansichten wird der Stapel auf
die eine Spalte zurückgesetzt:

```rust
column_view.sort_by_column(None, order);              // leert den Stapel
column_view.sort_by_column(Some(&column), order);     // setzt die eine Spalte
```

Der Zweischritt über `None` ist bewusst gewählt: er räumt den Stapel garantiert,
statt sich darauf zu verlassen, dass GTKs `sort_by_column` das intern schon tut.
Beide Aufrufe lösen erneut `notify` aus, deshalb braucht der Handler ein
`Cell<bool>`-Reentranzschloss; ohne das dreht er sich im Kreis.

**Dieser Teil ist der einzige, dessen GTK-Verhalten ich nicht gemessen habe.**
Er stützt sich auf die Doku zu `GtkColumnViewSorter`. Die Umsetzung muss ihn
empirisch nachweisen — ein Display-Test, der zwei Header nacheinander klickt
und danach zählt, wie viele Spalten einen Sortierindikator tragen. Kommt dabei
heraus, dass GTK den Stapel nicht hergibt, ist das ein Befund für den Plan, kein
Anlass, Teil 2 stillschweigend fallenzulassen.

### Teil 3 — Releases verliert seine Trailing-Pins

Statt einen dritten Pin-Zustand („Position fest, aber ausblendbar") in
`ColumnKey` einzuführen, geht Releases denselben Weg, den Concerts bereits
gegangen ist:

```rust
fn pin(self) -> Option<Pin> {
    match self {
        Self::Cover => Some(Pin::Leading),
        _ => None,
    }
}
```

`Status` und `Buy` wandern damit in das freie Band und werden ausblendbar,
verschiebbar und Teil des Spalten-Editors. `DEFAULT_VISIBLE` muss beide
aufnehmen — bisher waren sie nur durch den Pin-Zwang sichtbar.

Weil `registry.rs:95-99` von jeder nicht gepinnten Spalte eine Widget-ID
verlangt, bekommen `status_column` und `link_column`
(`releases_columns.rs:150-388`) je eine. Einen Sorter bekommen sie nicht.
`Cover` bleibt gepinnt und ID-los — `header_dnd::is_pinned_leading`
(`header_dnd.rs:190-192`) erkennt es genau daran.

Die Spaltenreihenfolge ändert sich dadurch **nicht**: `normalize` stellt den
führenden Pin voran und hängt das freie Band in `ALL`-Reihenfolge an, also
weiterhin `Cover, Date, Title, Artist, Type, Status, Buy`.

Gespeicherte Releases-Layouts brauchen keine Migration: `serialize` hat
`status` und `buy` bisher in beide Listen geschrieben (der Pin erzwang es),
also bleiben sie nach dem Parsen sichtbar.

Die Infrastruktur in `reprise-view/src/columns/` und
`reprise-gnome/src/ui/table_columns/` wird nicht angefasst. `Pin` behält seine
zwei Zustände und seine Bedeutung.

### Teil 4 — Neuer Concerts-Default plus einmaliges Zurücksetzen

```rust
const ALL: [ConcertColumn; 7] = [Artist, Date, City, Venue, Distance, Tickets, Source];
const DEFAULT_VISIBLE: [ConcertColumn; 5] = [Artist, Date, City, Distance, Tickets];
```

Ergebnis: `Artist, Date, City, Distance, Tickets` sichtbar, `Venue` und
`Source` aus — die Anordnung aus dem Screenshot des Nutzers.

Eine Migration `migrate_v75` nach dem Muster von `migrate_v62`
(`db_releases_view_scope.rs:1-26`) löscht den gespeicherten Schlüssel einmalig:

```sql
DELETE FROM settings WHERE key = 'ui.column_layout.concerts';
```

`ui.column_widths.concerts` bleibt stehen — Breiten sind von Reihenfolge und
Sichtbarkeit unabhängig, und wer eine Spalte breitgezogen hat, will sie nicht
zurückgesetzt bekommen. `SUPPORTED_SCHEMA_VERSION` steigt von 74 auf 75.

## Was das kostet

Zwei Nebenwirkungen, die der Nutzer kennt und in Kauf nimmt:

1. **Das Zurücksetzen trifft jeden**, auch den Nutzer selbst. Wer sein
   Concerts-Layout angepasst hat, findet nach dem Update den neuen Default vor.
2. **Eine ausgeblendete Status-Spalte macht das Verstecken einer Release
   unerreichbar**, eine ausgeblendete Link-Spalte den Kaufweg — Releases hat
   kein Zeilen-Kontextmenü. Genau dafür war der Pin da. Concerts hat dieselbe
   Abwägung für Tickets bereits zugunsten der Ausblendbarkeit entschieden;
   Releases zieht nach. Umkehrbar ist es aus dem Kopfzeilen-Popover.

## Prüfbarkeit

Neue Tests, ohne Display:

- `sort_rows` (Concerts) je neuem Schlüssel: Sortierung, Richtungsumkehr,
  Gleichstand fällt auf das Datum.
- `sort_rows` (Releases) je neuem Schlüssel, plus der bestehende Datums-Test
  unverändert grün.
- `ConcertColumn`-Default: `Layout::default()` liefert genau die fünf
  sichtbaren Spalten in der neuen Reihenfolge.
- `ReleaseColumn`: nur `Cover` ist gepinnt; `Layout::default()` zeigt
  `Status` und `Buy`; ein gespeichertes Layout aus der Zeit vor der Änderung
  parst unverändert.
- Migration: `user_version` 75 nach dem Lauf, Zeile in `settings` weg, ein
  anderer `ui.*`-Schlüssel unberührt.

Display-gebunden:

- Klick auf jeden sortierbaren Header ordnet die Zeilen tatsächlich um
  (nicht nur den Indikator) — für beide Tabellen.
- Nach zwei Klicks auf verschiedene Header trägt genau **eine** Spalte einen
  Sortierindikator (der Nachweis für Teil 2).
- Das Kopfzeilen-Popover von Releases listet Status und Link und blendet beide
  aus und wieder ein.

## Abgrenzung

Nicht Teil dieses Entwurfs:

- Echte Mehrfachsortierung (erst Artist, bei Gleichstand Datum). Verworfen;
  der Nutzer hat sich für eine Spalte entschieden.
- Ein Zeilen-Kontextmenü als Ersatzweg zu Ticket-, Kauf- und Status-Aktion.
  Wäre die saubere Antwort auf Nebenwirkung 2, ist aber eigener Umfang.
- `Cover` ausblendbar machen.
- Die Sortierung zu persistieren.

## Kollision mit laufender Arbeit

Zwei Stränge von `updates-concerts-releases-rework` sind in Arbeit und noch
nicht in `dev`. Strang 2 hat `releases_view.rs` in Alleinbesitz — dort sitzt
`wire_sorting`. Der Nutzer hat entschieden, trotzdem jetzt zu bauen; die
Auflösung ist eine Funktion von rund zwanzig Zeilen. `releases_columns.rs` hat
sich Strang 2 ausdrücklich gesperrt, `concerts/**` und `reprise-core` sind
frei, weil Strang 1 bereits in `dev` gelandet ist.
