---
slug: concerts-duplicate-events
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Dieselbe Veranstaltung erscheint doppelt in der Concerts-Liste

**Befund mit eingegrenzter Ursache, kein Plan.** Gemeldet am 16.08.2026: *„es
sollten keine Events mehrfach angezeigt werden."* Belegt durch einen Screenshot
der Concerts-Ansicht (laufender Build 0.1.13 = `dev`-Kopf `95b4b30016`), Filter
`Zurich · 1000 km`, 39 von 413 Konzerten.

## Symptom

Zwei aufeinanderfolgende Zeilen sind in jeder sichtbaren Spalte identisch:

| Artist | Date | City | Distance | Tickets |
| --- | --- | --- | --- | --- |
| Electric Callboy | 14.02.2027 | Amsterdam | 607 km | Unknown |
| Electric Callboy | 14.02.2027 | Amsterdam | 607 km | Unknown |

## Eingegrenzt: es muss der Veranstaltungsort sein

Der Schlüssel gegen Dubletten ist `dedupe_key(date_key, city, venue)`
(`crates/reprise-core/src/concerts/dedupe.rs:20-27`) — Datum, Stadt und
**Veranstaltungsort**, jeweils unicode-normalisiert und kleingeschrieben
(`normalize_component`, `:8-18`). In der Datenbank steht darauf eine harte
Zusage: `dedupe_key TEXT NOT NULL UNIQUE` (`crates/reprise-core/src/db_concerts.rs:40`).

**Daraus folgt zwingend:** zwei Zeilen mit gleichem Datum, gleicher Stadt und
gleichem Ort *können* nicht beide existieren. Die beiden sichtbaren Zeilen
haben also **verschiedene Veranstaltungsort-Zeichenketten** — die Tabelle zeigt
die Spalte `Venue` gar nicht, deshalb sieht es wie eine exakte Dublette aus.

Typische Auslöser dieser Art (ungeprüft, welcher hier greift): `AFAS Live` vs.
`AFAS Live Amsterdam`, `Melkweg (Max)` vs. `Melkweg`, ein Zusatz wie `- Main
Hall`, oder zwei Anbieter mit unterschiedlicher Schreibweise. `merge()`
(`dedupe.rs:29-45`) fasst Ticketmaster und Bandsintown zusammen — aber nur bei
**identischem** Schlüssel.

### Nächster Schritt: den Ort sichtbar machen

Bevor irgendetwas gebaut wird, die zwei Zeilen aus der Datenbank holen und die
`venue`-Werte vergleichen:

```sql
SELECT venue, provider, ticket_source, dedupe_key
  FROM concert_events
 WHERE date_key LIKE '2027-02-14%' AND city = 'Amsterdam';
```

Das entscheidet, ob eine Normalisierung des Ortsnamens reicht (Klammerzusätze,
Stadtname am Ende, Interpunktion) oder ob ein unschärferer Vergleich nötig ist.
**Nicht raten** — jede Ortsnormalisierung, die zu grob ist, verschmilzt zwei
echte Veranstaltungen in derselben Stadt am selben Tag.

## Zweiter Befund, unabhängig davon: der Schlüssel kennt den Künstler nicht

`dedupe_key` enthält **keinen** Künstler, und die Eindeutigkeit gilt
tabellenweit. Beim Einfügen entscheidet
`pipeline.rs:407-417` (`ON CONFLICT(dedupe_key) DO UPDATE`), dass ein
*exakter* Künstler einen *ähnlichen* verdrängt (`is_similar`), sonst bleibt der
bestehende stehen.

Folge: **Zwei verschiedene Künstler aus der Bibliothek am selben Tag, in
derselben Stadt, am selben Ort — also ein Festival — teilen sich eine Zeile.**
Der zweite überschreibt Datum/Ort des ersten oder fällt still weg. Das ist die
Kehrseite desselben Schlüssels und **nicht** das, was der Nutzer gemeldet hat;
es steht hier, damit es beim Umbau nicht übersehen wird. Ungeprüft, ob es in
seiner Bibliothek bereits auftritt.

Wer den Schlüssel anfasst, muss beides zugleich lösen: unschärfer beim Ort,
schärfer beim Künstler.

## Offene Fragen

- Welcher der beiden Anbieter liefert welche Schreibweise? (`provider`-Spalte
  in der Abfrage oben.)
- Soll die Liste eine **Venue**-Spalte bekommen? Ohne sie sieht jede
  Ort-Variante wie ein Fehler aus, auch nach dem Fix — und zwei echte
  Veranstaltungen am selben Tag in derselben Stadt wären ununterscheidbar.
