# Handover: origin/dev wieder grün machen

> **ERLEDIGT, 2026-08-11 23:48.** `origin/dev` steht auf **`ffb8dc2ad7`**
> (#415). Der Quality-Gate auf `dev` selbst ist grün gelaufen — Lauf
> [31539624145](https://github.com/marvinbaudach/reprise/actions/runs/31539624145),
> `Quality gate: success`, dazu `Cross-target: success`. Das Protokoll unten
> bleibt als Befundlage stehen; was tatsächlich im Weg stand, steht in
> „Bilanz" am Ende.

Stand: 2026-08-11, 19:40. Ziel ist, dass der CI-Quality-Gate auf `dev`
durchläuft. Er war es zuletzt an keinem der letzten sechs Läufe.

`origin/dev` stand auf **`dec75b8a18`** („The gates on dev are green again
(#408)") und steht seit 19:46 auf **`cd4f60aae0`** (#409, gc-Scope — für die
Gate-Frage folgenlos).

## Ausgangslage: was gemessen wurde

Die Gate-Liste ist `scripts/check-merge-readiness.sh` (CI ruft sie über
`scripts/ci-quality.sh`). Gemessen wurde jede Stufe einzeln gegen
`origin/dev` @ `4f6dfc7cb2` im Worktree `~/Projects/reprise-dev`.

**`check-merge-readiness.sh` niemals selbst starten** — sie fährt pro UX-Regel
einen eigenen Display-Test-Lauf mit eigenem Xvfb und terminiert praktisch nie.
Immer die Stufen einzeln fahren.

Grün von Anfang an: `check-device-sync-gstreamer.sh`,
`check-accessibility-semantics.sh`, `check-input-parity.sh`,
`check-runtime-service-install.sh`, `check-motion-tokens.sh`,
`scripts/tests/gettext-catalogs.sh`, `cargo fmt`, `cargo clippy`,
`cargo audit`, `cargo test -p reprise-platform-linux -- --test-threads=1`,
`check-display-tests.sh --motion` und `--css`, die Runtime-Service-Bus-Tests.

## Erledigt und gemerged: PR #408

https://github.com/marvinbaudach/reprise/pull/408 — squash-gemerged als
`dec75b8a18`. Sieben Befunde, alle ohne Verhaltensänderung:

1. Zwei Dateien über der 800-Zeilen-Grenze aufgeteilt
   (`queue_boundary_tests.rs` 804, `library/session.rs` 803).
2. 23 rohe Compose-Farben aus `NowPlayingFog/Scene/Sheet.kt` nach
   `NocturneTheme.kt` benannt — kein ARGB-Wert und kein Alpha-Faktor verändert.
3. Dead-Code-Allowlist-Drift: `is_settled` in `ui/list_geometry.rs` ist jetzt
   `#[cfg(test)]` statt `#[allow(dead_code)]`. Die Allowlist wuchs nicht.
4. 23 Tests von `NR-20`/`NR-25` (beide `[replaced]`) einzeln zugeordnet:
   2 → NR-30, 13 → NR-31, 2 → FIL-2a, 1 → FIL-6, 3 ohne Präfix.
5. rustdoc: öffentliche Doku verlinkte auf das private `ALREADY_GONE`.
6. rustdoc: `[podcasts]` und `[super::style]` lösten nicht auf.
7. CONTRAST-5: `radio/css.rs` malte Text mit `@accent_color`.

**Lehre daraus, die für den Rest gilt:** Punkt 2 und 6 waren erst *nach* der
ersten Reparatur sichtbar. Der Architektur-Lint stieg am 800-Zeilen-Fehler aus
und erreichte seinen Android-Abschnitt nie; rustdoc brach am
android-ffi-Fehler ab. **Nach jeder Reparatur neu messen**, nicht der ersten
grünen Meldung glauben.

## In Arbeit

### Paket 2 — Display-Tests (Codex läuft)

- Plan: `docs/plans/dev-green-display-tests.md`
- Worktree: `~/Projects/reprise-dev-green-display-tests`
- Branch: `feature/dev-green-display-tests` (basiert auf `4f6dfc7cb2`,
  **muss vor dem PR auf `dec75b8a18` nachgezogen werden** — merge-readiness
  verlangt, dass die Basis Vorfahre ist)
- systemd-Unit: `reprise-codex-display`

Elf von 424 regelbenannten Display-Tests sind rot (einzeln nachgefahren).
`stats_19_period_switch_tweens_bars…` und `fb_9_chip_end_inset…` waren nur im
Rudel rot und sind einzeln grün — die nicht anfassen.

Vier Commits stehen schon:

| Commit | Baustelle |
|---|---|
| `8074d81812` | Link-Rollen an Metadaten-Links (browse_4, stats_22) |
| `cefada6d7e` | Podcast-Gruppenkopf (src_11, src_4b) |
| `a43076bc1d` | Preferences-Gegenprobe (fb_9_counterprobe) |
| `23d8df073a` | verzögerte Zentrierung (start_3, fil_9) |

Offen sind damit noch vier Tests: die drei Anker-Tests
(`tag_1_year_save…`, `tag_1_query_reloading…`, `browse_11…`) und
`tag_1_restoring_dialog_focus…`.

### Paket 3 — android-ffi hängt an der Scan-Reihenfolge (Codex läuft)

- Plan: `docs/plans/dev-green-android-ffi-scan-order.md`
- Worktree: `~/Projects/reprise-dev-green-android-ffi-scan-order`
- Branch: `feature/dev-green-android-ffi-scan-order` (basiert auf `cd4f60aae0`)
- systemd-Unit: `reprise-codex-ffi`

Nachtrag 19:52: Der erste Anlauf um 19:40 lief ins Leere — Worktree und Branch
existierten nie, die Unit starb mit Exit 1 und `--collect` räumte sie weg. Neu
aufgesetzt auf `cd4f60aae0` (origin/dev nach #409) und neu gestartet.

**Das ist der Befund, der den CI-Gate auf `dev` aktuell zu Fall bringt.** Zwei
Tests in `crates/reprise-android-ffi/src/lib_tests.rs` vergleichen Track-IDs
aus der Scan-Reihenfolge; die folgt der `readdir`-Reihenfolge des
Dateisystems. Lokal auf tmpfs grün, auf ext4/btrfs und auf GitHubs Runner rot.

## Was danach noch offen ist

Wenn Paket 2 und 3 gemerged sind, muss der Gate **einmal ganz** laufen — auf
CI, nicht nur lokal. Erst dann ist die Frage beantwortet, ob hinter den
Display-Tests noch weitere Stufen liegen, die auf dem Runner anders ausgehen
als hier. Der Quality-Gate-Lauf von #408 kam bis zu den Workspace-Tests und
brach dort ab; alles danach ist auf CI schlicht ungetestet.

Zwei Punkte, die ich bewusst **nicht** angefasst habe:

- Für die Status-Werte `upcoming` / `Missing` / `Incomplete` /
  `X of Y tracks` gibt es keine aktive Regel mehr in `docs/ux-rules.md` — nur
  die abgelöste NR-17 nannte sie. Drei Tests haben deshalb ihr Regelpräfix
  verloren. Die Lücke im Regeldokument ist echt und offen.
- `view_state_memory.rs::restore_scroll_when_ready` ruft noch
  `list_geometry_changed::on_changed_once` und schreibt synchron aus der
  Emission — dasselbe Muster, das `744f8d953b` an anderer Stelle behoben hat.
  Hängt an keinem roten Test, steht aber als Auflage im Display-Plan.

## Wie man hier weiterarbeitet

### Einen einzelnen Display-Test fahren

```
env GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  GIO_USE_VFS=local GTK_USE_PORTAL=0 \
  dbus-run-session -- xvfb-run --server-num=4990 \
  cargo test -p reprise-gnome <voller::test::pfad> -- --ignored --exact
```

### Die ganze regelbenannte Suite

```
DISPLAY_TEST_JOBS=2 scripts/check-display-tests.sh --rule-named
```

Rote Tests danach **einzeln** nachfahren, bevor man sie sich zuschreibt. Das
Skript `~/.cache/reprise-gate-run/rerun-reds.sh` macht genau das
für eine Liste von Testnamen (Ergebnis in `reruns/summary.txt`).

### Workspace-Tests

```
env TMPDIR=/tmp cargo test --locked --workspace \
  --exclude reprise-platform-linux --no-fail-fast
```

`TMPDIR=/tmp` ist bis zum Abschluss von Paket 3 Pflicht, sonst fallen vier
android-ffi-Tests falsch-rot um. `--no-fail-fast`, weil cargo sonst beim
ersten roten Target aufhört und den Rest verschweigt. `reprise-platform-linux`
separat mit `-- --test-threads=1`, sonst stören sich die GStreamer-Tests.

### Lange Läufe abkoppeln

Hintergrund-Bash aus dem Harness wird gekillt — `setsid nohup` hat in dieser
Sitzung **nicht** gereicht. Verlässlich ist eine eigene systemd-User-Unit:

```
systemd-run --user --unit=<name> --collect ~/.cache/reprise-gate-run/<skript>.sh
systemctl --user is-active <name>
journalctl --user -u <name> -n 5 --no-pager
```

Das Skript darf dabei **nicht** im Scratchpad unter `/tmp/claude-1000/…`
liegen — systemd startet es dort mit `status=203/EXEC`, obwohl es ausführbar
ist. Deshalb liegen alle Läufe unter `~/.cache/reprise-gate-run/`.

### Codex-Betrieb

- Auf den **Prozess** warten, nie auf `.pipeline-codex.md` — die Datei
  entsteht schon beim Start des Laufs.
- `.pipeline-codex.md` ist im Repo **getrackt**. Codex überschreibt sie, und
  `check-architecture.sh` fällt dann über ein `new blank line at EOF`. Vor
  jedem Gate-Lauf und vor jedem Merge `git checkout -- .pipeline-codex.md`.
- `heavy-run` verschluckt stderr. Ein `cargo doc` darunter liefert bei einem
  Fehlschlag nur den Exit-Code und ein leeres Log. Für Doc- und Clippy-Läufe
  direkt starten.

### Merge

Das Repo erlaubt **keine Merge-Commits**. PRs gehen mit
`gh pr merge <nr> --squash --admin` nach `dev`.

## Bilanz: was tatsächlich im Weg stand

Fünf Blocker, von denen diese Übergabe nur zwei kannte. Der Gate ist eine
Kette — jede rote Stufe verdeckt alles dahinter, und genau so kamen sie
nacheinander zum Vorschein:

| # | Blocker | Herkunft | erledigt in |
|---|---|---|---|
| 1 | android-ffi verglich Scan-IDs aus der `readdir`-Reihenfolge | bekannt (Paket 3) | #412 |
| 2 | elf rote regelbenannte Display-Tests | bekannt (Paket 2) | #415 |
| 3 | drei rohe Compose-Farben außerhalb des Theme-Verzeichnisses | neu, aus #414 | #416 |
| 4 | `crates/reprise-core/src/visuals/spectrogram_frame_tests.rs` unformatiert | neu, aus #414 | `55ef7a4fe5` in #415 |
| 5 | `doc_5c` rief ImageMagicks `import` und `expect`te es | alt aus #376 | `ff6b28d39f` in #415 |

Zu #5: Der Test war nie kaputt — die Geometrie stimmte (`issues=358..406`,
`card=406..470`). Er starb an einer Debug-Instrumentierung, die auf jedem Lauf
ein PNG nach `/tmp` schrieb und auf einem Runner ohne ImageMagick mit
`NotFound` abbrach. Sichtbar wurde das erst, als der Gate die Display-Stufe auf
CI zum ersten Mal seit sechs Läufen überhaupt erreichte.

Nebenbei erledigt: die im Abschnitt „Was danach noch offen ist" notierte
Auflage. `view_state_memory.rs` benutzt kein `on_changed_once` mehr; die
Wiederherstellung der Blickposition liegt jetzt generationsgeschützt in
`reload_anchor_scroll.rs` und schreibt über `after_changed_once`, abgesichert
mit einem `debug_assert!` gegen Schreiben aus der Emission heraus.

**Nachgeprüft am 2026-08-12, 02:30 — die Lücke gibt es nicht (mehr).** Der
Punkt stand als „echt und offen" in dieser Übergabe; gegen den heutigen Stand
stimmt er nicht:

- `NR-17` steht in `docs/ux-rules.md:1936` auf **`[active]`** und nennt die
  Werte wörtlich: „Status is `upcoming`, `Missing`, `Incomplete`, or — when the
  length is known — `X of Y tracks`."
- Beide Tests dazu tragen ihr Präfix:
  `nr_17_release_status_distinguishes_upcoming_incomplete_and_missing`
  (`crates/reprise-core/src/artist_news_view_tests.rs:41`) und
  `nr_17_status_pills_describe_discography_gaps`
  (`crates/reprise-gnome/src/ui/releases/releases_presentation.rs:196`).
  Präfixlose Varianten existieren nirgends.
- Beide Namen stammen unverändert aus #89 (2026-07-27), NR-17 ist seit
  2026-07-28 aktiv — `git log -S` zeigt für beide Zeichenketten genau einen
  Commit, sie wurden also nie entfernt und wieder eingeführt.

Die in #408 präfixlos gewordenen Tests sind andere: Sitzungs- und
Queue-Persistenz in `reprise-core`, kein Display-Test und von der
UX-Rückverfolgbarkeit nicht verlangt. Es ist also nichts zu tun.

### Zwei Betriebsfallen aus diesem Lauf

- **Paket 3 lief nie.** Die Unit `reprise-codex-ffi` stand auf `active`,
  aber Worktree und Branch existierten nicht; der Lauf starb mit Exit 1, und
  `--collect` räumte die Unit weg. `systemctl show` meldete danach
  `Result=success`. Nur `journalctl` sagte die Wahrheit. Bei einer
  `--collect`-Unit ist der Exit-Status nach dem Ende wertlos.
- **Harness-Hintergrund-Bash wird abgeräumt.** Zwei Watcher wurden binnen
  einer Minute gekillt. Verlässlich für lange Wartezeiten war der persistente
  `Monitor` mit Poll-Schleife.
