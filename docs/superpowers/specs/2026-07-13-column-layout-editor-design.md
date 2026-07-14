# Spaltenlayout-Editor und Rhythmbox-Fundhinweis — Design

## Ziel

Reprise erhält eine native, jederzeit erreichbare Oberfläche zum Bearbeiten des
Bibliotheks-Spaltenlayouts. Zusätzlich zeigt der Ersteinrichtungsassistent den
optionalen Rhythmbox-Import nur dann an, wenn das bekannte read-only
GSettings-Schema samt `visible-columns`-Schlüssel tatsächlich vorhanden ist.

## Umfang

- Neuer Hauptmenüpunkt `Edit column layout…`.
- Libadwaita-Dialog mit allen neun Spalten in aktueller Reihenfolge.
- Cover und Title bleiben feste, sichtbare Leitspalten an Position eins und zwei.
- Die übrigen Spalten lassen sich ein-/ausblenden und per Ganzzeilen-Drag-and-drop
  umordnen. `Alt`+`↑/↓` bleibt als zugängliche Tastaturalternative erhalten, ohne
  jede Zeile mit zusätzlichen Pfeil-Schaltflächen zu überladen.
- Jede Änderung wird sofort über den bestehenden `TrackList::apply_column_layout`
  Pfad persistent angewandt. `Reset to Default` stellt das definierte Standardlayout
  wieder her.
- Der First-run-Wizard zeigt bei erfolgreicher Erkennung eine standardmäßig
  ausgeschaltete Zeile `Rhythmbox found` mit einem expliziten Importhinweis. Ohne
  Schema/Schlüssel wird keine irreführende Rhythmbox-Zeile angezeigt.
- Der bestehende manuelle Import bleibt im Hauptmenü erhalten.

## Architektur und Datenfluss

`column_layout.rs` bleibt Eigentümer der reinen Layout-Invarianten. Neue reine
Operationen schalten eine optionale Spalte, verschieben nur frei bewegliche Spalten
und stellen den Default wieder her. Sie werden vor GTK-Code testgetrieben.

`column_layout_editor.rs` besitzt ausschließlich den Dialog und seine Widgets. Ein
lokaler `Rc<RefCell<ColumnLayout>>` hält den Arbeitsstand. Callback-Code kopiert den
Stand aus jedem `RefCell`-Borrow heraus, bevor GTK, Persistenz oder ein erneuter
Listenaufbau aufgerufen werden. DragSource und DropTarget übertragen nur die stabile
`ColumnId`-Zeichenfolge; dieselbe reine Move-Operation bedient Dragging und die
über GTK-Accessibility-Metadaten angekündigten Tastaturkürzel.

`TrackList` liefert das gespeicherte normalisierte Layout und wendet Änderungen über
den bestehenden Persistenzpfad an. `primary_menu.rs` verdrahtet nur die neue Action
und delegiert den Dialogbau in das neue Modul.

`first_run.rs` verwendet `column_layout::rhythmbox_layout_available()`. Die Abfrage
liest ausschließlich Schema-Metadaten und die vorhandene Einstellung; sie schreibt
nie nach Rhythmbox. Der Schalter bleibt standardmäßig aus, somit erfolgt kein Import
ohne ausdrückliche Nutzeraktion.

## Fehlerbehandlung

- Schlägt das Speichern einer Editoränderung fehl, bleibt der vorherige Arbeitsstand
  aktiv und ein Toast erklärt den Fehler.
- Ungültige oder fremde Drag-Payloads sind No-ops.
- Fehlendes Rhythmbox-Schema oder fehlender Schlüssel bedeutet lediglich: kein
  Fundhinweis. Der manuelle Import liefert weiterhin seinen bestehenden Fehler-Toast.

## Tests und QA

- Reine Tests für Toggle-Invarianten, feste Spalten, Move-Grenzen, Reset und
  Rhythmbox-Angebotsentscheidung.
- GTK-Displaytests beweisen vollständige, ganzzeilige Drag-/Drop-Controller, feste
  Zeilen sowie die pfeilfreie Darstellung mit Tastaturcontroller; sie laufen einzeln
  unter Xvfb.
- Isolierter First-run-Smoke mit Environment-Fixture beweist sichtbaren Fundhinweis,
  Default-off und expliziten Import.
- Volle Gates, Core-Purity, Release-Checker und manuelle native-GNOME-Prüfung für
  Dialogdarstellung und echtes Pointer-Dragging.

## Explizit nicht Teil dieser Etappe

- Änderungen an Rhythmbox-GSettings oder Import weiterer Rhythmbox-Daten.
- Frei konfigurierbare Cover-/Titel-Sichtbarkeit.
- Spaltenbreiten-Persistenz oder benutzerdefinierte Spalten.
- Veröffentlichung, Screenshots oder Änderungen an echten Musikdateien/Benutzerdaten.
