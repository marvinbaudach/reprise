# New Releases: Abdeckung für Artists mit Single-Titeln

**Datum:** 2026-07-24
**Status:** Design, freigegeben
**Betrifft:** `reprise-core/src/artist_news*.rs`, `reprise-gnome/src/ui/new_releases/`

## Problem

Ein Artist, von dem die Bibliothek nur eine Single enthält, wird über neue Alben
praktisch nie informiert. Der Auslöser ist der Vorab-Single-Fall: Wer die Lead-Single
eines angekündigten Albums besitzt, will genau über dieses Album benachrichtigt werden.
Heute passiert das nicht.

Die Messung an einer realen Bibliothek (167 Artists, 349 Alben, davon 163 mit genau
einem Track) zeigt vier zusammenwirkende Ursachen.

### U1 — Die Rangfolge bestraft Singles

`artist_news::artists_for_fetch` sortiert Kandidaten nach `SUM(play_count) DESC`.
Ein Artist mit einem einzigen Track hat wenig Plays und landet im Schwanz der Liste.

- Scope `TopArtists` (`TOP_ARTIST_COUNT = 20`): der Artist wird **nie** abgefragt.
- Scope `AllArtists`: nur `DAILY_REST_COUNT = 5` Artists pro Tag aus dem Rest, über
  eine `day_index`-Modulo-Rotation. Ein voller Durchlauf über 145 Rest-Artists dauert
  rund 30 Tage. Läuft die App an einem Tag nicht, fallen dessen 5 Artists aus.

### U2 — Ergebnislose Artists verbrauchen das Budget bei jedem Lauf neu

`artist_cache_is_fresh` (`artist_news.rs:362`) prüft die Frische über
`MAX(fetched_at) FROM new_releases WHERE artist_mbid = ?`. Ein Artist ohne gefundene
Releases erzeugt keine Zeile, hat also nie einen Cache-Eintrag, gilt bei jedem Lauf
als veraltet und wird alle 6 Stunden komplett neu abgefragt. Der 7-Tage-TTL schützt
nur Artists, die zufällig gerade News haben.

Dasselbe Muster in `artist_news_refresh::latest_fetched_at` (`artist_news_refresh.rs:54`):
Ist `new_releases` leer, liefert es dauerhaft `NULL`, und `refresh_due` schlägt bei
jedem Timer-Tick an.

In der Messbibliothek haben nur **28 von 165** gültigen Kandidaten überhaupt eine
aufgelöste MusicBrainz-MBID — die restlichen 137 kosten pro Rotationsslot einen
zusätzlichen Request für die Artist-Suche.

### U3 — Ein einziger Track lässt das ganze Album als besessen gelten

`local_albums` (`artist_news.rs:376`) sammelt alle Album-Tags eines Artists.
`parse_release_group` (Zeile 673) verwirft jede Release-Group, deren normalisierter
Titel darin vorkommt. Wie viele Tracks des Albums tatsächlich vorhanden sind, wird
nicht geprüft.

In der Messbibliothek betrifft das **23 Ein-Track-Einträge**, deren Album-Tag nach
einem echten Albumnamen aussieht (nicht `- Single`, nicht `[EP]`, nicht identisch mit
dem Tracktitel). Beispiel: `As I Lay Dying – "A Greater Foundation"`, getaggt mit dem
Album `Awakened` — damit gilt das komplette Album als vorhanden.

Der Vorab-Single-Fall ist die schädlichste Ausprägung: Eine Lead-Single wird
typischerweise mit dem Namen des kommenden Albums getaggt. Dann unterdrückt
ausgerechnet diese Single die Meldung über das Album, auf das gewartet wird.

### U4 — Erschienene Singles werden grundsätzlich verworfen

`parse_release_group` (Zeile 679) lässt `primary-type == "single"` nur durch, wenn das
Release in der Zukunft liegt **und** ein taggenaues Datum hat. Bereits erschienene
Singles tauchen nie auf.

## Lösung

Vier Bausteine. A und B beheben die Auswahl der Artists, D den Bibliotheks-Filter,
E öffnet Singles als Release-Typ.

### A — Fetch-Ledger

