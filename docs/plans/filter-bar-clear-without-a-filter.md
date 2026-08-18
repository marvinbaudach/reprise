---
slug: filter-bar-clear-without-a-filter
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: „Clear" steht in der Filterleiste, obwohl gar kein Filter gesetzt ist

**Nur ein Befund, kein Plan.** Gemeldet am 16.08.2026: *„es sollte kein Clear
angezeigt werden, wenn gar kein Filter gesetzt ist."* Belegt durch einen
Screenshot der YouTube-Ansicht (laufender Build 0.1.13 = `dev`-Kopf
`95b4b30016`).

## Was im Screenshot steht

Die Leiste zeigt links **`+ Add filter`** ohne einen einzigen Chip, rechts
**`4 channels · 106 episodes · 26 new · 1 selected`** und daneben **`Clear`**.
Es ist also tatsächlich kein Filter gesetzt.

## Es sind zwei verschiedene Schaltflächen — die sichtbare ist nicht die Filter-Schaltfläche

`crates/reprise-gnome/src/ui/podcasts/podcasts_filter_bar.rs`:

| Schaltfläche | Beschriftung | Sichtbar wenn |
| --- | --- | --- |
| `clear_all` | **„Clear all"** (`strings_podcasts.rs:47`) | ein Filter aktiv ist — `:319`, `active(&filter)` |
| `clear_selection` | **„Clear"** (`strings_podcasts.rs:50`) | `selected_count > 0` — `:219` |

Im Screenshot ist `selected_count == 1` („1 selected"), also erscheint
`clear_selection`. Der Code tut damit genau, was er soll: **die sichtbare
Schaltfläche räumt die Auswahl, nicht den Filter.** Der Fehler liegt in der
Lesbarkeit, nicht in der Logik:

- sie sitzt in der **Filter**leiste, direkt neben `+ Add filter`
- sie heißt bloß „Clear", während die Filter-Variante „Clear all" heißt — der
  kürzere Text liest sich wie der allgemeinere
- die zugehörige Auswahl ist im Screenshot **nirgends sichtbar**: die markierte
  Episode liegt in einem eingeklappten Kanal. Der Nutzer sieht eine Aktion für
  einen Zustand, den er nicht sehen kann.

## Lösungsrichtungen (offen)

1. **Beschriften, was sie räumt** — „Clear selection" statt „Clear". Kleinster
   Eingriff, nimmt die Verwechslung mit dem Filter weg. Kostet Platz und eine
   neue Übersetzung.
2. **Aus der Filterleiste heraus** — die Auswahl-Aktion dorthin, wo die Auswahl
   steht (zur Zählung „1 selected"), und die Filterleiste bleibt der
   Filterleiste vorbehalten.
3. **Nur zeigen, wenn die Auswahl sichtbar ist** — bei eingeklappten Kanälen
   ohne sichtbare markierte Zeile gar nicht anbieten. Klingt sauber, ist aber
   die aufwendigste Variante und verbirgt eine echte Aktion.

Empfehlung: **1 + 2.** Der Nutzer will keine Aktion für einen Filter sehen, den
es nicht gibt — beides zusammen erfüllt das, ohne die Auswahl-Aktion zu
verlieren.

## Offene Fragen

- Ist die Auswahl hier überhaupt gewollt? „1 selected" stammt vermutlich vom
  Anklicken der Episode, die abgespielt werden sollte — wenn Abspielen eine
  Auswahl hinterlässt, ist die Zählung selbst schon irreführend.
- Gilt derselbe Umbau für die **Podcasts**-Ansicht? Sie teilt sich diese
  Leiste (`podcasts_filter_bar.rs` bedient beide Arten).
- Betroffene Tests: `podcasts_view_tests.rs` und die Zählzeile aus
  `podcast_summary_with_selection` (`:212`).
