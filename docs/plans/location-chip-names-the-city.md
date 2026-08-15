---
slug: location-chip-names-the-city
worktree: /home/marvin/Projects/reprise-location-chip-names-the-city
branch: feature/location-chip-names-the-city
phase: refactored
codex_session:
created: 2026-08-15
---
# Der Standort-Chip nennt die Stadt

**Ziel.** Der aktive Filter-Chip der Concerts-Ansicht liest sich `Zürich · 500 km`
statt `Zürich, Bezirk Zürich, Zürich, Schweiz/Suisse/Svizzera/Svizra · 500 km`.
Die Stadt allein, in der Sprache der Oberfläche.

**Herkunft.** Vom Eigentümer am 15.08.2026 per Screenshot gemeldet, aufgenommen
in `docs/plans/feed-tags-mark-the-exception.HANDOFF.md` §3. Das ist **keine
Entwurfsfrage, sondern eine verletzte Regel**: CONC-2 (`docs/ux-rules.md`) sagt
seit jeher *„With location, the active chip reads `{city} · {radius} km`"*.

> **Lesestand.** Alle Aussagen unten sind gegen `origin/dev` @ `9fecc6d8f5`
> geprüft. Der geteilte Hauptcheckout steht auf einem fremden Stand und taugt
> nicht als Basis: lesen per `git show origin/dev:<pfad>`, arbeiten in einem
> eigenen Worktree. Wo eine Zeilennummer nicht mehr passt, gilt der genannte
> Bezeichner — nachziehen, nicht raten.

## 1. Was tatsächlich kaputt ist

Die Formatierung tut, was CONC-2 verlangt. Sie bekommt nur keine Stadt gereicht.

| Stelle | Datei | Was sie tut |
|---|---|---|
| formatiert | `strings_concerts.rs` · `concerts_location_radius` | setzt `{name} · {radius} km` — korrekt |
| ruft auf | `concerts_filter_bar.rs` · `chip_label` | reicht `location.name` durch |
| hält | `location.rs:38-43` · `AppLocation.name` | ein einziges `String`-Feld |
| füllt | `preference_location.rs:34-46` · `geocode_decision` | `name: location.display_name` ← **hier entsteht der Fehler** |
| liefert | `geocode.rs:41-48` · `parse_geocode` | nimmt Nominatims `display_name` roh |

`parse_geocode` liest das `address`-Objekt **bereits** — aber nur nach
`country_code` (`geocode.rs:49-55`). Die Stadt liegt in derselben Antwort und
wird weggeworfen. Die URL fragt `addressdetails=1` schon an
(`geocode.rs:18-22`). **Es braucht keinen zweiten Abruf.**

Es gibt genau **einen** Aufrufer von `geocode()`: `preference_location.rs:249`.

## 2. Warum es niemand gemerkt hat

`conc_2_location_chip_names_the_city_and_off_state_names_the_radius`
(`concerts_filter_bar_tests.rs:35`) füttert die Formatierung mit
`Some("Zürich")` — einem bereits kurzen Namen — und prüft `"Zürich · 500 km"`.

Der Test misst den **Formatierer**, nicht die **Kette**. Er ist grün und bleibt
grün, egal was in `location.name` steht. CONC-2 ist damit nur zur Hälfte
bewiesen: die Regel sagt „the chip reads `{city}`", der Test beweist „wenn man
eine Stadt hineingibt, kommt eine Stadt heraus". Ob jemals eine Stadt hineingeht,
fragt er nicht.

**Das ist der eigentliche Befund.** Ein Fix ohne Test auf der Geocode-Seite
reparierte das Symptom und ließe die Lücke stehen.

## 3. Was NICHT gebaut wird

Die Übergabe (§3, Punkt 3) verlangt für den Altbestand *„eine Migration oder ein
erneutes Geocodieren beim Lesen"*. **Das ist überholt.** `AGENTS.md:269-271`:

> Reprise has **not** shipped and there are **no existing installations**.
> Migrations, compatibility fallbacks, dual-write paths and deprecated-key
> readers are therefore *not* a design criterion anywhere in this repo.
>
> Where a clean data model and a backwards-compatible one collide, take the
> clean one and delete the old shape outright.

Kein `SCHEMA_V19`, kein Lese-Fallback, keine Heuristik auf dem alten String.
Der einzige Altbestand ist die Datenbank des Eigentümers — die genau den
Screenshot erzeugt hat. Sie heilt, indem der Standort einmal neu gesetzt wird:
Aufgabe 5, ein Abnahmeschritt, keine Codezeile.

