# Header- und Sidebar-Redesign — Implementierungsplan

## Globale Randbedingungen

Basis ist `34cbe86` auf `feature/header-sidebar-redesign`. Die parallele
Playerbar-Arbeit auf `main` bleibt unangetastet und wird erst vor dem späteren
Merge integriert. TDD RED→GREEN; englischer Code/UI/Commit, deutsche interne
Dokumentation; keine realen Musikdateien oder Datenbanken; jeder App-Lauf mit
privatem `XDG_DATA_HOME` und `XDG_CACHE_HOME`, eigener D-Bus-Session, Xvfb,
`GDK_BACKEND=x11`, leerem `WAYLAND_DISPLAY` und `fakesink`; alle Gates vor
jedem Implementierungscommit; jede wesentlich geänderte Datei unter 800 Zeilen.

## Aufgabe 1 — Fensterbreite flache Headerbar

**Dateien:**

- neu: `crates/reprise-gnome/src/ui/library_chrome.rs`
- ändern: `crates/reprise-gnome/src/ui/mod.rs`
- ändern: `crates/reprise-gnome/src/ui/window.rs`
- ändern: `crates/reprise-gnome/src/ui/minimal_view.rs`
- ändern: `crates/reprise-gnome/src/ui/compact_mode_controls.rs`

**Schnittstellen:**

```rust
pub(super) struct LibraryChrome {
    pub(super) root: adw::ToolbarView,
}

pub(super) fn build(
    header: &adw::HeaderBar,
    navigation: &adw::NavigationSplitView,
) -> LibraryChrome;

pub(super) fn style_header(
    header: &adw::HeaderBar,
    search: &gtk4::SearchEntry,
);
```

RED: ignorierten Displaytest anlegen, der flachen äußeren Toolbarstil,
`navigation` als Content, Header als Top-Bar, `Strict`-Zentrierung und die
begrenzte Suche prüft; gezielt unter isoliertem Xvfb ausführen und erwartetes
Fehlschlagen sehen. GREEN: äußeren Library-Root bauen, Header aus dem inneren
Content-Toolbar entfernen, Suche nach Installation aller Aktionen rechts
einordnen und `MinimalView` auf einen allgemeinen `GtkWidget`-Root umstellen.
Bestehenden Compact-Mode-Displaytest an den allgemeinen Root anpassen. Volle
Gates, Diff gegen Spezifikation prüfen und committen.

Commit: `feat: span the library header across the window`.

## Aufgabe 2 — Mockupnahe Sidebar-Hierarchie

**Dateien:**

- neu: `crates/reprise-gnome/src/ui/sidebar_presentation.rs`
- ändern: `crates/reprise-gnome/src/ui/mod.rs`
- ändern: `crates/reprise-gnome/src/ui/sidebar.rs`
- ändern: `crates/reprise-gnome/src/ui/library_shell.rs`

**Schnittstellen:**

```rust
pub(super) enum NavIcon {
    Library,
    Queue,
    Playlist,
    RecentlyPlayed,
    TopRated,
    RecentlyAdded,
    GenericSmart,
    ImportErrors,
    Missing,
}

pub(super) fn smart_icon(sort_field: &str) -> NavIcon;
pub(super) fn build_nav_row(title: &str, count: Option<i64>, icon: NavIcon)
    -> gtk4::ListBoxRow;
pub(super) fn append_header(listbox: &gtk4::ListBox, text: &str);
pub(super) fn append_new_playlist_row(listbox: &gtk4::ListBox)
    -> gtk4::ListBoxRow;
```

RED: Unit-Tests für feste Source-Icons, drei Smart-Sortierfelder und neutralen
Fallback anlegen; Displaytest für Iconspalte/Zähler sowie Split-View-Metriken
anlegen und erwartetes Fehlschlagen sehen. GREEN: Widgetbau aus `sidebar.rs`
extrahieren, alle Zeilen mit einer einheitlichen Symbolspalte versehen und
Split-View auf 220–280 px sowie 0,22 Breitenanteil begrenzen. DnD,
Kontextmenü, Selektion und Callbackpfade bleiben unverändert. Volle Gates,
isolierten Pointer-/Screenshot-Lauf, adversarielle Gesamtprüfung und
Dateigrößencheck ausführen.

Commit: `feat: align sidebar rows with the design mockup`.

## Aufgabe 3 — Zusammenführung und QA-Handoff

**Dateien:**

- ändern: `docs/agent-workflow/MANUAL-QA.md`
- ändern: `.superpowers/sdd/progress.md` (lokal/ignoriert)
- ändern: `docs/agent-workflow/STATUS.md` erst unter freiem `main`-Lock

Aktuelles `main` nach Abschluss der parallelen Playerbar-Arbeit integrieren.
Konflikte so lösen, dass Header und Playerbar beide fensterbreit sind und der
Compact-Mode denselben allgemeinen Library-Root wiederherstellt. Gesamte
Release-Batterie und isolierten Screenshot-Lauf erneut ausführen. Manuelle
GNOME-/Wayland-Prüfpunkte für Headerbreite, Zentrierung, Icon-Theme, HiDPI,
schmale Navigation und geöffnetes Info-Panel dokumentieren. Unter freiem Lock
in `main` mergen, Koordinationsboard aktualisieren und Lock freigeben. Nicht
pushen.

Commit: `docs: record header and sidebar visual QA`.
