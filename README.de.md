# Reprise

[English](README.md) · [Deutsch](README.de.md)

Reprise ist für Menschen, die ihre Musik noch selbst besitzen — und für Devs,
die native Desktop-UX, portables Domänendesign und messbare Systemarbeit in
einer Rust-Codebase verbinden wollen. Eine GTK4-/libadwaita-App für GNOME trifft
auf einen GUI-freien Core und explizite Linux-Plattformverträge.

> **Status:** aktive Alpha. Reprise ist noch kein öffentliches Release.

## Warum Reprise

- **Local-first mit Tiefe.** Scans grosser Bibliotheken, Metadaten, Suche,
  Playlists, Hörverlauf, Android-Sync und Dateisicherheit funktionieren ohne
  Cloudkonto für die Musiksammlung.
- **Nativ per Design.** GTK4/libadwaita besitzt das GNOME-Erlebnis; GStreamer,
  MPRIS, MTP, Keyring und Papierkorb bleiben am Linux-Rand.
- **Gebaut zum Nachvollziehen.** Architektur, UX, Accessibility, Performance
  und Delivery sind ausführbare Verträge statt README-Versprechen.

## Architektur

![Reprise-Architektur: ein portabler Rust-Core, ein Linux-Plattformadapter und ein natives GTK4-/libadwaita-Frontend mit erzwungener Abhängigkeitsrichtung.](docs/assets/reprise-architecture.svg)

| Crate | Verantwortet | Darf nicht verantworten |
|---|---|---|
| `reprise-core` | Bibliothek, SQLite-Queries, Queue-Semantik, Scanner, Playlists, Settings und Plattformverträge | Abhängigkeiten zu GTK, libadwaita, GStreamer, zbus oder GLib |
| `reprise-platform-linux` | GStreamer-Wiedergabe und -Analyse, MPRIS/D-Bus, MTP, Papierkorb und weitere Linux-Adapter | Produkt-UI oder duplizierte Fachregeln |
| `reprise-gnome` | GTK4-/libadwaita-Präsentation, Interaktionszustand, Accessibility und Desktop-Komposition | Produktives SQL, blockierendes HTTP oder direkte GStreamer-Orchestrierung |
| `reprise-cli` | Headless-CLI über Core-Fassaden: Playlists, Suche, Library-Summary, Scan und Instrumental-Jobs | Andere Workspace-Crates als reprise-core (ausser den feature-gegateten mpris-/worker-Ausnahmen) oder produktives SQL |
| `reprise-mcp` | Lokaler stdio-MCP-Server: read-only Library-Resources und capability-gegatete Create-Tools für Agenten | Andere Workspace-Crates als reprise-core, produktives SQL oder Playback-/Queue-/Tag-/Delete-Tools |
| `reprise-stems` | Portables Stem-Separation-Backend (ML-Inferenz) für die experimentellen Instrumental-Jobs | Andere Workspace-Crates als reprise-core oder GUI-/Engine-Kopplung |

Die gemeinsame Engine besitzt Verhalten und Daten; Plattform-Crates
implementieren schmale Verträge, während jedes Frontend nativ bleibt. Die
`reprise-cli`- und `reprise-mcp`-Frontends laufen als eigene Prozesse über
dieselbe Datenbank, und ein Change-Log-Notifier lässt ihre Änderungen in einer
laufenden GTK-App live erscheinen — ohne Neustart.
`scripts/check-architecture.sh` erzwingt Abhängigkeitsrichtung, Core Purity,
Dateigrössen und bekannte Kopplungsgrenzen der Präsentationsschicht.

## Engineering-Verträge

- **Verhalten ist spezifiziert.** Das verbindliche
  [UX-Regelwerk](docs/ux-rules.md) ordnet jeder aktiven Regel einen benannten
  Rust- oder CUA-Test zu, einschliesslich Tastatur, Fokus, Accessibility,
  Feedback und Reduced Motion.
- **Grosse Bibliotheken bleiben begrenzt.** Das Trackmodell kombiniert
  GTK-Widget-Virtualisierung mit lazy geladenen 200-Zeilen-SQLite-Fenstern und
  einem festen Cache-Budget. Akzeptierte Vergleiche nutzen generierte Profile
  mit 10.000 und 100.000 Tracks.
