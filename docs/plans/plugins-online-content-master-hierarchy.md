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

## Dritter Entwurf, 24.08.2026 — Fortschritt, Hierarchie, kein Auto-Aufklappen

Quelle: `Plugins Preferences.dc.html` (Claude-Design-Projekt
`c947ce4e-8f29-4551-93c0-0fde5e0f82de`, Variante `1a`) plus die Textfassung
`agent-prompt-plugins-hierarchy.md` desselben Projekts. Drei benannte Probleme
am Stand vom 22.08.:

1. Der Fortschritts-Chip hing als Overlay über der Überschrift `Plugins`.
2. `Online content` sah aus wie ein Plugin unter Gleichen.
3. Der Hauptschalter klappte beim Einschalten alle Unter-Einstellungen auf.

### 1. Fußleiste statt Kopf-Overlay (`SET-18`)

`preferences_window::build` nimmt keinen Chip und keine Kantenlinie mehr
entgegen, sondern **eine** Fußleiste, die als `AdwToolbarView::add_bottom_bar`
hängt — fest, nicht mitscrollend, auf jeder Seite an derselben Stelle. Die
Platzierungsmechanik für den Kopf (`place_chip_when_visible`,
`header_end_inset`, `descendant_center_box`) ist ersatzlos entfallen.

`preference_background_bar.rs` baut die Leiste: Abschnittslabel
`Background activity` mit Zähler-Badge, danach eine Zeile je laufendem Job in
fester Spaltenordnung (Besitzer 132px, Beschreibung flexibel, Balken 150px,
Prozent 44px rechtsbündig, Abbrechen). Der Balken ist ein `GtkProgressBar` mit
4px-Spur und flacher Akzentfüllung — keine Animation, keine Streifen.

**Jeder Job trägt den Namen seines Plugins.** Das war der eigentliche Defekt:
`ScanControls::show_batch_progress` ist *ein* Slot, den sich Artwork und Online
Lyrics teilten — deshalb tauchte der Lyrics-Check erst auf, wenn Artwork
abgeschaltet wurde. Jeder Besitzer schreibt jetzt in sein eigenes Fach
(`BackgroundBar::publish(JobOwner, …)`); zwei Jobs können sich keinen
Anzeigeplatz mehr wegnehmen.

Der Scan der Bibliothek behält seine eigene Darstellung (`ScanChromeView`), wird
aber in die Fußleiste eingehängt statt über den Kopf gelegt. Der Toast im
Hauptfenster bleibt unangetastet.

### 2. Der Master verlässt die Kartenliste (`SET-11a`)

`preference_online_master.rs` baut `Online content` als eigenständiges Widget:
Titel auf 1.2em/600, Zustands-Badge (`{on} of {total} plugins on` /
`all {total} plugins off`), Beschreibung über die volle Breite, Schalter 54×28
gegen 46×24 der Kinder, ganze Zeile per `GestureClick` schaltbar (ein Klick,
der auf dem Schalter landet, wird über `pick()` ausgenommen, sonst hebt er sich
selbst auf).

Die sieben Kinder liegen darunter in **einer** `AdwPreferencesGroup` — die
Kartenfläche des zweiten Entwurfs ist damit zurück, und mit ihr entfällt die
linke Chevron-Rinne: sie existierte, um Zeilentitel und Gruppenüberschriften auf
eine Kante zu bringen, und die Kinder-Karte ist jetzt bewusst 18px eingerückt
(`SET-14b`). Links der Karte eine 2px-Leiste als `linear-gradient` auf einer
eigenen `GtkBox`, damit die Deckkraft der Karte sie nicht mitdimmt.

**Aus-Zustand:** Deckkraft 0.42 statt `set_sensitive(false)` — der Entwurf
verlangt lesbar, nicht ausgegraut. Nicht bedienbar wird die Karte über
`can_target(false)`/`can_focus(false)`. Kein Einklappen mehr hinter „Show the N
sources": die Zeilen bleiben stehen, damit beim Umschalten nichts unter dem Blick
wegrutscht. Darunter die Hinweiszeile mit den Namen der Seitenleisten-Einträge.

