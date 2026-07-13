# Fensterdekorationsmodus — Implementierungsplan

## Globale Randbedingungen

Basis ist `39fb0ad` auf `feature/header-decoration-mode`. CSD ist der Standard.
TDD RED→GREEN; englischer Code/UI/Commit, deutsche interne Dokumentation; keine
reale Musik, Datenbank, Konten oder produktiven App-Läufe; jeder App-Lauf mit
privatem `XDG_DATA_HOME` und `XDG_CACHE_HOME`, eigener D-Bus-Session, Xvfb,
`GDK_BACKEND=x11`, leerem `WAYLAND_DISPLAY` und `fakesink`; alle Gates vor jedem
Implementierungscommit; Core-Purity nach Core-Änderungen; jede wesentlich
geänderte oder neue Rust-Datei unter 800 Zeilen.

## Aufgabe 1 — Typisierte persistente Dekorationseinstellung

**Dateien:**

- ändern: `crates/reprise-core/src/library/settings.rs`

**Schnittstellen:**

```rust
pub enum WindowDecorationMode { Client, System }
pub fn get_window_decoration_mode(conn: &Connection) -> WindowDecorationMode;
pub fn set_window_decoration_mode(
    conn: &Connection,
    value: WindowDecorationMode,
) -> Result<(), rusqlite::Error>;
```

RED: Tests ergänzen, die `Client` als fehlenden Standard fordern, beide Varianten
roundtrippen und einen unbekannten Token auf `Client` zurückfallen lassen; den
gezielten Coretest ausführen und den erwarteten Compile-/Testfehler sehen. GREEN:
Konstante, Enum und tolerante Getter/Setter mit `client`/`system` implementieren.
Gezielten Test, vollständige Gates, Core-Purity und Dateigröße prüfen; Diff
adversariell gegen Spezifikation prüfen.

Commit: `feat: persist window decoration mode`.

## Aufgabe 2 — Live-Controller und Darstellungseinstellung

**Dateien:**

- neu: `crates/reprise-gnome/src/ui/window_decorations.rs`
- neu: `crates/reprise-gnome/src/ui/preference_window_decorations.rs`
- neu: `crates/reprise-gnome/src/ui/window_decoration_strings.rs`
- ändern: `crates/reprise-gnome/src/ui/mod.rs`
- ändern: `crates/reprise-gnome/src/ui/preferences.rs`
- ändern: `crates/reprise-gnome/src/ui/window.rs`
- ändern: `po/POTFILES.in`, `po/reprise.pot`, `po/de.po`

**Schnittstellen:**

```rust
pub(super) struct WindowDecorations;
impl WindowDecorations {
    pub(super) fn new(
        window: &adw::ApplicationWindow,
        library_header: &adw::HeaderBar,
        compact_root: Option<&gtk4::Widget>,
    ) -> Rc<Self>;
    pub(super) fn apply(&self, mode: WindowDecorationMode);
}

pub(super) fn row(context: &Rc<PreferencesContext>) -> adw::ComboRow;
pub(super) fn mode_from_index(index: u32) -> WindowDecorationMode;
pub(super) fn mode_index(mode: WindowDecorationMode) -> u32;
```

RED: Zuerst reine Index-Tests und einen ignorierten Displaytest anlegen. Der
Displaytest baut Library-Header plus alle Compact-Dekorationsformen, wendet beide
Modi an und fordert die exakten `decorated`-, Title-Button- und WindowControls-
Zustände. Tests gezielt ausführen und den erwarteten Compile-/Assertionsfehler
sehen. GREEN: Controller, Widgetsammlung und Preference-Row implementieren;
Startanwendung vor `window.present()`, erfolgreiche Live-Persistenz und
vollständige gettext-Texte verdrahten. `preferences.rs` und `strings.rs` nicht
weiter überladen. Gezielte Tests inklusive isoliertem Displaytest, vollständige
Gates, Dateigrößen und isolierten App-Smoke ausführen; Gesamt-Diff adversariell
prüfen und gefundene Fehler beheben.

Commit: `feat: add live window decoration preference`.

## Aufgabe 3 — QA-Handoff und Merge

**Dateien:**

- ändern: `docs/agent-workflow/MANUAL-QA.md`
- ändern: `.superpowers/sdd/progress.md` (lokal/ignoriert, falls vorhanden)
- ändern: `docs/agent-workflow/STATUS.md` erst unter freiem `main`-Lock

Die manuelle Liste um CSD-Standard, Live-Umschaltung, Persistenz,
Library/Compact-Konsistenz, Drag/Resize und compositorabhängigen SSD-Fallback
ergänzen. Gesamte Release-Batterie, Core-Purity, alle Displaytests und den
isolierten Pointer-/Screenshot-Harness ausführen. Branch gegen aktuelles `main`
prüfen, bei Bedarf integrieren, unter freiem Lock lokal nach `main` mergen,
Koordinationsboard und lokale Ledgerzeile aktualisieren, Lock wieder freigeben.
Nicht pushen.

Commit: `docs: record window decoration mode QA`.
