# Vorbestehende Fundstellen, aufgedeckt beim New-Releases-Umbau

**Datum:** 2026-07-25
**Kontext:** Aufgefallen während `docs/superpowers/plans/2026-07-24-new-releases-single-coverage.md`
und dem Nachlauf auf `chore/new-releases-followup`. Keiner dieser Punkte wurde
von jener Arbeit verursacht — sie lagen vorher schon da und wurden nur sichtbar.
Bewusst nicht mitbehoben, um die Änderung fokussiert zu halten.

## i18n

**Die meson-Targets für Übersetzungen sind für dieses Projekt funktionsunfähig.**
`po/meson.build`s `reprise-pot` und `update-po` konfigurieren kein
Rust-Keyword, kennen also `N_!` nicht. Ein Lauf über sie würde 671 von 673
Nachrichten aus dem Katalog werfen. Der einzige funktionierende Weg ist derzeit
die `xgettext`-Invokation, die in `scripts/tests/gettext-catalogs.sh` steht —
das Gate-Skript ist damit faktisch auch das Regenerierungswerkzeug. Sinnvoll
wäre, die Invokation an eine Stelle zu ziehen, die beide benutzen.

**Eine falsche Übersetzung in `de.po` und `es.po`.** Die msgid aus
`crates/reprise-gnome/src/ui/strings_issues.rs:73` trägt eine msgstr, die
inhaltlich zu einem Rhythmbox-Import gehört und nicht zur msgid passt. Vermutlich
ein verrutschter Eintrag aus einer früheren Katalogpflege.

**`format_total_duration` in `crates/reprise-core/src/format.rs:41` ist nicht
übersetzbar.** Die Einheitenwörter und die Konjunktion sind dort hart englisch
verdrahtet, weshalb die Statuszeile auch bei `LANG=de_DE` „1.654 Titel · 4 days,
3 hours and 46 minutes" zeigt — die vordere Hälfte übersetzt, die hintere nicht.
Visuell belegt in einem Xvfb-Start mit deutschem Katalog am 25.07.2026.

Das ist nicht durch einen `POTFILES.in`-Eintrag zu heilen: `reprise-core` bindet
gar keine gettext-Domain, das Makro `N_!` und `strings::text` leben
ausschließlich in `reprise-gnome`. Entweder wandert die Formatierung in die
UI-Schicht, oder die Funktion gibt strukturierte Werte zurück, die die UI
zusammensetzt. Letzteres wäre das Sauberere, weil Pluralformen und Wortstellung
ohnehin sprachabhängig sind und `format!` sie nicht abbilden kann.

**`create_instrumental_toast` in `strings_track_menu.rs`** legt beide
Plural-Literale einzeln in `N_!`, statt ein echtes Plural-Paar zu bilden.
`xgettext` extrahiert daher zwei flache Strings. Funktioniert, aber Sprachen mit
mehr als zwei Pluralformen können es nicht korrekt abbilden.

## Tests

**`crates/reprise-core/src/events/` hat einen zeitabhängigen Test**, der unter
voller Parallellast gelegentlich fehlschlägt und einzeln immer besteht. Zweimal
während des Umbaus aufgetreten, beide Male beim Wiederholen grün. Sollte
entweder deterministisch gemacht oder als `#[ignore]` mit Begründung markiert
werden, statt still zu flackern.

## Architektur

**Fünf Dateien liegen unmittelbar unter dem 800-Zeilen-Limit** aus
`scripts/check-architecture.sh` — die nächste Erweiterung reißt das Gate:

| Datei | Zeilen |
|---|---|
| `crates/reprise-gnome/src/ui/preferences/preference_sync.rs` | 799 |
| `crates/reprise-gnome/src/ui/tag_edit/autocomplete_entry.rs` | 797 |
| `crates/reprise-core/src/scrobbling.rs` | 796 |
| `crates/reprise-core/src/queue_tests.rs` | 793 |
| `crates/reprise-gnome/src/ui/track_list/track_menu.rs` | 792 |

Beim New-Releases-Umbau ist genau das passiert: `artist_news.rs` wuchs von 763
auf 932 Zeilen und musste nachträglich aufgeteilt werden. Ein Limit, das erst
beim Reißen auffällt, kostet einen Extra-Refactor mitten im Review.
