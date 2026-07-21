# Reprise

[English](README.md) · [Deutsch](README.de.md)

Reprise ist ein nativer GTK4-/libadwaita-Musikplayer für GNOME. Die Fachlogik
liegt in einem portablen Rust-Core ohne GUI-Abhängigkeiten; Linux-Wiedergabe und
Desktop-Integration hängen hinter expliziten Plattformverträgen.

> **Status:** aktive Alpha. Reprise ist noch kein öffentliches Release.

## Produktumfang

- Grosse lokale Bibliotheken mit inkrementellem Scan, Move-Reconciliation,
  Suche, Album-/Artist-Ansichten, Playlists, Queue, Ratings und Play History.
- GStreamer-Wiedergabe mit Gapless-Übergängen, Crossfade, ReplayGain,
  Zehnband-Equalizer, synchronisierten Lyrics und optionalem Scrobbling.
- Native GNOME-Integration über MPRIS, Medientasten, Benachrichtigungen,
  Session Restore, System-Keyring und bestätigte Papierkorb-Flows.
- Android-USB-/MTP-Browsing und -Synchronisierung mit expliziten Plänen,
  Fortschritt, Abbruch und begrenzten Gerätepfaden.

## Architektur

![Reprise-Architektur: ein portabler Rust-Core, ein Linux-Plattformadapter und ein natives GTK4-/libadwaita-Frontend mit erzwungener Abhängigkeitsrichtung.](docs/assets/reprise-architecture.svg)

| Crate | Verantwortet | Darf nicht verantworten |
|---|---|---|
| `reprise-core` | Bibliothek, SQLite-Queries, Queue-Semantik, Scanner, Playlists, Settings und Plattformverträge | Abhängigkeiten zu GTK, libadwaita, GStreamer, zbus oder GLib |
| `reprise-platform-linux` | GStreamer-Wiedergabe und -Analyse, MPRIS/D-Bus, MTP, Papierkorb und weitere Linux-Adapter | Produkt-UI oder duplizierte Fachregeln |
| `reprise-gnome` | GTK4-/libadwaita-Präsentation, Interaktionszustand, Accessibility und Desktop-Komposition | Produktives SQL, blockierendes HTTP oder direkte GStreamer-Orchestrierung |

Die gemeinsame Engine besitzt Verhalten und Daten; Plattform-Crates
implementieren schmale Verträge, während jedes Frontend nativ bleibt.
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

## Bauen und starten

Voraussetzungen:

- Rust 1.92+ (Edition 2021), Meson 1.3+ und Ninja
- GTK 4.22+, libadwaita 1.9+, SQLite, gettext und übliche GNOME-Build-Tools
- GStreamer 1.x plus die für die Musikdateien benötigten Codec-Plugins
- GVfs mit MTP-Volume-Monitor für Android-Geräte

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