### 3. Schalter und Aufklappen sind entkoppelt

`AdwExpanderRow::set_enable_expansion` setzt `expanded` auf denselben Wert —
genau das war die Ursache. Die Zeilen benutzen `show-enable-switch` nicht mehr;
der Schalter ist ein Suffix-`GtkSwitch`, `enable-expansion` bleibt immer wahr.
Damit öffnet kein Schalter mehr irgendetwas, und der Aufklapp-Zustand bleibt
beim Aus- und Wiedereinschalten des Masters unberührt. Der Gegenbeweis steht im
Test: `set_11a_switching_a_plugin_on_never_opens_its_settings` fährt zuerst die
alte Verdrahtung und zeigt, dass sie wirklich aufklappt.

### Abweichungen vom Entwurf, bewusst

- Der Entwurf zeigt **fünf** Plugins und schreibt „5 of 5 plugins on",
  „5 plugins paused" und „Concerts, New Releases and YouTube". Real sind es
  **sieben** (dazu Podcasts und Radio). Die Zahlen und die Namensliste werden
  deshalb aus der echten Modulliste gefüllt statt wörtlich übernommen — sonst
  stünde eine falsche Zahl in der Oberfläche.
- Die Chevron-Position folgt dem Entwurf (hinterer Slot), nicht mehr der Rinne
  aus dem zweiten Entwurf. Die Schalter teilen sich weiter eine rechte Kante,
  weil Zeilen ohne Chevron dessen Breite reservieren.
- `Connected services` kommt im Entwurf nicht vor und bleibt unverändert.

### Der Preis der Fußleiste, gemessen

