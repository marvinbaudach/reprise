---
slug: radio-favicons-cold-start
worktree: /home/marvin/Projects/reprise-radio-favicons-cold-start
branch: feature/radio-favicons-cold-start
phase: planned
codex_session:
created: 2026-08-13
---

# Radio-Favicons beim Kaltstart, plus zwei Nachzügler aus der Artwork-Review

Umsetzt Punkt **1**, **4a** und **4b** aus
`docs/plans/artwork-load-path-preexisting-weaknesses.md`.

**Ausdrücklich NICHT Teil dieses Auftrags:** Punkt 2 (atomare Durchsetzung der
Cache-Obergrenze in `reprise-core/src/remote_image/cache.rs`) und Punkt 3
(Bild-URLs im Log). Beide bleiben unangetastet — auch nicht „wo wir schon dabei
sind". Wer beim Arbeiten über `enforce_bound` oder über eine URL-loggende Zeile
stolpert: liegen lassen, höchstens in der Handover-Notiz erwähnen.

**`reprise-core` wird in diesem Plan gar nicht angefasst.** Alle Codeänderungen
liegen in `crates/reprise-gnome/src/ui/`, dazu ein neues Skript unter `scripts/`.

Alle Zeilennummern unten sind gegen `origin/dev` = `4912275130` verifiziert
(13.08.2026). Der lokale Haupt-Checkout hängt weit zurück und ist als Referenz
unbrauchbar; im Worktree wird ohnehin frisch von `origin/dev` gebrancht.

---

## Verifizierter Ist-Zustand

| Sache | Stelle auf `origin/dev` |
| --- | --- |
| `static GATE_OPEN: AtomicBool = AtomicBool::new(false)` | `crates/reprise-gnome/src/ui/podcasts/source_image.rs:66` |
| `recompute_gate(conn)` (Schreibpfad aus den Einstellungen) | `source_image.rs:161-166` |
| `gate_open()` (Lesepfad) | `source_image.rs:170-172` |
| Schreibstelle in `load_texture`, direkt vor dem Einreihen | `source_image.rs:374` |
| einziger `recompute_gate`-Aufrufer | `ui/preferences/preferences.rs:473` |
| **einziger** `gate_open()`-Aufrufer | `ui/radio/radio_columns.rs:303` |
| Lesestelle des Workers | `ui/podcasts/source_artwork_queue.rs:138` |
| einziger `source_artwork_queue::queue`-Aufrufer | `source_image.rs:380` |
| `catch_unwind(AssertUnwindSafe(…))` ohne Begründung | `source_artwork_queue.rs:204-206` |
| einziger Panik-Test, Panik im `fetch`-Closure | `source_artwork_queue.rs:425`, Panik in `:437` |
| bereits vorhandenes `images_allowed(db)` für Radio | `ui/radio/add_dialog.rs:35-38` |
| dessen zweiter Nutzer | `ui/radio/add_dialog_rows.rs:10,39` |
| dessen Testabdeckung | `ui/radio/add_dialog_tests.rs:213-227` |

Drei Befunde, die die Entscheidung unten tragen und deshalb explizit
festgehalten sind:

1. **`radio_columns.rs:303` ist der einzige Leser des Atomics** im ganzen Repo
   (`git grep -n 'gate_open' origin/dev` — die weiteren Treffer stehen in
   `docs/plans/ux-rules-motion.md` und sind eine gleichnamige Shell-Variable).
2. **`queue()` hat genau einen Aufrufer**, und der steht sechs Zeilen hinter dem
   `GATE_OPEN.store` (`source_image.rs:374` → `:380`). Ein Worker liest das
   Atomic also nie, ohne dass derselbe Request es vorher frisch geschrieben
   hätte. Der Startwert `false` erreicht die Fetch-Entscheidung strukturell nie.
3. **Jede andere Ansicht rechnet `images_allowed` selbst aus.** Der Modul-Kopf
   von `source_image.rs:5-14` schreibt das sogar als Vertrag fest: „every
   caller passes `images_allowed`, already computed as
   `online_sources::network_allowed(conn, &modules::ARTWORK_MODULE)` at its own
   call site — this widget never reads settings itself." `podcasts_view.rs:391`
   tut das pro Render-Durchgang, `radio/add_dialog.rs:35` pro Aufruf. Die
   Radio-Tabelle ist die **einzige** Stelle, die den Vertrag bricht.

---

## Entscheidung zu Punkt 1: Richtung (b), und (a) entfällt damit

Der Handover nennt zwei Richtungen. Gewählt ist **(b): `radio_columns` rechnet
den Wert selbst**, über dieselbe `images_allowed(&conn)`-Funktion, die der
Radio-Add-Dialog schon benutzt. **(a)** — das Gate einmal beim Fenster-/App-Start
berechnen — wird **nicht** zusätzlich eingebaut.

Begründung, in der Reihenfolge ihrer Tragfähigkeit:

- **(b) beseitigt die Ursache, (a) nur das Symptom.** Der Fehler ist nicht „das
  Atomic ist beim Start leer", sondern „eine Ansicht liest einen prozessweiten
  Nebeneffekt statt ihrer eigenen Eingabe". Mit (a) bliebe genau diese verdeckte
  Kopplung bestehen, nur mit einem zufällig passenden Startwert. Der nächste
  Pfad, der das Atomic mit einem anderen Zeitpunkt füllt, bringt sie zurück.
- **(b) macht die Radio-Tabelle vertragskonform** mit dem Modul-Kopf von
  `source_image.rs:5-14` (siehe Befund 3). Nach (b) hat der Satz „this widget
  never reads settings itself" wieder ausnahmslos recht.
- **(b) ist verifizierbar durch Löschen.** Nach dem Umbau hat `gate_open()`
  keinen Aufrufer mehr und verschwindet (Task 2). Eine Regression müsste die
  Funktion wieder einführen — das fällt in jedem Review auf. (a) hätte kein
  solches Beweisstück.
- **(a) wäre nachweislich wirkungslos für den Worker.** Wegen Befund 2 liest kein
  Worker je den Startwert des Atomics; jeder Request schreibt vorher. Ein
  `recompute_gate` beim Fensterbau würde also nur den Wert setzen, den
  `radio_columns` liest — und genau dieser Leser fällt mit (b) weg. Danach wäre
  (a) toter Code, der zu warten wäre.
