# #403 + #404 — Rollen an den Konstruktor, Aktionen an echte Knöpfe

Stand 2026-08-11, 22:05 · Grundlage: eigene Messung, nicht die Annahmen aus
`docs/plans/a11y-and-feedback-findings.HANDOFF.md`

## Was gemessen wurde (und was das umstößt)

Wegwerf-Probe in PyGObject gegen **GTK 4.22.4**, headless auf eigenem X-Server,
angehängt an den echten AT-SPI-Bus der Sitzung. Vierzehn Varianten einer
`ListBoxRow`, gebaut wie `navigation_row`, plus eine `ColumnView` mit
sortierbarem und unsortierbarem Kopf. Drei Ergebnisse tragen diesen Plan:

1. **`set_accessible_role()` nach dem Bauen kommt am AT-SPI-Bus nie an.** GTKs
   eigener Getter meldet die neue Rolle (`get_accessible_role() -> list-item`),
   der Knoten am Bus bleibt `panel`. Nur die Rolle **im Builder**
   (`accessible_role` als Konstruktor-Eigenschaft) landet: V9–V12/V14 erscheinen
   als `list item`, `button`, `menu item`, `page tab` — V1–V8 nicht.
2. **Bleibt die Rolle auf dem Vorgabewert, verwirft GTK auch das Label.** V13
   setzt nur `update_property(Label)` und bleibt `panel` mit leerem Namen: die
   Vorgaberolle ist `generic`, und ARIA verbietet dort einen vom Autor gesetzten
   Namen. Sobald die Rolle im Konstruktor sitzt, landet dasselbe
   `update_property(Label)` sauber (V14). **Die Seitenleistenzeilen sind heute
   also nicht nur aktionslos, sondern auch namenlos** — die Übergabe nahm das
   Gegenteil an.
3. **Keine Rolle erzeugt je eine Aktion.** Auch `accessible_role = Button` am
   Konstruktor liefert `actions=[]`. Aktionen kommen ausschließlich vom
   Widget-Typ: ein echter `GtkButton` meldet `click`. Für `actions != []` muss
   ein echter Knopf in der Zeile sitzen (in der Probe: V3/V4).

Zu #404 zusätzlich: der Spaltenkopf ist ein `GtkColumnViewTitle` mit CSS-Namen
`button`, aber **kein** `GtkButton`. Am Bus: Rolle `filler`, null Aktionen —
auch mit gesetztem Sorter. GTK bietet keine API, das von außen zu ändern; der
interne Titel ist nachträglich nicht umzurollen (siehe Ergebnis 1).

## Ziel

- **#403:** die Navigationszeilen der Seitenleiste tragen Rolle **und** Namen am
  Bus und bieten eine echte Aktion an, die tatsächlich navigiert.
- **#404:** Sortieren wird für Assistenztechnik erreichbar — nicht am Kopf
  (GTK-blockiert), sondern über eine zusätzliche, echte Bedienung. Der
  GTK-Fehler wird getrennt upstream gemeldet (nicht Teil dieser Aufgabe).

## Aufgabe A — Seitenleiste: Rolle im Builder, Aktion im Knopf

Einstieg: `crates/reprise-gnome/src/ui/sidebar/sidebar_presentation.rs`,
Verdrahtung: `crates/reprise-gnome/src/ui/sidebar/sidebar_row_wiring.rs`
(`route_row`, `connect_row_activated`). Weitere Dateien nach Bedarf — die Liste
ist ein Einstieg, keine Grenze.

1. **Alle `set_accessible_role`-Aufrufe in `sidebar_presentation.rs` wandern in
   den jeweiligen Builder** (`gtk4::ListBoxRow::builder().accessible_role(…)`,
   `gtk4::Label::builder().accessible_role(…)`). Betroffen sind die dekorativen
   `Presentation`-Stellen (Warnpunkt, Abschnittsüberschrift, Kopfzeile) genauso
   wie die `ListItem`-Zeilen. Nachträgliches `update_property(Label)` darf
   bleiben — das funktioniert, sobald die Rolle steht.
2. **Die Navigationszeile bekommt einen echten `gtk4::Button` als Kind**, der
   Icon und Beschriftung trägt und beim Klick exakt denselben Weg auslöst wie
   heute `row-activated` (`route_row`). Kein zweiter Navigationspfad: der
   Knopf ruft dieselbe Funktion, damit Zeigerbedienung, Tastatur und
   Assistenztechnik nicht auseinanderlaufen. Das gilt für die
   Navigationszeilen, die Problem-Zeilen (`build_issue_nav_row`) und die
   Playlist-Aktionszeilen (`append_playlist_action_row`).