Ebenfalls nicht gebaut: **kein Umbau an `http.rs`** (§4a), und der **Portal-Pfad
bleibt unangetastet** (Falle F-4).

## 4. Beschlüsse

Aus dem Grill vom 15.08.2026. Alle sieben sind entschieden; keiner wird in der
Umsetzung neu aufgemacht.

| # | Beschluss |
|---|---|
| B1 | **`AppLocation.name` *wird* die Stadt** — kein `location.full_name`, keine Koordinaten-Anzeige als Ersatz. Der Chip zeigt sie allein |
| B8 | **Die Einstellungen zeigen Stadt und Land**, nicht die Stadt allein und nicht den vollen Namen. Dafür kommt ein `country`-Feld dazu — siehe §4c |
| B2 | **Kette `city → town → village → municipality`**, danach erstes Komma-Segment. `suburb`/`neighbourhood`/`borough` kommen **nicht** vor |
| B3 | **Oberflächensprache**, nicht Systemgebietsschema — `active_gui_language()`, `_` → `-` |
| B4 | **Drei String-Tests plus Sichtabnahme** mit echtem Abruf, Screenshot vorher/nachher |
| B5 | **CONC-2 wird ergänzt**, nicht ersetzt und nicht durch eine zweite Regel verdoppelt |
| B6 | **`GeocodedLocation.display_name` wird durch `city` ersetzt**, nicht ergänzt — niemand läse es mehr, und `AGENTS.md:273-275` verlangt, die alte Form zu löschen |
| B7 | **Die Kürzung lebt in `parse_geocode` (reprise-core)**, nicht in `geocode_decision` (reprise-gnome) — nur so sind die Tests reine String-Tests ohne GTK-Kontext |

### B2 im Detail — warum die Stadt und nicht der Stadtteil

Für „Kreuzberg" liefert dieselbe Antwort `suburb: "Kreuzberg"` **und**
`city: "Berlin"`. Der Chip nennt `Berlin · 500 km`. Zwei Gründe: der Radius steht
direkt daneben, und bei 500 km ist der Stadtteil eine Genauigkeit, die der Zahl
widerspricht; und Konzerte werden nach Stadt beworben, nicht nach Kiez.

### B3 im Detail — die zwei Fallen der Sprachwahl

`active_gui_language()` (`i18n.rs:44-45`) liefert einen **gettext**-Namen, der
durch `po/LINGUAS` gefiltert ist — heute `ar bn de es fr hi zh_CN`, sonst `en`.

- **Unterstrich.** `zh_CN` ist kein gültiger Accept-Language-Wert; HTTP will
  `zh-CN`. Ohne die Umformung ist der Parameter **still wirkungslos** — Nominatim
  antwortet trotzdem, nur wieder ortsüblich. Siehe Falle F-1.
- **Der gettext-Filter ist hier das *richtige* Filter.** Läuft die Oberfläche auf
  Englisch, weil Reprise die Systemsprache nicht übersetzt, soll auch die Stadt
  englisch kommen. Die Anforderung sagt „in der Sprache der Oberfläche" — nicht
  „des Systems".
- **Ohne Sprache entfällt der Parameter**, statt auf `en` zu fallen.
  `active_gui_language()` ist ein `OnceLock` und liefert vor `i18n::init()`
  `None`. Das trifft nur Tests, und dort ist „ortsüblich" ehrlicher als ein
  erfundenes Englisch.

`geocode()` lebt in `reprise-core`, `active_gui_language()` in `reprise-gnome`.
Die Abhängigkeit läuft nur in eine Richtung: **der Aufrufer reicht die Sprache
hinein**, `reprise-core` liest keinen globalen Zustand.

## 4c. Stadt und Land in den Einstellungen (B8)

**Nachgetragen am 15.08.2026 auf Ansage des Eigentümers**, nachdem B1 bereits
stand. B8 setzt den Teil von B1 außer Kraft, der lautete „alle sechs Flächen
zeigen die Stadt allein" — die Chip-Entscheidung selbst bleibt unberührt.

Der Ländername liegt **in derselben Antwort**: `address.country` steht direkt
neben dem bereits gelesenen `address.country_code`
(`geocode.rs:49-55`) und trägt den Anzeigenamen, nicht den ISO-Code.

