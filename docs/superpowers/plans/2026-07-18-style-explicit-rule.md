# STYLE-1 — Wirkung explizit, nicht geerbt (Entwurf, 2026-07-18)

Fertiger Wortlaut für `docs/ux-rules.md`. **Noch nicht eingetragen:** Die
Datei gehört gerade dem laufenden Codex-Task (Sektionen Q und J). Nach dessen
Merge als **Sektion S** anhängen (P ist auf main die letzte, Q und R kommen
mit `feat/search-and-new-releases`).

## Anlass

Vier Fälle an einem Tag, alle mit grünem Test, alle erst im Screenshot
aufgefallen:

| Fall | Gesetzt | Tatsächlich gerendert | Ursache |
|---|---|---|---|
| Headerbar-Fläche | `@headerbar_bg_color` | `#16181b` (Fensterfarbe) | `ToolbarStyle::Flat` schluckt Bar-Hintergründe |
| Such-Streifen | zweite Top-Bar | wirkt schwebend | dieselbe Flat-Falle |
| Headerbar-Titel | `set_title_widget(NONE)` | „Reprise" mittig | Adwaita fällt auf den Fenstertitel zurück |
| Sidebar-Breite | `max-sidebar-width = 240` | 295 px | Label ohne `ellipsize` erzwingt Mindestbreite |

Gemeinsamer Nenner ist **nicht** `Flat`, sondern: eine gesetzte Property
bleibt wirkungslos, weil der Default-Zustand etwas anderes tut als erwartet —
und der Test prüft die Property statt das Ergebnis.

---

## Sektion S. Flächen & Geometrie

Was sichtbar wirken soll, muss explizit gesetzt sein. Geerbte oder
Framework-Defaults zählen nicht als gesetzt: Sie sind der häufigste Grund,
warum eine Property gesetzt ist und trotzdem nichts passiert.

- **STYLE-1** [geplant] [gtk] — **Wirkung explizit, nicht geerbt.** Jede
  Fläche, die sich vom Inhalt absetzen soll (Headerbar, eingeblendete
  Leisten, Sidebar-Kanten, Panels), trägt Hintergrund **und** Trennlinie
  ausdrücklich; jede Geometrie, die verbindlich ist (feste Breiten,
  Mindesthöhen), wird gegen ihre tatsächliche Allokation geprüft.
  `flat` bleibt genau dort, wo bewusst **keine** Abgrenzung gewollt ist.
  Bekannte Fallen, die diese Regel adressiert: `AdwToolbarView` mit
  `ToolbarStyle::Flat` unterdrückt Bar-Hintergründe (auch
  `@headerbar_bg_color`); eine `AdwHeaderBar` ohne Titel-Widget rendert
  ersatzweise den Fenstertitel (`show-title` muss zusätzlich aus); ein
  `GtkLabel` ohne `ellipsize` meldet seinen vollen Text als **Mindest**breite
  und hebelt damit jedes `max-width` des Containers aus;
  `AdwOverlaySplitView` rechnet ohne `sidebar-width-unit = Px` in `sp`.
  **Testregel:** Absicht darf geprüft werden, aber bei Flächen und Geometrie
  muss das **Ergebnis** belegt sein — nicht „Property X ist gesetzt", sondern
  „die Fläche hat sichtbaren Hintergrund" bzw. „die Spalte bleibt bei
  schmalem Fenster auf ihrer Breite". Was das Framework garantiert, wird auf
  Existenz getestet; was ausbleiben kann, auf Wirkung (dieselbe Denkfigur wie
  TIP-1a/2a und SEARCH-2).

---

## Ergänzung für `RELEASING.md`

Unter den manuellen Abnahmepunkten:

> - **STYLE-1 „Schweben"-Test** [manuell] — Jede einblendbare Leiste
>   (Suchleiste, Banner, Fortschrittskarte) einmal öffnen: Klappt sie flach
>   über den Inhalt, ohne eigene Fläche und Kante, fehlt der Hintergrund —
>   `ToolbarStyle::Flat` hat ihn geschluckt. Gegenprobe in allen drei
>   Dark-Themes, weil die Fensterfarbe je Theme anders danebenliegt.

---

## Umsetzungshinweis

Beim Eintragen: Sektionsbuchstaben gegen den dann aktuellen `main`-Stand
prüfen (heute wären Q und R durch den Search/NR-Branch belegt, S wäre frei) —
genau so, wie es der Kommentar am Kopf von Sektion O vormacht.
