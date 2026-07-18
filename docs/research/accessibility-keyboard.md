# Tastatur-Accessibility für die Reprise-GUI

Stand: 2026-07-18

## Zweck und Quellenstatus

Diese Notiz leitet einen Regelkern für die native GTK4/libadwaita-Oberfläche von
Reprise ab. Für GNOME-Anwendungen ist die
[GNOME Human Interface Guideline](https://developer.gnome.org/hig/) (HIG) die
maßgebliche Plattformkonvention; die
[GTK-Dokumentation](https://docs.gtk.org/gtk4/section-accessibility.html)
beschreibt den technischen Accessibility-Vertrag der Widgets. WCAG 2.2 ist ein
normativer Standard für Webinhalte, nicht für eine native GTK-Anwendung. GTK
selbst bezeichnet WCAG aber ausdrücklich als webfokussiert und dennoch
nützlich, daher dient WCAG hier als zusätzlicher Mindest- und Prüfkatalog, nicht
als Behauptung einer formalen WCAG-Konformität von Reprise
([GTK Accessibility, „Other resources“](https://docs.gtk.org/gtk4/section-accessibility.html#other-resources)).

Die WAI-ARIA Authoring Practices (APG) werden nur dort ergänzend verwendet, wo
sie Fokus-Lebenszyklen konkretisieren. Das W3C bezeichnet die APG ausdrücklich
als informative, nicht normative Ressource; sie ersetzt daher weder GNOME-HIG
noch GTK-Verhalten
([WAI-ARIA APG: Introduction](https://www.w3.org/WAI/ARIA/apg/about/introduction/)).

## Normativer Regelkern für Reprise

Die folgenden Formulierungen bilden die Recherchegrundlage für die stabilen
Regelvorschläge `ACC-1` bis `ACC-9` in `docs/ux-rules.md`. Die Überschriften
hier sind Themengruppen, keine konkurrierenden Regel-IDs.

### Themengruppe 1: Vollständige Bedienbarkeit

Jede sichtbare, mit Maus oder Touch auslösbare App-Funktion **muss** allein mit
der Tastatur erreichbar und ausführbar sein. Es darf keine zeitkritische
Tastensequenz erfordern. Die HIG verlangt ausdrücklich, dass jede Aktion auch
per Tastatur möglich ist; WCAG 2.2 formuliert denselben Mindestmaßstab für
Webinhalte ([GNOME HIG: Keyboard](https://developer.gnome.org/hig/guidelines/keyboard.html),
[WCAG 2.2, 2.1.1 Keyboard](https://www.w3.org/TR/WCAG22/#keyboard)).

Ein Widget, das per Tastatur betreten werden kann, **muss** auch mit üblichen
Tasten wieder verlassen werden können. Nichtstandardisierte Ausstiege sind
nicht zulässig, ohne den Weg zugänglich zu erklären; dies folgt ergänzend aus
dem Verbot von Tastaturfallen
([WCAG 2.2, 2.1.2 No Keyboard Trap](https://www.w3.org/TR/WCAG22/#no-keyboard-trap)).

### Themengruppe 2: Plattformübliche Tasten

Custom UI **muss** mindestens dasselbe Tastaturverhalten anbieten wie das
entsprechende native GTK-Control. GTK reserviert `Tab`/`Shift+Tab` für den
Fokuswechsel und erlaubt Aktivierung mit Tastatur; die GNOME-HIG definiert den
vollständigen Standardsatz
([GTK Input and Event Handling](https://docs.gtk.org/gtk4/input-handling.html),
[GNOME HIG: Standard Navigation Keys](https://developer.gnome.org/hig/guidelines/keyboard.html#standard-navigation-keys)).

| Taste | Erwartetes Verhalten |
| --- | --- |
| `Tab` / `Shift+Tab` | nächstes / vorheriges Bedienelement |
| `Ctrl+Tab` / `Shift+Ctrl+Tab` | Fokuswechsel, wenn das fokussierte Control `Tab` selbst benötigt |
| Pfeiltasten | räumliche Navigation oder Navigation innerhalb eines zusammengesetzten Controls |
| `Enter` | fokussiertes Control oder fokussierten Inhalt aktivieren |
| `Space` | Zustand eines Controls umschalten; bei Buttons ebenfalls aktivieren |
| `F10` | primäres oder sekundäres Menü öffnen |
| `Menu` / `Shift+F10` | Kontextmenü am fokussierten Ort öffnen |
| `Esc` | transienten Container wie Menü, Popover oder Dialog schließen |

Für implementierte Standardfunktionen **müssen** die GNOME-Belegungen verwendet
werden; wichtige Reprise-relevante Beispiele sind `Ctrl+F` für Suche,
`Ctrl+,` für Einstellungen, `F1` für Hilfe, `Ctrl+?` für die
Tastaturkurzübersicht, `F9` für ein Seitenpanel sowie `Alt+Links/Rechts` für
Zurück/Vor. Für das System reservierte Kombinationen, insbesondere mit `Super`,
dürfen nicht durch die App belegt werden
([GNOME HIG: Standard Keyboard Shortcuts](https://developer.gnome.org/hig/reference/keyboard),
[GNOME HIG: Shortcut Keys](https://developer.gnome.org/hig/guidelines/keyboard.html#shortcut-keys)).

Beschriftete Controls sollten übersetzbare Mnemonics erhalten, insbesondere
für häufige Aktionen. Die HIG beschreibt Mnemonics als `Alt` plus Buchstabe und
warnt davor, Konfliktfreiheit nur in der Ausgangssprache zu prüfen
([GNOME HIG: Access Keys](https://developer.gnome.org/hig/guidelines/keyboard.html#access-keys)).

### Themengruppe 3: Native Listen- und Auswahlsemantik erhalten

`GtkListView`, `GtkGridView` und `GtkColumnView` sollen ihre native
Tastenbehandlung behalten; eigene Controller dürfen sie nicht unabsichtlich
übersteuern. Für GTK-Listen gelten Pfeile, `Home`/`End`, `PageUp`/`PageDown`,
`Enter`, `Space`, `Ctrl+A` und `Ctrl+Shift+A`; `Ctrl` navigiert ohne
Auswahlverschiebung und `Shift` erweitert die Auswahl, soweit das
SelectionModel dies unterstützt
([GtkListBase: Shortcuts and Gestures](https://docs.gtk.org/gtk4/class.ListBase.html#shortcuts-and-gestures)).

Fokus und Auswahl sind zwei verschiedene Zustände und **müssen** visuell sowie
im Accessible Tree unterscheidbar bleiben. Bei Mehrfachauswahl darf bloßes
Fokus-Browsen nicht unbemerkt die bestehende Auswahl zerstören. Diese
Präzisierung entspricht der ergänzenden W3C-Praxis für zusammengesetzte
Controls
([WAI-ARIA APG: Focus vs. Selection](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/#kbd_focus_activedescendant)).

### Themengruppe 4: Fokusreihenfolge, Sichtbarkeit und Lebenszyklus

Die Tab-Reihenfolge **muss** vollständig, stabil und der visuellen beziehungsweise
inhaltlichen Leserichtung entsprechend sein. GTK folgt normalerweise dem
internen Widget-Baum, die HIG verlangt aber ausdrücklich, die tatsächliche
Reihenfolge zu testen; Labels sollen ihrem Control in der Fokusreihenfolge
unmittelbar vorausgehen
([GNOME HIG: Keyboard Navigation](https://developer.gnome.org/hig/guidelines/keyboard.html#keyboard-navigation),
[WCAG 2.2, 2.4.3 Focus Order](https://www.w3.org/TR/WCAG22/#focus-order)).

Jeder Tastaturfokus **muss** sichtbar sein und darf durch Overlays, Sticky
Chrome oder Scrollposition nicht vollständig verdeckt werden. Reprise soll die
nativen GTK/libadwaita-Fokusindikatoren nicht per CSS entfernen oder durch einen
nicht gleichwertigen Eigenbau ersetzen
([WCAG 2.2, 2.4.7 Focus Visible](https://www.w3.org/TR/WCAG22/#focus-visible),
[WCAG 2.2, 2.4.11 Focus Not Obscured (Minimum)](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)).

Wenn das fokussierte Widget durch Navigation, Filterung, Löschen, einen
Stack-Wechsel oder adaptives Ein-/Ausblenden verschwindet, **muss** der Fokus
gezielt auf einen logischen stabilen Nachfolger wechseln: bevorzugt auf das
nachfolgende Element, sonst auf den auslösenden Control oder den neuen
Ansichts-Einstieg. Diese Reprise-Regel ist aus der geforderten Fokusordnung und
der ergänzenden APG-Fokuspersistenz abgeleitet
([WAI-ARIA APG: Persistence of focus](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/#discernible-and-predictable-keyboard-focus)).

Beim Öffnen eines Dialogs **muss** der Fokus auf dem zuerst sinnvoll zu
bedienenden Element liegen. `Esc` aktiviert Abbrechen; `Enter` darf die
affirmative Standardaktion auslösen, aber nicht bei irreversiblen,
destruktiven oder anderweitig unbequemen Aktionen
([GNOME HIG: Dialogs](https://developer.gnome.org/hig/patterns/feedback/dialogs.html#general-guidelines)).
Beim Schließen kehrt der Fokus zum Auslöser zurück, außer das Ergebnis führt
logisch zu einem anderen Ziel; dies ist eine ergänzende, nicht native-GTK-
normative Dialogpraxis
([WAI-ARIA APG: Modal Dialog](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/)).

### Themengruppe 5: Semantik ist Verhalten, nicht Dekoration

Wo möglich, **müssen** native Controls (`GtkButton`, `GtkEntry`,
`GtkListView` usw.) verwendet werden, weil sie `GtkAccessible` und ihr
Tastaturverhalten bereits implementieren. App-spezifische Namen,
Beschreibungen und Beziehungen sind zu ergänzen, wenn die Defaults den Zweck
nicht ausreichend ausdrücken
([GTK Accessibility: The standard accessibility interface](https://docs.gtk.org/gtk4/section-accessibility.html#the-standard-accessibility-interface)).

Jedes interaktive Element **muss** einen kurzen, beschreibenden zugänglichen
Namen haben. GTK kann einen fehlenden Namen zwar ersatzweise aus Label oder
Tooltip ableiten, empfiehlt für Icon-only-Controls aber explizite,
lokalisierte Accessible-Properties. Reprise soll sich deshalb nicht auf einen
nur bei Pointer-Hover sichtbaren Tooltip als Namensvertrag verlassen
([GNOME HIG: Accessible Names](https://developer.gnome.org/hig/guidelines/accessibility.html#accessible-names),
[GTK Accessibility: Application development rules](https://docs.gtk.org/gtk4/section-accessibility.html#application-development-rules)).

Bei Custom Widgets **müssen** Rolle, Name beziehungsweise Beschreibung,
Zustände, Werte und Beziehungen dem sichtbaren Verhalten entsprechen und bei
jeder Zustandsänderung aktualisiert werden. GTK-Rollen sind nach der
Instanziierung unveränderlich, Attribute dagegen dynamisch
([GTK Accessibility: Roles and attributes](https://docs.gtk.org/gtk4/section-accessibility.html#accessible-roles-and-attributes),
[WCAG 2.2, 4.1.2 Name, Role, Value](https://www.w3.org/TR/WCAG22/#name-role-value)).

Eine Rolle ist laut GTK ein Versprechen: `BUTTON` auf einem beliebigen Widget
erzeugt kein Button-Verhalten. Ein buttonartiges Custom Widget **muss** daher
auch fokussierbar sein, mit den üblichen Tasten aktivieren und eine parameterlose
Action exportieren. Bei Value-Controls sind aktueller Wert und Grenzen zu
exponieren und die Änderung über zugängliche Actions beziehungsweise
`GtkAccessibleRange` anzubieten
([GTK Accessibility: A role is a promise](https://docs.gtk.org/gtk4/section-accessibility.html#a-role-is-a-promise),
[GTK Accessibility: Design patterns and custom widgets](https://docs.gtk.org/gtk4/section-accessibility.html#design-patterns-and-custom-widgets)).

Sichtbarer Zustand und Accessible Tree **müssen** synchron sein, insbesondere
für `selected`, `checked`, `pressed`, `expanded`, `disabled`, `hidden`, `busy`
und `invalid`. Rein dekorative Elemente erhalten `PRESENTATION`; das ist nicht
mit dem transienten Zustand `HIDDEN` zu verwechseln
([GTK Accessibility: States](https://docs.gtk.org/gtk4/section-accessibility.html#list-of-accessible-states),
[GTK Accessibility: Hiding UI elements](https://docs.gtk.org/gtk4/section-accessibility.html#hiding-ui-elements-from-the-accessible-tree)).

Nichtstandardisierte Tastaturinteraktionen **müssen** über
`GTK_ACCESSIBLE_PROPERTY_HELP_TEXT` auffindbar gemacht werden; hinterlegte
Shortcuts sollen zusätzlich über `KEY_SHORTCUTS` exponiert werden
([GTK Accessibility: Application development rules](https://docs.gtk.org/gtk4/section-accessibility.html#application-development-rules),
[GTK Accessibility: Accessible properties](https://docs.gtk.org/gtk4/section-accessibility.html#list-of-accessible-properties)).

### Themengruppe 6: Drag-and-drop braucht gleichwertige Alternativen

Keine Funktion darf ausschließlich über Ziehen erreichbar sein. Jede Reorder-,
Queue-, Playlist-, Spalten- oder Transfer-DnD-Aktion **muss** zusätzlich über
einen fokussierbaren Tastaturweg erreichbar sein, etwa über
`Alt+Pfeil hoch/runter`, „Nach oben/Nach unten“, „An den Anfang“, ein
Kontextmenü oder Ausschneiden/Einfügen. GNOME nennt Drag-and-drop ausdrücklich
als Fall, der zusätzliche Überlegung für vollständige Tastaturbedienung braucht
([GNOME HIG: Keyboard](https://developer.gnome.org/hig/guidelines/keyboard.html)).

Getrennt davon soll jede Drag-Funktion auch mit einem einzelnen Pointer ohne
Ziehen möglich sein, sofern Ziehen nicht wesentlicher Teil der Funktion ist.
Das ist WCAG 2.5.7 für Webinhalte und wird hier als zusätzliche Reprise-
Qualitätsregel übernommen
([WCAG 2.2, 2.5.7 Dragging Movements](https://www.w3.org/TR/WCAG22/#dragging-movements)).

Die Alternative **muss** dieselbe fachliche Wirkung, dieselben Grenzen und
dieselbe Rückmeldung haben wie der DnD-Pfad. Ein vorhandener
`GtkDragSource`/`GtkDropTarget` stellt nur die DnD-Mechanik bereit und ist kein
Nachweis der alternativen Bedienbarkeit
([GTK Drag-and-Drop](https://docs.gtk.org/gtk4/drag-and-drop.html)).

## Empfohlene Teststrategie

### 1. Regel- und Policy-Tests ohne Display

Für jede aktive ACC-Regel sollte es genau einen regelbenannten Primärtest geben.
Reine Funktionen sollen insbesondere Shortcut-Mappings, Modifier-Prioritäten,
zulässige Reorder-Schritte, Fokus-Fallback-Entscheidungen und
Name/Rolle/Zustands-Mappings tabellarisch prüfen. Diese Tests sind schnell,
beweisen aber allein weder reale Event-Zustellung noch den Accessible Tree.

### 2. Gemappte GTK-Tests

Jede eigenständige Ansicht und jeder Dialog braucht mindestens einen Test mit
realisiertem, gemapptem Widget-Baum. Zu prüfen sind:

- vollständige Vorwärts- und Rückwärts-Tabfolge;
- `Enter`, `Space`, Pfeile, `Home`/`End`, Auswahl und Mehrfachauswahl;
- `F10`, `Menu`/`Shift+F10` und `Esc` für Menüs, Popover und Dialoge;
- Fokus nach Öffnen, Schließen, Ansichtswechsel, Filterung, Löschen und
  dynamischem Hide/Replace;
- Sichtbarkeit und Scroll-to-focus des fokussierten Elements;
- DnD-Wirkung über den alternativen Tastaturpfad;
- Rolle, zugänglicher Name, Zustand, Wert und Beziehungen im Accessible Tree.

Die Tastenanforderungen stammen aus der
[GNOME-HIG](https://developer.gnome.org/hig/guidelines/keyboard.html) und den
[GtkListBase-Shortcuts](https://docs.gtk.org/gtk4/class.ListBase.html#shortcuts-and-gestures);
GTK empfiehlt außerdem ausdrücklich, Accessible-Attribute während der
Entwicklung zu prüfen
([GTK Accessibility: Application development rules](https://docs.gtk.org/gtk4/section-accessibility.html#application-development-rules)).

### 3. Semantische End-to-End-Tests

Der vorhandene `scripts/cua-e2e`-Pfad ist die geeignete Reprise-Basis, weil er
`GTK_A11Y=atspi` und `NO_AT_BRIDGE=0` setzt und bereits Accessibility-Snapshots
der echten App abfragt. Neue Szenarien sollten ausschließlich Tastaturaktionen
verwenden und nach jedem Übergang sowohl sichtbaren Zustand als auch Rolle,
Name, Fokus und Zustand im AT-SPI-Baum prüfen. `scripts/ptr-e2e` ist dafür nicht
geeignet, weil dessen Runner Accessibility bewusst mit `GTK_A11Y=none` und
`NO_AT_BRIDGE=1` abschaltet.

### 4. Manuelle GNOME-Abnahme

Vor einem Release ist ein kompletter Lauf ohne Maus erforderlich. Zusätzlich
fordert die GNOME-HIG Tests mit High Contrast, großer Schrift, Screenreader und
Bildschirmtastatur; beim Screenreader-Test soll die App auch mit ausgeschaltetem
Display bedienbar bleiben
([GNOME HIG: Testing for Accessibility](https://developer.gnome.org/hig/guidelines/accessibility.html#testing-for-accessibility)).
Der GTK Inspector soll dabei die Accessible-Attribute und sein
Accessibility-Overlay prüfen
([GTK Accessibility: Application development rules](https://docs.gtk.org/gtk4/section-accessibility.html#application-development-rules)).

Empfohlene manuelle Zustandsmatrix: frisches Profil, leere Bibliothek, große
Bibliothek, laufende und pausierte Wiedergabe, gefilterte Trackliste,
Mehrfachauswahl, Queue und Playlist, beide Sidebar-Zustände, schmales und
breites Fenster, geöffnete Popover/Dialoge sowie deaktivierte und fehlerhafte
Controls.

## Kurze Reprise-Risikoanalyse

Die folgende Einschätzung ist eine Code-Sichtung, kein vollständiger
Accessibility-Audit und kein Nachweis realer AT-SPI-Ausgabe.

### Hohes Risiko

1. **Waveform-Seeking ist pointer-only.**
   `ui/player_bar/waveform_seek.rs` baut eine `GtkDrawingArea` mit
   `EventControllerMotion` und `GestureDrag`, setzt aber weder Fokusierbarkeit
   noch Value-Control-Rolle/-Wert noch Tasten zur Wertänderung. Click/Drag zum
   Suchen ist damit voraussichtlich nicht per Tastatur erreichbar.

2. **Klickbare Labels und hoverabhängige Controls.**
   `ui/library_views/album_card.rs` macht den Künstler-Untertitel per
   `GestureClick` zum Deep-Link, ohne ein natives fokussierbares Control zu
   verwenden. `ui/track_list/rating.rs` verwendet zwar echte Star-Buttons,
   versteckt sie bei einer unbewerteten Zeile im Ruhezustand aber hinter einer
   nicht interaktiven Gedankenstrich-Darstellung. Beide Pfade müssen auf
   Tastatur-Erreichbarkeit, Namen und Fokusdarstellung geprüft werden.

3. **Track-/Queue-/Playlist-Reorder ist primär DnD.**
   `ui/track_list/track_list_dnd.rs` und `ui/sidebar/sidebar_dnd.rs` decken
   mehrere fachliche Drop-Pfade ab. Das Kontextmenü bietet teilweise „Move to
   top“, aber ein gleichwertiger Tastaturpfad für jede zulässige Zielposition
   und jede DnD-Wirkung ist aus der Sichtung nicht erkennbar.

### Mittleres Risiko

4. **Viele eigene Gesten und Key-Controller.**
   Reprise nutzt direkte `GestureClick`-/`GestureDrag`-/`EventControllerKey`-
   Verdrahtung unter anderem in Albumkarten, Lyrics, Device Cards, Sidebar,
   Spaltenheadern und Kontextmenüs. Jeder dieser Controller kann native
   Listen-, Aktivierungs- oder `Esc`-Semantik übersteuern und braucht deshalb
   einen zustandsbezogenen Keyboard- und Accessible-Tree-Test.

5. **Dynamische Fokus-Lebenszyklen.**
   `ui/window/window_navigation.rs`, `ui/window/library_shell.rs`, die Sidebar
   und das Now-Playing-Panel blenden Widgets ein/aus oder wechseln
   Stack-Seiten. Es gibt bereits gezielte Fokusübergaben und Tests, aber der
   Vertrag ist nicht flächendeckend für Filterung, Löschen, Panel-Schließen,
   Popover-Ende und alle adaptiven Breakpoints abgesichert.

6. **Eigene Accessible-Rollen sind nur punktuell.**
   Positive Beispiele sind `SEARCH_BOX` in `ui/window/window.rs`, `IMG` in
   `ui/track_list/track_cover.rs`, dekoratives `PRESENTATION` in der Sidebar
   sowie `KEY_SHORTCUTS` für `Alt+Pfeil` im Spalteneditor. Die Sichtung zeigt
   aber keinen zentralen Audit, der für jedes Custom Widget Verhalten, Rolle,
   Namen, Werte und dynamische Zustände gemeinsam verifiziert.

### Vorhandene gute Grundlagen

- Die Trackliste unterstützt `Menu` und `Shift+F10` explizit in
  `ui/track_list/track_list_context_keys.rs`.
- Der Spalteneditor bietet neben DnD `Alt+Pfeil hoch/runter` und exponiert den
  Shortcut im Accessible Tree (`ui/track_list/column_layout_editor.rs`).
- `ui/shortcuts.rs` schützt Texteingaben davor, dass der globale
  Wiedergabe-Shortcut `Space` ein Leerzeichen verschluckt, und führt `Esc` aus
  der Suche gezielt zurück zur Trackliste.
- `ui/window/library_shell.rs` und `ui/window/window_navigation.rs` enthalten
  bereits explizite Fokusübergaben bei einigen Ansichts- und Sidebar-Wechseln.
- `scripts/cua-e2e` liefert bereits eine isolierbare, semantische AT-SPI-
  Testbasis; die Lücke ist vor allem die systematische Keyboard-only-Matrix.

## Konsequenz für einen Umsetzungsplan

Die sichere Reihenfolge ist: zuerst den ACC-Regelvertrag und einen
Accessibility-Inventar-Test etablieren, danach pointer-only Custom Controls
reparieren, dann DnD-Alternativen und Fokus-Lebenszyklen schließen, anschließend
alle Ansichten über gemappte GTK- und CUA-Keyboard-Flows abdecken und zuletzt
die manuelle GNOME-/Orca-Matrix abnehmen. Eine bloße Sammlung globaler
Shortcuts wäre kein ausreichender Abschluss: entscheidend sind erreichbare
Controls, korrekte Semantik, stabile Fokusführung und gleichwertige Wirkung in
jedem GUI-Zustand.
