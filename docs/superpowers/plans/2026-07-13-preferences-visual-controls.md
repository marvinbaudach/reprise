# Preferences: visuelle Layout-Steuerung — Implementierungsplan

> **Status:** bereit zur Ausfuehrung  
> **Spezifikation:** `docs/superpowers/specs/2026-07-13-library-preferences-polish-design.md`  
> **Basis:** `e7c5f5f` (`feature/preferences-visual-controls`)  
> **Ausgangswert:** 708 Workspace-Tests bestanden; `cargo audit` meldet nur das akzeptierte
> `RUSTSEC-2024-0436` (`paste` via `lofty`).

## Ziel und Abgrenzung

Dieser Plan schliesst ausschliesslich die noch offenen visuellen Preferences-Punkte der
Bibliotheks-Politur ab:

- Farbschema als drei native Auswahlkarten mit Vorschau und Rollback,
- Playerleistenposition als zwei native Vorschaukarten,
- persistente Schalter fuer Filterleiste und Informationsspalte,
- klare Gruppen fuer Playerleiste, Bibliotheksfenster und Spalten.

Kompaktlayouts, Coverdownload, Playbacksemantik und weitere Roadmap-Stufen bleiben unveraendert.
Die inzwischen ausdruecklich beschlossene automatische Cover-Suche wird nicht wieder als Plugin
oder Schalter eingefuehrt.

## Task 1: Filterleisten-Sichtbarkeit typisiert persistieren

**Dateien:**

- `crates/reprise-core/src/library/settings.rs`
- `crates/reprise-gnome/src/ui/browse_bar.rs`
- `crates/reprise-gnome/src/ui/track_list.rs`

**Schritte:**

1. Zuerst einen fehlschlagenden Core-Test fuer sichtbaren Standard und `false`/`true`-Roundtrip
   von `ui.browse_visible` hinzufuegen und den erwarteten Compile-Fehler belegen.
2. Typisierte Getter/Setter implementieren.
3. `BrowseBar` laesst Quellensichtbarkeit und Nutzerwunsch getrennt bestehen und zeigt sich nur,
   wenn beide wahr sind. Aktive Filter bleiben beim Ausblenden erhalten.
4. Einen schmalen `TrackList`-Delegationspunkt fuer sofortige UI-Anwendung ergaenzen.
5. Betroffene Tests, Core-Purity und alle Projekt-Gates ausfuehren; Diff adversarial pruefen.
6. Commit: `feat: persist filter bar visibility`

## Task 2: Native visuelle Auswahlkarten

**Dateien:**

- `crates/reprise-gnome/src/ui/preferences.rs`
- `crates/reprise-gnome/src/ui/preference_choice_cards.rs`
- `crates/reprise-gnome/src/ui/preference_appearance.rs`
- `crates/reprise-gnome/src/ui/preference_layout.rs`
- `crates/reprise-gnome/src/ui/preference_visual_strings.rs`
- `crates/reprise-gnome/src/ui/mod.rs`
- gettext-Kataloge und Quellenliste

**Schritte:**

1. Zuerst pure fehlschlagende Tests fuer Auswahlabbildung und Rollbackentscheidung hinzufuegen.
2. Eine kleine native Kartenkomponente mit gruppierten `GtkCheckButton`s, eindeutigen Labels,
   Accessible Names und einfachen Vorschauwidgets implementieren.
3. Darstellung und Layout aus der fast vollen `preferences.rs` in fokussierte Seitenmodule
   extrahieren.
4. Farbschema-Combo durch System/Hell/Dunkel-Karten ersetzen; erst speichern, dann anwenden.
   Bei Fehler vorherige Auswahl und Theme wiederherstellen und Toast zeigen.
5. Playerleisten-Combo durch Oben/Unten-Vorschaukarten ersetzen; ebenfalls persist-first mit
   Rollback und Toast.
6. Neue UI-Texte zentralisieren, gettext aktualisieren und vollstaendige Uebersetzung pruefen.
7. Betroffene Tests und alle Projekt-Gates ausfuehren; Diff adversarial pruefen.
8. Commit: `feat: add visual preference choice cards`

## Task 3: Bibliotheksfenster-Schalter und Abschluss

**Dateien:**

- `crates/reprise-gnome/src/ui/preferences.rs`
- `crates/reprise-gnome/src/ui/preference_layout.rs`
- `crates/reprise-gnome/src/ui/info_panel.rs`
- `crates/reprise-gnome/src/ui/window.rs`
- isolierte Smoke-/Displaytests und Projektdokumentation

**Schritte:**

1. Zuerst fehlschlagende Struktur-/Displaytests fuer die vier Bibliotheksfenster-Schalter und
   beide Playerleisten-Vorschauen hinzufuegen.
2. Filterleiste und Informationsspalte in `PreferencesContext` kontrollierbar machen, ohne
   Ownership-Zyklen oder `RefCell`-Borrows ueber GTK-Aufrufe.
3. Gruppen `Player Bar`, `Library Window` und `Columns` aufbauen. Sidebar, Filterleiste,
   Informationsspalte, Statuszeile und Dichte wirken sofort nach erfolgreicher Persistenz.
4. Alle fehlerfaehigen Schalter stellen bei Persistenzfehler ihren vorherigen Zustand wieder her
   und zeigen den gemeinsamen Toastpfad.
5. Isolierten Preferences-Smoke inklusive Neustart-Persistenz sowie den passenden Displaytest
   ausfuehren; echte Rendering-/Pointer-Beurteilung fuer den manuellen GNOME-Pass dokumentieren.
6. Vollstaendige Gates, Rustdoc, gettext, Core-Purity, Dateigrenzen und Releasechecker ausfuehren.
7. Gesamtdiff adversarial gegen Spezifikation und Plan pruefen, gefundene Defekte beheben und
   betroffene Gates wiederholen.
8. Commit: `feat: complete library layout preferences`

## Integration und Abschluss

1. Fortschrittsledger und `docs/agent-workflow/STATUS.md` aktualisieren.
2. Feature-Branch mit `--no-ff` nach `main` mergen: `Merge Preferences visual controls`.
3. Auf `main` die relevanten Gates erneut pruefen.
4. Lock freigeben und committen:
   `docs: release work lock; merge Preferences visual controls`.
5. Nicht pushen. Ausschliesslich verbleibende manuelle native GNOME-Pruefungen im Abschlussbericht
   auffuehren.
