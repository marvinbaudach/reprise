# Systemdekorations-Fallback — Implementierungsplan

## Globale Randbedingungen

- TDD RED→GREEN; englischer Code, Kommentare, UI und Commit, deutsche interne
  Dokumentation.
- Keine reale Musik, Datenbank oder Desktop-Session; jeder App-/Displaylauf ist
  vollständig gemäß `AGENTS.md` isoliert.
- Alle Gates vor dem Commit; jede wesentlich geänderte Rust-Datei unter 800
  Zeilen; keine Core-Abhängigkeit von GTK/libadwaita/GStreamer/zbus.

## Aufgabe 1 — Controls bis zur bestätigten SSD behalten

**Dateien:**

- ändern: `crates/reprise-gnome/src/ui/window_decorations.rs`
- ändern: `docs/agent-workflow/MANUAL-QA.md`
- ändern: `docs/agent-workflow/STATUS.md` beim Abschluss
- ändern: `.superpowers/sdd/progress.md` beim Abschluss

**Schnittstellen:**

```rust
fn client_controls_visible(mode: WindowDecorationMode, desktop_decorated: bool) -> bool;
fn desktop_decorated(window: &adw::ApplicationWindow) -> bool;
impl WindowDecorations {
    fn sync_controls(&self);
}
```

1. RED: Reinen Test ergänzen, der im Systemmodus ohne bestätigte SSD sichtbare
   Client-Controls verlangt und sie nur mit bestätigter SSD verbirgt. Den Test
   ausführen und den erwarteten Compilefehler beobachten.
2. GREEN: Projektion implementieren, auf `notify::css-classes` reagieren und
   nach Realisierung sowie jedem Moduswechsel synchronisieren.
3. Den Displaytest erweitern: System ohne `ssd` behält Library- und Compact-
   Controls; eine simulierte `ssd`-Klasse verbirgt sie; Entfernen stellt sie
   wieder her. Gezielte Tests isoliert ausführen.
4. Manuelle QA um den garantierten Fallback ergänzen. Vollständige Gates,
   Dateigrößen, adversarielle Diff-Prüfung und isolierten Start-Smoke ausführen.
5. Commit: `fix: preserve controls without system decorations`.
