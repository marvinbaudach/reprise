# Aufgabe: Concerts — Credential-Feedback & Live-Aktualisierung der Ansicht

Kleiner Folge-Task nach dem Concerts-Merge (`e38791b251` auf `dev`). Drei
Punkte: ein echter Bug, eine UX-Lücke, ein neues Feature. Basis ist `dev`,
Arbeit bitte in einem eigenen Worktree/Branch (`fix/concerts-key-feedback`).

## Befund aus der Praxis (verifiziert, nicht raten)

Nutzer hat in Einstellungen → Plugins → Concerts den Ticketmaster-Key
eingetragen, Enter gedrückt, Dialog geschlossen. Die Concerts-Ansicht zeigte
weiterhin die StatusPage „Concerts needs an API key".

Nachgemessen in der laufenden Installation:

```
sqlite3 ~/.local/share/reprise/reprise.db \
  "SELECT key, length(value) FROM settings WHERE key LIKE 'concerts%';"
→ concerts.ticketmaster_apikey|32     ← Key IST gespeichert
→ concerts.location_lat/lon/name gesetzt (Zürich)
```

**Der Key wird also korrekt persistiert.** Das Problem ist ausschließlich,
dass die bereits offene Ansicht ihren Zustand nicht neu bewertet — plus
fehlendes Feedback, das dem Nutzer überhaupt sagt, dass etwas passiert ist.

## Punkt 1 — BUG: Ansicht aktualisiert sich nicht nach Settings-Änderung

**Ursache (belegt):**

- `crates/reprise-gnome/src/ui/preferences/preference_concerts.rs:203` —
  `PasswordEntryRow::connect_changed` schreibt bei jedem Tastendruck via
  `save_setting` in die Settings-Tabelle. Das ist SET-4-konform (Settings
  wirken sofort, kein Apply/OK) und funktioniert.
- `crates/reprise-gnome/src/ui/concerts/concerts_view.rs:379-383` —
  `has_credentials` wird beim Rendern aus `concerts::config::credentials()`
  gelesen, also frisch aus der DB. Der Wert ist korrekt, wird aber nie neu
  ausgewertet, solange niemand `refresh()` aufruft.
- `crates/reprise-gnome/src/ui/window/library_shell.rs:202` — der EINZIGE
  Aufrufer von `concerts_view.refresh()` ist die Sidebar-Auswahl. Wer bereits
  auf der Concerts-Seite steht und die Einstellungen öffnet/schließt, bekommt
  kein Refresh.
- `crates/reprise-gnome/src/ui/preferences/preferences.rs:274` —
  `dialog.connect_closed` hält nur die Scan-Progress-View am Leben und
  benachrichtigt keine Ansicht.

**Erwartetes Verhalten:** Sobald ein Credential gesetzt (oder geleert) wird,
verlässt die Concerts-Ansicht den `NoCredentials`-Zustand bzw. kehrt dorthin
zurück — **ohne Neustart und ohne Umweg über die Sidebar**. SET-4 heißt
„sofort", nicht „beim nächsten Seitenwechsel"; ideal wird die Ansicht schon
aktualisiert, während der Dialog noch offen ist.

**Vorhandenes Muster nutzen, nichts Neues erfinden:**
`crates/reprise-gnome/src/ui/concerts/concerts_worker.rs:40-183` hat bereits
`EnabledSubscribers` + `subscribe_enabled(...)` — genau dieser Kanal meldet
heute schon den Modul-Toggle live an View und Sidebar
(`preference_plugins.rs:182` → `context.concerts.set_enabled(...)`). Der
naheliegende Weg ist, denselben Runtime-Kanal um eine
Credentials-/Settings-Änderung zu erweitern (z. B.
`ConcertsRuntime::notify_settings_changed()`), den die Credential-Rows in
`preference_concerts.rs` auslösen. Alternative Wege sind erlaubt, wenn sie
sauberer sind — aber kein Polling und kein Timer.

**Bitte mitprüfen:**

- Gilt dasselbe für die **Releases-Ansicht** (`ui/releases/releases_view.rs`)
  und die Sidebar-Zähler? Wenn ja, im selben Zug mitziehen.