3. **Optik darf sich nicht ändern.** Der Knopf ist flach und transparent, ohne
   eigenen Hintergrund, ohne eigene Mindesthöhe, ohne zweiten Hover-Effekt: die
   Zeile in `.navigation-sidebar` behält ihr bisheriges Hover-/Auswahlbild.
   Wenn dafür eine CSS-Regel nötig ist, gehört sie in die bestehende
   Stilschicht (`ui/style/`), nicht als Inline-Ausnahme in die Seitenleiste.
   Padding, Icon-Abstand (`ROW_SPACING`), Randabstände (`ROW_HORIZONTAL_MARGIN`,
   `ROW_VERTICAL_MARGIN`) und Ellipsierung bleiben unverändert.
4. **Fokus darf nicht doppelt werden.** Heute ist die Zeile fokussierbar; mit
   einem fokussierbaren Knopf darin entstünden zwei Tab-Stopps pro Zeile.
   Entscheide bewusst, wer den Fokus trägt, und halte fest: Pfeiltasten
   bewegen weiterhin innerhalb der Liste, Enter/Leertaste navigieren, und der
   Fokus bleibt nicht in einer Zeile hängen.

## Aufgabe B — Sortieren ohne Spaltenkopf erreichbar machen

Heute ist der Klick auf den Spaltenkopf die **einzige** Sortierbedienung
(`ui/track_list/track_list_sort.rs:61` verdrahtet den `ColumnViewSorter`,
`:112` hält `shared.sort`, danach `reload`). Es gibt kein Menü, keine GAction,
kein Tastenkürzel.

1. Eine **zusätzliche, bedienbare Sortier-Bedienung** ergänzen: ein
   `MenuButton` mit Menümodell (Sortierfeld + Richtung), platziert in der
   bestehenden Leiste über der Liste (`ui/browse/browse_bar.rs` neben
   `+ Add Filter` ist der naheliegende Ort).
2. Sie schreibt **denselben** Zustand wie der Kopfklick (`shared.sort`) und löst
   denselben `reload` aus — keine zweite Wahrheit über die Sortierung. Zustand
   in beide Richtungen spiegeln: Kopfklick muss die Menü-Markierung mitführen.
3. Beschriftung und Rolle: echter `MenuButton` (der meldet `click` am Bus),
   Menüeinträge mit sprechenden Namen. Die Bezeichner gehören zu den
   bestehenden `strings`, nicht als Literale in den Code.
4. Optik: der Knopf muss zur bestehenden Leiste passen (gleiche Höhe, gleiche
   Stilklassen wie `+ Add Filter`), nicht als Fremdkörper danebenstehen.

## Nicht-Ziele

- Der **systemische Durchmarsch** über die restlichen ~18 nachträglichen
  `set_accessible_role`-Stellen im Crate ist **nicht** Teil dieser Aufgabe. Sie
  sind nach derselben Messung ebenfalls wirkungslos; das wird als Folgearbeit
  festgehalten, nicht hier miterledigt.
- Der Spaltenkopf selbst wird **nicht** durch eigene Widgets ersetzt.
- Der GTK-Fehlerbericht (Kopf ohne Aktion, Rollensetzer ohne Wirkung) läuft
  außerhalb dieser Aufgabe.

## Verifikation — und was sie nicht beweist

**Wichtig, sonst entsteht ein Scheinbeweis:** Ein Unit-Test, der
`row.accessible_role()` prüft, ist **wertlos** — der Getter meldet auch im
kaputten Zustand die gewünschte Rolle (Messergebnis 1). Was tatsächlich trägt:

- **Strukturtests** (headless, in der bestehenden Suite): die Navigationszeile
  enthält einen `gtk4::Button`, dessen zugänglicher Name der Zeilenbeschriftung
  entspricht; Klick auf diesen Knopf löst dieselbe Navigation aus wie
  `row-activated`. Ebenso: die Sortier-Bedienung schreibt denselben
  Sortierzustand wie der Kopfklick.
- **Kein** Test darf sich damit begnügen, dass ein Testdouble reagiert — der
  Pfad muss durch den Produktionscode laufen.
- **Die AT-SPI-Abnahme läuft außerhalb der Suite** und macht der Auftraggeber:
  echter Baum-Dump der laufenden App, erwartet wird pro Navigationszeile ein
  Knoten mit gesetztem Namen **und** `actions` mit `click`, sowie ein
  bedienbarer Sortier-Knopf. Erst das schließt #403.
- Übliche Gates: `cargo fmt`, `cargo clippy`, `cargo test` für die berührten
  Crates. **`dev` ist derzeit an `crates/reprise-android-ffi` rot**
  (`browse_surface_*_in_core_order`, readdir-abhängig) — das ist Fremdschuld,
  nicht Anlass, hier etwas zu reparieren.

## Reihenfolge

Aufgabe A zuerst und vollständig (Rollen, Knopf, Optik, Fokus), danach
Aufgabe B. A ist die Aufgabe, die den belegten Befund schließt; B ist die
Antwort auf eine GTK-Grenze und darf A nicht aufhalten.
