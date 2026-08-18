---
slug: radio-genre-chip-drops-the-country
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Der Genre-Chip bei Radio soll nicht aufs Land einschränken

**Design-Ansage des Nutzers, kein Plan.** Gemeldet am 16.08.2026: *„bei Radio
braucht es bei der Musikgenre die Einschränkung zum Land nicht. So viele Radios
gibt es auch nicht auf der Welt."* Belegt durch einen Screenshot des Dialogs
**Add Station** (laufender Build 0.1.13 = `dev`-Kopf `95b4b30016`).

## Ist-Zustand

Der Dialog zeigt drei Chips: **`Metalcore in CH`**, **`Top voted`**,
**`Near you`**. Der erste ist der Bibliotheks-Chip; er verbindet das
meistgehörte Genre mit dem Ländercode aus dem app-weiten Standort.

`crates/reprise-gnome/src/ui/radio/radio_chips.rs:63-81`
(`library_suggestion`): der Ländercode geht sowohl in die **Beschriftung**
(`radio_chip_genre_in_country`, `strings_radio.rs:83-89`) als auch in die
**Suche** (`SearchCriteria { tag, country_code }`).

Der Kommentar darüber (`:60-62`) hält die damalige Begründung fest — ohne
gespeichertes Land sucht der Chip bereits weltweit, *„a country-only chip would
just duplicate ‚Near you'"*. Der Nutzer dreht das weiter: das Land gehört
**gar nicht** an den Genre-Chip, denn „Near you" deckt den Ortsfall schon ab.

## Was zu tun ist

`library_suggestion` verliert den Standort-Parameter:

- Beschriftung = **nur der Genre-Name** (`Metalcore` statt `Metalcore in CH`)
- `SearchCriteria { tag: Some(genre.tag), country_code: None }` — weltweite
  Suche

Damit fällt `radio_chip_genre_in_country` (`strings_radio.rs:83-89`) ersatzlos
weg, samt der Übersetzungszeichenkette `{genre} in {country}`. Der
Aufrufer, der heute die `AppLocation` durchreicht, gibt sie nicht mehr weiter.

**Mitziehen:**

- Regel `RAD-5` in `docs/ux-rules.md` — sie beschreibt heute ausdrücklich den
  Chip „Metal in DE"; ohne Anpassung bleibt eine verwaiste Regel stehen
  (vgl. Memory *removing-behaviour-orphans-a-ux-rule*)
- Tests, die den zusammengesetzten Chip-Text festnageln (`radio_chips`-Tests
  und die Zeichenketten-Tests in `strings_radio.rs`)
- `po/` — eine Zeichenkette entfällt

## Offene Fragen

- Bleiben **drei** Chips oder wird daraus eine andere Aufteilung? Ohne
  Länderbezug lauten sie `Metalcore` · `Top voted` · `Near you` — das liest
  sich sauber, aber der erste Chip verliert seine Erklärung, warum gerade
  dieses Genre erscheint (er stammt aus der meistgehörten Musik).
- Bleiben **drei** Chips? Ohne Länderbezug lauten sie `Metalcore` ·
  `Top voted` · `Near you`.

## Mit aufgenommen: zwei Genre-Schreibweisen nebeneinander

Vom Nutzer am 16.08.2026 ausdrücklich als eigener Punkt bestätigt.

Im selben Bild stehen zwei Schreibweisen desselben Begriffs untereinander:

- Chip: **`Metalcore`** — großgeschrieben
- Genre-Spalte der Stationsliste: **`death metal`** — klein

Das sind **zwei verschiedene Quellen**, nicht eine inkonsistente:

| Ort | Feld | Herkunft |
| --- | --- | --- |
| Chip | `TopGenre.name` (`crates/reprise-core/src/library/taste.rs:24-32`) | die **eigene Bibliothek** — „the library's own spelling, in the variant it played most" |
| Spalte | `RadioSearchResult.genre` (`crates/reprise-core/src/radio/search.rs:43`, gefüllt `:188` aus `tags.first()`) | **radio-browser.info**, dessen Tags konventionell kleingeschrieben sind |

Der Chip nennt also, was *du* hörst, die Spalte, wie der Sender sich *dort*
verschlagwortet hat. Beide sind für sich korrekt — nebeneinander liest es sich
wie ein Formatierungsfehler.

Zu entscheiden (offen):

1. **Nichts tun.** Die Quellen sind verschieden und dürfen verschieden
   aussehen. Kostet nichts, sieht aber weiterhin uneinheitlich aus.
2. **Nur zur Anzeige vereinheitlichen.** Die Genre-Spalte in Title Case
   darstellen. Achtung: das ist reine Darstellung — der Suchbegriff gegen
   radio-browser muss unverändert klein bleiben, sonst finden die Chips
   nichts mehr.
3. **Beide klein anzeigen.** Die Schreibweise der Fremdquelle als Standard
   nehmen und den Chip ebenfalls kleinschreiben. Widerspricht der ausdrücklich
   dokumentierten Absicht in `taste.rs:24-27`, die eigene Schreibweise zu
   erhalten.

Empfehlung: **2**, mit einem Test, der belegt, dass die Suchanfrage die
Kleinschreibung behält.