Der gemeldete Fehlerstring beweist das nebenbei: sein Schwanz
`Schweiz/Suisse/Svizzera/Svizra` **ist** der unlokalisierte `country`-Wert. Die
Sprachanforderung aus B3 repariert ihn also mit — aus demselben Abruf, ohne
Zusatzaufwand.

Daraus folgt die Datenform:

- `GeocodedLocation`: `city: String` (Pflicht, §5 garantiert sie) **plus**
  `country: Option<String>` (Anzeigename). `country_code` bleibt **unverändert**
  — es ist ISO und gehört `RAD-5`, nicht der Anzeige (Falle F-2).
- `AppLocation`: `name` (Stadt) plus `country: Option<String>`, persistiert unter
  einem neuen Settings-Schlüssel `location.country`. `store()` und `clear()`
  führen ihn mit; beide sind der einzige Schreib- bzw. Löschpfad
  (`location.rs:81-104`).

### Wer was zeigt

| Fläche | Datei | Zeigt |
|---|---|---|
| Concerts-Chip | `concerts_filter_bar.rs:334` | **Stadt** — CONC-2, unverändert |
| Filter-`city`-Feld | `concerts_view.rs:444` | **Stadt** — speist den Chip |
| Einstellungen: Untertitel | `preference_location.rs:205` | **Stadt, Land** |
| Einstellungen: Startwert Suchfeld | `preference_location.rs:242` | **Stadt, Land** — dieselbe Zeichenkette, die darüber steht, und eine bessere Neusuche als die Stadt allein |
| Einstellungen: Referenz-String | `preference_concerts.rs:26` | **Stadt, Land** — liegt auf derselben Seite |
| MCP-Katalog | `catalog_resources.rs:126` | **Stadt** — `country_code` steht dort ohnehin als eigenes Feld |

Fehlt das Land (Portal-Pfad, Falle F-4), zeigen alle Flächen schlicht den Namen
ohne Anhang — kein Komma, kein Platzhalter. Die Zusammensetzung
`{Stadt}, {Land}` gehört in **eine** Hilfsfunktion neben
`concerts_location_radius` in `strings_concerts.rs`, nicht dreimal an die
Aufrufstellen kopiert.

## 4a. `accept-language` ist ein Query-Parameter — nachgeschlagen

Nominatims Doku für `/search`: `accept-language` wird **sowohl als
Query-Parameter als auch als HTTP-Header** akzeptiert, Format ist der
Accept-Language-Header-Aufbau.

Der Query-Weg ist hier der richtige: `http::get(url)` (`http.rs:42-72`) nimmt
**keine** Header entgegen, und ihn dafür umzubauen zöge jeden anderen Anbieter
mit hinein. `geocode_url` hängt den Parameter an — eine Datei, kein
Transport-Umbau.

## 5. Die letzte Instanz der Kette

Dieselbe Doku sagt ausdrücklich **keine** Rangfolge und **keine** Garantie für
die Schlüssel des `address`-Objekts zu. Belegt sind unter anderem `city`, `town`,
`village`, `hamlet`, `borough`, `suburb`, `neighbourhood` — welche erscheinen,
hängt vom Land ab.

Die Kette braucht deshalb eine **letzte Instanz, die nie leer ist**: das erste
Komma-Segment des (jetzt lokalisierten) `display_name`. Für den gemeldeten Fall
ist das `Zürich` — dieselbe Antwort wie über `address.city`, aber sie hält auch
dort, wo das `address`-Objekt nichts Passendes trägt.

`display_name` bleibt also **Eingabe** von `parse_geocode` und verschwindet nur
aus der **Ausgabe** `GeocodedLocation` (B6).

## 6. Aufgaben

> Reihenfolge ist bindend: 1 vor 2 (der Test braucht die Feldwahl), 3 nach 1
> (die Signatur muss stehen), 6 zuletzt.

**Aufgabe 1 — Stadt und Land aus der Antwort ziehen.** Ein Commit.
`geocode.rs`: `GeocodedLocation.display_name` wird zu `city` (B6), dazu kommt
`country: Option<String>` aus `address.country` (B8, §4c). `country_code` bleibt
unverändert. `parse_geocode` wählt die Stadt nach der Kette aus B2, letzte Instanz
erstes Komma-Segment von `display_name` (§5). `geocode_url` und `geocode` nehmen
die Sprache als Parameter und hängen `accept-language` an; `None` lässt den
Parameter weg (B3). Ein leeres oder nur aus Leerraum bestehendes Feld zählt als
nicht vorhanden — `parse_geocode` filtert `country_code` bereits genau so
(`geocode.rs:52-54`), dieselbe Behandlung gilt für Stadt und Land.