Eine feste Leiste am Fuß kostet feste Höhe. Der Zeiger-Harness hat es gefunden,
bevor es jemand gesehen hätte: die letzten beiden Schalterzeilen der
Layout-Seite fielen unter die Falz und waren nicht mehr anklickbar
(`Layout switch hid the filter bar (expected '0', got '')` — der Klick landete
in der Leiste). Der Dialog zahlt das jetzt selbst statt der Seiten:
`content_height` steigt von 680 auf 752, also um die 72px, die die Leiste im
höchsten *Ruhe*-Zustand einnimmt (Gate aus, mit der Zeile „No online jobs").
Mit Gate an und ohne Jobs sind es 46px. `scripts/ptr-e2e/preferences.sh` führt
dieselbe Zahl, weil alle Y-Koordinaten dort relativ zur Dialogoberkante liegen.

Breite kostet sie dagegen keine — und das war der zweite Fund, allerdings erst
im zweiten Anlauf. Der Display-Test
`set_18_background_activity_never_reaches_the_dialog_head` verglich die
x-Position des Dialogtitels vor und nach dem Start zweier Jobs und meldete
einen Versatz: erst +43px, nach einem `set_width_chars(8)` auf der
Beschreibungsspalte in Isolation grün, im nächsten Vollauf +73px. Beide
Erklärungen dazu waren falsch, und die Messung sagt, warum:

- **Ein ellipsierendes `GtkLabel` verlangt nicht seinen ganzen Text als
  Minimum.** Gemessen: 13px, die Breite der Ellipse. `set_width_chars(8)` hat
  das Minimum von 13 auf 72px *angehoben* und dem Dialog 59px Luft genommen —
  die Zeile hat nichts repariert, sie hat geschadet.
- **Der Titel wanderte, weil das Testfenster zu schmal ist, nicht der Dialog.**
  `xvfb-run` startet hier mit 640x480; `parent.set_default_size(900, 760)`
  greift ohne Fenstermanager nicht, das Elternfenster kam mit 630px hoch, und
  der Dialog erreichte seine 760px nie. Alles darunter wurde vom Harness
  gequetscht. Das erklärt grün allein, rot im Rudel und einen anderen Versatz
  je Lauf: gemessen wurde die Fenstergröße, nicht die Regel.

Die Regel selbst — die Leiste darf den Dialog Höhe kosten, niemals Breite —
steht deshalb jetzt als Messung statt als Beobachtung: die Mindestbreite des
Dialoginhalts mit laufenden Jobs muss in die gesetzten 760px passen. Gemessen
547px, vorher 716px bei 44px Luft.

### Die Spaltenbreiten sind Proportionen des Entwurfs, nicht seine Pixel

Beim Nachmessen fiel der eigentliche Schaden auf, den kein Test bewachte: die
Zeile stand im echten Dialog als „Album cover…" da. Die Spaltenbreiten des
Entwurfs (132 / 150 / 44) stammen aus einer breiteren Zeile; in diesen Dialog
übernommen ließen sie der Beschreibung 101px von den 197px, die sie braucht.
Genau die Zahl, die den Job benennt — „Album covers · 1942 of 2132" — fiel weg.

Gemessen am 24.08.2026, Adwaita-Standard: „Online Lyrics" 90px, „100%" 39px,
die längste englische Beschreibung 197px, die gepinnte Seitenleiste 195px der
760px. Daraus die neuen Breiten 100 / 92 / 40 bei 12px Spaltenabstand und 20px
Innenabstand — der Beschreibung bleiben 211px. Zwei Nebenbefunde:

- Adwaita gibt dem Trog einer `GtkProgressBar` `min-width: 150px`. Ein
  kleineres `width-request` verliert dagegen lautlos; die Breite steht deshalb
  im Stylesheet.
- Die Beschreibung ellipsiert jetzt aus der *Mitte*. Was überläuft, ist eine
  Übersetzung, und die Zählung am Ende ist die Hälfte, die zählt.

Bewacht von `set_18_a_running_job_keeps_the_count_it_is_reporting`: gerechnet,
nicht angesehen — im 640px-Xvfb sieht kein Display-Test je die gesetzten 760px
allokiert, wohl aber, was jede Spalte verlangt. Das Werkzeug daneben,
`measure_background_bar_width_budget`, druckt dieselbe Bilanz, wenn wieder eine
Spaltenbreite zur Debatte steht.

### Belege

- `artifacts/plugins-online-content/plugins-master-bracket-on.png` — echte App,
  isolierte Xvfb-Sitzung, Gate an, alle sieben Plugins an: Badge im Akzent,
  Akzent-Leiste links, Karte voll deckend, nichts aufgeklappt.
- `artifacts/plugins-online-content/plugins-master-bracket-off.png` — derselbe
  Weg mit Gate aus: Karte auf 0.42, Badge grau, Fußleiste sagt „No online jobs
  — Online content is off".
- `artifacts/plugins-online-content/background-bar-running.png` — die Leiste
  mit zwei laufenden Jobs, gerendert auf einem 1600x900-Xvfb, damit der Dialog
  seine gesetzten 760px wirklich bekommt: beide Zeilen benannt, beide Zählungen
  vollständig lesbar.
- `scripts/ptr-e2e/run.sh` mit `PTR_E2E_PREFERENCES_ONLY=1`: alle Prüfungen
  grün, echte Zeigerereignisse auf dem echten Fenster.
- Display-Suite: 799 Tests, 0 rot, dazu die neuen unter `SET-11a`, `SET-14b`,
  `SET-18`; der Gegenbeweis zu Punkt 3 fährt zuerst die alte Verdrahtung und
  zeigt, dass sie wirklich aufklappt. Gegenprobe zum Breiten-Wächter:
  `TRACK_WIDTH_PX` zurück auf 150 → rot mit „leaves the description 153 px of
  the 197 px it needs", zurückgenommen → grün.
