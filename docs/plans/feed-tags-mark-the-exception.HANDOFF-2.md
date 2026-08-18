# Übergabe — Feed-Tags umgesetzt und geprüft, Standort-Chip geplant

**Stand:** 15.08.2026, 11:35 · **Löst ab:**
`docs/plans/feed-tags-mark-the-exception.HANDOFF.md` (Plan umgesetzt, dessen §3
ist zu einem eigenen Plan geworden).

## Kurzfassung

Zwei Stränge, beide bewegt. Der Feed-Tags-Plan ist **implementiert, reviewt und
im Refactor** — das ist der einzige Punkt, an dem gerade ein Prozess läuft. Der
Standort-Befund aus §3 der Vorgänger-Übergabe ist **gegrillt und als fertiger
Plan abgelegt**, noch ohne Code.

Ein Befund ist dabei entstanden, den niemand gesucht hat: der Prüfstand des
Feed-Tags-Plans behauptet fünf Tatsachen, statt sie zu messen (§2).

## 1. Feed-Tags — läuft gerade

**Worktree:** `/home/marvin/Projects/reprise-feed-tags-mark-the-exception`
**Branch:** `feature/feed-tags-mark-the-exception` · **Plan:** `phase: reviewed`

Fünf Commits auf der Basis `9fecc6d8f5`:

```
bb4ddcaa07  Tag the exception, not the state, in the Updates feed
bc325bca75  Prove the popover pill and the table pill are one pill
96f9ece9d3  Record R4 as closed by construction
429b5390b3  Capture the counter-proof to 41-footer-loaded
6ca6c9805d  docs: carry the plan on the branch that implements it
```

Der letzte ist **nicht** von Codex. Er hatte den Plan bewusst als „preserved
user-owned untracked plan" liegen gelassen — genau die Falle aus `#467`:
`land.sh` findet den Plan über seine `branch:`-Zeile und entfernt beim Merge den
Worktree, ein ungetrackter Plan wäre danach weg.

### Was Codex gebaut hat

Vier Töne statt drei (Grill-Beschluss), `Off sale` und `Unknown` bekommen einen
Tag, `On sale` nicht, R4 als datierter Nachtrag geschlossen, NR-39 im Regelwerk,
Sondenskript getrackt und Bilder nicht. Gates: fmt, Clippy strict/locked,
rustdoc, Workspace-Tests, UX-Traceability (380 Regeln), ShellCheck, Architektur,
Frontend-Thinness — alle grün. Display-Suite `dev` 715/716 gegen Branch 715/717;
beide Branch-Fehlschläge sind unberührte list-geometry-Tests und liefen isoliert
auf **beiden** Ständen grün (bekanntes Rudel-Flackern).

**Die eine Abweichung:** die numerische Farbstichprobe *aus dem Screenshot* für
den `Off sale`-Tag konnte nicht laufen — `cua-driver 0.19.3` reicht den
verschachtelten Pillentext ohne benutzbaren AT-SPI-Rahmen durch. Das vorab
festgelegte Kriterium ist davon **nicht** betroffen: der deterministische
GTK-Geometrie-/Farbtest ist grün. Blockiert war die weichere Auswertung obendrauf,
und das Manifest trägt sie als `status=blocked` samt `missing_verification` ein.

### Review: Produktionscode sauber, Prüfstand nicht

Drei Reviewer (Sonnet/high), jeder auf seinen Dateisatz begrenzt.
`security-reviewer` wurde weggelassen — die Änderung fasst weder Auth noch
Eingabeverarbeitung, externe APIs oder Zahlungspfade an.

- **Rust** (`css.rs`, `feed_row.rs`, `concerts_section.rs`): **Approve, null
  Befunde.** Nicht nur gelesen — `cargo check --all-targets`, `clippy -D
  warnings` und `cargo test -p reprise-gnome updates::` liefen sauber. Der
  Reviewer hat die Tonparität zusätzlich durch direkten CSS-Vergleich bestätigt
  und begründet, dass `nr_39_the_feed_tags_only_the_exception` bei jeder Mutation
  der drei Zuordnungen rot würde — also ein echter Test, keine Tautologie.
- **Regelwerk** (`ux-rules.md`, R4-Nachtrag, `progress.md`): **null Befunde.**
  Der R4-Hunk ist `@@ -1697,3 +1697,39 @@` — jede Zeile `+`, keine `-`. Der
  Anforderungstext steht wörtlich. Der genannte Test existiert unter genau dem
  Namen (`concerts_section.rs:323`).
- **Sonde** (`probe-feed-tags.sh`, `manifest.txt`): **sechs Befunde**, siehe §2.

### Der Refactor, der gerade läuft

Codex, PID 908506, gestartet 11:26, im selben Worktree. Auftrag: **F1, F2+F3,
F4** — die vom Eigentümer angenommenen Befunde. F5 und F6 stehen ausdrücklich als
„nicht anfassen" im Auftrag, sonst repariert Codex sie ungefragt mit.

