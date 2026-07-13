# Systemgebundenes Farbschema — Implementierungsplan

> **Status:** bereit zur Ausführung  
> **Spezifikation:** `docs/superpowers/specs/2026-07-13-system-color-scheme-design.md`  
> **Basis:** `3b958b9` (`feature/system-only-appearance`)

## Task 1: Manuelle Farbschema-Auswahl entfernen

**Dateien:**

- `crates/reprise-core/src/library/settings.rs`
- `crates/reprise-gnome/src/ui/preference_appearance.rs`
- `crates/reprise-gnome/src/ui/preference_choice_cards.rs`
- `crates/reprise-gnome/src/ui/preferences.rs`
- `crates/reprise-gnome/src/ui/strings.rs`
- `crates/reprise-gnome/src/ui/preference_visual_strings.rs`
- `scripts/ptr-e2e/preferences.sh`
- gettext-Kataloge

**Schritte:**

1. Zuerst einen fehlschlagenden Strukturtest ergänzen, der genau eine Appearance-Sektion
   `WindowDecorations` verlangt, und den erwarteten Compile-Fehler belegen.
2. Farbschema-Karten, Vorschauen, typisierte Legacy-API, Persistenz-/Rollbackpfad und unbenutzte
   Texte entfernen; vorhandene Datenbankzeilen werden nicht migriert oder gelöscht.
3. Beim initialen UI-Aufbau `AdwStyleManager` immer auf `Default` setzen und den Preferences-Smoke
   so ändern, dass kein Farbschema mehr geschrieben oder erzwungen wird.
4. Den fokussierten Pointertest auf Abwesenheit eines neuen `ui.color_scheme`-Werts umstellen.
5. Gettext regenerieren; gezielten Display-/Pointertest und vollständige Projekt-Gates ausführen.
6. Diff adversarial gegen Spezifikation und Nutzerentscheidung prüfen.
7. Commit: `refactor: always follow system color scheme`

## Integration und Abschluss

1. Fortschrittsledger und Koordinationsstatus aktualisieren.
2. Feature-Branch mit `--no-ff` nach `main` mergen: `Merge system-only appearance`.
3. Relevante Gates auf `main` wiederholen, Lock freigeben und nicht pushen.