Neue Tabelle `artist_news_fetch`, die pro Artist festhält, wann zuletzt ein Versuch
stattfand — unabhängig vom Ergebnis.

| Spalte | Typ | Zweck |
|---|---|---|
| `artist_key` | `TEXT PRIMARY KEY` | `lower(trim(artist))`, dieselbe Gruppierung wie `artists_for_fetch` |
| `artist_mbid` | `TEXT` nullable | gecachte Auflösung |
| `last_attempt_at` | `INTEGER NOT NULL` | Grundlage der Frischeprüfung |
| `last_outcome` | `TEXT NOT NULL` | `ok` \| `unmatched` \| `failed` |
| `releases_found` | `INTEGER NOT NULL DEFAULT 0` | trennt "geprüft, nichts gefunden" von "nie geprüft" |

Der Schlüssel ist bewusst der Name und nicht die MBID: Artists ohne aufgelöste MBID
sind genau die, die mitgezählt werden müssen. Dadurch kann die Frischeprüfung **vor**
die MBID-Auflösung gezogen werden — ein frischer Artist kostet dann null Requests
statt einem.

Änderungen:

- `artist_cache_is_fresh` liest `last_attempt_at` aus dem Ledger statt `fetched_at`
  aus `new_releases`. Vergleich weiterhin gegen `FETCH_TTL_SECONDS` (7 Tage).
- Die Prüfung wandert in `refresh_with` vor `resolve_artist_mbid`.
- `refresh_with` schreibt in **jedem** Zweig einen Ledger-Eintrag: nach erfolglosem
  MBID-Match (`unmatched`), nach fehlgeschlagenem oder ungültigem Fetch (`failed`),
  nach erfolgreichem Upsert (`ok` mit `releases_found`).
- `latest_fetched_at` liest `MAX(last_attempt_at)` aus dem Ledger.

### B — Rotation nach "am längsten nicht geprüft"

`artists_for_fetch` behält die Top-20 nach Play-Count als bevorzugte Gruppe. Der Rest
wird nicht mehr über `day_index`-Modulo geschnitten, sondern nach
`last_attempt_at ASC` sortiert, nie geprüfte zuerst (`NULLS FIRST`).

- `FetchScope::AllArtists { day_index }` wird parameterlos zu `FetchScope::AllArtists`.
- `configured_fetch_scope` braucht kein `today` mehr; die Signatur verliert den
  Parameter. Aufrufer: `preference_new_releases.rs:14`.
- `DAILY_REST_COUNT` wird zu `REST_ARTISTS_PER_RUN = 30` umbenannt und angehoben, weil
  die Zahl pro Lauf gilt und nicht pro Tag. Das Limit gilt **nur für die Rest-Gruppe**;
  die Top-20 kommen unverändert zusätzlich dazu.
- Die Ledger-Frischeprüfung aus A gilt für **beide** Gruppen. Auch ein Top-20-Artist
  wird innerhalb von `FETCH_TTL_SECONDS` nicht erneut abgefragt — genau das behebt U2.

Fällt ein Lauf aus, geht nichts verloren — die Übersprungenen sind beim nächsten Lauf
schlicht die ältesten.

Der höhere Wert ist unkritisch, weil die 7-Tage-Frische im Ledger das System selbst
begrenzt: Nach dem ersten Durchgang findet ein Lauf keine veralteten Artists mehr und
macht nichts. MusicBrainz ist auf 1 Request/Sekunde gedrosselt
(`musicbrainz.rs:14, MIN_REQUEST_INTERVAL`); die Erstbefüllung von 165 Artists
entspricht rund 330 Requests bzw. etwa 5,5 Minuten, verteilt über ein bis zwei Tage.

### D — Besitz messen statt Titel vergleichen

Zwei Regeln, nach Release-Datum getrennt:

**Angekündigte Releases** (`first_release_date >= heute`, entspricht `NewsKind::Upcoming`):
Der Bibliotheks-Filter wird **gar nicht** angewandt. Ein noch nicht erschienenes Album
kann nicht besessen werden; ein Titel-Match ist dort per Definition ein falsch
getaggter Vorab-Track. Das ist ein exaktes Kriterium, keine Heuristik, und deckt den
Vorab-Single-Fall vollständig ab — auch dann, wenn mehrere Tracks (Single plus
B-Seite) mit dem Albumnamen getaggt sind.

