# Accessibility & Tastatur — Implementierungsplan

Stand: 2026-07-18
Ausgangsbasis: `main` bei `e0493d0`
Arbeitsbranch: `feat/accessibility-keyboard`

## Ziel

Jede aktuell ausgelieferte Reprise-Oberfläche muss ausschließlich mit der
Tastatur verständlich, vollständig und ohne Fokus-Sackgasse bedienbar sein.
Der Plan behandelt Tastaturbedienung nicht als Sammlung zusätzlicher
Shortcuts, sondern als durchgängigen Vertrag aus:

1. vollständiger Parität zu Maus-/Touch-Aktionen;
2. logischer Fokusordnung und stabilem Fokus-Lebenszyklus;
3. nativer Semantik für GTK, AT-SPI und Screenreader;
4. sichtbarem Fokus und fokusäquivalenten Hover-Affordances;
5. Tastaturalternativen für Drag-and-drop und eigene Werte-Controls;
6. automatisierter Abdeckung jeder GUI-Fläche plus ehrlicher manueller
   GNOME-Abnahme.

Die zugehörigen Regelvorschläge stehen als `ACC-1` bis `ACC-9` in
`docs/ux-rules.md`. Sie bleiben bis zur vollständigen Umsetzung und ihrem
regelbenannten Nachweis `[geplant]`.

## Abgrenzung

Dieser Plan umfasst alle bestehenden Oberflächen in `reprise-gnome`:
Hauptfenster, Sidebar, Tracks/Albums/Artists, Queue/Playlists, Filter,
Player-Leiste, Now-Playing/Lyrics, Issues, Geräte/Sync, Stats, Preferences,
First Run, Tag-Editor, Import-/Bestätigungsdialoge, Popover, Compact/Minimal
View und Portaldialog-Aufrufe.

Nicht Teil dieser Stufe sind:

- ein visuelles Redesign außerhalb notwendiger Fokusindikatoren;
- neue Produktfunktionen oder neue Roadmap-Views;
- vollständige WCAG-Zertifizierung;
- GNOME-/GTK-Upstream-Fixes;
- native Wayland-, Media-Key- oder Lock-Screen-Verifikation;
- Änderungen am Core-Datenmodell, sofern keine echte Keyboard-Operation sie
  zwingend benötigt.

Screenreader-Semantik, High Contrast und Large Text werden dennoch geprüft,
weil ein scheinbar funktionierender Tastaturweg ohne Namen, Rolle oder
sichtbaren Fokus keine belastbare Accessibility-Lösung ist.

## Normative Grundlage

