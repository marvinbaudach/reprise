---
slug: podcast-add-dialog-worldwide-chip
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: „Add Podcast" soll neben „Popular in CH" auch weltweit beliebte zeigen

**Wunsch des Nutzers, kein Plan.** Festgehalten am 16.08.2026:
*„vielleicht noch popular weltweit dazu"* — belegt durch einen Screenshot des
Dialogs **Add Podcast** (laufender Build 0.1.13 = `dev`-Kopf `95b4b30016`).

## Ist-Zustand

Der Dialog zeigt genau **einen** Chip: **Popular in CH**, darunter die Liste
`PODCASTS · TOP IN CH`. Das Land kommt aus dem app-weiten Standort
(`add_dialog_chips.rs:30-38`, `dialog_country`) und fällt auf das
Gebietsschema zurück, wenn kein gültiger Ländercode gespeichert ist.

- Chip-Modell: `crates/reprise-gnome/src/ui/podcasts/add_dialog_chips.rs:9-22`
  — `AddDialogChip` kennt heute nur `Charts { country }` und `LibraryGenre`
- Entscheidung, ob der Chip erscheint: `chip_for()` `:47-58` — nur online und
  nur mit Netz-Einwilligung (`NET-1a`) und Erreichbarkeit (`NET-3`)
- Texte: `strings_podcasts.rs:450` (`Popular in {country}`), `:458`
  (`PODCASTS · TOP IN {country}`)

## Die technische Hürde: Apple kennt kein „weltweit"

`chart_url()` (`crates/reprise-core/src/podcasts/itunes_charts.rs:32-37`) baut

```
{CHART_ENDPOINT}/{country}/podcasts/top/{CHART_LIMIT}/podcasts.json
```

Der Ländercode ist ein **Pfadsegment** — es gibt keinen Storefront „global".
Eine weltweite Liste muss also erfunden werden. Drei Wege, keiner davon
kostenlos:

1. **US als Stellvertreter.** Ein Chip „Popular worldwide", der intern `us`
   abruft. Ehrlich beschriftet wäre das „Popular in US" — der Chip würde also
   etwas behaupten, was er nicht liefert. Billigste Variante, aber sie lügt.
2. **Mehrere Storefronts mischen.** N Länder abrufen und die Ergebnisse nach
   Häufigkeit/Rang zusammenführen. Das ist ehrlich „weltweit", kostet aber N
   Abrufe pro Öffnen des Dialogs und braucht eine Rangregel.
3. **Zweiter Chip mit expliziter Länderwahl** statt „weltweit" — z. B.
   „Popular in …" mit Auswahl. Löst den Wunsch nur halb.

**Vor der Umsetzung entscheiden.** Ich empfehle 2 mit einer kleinen, fest
verdrahteten Länderliste und einem Abruf pro Land, gecacht wie die bestehende
Chart-Abfrage — 1 klingt einfach, macht aber aus einer US-Liste eine
Weltbehauptung.

## Was der Umbau berührt

- `AddDialogChip` bekommt eine dritte Form (z. B. `Charts { scope }` mit
  `Scope::Country(String) | Scope::Worldwide`) — der `label()`-`match`
  (`:16-21`) und alle Konstruktionsstellen ziehen mit
- `chip_for()` gibt heute `Option<AddDialogChip>` zurück, künftig mehrere
  Chips → Signatur und Aufrufer (`add_dialog*`) anpassen
- Zwei neue Texte: Chip-Beschriftung und Listenüberschrift (Muster
  `strings_podcasts.rs:450`/`:458`), plus `po/`-Nachzug
- Netz-Gate gilt unverändert für beide Chips (`NET-1a`, `NET-3`)
- Regelwerk: `SRC-19` in `docs/ux-rules.md` beschreibt heute *„der Chip nennt
  das Land des Standorts"* — die Regel muss ergänzt werden, sonst steht ein
  zweiter Chip regelwidrig da (vgl. Memory *removing-behaviour-orphans-a-ux-rule*)

## Offene Fragen

- Soll der Weltweit-Chip **immer** erscheinen oder nur, wenn kein Standort
  gesetzt ist? (Bei gesetztem Standort zwei Chips nebeneinander.)
- Reihenfolge: Land zuerst, weltweit daneben — oder umgekehrt?
- Gilt derselbe Wunsch für den **YouTube**-Add-Dialog? Der teilt sich das
  Chip-Modell (`add_dialog_chips.rs`), hat aber `LibraryGenre` statt Charts.
