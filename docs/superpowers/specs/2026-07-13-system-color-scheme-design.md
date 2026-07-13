# Systemgebundenes Farbschema

## Ziel

Reprise folgt unter Linux immer dem vom Desktop vorgegebenen Farbschema. Die manuelle Auswahl
zwischen System, Hell und Dunkel entfällt aus den Einstellungen. Ein historisch gespeicherter Wert
unter `ui.color_scheme` darf das Desktop-Farbschema nicht mehr überschreiben.

## Umfang

- Die Seite **Darstellung** behält ausschließlich die Einstellung für Fensterdekorationen.
- `AdwStyleManager` wird beim Aufbau des Hauptfensters ausdrücklich auf `Default` gesetzt.
- Bestehende `light`- oder `dark`-Werte bleiben als harmlose Legacy-Daten in der Datenbank, werden
  aber weder gelesen noch neu geschrieben.
- Farbschema-Texte, Vorschau-CSS, Smoke-Aktionen und Pointerprüfungen werden entfernt.
- Die wiederverwendbare Kartenkomponente bleibt erhalten, weil die Playerleistenposition sie nutzt.

## Fehler- und Kompatibilitätsverhalten

Es gibt keinen speicherbaren Farbschema-Wert und damit keinen Persistenzfehlerpfad. Bestehende
Profile wechseln beim nächsten Start zurück auf das Systemfarbschema; andere Einstellungen und
Nutzerdaten bleiben unverändert.

## Verifikation

- Ein RED/GREEN-Strukturtest belegt, dass Darstellung nur Fensterdekorationen enthält.
- Ein Displaytest belegt, dass ein zuvor erzwungenes Farbschema auf `Default` zurückgesetzt wird.
- Der isolierte Preferences-Pointertest prüft, dass `ui.color_scheme` nicht geschrieben wird.
- Vollständige Projekt-Gates, gettext, Rustdoc, Core-Purity und Releasechecker bleiben grün.

## Explizit nicht Teil dieser Änderung

- Kein eigener Theme-Schalter und keine automatische GNOME-Dark-Mode-Logik außerhalb libadwaita.
- Keine Migration oder Löschung bestehender Datenbankwerte.
- Keine Änderung an Fensterdekoration, Playerleistenposition oder Kompaktlayouts.