- `GATE_OPEN` und `recompute_gate` **bleiben** und behalten ihren Zweck: Sie sind
  der Schreibpfad, über den ein Abschalten in den Einstellungen eine bereits
  eingereihte, noch nicht abgearbeitete Aufgabe stoppt (`NET-1a`, dokumentiert in
  `source_image.rs:44-66` und `:149-160`). Nur der Lesepfad `gate_open()` stirbt.

**Ist das Atomic überhaupt der richtige Mechanismus?** Für den verbleibenden
Zweck ja: Die Worker-Threads haben keinen `Db`, dürfen keinen öffnen (steht so
in `source_image.rs:47-53`), und der Wert muss zwischen Einreihen und Abholen
noch kippen können. Ein `AtomicBool`, der von der GTK-Seite geschrieben und vom
Worker unmittelbar vor `resolve` gelesen wird, ist dafür die passende Form.
Falsch war nur, ihn zusätzlich als *Zustandsquelle für die UI* zu benutzen.
Genau diese Doppelrolle beendet (b).

**Kosten von (b), offen benannt.** Der Wert wird künftig pro Bind einer
Artwork-Zelle aus der Datenbank gelesen (`network_allowed` =
`settings::get_bool_in` + `modules::is_enabled_in`, zwei Punkt-Lookups auf der
bereits offenen Verbindung). Das passiert im selben Bind, der ohnehin einen
`gtk4::Stack`, ein `gtk4::Image` und ein Fallback-Widget baut und die URL durch
`glib::Uri::parse` schickt — die beiden SELECTs sind dort nicht der dominante
Posten. Die Alternative (Wert in einem `Cell` cachen, das die `RadioView` pro
Snapshot auffrischt) wird **nicht** genommen: Sie führt genau die Sorte
Momentaufnahme wieder ein, die `ConnectivitySource` in derselben Datei
(`radio_columns.rs:19-21`) ausdrücklich vermeidet („read at right-click time …
never a stale snapshot"). Siehe „Offene Fragen", die Kosten sind geschätzt und
nicht gemessen.

---

## Entschiedene Kleinfragen

Diese Punkte sind bewusst so festgelegt und keine offenen Enden mehr:

- **`network_allowed(...).unwrap_or(false)`, nicht `network_allowed_or_off(...)`.**
  Die konsolidierte Form ist laut ihrem eigenen Doc-Kommentar
  (`online_sources.rs:104-112`) die für Frontends vorgesehene, loggt aber bei
  jedem Lesefehler eine Warnung — hier also **pro Zeilen-Bind**, beim Scrollen
  fortlaufend. Task 1 erbt deshalb die Form von `radio/add_dialog.rs:35`, die
  `unwrap_or(false)` benutzt, dasselbe Ergebnis liefert und bereits getestet ist
  (`add_dialog_tests.rs:213-227`).
- **`images_allowed` zieht nach `ui/radio/mod.rs`.** `radio_columns` von
  `add_dialog` importieren zu lassen wäre die falsche Abhängigkeitsrichtung: Die
  Tabelle würde am Dialog hängen. Der Fallback bleibt ausdrücklich erlaubt —
  zeigt sich beim Umsetzen, dass der Umzug mehr Aufrufer berührt als er wert ist,
  darf die Funktion in `add_dialog.rs` bleiben und von dort importiert werden.
  Der Vertrag ist „eine Funktion, eine Autorität", nicht ihr Dateiname.
- **Der Doku-Fehler bei `track_list_smoke.rs:52-58`** (die Liste der akzeptierten
  `REPRISE_SMOKE_SOURCE`-Werte ist unvollständig; `parse_smoke_source:125-148`
  nimmt auch `radio`, `podcasts`, `youtube`, `concerts`, `releases` an) wird
  **nicht** mitrepariert. Er ist vorbestehend und gehört nicht zu diesem Auftrag.

---

## Harte Rahmenbedingungen

Diese Punkte haben in diesem Projekt schon Läufe gekostet. Sie gelten für alle
Tasks.

- **Nie ein echtes App-Fenster auf dem Desktop des Nutzers öffnen.** Jede
  Verifikation läuft headless (Xvfb + openbox, isoliertes Profil). Eine
  GTK4-Instanz mit der echten Session findet über den Session-Bus die *laufende*
  App des Nutzers und kapert sie — dann misst der Lauf die falsche Instanz.
  Deshalb: eigenes `XDG_DATA_HOME`/`XDG_CONFIG_HOME`/`XDG_CACHE_HOME` **und**
  `dbus-run-session`.
- **`cargo test -p reprise-gnome --lib` läuft ins Leere** und meldet trotzdem
  Erfolg — dieses Target gibt es dort nicht. Immer `--bin reprise` benutzen.
- **`--exact` ohne exakt passenden Testnamen läuft ebenfalls ins Leere.** Nie
  „keine Fehler im Output" als Erfolg werten. Erfolgsprüfung ausschließlich:
  `grep -c '^test result: FAILED'` = 0 **und** die erwartete Testzahl in
  `test result: ok. N passed` prüfen.
- **Die Display-Suite ist auf `dev` bereits teilweise rot und im Rudel flaky.**
  Rot heißt dort nicht automatisch Eigenschaden — siehe „Vorschaden von
  Eigenschaden trennen".
- **Nicht stashen.** `git stash` ist repo-global und trifft die parallel
  laufenden Worktrees. Wo ein zweiter Stand gebraucht wird, wird ein zweiter
  Worktree angelegt.
- **Dateigrößen-Lint** (`scripts/check-architecture.sh:20-24`): jede `.rs` unter
  `crates/` muss **unter 800 Zeilen** bleiben. Stand der betroffenen Dateien auf
  `origin/dev`:

  | Datei | Zeilen | Luft |
  | --- | ---: | ---: |
  | `ui/radio/radio_view.rs` | 794 | **5** |
  | `ui/radio/add_dialog.rs` | 746 | 53 |
  | `ui/podcasts/source_image.rs` | 733 | 66 |
  | `ui/radio/radio_columns.rs` | 670 | 129 |
  | `ui/podcasts/source_artwork_queue.rs` | 453 | 346 |

  `radio_view.rs` hat **fünf** Zeilen Luft. Task 1 ist deshalb so zugeschnitten,
  dass dort **genau eine** Zeile hinzukommt. Wer dort mehr braucht, verschiebt
  vorher einen bestehenden Block heraus, statt den Lint zu reißen.

### Dateilisten sind Startpunkt, kein Zaun

> Angrenzende Dateien dürfen minimal geändert und in der Commit-Message genannt
> werden; anhalten nur, wenn der *Vertrag* falsch ist.

Konkret: Wenn eine Signaturänderung Aufrufer in Dateien berührt, die unten nicht
aufgezählt sind, werden diese Aufrufer mitgezogen — das ist erwartet und kein
Grund, den Task abzubrechen. Angehalten und zurückgemeldet wird nur, wenn sich
beim Lesen herausstellt, dass eine der oben tabellierten Fundstellen auf
`origin/dev` nicht mehr stimmt oder die gewählte Richtung dort strukturell nicht
trägt.

---

## Task 1 — Die Radio-Tabelle rechnet ihre Bild-Erlaubnis selbst

**Ziel.** `artwork_column` in der Radio-Tabelle leitet `images_allowed` aus den
Einstellungen ab statt aus dem prozessweiten Atomic. Damit zeigt eine Sitzung,
die direkt in der Radio-Ansicht startet, die Favicons eines zustimmenden Nutzers
— ohne dass vorher irgendeine andere Ansicht das Atomic gefüllt haben muss.

**Startpunkt-Dateien.**

- `crates/reprise-gnome/src/ui/radio/radio_columns.rs` (Leser `:257-313`,
  `append_columns` `:315-321`, Typ-Aliase `:18-21`)
- `crates/reprise-gnome/src/ui/radio/radio_view.rs` (`RadioView::new` hält
  `conn: Rc<Db>`, Aufruf von `append_columns` bei `:137`)
- `crates/reprise-gnome/src/ui/radio/add_dialog.rs:35-38` (die vorhandene
  `images_allowed`-Funktion)
- `crates/reprise-gnome/src/ui/radio/add_dialog_rows.rs:10` (deren Import)
- `crates/reprise-gnome/src/ui/radio/mod.rs` (38 Zeilen — neuer Ort)

**Vorgehen.**

1. **Eine Autorität für „darf Radio Bilder holen".** `images_allowed(db)` existiert
   bereits in `add_dialog.rs:35` und ist genau die gesuchte Rechnung
   (`network_allowed(db, &ARTWORK_MODULE).unwrap_or(false)`). Sie wird
   wiederverwendet, **keine zweite Kopie**. Sie zieht nach `ui/radio/mod.rs`
   (siehe „Entschiedene Kleinfragen"); die bisherigen Nutzer
   (`add_dialog.rs:701`, `add_dialog_rows.rs:39`, `add_dialog_tests.rs`) ziehen
   den Import nach.
2. **Injektionsnaht in `radio_columns.rs`**, in der Form, die diese Datei schon
   zweimal benutzt (`LiveState` `:18`, `ConnectivitySource` `:21`):
   `pub(super) type ImagesAllowedSource = Rc<dyn Fn() -> bool>;` mit einem
   Doc-Kommentar in derselben Machart wie der von `ConnectivitySource`: zur
   Bind-Zeit gelesen, damit eine gebundene Zelle nie eine veraltete
   Zustimmungs-Momentaufnahme trägt (`NET-1a`/`SRC-11`).
3. **Konstruktor in `radio_columns.rs`, nicht in `radio_view.rs`:**
   `pub(super) fn images_allowed_source(conn: &Rc<Db>) -> ImagesAllowedSource`,
   die den `Rc<Db>` klont und `super::images_allowed(&conn)` aufruft. Grund: In
   `radio_view.rs` sind nur **5** Zeilen Luft bis zum 800-Zeilen-Lint (siehe
   Rahmenbedingungen); so kommt dort genau eine Zeile hinzu — das neue Argument
   am `append_columns`-Aufruf `:137`.
4. `append_columns` und `artwork_column` nehmen die Quelle entgegen;
   `artwork_column`s `connect_bind` ruft sie auf und übergibt das Ergebnis an
   `SourceImage::new_after_startup_with_initials` statt `gate_open()`
   (`radio_columns.rs:303`).
5. Die vier bestehenden `append_columns`-Aufrufe in den Tests derselben Datei
   (`:465`, `:517`, `:552`, `:581`) bekommen das neue Argument. Wo kein `Db`
   nötig ist, reicht `Rc::new(|| false)` bzw. `Rc::new(|| true)`.

**Definition of Done.**

- `radio_columns.rs` enthält den Bezeichner `gate_open` nicht mehr.
- Neuer Test (nicht display-gebunden) in `radio_columns.rs`, nach dem Vorbild von
  `add_dialog_tests.rs:213-227`: Aus einer frischen Test-Datenbank
  (`crate::test_db::open()`) liefert `images_allowed_source(&conn)()`
  – `false` bei Voreinstellung,
  – `true` nach `modules::set_enabled(ARTWORK_MODULE, true)` +
    `online_sources::set_enabled(true)`,
  – `false`, wenn der globale Schalter aus ist, das Modul aber an.
  Der Test läuft in einem Prozess, in dem nichts das Atomic veröffentlicht hat —
  das ist die Kaltstart-Regression in Testform.
- Zweiter, sehr billiger Test nach dem Muster von
  `src_4a_the_state_cell_offers_no_hover_star` (`radio_columns.rs:638-646`):
  `include_str!("radio_columns.rs")` enthält weder `gate_open` noch
  `source_image::gate`. Das hält die entkoppelte Form fest, ohne einen Display zu
  brauchen.
- `scripts/check-architecture.sh` grün (insbesondere `radio_view.rs` < 800).

**Verifikation.**

```bash
cd <worktree>
mkdir -p .tmp/logs
cargo test -p reprise-gnome --bin reprise radio_ \
  > .tmp/logs/t1-radio.log 2>&1; echo "exit=$?"
grep -c '^test result: FAILED' .tmp/logs/t1-radio.log   # muss 0 sein
grep '^test result:' .tmp/logs/t1-radio.log             # muss "ok. N passed" zeigen, N > 0
cargo test -p reprise-gnome --bin reprise source_image \
  > .tmp/logs/t1-source-image.log 2>&1
grep -c '^test result: FAILED' .tmp/logs/t1-source-image.log
scripts/check-architecture.sh > .tmp/logs/t1-arch.log 2>&1; echo "arch=$?"
```

Erwartung: beide `FAILED`-Zählungen 0, `arch=0`, und in `t1-radio.log` steht eine
Zeile `test result: ok.` mit mindestens den beiden neuen Tests.

---

## Task 2 — `gate_open()` entfernen und die Doku nachziehen

**Ziel.** Der Lesepfad des Atomics verschwindet; der Schreibpfad und seine
Begründung bleiben korrekt beschrieben.

**Startpunkt-Datei.** `crates/reprise-gnome/src/ui/podcasts/source_image.rs`
(Modul-Kopf `:16-22`, Doc über `GATE_OPEN` `:44-66`, `recompute_gate` `:149-166`,
`gate_open` `:168-172`).

**Vorgehen.**

1. `gate_open()` (`:170-172`) samt Doc-Kommentar löschen. Nach Task 1 hat sie
   keinen Aufrufer mehr; ohne Löschung schlägt der Dead-Code-Lint zu.
2. Doku begradigen: Der Satz in `:168-169` („Current source-artwork permission
   for rows that bind after the gate was published by startup or Preferences")
   beschrieb genau die Kopplung, die jetzt weg ist, und verschwindet mit der
   Funktion. Im Modul-Kopf (`:16-22`) und im `GATE_OPEN`-Doc (`:44-66`) wird
   klargestellt, dass das Atomic **ausschließlich** der Fetch-Zeit-Kanal zu den
   Worker-Threads ist: geschrieben von `load_texture` (`:374`) und von
   `recompute_gate` (`:165`), gelesen **nur** vom Worker
   (`source_artwork_queue.rs:138`). Kein UI-Pfad liest ihn.
3. `recompute_gate` und sein Aufrufer in `preferences.rs:473` bleiben unverändert.
   Ebenso die beiden Tests `source_image.rs:588-613` und alle
   `GATE_OPEN`-Manipulationen in den Tests von `source_artwork_queue.rs`.

**Definition of Done.**

- `git grep -n 'gate_open' -- crates` liefert keinen Treffer mehr.
- `git grep -n 'recompute_gate' -- crates` liefert weiterhin `source_image.rs`
  (Definition + zwei Tests) und `preferences.rs:473`.
- Kein `dead_code`-Warning; Clippy strikt grün.

**Verifikation.**

```bash
git grep -n 'gate_open' -- crates ; echo "treffer=$?"   # 1 = kein Treffer = gut
cargo clippy -p reprise-gnome --all-targets -- -D warnings \
  > .tmp/logs/t2-clippy.log 2>&1; echo "clippy=$?"
cargo test -p reprise-gnome --bin reprise src_11 \
  > .tmp/logs/t2-src11.log 2>&1
grep -c '^test result: FAILED' .tmp/logs/t2-src11.log
```

---

## Task 3 — Der Beweis: Kaltstart direkt in die Radio-Ansicht

**Ziel.** Der Lauf, den der Handover ausdrücklich schuldet: isoliertes Profil,
leerer Bild-Cache, zugestimmter Nutzer, App startet **direkt** in der
Radio-Ansicht → Favicons sind da. Grüne Tests beweisen hier nichts.

**Startpunkt-Dateien.**

- `scripts/verify-now-playing-scene.sh` — **zuerst lesen**; das ist der
  vorhandene Zuschnitt „ein Skript, eine visuelle Abnahme" und die Vorlage.
- `scripts/ptr-e2e/run.sh:300-330` — die belastbare Rezeptur für Display, Profil
  und Session-Bus (Xvfb, openbox, `GDK_BACKEND=x11`, `WAYLAND_DISPLAY=`,
  `GTK_A11Y=none`, `NO_AT_BRIDGE=1`, `REPRISE_AUDIO_SINK=fakesink`,
  `REPRISE_LOG=debug`, `dbus-run-session`, Screenshot per `import`).
- `crates/reprise-gnome/src/ui/window/library_shell.rs:133-153`
  (`arm_smoke_detail_view`) — der Hook, der `REPRISE_SMOKE_SOURCE=radio` per
  `sidebar.refresh_and_select(ViewSource::Radio, …)` in die Radio-Ansicht
  schickt. `parse_smoke_source` (`track_list_smoke.rs:125-148`) akzeptiert
  `"radio"`; der Doc-Kommentar bei `track_list_smoke.rs:52-58` zählt es
  fälschlich nicht mit auf — der Code gilt.
- `crates/reprise-gnome/src/ui/window/window_smoke.rs:12-34` (`REPRISE_SMOKE_QUIT`
  und `REPRISE_SMOKE_QUIT_DELAY_SECS`, Vorgabe 3 s).

**Bekannte Pfade — nicht mehr zu raten.**

| Sache | Wert |
| --- | --- |
| Datenbank | `$XDG_DATA_HOME/reprise/reprise.db` (`db::default_path`, `db.rs:88-92`, über `dirs::data_dir()`) |
| Bild-Cache | `$XDG_CACHE_HOME/reprise/covers/remote-images-persistent/` (`remote_image/cache.rs:20-31` über `cover::cache_dir`, `cover.rs:191-202`, `dirs::cache_dir()`) |
| Startruhe | `QUIET_INTERVAL = 100 ms` (`ui/startup_quiet.rs:14`), ausgelöst nach dem ersten gemalten Frame plus einem Low-Priority-Slot |
| HTTP-Zeitlimit je Abruf | 15 s (`podcasts/source_artwork.rs:15`), 8 Worker parallel (`source_artwork_queue.rs:10`) |
| Stationsschema | `radio_stations(id, uuid, name, stream_url, homepage, favicon_url, genre, codec, bitrate_kbps, country_code, votes, added_at, removed_at)` (`db_podcasts_radio.rs:45-58`) |

**Vorgehen.**

1. **Neues Skript `scripts/verify-radio-favicons.sh`.** Es nimmt das zu prüfende
   Repo-Wurzelverzeichnis als Argument (Vorgabe: das eigene), baut dort, hebt ein
   Xvfb + openbox, legt ein Scratch-Profil an (`XDG_DATA_HOME`, `XDG_CONFIG_HOME`,
   `XDG_CACHE_HOME` unter einem `mktemp -d`), startet die App unter
   `dbus-run-session`, macht Screenshots und schreibt alle Belege nach
   `RADIO_FAVICON_OUT_DIR`. **Dasselbe Skript** fährt beide Läufe — nur das
   Arbeitsverzeichnis unterscheidet sich.
2. **Bau ohne `--features test-fixtures`.** `ptr-e2e` setzt das Feature, weil es
   `REPRISE_MUSICBRAINZ_FIXTURE_DIR` benutzt, um MusicBrainz offline zu halten
   (`run.sh:317-318`). Dieser Lauf braucht das Gegenteil: Die Artwork-Abrufe
   sollen echt hinausgehen, und für Artwork existiert ohnehin keine
   Fixture-Route. Das Profil hat keine Musik und keine Abos, außer Radio und
   Artwork ist kein Modul eingeschaltet — also kann auch nichts anderes eine
   Anfrage stellen. Das hält die Cache-Zählung unten sauber.
3. **Datenbank vorbereiten.** Die App einmal mit kurzem `REPRISE_SMOKE_QUIT`
   starten, damit `db::open_migrated` das Schema anlegt; danach mit `sqlite3` in
   `$XDG_DATA_HOME/reprise/reprise.db` schreiben. Die Bool-Kodierung der
   Settings-Werte **aus `settings::set_bool_in`/`get_bool_in` ablesen**, nicht
   `'1'` oder `'true'` vermuten. Zu setzen:
   - `online-sources-enabled` = an
   - `module.artwork.enabled` = an
   - `module.radio.enabled` = an
   - `online_sources.first_enable_completed` = an (sonst kann ein späterer
     Toggle die Modulwahl neu setzen, `online_sources.rs:42-58`)
   - drei Zeilen in `radio_stations` mit paarweise verschiedenen
     `stream_url`-Werten (die Spalte ist `UNIQUE`), gesetztem `added_at`,
     `removed_at` NULL und diesen `favicon_url`:
     - `https://raw.githubusercontent.com/marvinbaudach/reprise/dev/data/brand/favicon-32.png`
     - `https://raw.githubusercontent.com/marvinbaudach/reprise/dev/data/brand/apple-touch-icon-180.png`
     - `https://raw.githubusercontent.com/marvinbaudach/reprise/dev/data/brand/play-store-icon-512.png`

     Alle drei sind auf `origin/dev` vorhanden (per `git ls-tree` geprüft), das
     Repo ist öffentlich. **Sie zeigen dasselbe Markenmotiv in drei Größen** —
     die drei Kacheln lassen sich also nicht voneinander unterscheiden. Für den
     Beweis genügt das: Es zählt „Bild statt Initialen", nicht „drei verschiedene
     Bilder". Die drei URLs sind trotzdem verschieden, und nur darauf beruht die
     Cache-Zählung (der Cache-Schlüssel ist der URL-Hash,
     `remote_image/cache.rs:52-54`).
4. **Vorflug-Prüfung, nicht optional.** Vor jedem App-Start prüft das Skript alle
   drei URLs:

   ```bash
   curl -sS -o /dev/null -w '%{http_code} %{num_redirects}\n' "$url"
   ```

   Erwartet: `200 0` für jede. **Warum das sein muss:** Ein fehlgeschlagener
   Abruf erzeugt **gar keine Logzeile** — `remote_image::resolve` faltet den
   Fehler stumm zu `ImageOutcome::FetchFailed` (`remote_image/mod.rs:85-92`), der
   Wartende bekommt `Ok(None)` und `source_image.rs:383` kehrt wortlos zurück.
   Ohne Vorflug wäre ein Nachher-Lauf mit 0 Cache-Dateien nicht von einem
   widerlegten Fix zu unterscheiden. Der `num_redirects`-Teil ist ebenfalls
   nötig: `source_artwork::fetch` fährt mit `max_redirects(0)`, eine Umleitung
   würde als `HttpStatus`-Fehler enden.
5. **Leerer Bild-Cache ist Vorbedingung *und* Beweisträger.** Das frische
   `XDG_CACHE_HOME` liefert ihn; das Skript prüft vor dem Start, dass
   `$XDG_CACHE_HOME/reprise/covers/remote-images-persistent/` fehlt oder leer
   ist, und bricht sonst ab. Bei warmem Cache liefert `process_job` das Bild aus
   dem cache-only-`resolve` (`source_artwork_queue.rs:123-137`) noch **vor** der
   Gate-Abfrage (`:138`) — die Favicons erschienen dann auch mit dem Fehler, und
   der Lauf wäre wertlos.
6. **App-Umgebung:** `REPRISE_SMOKE_SOURCE=radio`, `REPRISE_AUDIO_SINK=fakesink`,
   `REPRISE_LOG=debug`, `REPRISE_SMOKE_QUIT=1`,
   **`REPRISE_SMOKE_QUIT_DELAY_SECS=25`**. Begründung der Zahl: Die Startruhe
   fällt mit 100 ms nicht ins Gewicht (`startup_quiet.rs:14`); die Wartezeit wird
   von den drei echten HTTPS-Abrufen bestimmt. Die drei laufen auf verschiedenen
   der acht Worker parallel, das Zeitlimit je Abruf ist 15 s — 25 s decken also
   einen vollen Timeout-Fall plus Fensterbau und TLS-Handschlag ab und beenden
   den Lauf trotzdem zügig, wenn der Host nicht erreichbar ist. Die Vorgabe von
   3 s (`window_smoke.rs:15`) wäre viel zu knapp.
7. **Screenshots** bei festen Zeitpunkten (z. B. 8 s, 16 s, 23 s nach dem Start),
   damit ein langsamer Abruf nicht die einzige Aufnahme entwertet. Die
   Cache-Zählung nach dem Beenden ist von der Aufnahmezeit unabhängig und bleibt
   der harte Beleg.
8. **Zwei Läufe, sonst beweist der Lauf nichts.**
   - **Vorher-Lauf** gegen einen zweiten Worktree auf `origin/dev`:
     `git worktree add /tmp/reprise-dev-favicons origin/dev`, dann das Skript mit
     diesem Verzeichnis als Argument. **Kein** `git stash`, **kein**
     `git checkout <merge-base> -- <dateien>`. Der zweite Build ist kalt und
     teuer — über den Lastregler fahren, damit er nicht mit anderen Sessions um
     Kerne kämpft.
   - **Nachher-Lauf** im eigenen Worktree, nach Task 1 + 2.
   - Der Worktree wird am Ende wieder entfernt (`git worktree remove`).
9. **Belege ablegen** und ihre Pfade in der Handover-Notiz nennen: `before-*.png`,
   `after-*.png`, `app-before.log`, `app-after.log`, `preflight-before.txt`,
   `preflight-after.txt`, `cache-before.txt`, `cache-after.txt` sowie die genaue
   Kommandozeile beider Läufe.

**Definition of Done.**

- **Nachher:** genau **3** Dateien in
  `$XDG_CACHE_HOME/reprise/covers/remote-images-persistent/`.
  **Vorher:** **0** Dateien. Das ist der harte, nicht-visuelle Nachweis: Ohne den
  Fix fragt die Radio-Ansicht nie nach, mit dem Fix landen die drei Bilder auf
  der Platte.
- Beide Vorflug-Prüfungen zeigen dreimal `200 0`. Ohne das ist der jeweilige Lauf
  ungültig und wird wiederholt — nicht als Ergebnis gewertet.
- `after-*.png` zeigt in der Artwork-Spalte die Reprise-Marke, `before-*.png` die
  Initialen-Kacheln. Beide Screenshots bestehen den Blankness-Check (nicht
  schwarz, nicht leer) — sonst ist der Lauf ungültig, nicht der Fix bewiesen.
- `app-after.log` enthält die Zeile aus `arm_smoke_detail_view` („smoke: opening
  detail view through sidebar source routing") mit `source=radio` — Beleg, dass
  die Sitzung wirklich in der Radio-Ansicht war und nicht in der Bibliothek.
- Kein Fenster ist je auf dem Desktop des Nutzers erschienen; jeder Lauf lief
  unter eigenem `DISPLAY` und eigenem Session-Bus.

**Verifikation.**

```bash
OUT=/tmp/reprise-radio-favicons

# Vorher (origin/dev)
git worktree add /tmp/reprise-dev-favicons origin/dev
RADIO_FAVICON_OUT_DIR=$OUT/before \
  scripts/verify-radio-favicons.sh /tmp/reprise-dev-favicons > .tmp/logs/t3-before.log 2>&1
echo "before-exit=$?"

# Nachher (dieser Branch)
RADIO_FAVICON_OUT_DIR=$OUT/after \
  scripts/verify-radio-favicons.sh > .tmp/logs/t3-after.log 2>&1
echo "after-exit=$?"

# Der harte Beleg — die Zahlen stehen in den vom Skript geschriebenen Dateien:
cat $OUT/before/cache-before.txt   # 0
cat $OUT/after/cache-after.txt     # 3

# Vorflug muss in beiden Läufen dreimal "200 0" zeigen:
cat $OUT/before/preflight-before.txt
cat $OUT/after/preflight-after.txt

grep 'opening detail view' $OUT/after/app-after.log

git worktree remove /tmp/reprise-dev-favicons
```

Die Zählung im Skript selbst:

```bash
find "$XDG_CACHE_HOME/reprise/covers/remote-images-persistent" -type f 2>/dev/null | wc -l
```

---

## Task 4 — `AssertUnwindSafe` bekommt seine Begründung (Punkt 4a)

**Ziel.** Die Zusicherung wird als das benannt, was sie ist: eine Aussage über
den konkret übergebenen Closure, die der Typ nicht erzwingt.

**Startpunkt-Datei.** `crates/reprise-gnome/src/ui/podcasts/source_artwork_queue.rs:198-212`
(`run_worker`, `catch_unwind` bei `:204-206`).

**Vorgehen.** Genau **eine** erklärende Zeile (bei Bedarf zwei, wenn der Satz
sonst nicht lesbar wird) unmittelbar über dem `catch_unwind`, in der Machart der
`// SAFETY:`-Disziplin des Projekts. Sie muss die tatsächliche Annahme benennen,
nicht nur wiederholen, dass gefangen wird: Der `fetch`-Closure ist heute
zustandslos (er umschließt die freie Funktion
`reprise_core::podcasts::source_artwork::fetch`, siehe `:42-45`); derselbe
`&mut dyn FnMut` wird für **jeden weiteren Auftrag der Schleife**
wiederverwendet, also bricht die Annahme still, sobald jemand ihm eigenen
Zustand gibt (Wiederholungszähler, kleiner Verbindungs-Cache) und nach einer
Panik ein halbfertiger Zustand weiterlebt.

**Kein Umbau.** `run_worker`, `process_job` und `finish_without_image` bleiben
zeichengleich.

**Definition of Done.** Der Diff dieses Tasks besteht ausschließlich aus
Kommentarzeilen. `cargo fmt --check` und Clippy grün.

**Verifikation.**

```bash
git diff --stat                      # nur source_artwork_queue.rs, nur + bei Kommentaren
cargo fmt --all --check; echo "fmt=$?"
cargo test -p reprise-gnome --bin reprise src_11_ > .tmp/logs/t4.log 2>&1
grep -c '^test result: FAILED' .tmp/logs/t4.log
```

---

## Task 5 — Panik-Test für das Dekodieren (Punkt 4b)

**Ziel.** Der wahrscheinlichere Panikort ist abgedeckt: eine Panik **nach**
`std::mem::take(waiters)` (`source_artwork_queue.rs:158`), also im Dekodierteil
der Schleife, hinterlässt keine gestrandete URL.

**Startpunkt-Dateien.**

- `crates/reprise-gnome/src/ui/podcasts/source_artwork_queue.rs` (Schleife
  `:150-195`, `run_worker` `:198-212`, vorhandener Panik-Test `:424-452` als
  Formvorlage)
- `crates/reprise-gnome/src/ui/podcasts/source_image_texture.rs:33-52`
  (`decode_pixels`)

**Sachkorrektur zum Handover.** Dort steht, `decode_pixels` „schickt geladene
Bytes durch die pixbuf-FFI". Das stimmt nicht: Die Funktion nimmt ein `&Path`
und ruft `Pixbuf::from_file_at_scale(path, width*2, height*2, true)` — sie liest
die Datei selbst, es werden keine Bytes hineingereicht. Am Befund ändert das
nichts (der Aufruf liegt hinter `mem::take` und geht durch die FFI), aber die
Testkonstruktion muss von der echten Signatur ausgehen.

**Was der Test festnagelt.**

1. Nach der Panik im Dekodieren wird der `pending`-Eintrag entfernt.
2. Eine spätere Anfrage für **dieselbe** URL startet einen frischen Auftrag und
   läuft durch (`Ok(Some(_))`).
3. Die Wartenden der abgebrochenen Charge sehen einen **geschlossenen Kanal**
   (`Err`), nicht `Ok(None)`. **Das ist so gewollt und bleibt so.**
   `source_image.rs:381-389` behandelt beide Fälle identisch (beide enden auf
   `return`, die Kachel bleibt auf dem Fallback). Die Antwort-Semantik gegenüber
   Wartenden wird **nicht** angefasst.

**Vorgehen.**

1. **Erst herausfinden, wie die Panik zuverlässig ausgelöst wird**, mit einem
   Wegwerf-Probelauf, **bevor** der Test geschrieben wird:
   - **Kandidat (a): Breite/Höhe 0 durch den Wartenden.** `decode_pixels` bekommt
     `waiter.width`/`waiter.height` (`:174`), und die kommen direkt aus
     `submit(url, width, height, scope)` — also aus der Hand des Tests. Bei `0`
     greift in gdk-pixbuf die `g_return_val_if_fail`-Vorbedingung von
     `gdk_pixbuf_new_from_file_at_scale`: Rückgabe NULL **ohne** gesetzten
     `GError`. Die gtk-rs-Bindung nimmt daraufhin den `Ok`-Zweig, und
     `from_glib_full` assertiert auf dem Null-Zeiger — eine echte, entrollende
     Panik genau an der richtigen Stelle der Schleife. `saturating_mul(2)`
     (`source_image_texture.rs:38-40`) macht aus 0 wieder 0, der Weg ist also
     offen. Der Probelauf muss dennoch bestätigen, dass es **paniert und nicht
     abbricht** — etwa wenn in der Testumgebung `G_DEBUG=fatal-criticals` gesetzt
     ist, wird aus der Kritik ein Abbruch, den `catch_unwind` nicht fängt.
   - **Kandidat (b), ausdrücklich freigegeben, falls (a) nicht trägt:** eine eng
     umrissene, ausschließlich `#[cfg(test)]`-sichtbare Dekodier-Naht in
     `process_job` — der Dekodieraufruf geht über eine Hilfsfunktion, die im
     Testfall einen hinterlegten Haken aufruft. Das ändert **kein**
     Laufzeitverhalten und **keine** Antwort-Semantik. Das Umbau-Verbot des
     Handovers zielt allein auf die `Ok(None)`-vs-geschlossener-Kanal-Frage
     gegenüber Wartenden; die bleibt unangetastet. **Codex hält deswegen nicht an
     und fragt nicht nach** — (b) ist genehmigt.
   Welche Variante gewählt wurde und warum, gehört als zwei Sätze in den
   Testkommentar.
2. **Aufbau** wie der bestehende Test `:424-452`: `GATE_TEST_LOCK` halten,
   `GATE_OPEN` auf `true`, `ArtworkQueue::test_queue()`, `run_worker` in einem
   eigenen Thread, `fetch`-Closure liefert `TINY_PNG` (`:260-266`) — die Panik
   kommt diesmal **nicht** aus `fetch`.
3. **Rennen vermeiden — das ist die eigentliche Falle dieses Tests.** Die Panik
   verwirft den bereits aus `pending` entnommenen Wartenden; sein `Sender` fällt
   während des Entrollens, der Empfänger wacht also mit `Err` auf, **bevor**
   `run_worker` `finish_without_image` (`:209`) ausgeführt und den Schlüssel
   entfernt hat. Ein sofortiges zweites `submit` derselben URL würde dann an den
   noch bestehenden Eintrag angehängt, **keinen** neuen Auftrag einreihen und
   ewig warten. Der bestehende Test `:424` hat dieses Problem nicht, weil dort
   `finish_without_image` selbst antwortet und der Empfänger erst danach
   aufwacht. Deshalb: Der Test wartet mit einer Frist (z. B. 5 s, Abbruch mit
   klarer Meldung) darauf, dass `queue.pending` den Schlüssel nicht mehr enthält
   — der Test liegt im selben Modul und kann `super::ArtworkKey { url }` bilden —
   und reicht erst danach die zweite Anfrage ein.
4. Name in der Konvention der Datei, z. B.
   `src_11_a_panic_while_decoding_frees_the_url_for_a_fresh_job`.

**Definition of Done.**

- Der neue Test läuft ohne `#[ignore]` (kein Display nötig) und ist zehnmal
  hintereinander grün — die Rennfreiheit aus Schritt 3 ist damit belegt, nicht
  behauptet.
- Der bestehende Test `:425` bleibt unverändert und grün.
- Die Antwort-Semantik von `process_job` gegenüber Wartenden ist unverändert.

**Verifikation.**

```bash
for i in $(seq 1 10); do
  cargo test -p reprise-gnome --bin reprise src_11_a_panic_while_decoding \
    >> .tmp/logs/t5-loop.log 2>&1
done
grep -c '^test result: FAILED' .tmp/logs/t5-loop.log     # muss 0 sein
grep -c '^test result: ok' .tmp/logs/t5-loop.log         # muss 10 sein
cargo test -p reprise-gnome --bin reprise source_artwork_queue \
  > .tmp/logs/t5.log 2>&1
grep -c '^test result: FAILED' .tmp/logs/t5.log
```

Der zweite Zähler ist wichtig: Ein Tippfehler im Filter liefert „0 failed" bei
null gelaufenen Tests und sähe wie Erfolg aus.

---

## Parallelität und Reihenfolge

```
Strang A (Fix + Beweis, sequenziell):
  Task 1  →  Task 2  →  Task 3 (Vorher-Lauf und Nachher-Lauf)

Strang B (unabhängig, parallel zu A startbar):
  Task 4  →  Task 5
```

- **Task 4 und 5 gehören zusammen in einen Strang**, weil beide dieselbe Datei
  besitzen (`source_artwork_queue.rs`). Zwei parallele Agenten auf derselben
  Datei enden in Konflikten; nacheinander im selben Strang, aber als getrennte
  Commits.
- **Task 2 ist erst nach Task 1 compilierbar** (vorher hat `gate_open` noch einen
  Aufrufer).
- **Task 3 kommt nach Task 1 und 2**, weil der Nachher-Lauf den Fix braucht. Der
  Vorher-Lauf braucht ihn nicht — er läuft in einem eigenen Worktree auf
  `origin/dev` und kann jederzeit gefahren werden, auch schon während Task 1
  läuft, wenn Kerne frei sind.
- Strang A und Strang B berühren keine gemeinsame Datei und dürfen in getrennten
  Worktrees laufen. Wenn beides in einem Worktree läuft: erst A, dann B, jeweils
  eigene Commits.

---

## Commit-Zuschnitt

Fokussierte Commits, englische Nachrichten im Format `<type>: <description>`:

1. `fix: compute radio artwork permission from settings, not the published gate`
   — Task 1 (`ui/radio/*`; nennt die mitgezogenen Aufrufer in der Body-Zeile).
2. `refactor: drop the read side of the source-artwork gate` — Task 2
   (`ui/podcasts/source_image.rs`).
3. `test: prove radio favicons load on a cold start` — Task 3 (Skript + die in
   der Nachricht referenzierten Belegpfade).
4. `docs: name the unwind-safety assumption of the artwork worker` — Task 4.
5. `test: cover a panic while decoding queued source artwork` — Task 5.

Kein Commit fasst zwei Tasks zusammen. Commit 2 darf nicht vor Commit 1 landen.

---

## Vorschaden von Eigenschaden trennen

Die Display-Suite ist auf `dev` bereits teilweise rot und im Rudel flaky. Bevor
irgendein roter Lauf diesem Branch zugeschrieben wird:

```bash
# im Worktree, auf dem Branch:
scripts/check-display-tests.sh --rule-named > .tmp/logs/display-branch.log 2>&1
grep -E '^(FAIL|not ok|.*has [0-9]+ lines)' .tmp/logs/display-branch.log | sort > .tmp/logs/fails-branch.txt

# Gegenprobe: derselbe Aufruf auf origin/dev, in einem separaten Worktree
git worktree add /tmp/dev-check origin/dev
(cd /tmp/dev-check && scripts/check-display-tests.sh --rule-named) \
  > .tmp/logs/display-dev.log 2>&1
grep -E '^(FAIL|not ok)' .tmp/logs/display-dev.log | sort > .tmp/logs/fails-dev.txt

diff .tmp/logs/fails-dev.txt .tmp/logs/fails-branch.txt
```

Nur Zeilen, die **ausschließlich** in `fails-branch.txt` stehen, sind
Eigenschaden. Alles andere ist Vorschaden und wird in der Handover-Notiz
festgehalten, nicht repariert. Ein einzelner roter Testname wird vor dem Urteil
dreimal einzeln nachgefahren — das Rudel ist flaky, der Einzellauf meist nicht.

Zusätzlich, weil `check-display-tests.sh --rule-named` nur Tests mit einem
aktiven Regelpräfix fährt (`:65`): Die in Task 1 und Task 5 neu geschriebenen
Tests sind bewusst **nicht** display-gebunden und laufen im normalen
`cargo test -p reprise-gnome --bin reprise`. Sie fallen also nicht in die Lücke
der ungelaufenen ignorierten Tests.

---

## Was dieser Plan bewusst liegen lässt

Damit der nächste Leser nicht glaubt, hier sei alles erledigt:

- **Punkt 2 des Handovers** (`enforce_bound` setzt die Cache-Obergrenze nicht
  atomar durch, `reprise-core/src/remote_image/cache.rs:128-149`). Selbstheilend,
  nicht angefasst.
- **Punkt 3 des Handovers** (Bild-URLs im Debug-Log). Wartet laut Handover auf
  einen Anlass; nicht angefasst.
- **Es gibt keine Fixture-Route für Quellen-Artwork.** Podcasts, Radio, Concerts,
  Lyrics und MusicBrainz haben je ein `REPRISE_*_FIXTURE_DIR`; Artwork nicht, weil
  `source_artwork::fetch` einen eigenen ureq-Agenten baut und an
  `podcasts::http` vorbeigeht. Zusammen mit `validate_remote_url` und dem
  `PublicOnlyResolver` (`podcasts/source_artwork.rs:33-46`, `:76-105`), die jede
  lokale Gegenstelle sperren, heißt das: **Artwork ist headless nicht offline
  testbar.** Deshalb geht der Beweis in Task 3 gegen echte URLs. Das ist eine
  echte Lücke und wird wieder beißen, sobald jemand Artwork in einer
  E2E-Umgebung braucht — dann ist eine feature-gegatete Fixture-Route nach dem
  Vorbild von `concerts/http.rs:15-110` der Weg.
- **Der Doku-Fehler bei `track_list_smoke.rs:52-58`** (unvollständige Liste der
  `REPRISE_SMOKE_SOURCE`-Werte) bleibt stehen.
- **`network_allowed_or_off`** wird hier bewusst nicht benutzt (Begründung oben).
  Wenn das Projekt die konsolidierte Form erzwingen will, ist das eine eigene,
  kleine Aufräumaufgabe, die dann auch `radio/add_dialog.rs:35` betrifft.

---

## Offene Fragen / Widersprüche

1. **Die Bind-Zeit-Kosten aus Task 1 sind geschätzt, nicht gemessen.** Zwei
   SQLite-Punkt-Lookups pro gebundener Artwork-Zelle sind gegenüber dem
   Widgetbau derselben Bind-Runde plausibel vernachlässigbar, aber niemand hat es
   hier nachgemessen. Wer das anders wiegt, hat als Alternative den in einer
   `Cell` gepufferten Wert samt Auffrischpunkten — mit dem dokumentierten
   Nachteil, dass eine Änderung in den Einstellungen bereits gebundene Zeilen
   erst beim nächsten Snapshot erreicht.
2. **Die Panik-Quelle in Task 5 ist unbewiesen.** Kandidat (a) (Breite 0 →
   Null-Zeiger in der gtk-rs-Bindung) ist aus dem Code hergeleitet, aber nicht
   ausgeführt. Sollte er statt einer entrollenden Panik einen Prozessabbruch
   auslösen, ist der Test so nicht baubar — dann greift die ausdrücklich
   freigegebene Rückfallebene (b), die Test-Naht. Das kann erst beim Umsetzen
   entschieden werden; ein Abbruch des Tasks ist in keinem der beiden Fälle
   vorgesehen.
3. **Der Beweis-Lauf hängt am Netz und an GitHub.** Er holt drei echte PNGs von
   `raw.githubusercontent.com`. Fällt ein Abruf aus, ist der **Lauf ungültig**,
   nicht der Fix widerlegt — und weil ein fehlgeschlagener Abruf **keine**
   Logzeile hinterlässt (`remote_image/mod.rs:85-92` faltet den Fehler stumm zu
   `FetchFailed`), ist die Vorflug-Prüfung mit `curl` der einzige Weg, beides
   auseinanderzuhalten. Sie ist deshalb Pflicht, nicht Kür. Wiederholbar ist der
   Lauf damit nur, solange das Repo öffentlich bleibt und die drei Dateipfade auf
   `dev` existieren.