**Bereits erschienene Releases:** `local_albums` zählt ein Album erst ab
`OWNED_ALBUM_MIN_TRACKS = 2` vorhandenen Tracks als besessen. Alles darunter passiert
den Filter.

Der Restfall bleibt bewusst heuristisch: Ein vor drei Wochen erschienenes Album, von
dem zwei mit Albumnamen getaggte Tracks vorliegen, gilt als vorhanden. Ab zwei Tracks
ist "das Album ist da" die wahrscheinlichere Erklärung.

`local_album_set` (Zeile 442), das `query_releases` für die Präsenz-Anzeige benutzt,
behält seine Schwellenlosigkeit — es filtert nicht, es beschreibt. Es liefert künftig
`(Artist, Album) → Trackzahl` statt einer blanken Menge; die Schwelle wird erst bei der
Abbildung auf `LibraryPresence` in D2 angewandt.

Reihenfolge in `parse_release_group`: Der `NewsKind` muss künftig **vor** dem
Bibliotheks-Abgleich bestimmt werden, weil die Datumsregel entscheidet, ob der Abgleich
überhaupt stattfindet. Heute läuft es umgekehrt.

### D2 — Teilbesitz sichtbar machen

Folge aus D: Alben mit einem vorhandenen Track erscheinen jetzt und würden vom
heutigen `in_library`-Bool als "In Bibliothek" beschriftet, mit der Primäraktion
"In Bibliothek zeigen". Das ist genau die falsche Botschaft für den Zielfall.

`StoredRelease.in_library: bool` wird zu `presence: LibraryPresence`:

| Zustand | Bedingung | Chip | Primäraktion |
|---|---|---|---|
| `Absent` | kein Track vorhanden | "Erschienen" | Ankündigung öffnen |
| `Partial` | 1 Track vorhanden | "Single vorhanden" | Ankündigung öffnen |
| `Complete` | ≥ `OWNED_ALBUM_MIN_TRACKS` Tracks | "In Bibliothek" | In Bibliothek zeigen |

`Complete` bleibt erreichbar: Die Präsenz wird zur Abfragezeit in `query_releases`
berechnet, nicht beim Fetch. Wird das Album später angeschafft, kippt der Zustand beim
nächsten Durchlauf.

Betroffene Stellen: `StoredRelease`, `local_album_set` (liefert Trackzahlen statt einer
Menge), `query_releases`, `release_row::chip_presentation`,
`release_row::primary_action`, `history_page::history_action`.

Die bestehende Vorrangregel bleibt unangetastet: `chip_presentation` prüft `Upcoming`
vor der Präsenz, `primary_action` schickt bei Upcoming zur Ankündigung.

### E — Singles-Schalter

Neues Setting `module.new_releases.include_singles`, Standard `false`, analog zu
`module.new_releases.all_artists` über `library::settings::{get_bool, set_bool}`.

- UI: `adw::SwitchRow` unter der bestehenden Scope-Zeile in
  `preference_new_releases.rs`.
- `parse_release_groups` und `parse_release_group` bekommen den Schalter durchgereicht;
  `refresh_with` liest ihn einmal pro Lauf.
- Ist er an, durchlaufen erschienene Singles dieselbe `NEWS_WINDOW_DAYS`-Regel (90 Tage)
  wie Alben und EPs.
- Angekündigte Singles behalten ihre heutige Sonderregel (taggenaues Datum
  erforderlich) und bleiben **unabhängig vom Schalter immer aktiv** — sonst wäre bei
  ausgeschaltetem Schalter weniger sichtbar als heute.

## Migration

Neues Modul `db_artist_news_fetch.rs` nach dem Muster von `db_new_releases_history.rs`,
`user_version` 29 → **30**.

```sql
CREATE TABLE artist_news_fetch (
  artist_key      TEXT PRIMARY KEY,
  artist_mbid     TEXT,
  last_attempt_at INTEGER NOT NULL,
  last_outcome    TEXT NOT NULL,
  releases_found  INTEGER NOT NULL DEFAULT 0
);
```