- Die [GNOME-HIG zu Tastaturbedienung](https://developer.gnome.org/hig/guidelines/keyboard.html)
  fordert Parität zu Pointer-Aktionen, logische Tab-Reihenfolge sowie die
  Standardsemantik für Tab, Shift+Tab, Enter, Space, F10,
  Menü-Taste/Shift+F10 und Esc.
- Die [GNOME-HIG zu Accessibility](https://developer.gnome.org/hig/guidelines/accessibility.html)
  fordert kurze beschreibende zugängliche Namen und reale Prüfungen mit
  Keyboard, High Contrast, Large Text, Screenreader und Bildschirmtastatur.
- Die [GTK4-Accessibility-Dokumentation](https://docs.gtk.org/gtk4/section-accessibility.html)
  behandelt eine Rolle als Verhaltensversprechen: Eine per Gesture klickbare
  eigene Fläche braucht nicht nur `Button`-Rolle, sondern auch eine
  aktivierbare Action und die erwartete Tastatursemantik. Nichtstandard-
  Interaktionen brauchen zugänglichen Hilfetext.
- Die [GNOME-Referenz für Standardshortcuts](https://developer.gnome.org/hig/reference/keyboard)
  ist für vorhandene Standardaktionen bindend; eigene Belegungen dürfen die
  System- und Zugriffstasten nicht verdrängen.
- WCAG 2.2 dient als ergänzende Prüfheuristik für
  [Fokusordnung](https://www.w3.org/WAI/WCAG22/Understanding/focus-order.html)
  und [sichtbaren Fokus](https://www.w3.org/WAI/WCAG22/Understanding/focus-visible).
  GTK-/GNOME-Konventionen bleiben für die konkrete Desktop-Interaktion
  vorrangig.

## Arbeitsentscheidungen

### Native Controls vor eigener Gesten-Semantik

Wenn ein Element wie ein Button, Toggle, Link oder Range handelt, wird zuerst
ein entsprechendes GTK/libadwaita-Control verwendet und nur visuell
angepasst. Ein `GestureClick` auf `Label`, `Image`, `Box` oder `DrawingArea`
bleibt nur, wenn ein natives Control technisch ungeeignet ist und Rolle,
Name, Action, Fokus, Tasten und Tests vollständig mitgeliefert werden.

### Ein Tab-Stop pro Collection, sekundäre Aktionen im Kontext

Sidebar, `ColumnView`, `GridView`, `ListBox` und vergleichbare Collections
sind jeweils ein Tab-Stop. Pfeile bewegen den aktiven Eintrag. Unteraktionen
einer Row/Card erzeugen nicht automatisch eine lange Kette verschachtelter
Tab-Stops: Häufige native Buttons dürfen fokussierbar bleiben; weitere
sekundäre Aktionen stehen im per Menü-Taste/Shift+F10 erreichbaren
Kontextmenü. Der Mausweg und der Tastaturweg rufen dieselbe Action auf.

### Fokus ist logische Identität, nicht Widget-Identität

Viele Reprise-Views bauen GTK-Widgets bei Filter, Scan, Geräte-Update oder
Navigation neu. Fokuswiederherstellung speichert deshalb eine stabile
Domänenidentität (`track_id`, Album-Key, Artist-Name, Geräte-ID,
Issue-/Playlist-ID) und nicht bloß eine Position oder ein altes `Widget`.
Wenn das Ziel verschwindet, gilt deterministisch: nächstes bedienbares Ziel,
sonst vorheriges, sonst stabiler View-Container.

### Globale Shortcuts sind fokussensitiv

Space löst auf passivem Content, in passiven Collections und selbst bei
fokussiertem linken Sidebar-Toggle Play/Pause aus; der Toggle klappt die
Sidebar nur per Pointer oder Enter um. Andere per Tastatur fokussierte
Buttons, Toggles und Werte-Controls behalten Space lokal. Dasselbe
Fokusprinzip gilt für Enter, Escape, Pfeile und Page-Tasten.
Popover/Dialoge und Texteingaben gewinnen immer vor Fenster-Shortcuts. Diese
Priorität wird mit echten Key-Events getestet, nicht aus Controller-Reihenfolge
abgeleitet.

### Keine Pointer-Koordinaten als Accessibility-Nachweis

Die abschließenden Keyboard-Szenarien verwenden AT-SPI-Ziele, Key-Events und
Fokuszustände. Pointer-E2E bleibt für Hit-Testing/DnD-Gefühl nützlich, beweist
aber keine Tastaturbedienung. Jeder App-Lauf bleibt mit privatem D-Bus, Xvfb,
Scratch-XDG und `REPRISE_AUDIO_SINK=fakesink` von Nutzerdaten und Desktop
isoliert.

## Bestandsanalyse auf `e0493d0`

### Bereits gute Grundlagen

- Tracklisten aktivieren per Enter und öffnen ihr Kontextmenü per
  Menü-Taste/Shift+F10.
- Das Album-Grid besitzt Pfeilnavigation, Enter-Aktivierung, Keyboard-
  Kontextmenü und einen eigenen `:focus-visible`-Ring.
- Die Sidebar trennt Fokus-Browsing von Aktivierung; bloßes Tabbing/Arrowing
  routet nicht mehr in eine andere View.
- Der Tag-Editor besitzt mit TAG-8 eine detaillierte Enter-/Esc-/Tab-
  Semantik und Ctrl+Page-Up/Down-Navigation.
- Der Spalteneditor bietet Alt+Pfeil als Alternative zum internen Reorder und
  exponiert `KeyShortcuts`.
- Compact View und Track-/Album-Menüs besitzen bereits Keyboard-
  Kontextmenüpfade.
- Standardcontrols wie Button, Switch, ComboRow, DropDown, Scale,
  SearchEntry und native libadwaita-Dialoge bringen einen belastbaren
  Grundvertrag mit.

### Nachgewiesene oder sehr wahrscheinliche Lücken

| Fläche | Befund | Risiko |
|---|---|---|
| Player-Leiste | Cover, Titel und Artist sind passive `Image`/`Label` mit `GestureClick` | Pointer-only Aktionen, keine Rolle/Name/Fokus |
| Waveform | `DrawingArea` mit `GestureDrag`, ohne Key- oder Range-Vertrag | Seek ist per Tastatur nicht erreichbar |
| Artist-Detail | Top-Track ist eine `Box` mit Double-Click-Gesture | Enter/Space und Fokus fehlen |
| Lyrics | Synced-Zeilen sind per Gesture seekbar | NPP-8 ist nur per Pointer operabel |
| Album-Card | Artist-Untertitel ist ein klickbares `Label`; Play ist verschachtelter Hover-Button | unklare/duplizierte Fokuswege |
| Sidebar-Aktivität | Geräte-, Scan- und Relink-Karten aktivieren über Gestures auf Containern | Karte ist semantisch/passiv für Keyboard/AT-SPI |
| Issues | Row-Pills erscheinen nur bei Hover; mehrere Kontextmenüs sind nur per Rechtsklick verdrahtet | Aktionen sind nicht auffindbar/erreichbar |
| DnD | Queue-/Playlist-Reorder und Drop auf Sidebar-Ziele sind pointerzentriert | keine gleichwertige Reihenfolge-/Add-Operation |
| Suche | Zweites Esc fokussiert immer die Trackliste | falsches Ziel in Albums/Artists/Stats/Issues/Device |
| View-Rebuilds | mehrere Views entfernen Kinder vollständig und bauen sie neu | Fokusverlust bei Filter/Sync/Refresh möglich |
| View-Switcher CSS | `outline: none` ohne lokalen `:focus-visible`-Ersatz | Tastaturfokus kann unsichtbar sein |
| Semantik | explizite Namen/Rollen/States sind nur punktuell gesetzt | AT-SPI/Orca kann Controls unvollständig melden |
| Dialoge/Popover | viele eigene Esc- und Stack-Pfade, kein gemeinsamer Fokusvertrag | Fokus kann nicht zum Auslöser zurückkehren |

### Noch live zu verifizieren

Der CUA-Systemprobe ist in der aktuellen Sandbox vor dem App-Start mit
`Operation not permitted` blockiert. Das ist ein Host-/Socket-Limit, kein
Reprise-Befund. Darum sind folgende Aussagen bis zum isolierten Lauf auf
einem AT-SPI-fähigen Host ausdrücklich Hypothesen:

- konkrete Tab-Reihenfolge und sichtbarer Fokus im vollständigen Fenster;
- tatsächliche Fokusfalle/-rückgabe von AdwDialog, FileDialog und Popover;
- GTKs reale Priorität zwischen globalem Space und fokussierten Controls;
- zugängliche Namen/Zustände, die GTK implizit aus Tooltip/Label ableitet;
- Orca-Ausgabe und High-Contrast-Darstellung.

## Ziel-Inventar der Keyboard-Flows

Jede Zeile wird im finalen CUA-Sweep mindestens einmal allein per Tastatur
erreicht, bedient und wieder verlassen.

| Bereich | Muss abgedeckt sein |
|---|---|
| App-Shell | Startfokus, Header, Sidebar-Toggle, Back/Forward, View-Switcher, Suche, Primärmenü, Shortcuts/Help/About |
| Sidebar | Orte, Playlists, New/Import Playlist, Issues, Device-Card, Scan/Relink-Karten, Collapse/Overlay-Modus |
| Tracks/Playlist/Queue | Arrow/Home/End/Page, Multi-Select, Enter, Rating, Sort, Filter, Kontextmenü, Queue-Sektionen |
| Albums | Grid-Roving-Focus, Open, Play/Queue, Artist-Ziel, Kontextmenü, Back/Forward-Fokus |
| Artists | Master-Liste, Detail, Album-Cards, Top-Tracks, Show-all, Hero-Menü |
| Player/Now Playing | Transport, Shuffle/Repeat, Volume, Queue, Cover/Titel/Artist, Panel-Tabs, Up Next, Lyrics, Waveform |
| Issues/Import | Gruppen, Collapse, Row-Auswahl, Pills/Menüs, Locate, Remove/Undo, Retry/Dismiss/Export |
| Device/Sync | Filterchips, Trackliste, Sync/Cancel, Settings, Eject, Add-to-playlist-Drop-Alternative |
| Preferences | Seitennavigation, alle Switch/Combo/Scale/Entry-Flächen, Scrobbler, Sync, Column-Editor |
| Modale | First Run, Tag-Editor, Confirm/Discard/Delete/Locate, Import-Fortschritt, FileDialog, About/Shortcuts |
| Stats | Jahresauswahl, Scrollen, nicht-interaktive Charts/Listen ohne falsche Fokus-Stops |
| Compact/Minimal | Restore, Transport, Volume, Menü, Always-on-top, Preferences, Quit, Escape/Ctrl+W |

## Umsetzung — taskweise, strikt TDD

Jeder Task beginnt mit einem roten Test, führt nur die kleinste notwendige
Änderung ein, läuft durch alle Repo-Gates, wird adversarial gegen Regeln und
Inventar geprüft und endet in genau einem Commit. Die Ledger-Zeile wird nach
dem Commit ergänzt. Keine Regel wird vor ihrer vollständigen Abdeckung auf
`[aktiv]` gesetzt.

### Task KBD-1 — Keyboard-/AT-SPI-Testfundament erweitern

**Ziel:** Das bestehende CUA-Harness kann echte Key-Events, Fokuszustände,
Tab-Sequenzen, Fensterwechsel und Fokus-Rückgabe beweisen.

**Red:**

- Contracttests für noch fehlende `cua_press_key_label`,
  `cua_press_key_focused`, `cua_hotkey`, `assert_focused_label`,
  `assert_focus_within` und `assert_focus_returns_to` ergänzen.
- Ein Fake-Driver-Szenario muss scheitern, wenn kein Vorher-/Nachher-Snapshot
  existiert, der Key am falschen PID landet oder Fokus nach der Aktion nicht
  den erwarteten semantischen Knoten trägt.
- Der Runner muss auf `degraded`, `suspected_noop`, Escalation und fehlende
  `focused`-States fail-closed reagieren.

**Green:**

- `scripts/cua-e2e/lib.sh` um die Key-/Fokus-Primitiven erweitern.
- `scripts/tests/cua-e2e.sh` mit deterministischem Fake-Tree ausbauen.
- `scripts/cua-e2e/keyboard.sh` als separaten, vom Pointer-Sweep unabhängigen
  Szenario-Runner anlegen.
- Ein Manifest hält die obige Oberflächenliste und das zugehörige Szenario;
  fehlende Bereiche lassen den Contracttest fehlschlagen.

**Prüfung:** Shellcheck/Contracttest, danach vollständige Gates. Der echte
CUA-Lauf darf bei blockiertem Host nur als `deferred host check` dokumentiert
werden, nie als grün.

**Commit:** `test(a11y): add keyboard and focus acceptance primitives`

### Task KBD-2 — Shell, Fokusziel und Shortcut-Priorität härten

**Ziel:** App-Shell, Sidebar und globale Aktionen bilden einen stabilen
Fokusgraphen.

**Red:**

- Displaytest: zweites Search-Esc gibt Fokus an Tracks, Albums, Artists,
  Stats, Issues und Device jeweils an deren aktiven Container zurück.
- Displaytest: Sidebar-Arrow verändert Fokus/Selektion, routet erst auf
  Enter/Space.
- Key-Delivery-Test: Space toggelt Playback auf passivem Content und in
  passiven Collections, aber nie auf Entry, per Tastatur fokussiertem
  Button/Toggle, Range oder offenem Popover/Dialog. Der linke Sidebar-Toggle
  bleibt davon unabhängig immer ein globales Play/Pause-Ziel.
- Tests für F10, Ctrl+W, Ctrl+Q und die Synchronität der Help-Liste.

**Green:**

- Ein `ActiveContentFocus`-Adapter ersetzt die feste TrackList-Abhängigkeit
  der Esc-Logik; jede View exponiert genau ein stabiles Fokusziel.
- Globale Actions prüfen eine zentrale Fokus-/Transient-Entscheidung statt
  einzelner Widget-Ausnahmen.
- Standardshortcuts werden als Actions verdrahtet; vorhandene
  Alt+Left/Right-, Ctrl+F/L/,/?- und F1-Pfade bleiben unverändert.
- Navigation und Back/Forward speichern/restaurieren Fokus logisch pro View.

**Betroffene Dateien:** `ui/shortcuts.rs`, `ui/help.rs`,
`ui/window/window_runtime_wiring.rs`, `ui/window/library_shell.rs`,
`ui/sidebar/sidebar_row_wiring.rs`, die Focus-Adapter der Views.

**Commit:** `fix(a11y): harden shell focus routing and shortcut scope`

### Task KBD-3 — Library-Collections keyboard-complete machen

**Ziel:** Tracks, Albums und Artists sind als native roving Collections
bedienbar, ohne verschachtelte oder pointer-only Aktionen.

**Red:**

- Trackliste: Tab landet einmal im `ColumnView`; Arrow/Home/End/Page bewegen,
  Enter aktiviert, Space selektiert, Menu/Shift+F10 öffnet auf der
  Tastaturselektion.
- Album-Grid: Card-Open, Play/Queue, Artist-Navigation und Kontextmenü sind
  vom fokussierten Card-Item erreichbar; kein doppelter Card-/Child-Stop.
- Artist: Master-Auswahl aktiviert nicht beim bloßen Fokuswechsel;
  Album-Cards und Top-Tracks besitzen Enter-/Menüpfade.
- Back/Forward restauriert Collection und logischen Eintrag.

**Green:**

- Grid/List-native Activation bleibt der Primärpfad.
- Sekundäre Card-/Row-Aktionen wandern in das Keyboard-Kontextmenü oder in
  echte Controls; bestehende Maus-Aktionen delegieren auf dieselben Actions.
- Passive Double-Click-Boxes werden durch native Rows/Buttons oder einen
  vollständig semantischen Action-Surface ersetzt.
- Selection/Fokus und Playing-Markierung bleiben getrennte Zustände.

**Betroffene Dateien:** `ui/track_list/*`, `ui/library_views/album_*`,
`ui/library_views/artist_*`, `ui/nav_history.rs`.

**Commit:** `fix(a11y): make library collections keyboard complete`

### Task KBD-4 — Issues, Geräte und Aktivitätskarten erschließen

**Ziel:** Jede Karte, Row und Inline-Aktion in Issues/Import/Device/Progress
hat einen verständlichen Fokus- und Aktivierungspfad.

**Red:**

- Scan-, Relink- und Device-Card müssen Name, Rolle, Zustand und Aktivierung
  per Enter/Space exponieren.
- Missing-/Import-Row-Kontextmenüs öffnen per Menu/Shift+F10 auf der aktuellen
  Auswahl.
- Hover-Pills sind bei Row-Fokus sichtbar oder im Keyboard-Kontextmenü
  vollständig vertreten.
- Rebuild/Collapse/Retry/Dismiss/Remove erhält Fokus logisch; entfernte Rows
  nutzen die ACC-6-Fallback-Reihenfolge.

**Green:**

- Container-Gestures durch `Button`/`ActionRow` ersetzen oder über einen
  gemeinsamen Action-Surface-Helper vollständig semantisieren.
- Pointer- und Keyboard-Menüs verwenden denselben Model-/Action-Builder.
- `busy`, `expanded`, `disabled`, Progress-Name/Wert und Cancel-Aktion werden
  dynamisch aktualisiert.

**Betroffene Dateien:** `ui/issues/*`, `ui/import_errors_view.rs`,
`ui/scan/scan_progress.rs`, `ui/sidebar/sidebar_device_card.rs`,
`ui/device_view/device_view.rs`.

**Commit:** `fix(a11y): expose issue device and activity surfaces to keyboards`

### Task KBD-5 — Player, Waveform und Lyrics bedienen

**Ziel:** Sämtliche Player- und Now-Playing-Aktionen funktionieren ohne
Pointer.

**Red:**

- Cover, Titel und Artist der Player-Leiste besitzen eindeutige Fokus-Stops,
  Namen und Enter-/Space-Aktivierung mit denselben Callbacks wie Click.
- Transport/Volume/Queue behalten native Tasten; globales Space stört sie
  nicht.
- Waveform meldet Range-Min/Max/Now/Text und unterstützt Arrow,
  Page-Up/Down, Home/End mit einem einzigen Seek-Commit pro Key.
- Lyrics-Liste ist ein roving Focus-Container; Arrow bewegt, Enter seekt nur
  bei synced Lyrics; unsynced Text ist kein falscher Action-Stop.
- Now-Playing-Tabs exponieren TabList/Tab/Selected/Controls korrekt.

**Green:**

- Passive Player-Metadaten in echte flache Controls oder vollständige
  Action-Surfaces umwandeln.
- `WaveformSeek` erhält eine zentrale `SeekStep`-Entscheidung, fokussierbare
  Range-Semantik und zugänglichen Zeitwert; Drag und Keys rufen denselben
  Commit-Pfad.
- Lyrics-Zeilen verwenden List-/Row-Aktivierung statt Gesture-only Click.

**Betroffene Dateien:** `ui/player_bar/*`, `ui/now_playing/*`, `ui/lyrics/*`,
`ui/playback/*`, `ui/compact/*`.

**Commit:** `fix(a11y): make player waveform and lyrics keyboard operable`

### Task KBD-6 — Dialog-/Popover-Fokusvertrag vereinheitlichen

**Ziel:** Jede transiente Ebene startet, enthält, schließt und restauriert
Fokus deterministisch.

**Red:**

- Tab/Shift+Tab verlassen keinen offenen Dialog/Popover.
- Esc-Kaskaden: Autocomplete → Tag-Editor → auslösende Library-Row;
  Browse-Chooser → Browse-Button; Kontextmenü → fokussierte Row/Card;
  Bestätigung → auslösender Button/Row.
- First Run startet auf der primären sinnvollen Aktion; Preferences auf der
  gewählten Seite; Rhythmbox-Import behält Fokus über Selection → Progress →
  Complete.
- Ctrl+W schließt die oberste schließbare Ebene, ohne darunterliegende App-
  Aktion auszulösen.

**Green:**

- Gemeinsamer `TransientFocusGuard` speichert den schwachen Auslöser, setzt
  Initialfokus nach Present und restauriert bei Close mit stabilem Fallback.
- Eigene Esc-Controller delegieren an eine zentrale Kaskadenentscheidung;
  native Dialogsemantik wird nicht doppelt überschrieben.
- Primäraktionen und häufige Dialogbuttons erhalten übersetzbare Mnemonics;
  Konflikte werden pro Oberfläche getestet.

**Betroffene Dateien:** `ui/tag_edit/*`, `ui/preferences/*`, `ui/first_run.rs`,
`ui/dialogs.rs`, `ui/delete_tracks.rs`, `ui/issues/missing_dialogs.rs`,
`ui/browse/*`, `ui/track_list/column_layout_editor.rs`, `ui/about.rs`,
`ui/help.rs`, `ui/sidebar/sidebar_playlist_creation.rs`.

**Commit:** `fix(a11y): unify dialog and popover focus lifecycle`

### Task KBD-7 — DnD und Reorder mit Tastaturalternativen versehen

**Ziel:** Keine Move-/Add-Operation hängt ausschließlich an Drag-and-drop.

**Red:**

- Playlist-/Queue-Reorder per Keyboard produziert exakt dieselbe
  `ReorderMove`/`QueueReorderOp` wie der Drop-Pfad und respektiert Sort-,
  Filter-, Section- und Playing-Guards.
- „Zu Playlist/Queue hinzufügen" ist von der fokussierten Trackselektion per
  Kontextmenü erreichbar und delegiert auf dieselben Membership-/Queue-
  Funktionen wie Sidebar-Drop.
- Spaltenreorder per Alt+Arrow bleibt mit DnD synchron; Header-Reorder hat
  über den Column-Editor denselben erreichbaren Persistenzpfad.
- Nicht zulässige Moves sind disabled und benannt, nie stille No-ops.

**Green:**

- Reorder-/Add-Commands werden als gemeinsame Actions aus den bestehenden
  reinen Decision-Funktionen aufgebaut.
- Kontextmenüs bieten Move up/down/to top bzw. Add-Ziele nur, wenn sie für
  den aktuellen Kontext gültig sind.
- `KeyShortcuts`/HelpText dokumentieren nichtstandardisierte Reorder-Tasten.

**Betroffene Dateien:** `ui/track_list/track_list_dnd.rs`,
`ui/track_list/track_menu.rs`, `ui/track_list/track_list_context_menu.rs`,
`ui/sidebar/sidebar_dnd.rs`, `ui/track_list/column_*`, Queue-/Playlist-Wiring.

**Commit:** `fix(a11y): add keyboard alternatives for drag and drop`

### Task KBD-8 — Semantik-, Fokus- und Hover-Audit schließen

**Ziel:** Der gesamte GTK-Baum besitzt ehrliche Namen/Rollen/States und
sichtbare Fokusindikatoren; neue pointer-only Flächen werden zum Gate-Fehler.

**Red:**

- Widget-Walks über jede konstruierbare Oberfläche melden namenlose
  interaktive Controls, falsche Rollen, fehlende `selected/checked/expanded`
  States, dekorative Doppelmeldungen und unsichtbare Fokus-Stops.
- CSS-Gate schlägt bei `outline: none` ohne gleichwertige
  `:focus-visible`-Regel fehl.
- Input-Parity-Gate findet jede neue `GestureClick`, `GestureDrag`,
  `DragSource`, `DropTarget` und Pointer-Cursor-Stelle ohne dokumentierten,
  getesteten Keyboard-Partner.
- Displaytest beweist Fokusindikatoren mindestens an Shell, Switcher,
  Trackliste, Grid, Sidebar, Player, Dialog und Custom-Range.

**Green:**

- Zugängliche Labels/Relations/States zentral ergänzen und bei Statewechseln
  aktualisieren; Dekoration aus dem Tree nehmen.
- View-Switcher- und weitere CSS-Fokuslücken schließen, ohne Theme-Defaults
  unnötig zu überschreiben.
- Hover-only Controls erhalten `:focus-within`-Darstellung oder vollständige
  Keyboard-Menüparität.
- Ein schmaler `scripts/check-input-parity.sh`-Gate verlangt für jede eigene
  Pointer-/Drag-Fläche einen expliziten Rule-/Test-Verweis; keine pauschale
  Datei-Allowlist.

**Commit:** `test(a11y): gate semantics focus visibility and input parity`

### Task KBD-9 — Regelbenannte End-to-End-Abnahme und Status-Flips

**Ziel:** Alle automatisierbaren ACC-Regeln werden erst nach einem
vollständigen Keyboard-Sweep einklagbar.

**Red:**

- `acc-1-keyboard-only-surface-sweep` durchläuft das Ziel-Inventar ohne
  Pointer-Aktion.
- `acc_2_every_interactive_surface_has_name_role_state_and_action` prüft den
  Widget-/AT-SPI-Vertrag.
- `acc-3-tab-order-and-roving-collections`,
  `acc-4a-space-routes-global-and-local-controls`,
  `acc-5-transients-and-navigation_restore_focus`,
  `acc_6_dynamic_updates_preserve_logical_focus`,
  `acc-8-direct-manipulation-has-keyboard-equivalence` und
  `acc_9_help_matches_registered_standard_shortcuts` scheitern gezielt gegen
  jeweils eine zurückgenommene Implementierung.

**Green:**

- Echte isolierte CUA-Läufe für leeres und bestücktes Profil, schmale und
  breite Fenster, jedes Inventar-Ziel und alle transienten Ebenen.
- Nach jeder Aktion Snapshot; Fokus, State und sichtbarer Effekt werden
  gemeinsam geprüft. Keine Koordinaten, kein stilles Escalation-Fallback.
- Erst in diesem Commit `ACC-1/2/3/4/5/6/8/9` von `[geplant]` auf `[aktiv]`
  setzen und Traceability laufen lassen.
- `ACC-7` bleibt bis zur manuellen Sichtprüfung `[geplant]`.

**Commit:** `test(a11y): activate the automated keyboard accessibility rules`

### Manuelle GNOME-Abnahme und Stage-Closeout (kein Implementierungstask)

Nach KBD-9 folgt die reale Sicht- und Assistive-Technology-Prüfung. Sie ist
kein weiterer Implementierungstask und erzeugt nur dann einen Commit, wenn
die Prüfung bestanden wurde und `ACC-7` samt Release-Checkliste ehrlich
aktiviert werden kann.

**Manuelle Matrix:**

1. komplette App nur mit Tastatur, ohne Pointer;
2. Default- und High-Contrast-Theme: Fokus an jedem Stop sichtbar und von
   Selection/Hover/Playing unterscheidbar;
3. Large Text: keine abgeschnittenen primären Controls oder unerreichbaren
   Aktionen;
4. Orca: Namen, Rollen, States, Werte und Kontext verständlich; Bedienung bei
   ausgeschaltetem Monitor möglich;
5. On-Screen-Keyboard: alle Entry-/Autocomplete-/Save-Pfade nutzbar;
6. echtes GNOME/Wayland: Dialoge, Portal-FileChooser und Shortcut-Priorität;
7. reduzierte Animation: Fokus/State bleibt sichtbar (MOT-7).

**Abschluss:**

- Ergebnisse mit wörtlicher `ACC-7`-Referenz in `RELEASING.md` aufnehmen.
- Bei bestandener Sichtprüfung `ACC-7` im selben Commit auf `[aktiv]`
  setzen; andernfalls bleibt die Regel ehrlich `[geplant]` und der konkrete
  Befund wird als Manual Check dokumentiert.
- Vollständige Gate-Batterie, Datei-Limits, Input-Parity-Lint, CUA-Evidence,
  Ledger und ggf. Koordinationsboard aktualisieren; Lock freigeben.

**Commit bei bestandener Sichtprüfung:**
`docs(a11y): activate the visible focus acceptance rule`

## Pflicht-Gates je Task

```sh
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
scripts/check-display-tests.sh --rule-named
scripts/check-input-parity.sh          # ab KBD-8
git diff --check
```

Nach Änderungen an `reprise-core` zusätzlich:

```sh
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'
```

Die Ausgabe muss leer bleiben. Jeder wesentlich geänderte Code-File endet
unter 800 Zeilen; bestehende strengere Architekturgrenzen gelten weiter.

Jeder echte App-/CUA-Lauf enthält vollständig:

```sh
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  <REPRISE_SMOKE_* hooks> cargo run
```

Das existierende private AT-SPI-Harness darf diese Hülle intern aufbauen;
die Evidence muss Scratch-XDG, privaten Bus, X11/Xvfb und Fake-Audio
nachweisen.

## Adversarial Review je Task

Vor jedem Commit wird gezielt nach folgenden Fehlerklassen gesucht:

- Pointer- und Keyboard-Pfad laufen in verschiedene Callback-/Guard-Pfade;
- fokussierbare passive Controls oder aktive Controls ohne Fokus;
- doppelte Tab-Stops innerhalb derselben Row/Card;
- Navigation beim Fokuswechsel statt erst bei Aktivierung;
- Fokus wird durch Rebuild/Sort/Filter/Async-Update verloren oder gestohlen;
- Esc schließt mehrere Ebenen oder gibt Fokus an ein falsches/zerstörtes
  Widget zurück;
- globales Space/Enter/Escape/Pfeil überschreibt lokale Widget-Semantik;
- Role/State/Name stimmt nicht mit der sichtbaren Funktion überein;
- hidden/disabled Controls bleiben im Fokuspfad;
- DnD-Alternative umgeht Sort-/Filter-/Identity-/Persistenz-Guards;
- Fokusindikator ist nur im Default-Theme oder nur bei Hover sichtbar;
- ein Test prüft nur einen Helper, aber nicht die reale Signal-/Action-
  Verdrahtung;
- CUA-Test nutzt Pointer oder Koordinaten und behauptet Keyboard-Abdeckung.

## Definition of Done der Accessibility-Stufe

- Alle Tasks KBD-1 bis KBD-9 sind in Reihenfolge umgesetzt und committed.
- Das vollständige GUI-Inventar besitzt einen isolierten Keyboard-CUA-Flow.
- `ACC-1/2/3/4/5/6/8/9` sind mit regelbenannten Tests `[aktiv]`.
- Alle Maus-/Touch-/DnD-Aktionen besitzen einen gleichwertigen Keyboard-Pfad
  auf demselben Action-/Guard-/Persistenzpfad.
- Fokusordnung, -sichtbarkeit, -restauration und dynamische Erhaltung sind
  auf allen Flächen geprüft.
- Help/Shortcuts, zugängliche Properties und tatsächliche Actions sind
  synchron.
- Pflicht-Gates, Core-Purity und Datei-Limits sind grün.
- Ledger/Koordinationsstand sind aktuell, Lock ist freigegeben.
- Verbleibend sind ausschließlich ehrlich dokumentierte manuelle Checks;
  `ACC-7` wird ohne echte Sichtprüfung nicht als aktiv bezeichnet.

Die nächste Roadmap-Stufe beginnt dadurch nicht automatisch.