Der Auftrag liegt in
`/tmp/claude-1000/-home-marvin-Projects-reprise/fc4f0aa5-efcd-4372-a76b-914b2adb77ed/scratchpad/feed-tags-findings.md`
und als `.pipeline-task.md` im Worktree. Zum Zeitpunkt dieser Übergabe ist
`probe-feed-tags.sh` bereits verändert, aber noch nicht committet.

**Zwei Dinge sind nach dem Lauf zu prüfen** — es sind die wahrscheinlichsten
Wege, wie dieser Refactor danebengeht:

1. Führt das Manifest wirklich **drei** Zustände, oder schreibt ein blockierter
   Lauf doch wieder ein blankes `present`? Das war der ganze Punkt von F1.
2. Blockiert der `git status --porcelain`-Wächter aus F2 den Prüfstand selbst?
   Der Worktree ist während der Arbeit naturgemäß schmutzig. Ein Fix, der das
   Werkzeug unbenutzbar macht, ist schlimmer als der Befund.

Danach: `phase: refactored` setzen, Diff vorlegen, dann landen.

## 2. Der Befund, den niemand gesucht hat

`probe-feed-tags.sh:421-425` und `:448-452` enthalten diese fünf Zeilen als
literale Konstanten, byte-identisch in **beiden** Schreibpfaden, keine liest eine
Variable:

```
printf 'current_popover_unknown_tag=present\n'
printf 'current_popover_on_sale_tag=absent\n'
printf 'current_table_all_three_ticket_values=present\n'
printf 'control_popover_unknown_tag=absent\n'
printf 'control_popover_on_sale_tag=absent\n'
```

`private_cleanup` (`:463-473`) ruft `write_blocked_manifest` bei **jedem**
Nicht-Null-Exit. Schlägt also ausgerechnet
`assert_snapshot_contains "$popover_snapshot" "Unknown"` (`:309`) fehl — eine
echte Regression, die den Tag entfernt —, bricht `set -e` dort ab, und das
Manifest schreibt anschließend `current_popover_unknown_tag=present`. Das
Dokument behauptet die Tatsache, deren Prüfung soeben gescheitert ist.

Für den committeten Lauf stimmen die fünf Werte, weil die Zusicherungen
durchliefen, bevor `measure_pills` blockierte. **Das ist Glück, kein Entwurf**,
und ein Leser kann beides nicht unterscheiden.

Deshalb verlangt der Refactor-Auftrag **drei** Zustände, nicht die zwei, die der
Reviewer vorschlug: geprüft-und-zutreffend, geprüft-und-nicht-zutreffend,
nicht-erreicht. Ohne die dritte Stufe bleibt die Lücke.

**Angenommen:** F1 (oben), F2 (`current_commit` kann einen anderen Baum benennen
als den gebauten — der Kontrollarm kommt aus `git archive`, der Fix-Arm aus dem
ungeprüften Arbeitsbaum), F3 (`REPRISE_FEED_TAGS_CONTROL_BINARY` akzeptiert jede
ausführbare Datei, das Manifest druckt trotzdem `control_commit=b6be7cdc61`;
außerdem Kurz-SHA gegen Voll-SHA), F4 (verwaister `reprise`-Prozess: `app_pid`
ist `local` und erreicht die Exit-Falle nie).

**Abgelehnt und ausdrücklich nicht zu „reparieren":** F5 (fest verdrahtete
AT-SPI-Pfade — identisch zu `scripts/cua-common/session.sh`, `cua-e2e/run.sh`
und `filter_clear_matrix.sh`; eine Änderung hier allein schüfe Inkonsistenz),
F6 (Prüfsummen der Beweisbilder).

## 3. Standort-Chip — geplant, kein Code

**`docs/plans/location-chip-names-the-city.md`** · `phase: planned` ·
Branch-Name steht drin: `feature/location-chip-names-the-city` ·
317 Zeilen, sieben Aufgaben, neun Fallen.

```
/code docs/plans/location-chip-names-the-city.md
```

### Was der Grill entschieden hat

| # | Beschluss |
|---|---|
| B1 | `AppLocation.name` **wird** die Stadt; der Chip zeigt sie allein |
| B2 | Kette `city → town → village → municipality`, danach erstes Komma-Segment. `suburb` kommt **nicht** vor — „Kreuzberg" ergibt `Berlin · 500 km` |
| B3 | Oberflächensprache (`active_gui_language()`), **nicht** Systemgebietsschema, `_` → `-` |
| B4 | Drei String-Tests (kein Netz) plus Sichtabnahme mit echtem Abruf |
| B5 | CONC-2 wird **ergänzt**, nicht ersetzt und nicht verdoppelt |
| B6 | `GeocodedLocation.display_name` wird durch `city` **ersetzt**, nicht ergänzt |
| B7 | Die Kürzung lebt in `parse_geocode` (reprise-core), nicht in der GNOME-Schicht |
| B8 | **Die Einstellungen zeigen Stadt und Land** — nachgetragen auf Ansage des Eigentümers, setzt den „alle Flächen zeigen die Stadt"-Teil von B1 außer Kraft |

