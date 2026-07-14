# Sidebar-Problemquellen aufräumen — Implementierungsplan

**Spezifikation:**
`docs/superpowers/specs/2026-07-14-sidebar-issue-cleanup-design.md`

## Task 1: Bulk-Bereinigung vollständig implementieren

**Dateien:**

- ändern: `crates/reprise-core/src/queries/maintenance.rs`
- ändern: `crates/reprise-core/src/queries/mod.rs`
- ändern: `crates/reprise-core/src/queries/tests_maintenance.rs`
- ändern: `crates/reprise-gnome/src/ui/mod.rs`
- ändern: `crates/reprise-gnome/src/ui/sidebar.rs`
- ändern: `crates/reprise-gnome/src/ui/window.rs`
- erstellen: `crates/reprise-gnome/src/ui/sidebar_issue_cleanup.rs`
- erstellen: `crates/reprise-gnome/src/ui/sidebar_issue_strings.rs`
- ändern: `po/POTFILES.in`, `po/reprise.pot`, `po/de.po`
- ändern: `docs/agent-workflow/MANUAL-QA.md`

1. RED: Core-Tests ergänzen, die alle Importfehler löschen und alle fehlenden
   Einträge entfernen; Live-Titel und Dateien bleiben unberührt, Playlist-
   Positionen bleiben lückenlos und die entfernten IDs sind deterministisch.
2. GREEN: die kleinsten Core-Bulk-Verträge implementieren und exportieren.
3. RED/GREEN: die reine Kontextmenüprojektion für Importfehler und Missing
   fixieren; nur Missing verlangt Bestätigung.
4. Rechtsklickgesten, native Popover-Menüs, den destruktiven Missing-Dialog,
   Toasts, Sidebar-Neuaufbau und Queue-Purge-Callback verdrahten.
5. Den vollständig isolierten GTK-Test ausführen: beide Problemzeilen besitzen
   sekundäre Klickgesten, Bulk-Mutationen entfernen nur erwartete Daten, leere
   Quellen verschwinden und Missing fällt auf Music zurück.
6. Deutsche Übersetzungen und Manual QA aktualisieren. Die bestehende
   Cover-Scan-Statuszeile ausdrücklich nicht verändern.
7. Adversarial Whole-Diff-Review durchführen; alle Projekt-Gates,
   Core-Purity, gettext und `< 800` Zeilen ausführen.
8. Commit: `feat: clean up sidebar issue sources`
