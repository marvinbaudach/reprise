---
slug: sidebar-marks-the-visible-view
worktree: /home/marvin/Projects/reprise-sidebar-marks-the-visible-view
branch: feature/sidebar-marks-the-visible-view
phase: refactored
codex_session:
created: 2026-08-14
---
# Die Sidebar markiert die sichtbare Ansicht — und der markierte Eintrag ist klickbar

**Status:** Final. Gegrillt am 2026-08-14; die sieben Beschlüsse unten überstimmen
jede abweichende Formulierung im Fließtext.
**Crate:** `reprise-gnome` (Binary `reprise`).

## Beschlüsse aus dem Grill (verbindlich)

1. **Leerfall.** Ist die Doctor-Ansicht sichtbar, ohne dass eine Doctor-Zeile existiert
   (0 offene Findings, oder Öffnen über Kebab-Menü / „Edit tag"), wird **nichts** markiert.
   Kein Einblenden einer Zeile ohne Issues, keine Umgestaltung der Sidebar. Konsistent mit
   `sidebar_rebuild.rs:405-418`.
2. **Ein Feld statt einer Parallelliste.** `Shared` bekommt
   `doctor_row: RefCell<Option<gtk4::ListBoxRow>>` — ein einzelnes Feld, **keine** Liste.
   Es gibt genau eine quellenlose Zeile; eine Vec dafür ist Vorratshaltung, und
   `sidebar.rs` hat 17 Zeilen Luft. Wo unten noch ein Vec-Typ oder von „Parallelstruktur"
   die Rede ist, gilt dieses Feld.
3. **Kartenoptik über kombinierte Klassen.** Die Behauptung des Entwurfs, die drei
   Emphasis-Klassen besäßen nur `border-color`, ist **falsch**: `.device-card-connected`
   und `.device-card-active` setzen selbst `background-color`
   (`sidebar_device_card_text.rs:107-108`). Eine „zuletzt deklariert gewinnt"-Regel nähme
   der gerade synchronisierenden, geöffneten Karte ihre Akzentfläche. Stattdessen
   verstärkt die neue Klasse die Fläche des jeweiligen Zustands über höhere Spezifität:

   ```
   .device-card-current.device-card-remembered { background-color: alpha(@window_fg_color, 0.13); }
   .device-card-current.device-card-connected  { background-color: alpha(@window_fg_color, 0.16); }
   .device-card-current.device-card-active     { background-color: alpha(@accent_color, 0.20); }
   ```

   Werte sind ein Ausgangspunkt, kein Dogma; Theme-Tokens sind Pflicht, Literale verboten.
   Akzent bleibt Akzent, „offen" ist eine Stufe mehr Fläche.
4. **Ein Rechenweg, zwei Auslöser.** `route_row` schreibt `current_place` **nicht** selbst.
   Es ruft nach `on_select` dieselbe Ableitung, die auch das Stack-Notify ruft (Stack lesen
   → Ort ableiten → `apply_marking`). Damit gibt es genau **eine** Stelle, die den Ort
   berechnet, und die offene Annahme über die Synchronität von
   `notify::visible-child-name` (Falle 12) wird gegenstandslos statt umschifft. Zwei
   unabhängige Schreiber auf denselben Zustand sind genau die Drift, aus der dieser Bug
   entstanden ist.
5. **Beide Zugaben sind verbindlich**, nicht optional: die Split-View-Lücke des
   Doctor-Öffnungswegs (Task 6, Punkt 3 — eigener Commit) und der Drift-Wächter (Task 8).
6. **NAV-18 wird direkt `[active]`** eingetragen, zusammen mit seinen Tests. Der
   Ownership-Vorbehalt aus `AGENTS.md:172-180` (Flathub-Strang A) ist erloschen: es gibt
   keinen Flathub-Plan mehr unter `docs/plans/`, und `docs/ux-rules.md` wurde zuletzt am
   2026-08-13 aus einem gewöhnlichen Feature-PR (#461) geändert. Die Rückfallvariante
   „`[planned]` + `<!-- REVIEW: rule proposal -->`" aus Task 7 entfällt.
7. **Ein Strang.** Der Zwei-Strang-Notschnitt am Ende ist dokumentiert, wird aber **nicht**
   gefahren. Tasks in Nummernfolge, ein Commit pro Task.
**Regel-ID:** neu, **NAV-18** (nächste freie ID; `NAV-17` ist die höchste vergebene,
`docs/ux-rules.md:263`).

---

## 1. Kontext und Befund

### 1.1 Zwei Symptome, eine Wurzel

Der Nutzer meldet zwei Dinge, die wie zwei Fehler aussehen und einer sind:

**Symptom A — falsche Markierung.** In der linken Spalte bleibt die *verlassene* Quelle
markiert, während eine quellenlose Ansicht sichtbar ist: aus der Musikansicht heraus
bleibt **Music** markiert, aus **My Stats** heraus bleibt **My Stats** markiert, obwohl die
**Library-Doctor**-Ansicht auf dem Schirm ist.

**Symptom B — toter Klick.** Genau der so markierte Eintrag ist unklickbar. Der Nutzer war
auf **My Stats**, ist von dort über den Interpreten-„Edit tag" im Library Doctor gelandet,
klickt auf **My Stats** — und nichts passiert. Die Doctor-Ansicht bleibt stehen. Der
offensichtlichste Rückweg reagiert nicht; der Nutzer sitzt fest.

Betroffen sind genau zwei Oberflächen: **Library Doctor** und die **Device-Sync-Seite**.
Alle echten Quellen-Reiter (Podcasts, YouTube, Radio, Queue, Releases, Concerts, My Stats,
Missing, Import errors) markieren und routen untereinander korrekt.

Symptom B ist der ärgerlichere Teil, und er ist der Grund, warum ein rein *visueller* Fix
nicht reicht: wer die Markierung nur woandershin malt, lässt den toten Klick stehen.

### 1.2 Die gemeinsame Wurzel (belegt)

Die Sidebar kennt genau **einen** Zustand, und der hängt an `ViewSource`:

* `Shared::current_source: RefCell<ViewSource>` — `sidebar/sidebar.rs:117-120`.
* `Shared::rows: RefCell<Vec<RowEntry>>` mit `RowEntry = (ListBoxRow, ViewSource, String)`
  — `sidebar/sidebar.rs:79`, `:121-124`.
* `route_row` sucht die Zeile in `shared.rows`, warnt bei Nichtfund und **verwirft die
  Navigation**, wenn `*shared.current_source.borrow() == source`
  („Same logical source as before … nothing to notify" → `return`) —
  `sidebar/sidebar_row_wiring.rs:61-91`, Warnung `:72`, Dedup `:75-80`.
* `wire_row_activated_on` ruft ausschließlich `route_row` plus `on_show_content` —
  `sidebar/sidebar_row_wiring.rs:134-155`. `on_show_content` schaltet nur im **kollabierten**
  Split-View die Content-Seite nach vorn (`window_navigation::show_content_callback`
  `ui/window/window_navigation.rs:97-119` → `activate_sidebar_route` `:70-79`) und fasst
  den `content_stack` überhaupt nicht an. Es hilft hier also nicht.
* `rebuild` markiert am Ende **immer** die Zeile von `current_source` neu —
  `sidebar/sidebar_rebuild.rs:404-435`.
* `wire_focus_leave_resync` schnappt die Markierung beim Fokusverlust zu `current_source`
  zurück — `sidebar/sidebar_row_wiring.rs:110-127`.

**Library Doctor ist keine Quelle.** Die Zeile entsteht über `add_issue_action_row`
(`sidebar/sidebar_rebuild.rs:382-390`), die explizit `row.set_selectable(false)` setzt
(`:608`) und die Zeile **nicht** in `shared.rows` einträgt (`:599-618` — kein
`rows.borrow_mut().push`, anders als `add_row` `:557-558` und `add_issue_row` `:573-576`).
Ein `ViewSource::LibraryDoctor` existiert nicht (`reprise-core/src/view_source.rs:19 ff.`).
Die Ansicht selbst ist ein `content_stack`-Kind namens `"library-doctor"`
(`ui/window/window.rs:261-262`).

**Das Öffnen des Doctors lässt `current_source` unangetastet.** `open_findings`
(`library_doctor/mod.rs:412-425`) und `open_for_selection` (`:427-435` — genau der
„Edit tag"-Weg) rufen nur `open_review()` (`:635`) bzw. `open_available()` → `open_root_page()`
→ `navigation.show_root()` (`:437-444`), und `DoctorNavigation::show_root`
(`library_doctor/navigation.rs:32-37`) schaltet lediglich den `content_stack` und die
`AdwNavigationView` um. Die Sidebar berührt der Doctor nur über
`self.sidebar.refresh(...)` (`library_doctor/mod.rs:631`) und `append_doctor_card`
(`:224`) — beides ändert `current_source` nicht, der Refresh markiert die alte Quelle sogar
**erneut**.

Der „Edit tag"-Einstieg aus My Stats ist belegt: `stats_view.set_on_unify_spellings(...)`
→ `library_doctor.open_for_selection(ids)` — `ui/window/window_runtime_wiring.rs:174-181`.

**Device Sync ist ebenfalls keine Quelle.** `window_navigation::open_device_place`
(`ui/window/window_navigation.rs:121-137`) schiebt die Seite per `device_sync_page::open`
in den `content_stack`; das Kind heißt `"device-sync"`
(`ui/device_sync/device_sync_page.rs:319-322`, sichtbar gemacht `:344`). Einstiegspunkt ist
die Gerätekarte — ein `gtk4::Overlay` mit einem `gtk4::Button` darin
(`sidebar/sidebar_device_card.rs:33-51`, `:73-83`, Klick `:217-220`), platziert im
`activity_slot` (`sidebar/sidebar_activity_slot.rs:48-53`), also gar kein `ListBoxRow`.

### 1.3 Warum aus einer Wurzel zwei Symptome werden

`current_source` ist gleichzeitig **zwei** Dinge:

1. *„Was ist markiert?"* — gelesen von `rebuild` (`sidebar_rebuild.rs:404`) und vom
   Fokus-Resync (`sidebar_row_wiring.rs:115`). Weil er beim Öffnen einer quellenlosen
   Ansicht stehen bleibt, wird die verlassene Quelle immer wieder neu markiert → **Symptom A**.
2. *„Was ist gerade aktiv, also nicht erneut zu routen?"* — der Dedup-Schlüssel in
   `route_row` (`:75-80`). Weil er stehen bleibt, hält die Dedup genau den markierten
   Eintrag für „schon aktiv" und verwirft den Klick → **Symptom B**.

Deshalb löst **ein** Mechanismus beides, aber nur, wenn er beide Rollen bedient:
ein Fix, der die Markierung visuell umhängt, aber die Dedup unverändert lässt, lässt den
toten Klick bestehen; ein Fix, der nur die Dedup entschärft, lässt die falsche Markierung
bestehen. Der gewählte Mechanismus (§2, E1/E2) ersetzt **den Schlüssel selbst**:
markiert wird nach dem *aktiven Ort*, und dedupliziert wird gegen den *aktiven Ort* —
dieselbe Größe, ein Begriff, beide Symptome.

Nebenbefund derselben Zeile: der tote Klick verletzt `docs/ux-rules.md` **BROWSE-3**
[active] [gtk] („Sidebar entries are absolute destinations. Every activation also leaves
utility pages and routes into the active target view", `docs/ux-rules.md:4579-4583`).

### 1.4 Das entscheidende Präzedenz-Muster im Repo

`ui/window/library_chrome.rs:70-94` löst **exakt dieselbe Aufgabe** bereits richtig, nur für
die Kopfzeile: `connect_visible_child_name_notify` auf dem `content_stack`, ein
`Rc<DoctorChrome>` als schwache Referenz, und `DoctorChrome::sync` (`:98-111`) liest
`content_stack.visible_child_name()` und schaltet Titel/Buttons um. Genau deshalb stimmt die
Kopfzeile im Doctor heute, während die Sidebar-Markierung nicht stimmt: der Sidebar fehlt
diese Anbindung. `library_doctor/navigation.rs:69-71` (`is_visible`) tut dasselbe ein
zweites Mal.

---

## 2. Entscheidungen (begründet, nicht vorab gesetzt)

### E1 — Ein gemeinsamer Zustand, keine zwei Sonderlocken

Neuer, expliziter Sidebar-Zustand `SidebarPlace` neben `current_source`:

```rust
pub(in crate::ui) enum SidebarPlace {
    /// Eine echte ViewSource ist sichtbar; die bestehende
    /// `current_source`-Markierung regiert unverändert.
    Source,
    /// Library Doctor ist sichtbar.
    LibraryDoctor,
    /// Die Device-Sync-Seite dieses Geräts ist sichtbar.
    Device(String),
    /// Eine quellenlose Seite ohne zuordenbaren Sidebar-Eintrag.
    /// Nichts wird markiert.
    Unknown,
}
```

Genau **eine** Funktion stellt die Markierung her — `sidebar_place::apply_marking(&Shared)`.
Sie ist der einzige Ort, der `select_row_in_its_listbox` / `unselect_all` / die
Geräte-Markierung anfasst. Aufgerufen wird sie von vier Stellen (Task 3): `rebuild`-Schwanz,
Fokus-Resync, `content_stack`-Notify, `restore_source`.

**`current_place` ersetzt `current_source` nicht** — es ergänzt es. `current_source` bleibt
Eingang von `find_row`/`restore_source`/`sync_current_source`/`prepare_history_reroute`
(`sidebar/sidebar_session.rs:11-59`) und behält seine Bedeutung „welche Quelle zeigt die
Trackliste".

### E2 — Der Dedup-Schlüssel wird `(place, source)`, **nicht** ein zurückgesetztes `current_source`

Das ist die Zeile, die Symptom B behebt. `route_row`s Dedup (`sidebar_row_wiring.rs:75-80`)
darf nur greifen, solange eine **Quelle** sichtbar ist:

```rust
if matches!(*shared.current_place.borrow(), SidebarPlace::Source)
    && *shared.current_source.borrow() == source
{
    return;
}
```

Gelesen: *dedupliziere nur, wenn der aktive Ort wirklich diese Quelle IST.* Solange Doctor
oder Device-Seite sichtbar sind, ist der aktive Ort **keine** Quelle — also routet jeder
Klick auf **jede** Quellenzeile, auch auf die zuletzt aktive. Jeder heutige Dedup-Fall
bleibt erhalten, weil der Normalzustand `Source` ist; die ausführlichen Doc-Kommentare
`sidebar_row_wiring.rs:13-24` und `:93-104` gelten unverändert weiter (sie begründen, warum
die Dedup ein *Wertvergleich* und kein Zeitfenster ist — daran ändert sich nichts, der
Vergleich bekommt nur eine zweite Komponente).

**Die verworfene Alternative: `current_source` beim Öffnen zurücksetzen.** Dafür gibt es
Vorarbeit — `sidebar_session.rs:36-59` mit `sync_current_source` und
`prepare_history_reroute`, dessen `reroute_baseline` (`:43-49`) *absichtlich einen falschen
Wert* einträgt, damit die Dedup den History-Rücksprung nicht schluckt. Der eigene
Doc-Kommentar dort (`:51-56`) beschreibt exakt unser Problem: „Re-baselining from that stale
source would deduplicate a history return to Library and leave the detail page visible."
Und der Test heißt `acc_5_history_reroute_cannot_be_deduplicated_as_the_target_source`
(`sidebar_session.rs:166-176`).

Das ist **eine Warnung, kein Vorbild.** Es wäre die dritte Wiederholung desselben
Sentinel-Kniffs (History, My Stats, jetzt Doctor/Device), und jede Wiederholung schreibt
absichtlich einen unwahren Wert in ein Feld, dessen Name etwas Wahres verspricht. Der
Schlüssel ist einfach zu grob — das gehört repariert, nicht ein drittes Mal umschifft.
Der `(place, source)`-Schlüssel macht `prepare_history_reroute` nicht sofort überflüssig
(es adressiert einen anderen Fall: eine Detailseite hat `current_source` auf `Library`
stehen lassen, obwohl der Ort eine Quelle **ist**), aber er verhindert, dass der Kniff ein
weiteres Mal kopiert wird. **Ausdrücklich nicht Teil dieses Plans:**
`prepare_history_reroute` zu entfernen. Das ist eine eigene Aufräumaufgabe mit eigener
Regressionslast.

### E2a — Die Umkehrung: kein doppeltes Navigieren, kein Zurücksetzen

Ein Klick auf die Doctor-Zeile oder die Gerätekarte, während deren Ansicht **schon**
sichtbar ist, darf nicht doppelt navigieren und die Ansicht nicht zurücksetzen. Das gilt
schon heute weitgehend und muss beim Umbau erhalten bleiben:

* **Doctor-Zeile.** Sie feuert `win.library-doctor-findings` (`sidebar_rebuild.rs:611-612`)
  → `open_findings` (`library_doctor/mod.rs:417-425`) → `open_review`
  (`:635-638`, kehrt ohne Scan sofort zurück) → `DoctorNavigation::show_review`
  (`navigation.rs:47-59`). Dessen erster Zweig behandelt genau diesen Fall: ist die
  angezeigte Review-Seite **dieselbe**, wird nur `pop_to_tag` gerufen, nicht erneut
  gepusht (`navigation.rs:50-52`, mit ausführlicher Begründung `:39-46`). `show_content`
  (`:73-81`) ist idempotent, weil `content_stack::show_page` bei gleichem Ziel dasselbe
  Kind erneut setzt. **Der neue Code darf hier nichts hinzufügen:** `route_row` kehrt für
  die Doctor-Zeile still zurück (E4) und ruft weder `on_select` noch `on_show_content`.
* **Gerätekarte.** `device_sync_page::open` baut die Seite jedes Mal neu und ersetzt das
  vorherige Stack-Kind (`device_sync_page.rs:319-322`). Ein zweiter Klick auf die schon
  offene Karte baut die Seite also neu auf — das ist **Bestandsverhalten**, unabhängig von
  diesem Fix, und wird hier **nicht** geändert. Der Fix darf es nur nicht verschlimmern:
  `apply_marking` markiert idempotent (`add_css_class` auf eine bereits gesetzte Klasse ist
  ein No-op) und löst keinen Öffnungsvorgang aus.

Beides wird in Task 6 mit je einem Test festgenagelt, damit der Umbau es nicht unbemerkt
kaputtmacht.

### E3 — Wahrheitsquelle: der sichtbare `content_stack`, plus ein Geräte-Hinweis

**Gewählt: am Stack.** Begründung, jede belegt:

1. Es gibt das Signal wirklich, und es wird im Repo für genau diese Klasse Problem bereits
   benutzt: `library_chrome.rs:89` und `:98-111`.
2. Alle Wege enden im selben Flaschenhals `content_stack::show_page`
   (`ui/window/content_stack.rs:30-51`, `set_visible_child_full` `:50`): Quellen über
   `library_shell.rs:219-269`, Doctor über `library_doctor/navigation.rs:73-81`, Device
   über `device_sync_page.rs:344`.
3. Es deckt Eintrittspunkte ab, die eine Aufrufer-Lösung vergessen würde. Der Doctor hat
   **drei**: die ISSUES-Zeile (`sidebar_rebuild.rs:388` → `primary_menu.rs:190-195` →
   `window_runtime_wiring.rs:258` → `open_findings`), das Kebab-Menü
   (`primary_menu.rs:183-188` → `window_runtime_wiring.rs:257` → `open`,
   `library_doctor/mod.rs:234-236`) und den „Edit tag"-Weg aus My Stats
   (`window_runtime_wiring.rs:174-181` → `open_for_selection`,
   `library_doctor/mod.rs:242-244`). Eine Lösung an den Aufrufern müsste alle drei
   anfassen und beim vierten wieder brechen; die Lösung am Stack deckt alle auf einmal ab.

**Die eine Lücke, ehrlich benannt:** der Kindname der Device-Seite ist die Konstante
`"device-sync"` **ohne** device_id (`device_sync_page.rs:322`); `open` entfernt sogar das
vorherige Kind gleichen Namens (`:319-321`). Der Stack allein kann also nicht sagen,
*welches* Gerät offen ist. Den Kindnamen um die id zu erweitern wäre invasiv (es bricht
`:319-321`, `window_navigation.rs:390-393` und `device_sync_page_tests.rs`) — deshalb:
**der Stack liefert die Art des Ortes, ein sidebar-lokaler Hinweis die device_id.**
Der Hinweis kostet keine neue Fernwirkung, weil die Sidebar die id ohnehin schon in der
Hand hält: die Karte ruft `on_open(id, name)` selbst auf
(`sidebar_device_card.rs:213-220`), und `Sidebar::bind_device_sync`
(`sidebar/sidebar.rs:460-467`) reicht dieses Callback durch. Wir wickeln es dort ein und
schreiben `Shared::open_device` vor dem Weiterreichen.

Der Resolver ist rein und ohne GTK testbar (Repo-Muster: `resolve_select_source`
`sidebar/sidebar.rs:532-541`, `transition_for_switch` `content_stack.rs:19-28`,
`reroute_baseline` `sidebar_session.rs:43-49`):

```rust
pub(in crate::ui) fn place_for_content_page(
    visible_child: Option<&str>,
    open_device: Option<&str>,
) -> SidebarPlace
```

`"library-doctor"` → `LibraryDoctor`; `"device-sync"` mit id → `Device(id)`;
`"device-sync"` ohne id → `Unknown` (+ `tracing::warn!`); **alles andere → `Source`**.
Begründung für den Default: die Menge der Quellseiten ist offen (jede neue `ViewSource`
bringt eine Seite mit), die Menge der quellenlosen Orte ist klein und bewusst geschlossen.
Gegen das Abdriften dieser Annahme steht Task 8 (Quelltext-Wächter).

### E4 — Doctor-Zeile: echte `ListBox`-Selektion, keine eigene CSS-Klasse

**Gewählt: echte Selektion** (`row.set_selectable(false)` in `sidebar_rebuild.rs:608`
entfällt).

* **Optik:** Die selektierte Zeile im GNOME-`navigation-sidebar` wird von libadwaitas
  eigenem Stylesheet gezeichnet. Im App-CSS gibt es **keine** `navigation-sidebar`-Regel
  (`app_css()` `ui/style/mod.rs:101-149`; die einzigen Treffer für `navigation-sidebar` im
  Quelltext sind `add_css_class`-Aufrufe: `sidebar/sidebar.rs:254`, `:471`,
  `preferences/preferences_window.rs:188`). Eine eigene Klasse müsste den
  libadwaita-Selektionslook nachbauen und bei jedem Adwaita-Update nachziehen —
  `sidebar_presentation.rs:11` dokumentiert genau diese Schuld bereits für die Geometrie.
* **A11y:** Die Zeile trägt schon `AccessibleRole::ListItem` als Konstruktor-Property
  (`sidebar_presentation.rs:355-362`), und `sidebar_presentation.rs:672-704` verbietet per
  Test nachträgliche Rollen-Setter im ganzen `sidebar/`-Baum. Echte Selektion in einer
  `SelectionMode::Single`-`ListBox` (`sidebar/sidebar.rs:472`) ist der von GTK selbst
  gepflegte Weg, „current" auszudrücken.
* **Routing:** `route_row` würde die Zeile nicht in `shared.rows` finden und warnen
  (`sidebar_row_wiring.rs:68-74`). Deshalb bekommt `Shared` das Feld
  `doctor_row: RefCell<Option<gtk4::ListBoxRow>>`, das `route_row` **vor** der
  Warnung konsultiert und bei Treffer still zurückkehrt (kein `current_source`-Wechsel,
  kein `on_select`, kein `on_show_content` — siehe E2a). Der ACC-8-Tastaturpfad
  (`row.connect_activate`, `sidebar_rebuild.rs:611-612`) und der Button-Pfad
  (`sidebar_presentation.rs:364-370` ruft `grab_focus()` + `activate()`) bleiben
  unverändert; die Zeile ist bereits `.activatable(true)` und `.focusable(true)`
  (`sidebar_presentation.rs:358-359`).
* **`a11y-semantics`-Vertrag:** Die Annotation `sidebar_rebuild.rs:609` wird von
  `scripts/check-accessibility-semantics.sh` maschinell geprüft — sie muss unmittelbar **vor** jedem
  `set_focusable(true)` stehen und dem Muster `role=... name=... state=... action=...`
  genügen. `state=` erlaubt `[a-z0-9._/-]+`, **kein `+`**. Neue Fassung deshalb z. B.
  `// a11y-semantics: role=list-item name=library-doctor state=focusable/selectable action=activate`.
  `row.set_focusable(true)` (`:610`) bleibt stehen, damit der Marker seinen Anker behält.

### E5 — Gerätekarte: eigene CSS-Klasse auf einer zweiten Achse + explizites AT-SPI-`Selected`

Die Karte ist kein `ListBoxRow`, echte Selektion ist ausgeschlossen. Repo-Präzedenz für
genau diese Kombination: `podcasts/podcasts_view_selection.rs:33-39` (CSS-Klasse **und**
`update_state(&[gtk4::accessible::State::Selected(Some(selected))])`), getestet mit
`gtk4::test_accessible_has_state(&row, gtk4::AccessibleState::Selected)`
(`podcasts/podcasts_view_tests.rs:137`).

**Koexistenz mit dem Fortschritt:** `DeviceCard::update` entfernt bei jedem
Fortschritts-Tick genau drei Klassen aus einer expliziten Liste
(`sidebar_device_card.rs:244-249`) und setzt dann eine davon (`:252-257`). Eine **vierte**
Klasse `device-card-current` überlebt diese Schleife unangetastet — das ist der ganze Trick
und wird mit einem eigenen Test festgenagelt (Task 5, Test 2). Achtung:
`device-card-active` heißt im Bestand *synchronisiert gerade*, **nicht** *offen*
(`sidebar_device_card_text.rs:108`, `sidebar_device_card.rs:252-261`) — der neue Name darf
damit nicht kollidieren.

**Farbaufteilung, damit sich beide Zustände nicht widersprechen (Grill-Beschluss 3).**
Achtung, der naheliegende Weg ist falsch: die drei Emphasis-Klassen besitzen **nicht** nur
`border-color`, sondern setzen selbst `background-color` (`sidebar_device_card_text.rs:107-108`).
Eine einzelne `.device-card-current`-Regel, die „zuletzt deklariert" gewinnt, nähme der
gerade synchronisierenden **und** geöffneten Karte ihre Akzentfläche — genau der Zustand,
in dem der Nutzer den Fehler gemeldet hat. Deshalb wird die Fläche des jeweiligen Zustands
über **höhere Spezifität verstärkt**, nicht ersetzt:

```
.device-card-current.device-card-remembered { background-color: alpha(@window_fg_color, 0.13); }
.device-card-current.device-card-connected  { background-color: alpha(@window_fg_color, 0.16); }
.device-card-current.device-card-active     { background-color: alpha(@accent_color, 0.20); }
```

Akzent bleibt Akzent, „offen" ist eine Stufe mehr Fläche. Hover-Varianten analog eine Stufe
darüber. Farbwerte kommen ausschließlich aus Theme-Tokens (`@window_fg_color`,
`@accent_color`, `@reprise_accent_text_color`) — nie als Literal; das ist im Modul bereits
als Vertrag dokumentiert (`sidebar_device_card.rs:464-466`). Die Alpha-Werte oben sind ein
Ausgangspunkt, kein Dogma.

Nur **eine** Karte trägt die Klasse: `apply_marking` reicht `Option<&str>` durch, und die
Device-Section setzt/entfernt sie für jede registrierte Karte.

### E6 — Räumen: beide `ListBox`en, immer über denselben Weg

Solange `current_place != Source` ist, gilt: `shared.listbox.unselect_all()` **und**
`shared.issues_listbox.unselect_all()`. Das ist schleifenfrei, weil `wire_row_selected_on`
bei `row == None` sofort zurückkehrt (`sidebar_row_wiring.rs:46-49`). Umgekehrt räumt das
Selektieren der Doctor-Zeile die Hauptliste automatisch, weil derselbe Handler die
Schwesterliste leert (`:50`).

Wenn `LibraryDoctor` aktiv ist, aber **keine** Doctor-Zeile existiert (der Zähler ist auf 0
gefallen, `doctor_issue_visible` `sidebar_rebuild.rs:438-440`, Sichtbarkeitsgate `:382`;
oder der Doctor wurde über Kebab-Menü bzw. „Edit tag" ohne offene Findings geöffnet),
bleibt **nichts** markiert. Das ist dieselbe Haltung, die `rebuild` für Scope-Ansichten
bereits einnimmt („leaving the selection empty", `sidebar_rebuild.rs:405-418`) — nicht neu
erfunden. Wichtig: der tote Klick ist auch in diesem Zustand behoben, weil E2 am *Ort*
hängt und nicht daran, ob eine Zeile markiert ist.

---

## 3. Erwartete Berührungspunkte

Das ist eine **Landkarte, keine Zwangsjacke.** Codex darf angrenzende Dateien anfassen, wenn
die Umsetzung es verlangt (Re-Exports, Sichtbarkeiten, Testfixtures, Nachziehen von
Zeilennummern in Kommentaren). Was nicht verhandelbar ist, sind die Gates in §5.

| Datei | Was | Zeilen heute / Limit |
|---|---|---|
| `sidebar/sidebar_place.rs` **(neu)** | `SidebarPlace`, `place_for_content_page`, `apply_marking`, `bind_content_stack` | 0 / 800 |
| `sidebar/sidebar_place_tests.rs` **(neu)** | alle neuen Tests | 0 / 800 |
| `sidebar/sidebar.rs` | 3–4 `Shared`-Felder + Initialisierung; **Entlastung nötig** | **583 / 600** |
| `sidebar/sidebar_rebuild.rs` | Doctor-Zeile selektierbar + `doctor_row`; Rebuild-Schwanz | 624 / 800 |
| `sidebar/sidebar_row_wiring.rs` | `route_row`-Dedup + `doctor_row`-Zweig; Fokus-Resync | 155 / 800 |
| `sidebar/sidebar_session.rs` | `restore_source` setzt `current_place = Source` | 177 / 800 |
| `sidebar/sidebar_device_card.rs` | `set_current`, AT-SPI-`Selected` | 593 / 800 |
| `sidebar/sidebar_device_card_text.rs` | `.device-card-current` CSS | 441 / 800 |
| `sidebar/sidebar_device_section.rs` | Markierungs-Handle, Neuanwendung in `render` | 297 / 800 |
| `ui/window/library_shell.rs` | **eine Zeile**: `bind_content_stack` | 543 / 800 |
| `docs/ux-rules.md` | NAV-18 | — |

### 3.1 Größenregeln — zwei harte Klippen

`scripts/check-architecture.sh:20` (< 800 Zeilen für **jede** `.rs` unter `crates/`, Tests eingeschlossen) und
`:32-38` (`sidebar/sidebar.rs` ist ein UI-Orchestrator, **< 600**).

* **`sidebar/sidebar.rs` = 583/600.** Die neuen `Shared`-Felder mit Doc-Kommentaren plus
  Initialisierung passen rechnerisch, lassen aber praktisch keine Luft.
  **Pflicht-Auslagerung, im selben Task:** `select_row_in_its_listbox` (`:510-521`),
  `resolve_select_source` (`:523-541`), `has_sidebar_row` (`:543-553`) und `find_row`
  (`:555-567`) wandern nach `sidebar_place.rs` und werden aus `sidebar/mod.rs` (`:27-30`)
  unverändert re-exportiert. Das sind ~58 Zeilen Entlastung und thematisch genau der neue
  Ort („wer ist markiert"). Die Testmodule `sidebar_tests.rs` / `sidebar_layout_tests.rs`
  greifen über `use super::*` (`sidebar_tests.rs:2`) darauf zu — der Re-Export in `mod.rs`
  hält das am Leben.
* **`sidebar/sidebar_tests.rs` = 791/800.** Dort darf **kein** neuer Test hinzukommen. Die
  `Shared`-Literal-Erweiterung in `test_shared` (`sidebar_tests.rs:9-36`) kostet 3–4 Zeilen
  (794–795) — noch tragbar; wenn es enger wird, gehört `test_shared` nach
  `sidebar_place.rs` unter `#[cfg(test)]` und wird von dort re-exportiert.
* **`sidebar/sidebar_presentation.rs` = 793/800.** Nicht anfassen.
* **`ui/window/window.rs` = 599/600.** **Null Luft.** Deshalb wird `bind_content_stack`
  nicht dort verdrahtet, sondern in `library_shell::wire_source_routing`
  (`library_shell.rs:158-175`) — die Funktion hat `sidebar: &Rc<Sidebar>` (`:159`) **und**
  `content_stack: &gtk4::Stack` (`:170`) bereits in der Signatur, und die Datei hat 257
  Zeilen Luft.

---

## 4. Tasks

Reihenfolge ist Abhängigkeitsreihenfolge. Jede Task ist einzeln grün zu bekommen.

---

### Task 1 — `SidebarPlace` und der reine Resolver

**Was.** Neue Datei `crates/reprise-gnome/src/ui/sidebar/sidebar_place.rs`; Anmeldung in
`sidebar/mod.rs` (neben `:1-22`). Enthält `SidebarPlace` (E1), `place_for_content_page` (E3)
und die Konstanten für die beiden Stack-Kindnamen. Die Namen werden **nicht** neu erfunden:
`LIBRARY_DOCTOR_PAGE` spiegelt `window.rs:262` bzw. `library_doctor/navigation.rs:7`
(`ROOT_TAG`), `DEVICE_SYNC_PAGE` spiegelt `device_sync_page.rs:322`. Wo möglich die
bestehenden Konstanten wiederverwenden statt Literale zu duplizieren.

Zusätzlich in derselben Task die Entlastungs-Umzüge aus §3.1
(`select_row_in_its_listbox`, `resolve_select_source`, `has_sidebar_row`, `find_row`)
inklusive Re-Export in `sidebar/mod.rs:27-30`. Der Testmodul-Anhang `sidebar.rs:569-583`
bleibt, wo er ist; `sidebar_tests.rs:640-697` testet `resolve_select_source` weiter über
`use super::*`.

**Warum.** Ein reiner, GTK-freier Kern ist im Repo die Konvention für jede
Entscheidungslogik (`sidebar.rs:523-531` begründet das ausdrücklich für
`resolve_select_source`), und er ist der einzige Teil, der ohne Display prüfbar ist. Der
Umzug schafft gleichzeitig den Platz, den Task 2 in `sidebar.rs` braucht.

**Verifikation.**

```bash
cd /home/marvin/Projects/reprise
cargo test -p reprise-gnome --bin reprise nav_18_ 2>&1 | grep -E '^test result:'
scripts/check-architecture.sh
```

Erwartete Beobachtung: `test result: ok. 1 passed;` für
`nav_18_only_the_two_placeless_pages_leave_the_source_marking`, und der Arch-Lint meldet
`sidebar/sidebar.rs` **unter** 600 sowie keine Datei >= 800.

Der Test (in `sidebar_place_tests.rs`, ohne `#[ignore]`, weil rein):

```
place_for_content_page(Some("library"),        None)          == Source
place_for_content_page(Some("stats"),          None)          == Source
place_for_content_page(Some("podcasts"),       None)          == Source
place_for_content_page(None,                   None)          == Source
place_for_content_page(Some("library-doctor"), None)          == LibraryDoctor
place_for_content_page(Some("device-sync"),    Some("pixel")) == Device("pixel")
place_for_content_page(Some("device-sync"),    None)          == Unknown
```

**Kontrollarm.** Den Match-Arm `"library-doctor" => SidebarPlace::LibraryDoctor` auf
`SidebarPlace::Source` zurückrollen. Erwartung: `^test result: FAILED`. Ohne diesen Nachweis
zählt Grün nicht.

---

### Task 2 — Zustand in `Shared`, und **eine** Funktion, die markiert

**Was.** In `sidebar/sidebar.rs` (`Shared`, `:93-214`) die Felder ergänzen, jeweils mit
Doc-Kommentar im Stil der Nachbarn:

```rust
pub(in crate::ui) current_place: RefCell<SidebarPlace>,
pub(in crate::ui) doctor_row: RefCell<Option<gtk4::ListBoxRow>>,
pub(in crate::ui) open_device: RefCell<Option<String>>,
pub(in crate::ui) mark_device: RefCell<Option<Rc<dyn Fn(Option<&str>)>>>,
```

Initialisierung in `Sidebar::build` (`sidebar.rs:268-292`) und in
`sidebar_tests.rs::test_shared` (`:9-36`). Wenn der Platz in `sidebar.rs` knapp wird, ist
`mark_device` der erste Kandidat für einen eigenen kleinen Halter in `sidebar_place.rs`.

In `sidebar_place.rs` dann `apply_marking(shared: &Rc<Shared>)`:

* `Source` → bisheriges Verhalten: die Zeile zu `current_source` selektieren, mit
  `has_sidebar_row`/`resolve_select_source`-Fallback wie in `sidebar_rebuild.rs:404-435`
  (der Code zieht dorthin um bzw. wird von dort aufgerufen).
* `LibraryDoctor` → beide Listen leeren, dann die Zeile aus `doctor_row` mit
  `SidebarPlace::LibraryDoctor` selektieren, falls vorhanden.
* `Device(id)` → beide Listen leeren, `mark_device`-Callback mit `Some(id)` rufen.
* `Unknown` → beide Listen leeren, `mark_device` mit `None`.
* In **jedem** Nicht-`Device`-Zweig `mark_device(None)` rufen, damit keine Karte markiert
  bleibt, wenn der Nutzer vom Gerät zu einer Quelle zurückgeht.

**`RefCell`-Disziplin.** Jeder Borrow endet, bevor irgendein Callback oder eine
GTK-Selektion läuft — die Regel steht als Modul-Doc in `sidebar.rs:38-50` (`## Reentrancy`)
und wird an drei Stellen im Bestand vorgemacht (`sidebar_row_wiring.rs:83-90`, `:146-152`;
`sidebar_rebuild.rs:592-593`). Konkret: `current_place` und `current_source`
heraus**klonen**, Borrow fallen lassen, dann handeln.

**Warum.** Ohne einen einzigen Ort, der markiert, entstehen wieder drei Pfade, die einander
überschreiben — genau der heutige Fehler.

**Verifikation.**

```bash
cargo test -p reprise-gnome --bin reprise nav_18_ 2>&1 | grep -E '^test result:'
cargo clippy --all-targets --workspace -- -D warnings
scripts/check-architecture.sh
```

Erwartete Beobachtung: Task-1-Test weiterhin grün, Clippy sauber, `sidebar.rs` < 600. Eigene
Assertions hat diese Task noch nicht — sie wird von Task 3–6 gemessen. Das ist bewusst so
und **kein** Freibrief: wenn Task 3 rot ist, ist Task 2 nicht fertig.

**Kontrollarm.** Entfällt (reine Struktur-Task, kein Verhaltensanspruch). Der Kontrollarm für
den Zustand steckt in Task 3 und 4.

---

### Task 3 — `rebuild`, Fokus-Resync und `restore_source` respektieren den Ort (Symptom A)

**Was.**

1. `sidebar_rebuild.rs`: `doctor_row` in der Aufräumphase leeren, direkt neben
   `shared.rows.borrow_mut().clear()` (`:238`).
2. `sidebar_rebuild.rs:404-435`: Der Schwanz ruft `sidebar_place::apply_marking(shared)`.
   Die bestehende Scope-/Fallback-Logik (`:405-418`, `:419-435`) wandert **unverändert** in
   `apply_marking`s `Source`-Zweig — kein Verhaltenswechsel für Quellen, nur ein Umzug.
3. `sidebar_row_wiring.rs:110-127` (`wire_focus_leave_resync`): statt `find_row` +
   `current_source` ruft der `connect_leave`-Handler `apply_marking`. Die Doc-Begründung
   („snap the visual selection back to the source that is actually shown", `:105-109`) wird
   auf „zurück zu dem **Ort**, der tatsächlich sichtbar ist" umgeschrieben. Der Loop-Schutz
   bleibt derselbe: erneutes Selektieren dedupliziert in `route_row` (`:120-122`).
4. `sidebar_session.rs:11-29` (`restore_source`): setzt zusätzlich
   `current_place = SidebarPlace::Source`, bevor es selektiert. Sitzungswiederherstellung
   stellt immer eine Quelle her.
5. `sidebar_rebuild.rs:599-618` (`add_issue_action_row`): `row.set_selectable(false)`
   (`:608`) entfällt; die Zeile wird in `doctor_row` eingetragen; die `a11y-semantics`-Zeile
   (`:609`) bekommt `state=focusable/selectable` (E4).

**Warum.** Das sind genau die drei Pfade, die die Markierung heute zurückreißen — und der
Rebuild ist der schlimmste, weil der Doctor selbst laufend welche auslöst
(`library_doctor/mod.rs:631`, `library_shell.rs:210-213`, `sidebar.rs:406-408`).

**Verifikation.** Zwei Display-Tests in `sidebar_place_tests.rs`:

* `nav_18_the_doctor_page_marks_the_doctor_row_and_no_source_row`
* `nav_18_a_rebuild_while_the_doctor_is_visible_does_not_take_the_marking_back`

Aufbau beider (Muster aus `sidebar_tests.rs:390-446` und `library_chrome_tests.rs:80-100`):

```
let _main_context = crate::ui::test_main_context::lock_main_context();
gtk4::init().unwrap();
crate::ui::style::install();                    // App-CSS! siehe 5.2
let shared = test_shared();                     // sidebar_tests.rs:9
// eine offene Doctor-Findung in die In-Memory-DB schreiben, damit
// doctor_issue_visible(pending) wahr ist (sidebar_rebuild.rs:93-97, :438-440)
wire_row_selected(&shared); wire_row_activated(&shared); wire_focus_leave_resync(&shared);
rebuild(&shared, Some(ViewSource::MyStats), "test build");
let stack = gtk4::Stack::new();
stack.add_named(&gtk4::Label::new(Some("Stats")),  Some("stats"));
stack.add_named(&gtk4::Label::new(Some("Doctor")), Some("library-doctor"));
stack.set_visible_child_name("stats");
bind_content_stack(&shared, &stack);            // Task 4
// Fenster mit beiden ListBoxen praesentieren, dann:
crate::ui::window::content_stack::show_page(&stack, "library-doctor");
while gtk4::glib::MainContext::default().iteration(false) {}
```

`ViewSource::MyStats` als Ausgangsquelle ist bewusst gewählt: es ist genau der Fall, den der
Nutzer gemeldet hat.

Assertions Test 1: `issues_listbox.selected_row()` ist die Doctor-Zeile (aus `doctor_row`
geholt, **nicht** über den Index), und `listbox.selected_row().is_none()` — insbesondere ist
die My-Stats-Zeile nicht markiert.

Assertions Test 2 (der beweiskräftige): danach zusätzlich
`rebuild(&shared, None, "counts refresh");` + Pump, dann **erneut** dieselben Assertions
gegen die **neu gebaute** Doctor-Zeile (Zeilen-Identität wechselt bei jedem Rebuild —
`sidebar.rs:11-22`).

Befehl (Einzeltest, zweistufig, weil `--exact` den **vollen** Pfad braucht):

```bash
cd /home/marvin/Projects/reprise
cargo test -p reprise-gnome -- --ignored --list | sed -n 's/: test$//p' | grep nav_18
# Dann fuer jeden gefundenen vollen Namen:
env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
    XDG_CONFIG_HOME=$(mktemp -d) XDG_RUNTIME_DIR=$(mktemp -d) TMPDIR=$(mktemp -d) \
    GIO_USE_VFS=local GTK_USE_PORTAL=0 GSK_RENDERER=cairo GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  dbus-run-session -- xvfb-run --server-num=97 \
  cargo test -p reprise-gnome \
    'ui::sidebar::sidebar_place::tests::nav_18_a_rebuild_while_the_doctor_is_visible_does_not_take_the_marking_back' \
    -- --ignored --exact 2>&1 | grep -E '^test result:'
```

Erwartete Beobachtung: **`test result: ok. 1 passed;`**. `0 passed` bedeutet, dass `--exact`
ins Leere lief (falscher Pfad) — das zählt als Fehlschlag, exakt wie `scripts/check-display-tests.sh` es
prüft (dort: `grep -Ec "test result: ok\. 1 passed;"`). Beurteilt wird ausschließlich über
`^test result:`, nie über Bilanzzeilen.

**Kontrollarm.**
* Test 1: `row.set_selectable(false)` in `sidebar_rebuild.rs:608` wieder einsetzen →
  `^test result: FAILED` (die Zeile lässt sich nicht selektieren).
* Test 2: im `rebuild`-Schwanz den `apply_marking`-Aufruf durch den alten Block
  `sidebar_rebuild.rs:419-435` ersetzen → `^test result: FAILED` (My Stats wird
  nachmarkiert).

---

### Task 4 — Die Wahrheitsquelle anbinden und den toten Klick beheben (Symptom B)

**Was.**

1. `sidebar_place.rs`: `Sidebar::bind_content_stack(&self, stack: &gtk4::Stack)` als
   `impl`-Block außerhalb von `sidebar.rs` — dafür gibt es das ausdrückliche Präzedenz
   `sidebar_session.rs:61-63` („relocated from `sidebar.rs` (orchestrator size rule)").
   Inhalt: schwache `Rc<Shared>`-Referenz, `connect_visible_child_name_notify` (Muster
   `library_chrome.rs:88-93`), Ort neu berechnen, `apply_marking` rufen — **plus einmal
   sofort** beim Verdrahten, damit der Startzustand stimmt (`DoctorChrome` macht genau das:
   `library_chrome.rs:86`).
2. `library_shell.rs:158-175`: **eine** Zeile `sidebar.bind_content_stack(content_stack);`
   im Rumpf von `wire_source_routing`. Nicht in `window.rs` — dort ist kein Platz (§3.1).
3. `route_row` (`sidebar_row_wiring.rs:61-91`):
   * vor der Warnung `:72` in `doctor_row` nachschlagen; Treffer ⇒ still `return`
     (kein `current_source`-Wechsel, kein `on_select` — E2a);
   * Dedup nach E2 auf `(place, source)` erweitern (`:75-80`);
   * **unmittelbar nach** dem `on_select`-Aufruf **denselben Rechenweg** anstoßen, den auch
     das Stack-Notify benutzt — eine Funktion `sync_place_from_stack(&Shared)`, die den
     sichtbaren Kindnamen liest, `place_for_content_page` anwendet, `current_place`
     schreibt und `apply_marking` ruft. `route_row` **rät den Wert nicht selbst** und
     schreibt `current_place` nirgends direkt (Grill-Beschluss 4).
     **Begründung:** zwei unabhängige Schreiber auf denselben Zustand sind genau die
     Drift, aus der dieser Bug entstanden ist; ein Wert, der an zwei Stellen berechnet
     wird, ist zwei Werte. Mit einem einzigen Rechenweg ist es gleichgültig, ob GTK
     `notify::visible-child-name` synchron aus `set_visible_child_full`
     (`content_stack.rs:50`) emittiert — die Frage stellt sich nicht mehr, weil beide
     Auslöser dasselbe Ergebnis erzeugen. Der Aufruf ist idempotent; ein zusätzlich
     feuerndes Notify rechnet dasselbe noch einmal aus.
     Dafür muss der Stack in `Shared` erreichbar sein (schwache Referenz, gesetzt von
     `bind_content_stack`) — ist er es nicht (Tests ohne Stack, früher Start), bleibt
     `current_place` unverändert und `apply_marking` läuft trotzdem.
4. Den Doc-Kommentar über `route_row` (`sidebar_row_wiring.rs:58-60`) und über
   `wire_row_selected` (`:13-24`) um den zweiten Schlüsselteil ergänzen — die Dedup ist ab
   jetzt „gleiche Quelle **und** der Ort ist diese Quelle".

**Warum.** Ohne die Anbindung kennt die Sidebar den Ort nie (Symptom A bleibt); ohne die
erweiterte Dedup verwirft `route_row` weiter jeden Klick auf die zuletzt aktive Quelle
(Symptom B bleibt). Beide hängen an derselben Größe — das ist der ganze Punkt aus §1.3.

**Verifikation.** Der Kern-Test für den toten Klick, **zweimal**, für beide Einstiege:

* `nav_18_activating_the_marked_source_from_the_doctor_findings_routes_back`
  — Doctor über die ISSUES-Zeile (`open_findings`-Pfad).
* `nav_18_activating_the_marked_source_from_a_doctor_selection_routes_back`
  — Doctor über „Edit tag" aus einer Auswahl (`open_for_selection`-Pfad,
  `window_runtime_wiring.rs:174-181`). Das ist der Weg, auf dem der Nutzer es gemeldet hat.

Aufbau wie Task 3 (Ausgangsquelle `ViewSource::MyStats`), zusätzlich `on_select`-Recorder
(Muster `sidebar_tests.rs:26-32`) und `on_show_content`-Zähler (`:33-37`). Der `on_select`
des Tests schaltet den Stack wie die Produktion:
`crate::ui::window::content_stack::show_page(&stack, "stats")` — sonst misst der Test nur
den Callback und nicht die Ansicht.

Ablauf: Doctor sichtbar machen, dann
`find_row(&shared, &ViewSource::MyStats).unwrap().emit_by_name::<()>("activate", &[])`
(Muster `sidebar_layout_tests.rs:44`), pumpen.

Erwartete Beobachtung, alle drei zusammen:
1. `stack.visible_child_name().as_deref() == Some("stats")` — **die Ansicht ist zurück**;
2. der Recorder enthält `[ViewSource::MyStats]`, **obwohl `current_source` schon `MyStats`
   war**;
3. `listbox.selected_row()` ist die My-Stats-Zeile und `issues_listbox.selected_row().is_none()`.

Der Unterschied der beiden Tests liegt allein im Einstieg: der eine aktiviert die
Doctor-Zeile (`issues_listbox`-Zeile aus `doctor_row`, `activate()`), der andere ruft den
`open_for_selection`-Pfad nach. Weil die Wahrheitsquelle am Stack hängt (E3), reduziert sich
der zweite auf „Stack auf `library-doctor` schalten, ohne die Sidebar anzufassen" — genau das
tut `library_doctor/navigation.rs:73-81` in Produktion. Wenn ein Test dafür den echten
Coordinator braucht, ist `library_doctor/tests.rs:160-165` die Stelle, an der dieses Wissen
schon steckt; sonst genügt der Stack-Schalter, und der Test dokumentiert im Kommentar,
welchen Produktionspfad er nachbildet.

Befehl: wie Task 3, mit den vollen Pfaden dieser Tests.

**Kontrollarm (Pflicht, beide Tests).** Die Dedup-Bedingung in `route_row` auf die alte Form
`if *shared.current_source.borrow() == source { return; }` zurückrollen →
`^test result: FAILED`. Ohne den Fix kehrt `route_row` früh zurück: der Recorder bleibt leer
und `stack.visible_child_name()` bleibt `"library-doctor"`. Genau das ist die gemeldete
Beschwerde, in einem Test eingefangen.

**Zusatztest (Fokus, gehört fachlich hierher):**
`nav_18_focus_leaving_the_sidebar_does_not_snap_the_marking_back_to_a_source` — bei
sichtbarem Doctor eine Quellenzeile fokussieren (fokusgetriebene Selektion **ohne** Commit,
`sidebar_row_wiring.rs:37-42`), dann Fokus aus beiden Listen bewegen, pumpen. Erwartung: die
Doctor-Zeile ist wieder markiert, die Quelle nicht. Braucht dasselbe
`settle_until_active`-Warten wie `sidebar_tests.rs:487-534` (dort `:513-518` erklärt warum:
`has_focus()` verlangt ein aktives Toplevel, X liefert die Aktivierung verzögert).
**Kontrollarm:** den alten Rumpf von `wire_focus_leave_resync` (`:115-123`) wiederherstellen
→ `^test result: FAILED`.

---

### Task 5 — Die Gerätekarte markieren

**Was.**

1. `sidebar_device_card_text.rs` (`css()`, ab `:104`): Regel `.device-card-current`
   (+ `:hover`) **nach** den drei Emphasis-Regeln (`:106-108`) einfügen, nur Theme-Tokens (E5).
2. `sidebar_device_card.rs`: `pub(super) fn set_current(&self, current: bool)` — setzt/
   entfernt die Klasse auf `self.surface` und ruft
   `self.surface.update_state(&[gtk4::accessible::State::Selected(Some(current))])`
   (Muster `podcasts/podcasts_view_selection.rs:39`). **Nicht** in die Entfernungsliste von
   `update` (`:245-249`) aufnehmen — das ist der Koexistenz-Mechanismus.
3. `sidebar_device_section.rs`: `DeviceSection` merkt sich
   `current_id: RefCell<Option<String>>`; eine Funktion wendet sie auf die ganze
   `CardRegistry` (`:129`, `:187-219`) an. `render` (`:177-220`) wendet sie am Ende erneut
   an, damit eine **später** gebaute Karte (`:199-208`) die Markierung bekommt. `bind`
   (`:125-137`) gibt ein `Rc<dyn Fn(Option<&str>)>` zurück (oder nimmt `&Rc<Shared>`
   entgegen und schreibt `mark_device` selbst).
4. `Sidebar::bind_device_sync` (`sidebar.rs:460-467`): das `on_open`-Callback so
   durchreichen, dass `Shared::open_device` **vor** dem Weiterreichen gesetzt wird. Die Logik
   dafür gehört wegen §3.1 nach `sidebar_device_section.rs` oder `sidebar_place.rs`; in
   `sidebar.rs` bleiben nur die vorhandenen Zeilen.

**Warum.** Die Karte ist der einzige Sidebar-Eintrag ohne `ListBoxRow`; ohne eigene Achse für
„offen" würde jeder Fortschritts-Tick (`DeviceCard::update`, aufgerufen aus `render` `:199`
bei jeder `notify` des Runtimes) die Markierung löschen.

**Verifikation.** Drei Display-Tests:

* `nav_18_only_the_open_device_card_is_marked` — zwei `DeviceView`s
  (`sidebar_device_card.rs:555-588` liefert die Fixture `view(...)`, die zweite mit anderer
  `id`), Ort auf `Device("pixel")` setzen, dann: Karte A hat `device-card-current` und
  `gtk4::test_accessible_has_state(&surface, gtk4::AccessibleState::Selected)`; Karte B hat
  beides nicht; `listbox.selected_row().is_none()` **und**
  `issues_listbox.selected_row().is_none()`.
* `nav_18_a_sync_progress_update_keeps_the_open_device_marked` — nach dem Markieren
  `card.update(&view(PlannedSyncPhase::Syncing { .. }))` aufrufen und erneut prüfen;
  zusätzlich `has_css_class("device-card-active")`, damit bewiesen ist, dass **beide**
  Zustände nebeneinander bestehen.
* `nav_18_activating_a_source_from_the_device_page_routes_back_and_unmarks_the_card` —
  derselbe tote-Klick-Beweis wie Task 4, nur mit `"device-sync"` als sichtbarem Kind:
  Quelle aktivieren ⇒ `stack.visible_child_name() == Some("library")`, Quellenzeile markiert,
  **keine** Karte trägt mehr `device-card-current`.

Befehle wie Task 3. Zusätzlich muss der CSS-Parser-Wächter grün sein:

```bash
scripts/check-display-tests.sh --css 2>&1 | tail -5
```

Erwartete Beobachtung: `failed: 0 of N`.

**Kontrollarm.**
* Test 1: den `mark_device`-Aufruf im `Device`-Zweig von `apply_marking` entfernen →
  `^test result: FAILED`.
* Test 2: `"device-card-current"` in die Entfernungsliste `sidebar_device_card.rs:245-249`
  aufnehmen → `^test result: FAILED`. Das ist der Kontrollarm, der beweist, dass der Test
  wirklich die Koexistenz misst und nicht nur „die Klasse ist irgendwann mal gesetzt".
* Test 3: die Dedup-Bedingung in `route_row` zurückrollen (wie Task 4) →
  `^test result: FAILED`.

---

### Task 6 — Die Umkehrung, die verschwindende Doctor-Zeile, der kollabierte Fall

**Was.**

1. **Kein Doppel-Navigieren (E2a).** Beweisen, dass das Aktivieren der Doctor-Zeile bei
   bereits sichtbarem Doctor die Ansicht nicht zurücksetzt und nicht doppelt navigiert.
2. **Verschwindende Doctor-Zeile.** Beweisen, dass E6 hält, wenn `pending_doctor_count` auf
   0 fällt, während der Doctor sichtbar ist (`sidebar_rebuild.rs:382`, `:438-440`) — und
   dass der Rückweg trotzdem funktioniert (E2 hängt am Ort, nicht an einer markierten Zeile).
3. **Kollabierter Split-View:** `open_device_place` schließt das Sidebar-Overlay
   (`window_navigation.rs:133-135`), der Doctor-Weg tut das **nicht**
   (`library_doctor/navigation.rs:73-81` ruft nur `pop_to_page` + `show_page`). Das ist eine
   Bestandslücke gegen `activate_sidebar_route` (`window_navigation.rs:70-79`) und sollte
   hier mit erledigt werden: der Doctor-Öffnungsweg bekommt denselben
   `split_view.is_collapsed()`-Zweig. Falls das den Zuschnitt sprengt, ist es ein **eigener
   Commit** — dann gehört ein Satz dazu, dass die Markierung davon unberührt ist (sie hängt
   am Stack, nicht am Overlay).

**Warum.** Der Zähler fällt real auf 0 (Findings werden angewandt oder verworfen), und zwar
genau dann, wenn der Nutzer im Doctor steht — `acknowledge_scan`
(`library_doctor/mod.rs:622-633`) tut es und ruft im selben Atemzug
`sidebar.refresh(...)` (`:631`). Ohne diesen Test bewiese nichts, dass dann nicht wieder die
alte Quelle aufleuchtet. Und ohne den Umkehr-Test kann der Umbau unbemerkt eine
Doppelnavigation einbauen.

**Verifikation.** Drei Display-Tests:

* `nav_18_activating_the_doctor_row_while_the_doctor_is_visible_changes_nothing` —
  Doctor sichtbar und markiert; die Doctor-Zeile aktivieren; pumpen. Erwartung:
  `on_select`-Recorder **leer**, `on_show_content`-Zähler unverändert,
  `stack.visible_child_name() == Some("library-doctor")`, Doctor-Zeile weiter markiert.
* `nav_18_the_vanishing_doctor_row_leaves_nothing_marked_instead_of_the_old_source` —
  Doctor sichtbar und markiert; die offenen Findings in der In-Memory-DB entfernen;
  `rebuild(&shared, None, "findings applied")`; pumpen. Erwartung:
  `listbox.selected_row().is_none()` **und** `issues_listbox.selected_row().is_none()`;
  danach die Quellenzeile aktivieren ⇒ `stack.visible_child_name() == Some("stats")`
  (der Rückweg funktioniert auch ohne markierte Doctor-Zeile).
* Falls Punkt 3 hier erledigt wird: Display-Test analog zu
  `window_navigation.rs:344-394` (`opening_a_device_from_a_pushed_page_shows_the_device_page`)
  mit `split.set_collapsed(true)` und Erwartung `!split.shows_sidebar()`.

**Kontrollarm.**
* Test 1: in `route_row` den `doctor_row`-Zweig entfernen, sodass die Doctor-Zeile in die
  normale Routing-Logik fällt → `^test result: FAILED` (Warnung plus unerwünschtes Routing).
* Test 2: im `LibraryDoctor`-Zweig von `apply_marking` bei fehlender Zeile auf den
  `Source`-Zweig zurückfallen lassen → `^test result: FAILED` (die alte Quelle wird markiert).
* Test 3: den neuen `is_collapsed()`-Zweig entfernen → `^test result: FAILED`.

---

### Task 7 — Regelwerk: NAV-18

**Was.** In `docs/ux-rules.md`, Abschnitt **B. Navigation model**, nach `NAV-17` (`:263 ff.`),
eine neue Regel (englisch, wie das ganze Dokument):

> **NAV-18** [active] [gtk] — **The sidebar marks the visible view, and the marked entry
> stays clickable.** Exactly the sidebar entry whose view is visible in the content area
> carries the marking — including Library Doctor and the opened device card, neither of which
> is a `ViewSource`. At most one entry is marked at any time across both navigation lists and
> the device cards. When the visible view has no sidebar entry, nothing is marked. A sidebar
> rebuild never changes the marking. While a placeless view is visible, activating **any**
> source entry routes into it — including the source that was last visible (BROWSE-3);
> activating the entry of the already visible placeless view does nothing.

Achtung, zwei Regeln des Repos gelten hier gleichzeitig:
* `AGENTS.md` („UX rules are binding"): eine Regel wechselt `[planned]` → `[active]` **im
  selben Commit**, der das Verhalten implementiert und den Test hinzufügt. Deshalb wird
  NAV-18 direkt `[active]` eingetragen — zusammen mit den Tests aus Task 3–6.
* Der Ownership-Vorbehalt aus `AGENTS.md:172-180` (Flathub-Strang A besitzt
  `docs/ux-rules.md`) ist **erloschen** und gilt hier nicht: es gibt keinen Flathub-Plan
  mehr unter `docs/plans/`, und die Datei wurde zuletzt am 2026-08-13 aus einem
  gewöhnlichen Feature-PR (#461) geändert. Die Rückfallvariante `[planned]` +
  `<!-- REVIEW: rule proposal -->` entfällt (Grill-Beschluss 6).

**Warum.** `scripts/check-ux-traceability.sh` verlangt für jede `[active]`-Regel mindestens einen Test, dessen Name
die ID trägt (Richtung 1); umgekehrt ist ein Test, der eine unbekannte oder ersetzte ID
nennt, ein Fehler (Richtung 2). Der Display-Ignore-Marker
`#[ignore = "requires a display; run via xvfb-run"]` ist ausdrücklich für **jeden**
Regelstatus erlaubt (Richtung 3) — genau diese Zeichenkette benutzen, keine andere.

**Verifikation.**

```bash
scripts/check-ux-traceability.sh
scripts/check-display-tests.sh --rule-named 2>&1 | tail -5
```

Erwartete Beobachtung: „passed"-Ausgabe ohne `ERROR:`-Zeile; im `--rule-named`-Lauf
`failed: 0 of N` und die `nav_18_*`-Tests in der Liste.

**Kontrollarm.** `NAV-18` auf `[planned]` zu setzen ist **kein** Kontrollarm — planned
braucht keine Deckung, der Gate bliebe grün. Die echten zwei: (a) einen `nav_18_`-Test in
`nav_99_` umbenennen → `ERROR: test references unknown rule NAV-99`; (b) die Regel `[active]`
lassen und **alle** `nav_18_`-Tests umbenennen → Richtung-1-Fehler. Beides einmal vorführen.

---

### Task 8 — Drift-Wächter für neue quellenlose Seiten (verbindlich, Grill-Beschluss 5)

**Was.** Ein reiner Quelltext-Test in `sidebar_place_tests.rs`, der alle Literale aus
`content_stack.add_named(&..., Some("..."))` unter `crates/reprise-gnome/src/ui` einsammelt
und gegen eine bekannte Liste prüft. Wer künftig eine neue Seite einhängt, muss sie einmal
einordnen: Quelle oder Ort. Repo-Präzedenz für diese Testform:
`sidebar_presentation.rs:672-736` (`nav_11_sidebar_roles_are_constructor_properties` liest den
ganzen `sidebar/`-Baum von der Platte und hat eigene Plausibilitätsschranken gegen „der
Wächter hat nichts gefunden").

**Warum.** E3 wählt bewusst „alles Unbekannte ist eine Quelle". Das ist die richtige Vorgabe
für den offenen Fall, aber sie schweigt beim nächsten quellenlosen Ort — und dann kehrt
**beides** zurück, die falsche Markierung und der tote Klick. Dieser Wächter macht das
Schweigen laut.

**Verifikation.**
`cargo test -p reprise-gnome --bin reprise nav_18_ 2>&1 | grep -E '^test result:'` → `ok`.
**Kontrollarm:** einen erfundenen Namen aus der bekannten Liste entfernen →
`^test result: FAILED`.

---

## 5. Gates, Befehle und die bekannten Fallen dieses Repos

### 5.1 Exakte Befehle

```bash
cd /home/marvin/Projects/reprise

# Nicht-Display (schnell, waehrend der Arbeit):
cargo test -p reprise-gnome --bin reprise nav_18_

# Volle Testliste des Crates (es gibt KEIN --lib; das Target heisst --bin reprise,
# crates/reprise-gnome/Cargo.toml:10-12):
cargo test -p reprise-gnome --bin reprise

# Alle ignorierten Display-Tests einsammeln (liefert die vollen Pfade fuer --exact):
cargo test -p reprise-gnome -- --ignored --list | sed -n 's/: test$//p'

# Ein einzelner Display-Test: siehe Task 3 (env + Session-Bus + virtueller X-Server,
# dann `-- --ignored --exact` mit dem VOLLEN Pfad aus der Liste).

# Lokales Display-Gate (der Befehl, den dieses Repo dafuer hat):
scripts/check-display-tests.sh              # alle
scripts/check-display-tests.sh --rule-named # nur regelbenannte
scripts/check-display-tests.sh --css        # nur die CSS-Parser-Waechter

# Qualitaets-Gates, die dieser Fix beruehrt:
scripts/check-architecture.sh
scripts/check-accessibility-semantics.sh
scripts/check-input-parity.sh
scripts/check-ux-traceability.sh
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
```

Realistisch lokal fahrbar: alles oben. `scripts/check-merge-readiness.sh` ist der Aggregat-Lauf (`:51-117`),
verlangt aber einen **sauberen** Worktree inkl. untracked Dateien (`:37-41`) und ein
frisches `origin/main` (`:19-30`) — in einer Agenten-Sitzung meist erst am Ende sinnvoll.
`AGENTS.md` („Definition of Done") erlaubt ausdrücklich, die Einzel-Gates direkt zu fahren
und die nicht verfügbare Aggregat-Prüfung zu protokollieren.

### 5.2 Fallen und Gegenmaßnahmen

1. **App-CSS in Display-Fixtures.** Ohne installiertes App-CSS misst der Test
   Phantom-Geometrie und -Stile. Die Konvention des Repos: `crate::ui::style::install()`
   für das vollständige App-CSS (`sidebar/sidebar_layout_tests.rs:19`) oder
   `crate::ui::style::install_css_string_for_test(&...::css())` für ein einzelnes Modul-CSS
   (`ui/style/mod.rs:177-190`; angewandt in `sidebar_activity_slot.rs:256`). **Jeder** neue
   Display-Test in diesem Plan ruft `crate::ui::style::install()`. Für die
   Gerätekarten-Tests genügt das ohne Zusatz — `sidebar_device_card::css()` ist bereits Teil
   von `app_css()` (`ui/style/mod.rs:113`).
2. **`--exact` läuft ins Leere.** Ein `--exact` mit unvollständigem Namen beendet sich mit
   Status 0, nachdem **nichts** lief. Deshalb immer zuerst `--ignored --list`, dann den
   vollen Pfad wörtlich einsetzen — genau so macht es `scripts/check-display-tests.sh` (Kommentar dort:
   „`--exact` with a stale name exits zero after running nothing; that is a gate failure").
3. **Kein `--lib` in `reprise-gnome`.** Das Crate hat nur ein Bin-Target
   (`crates/reprise-gnome/Cargo.toml:10-12`). `--bin reprise` benutzen.
4. **Beurteilung nur über `^test result:`.** Grün ist `test result: ok. 1 passed;` (bzw.
   `N passed` bei Sammelläufen). Rot ist `^test result: FAILED`. Bilanzzeilen („running X
   tests", „warning: ...") sind kein Urteil.
5. **Haupt-Kontext-Lock.** Display-Tests, die ein Toplevel präsentieren, nehmen
   `crate::ui::test_main_context::lock_main_context()` als erste Zeile
   (`sidebar_layout_tests.rs:17`, `window_navigation.rs:274`). Ohne ihn flackern Nachbartests.
6. **Fokus braucht ein aktives Toplevel.** Der Fokus-Test aus Task 4 muss auf die
   Aktivierung warten — `sidebar_tests.rs:513-518` erklärt, warum, und `sidebar_tests.rs:80-90`
   (`settle_until_active`) liefert das Hilfsmittel.
7. **`a11y-semantics`-Marker.** `scripts/check-accessibility-semantics.sh` prüft die Zeile **direkt über** jedem
   `set_focusable(true)` und lässt in `state=` **kein `+`** zu (Zeichenklasse `[a-z0-9._/-]`).
   Beim Umschreiben von `sidebar_rebuild.rs:609` beachten.
8. **`input-parity`-Marker.** `scripts/check-input-parity.sh` verlangt `// input-parity: ACC-8 keyboard=...`
   vor jedem neuen Gesten-/DropTarget-Konstruktor und verbietet `outline: none` ohne
   `:focus-visible`-Ersatz in derselben Datei. Die neue CSS-Regel aus Task 5 darf also kein
   `outline: none` enthalten.
9. **Rollen sind Konstruktor-Properties.** `sidebar_presentation.rs:672-704` scannt den
   **ganzen** `sidebar/`-Baum nach der Zeichenkette `set_accessible_role` — auch die neuen
   Dateien. `update_property`/`update_state` sind erlaubt und im Bestand üblich
   (`sidebar_presentation.rs:353`, `:362`).
10. **Größenklippen.** §3.1. `sidebar.rs` < 600, alles < 800, `window.rs` hat 1 Zeile Luft.
11. **ANNAHME (offen).** Ob GTK4 für eine selektierte `ListBoxRow` den AT-SPI-Zustand
    `Selected` von sich aus exportiert, konnte ich am Code nicht belegen. Falls ein
    `gtk4::test_accessible_has_state(&doctor_row, gtk4::AccessibleState::Selected)` rot ist,
    wird in `apply_marking` zusätzlich explizit
    `row.update_state(&[gtk4::accessible::State::Selected(Some(true/false))])` gesetzt —
    dasselbe Muster wie bei der Gerätekarte (`podcasts_view_selection.rs:39`). Der Test
    bleibt in beiden Fällen bestehen; nur die Implementierung wird ggf. um zwei Zeilen länger.
12. **Erledigt durch Grill-Beschluss 4.** Ob `notify::visible-child-name` synchron aus
    `set_visible_child_full` (`content_stack.rs:50`) emittiert wird, ist nicht belegt —
    aber die Frage stellt sich nicht mehr, weil beide Auslöser (Notify und der Aufruf am
    Ende von `route_row`) **denselben** Rechenweg nehmen und dasselbe Ergebnis erzeugen.
    Jeder Display-Test pumpt zusätzlich die Hauptschleife
    (`while gtk4::glib::MainContext::default().iteration(false) {}`), bevor er misst.
13. **ANNAHME (offen).** Ich habe nicht verifiziert, wie ein Test bequem eine offene
    Doctor-Findung in die In-Memory-DB schreibt, damit
    `queries::count_pending_doctor_findings` (`sidebar_rebuild.rs:93-97`) > 0 liefert.
    `sidebar_tests.rs:66-79` (`doc_8a_the_issues_entry_appears_only_with_unreviewed_findings`)
    ist der Ort, an dem dieses Wissen bereits steckt — dort nachsehen und dieselbe Fixture
    verwenden, statt eine neue zu erfinden.
14. **ANNAHME (offen).** Ob ein Test den `open_for_selection`-Pfad ohne den echten
    `LibraryDoctorCoordinator` nachbilden darf, ist eine Testentwurfs-Frage, die ich nicht
    abschließend belegen konnte. Weil die Wahrheitsquelle am Stack hängt (E3), ist der
    beobachtbare Unterschied zwischen `open_findings` und `open_for_selection` **null** —
    beide enden in `content_stack::show_page(..., "library-doctor")`
    (`library_doctor/navigation.rs:80`). Der zweite Test darf das deshalb als Stack-Schalter
    nachbilden und **muss** im Kommentar benennen, welchen Produktionspfad er vertritt
    (`library_doctor/mod.rs:427-435` → `:437-444` → `navigation.rs:32-37`). Wer den echten
    Coordinator will, findet in `library_doctor/tests.rs:160-165` den Ansatz.

---

## 6. Was dieser Plan bewusst **nicht** tut

* **Kein `ViewSource::LibraryDoctor`.** Ein neuer Enum-Zweig in
  `reprise-core/src/view_source.rs` zöge `queries::query_track_count`, `label()`, die
  Sitzungs-Serialisierung und jedes `match` im Frontend nach sich — für etwas, das keine
  Trackliste ist. `ViewSource` bleibt „woher kommen die Tracks".
* **Kein Sentinel in `current_source`.** Begründung in E2: `prepare_history_reroute`
  (`sidebar_session.rs:51-59`) ist die Warnung, nicht das Vorbild.
* **Kein Entfernen von `prepare_history_reroute`.** Eigene Aufräumaufgabe mit eigener
  Regressionslast.
* **Keine Umbenennung des Stack-Kindes `"device-sync"`.** Begründung und Beleg in E3.
* **Kein Neubau-Verhalten der Device-Seite.** Dass ein zweiter Klick auf die schon offene
  Karte die Seite neu baut (`device_sync_page.rs:319-322`), ist Bestand und bleibt (E2a).
* **Kein neues History-Verhalten.** `NAV-2` ist `[planned]` (`docs/ux-rules.md:125-131`);
  dieser Fix ändert nichts an `nav_history`.

---

## Parallelität

**Ergebnis: nicht sinnvoll schneidbar. Ein Strang.**

Begründung, nicht Bequemlichkeit:

* **Tasks 1–4 und 6 fassen alle denselben Kern an.** Der Zustand (`Shared` in
  `sidebar/sidebar.rs:93-214`), sein einziger Anwender (`sidebar_place.rs`, neu) und die
  Pfade, die ihn lesen (`sidebar_rebuild.rs:404-435`, `sidebar_row_wiring.rs:61-91` und
  `:110-127`, `sidebar_session.rs:11-29`) sind eine einzige Zustandsmaschine. Es gibt keine
  disjunkte Dateigruppe: jeder Schnitt lässt mindestens zwei Stränge gleichzeitig
  `sidebar.rs` **und** `sidebar_place.rs` schreiben.
* **Die beiden Symptome lassen sich nicht auf zwei Stränge verteilen.** Markierung
  (Symptom A) und toter Klick (Symptom B) hängen an derselben Größe (§1.3): Symptom B wird
  in `route_row` behoben, Symptom A in `apply_marking` — beide lesen `current_place`, das
  Task 2 in `sidebar.rs` einführt. Ein Strang „nur Markierung" liefert einen Fix, der die
  Beschwerde des Nutzers nur halb erledigt und dessen Tests grün sind, während der Nutzer
  weiter feststeckt. Genau das ist die Falle, die dieser Plan vermeiden soll.
* **Task 5 (Gerätekarte) sieht trennbar aus, ist es aber nicht ganz.** Ihre Dateien
  (`sidebar_device_card*.rs`, `sidebar_device_section.rs`) sind zwar disjunkt zu Tasks 1–4,
  aber der einzige Auslöser der Markierung ist `apply_marking` in `sidebar_place.rs`
  (Task 2) und das Feld `Shared::mark_device` in `sidebar.rs` (Task 2). Ein paralleler
  Strang könnte nur die Karten-**Optik** ohne Auslöser bauen — und müsste seine Verifikation
  gegen einen von Hand gesetzten Zustand fahren, also gegen etwas, das im Produkt so nie
  entsteht. Ein Test ohne den echten Auslöser beweist nichts.
* **Der Größendruck erzwingt Serialität.** `sidebar/sidebar.rs` hat 17 Zeilen Luft bis zum
  600-Zeilen-Limit (`scripts/check-architecture.sh:32-38`) und `sidebar/sidebar_tests.rs` 9 Zeilen bis 800
  (`:20`). Zwei Stränge, die beide dort anbauen, kollidieren nicht nur textuell, sondern
  reißen abwechselnd den Arch-Lint auf. Die Entlastungs-Umzüge aus §3.1 müssen **vor** allem
  anderen liegen und von einem einzigen Strang gemacht werden.
* **Task 7 (Regelwerk) allein zu schneiden wäre schädlich.** `AGENTS.md` verlangt, dass eine
  Regel im **selben** Commit `[active]` wird wie ihr Test — ein eigener Strang für
  `docs/ux-rules.md` würde entweder eine deckungslose `[active]`-Regel (`scripts/check-ux-traceability.sh`
  Richtung 1 rot) oder eine Regel ohne Verhalten hinterlassen.

**Empfohlene Ausführung.** Ein Strang, Tasks in der Nummernfolge, ein Commit pro Task (so
verlangt es `AGENTS.md`, „How to resume", Punkt 3). Nach jeder Task: `cargo fmt --check`,
`cargo clippy --all-targets --workspace -- -D warnings`, `scripts/check-architecture.sh` und die in der Task
genannte Verifikation samt Kontrollarm.

**Wenn dennoch parallelisiert werden muss** (z. B. weil Task 5 einen zweiten Kopf braucht),
dann nur nach diesem Schnitt und in dieser Merge-Reihenfolge:

| # | Strang | Datei-Globs (disjunkt) |
|---|---|---|
| 1 | **Kern** (Tasks 1–4, 6, 7, 8 — beide Symptome) | `crates/reprise-gnome/src/ui/sidebar/{sidebar,sidebar_place,sidebar_place_tests,sidebar_rebuild,sidebar_row_wiring,sidebar_session,sidebar_tests,mod}.rs`, `crates/reprise-gnome/src/ui/window/library_shell.rs`, `crates/reprise-gnome/src/ui/library_doctor/navigation.rs`, `docs/ux-rules.md` |
| 2 | **Karte** (Task 5, Optik + AT-SPI) | `crates/reprise-gnome/src/ui/sidebar/sidebar_device_card.rs`, `.../sidebar_device_card_text.rs`, `.../sidebar_device_section.rs` |

Merge-Reihenfolge: **1, dann 2.** Strang 2 kann erst mit dem `mark_device`-Feld und
`apply_marking` aus Strang 1 verdrahtet werden; ein umgekehrter Merge liefert Optik ohne
Auslöser. Der dritte Test aus Task 5 (toter Klick auf der Device-Seite) gehört fachlich zu
Strang 1s Dedup-Fix und darf erst nach dem Merge grün verlangt werden — er steht deshalb
auch in den Cross-Checks unten. Mehr als zwei Stränge gibt der Zuschnitt nicht her.

**Post-Merge-Cross-Checks** (jede Prüfung, die eine Datei liest, die ihr Strang nicht
besitzt — deshalb gehören sie hierher und nicht in die Stränge):

1. `scripts/check-architecture.sh` — liest **alle** `crates/**/*.rs`; nur nach dem Merge ist bewiesen, dass die
   Summe beider Stränge `sidebar.rs` < 600 und alles < 800 lässt.
2. `scripts/check-accessibility-semantics.sh` und `scripts/check-input-parity.sh` — beide scannen den ganzen `ui`-Baum; die neuen Marker
   aus Strang 1 (Doctor-Zeile) und die neue CSS-Regel aus Strang 2 werden erst gemeinsam
   geprüft.
3. `cargo test -p reprise-gnome --bin reprise nav_11_sidebar_roles_are_constructor_properties`
   — der Wächter liest den **kompletten** `sidebar/`-Baum von der Platte
   (`sidebar_presentation.rs:706-736`), also Dateien beider Stränge.
4. `scripts/check-ux-traceability.sh` — vergleicht `docs/ux-rules.md` (Strang 1) mit den Testnamen aus **beiden**
   Strängen.
5. `scripts/check-display-tests.sh` — der einzige Lauf, in dem die Tests aus Strang 2 (Gerätekarte) gegen den
   Auslöser aus Strang 1 laufen, einschließlich
   `nav_18_activating_a_source_from_the_device_page_routes_back_and_unmarks_the_card`.
   Erwartung: `failed: 0 of N`.
6. `scripts/check-display-tests.sh --css` — die neue CSS-Regel aus Strang 2 wird im komponierten `app_css()`
   (`ui/style/mod.rs:101-149`) geparst, das Strang 2 nicht besitzt.
