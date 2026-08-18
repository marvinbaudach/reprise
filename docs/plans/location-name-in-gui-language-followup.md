---
slug: location-name-in-gui-language-followup
worktree:
branch:
phase: shipped
codex_session:
created: 2026-08-16
---
# TODO: Standort soll nur Stadt und Land in der Oberflächensprache nennen — ist bereits gelandet, nachweisen

**Befund plus Abgleich mit dem Bestand.** Gemeldet am 16.08.2026, 08:09/08:10:

> „Location brauchts nicht in mehreren Sprachen. einfach Stadt und Land in der
> GUI-Sprache" — und zum zweiten Screenshot: „auch hier".

Betroffen sind zwei Oberflächen, beide mit demselben Text:

1. **Preferences → Location**, Karte *City*:
   `Zürich, Bezirk Zürich, Zürich, Schweiz/Suisse/Svizzera/Svizra` (vierzeilig)
2. **Preferences → Plugins → Concerts** (aufgeklappt):
   `Location · Zürich, Bezirk Zürich, Zürich, Schweiz/Suisse/Svizzera/Svizra,
   within 1000 km`

## Das ist bereits behoben — auf `origin/dev`

`docs/plans/location-chip-names-the-city.md` (`phase: refactored`) hat genau
diese Anforderung entschieden, und **PR #515 ist gemergt**: *„The location chip
names the city, and the settings name the country with it"*. Nachgelesen auf
`origin/dev`:

- `crates/reprise-core/src/concerts/geocode.rs:55` — Kette
  `city → town → village → municipality`, `GeocodedLocation.city` statt
  `display_name` (`:9`, `:75`)
- `crates/reprise-gnome/src/ui/preferences/preference_location.rs:209` —
  `concerts_location_name(&location.name, location.country.as_deref())`, also
  **Stadt und Land** in den Einstellungen; `country`/`country_code` als eigene
  Felder (`:30-31`, `:44-45`)
- `crates/reprise-gnome/src/ui/strings_location.rs:49` —
  `"Location · {name}, within {radius} km"` für die Concerts-Zeile; mit `name`
  = Stadt liest sie sich künftig `Location · Zürich, within 1000 km`

Die Sprachwahl ist ebenfalls entschieden (Beschluss B3 des Plans):
`active_gui_language()`, `_` → `-` für `Accept-Language` — also
**Oberflächensprache**, nicht Systemgebietsschema. Genau das, was der Nutzer
verlangt.

## Warum der Screenshot es trotzdem zeigt — zwei mögliche Gründe

Beide sind **nicht** geprüft; sie schließen sich nicht aus:

1. **Der laufende Build ist älter als der Merge.** `origin/dev` steht auf
   `0.1.13` (`95b4b30016`), und die Version steigt bei jedem Merge nach `dev`.
   Welche Version die laufende App trägt, wurde nicht erhoben — das ist der
   erste Schritt.
2. **Der gespeicherte Wert stammt von vorher.** Der Plan hat bewusst **keine**
   Migration gebaut (`AGENTS.md`: keine Bestandsinstallationen, kein
   Kompatibilitätspfad). Der lange Name steht als `AppLocation.name` in der
   Datenbank und bleibt dort, bis der Standort **einmal neu gesetzt** wird —
   das war als Abnahmeschritt vorgesehen, nicht als Codezeile.

## Erledigt am 16.08.2026, 08:2x — Grund 2 war es

Der laufende Build ist **0.1.13**, gebaut am 15.08.2026 23:00, also der
`dev`-Kopf `95b4b30016` (22:56) — er enthielt den Fix bereits. Nach einmaligem
Neusetzen des Standorts liest die Karte **„Zurich, Switzerland"**
(Screenshot des Nutzers). Grund 1 (zu alter Build) ist damit ausgeschlossen,
Grund 2 (alter gespeicherter Wert, keine Migration) bestätigt.

Offen bleibt nur der letzte Abschnitt dieses Dokuments: die Concerts-Zeile
ohne Land.

## Nächste Schritte (erledigt, als Protokoll)

1. Version der laufenden App feststellen und mit `origin/dev` abgleichen.
2. Ist der Build neu genug: in Preferences → Location einmal **Use current
   location** drücken (oder den Ort über den Stift neu setzen) und beide
   Oberflächen erneut ansehen.
3. Steht danach immer noch der lange Name, ist es ein echter Fehler — dann
   greift die Kette aus `parse_geocode` nicht, und dieser TODO wird zu einem
   Befund mit eigener Diagnose.

## Bestätigt am 16.08.2026, 08:22: Filter zeigen nur die Stadt

Der Concerts-Filterchip liest sich **`Zurich · 500 km`** (Screenshot des
Nutzers), Kommentar: *„das ist jetzt korrekt. nur Stadt bei den Filtern"*.
CONC-2 ist damit erfüllt und die Frage „Land auch im Filter?" ist **beantwortet
mit nein**. Die Fußzeile derselben Ansicht nennt die Stadt ebenso
(„410 concerts hidden by the 500 km radius around Zurich").

## Was auch nach dem Fix offen bleibt

Es bleibt genau **eine** Stelle offen: die Zusammenfassungszeile im
Plugins-Bereich (Preferences → Plugins → Concerts, aufgeklappt), die laut
`strings_location.rs:49` nur `{name}` trägt — also `Location · Zurich, within
1000 km`, ohne Land. Sie ist kein Filter, sondern eine Einstellungs-Anzeige,
und dort lautet die Regel des Nutzers „Stadt und Land". Ob diese eine Zeile
nachziehen soll, ist die letzte offene Frage — eine kleine Ergänzung, kein
neuer Mechanismus.