- Gilt dasselbe für die anderen Concerts-Settings, die den Inhalt beeinflussen
  (Location, Default-Radius, „Include similar artists", window_days)? Der
  Nutzer hat Location und Similar ebenfalls gesetzt.
- Klickt man im `NoCredentials`-Zustand unten „Fetch now", greift
  `request_fetch` (`concerts_view.rs:378`) die Credentials frisch aus der DB
  und würde vermutlich laufen, während die StatusPage weiter „needs an API
  key" behauptet. Diesen Widerspruch mit auflösen.

## Punkt 2 — UX: keine Rückmeldung, dass der Key gespeichert wurde

Es gibt bei den Credential-Zeilen keinen Speichern-Button und keine
Bestätigung. Der Nutzer tippt, drückt Enter — sichtbar passiert nichts, also
wirkt es kaputt, obwohl gespeichert wurde. Zusätzlich verwirrt, dass die
City-Zeile daneben (`preference_concerts.rs:218,236`) einen Apply-Button
(`show_apply_button(true)` + `connect_apply`) hat und sich damit anders
verhält als die Key-Zeilen.

**Ziel:** Die Credential-Zeilen sollen sich erkennbar „quittiert" anfühlen und
konsistent mit der City-Zeile sein. Enter muss eine sichtbare Wirkung haben.
Wie genau (Apply-Button wie bei City, Häkchen-/Statuszeile, o. ä.) ist eine
Design-Entscheidung — begründe sie kurz im Commit und halte dich an die
Settings-Regeln in `docs/ux-rules.md` (Sektion SET, insbesondere SET-4) und
die Feedback-Regeln (FB). Weiter per Tastendruck speichern ist okay, solange
die Rückmeldung stimmt und keine Netz-Anfrage pro Tastendruck entsteht
(siehe Punkt 3).

## Punkt 3 — FEATURE: Key beim Eintragen prüfen und Ergebnis anzeigen

Wunsch des Nutzers: „es wäre toll wenn meine Eingabe direkt überprüft wird und
ich ein Feedback erhalte."

**Verhalten:** Beim Bestätigen einer Credential-Zeile (Enter/Apply — NICHT bei
jedem Tastendruck) wird eine billige Test-Anfrage gegen den jeweiligen
Provider gestellt und das Ergebnis inline an der Zeile angezeigt:

- gültig → positive Bestätigung („Key works" o. ä.)
- 401/403 → „Key was rejected" (klar unterscheidbar von Netzproblem)
- Netz-/Timeout-Fehler → „Could not verify — check your connection"
- leeres Feld → Zustand zurücksetzen, keine Anfrage

**Vorschläge für die Prüf-Endpunkte** (vor Umsetzung gegen die echte API
verifizieren, die Parser sind tolerant gebaut):

- Ticketmaster: `GET /discovery/v2/attractions.json?keyword=test&size=1&apikey=…`
- Bandsintown: Artist-Lookup wie in `concerts/bandsintown.rs::resolve`

**Zwingende Randbedingungen:**

- Netz NIEMALS im Main-Loop — über den bestehenden Worker/`one_shot_task`
  laufen lassen; die UI darf nicht blockieren (CONC-5).
- Den geteilten 1-req/s-Limiter aus `concerts/http.rs` benutzen, keinen
  zweiten Pfad aufmachen.
- Der Key darf **nie** in Logs, Fehlermeldungen oder Tracing-Ausgaben landen
  (die Security-Review hat das explizit geprüft — nicht aufweichen).
- Fixture-Seam für Tests nutzen (`REPRISE_CONCERTS_FIXTURE_DIR`, inzwischen
  hinter dem `test-fixtures`-Feature); kein Test kontaktiert das Netz.
- Neue Strings englisch über `N_!` in
  `crates/reprise-gnome/src/ui/strings_concerts.rs` (steht bereits in
  `po/POTFILES.in`). **Übersetzungen der neuen msgids in allen sieben
  Katalogen nicht vergessen** — der Repo-Gate dafür ist
  `scripts/tests/gettext-catalogs.sh` (muss grün sein, 0 fuzzy).
- Entscheidungslogik als pure, headless testbare Funktionen (Antwort →
  Feedback-Zustand), Widget-Teil dünn halten.

## Regelwerk

`docs/ux-rules.md`: Der Zustandsvertrag der Concerts-Ansicht ist **CONC-4**,
die Netz-/Worker-Regel **CONC-5** (Sektion AE). Wenn sich durch Punkt 1–3 die
Bedeutung einer aktiven Regel ändert, gilt der Append-only-Prozess des
Regelwerks: Ersatzregel anlegen (`CONC-4` → `CONC-4a` usw.), alte als
`[ersetzt durch …]` markieren und die regelbenannten Tests im **selben
Commit** umbenennen — `scripts/check-ux-traceability.sh` erzwingt das. Neue
Regel für die Credential-Prüfung (z. B. CONC-8) ist zulässig; als `[geplant]`
anlegen und im Implementierungs-Commit auf `[aktiv]` flippen.

## Arbeitsweise & Gates

- TDD: erst roter Test, dann Implementierung; ein fokussierter Commit pro
  logischer Einheit, Conventional Commits auf Englisch, **kein**
  Co-Authored-By-Trailer.
- Vor jedem Commit: `cargo fmt --check` · `cargo clippy --all-targets
  --workspace -- -D warnings` · `cargo test --workspace` · `cargo audit`
  (einzige akzeptierte Advisory: RUSTSEC-2024-0436) · nach Core-Änderungen
  `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'`
  muss leer sein · Skript-Gates `check-architecture.sh`,
  `check-motion-tokens.sh`, `check-input-parity.sh`,
  `check-accessibility-semantics.sh`, `check-display-tests.sh`,
  `check-ux-traceability.sh`, `scripts/tests/gettext-catalogs.sh`.
- Display-Tests mit `#[ignore = "requires a display; run via xvfb-run"]`
  markieren und **einzeln** verifizieren (nie im Rudel bewerten):

  ```
  dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d) \
    XDG_CACHE_HOME=$(mktemp -d) GDK_BACKEND=x11 WAYLAND_DISPLAY= \
    REPRISE_AUDIO_SINK=fakesink \
    cargo test -q -p reprise-gnome <name> -- --ignored --test-threads=1
  ```

  Neue GTK-Tests brauchen `gtk4::init().unwrap();` als erste Zeile, sonst
  panicken sie mit „GTK has not been initialized".
- Nicht pushen, nicht nach `dev` mergen. Achtung: GitHub Actions ist derzeit
  repo-weit durch ein Billing-Problem blockiert — lokale Gates sind der
  einzige Nachweis, entsprechend gründlich.

## Abschlussbericht

Pro Punkt: Commit-Hash, was genau geändert wurde, Testname. Dazu Gate-Status,
Liste der einzeln verifizierten Display-Tests, und jede Abweichung von diesem
Dokument mit Begründung.
