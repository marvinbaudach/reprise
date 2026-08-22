---
slug: plugins-online-content-master-hierarchy
worktree: /home/marvin/Projects/reprise-plugins-and-layout-preferences
branch: feature/plugins-and-layout-preferences
phase: shipped
codex_session:
created: 2026-08-16
---
# „Online content" ist sichtbar der Hauptschalter über allem darunter

**Design-Vorgabe des Nutzers, kein Plan.** Festgehalten am 16.08.2026, 08:09.
Aussage: *„hier Online content ist der Schalter für alles darunter"* — mit
einem Entwurf als Zielbild.

Entwurf abgelegt: `docs/plans/assets/online-content-master-mock.png`.

## Der Ist-Zustand (Preferences → Plugins)

Alle Zeilen der Gruppe sind **Geschwister mit gleichem Gewicht**: „Online
content" steht als erste Zeile in derselben `AdwPreferencesGroup` wie Artwork,
Online Lyrics, Concerts, New Releases, YouTube und Podcasts — gleiche Höhe,
gleiche Fläche, gleicher Einzug. Nichts zeigt, dass die erste Zeile die
übrigen beherrscht.

Dazu kommt eine **doppelte Beschriftung**: die Gruppe trägt den Titel „Online
content" (`preference_plugins.rs:289-295`) und die Master-Zeile darin trägt
denselben Titel noch einmal (`:281-287`) — beide aus derselben Konstante
`PLUGIN_GROUP_ONLINE_CONTENT` (`strings_online_sources.rs:12`).

## Das Zielbild (aus dem Entwurf gelesen)

1. **Der Master steht allein oben**, seine Beschreibung läuft über die volle
   Breite; darunter beginnt erst der Rest.
2. **Die Module sitzen in einem eigenen, eingerückten Behälter** mit eigener
   Fläche und Haarlinien zwischen den Zeilen — optisch *innerhalb* des Masters,
   nicht neben ihm.
3. **Der Titel steht genau einmal.** Im Entwurf gibt es keine Gruppenüberschrift
   über dem Master.
4. Aufklapp-Pfeile bleiben, wo sie heute sind (Concerts, New Releases, YouTube,
   Podcasts, Radio); Artwork hat keinen.

## Zeilen-Layout (zweiter Entwurf, Wortlaut des Nutzers)

Entwurf abgelegt: `docs/plans/assets/online-content-rows-mock.png`.

> „Die Kartenfüllung fällt weg, Zeilen laufen über die volle Breite und werden
> nur durch Haarlinien getrennt. Damit sitzen Zeilentitel und
> Gruppenüberschriften auf derselben linken Kante, der Toggle auf der rechten.
> Das Chevron wandert in eine reservierte Rinne links — zwei Spuren statt fünf."

Konkret gegen heute:

1. **Keine Kartenfläche mehr.** Die Zeilen liegen direkt auf dem Seitengrund,
   getrennt nur durch Haarlinien — kein gerundeter `.boxed-list`-Block.
2. **Volle Breite.** Zeilentitel und Gruppenüberschriften teilen sich **eine**
   linke Kante; der Schalter sitzt an der rechten.
3. **Chevron links, in einer reservierten Rinne.** Heute steht es rechts
   *hinter* dem Schalter; im Entwurf steht es links vor dem Titel, und die
   Rinne bleibt auch bei Zeilen ohne Chevron frei (Album Covers, Artist
   Portraits, Online Lyrics, Source Images haben keins) — dadurch bleibt die
   Titelkante über alle Zeilen hinweg gerade.
4. **Zwei Spuren statt fünf:** Rinne + Inhalt, Schalter rechtsbündig.

Das ersetzt die Formulierung „eingerückter Behälter" aus dem ersten Entwurf:
die Unterordnung entsteht nicht mehr über eine eigene Fläche, sondern über die
Rinne und die gemeinsame Kante. **Beide Entwürfe zusammen lesen**, der zweite
ist der jüngere.

## Achtung: Der zweite Entwurf zeigt eine andere Modulliste

Er listet **Album Covers**, **Artist Portraits** (aus!) und **Source Images**
als drei getrennte Zeilen — genau die Aufteilung, die am 12.08.2026 mit
`ccb1c33ead feat(preferences): unify online artwork plugins` zu **einem**
Schalter „Artwork" zusammengefasst wurde. `ARTIST_PORTRAITS_MODULE` existiert
im Code nicht mehr; maßgeblich ist `module.artwork.enabled`.

Der Entwurf würde also eine bereits gelandete Zusammenlegung zurückdrehen.
**Vor der Umsetzung klären, ob das gewollt ist** — vermutlich ist der Entwurf
schlicht älter als die Zusammenlegung und zeigt nur das Layout, nicht die
Modulliste.

## Offene Fragen

