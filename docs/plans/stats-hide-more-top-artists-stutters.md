---
slug: stats-hide-more-top-artists-stutters
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: „Hide more top artists" ruckelt (My Stats)

**Nur ein Befund, keine Messung.** Festgehalten am 16.08.2026, 08:06, gemeldet
vom Nutzer („wenn ich hide klicke bei artists, ist das sehr rucklig, da ist was
schief"), belegt durch einen Screenshot der Ansicht **My Stats**
(laufender Build: 0.1.13 = `dev`-Kopf `95b4b30016`; Bibliothek: 878 plays · 151 artists, 2 125 Titel).

## Symptom

Klick auf **Hide more top artists** (aufgeklappte Liste Rang 6–20, zwei
Spalten, je Zeile Künstlerbild + Balken) lässt die Oberfläche sichtbar
stocken. Ob das Aufklappen ebenfalls ruckelt, ist **nicht** angegeben; der
Nutzer nennt ausdrücklich *hide*.

Im Screenshot zusätzlich sichtbar: parallel lief Wiedergabe („Streaming ·
12 % loaded") — die Last kam also nicht aus einer leeren App.

## Code-Verortung (lokaler Hauptcheckout, ungeprüft gegen `origin/dev`)

`crates/reprise-gnome/src/ui/stats/stats_bands_card.rs`

Der Klick-Handler (`:196-210`) macht die Arbeit **synchron im Klick**, bevor
die Animation startet:

```rust
let reveal = !revealer.reveals_child();
state.render(reveal);          // <- volle Neuberechnung, auch beim Zuklappen
if reveal { revealer.set_visible(true); revealer.set_reveal_child(true); }
else      { revealer.set_reveal_child(false); }
```

Zwei Verdachtsmomente, beide unbelegt bis gemessen:

1. **`render(false)` rechnet die ganze Karte neu, obwohl nur zugeklappt wird**
   (`:44-62`): `snapshot.top_artists_sorted(sort_by)` sortiert alle Künstler
   neu (hier 151), danach `bands_row.set_data(&artists, share, sort_by)` — das
   baut die **Top-5-Heldenkarten samt Bildern** erneut auf, obwohl sich an
   ihnen nichts ändert. Beim Zuklappen ist davon nichts nötig; gebraucht wird
   nur `clear_continuation()`.
2. **Der Inhalt wird zerstört, bevor die Collapse-Animation läuft.**
   `clear_continuation()` (`:64-70`) leert beide Spalten sofort; erst danach
   fährt der `Revealer` (`:169-178`) seine Höhe von voll auf 0. Der Revealer
   animiert also eine *leere* Box, und die 15 Zeilen verschwinden in einem
   Schlag. Höhenanimation ist ohnehin layoutgebunden — jeder Frame löst ein
   volles Neu-Layout der Stats-Seite aus.

Zusatzverdacht: die Zeilen tragen Künstlerbilder
(`stats_bands_more::build_row`, `:89-100`, mit `StatsArtistImage` und einem
`generation`-Token). Ob `bands_row.set_data` beim Zuklappen erneut Bilder
anfordert oder dekodiert, ist **nicht** geprüft — das wäre die teuerste
Einzelursache.

**Gleiche Bauart, gleiches Risiko:** `stats_songs_card.rs:135-260` („Show/Hide
more top tracks"). Falls sich der Verdacht bestätigt, dort mitprüfen.

## Wie das zu messen ist, bevor jemand etwas ändert

Nicht raten, sondern an der laufenden App messen (vgl. Memory
*measuring-gtk-main-thread-stalls*):

- AT-SPI-Rundlaufzeit im 50-Hz-Takt als Stall-Detektor über den Klick legen —
  Leerlauf liegt bei 0,2–4 ms, ein Ruckler zeigt sich als einzelner Ausreißer.
- `/proc/<pid>/task/<pid>/stat` im 4-ms-Takt: State **R** + steigende Ticks =
  es rechnet (Sortieren/Layout/Decode), S/D = es wartet.
- Beide Richtungen messen (aufklappen **und** zuklappen), sonst ist unklar, ob
  `render()` oder der Revealer der Kostenträger ist.

## Lösungsrichtungen (offen)

1. Beim Zuklappen **nur** `clear_continuation()` aufrufen, nicht `render(false)`
   — die Top-5-Reihe bleibt unangetastet.
2. Das Leeren **nach** der Animation erledigen (`connect_child_revealed_notify`
   gibt es bereits, `:173-177`), damit der Revealer echten Inhalt ausfährt und
   nicht eine leere Box.
3. Fortsetzungszeilen einmal bauen und nur ein-/ausblenden, statt sie bei jedem
   Umschalten neu zu erzeugen.