- **Asynchrone UI-Arbeit bleibt identitätssicher.** Recycelte Zeilen und lange
  Worker nutzen Generation Tokens, damit veraltete Cover-, Metadaten-, Lyrics-
  oder Fortschrittsergebnisse kein anderes sichtbares Element übermalen.
- **Riskante Ränder sind explizit.** Netzwerkmodule sind Opt-in, Credentials
  liegen im System-Keyring, Dateimutationen verlangen eine Benutzeraktion und
  automatisierte Prüfungen nutzen isolierte Profile statt einer echten
  Musiksammlung.

Benchmarkmethoden, Einschränkungen und akzeptierte Evidenz stehen in
[TESTING.md](TESTING.md) und im [Engineering-Showcase](docs/showcase.md), nicht
als schnell veraltende Summen in diesem technischen Einstiegspunkt.

## Mitentwickeln

**Wähle deine Naht:** reine Bibliotheks-, Scanner-, Queue- oder Playlistlogik
in `reprise-core`; native Interaktion und Accessibility in `reprise-gnome`;
oder Audio-, Desktop- und Geräteadapter in `reprise-platform-linux`.

Starte mit [AGENTS.md](AGENTS.md) und dem [UX-Regelwerk](docs/ux-rules.md).
Änderungen beginnen mit einem fehlschlagenden Test, halten die Core-Grenze ein
und laufen per Pull Request durch `feature → dev → main`. Ziel ist nicht mehr
Code, sondern ein besserer Musikplayer mit Evidenz für seine Korrektheit.

## Bauen und starten

Voraussetzungen: Rust 1.92+, Meson 1.3+, Ninja, GTK 4.22+, libadwaita 1.9+,
SQLite, gettext, GStreamer 1.x und GVfs mit MTP-Volume-Monitor.

Installiere zusätzlich die GStreamer-Codecs für die Musikformate, die du
abspielen möchtest.

```sh
cargo build --locked --workspace
cargo run --locked -p reprise-gnome
cargo test --locked --workspace
```

Installation über Meson:

```sh
meson setup _build --prefix="$HOME/.local" -Dprofile=release
meson compile -C _build
meson install -C _build
```

Das Flatpak-Manifest zielt auf GNOME 50 und löst Cargo-Abhängigkeiten aus
gepinnten Checksums auf. Details stehen in [flatpak/README.md](flatpak/README.md).

## Verifikation

Die fokussierte lokale Basis:

```sh
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test --locked --workspace
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
```

Ein sauberer Merge-Kandidat durchläuft das vollständige Pull-Request-Gate mit
warnungsfreiem Rustdoc, Dependency Audit, isolierten GTK-Display-Suites sowie
den Accessibility- und Input-Verträgen:

```sh
MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh --no-fetch
```

Release-Kandidaten validieren über `scripts/check-release.sh` zusätzlich
Desktop-Metadaten, Flatpak-Quellen, Übersetzungen und einen optimierten
Meson-Install.

Display-Tests brechen geschlossen ab, wenn private D-Bus-/Xvfb-/AT-SPI-Dienste
nicht verfügbar sind; sie fallen nie auf Live-Desktop oder Benutzerprofil
zurück.

## Dokumentation

| Dokument | Zweck |
|---|---|
| [AGENTS.md](AGENTS.md) | Repository-Workflow, Sicherheitsgrenzen und Pflicht-Gates |
| [TESTING.md](TESTING.md) | Testebenen, Benchmarkmethode und Evidenzgrenzen |
| [docs/ux-rules.md](docs/ux-rules.md) | Verbindliche Interaktions- und Accessibility-Verträge |
| [docs/agents/branching.md](docs/agents/branching.md) | Pull-Request-Flow `feature → dev → main` |
| [docs/showcase.md](docs/showcase.md) | Portfolio-Positionierung und vertiefende Engineering-Evidenz |
| [RELEASING.md](RELEASING.md) | Packaging- und Release-Checkliste |

## Lizenz

Die portable Engine (`reprise-core`, `reprise-platform-linux`) steht unter
**MIT**. Das native GTK4-Frontend (`reprise-gnome`) steht unter
**GPL-3.0-or-later**. [LICENSING.md](LICENSING.md) erklärt die Gründe und
Komponentengrenzen.
