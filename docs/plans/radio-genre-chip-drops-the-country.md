---
slug: radio-genre-chip-drops-the-country
worktree: /home/marvin/Projects/reprise-radio-genre-chip-drops-the-country
branch: feature/radio-genre-chip-drops-the-country
phase: planned
codex_session:
created: 2026-08-20
---
# Der Bibliotheks-Chip sucht weltweit

Der Genre-Chip im Dialog **Add Station** schränkt die Suche heute auf das
gespeicherte Land ein. Der Nutzer hat das am 16.08.2026 verworfen: *„bei Radio
braucht es bei der Musikgenre die Einschränkung zum Land nicht. So viele Radios
gibt es auch nicht auf der Welt."* „Near you" deckt den Ortsfall bereits ab.

**Der Kern ist der Filter, nicht die Beschriftung.** Das Land fällt aus
`SearchCriteria`; dass es damit auch aus dem Label verschwindet, ist Folge, nicht
Ziel.

**Der Plan ist im Grill bewusst klein geschnitten worden.** Es geht um genau
eine Sache: das Land fällt aus dem Filter. Kein Title Case, kein neuer Helfer,
keine Grenzfall-Matrix, keine Chip-Umgestaltung. Was zusätzlich auftauchte,
steht unter „Nicht in diesem Plan" und ist dort *entschieden*, nicht vertagt.

