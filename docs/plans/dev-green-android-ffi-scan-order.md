---
slug: dev-green-android-ffi-scan-order
worktree: /home/marvin/Projects/reprise-dev-green-android-ffi-scan-order
branch: feature/dev-green-android-ffi-scan-order
phase: shipped
codex_session:
created: 2026-08-11
---
# origin/dev wieder grün: android-ffi hängt an der Scan-Reihenfolge

Der CI-Quality-Gate auf `dev` scheitert an zwei Tests in
`crates/reprise-android-ffi/src/lib_tests.rs`:

```
test result: FAILED. 105 passed; 2 failed
  tests::browse_surface_gets_one_albums_tracks_in_core_order        (Zeile 317)
  tests::browse_surface_search_matches_genre_metadata_in_core_title_order  (Zeile 361)
```

Lokal fallen unter denselben Bedingungen **vier** um; die zwei zusätzlichen sind
`browse_surface_lists_core_album_summaries_in_core_order` (ca. Zeile 199) und
`browse_surface_lists_core_artist_summaries_in_core_order` (ca. Zeile 236).

## Warum

Die Tests legen `blue-1.flac` und `blue-2.flac` an und vergleichen anschließend
ganze `TrackRow`/`AlbumRow`/`ArtistRow`-Werte — **inklusive `id` bzw.
`representative_uri`**. Beide ergeben sich aus der Einfügereihenfolge des Scans,
und die folgt der Reihenfolge, in der das Dateisystem seine Verzeichniseinträge
ausliefert. Die ist auf tmpfs eine andere als auf ext4/btrfs.

Belegt: derselbe Commit, derselbe Befehl, nur `TMPDIR` verschoben — mit
`TMPDIR=/tmp` (tmpfs) 59 grün, mit `TMPDIR` auf der NVMe 55 grün / 4 rot. Auf
GitHubs Runner fallen zwei davon.

Beispiel aus dem Fehlerbild: erwartet wird `id: 3` für `blue-2.flac`, geliefert
wird `id: 1` — beides korrekt, je nachdem, welche Datei der Scan zuerst sah.

## Was zu tun ist

Die Tests prüfen zwei verschiedene Dinge, und nur eines davon ist echt:

- **Die Reihenfolge der Ergebnisse** („in core order") ist die Aussage des
  Tests und muss erhalten bleiben. Sie ist eine Sortierung der Abfrage, keine
  Eigenschaft des Dateisystems.
- **Die konkrete `id`** ist reine Buchhaltung. Sie zu vergleichen prüft nichts
  über das Verhalten und macht den Test von der Verzeichnisreihenfolge abhängig.

Stelle die Assertions auf einen stabilen Schlüssel um — `uri` oder `title` —
statt auf `id`. Die Reihenfolge bleibt Teil der Behauptung: vergleiche also
weiterhin eine **Sequenz**, nur eben eine aus stabilen Feldern, nicht eine aus
ganzen Strukturen mit Ids darin.

Dasselbe gilt für `representative_uri` in den Album- und Artist-Zusammen-
fassungen: welche der zwei gleichwertigen Dateien als Vertreter gewählt wird,
hängt ebenfalls am Scan. Entweder der Test lässt beide Werte zu, oder — besser —
die Produktion wählt den Vertreter deterministisch (z. B. den kleinsten Pfad),
und der Test nagelt genau das fest. Entscheide anhand des Produktionscodes,
welche der beiden Aussagen dort wirklich gemeint ist, und sag es im Commit.

Ändere **nicht** die Fixture so, dass zufällig wieder die alte Reihenfolge
herauskommt (etwa durch Umbenennen oder Sleep zwischen den Dateien). Das
verschiebt das Problem nur auf das nächste fremde Dateisystem.

## Abnahme

Beides muss grün sein — der Punkt der Aufgabe ist, dass das Ergebnis nicht mehr
vom Dateisystem abhängt:

```
env TMPDIR=/tmp   cargo test --locked -p reprise-android-ffi
env TMPDIR="$HOME/.cache/reprise-gate-tmp" cargo test --locked -p reprise-android-ffi
```

Dazu:

```
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
```

`scripts/check-merge-readiness.sh` nicht starten, keine Display-Tests.