### Drei Sachen, die beim Planen erst gefunden wurden

- **Die Migration aus der Vorgänger-Übergabe fällt weg.** Deren §3 Punkt 3
  verlangt „eine Migration oder ein erneutes Geocodieren beim Lesen".
  `AGENTS.md:269-271` sagt: Reprise ist nicht ausgeliefert, es gibt keine
  Installationen, Migrationen sind *kein* Entwurfskriterium. Kein `SCHEMA_V19`.
  Der einzige Altbestand ist die DB des Eigentümers; sie heilt über die
  Sichtabnahme (Aufgabe 5).
- **Der Ländername kostet nichts.** `address.country` steht in derselben Antwort
  neben dem schon gelesenen `country_code` und trägt den Anzeigenamen. Der
  Schwanz des gemeldeten Fehlerstrings — `Schweiz/Suisse/Svizzera/Svizra` — *ist*
  dieses Feld, unlokalisiert. B3 repariert ihn also mit.
- **SET-15 wird sonst falsch.** Die Regel (`ux-rules.md:1210`) besitzt die
  Standort-Einstellungsseite und zählt **abschließend** auf, was das Leeren
  entfernt: „latitude, longitude, name, and country code". Ein neuer
  `location.country`-Schlüssel ohne diese Ergänzung macht die Regel unwahr.
  Steht als Aufgabe 4 im Plan, zusammen mit CONC-2.

### Warum CONC-2 grün war, während der Chip falsch aussah

`conc_2_location_chip_names_the_city_and_off_state_names_the_radius`
(`concerts_filter_bar_tests.rs:35`) füttert die Formatierung mit `Some("Zürich")`
— einem bereits kurzen Namen. Er misst den **Formatierer**, nicht die **Kette**,
und bleibt grün, egal was in `location.name` steht. Er bleibt unverändert
(Falle F-6); die neuen Tests treten daneben, nicht an seine Stelle.

## 4. Der Stand ringsum

- **Die Promotion ist gelaufen.** Bei Sitzungsbeginn lag `dev` 42 Commits vor
  `main`; jetzt ist `origin/main` auf `0ea3a4e73e` und `dev` nur noch **einen**
  Commit voraus. Die Sitzung mit dem Wake-Lock `showcase-relaunch` hatte sie sich
  vorgenommen — hier wurde nicht hineingegriffen.
- **`origin/dev` steht auf `bc1f117aef`.** Der Feed-Tags-Branch sitzt auf
  `9fecc6d8f5`; der Standort-Plan ist gegen denselben Stand geprüft. Vor dem
  `/code` des Standort-Plans neu fetchen.
- Der Hauptcheckout `/home/marvin/Projects/reprise` steht weiter auf einem
  **fremden, detachten Stand** (`be5f014d3b`). Wer dort misst, misst den falschen
  Baum. Alle Befunde dieser Sitzung wurden per `git show origin/dev:<pfad>`
  gelesen.

## 5. Was aufzuräumen ist

- **Wake-Lock `feed-tags-and-location`** wird von dieser Sitzung gehalten.
  Freigeben, wenn niemand mehr an den beiden Strängen arbeitet:
  `wake-lock release feed-tags-and-location`. Der Lock `ucr-open-questions` der
  Vorgänger-Sitzung wurde bereits freigegeben.
- **`/home/marvin/Projects/reprise-ucr-acceptance`** — Abnahme-Worktree mit 4 GB
  warmem Debug-Build. Neubau kostet gut anderthalb Stunden, deshalb nicht
  ungefragt gelöscht. Unverändert aus der Vorgänger-Übergabe.
- **`/home/marvin/Projects/reprise-ucr-release`** — nur für eine einzelne
  Release-Gate-Messung angelegt, kann weg.
- Der Feed-Tags-Worktree wird von `land.sh` selbst entfernt; nicht vorher
  anfassen, solange der Refactor läuft.

## 6. Eine Lehre für den nächsten Wächter

Der erste Stillstands-Wächter dieser Sitzung hat die **Logdatei** von
`codex-run.sh` beobachtet und nach 25 Minuten „hängt" gemeldet — während Codex
zwei Commits gemacht hatte. `codex-run.sh` schreibt erst am Ende in seine
Ausgabe. Das brauchbare Fortschrittssignal ist die **Commit-Zahl im Worktree**,
das brauchbare Lebenssignal `kill -0` auf den `codex exec`-Enkel. Beides steht
im zweiten und dritten Wächter dieser Sitzung.

## 7. Belege

Alles Dauerhafte steht in den beiden Plandateien und im Manifest des
Prüfstands. Die Reviewer-Befunde liegen strukturiert im Refactor-Auftrag
(`feed-tags-findings.md`, Pfad in §1). Die Tatsachenbehauptungen des
Standort-Plans sind gegen `origin/dev` @ `9fecc6d8f5` geprüft; die
Nominatim-Aussagen (`accept-language` als Query-Parameter, `address.country` als
Anzeigename) gegen die offizielle Doku, nicht aus dem Gedächtnis.
