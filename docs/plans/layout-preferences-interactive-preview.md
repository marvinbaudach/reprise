---
slug: layout-preferences-interactive-preview
worktree: /home/marvin/Projects/reprise-plugins-and-layout-preferences
branch: feature/plugins-and-layout-preferences
phase: shipped
codex_session:
created: 2026-08-22
---
# Die Layout-Seite zeigt eine anklickbare Vorschau statt zweier Choice-Cards

**Design-Vorgabe des Nutzers**, gestellt am 22.08.2026 zusammen mit dem
Plugins-Umbau (`plugins-online-content-master-hierarchy.md`). Entwurf:
Claude-Design-Projekt `3f0280f8-115b-478c-b964-82fe086c5243`, Datei
`Layout Preferences.dc.html`.

## Warum

Die Player-Bar-Position hing an zwei statischen Choice-Cards, die je ein
Mini-Fenster zeigten — zwei Bilder für eine Entscheidung, und die vier
Sichtbarkeits-Schalter darunter hatten mit diesen Bildern nichts zu tun. Der
Entwurf ersetzt beides durch **eine** zusammenhängende Vorschau des
Bibliotheksfensters, in der jede Region selbst das Bedienelement ist.

## Was jetzt steht

- `preference_layout_preview.rs` zeichnet das Mini-Fenster: Titelzeile,
  Player-Bar, Navigation-Sidebar, Filterleiste, Trackliste, Statusleiste,
  Details-Sidebar. Jede Region ist ein flacher `GtkButton` mit Tooltip und
  Accessible-Label; Hover setzt einen Akzentrahmen, `:focus-visible` einen
  Akzentring.
- Eine ausgeblendete Region verschwindet nicht spurlos: sie bleibt als
  gestrichelter Platzhalter mit „+" an ihrem Platz und kommt per Klick zurück.
  Seiten-Sidebars schrumpfen dabei auf einen schmalen Streifen, Leisten auf
  eine niedrige Zeile.
- Das Widget hält **keinen** eigenen Zustand. Es rendert `LayoutPreviewState`
  und meldet den gewünschten neuen Zustand über einen Callback zurück.
- Die Seitenreihenfolge ist fest: Navigation links, Content, Details rechts.
  Es gibt kein `SidebarPosition`-Enum — die Seiten sind nicht konfigurierbar.
- Gruppen der Seite: **Window Layout** (Vorschau, „Click a region to move or
  hide it"), **Window Regions** (Player Bar Position, die vier Regionen mit
  Kanten-Untertiteln, List Density), **Columns** (unverändert), am Fuß
  „Restore defaults".
- Player Bar Position und List Density sind `AdwToggleGroup`-Segmente statt
  Karten bzw. `AdwComboRow` — wie im Entwurf.

## Ein Speicherweg für beide Seiten

Vorschau und Schalter teilen einen `Rc<Cell<LayoutPreviewState>>`. Jede
Änderung läuft durch `commit`, egal woher sie kommt; danach synchronisiert
`LayoutControls::sync` beide Seiten (Guard-Flag gegen Rückkopplung).
`commit_changes` zerlegt die Anfrage in einzelne `LayoutChange`-Schritte und
behält bei einem abgelehnten Schritt dessen vorherigen Wert — mit Toast. Genau
dieser Teil ist ohne Datenbank testbar (`a_rejected_save_keeps_the_previous_state`).

## Entfernt

`preference_choice_cards.rs` — nach dem Umbau ohne Nutzer. Seine CSS-Klassen
(`.reprise-choice-preview`, `.reprise-preview-sidebar/-content/-player`) leben
in `preference_layout_preview::css()` weiter, ergänzt um `.reprise-preview-zone`
und `.reprise-preview-ghost`.

## Strings

`SHOW_FILTERS` → „Filter Bar", `SHOW_INFORMATION_PANEL` → „Details Sidebar",
`SHOW_SIDEBAR` → „Navigation Sidebar", `SHOW_STATUS_LINE` → „Status Bar", dazu
Kanten-Untertitel, Tooltips und die Vorschau-Beschreibung. 21 neue msgids, in
allen sieben Katalogen ergänzt, de und es übersetzt (der Gate verlangt beides
vollständig).

## Belege

- `artifacts/layout-preferences/layout-preview.png` — echte App, isolierte
  Xvfb-Sitzung.
- `artifacts/layout-preferences/layout-preview-clicked.png` — nach einem Klick
  auf die Player-Bar-Region: die Bar sitzt oben, der „Top"-Schalter darunter
  ist mitgezogen, und die **echte** Player-Bar im Fenster dahinter ist
  ebenfalls oben. `player_bar_position=top` steht in der Scratch-DB.
- `scripts/ptr-e2e/preferences.sh` klickt jetzt zwei Vorschau-Regionen und die
  Positions-Segmente; die Koordinaten sind am 22.08.2026 an der maximierten
  1600x900-Sitzung gegen das 760x680-Dialogrechteck gemessen.

UX-Regel: `SET-16`.