- **Was passiert mit dem inneren Block, wenn der Master aus ist?** Die
  Beschreibung verspricht „nothing below runs, no requests, sidebar entries
  hidden" — soll der Block dann ausgrauen, einklappen oder verschwinden? Der
  Entwurf zeigt nur den Ein-Zustand. Heute hängt das an
  `preference_online_module_effects.rs`.
- **Der Entwurf zeigt „Radio", aber kein „Online Lyrics"** — der Screenshot des
  Ist-Zustands zeigt „Online Lyrics" und (abgeschnitten) vermutlich auch Radio.
  Ist das eine bewusste Änderung der Modulliste oder nur ein Entwurf aus einem
  anderen Stand? **Vor der Umsetzung klären**, sonst verschwindet ein Modul
  aus Versehen.
- **Umsetzung in libadwaita:** eine `AdwPreferencesGroup` in eine Zeile einer
  anderen zu schachteln ist nicht der vorgesehene Weg. Realistische Varianten:
  (a) Master als eigene Gruppe ohne Titel, Module als zweite Gruppe mit
  Einzug und eigener Fläche per CSS; (b) ein `GtkListBox` mit `.boxed-list`
  innerhalb der Master-Gruppe. Was davon mit der App-CSS zusammenpasst, muss
  am Bildschirm entschieden werden — Display-Fixtures ohne App-CSS messen hier
  nichts Echtes.

## Code-Verortung

- `crates/reprise-gnome/src/ui/preferences/preference_plugins.rs`
  - `online_master_row()` `:281-287` — die Master-Zeile
  - `online_group_with_master()` `:289-295` — Gruppe + doppelter Titel
  - Aufbau der Seite `:378-401`, `:412`, `:546-548` — `local_group`,
    `online_group`, `connected_group`
- Texte: `crates/reprise-gnome/src/ui/strings_online_sources.rs:12`
  (`PLUGIN_GROUP_ONLINE_CONTENT`), `ONLINE_CONTENT_MASTER_DESCRIPTION`
- Wirkung des Masters: `preference_online_module_effects.rs`
- Betroffene Tests beim Umbau prüfen: `preference_plugins_tests.rs`,
  `preferences_search_index.rs:149-181` (der Suchpfad lautet dort
  „Plugins › Online content" — hängt an Gruppentitel **und** Zeilentitel).

## Umgesetzt am 22.08.2026

Beide Entwürfe zusammen, wie im Plan verlangt — der zweite (Zeilen-Layout)
gilt, der erste nur noch für die Hierarchie.

- **Der Titel steht genau einmal.** `online_group_with_master` baut die Gruppe
  ohne Titel; die Master-Zeile trägt „Online content" und bekommt die
  CSS-Klasse `reprise-online-master`, die ihren Titel auf Überschriftsgewicht
  hebt. Ihre Beschreibung läuft über die volle Breite
  (`set_title_lines(0)`/`set_subtitle_lines(0)`).
- **Keine Kartenfläche mehr.** `preference_plugin_chrome::css()` nimmt der
  `.boxed-list` auf der Plugins-Seite Fläche, Rahmen, Schatten und Radius; die
  Zeilen trennt nur noch eine Haarlinie.
- **Chevron links in einer reservierten Rinne.** Jede Zeile bekommt die Rinne,
  auch die ohne Chevron; die Gruppenüberschriften sind um dieselben 42px
  eingerückt, damit sie mit den Zeilentiteln auf einer Kante sitzen. Der
  eingebaute Pfeil von libadwaita bleibt als unsichtbarer Platzhalter hinter
  dem Schalter stehen — genau das hält alle Schalter auf einer rechten Kante
  (`SET-14a`).

**Fallstrick:** der erste Versuch traf nichts, weil der Selektor
`listbox.boxed-list` lautete. Eine `GtkListBox` rendert in GTK4 als Node
`list`, nie `listbox` — die Klasse allein adressieren.

## Die offenen Fragen sind beantwortet

- **Modulliste:** Artwork bleibt zusammengelegt. Der Zeilen-Entwurf ist älter
  als `ccb1c33ead` und zeigt nur das Layout, nicht die Module.
- **Master aus:** bleibt wie gehabt — die Zeilen klappen hinter „Show the N
  sources" ein (`apply_collapsed_group`), nur die Darstellung ändert sich.
- **libadwaita-Umsetzung:** Variante (a) light — eine Gruppe ohne Titel, die
  Unterordnung entsteht über Rinne, gemeinsame Kante und Haarlinien, nicht über
  eine eigene Fläche.

Beleg: `artifacts/plugins-online-content/plugins-flat-rows.png` (echte App,
isolierte Xvfb-Sitzung), plus die Display-Tests in
`preference_plugin_chrome.rs` und `preference_plugins_tests.rs`.
