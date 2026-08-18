---
slug: concerts-duplicate-events
worktree: /home/marvin/Projects/reprise-concerts-duplicate-events
branch: feature/concerts-duplicate-events
phase: planned
codex_session:
created: 2026-08-18
---
# Plan: Ein Konzert, eine Zeile — der Dublettenschlüssel steht auf dem falschen Feld

Aus dem Befund vom 16.08.2026 (*„es sollten keine Events mehrfach angezeigt
werden"*). Der Befund hatte den Verdacht auf den Veranstaltungsort eingegrenzt
und ausdrücklich verlangt, vor jedem Entwurf die echten `venue`-Werte zu lesen.
Das ist am 18.08.2026 geschehen und **ändert die Stoßrichtung**: eine
Normalisierung des Ortsnamens reicht nachweislich nicht. Gegrillt am 18.08.2026,
fünf Entscheidungen unten jeweils an der Messung begründet.

## Was gemessen wurde

Live-Datenbank (`~/.local/share/reprise/reprise.db`, Kopie mit WAL), 413 Zeilen
in `concert_events`, **ein** Anbieter (`provider = ticketmaster`, 43 Künstler),
alle Zeilen `is_similar = 0`. Fünf Dublettenpaare, alle mit identischem
`artist_key`, `date_key` und `city`:

| Künstler | Datum | Stadt | venue A | venue B | Abstand A↔B |
| --- | --- | --- | --- | --- | --- |
| Catch Your Breath | 15.11.2026 | New Haven | `Toad's Place` | `Toads Place - CT` | 0,7 km |
| Chelsea Grin | 28.11.2026 | Chicago | `Riviera Theatre` | `Riviera Theatre- IL` | 0,3 km |
| Electric Callboy | 14.02.2027 | Amsterdam | `Ziggo Dome` | `Ziggo Dome Club` | 0,5 km |
| Ocean Sleeper | 19.09.2026 | Grand Rapids | `Intersection` | `The Intersection` | **14 km** |
| Wage War | 15.01.2027 | Cardiff | `Cardiff University Students Union` | `Y Plas, Cardiff Students Union` | 0 km |

Die zweite Zeile ist jedes Mal **dieselbe Veranstaltung bei einem anderen
Ticketverkäufer oder in einem anderen Paket**, von der Ticketmaster-Discovery-API
als eigenes Event geliefert:

| Künstler | Auflistung A | Auflistung B |
| --- | --- | --- |
| Catch Your Breath | `ticketmaster.com/event/Z7r9jZ1A70U-U` | `etix.com/…` |
| Chelsea Grin | `ticketmaster.com/event/Z7r9jZ1A7P88F` | `axs.com/…` |
| Electric Callboy | `ticketmaster.nl/… VIP Upgrades` | `ticketmaster.nl/… Venue Premium Packages` |
| Ocean Sleeper | `ticketmaster.com/event/Z7r9jZ1AAZ3xp` | `etix.com/…` |
| Wage War | `ticketmaster.co.uk/…` | `universe.com/…?ref=ticketmaster` |

`ticket_availability` ist innerhalb jedes Paars identisch und taugt nicht zur
Unterscheidung; die Einfügereihenfolge liefert mal die eine, mal die andere
Schreibweise zuerst.

### Daraus folgt: keine Ortsnormalisierung löst das

Die fünf Abweichungen liegen in fünf verschiedenen Klassen — Apostroph,
angehängtes Bundesland (`- CT`, `- IL`, einmal ohne Leerzeichen), führendes
„The", ein zusätzliches Raumwort (`Club`), und ein vollständig anderer Name
(`Y Plas` ist der Saal *innerhalb* der Students Union). Eine Normalisierung, die
alle fünf einfängt, müsste `Ziggo Dome Club` mit `Ziggo Dome` und `Y Plas,
Cardiff Students Union` mit `Cardiff University Students Union` verschmelzen —
und wäre damit so grob, dass sie zwei echte Säle desselben Hauses zusammenwirft.

Auch die Koordinaten tragen nicht: identisch für Cardiff, 0,3–0,7 km für drei
Paare, **14 km** für Grand Rapids — bei demselben Veranstaltungsort. Ein
Geo-Radius, der Grand Rapids einfängt, greift quer durch jede Großstadt. Die
Startzeiten sind bis auf ein Paar identisch (`20:00` gegen `20:01`), ein
minutengenauer Vergleich scheidet damit ebenfalls aus.

## Die Entscheidung

`dedupe_key(artist_key, date_key, city)` — der Ort fällt ersatzlos aus dem
Schlüssel, der Künstler kommt hinein. Das löst beide Befunde in einem Zug:

- **Dubletten (der gemeldete Befund).** Alle fünf Paare fallen zusammen, weil
  sie sich ausschließlich im Ort unterscheiden.
- **Festival-Kollision (der zweite Befund).** Weil der Künstler im Schlüssel
  steht, teilen sich zwei Künstler am selben Tag, in derselben Stadt, am selben
  Ort keine Zeile mehr. Heute verdrängt dort einer den anderen
  (`pipeline.rs:440-455`).

**Der bewusst gewählte Preis:** zwei echte Auftritte *desselben* Künstlers am
selben Tag in derselben Stadt (Matinee und Abendshow in zwei Häusern) fallen zu
einer Zeile zusammen. In den gemessenen 413 Zeilen kommt das nicht vor. Test 5
hält den Verlust fest, statt ihn zu verschweigen.

`dedupe_key` **bleibt eine abgeleitete Textspalte** mit `UNIQUE`; nur ihr Inhalt
wechselt. Ein zusammengesetzter `UNIQUE(artist_key, date_key, city)` wäre
sauberer, würde aber die Normalisierung (Kleinschreibung, Akzente, Leerraum) aus
`dedupe_key()` in die Spalten selbst verlagern und einen vollen Tabellenumbau
erzwingen — derselbe Effekt, deutlich größerer Eingriff.

## Aufgaben

1. **Schlüssel umstellen.** `dedupe_key(artist_key, date_key, city)` in
   `crates/reprise-core/src/concerts/dedupe.rs`. `normalize_component` bleibt
   und gilt für alle drei Bestandteile. Alle Aufrufstellen mitziehen:
   `pipeline.rs` (Einfügen **und** die Stale-Abfrage), `merge()`.
2. **Sieger-Regel für zwei Auflistungen desselben Anbieters.** `merge()` behält
   die bestehende Regel (die erste gewinnt; Bandsintown schlägt Ticketmaster)
   und bekommt eine davor: es überlebt die Auflistung, deren `ticket_url` auf
   der Eigendomäne des Anbieters liegt (`ticketmaster.*`) — der verlässliche
   Kaufweg, und Ortsname und Link bleiben aus einer Quelle, also zueinander
   passend. Bei Gleichstand (Amsterdam: beide auf `ticketmaster.nl`) entscheidet
   weiterhin die Reihenfolge. Die Regel gehört in eine eigene, benannte Funktion,
   weil Aufgabe 3 sie ebenfalls aufruft.
3. **Migration `v76` in `db_concerts.rs`, in Rust.** Bestandszeilen lesen,
   `dedupe_key` mit **derselben** Funktion aus Aufgabe 1 neu berechnen,
   Kollisionen mit **derselben** Funktion aus Aufgabe 2 auflösen, Verlierer
   löschen. Kein Tabellenumbau, `UNIQUE` bleibt stehen. Registrierung in
   `db.rs:757` hinter `migrate_v75`. Dass die Migration den Produktionscode
   aufruft statt die Regel in SQL nachzubauen, ist Absicht: eine zweite
   Implementierung derselben Regel driftet.
4. **Keine Verdrängung ähnlicher Künstler.** Ein exakter und ein ähnlicher
   Künstler auf derselben Veranstaltung ergeben künftig zwei Zeilen; die zweite
   ist in der Artist-Spalte als „similar to X" gekennzeichnet
   (`concerts_columns.rs:47-48`). Die heutige Verdrängung über
   `ON CONFLICT … is_similar` entfällt damit als Sonderfall — jede
   Ersatzregel bräuchte wieder einen Ortsvergleich, und genau der ist laut
   Messung untauglich. Die `MIN(is_similar)`-Logik in der `ON CONFLICT`-Klausel
   bleibt: sie greift jetzt nur noch, wenn *derselbe* Künstler von „ähnlich" zu
   „exakt" wechselt.
5. **Keine UI-Änderung.** Die Spalte `Venue` existiert bereits und ist nur
   standardmäßig aus (`concerts_column_layout.rs`, Test
   `hiding_venue_by_default_moves_the_filler_to_the_artist_column`). Die offene
   Frage des Befunds ist damit beantwortet: nichts zu bauen.

## Tests

1. Die fünf gemessenen Paare als Tabellenfall in `dedupe_tests.rs`: jedes Paar
   ergibt genau eine Zeile, und zwar die mit der Anbieter-eigenen `ticket_url`.
   Für Amsterdam (beide auf `ticketmaster.nl`) gewinnt die erste.
2. Zwei **verschiedene** Künstler, gleicher Tag, gleiche Stadt, gleicher Ort →
   **zwei** Zeilen (Festival; heute rot).
3. Ein exakter und ein ähnlicher Künstler auf derselben Veranstaltung → **zwei**
   Zeilen, die zweite mit `is_similar = 1` und gesetztem `similar_to`.
4. Migration v76: eine Datenbank mit den fünf Paaren im Bestand verliert genau
   fünf Zeilen, `UNIQUE` hält danach, `user_version = 76`, und der Sieger ist je
   Paar der aus Test 1.
5. Derselbe Künstler, gleicher Tag, gleiche Stadt, **zwei Orte** → eine Zeile.
   Der Test hält den bewusst gewählten Verlust fest.

## Nachweis

1. Vor dem Umbau, in einer Kopie der Live-Datenbank: 413 Zeilen, fünf Gruppen.

   ```sql
   SELECT artist_name, date_key, city, COUNT(*) n
     FROM concert_events GROUP BY artist_key, date_key, city HAVING n > 1;
   ```

2. Nach dem Umbau (Migration allein, ohne Refresh): dieselbe Abfrage liefert
   **null** Gruppen, die Gesamtzahl liegt bei **408**.
3. Nach einem vollen Concerts-Refresh bleibt sie bei null Gruppen — der Fix
   wirkt nicht nur im Bestand, sondern auch im Zulauf.
4. Die Concerts-Ansicht mit eingeschalteter Venue-Spalte zeigt für jedes der
   fünf Paare genau eine Zeile.

## Parallelität

**Nicht teilbar.** Aufgaben 1–3 hängen alle am selben Schlüssel: die Migration
muss dieselbe Auflösungsregel aufrufen wie `merge()`, und beide hängen daran,
dass der Schlüssel bereits den Künstler enthält. Zwei Stränge würden dieselben
zwei Dateien (`dedupe.rs`, `pipeline.rs`) anfassen; ein disjunkter Dateischnitt
existiert nicht. Aufgabe 4 ist eine Streichung im selben `ON CONFLICT`-Block,
Aufgabe 5 ist leer.