**Aufgabe 1b — Das Land durchreichen und persistieren.** Ein Commit.
`location.rs`: `AppLocation` bekommt `country: Option<String>`, Settings-Schlüssel
`location.country`; `store()` und `clear()` führen ihn mit (`location.rs:81-104`).
`strings_concerts.rs` bekommt **eine** Hilfsfunktion, die `{Stadt}, {Land}`
zusammensetzt und bei fehlendem Land den Namen unverändert zurückgibt.

**Aufgabe 2 — Die Tests, die die Kette messen.** Ein Commit.
In `domain_tests.rs`, im Stil der dortigen `parse_geocode`-Tests
(`domain_tests.rs:61-91`: reine String-Eingabe, kein HTTP, kein Ratelimit):
- eine echte Nominatim-Antwort für Zürich mit vollem `display_name`, `address.city`
  **und** `address.country` → erwartet `Zürich` als Stadt und `Schweiz` als Land;
- eine Antwort ohne jeden Stadt-Schlüssel im `address` → erwartet das erste
  Komma-Segment als Stadt und `None` als Land, wenn auch `country` fehlt;
- `geocode_url` trägt `accept-language=de` für `de` — und **`zh-CN`, nicht
  `zh_CN`**, für `zh_CN` (Falle F-1). Ohne Sprache trägt die URL den Parameter
  gar nicht.

Dazu ein vierter, für die Zusammensetzung aus Aufgabe 1b: `{Stadt}, {Land}` mit
Land, und **der Name allein ohne angehängtes Komma**, wenn das Land fehlt. Genau
dort entsteht sonst das `Zürich, ` mit hängendem Trennzeichen.

Diese Tests sind der Beweis, den CONC-2 bisher nicht hat.

**Aufgabe 3 — Den Aufrufer verkabeln und die Flächen bedienen.** Ein Commit.
`preference_location.rs`: `geocode_decision` nimmt `city` und `country` statt
`display_name`; der Aufruf bei `:249` reicht `active_gui_language()` durch,
umgeformt nach B3. Untertitel (`:205`) und Startwert des Suchfelds (`:242`)
zeigen `{Stadt}, {Land}` über die Hilfsfunktion aus Aufgabe 1b, ebenso der
Referenz-String in `preference_concerts.rs:26`. Chip, Filter-`city`-Feld und
MCP-Katalog bleiben bei der Stadt allein (Tabelle in §4c).
`portal_decision` bleibt **unverändert** (Falle F-4) — es setzt weiterhin nur
den übersetzten Text und **kein** Land.

**Aufgabe 4 — Zwei Regeln nachziehen.** Ein Commit, zusammen mit nichts anderem.

`docs/ux-rules.md`, **CONC-2**: Der Regeltext bleibt **wörtlich stehen**. Angehängt
werden zwei Sätze: dass die Stadt aus Nominatims `address`-Objekt stammt (Kette
aus B2, sonst erstes Segment von `display_name`) und in der Oberflächensprache
erbeten wird — mit den Namen der Tests aus Aufgabe 2. **Keine zweite Regel** (B5).

`docs/ux-rules.md:1210`, **SET-15**: zwei Änderungen, beide zwingend.
1. Die Aufzählung *„Clearing Location removes only latitude, longitude, name, and
   country code"* wird um den neuen Ländernamen ergänzt. Ohne das ist die Regel
   nach Aufgabe 1b **falsch** — sie zählt die Schlüssel abschließend auf.
2. SET-15 besitzt die Einstellungsseite und muss deshalb sagen, dass City dort
   als `{Stadt}, {Land}` erscheint und ohne Land als Stadt allein (B8, §4c).

Der bestehende Test `clear_removes_the_full_location_including_the_country_code`
(`location.rs:152`) muss den neuen Schlüssel mit abdecken — das ist **keine**
Aufweichung von Falle F-3, sondern derselbe Test über einen echt hinzugekommenen
Schlüssel.