Vorbefüllung aus vorhandenen Daten, damit nach dem Update nicht die komplette
Bibliothek auf einen Schlag neu gefetcht wird:

```sql
INSERT OR IGNORE INTO artist_news_fetch
  (artist_key, artist_mbid, last_attempt_at, last_outcome, releases_found)
SELECT lower(trim(artist_name)), artist_mbid, MAX(fetched_at), 'ok', COUNT(*)
FROM new_releases
GROUP BY lower(trim(artist_name));
```

Artists ohne Zeile in `new_releases` bekommen bewusst keinen Eintrag: Sie gelten als
"nie geprüft" und kommen durch B sofort als Erste an die Reihe. Genau das ist das
gewünschte Verhalten.

## Fehlerbehandlung

- Netzwerk- und Parse-Fehler verhalten sich unverändert (`report.failed`, weiter mit
  dem nächsten Artist), erzeugen jetzt aber einen Ledger-Eintrag mit
  `last_outcome = 'failed'`. Damit blockiert ein dauerhaft fehlschlagender Artist nicht
  mehr die Rotation, indem er bei jedem Lauf erneut vorn steht.
- Ein `failed`-Eintrag unterliegt demselben `FETCH_TTL_SECONDS`, wird also nach 7 Tagen
  erneut versucht. Keine gesonderte Backoff-Logik — YAGNI, solange kein realer Fall
  dagegen spricht.
- Ledger-Schreibfehler werden wie bisher über `NewsError::Database` gemeldet und
  brechen den Lauf ab; ein halb geschriebener Ledger wäre schlimmer als ein
  abgebrochener Lauf.

## Tests

Nach dem Muster von `artist_news_tests.rs` und den `db_*_migration_tests.rs`.

**Ledger (A)**
- Ein Artist ohne Fundstellen gilt nach dem Lauf als frisch und wird beim zweiten Lauf
  übersprungen — Regressionstest für U2.
- `unmatched` und `failed` erzeugen ebenfalls einen Eintrag.
- `latest_fetched_at` liefert bei leerem `new_releases`, aber gefülltem Ledger einen
  Wert.

**Rotation (B)**
- Nie geprüfte Artists kommen vor lange geprüften, unabhängig vom Play-Count.
- Die Top-20-Gruppe bleibt bevorzugt.
- Ein ausgefallener Lauf führt zu keiner Lücke.

**Besitz (D)**
- Ein angekündigtes Album passiert den Filter auch bei Titel-Match mit zwei lokalen
  Tracks — der Zielfall.
- Ein erschienenes Album mit einem lokalen Track passiert den Filter.
- Ein erschienenes Album mit zwei lokalen Tracks wird gefiltert.

**Präsenz (D2)**
- `Absent`, `Partial`, `Complete` werden korrekt aus den Trackzahlen abgeleitet.
- `Upcoming` behält Vorrang vor der Präsenz in `chip_presentation` und
  `primary_action`.

**Singles (E)**
- Schalter aus: erschienene Single wird verworfen.
- Schalter an: erschienene Single innerhalb von 90 Tagen kommt durch.
- Angekündigte Single mit taggenauem Datum kommt in beiden Fällen durch.

**Migration**
- Vorbefüllung aus `new_releases` erzeugt keine sofortige Komplett-Neuabfrage.
- Wiederholter Aufruf ist idempotent.

## Bewusst nicht im Umfang

- **Der ✦-Badge bleibt wie er ist.** Ausdrücklicher Wunsch; keine Desktop-Notification,
  kein Toast, keine eigene Ansicht.
- **`has_excluded_secondary_type`** bleibt unverändert. Compilations, Live, Remix,
  Soundtrack, Mixtape und DJ-Mix bleiben draußen — bei Einträgen wie
  `PVRIS – Punk Goes Pop, Vol. 6` ist das richtig.
- **`NEWS_WINDOW_DAYS` (90 Tage) und `MAX_ITEMS` (20)** bleiben unverändert.
- **Kein Backoff für dauerhaft fehlschlagende Artists** über den TTL hinaus.