**Alle Zeilenangaben gegen `51e9c6c9bb`.** Basis dieses Worktrees ist
`origin/dev` = `40655644fc`; die zwei Commits dazwischen (#583, #584) fassen nur
`.github/workflows/ci.yml` und `scripts/tests/cua-*.sh` an, keine einzige
Quelldatei dieses Plans. Die Zeilennummern gelten also unverändert — trotzdem
vor dem Ändern kurz gegenlesen.

## Was gilt

`crates/reprise-gnome/src/ui/radio/radio_chips.rs:65-81`:

```rust
pub(super) fn library_suggestion(
    genre: Option<TopGenre>,
    location: Option<&AppLocation>,
) -> Option<LibrarySuggestion> {
    let genre = genre?;
    let country_code = location.and_then(|location| location.country_code.clone());
    let label = match country_code.as_deref() {
        Some(country) => strings::radio_chip_genre_in_country(&genre.name, country),
        None => genre.name.clone(),
    };
    Some(LibrarySuggestion {
        label,
        criteria: SearchCriteria { tag: Some(genre.tag), country_code },
    })
}
```

Der Ländercode fließt **doppelt** ein: in die Beschriftung und in
`SearchCriteria.country_code`. Letzteres ist der echte Filter —
`crates/reprise-core/src/radio/search.rs:130-150` (`criteria_url`) hängt ihn an
die Anfrage.

Der kopflose Fall existiert bereits und ist genau das Zielverhalten: ohne
gespeichertes Land ist das Label `genre.name` und `country_code` ist `None`.
**Der Umbau macht den Sonderfall zum Normalfall** — er erfindet kein neues
Verhalten, er löscht die Verzweigung.

Die beiden anderen Chips sind nicht betroffen: `near_you_action`
(`radio_chips.rs:34-44`) baut bewusst `SearchCriteria { tag: None, country_code }`
— das ist der Ortsfall und bleibt. „Top voted" ist ein statischer Label-Button.
**Es bleiben drei Chips** (`Metalcore` · `Top voted` · `Near you`); der Grill hat
die Frage gestellt und so beantwortet.

## Aufgaben

### Task 1 — Der Filter fällt

`library_suggestion` verliert den Parameter `location`. Label ist immer
`genre.name.clone()`, `criteria.country_code` ist immer `None`.

**Der Aufrufer zieht mit.** `crates/reprise-gnome/src/ui/radio/add_dialog.rs:369-383`
(`refresh_library_chip`) liest `reprise_core::location::app_location(&self.conn)`
**ausschließlich** für diesen Aufruf. Fällt der Parameter weg, ist die lokale
Variable samt DB-Abfrage tot — sie muss mit entfernt werden, sonst steht dort
ein Clippy-Fehler auf ein unbenutztes Ergebnis. `near_you_action` liest die
Location unabhängig davon (`add_dialog.rs:525-532`) und bleibt unberührt.

`strings::radio_chip_genre_in_country` (`strings_radio.rs:91-96`) verliert damit
seinen einzigen Aufrufer und wird **gelöscht**, samt der Zeichenkette
`{genre} in {country}`.

Das Label geht danach als blanker `genre.name.clone()` ins UI, ohne
`strings`-Funktion — **das ist beabsichtigt** und im Grill so entschieden: der
Chip zeigt einen Eigennamen aus der eigenen Bibliothek, und Eigennamen werden
nicht übersetzt. Es entsteht kein neuer Übersetzungs-Wrapper.

**Akzeptanz:** `library_suggestion` nimmt kein Land mehr entgegen; kein Pfad
setzt `SearchCriteria.country_code` für den Bibliotheks-Chip.

### Task 2 — Die Tests sagen die neue Zusage

Drei Tests in `radio_chips.rs` beschreiben heute die Fallunterscheidung:

| Zeile | Test | heute | danach |
| --- | --- | --- | --- |
| 220 | `rad_5_the_library_chip_filters_by_the_played_genre_and_the_stored_country` | `label == "Metal in DE"` | entfällt bzw. wird umgedreht |
| 226 | `rad_5_without_a_country_the_library_chip_searches_the_genre_worldwide` | `label == "Jazz"` | **das ist ab jetzt der einzige Fall** |
| 243 | `rad_5_a_countryless_location_does_not_narrow_the_library_chip` | `label == "Jazz"` | deckungsgleich mit 226 |

Die Namen der beiden überlebenden Tests tragen eine Bedingung („without a
country", „a countryless location"), die nach dem Umbau nichts mehr
unterscheidet — beide Zustände sind derselbe. Sie werden zu **einem** Test
zusammengeführt, der die unbedingte Zusage nennt, plus **einem** Test, der
ausdrücklich belegt, dass ein *gespeichertes* Land die Suche **nicht** mehr
verengt (`country_code == None` trotz vorhandener `AppLocation`). Dieser zweite
ist der eigentliche Regressionsschutz und muss neu geschrieben werden — heute
existiert er nicht, weil das Verhalten das umgekehrte war.

Insgesamt fünf Test-Aufrufstellen (`radio_chips.rs:205, 226, 243, 254-255`)
übergeben ein zweites Argument und passen sich der Signatur an.

Präfix `rad_5_` bleibt, damit `scripts/check-ux-traceability.sh` die Kennung
wiederfindet.

**Akzeptanz:** Ein Test scheitert, wenn jemand den Ländercode wieder in
`SearchCriteria` legt.

### Task 3 — RAD-5 und ihre Fernwirkungen

`docs/ux-rules.md`, RAD-5 (ab Zeile 6187). Dieser Satz ist die Zusage, die sich
dreht:

> A stored country narrows the search and shows in the label ("Metal in DE");
> without one the chip keeps the genre and searches worldwide rather than
> becoming a second "Near you".

Er wird ersetzt durch die unbedingte Form: der Chip sucht **immer** weltweit und
zeigt **nur** den Genre-Namen; der Ortsfall gehört „Near you" und nur ihm. Der
zweite Absatz von RAD-5 (Near you, Portal, Einwilligung) bleibt Wort für Wort
stehen.

**Drei Fernwirkungen, die sonst als verwaiste Verweise stehen bleiben**
(vgl. Memory *removing-behaviour-orphans-a-ux-rule*):

- `docs/ux-rules.md:5801-5827` — **SRC-19** zitiert das Beispiel ausdrücklich:
  *„The label uses the country **code**, matching RAD-5's 'Metal in DE'"*. SRC-19
  selbst (Apple-Podcasts-Länder-Chip) ändert sich **nicht**; nur der Beleg-Verweis
  muss auf eine Regel zeigen, die das Beispiel noch trägt, oder das Beispiel
  selbst mitbringen.
- `radio_chips.rs:1-13` — Modul-Docblock erklärt den „Metal in DE"-Fall.
- `crates/reprise-core/src/library/taste.rs:4-8` — Modul-Docblock nennt dasselbe
  Beispiel. **`taste.rs` wird ausschließlich im Docblock angefasst** — keine
  Signatur, keine Logik, kein Test dieser Datei.

**Akzeptanz:** `scripts/check-ux-traceability.sh` grün, und kein Dokument
verspricht mehr eine Länderverengung am Bibliotheks-Chip.

### Task 4 — Die Übersetzungen

Die msgid `{genre} in {country}` steht in **8 Dateien**: `po/ar.po`, `po/bn.po`,
`po/de.po`, `po/es.po`, `po/fr.po`, `po/hi.po`, `po/zh_CN.po` und
`po/reprise.pot`.

Sie entfällt ersatzlos. Die `.po`-Dateien werden **nicht von Hand** bearbeitet —
der `pot`-Lauf des Buildsystems zieht das nach. Der Plan verlangt nur, dass der
Lauf gemacht und sein Ergebnis mitcommittet wird.

**Akzeptanz:** Keine Quelldatei referenziert die Zeichenkette mehr.

### Task 5 — Gegenmessung

Mutationsprobe: in `library_suggestion` `country_code: None` wieder durch den
gelesenen Ländercode ersetzen — **genau ein Vorkommen** — und belegen, dass der
Regressionstest aus Task 2 rot wird. Erst committen, dann mutieren: `git
checkout --` stellt HEAD wieder her und verschluckt Uncommittetes wortlos.

## Nicht in diesem Plan

- **Die zwei Schreibweisen im Screenshot** (`Metalcore` im Chip, `death metal`
  in der Genre-Spalte). Der Grill hat den Punkt **gestrichen**, nicht vertagt.
  Er hätte einen Title-Case-Helfer gebraucht, den es im Repo nicht gibt
  (geprüft: kein `title_case`, `to_titlecase`, `capitalize` in `crates/`), samt
  einer Grenzfall-Matrix (`hip-hop`, `r&b`, `AOR` → ein naives „erster Buchstabe
  groß" macht daraus `Aor`, also schlechter als der Rohwert). Das steht in
  keinem Verhältnis zum Befund. `radio_presentation.rs` wird **nicht**
  angefasst.
- **„Near you".** Der Ortsfall bleibt unverändert; er ist der Grund, warum das
  Land am Genre-Chip entbehrlich ist.
- **Ein dritter/vierter Chip oder eine Erklärung, woher das Genre stammt.** Drei
  Chips bleiben, so wie sie sind.
- **SRC-19** selbst (Apple-Podcasts-Länder-Chart) — nur ihr Verweis auf RAD-5
  wird nachgezogen.

## Belege

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`
- `scripts/check-ux-traceability.sh`
- die Mutationsprobe aus Task 5

Display-Tests sind hier nicht gefordert: die Änderung ist Zeichenketten- und
Kriterien-Logik, kein Layout.

## Parallelität

**Ein Strang. Der Plan wird nicht geschnitten.**

Task 1 ändert die Signatur, Task 2 die Tests derselben Datei, Task 3 die Regel,
die Task 2s Testnamen tragen.

**Reihenfolge:** 1 → 2 → 3 → 4 → 5.

**Dateibesitz dieses Strangs:**

```
crates/reprise-gnome/src/ui/radio/radio_chips.rs
crates/reprise-gnome/src/ui/radio/add_dialog.rs
crates/reprise-gnome/src/ui/strings_radio.rs
crates/reprise-core/src/library/taste.rs        (nur der Modul-Docblock)
docs/ux-rules.md
po/*
```

**Achtung, geteilte Datei:** `docs/ux-rules.md` gehört auch den vier
Geschwisterplänen dieser Welle und dem Strang
`queue-centering-ignores-section-headers`. Alle fassen verschiedene Regeln an,
aber dieselbe Datei — der Konflikt wird **beim Landen** aufgeräumt, nicht vorher
vermieden. `radio_reveal.rs` fasst dieser Plan **nicht** an; die Überschneidung
mit dem Radio-Verzeichnis ist nur namentlich.

**Post-Merge-Querprüfungen:** keine.
