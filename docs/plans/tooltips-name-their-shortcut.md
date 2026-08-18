---
slug: tooltips-name-their-shortcut
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Tooltips sollen das Tastenkürzel mitnennen — mit Symbolen

**Wunsch des Nutzers, kein Plan.** Festgehalten am 16.08.2026:
*„tooltipps sollten auch den Keybinding enthalten, wenn es dafür einen gibt. so
werden die leute schneller mit der bedienung"*, nachgeschoben:
*„am besten mit Symbolen"*.

## Ist-Zustand, gemessen am 16.08.2026

- **13 Tastenkürzel** sind registriert (`set_accels_for_action`, verteilt über
  `ui/shortcuts.rs`, `ui/window/window_runtime_wiring.rs`,
  `ui/window/window_playing_source_wiring.rs`).
- **139 Tooltips** werden gesetzt (`set_tooltip_text`).
- **Keine einzige Stelle verbindet beides.** `accels_for_action` wird genau
  einmal gelesen (`shortcuts.rs:760`) — und dort nicht für einen Tooltip.

Die registrierten Kürzel:

| Aktion | Kürzel | Fundstelle |
| --- | --- | --- |
| `win.focus-search` | `<Control>f` | `shortcuts.rs:257` |
| `win.close` | `<Control>w` | `shortcuts.rs:275` |
| `app.quit` | `<Control>q` | `shortcuts.rs:285` |
| `win.jump-to-now-playing` | `<Control>l` | `window_playing_source_wiring.rs:166` |
| `win.open-primary-menu` | `F10` | `window_runtime_wiring.rs:274` |
| `win.nav-back` | `<Alt>Left` | `window_runtime_wiring.rs:341` |
| `win.nav-forward` | `<Alt>Right` | `window_runtime_wiring.rs:378` |
| `win.toggle-minimal-view` | `<Control>m` | `window_runtime_wiring.rs:504` |
| `win.preferences` | `<Control>comma` | `window_runtime_wiring.rs:505` |
| Wiedergabe/Pause | *bewusst leer* | `shortcuts.rs:197` (`&[]`) |

## Der tragende Gedanke: nicht 139 Tooltips anfassen

Die Zuordnung existiert bereits im Toolkit — jeder Knopf, der eine Aktion
auslöst, kennt seinen `action-name`, und GTK kennt zu jedem Aktionsnamen die
Kürzel. Ein Tooltip, der sein Kürzel selbst nachschlägt, ist eine **einzige**
Hilfsfunktion, kein Sweep über alle Bedienflächen:

```
tooltip_with_shortcut(widget, text)   // liest widget.action_name(),
                                      // fragt app.accels_for_action(),
                                      // hängt das Label an — oder auch nicht
```

Wo kein Kürzel registriert ist, bleibt der Tooltip unverändert. Das ist genau
die Bedingung aus dem Auftrag („wenn es dafür einen gibt") und zugleich der
Grund, warum die Funktion gefahrlos überall eingesetzt werden kann.

## „Mit Symbolen" — was GTK von sich aus liefert

`gtk4::accelerator_get_label()` übersetzt `<Control>l` in die Schreibweise der
Plattform und der Sprache des Nutzers — auf Linux „Ctrl+L", auf macOS „⌘L". Es
ist außerdem **übersetzt**: unter deutscher Oberfläche steht „Strg+L", nicht
„Ctrl+L". Das ist der Weg, der zu diesem Projekt passt, weil er die vorhandene
i18n-Kette nicht umgeht.

**Zu entscheiden:** ob „mit Symbolen" darüber hinausgehen soll — also `⌃`, `⇧`,
`⌥` statt „Ctrl", „Shift", „Alt" auch unter Linux. Das wäre eine bewusste
Abweichung von der GNOME-Konvention; die HIG schreibt für Linux die
ausgeschriebene Form. Zwei gangbare Lesarten:

1. **GTKs Label übernehmen** (empfohlen): plattform- und sprachrichtig, null
   Pflegeaufwand, konsistent mit dem Tastenkürzel-Fenster der App.
2. **Eigene Symbolschreibweise**: sieht kompakter aus, weicht aber von jeder
   anderen GNOME-App ab und muss selbst übersetzt und gepflegt werden.

Die Formatierung („Play (Strg+P)" vs. Tooltip mit Zeilenumbruch) gehört in
dieselbe Entscheidung.

## Fallstricke

- **Kürzel können leer sein, absichtlich.** `shortcuts.rs:197` registriert
  Wiedergabe/Pause ausdrücklich mit `&[]`. Die Funktion muss das als „kein
  Kürzel" behandeln, nicht als Fehler — und darf keine leere Klammer anhängen.
- **Mehrere Kürzel pro Aktion sind möglich.** `accels_for_action` liefert eine
  Liste. Festlegen: nur das erste zeigen (üblich) oder alle.
- **Nicht jeder Tooltip hängt an einem Aktions-Widget.** Die 139 Fundstellen
  umfassen auch Zeilen, Zellen und Bilder ohne `action-name`. Für die ändert
  sich nichts — und genau deshalb ist die Hilfsfunktion die richtige Form
  statt einer Pflichtregel für alle Tooltips.
- **Es gibt bereits ein Tastenkürzel-Fenster** (`shortcuts.rs`). Die Tooltips
  dürfen ihm nicht widersprechen; beide sollten dieselbe Quelle lesen.

## Berührte Stellen

| Datei | Rolle |
| --- | --- |
| `crates/reprise-gnome/src/ui/shortcuts.rs` | registriert die meisten Kürzel, liest `accels_for_action` schon einmal (`:760`) |
| `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs:274,341,378,504,505` | fünf weitere Kürzel |
| `crates/reprise-gnome/src/ui/window/window_playing_source_wiring.rs:166` | `Ctrl+L` — Sprung zum laufenden Titel |
| (neu) eine Hilfsfunktion neben `ui/strings.rs` | Tooltip + Kürzel-Label zusammensetzen |