**Aufgabe 5 — Abnahme am eigenen Bestand.** Kein Code, kein Commit.
Der gespeicherte Standort des Eigentümers trägt weiter den langen Namen (§3).
Ablauf: Screenshot des Ist-Chips → Einstellungen → Stadt neu suchen → Screenshot
des neuen Chips. Das ist die einzige Stufe, die einen echten Nominatim-Abruf
einschließt; die Tests aus Aufgabe 2 fahren bewusst auf Strings.

**Aufgabe 6 — Der Gate-Lauf.**
Die Gates aus `check-merge-readiness`. Bekannte rote dev-Gates sind als solche zu
benennen — mit dem Nachweis, dass sie auf `origin/dev` ebenso rot sind — nicht
stillschweigend zu übergehen.

## 7. Fallen

- **F-1: Der Unterstrich.** `zh_CN` als `accept-language` ist still wirkungslos:
  die Antwort kommt trotzdem, nur unlokalisiert. Ein Test, der bloß „Parameter
  vorhanden" prüft, fängt das nicht. Aufgabe 2 prüft deshalb den **Wert**.
- **F-2: `country_code` darf sich nicht ändern.** `RAD-5` filtert radio-browser
  danach; der Code ist ISO und von `accept-language` unberührt. Die bestehenden
  Tests in `location.rs:224-243` und `domain_tests.rs:80` müssen grün bleiben,
  **ohne angefasst zu werden**. Werden sie rot, ist der Fix falsch — nicht der
  Test.
- **F-3: Die bestehenden `location.rs`-Tests speichern `"Berlin, Deutschland"`.**
  Sie reichen den Namen direkt in `store()` und prüfen den Rundlauf. Sie bleiben
  grün und sind **kein** Gegenbeweis. Nicht „anpassen", nur weil ein langer Name
  darin vorkommt.
- **F-4: Der Portal-Pfad hat keine Stadt.** „Use current location" liefert nur
  Koordinaten, `O-4` verbietet einen Reverse-Geocode. `name` bleibt dort der
  übersetzte Text „Current location". Wer die Stadt zur Pflicht macht, bricht ihn.
- **F-5: Nominatims Ratelimit.** Ein Test, der wirklich abfragt, ist unhöflich
  und flaky. Aufgabe 2 arbeitet ausschließlich auf Strings — `parse_geocode` ist
  genau dafür öffentlich.
- **F-6: Der bestehende CONC-2-Test bleibt unverändert.** Er misst den
  Formatierer korrekt; er war nie falsch, nur unvollständig. Die drei neuen Tests
  treten **daneben**, nicht an seine Stelle.
- **F-7: Fünf weitere Flächen lesen `AppLocation.name`** — `concerts_view.rs:444`,
  `preference_concerts.rs:26`, `preference_location.rs:205` und `:242`,
  `catalog_resources.rs:126`. Sie zerfallen nach B8 in zwei Gruppen; welche wohin
  gehört, steht in der Tabelle in §4c. **Keine darf beim Umbau vergessen werden**,
  und keine darf `{Stadt}, {Land}` selbst zusammenbauen — dafür gibt es die eine
  Hilfsfunktion aus Aufgabe 1b.
- **F-8: Das hängende Komma.** Fehlt das Land — Portal-Pfad, oder eine
  Nominatim-Antwort ohne `address.country` — darf nirgends `Zürich, ` mit
  Trennzeichen am Ende stehen. Aufgabe 2 prüft genau diesen Fall.
- **F-9: `location.country` ist ein neuer Schlüssel, kein umbenannter.**
  `location.country_code` bleibt daneben bestehen und gehört weiter `RAD-5`. Wer
  die beiden zusammenlegt, bricht den Radio-Filter (Falle F-2).

## 8. Parallelität

**Kein Schnitt.** Aufgaben 1–3 fassen dieselbe Kette an: `geocode.rs` bestimmt
das Feld, `domain_tests.rs` prüft es, `preference_location.rs` reicht es weiter —
und die Signatur von `geocode()` ändert sich in allen dreien. Ein zweiter Strang
hätte keine eigene Datei und müsste auf die Signatur warten.

Aufgabe 4 (`docs/ux-rules.md`) wäre dateilich disjunkt, ist aber zwei Sätze; ein
eigener Worktree kostete mehr als er einspart, und die Hausordnung verlangt Regel
und Beweis ohnehin im Zusammenhang.

Ein Strang, sechs Aufgaben.
