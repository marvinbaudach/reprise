# Übergabe — Android: Queue-Reiter, Kontextmenüs, Suche

Stand: 2026-08-11, 09:25. Vorgänger-Session hatte 300k Kontext, daher diese Übergabe.

## Der Auftrag

Vier Wünsche des Nutzers an die Android-App, wörtlich:

1. Beim Öffnen der Suche soll der Fokus ins Feld springen, damit man sofort tippt.
2. Das Suchfeld ließ sich nicht wieder ausblenden.
3. Eine Queue als **fünfter Reiter**.
4. Titel und Alben sollen auf langes Drücken ein Kontextmenü bekommen: löschen,
   als nächstes einreihen, hinten anhängen.

Später ergänzt: **auch in der Player-Ansicht** soll gelöscht werden können.

## Wo alles liegt

| | |
|---|---|
| Worktree | `/home/marvin/Projects/reprise-mobile-queue-tab-and-context-menus` |
| Branch | `feature/mobile-queue-tab-and-context-menus`, Basis `75a24b35a9` (`origin/dev`) |
| Spec | `docs/superpowers/specs/2026-08-10-android-queue-search-context-menu-design.md` |
| Plan | `docs/superpowers/plans/2026-08-10-mobile-queue-tab-and-context-menus.md` |
| Review-Befunde | `.pipeline-findings.md` **im Worktree** (nicht im Hauptcheckout) |
| Pipeline-Phase | `reviewed` — muss nach Abschluss des Refactors auf `refactored` |

Der Hauptcheckout `dev-local` hängt weit hinter `origin/dev`. Alles inhaltlich
Relevante gegen `origin/dev` prüfen, nie gegen den lokalen Stand.

## Beschlossene Entscheidungen (aus der Grill-Runde mit dem Nutzer)

- „Löschen" heißt **wirklich vom Gerät löschen**, nicht nur aus der Bibliothek.
  SAF kennt keinen Papierkorb → immer Bestätigungsdialog, Abbruch löscht nichts.
- Der Queue-Reiter ist der **einzige** Ort der Queue; der Umschalter im
  Now-Playing-Sheet ist entfallen.
- Kontextmenü auf Titeln, Favoriten, Alben **und** Queue-Zeilen; Künstler nicht.
- Einreihen startet **nie** die Wiedergabe, auch nicht bei leerer Queue.
- Löschen des laufenden Titels **springt zum nächsten**, hält nicht an.
- Now-Playing-Überlaufmenü bekommt genau einen Eintrag: „Vom Gerät löschen…".
- Queue ist **kein** speicherbares Startziel (sonst startet man in eine leere
  Warteschlange).
- Die Konvergenz von Android auf `up_next::UpNext` ist ausdrücklich **nicht**
  Teil dieser Arbeit.

## Was fertig ist

Sieben Pakete von Codex, alle committet, 37 Dateien, +1983/−153:

    3206aa0db6  Kern: Queue::enqueue (belebt erschöpfte Queues wieder)
    69ec24be83  FFI: Einreihen + die eine gemeinsame Fensteranfang-Rechnung
    ca92a22755  Album-IDs, ungefenstert
    07e15daef7  Löschen: trash_boundary.rs, Queue-Abgleich, Weiterspringen
    5884fc37e0  Kotlin-Verkabelung über den Playback-Service
    6e65c963a1  Gemeinsames Kontextmenü + ID-only-Abspielroute
    96389d57f1  Queue als fünfter Reiter, Suchfokus, drei Schließwege

**Selbst nachgemessen** (nicht Codex geglaubt): Kern 2.387 bestanden / 0 rot /
2 ignoriert, FFI 100/100, Gradle 52 frische Suiten von 52 gesamt, 246 Tests,
0 failures, 0 errors. Die Gradle-Zahlen kamen aus den `TEST-*.xml` nach
vorherigem Löschen des Ergebnisordners.

Aus der Refactor-Phase (Opus) zusätzlich committet — die gesamte Rust-Seite der
Review-Befunde:

    f7a6611762  fix(core): keep a blank album out of the canonical album id list
    b82556036e  fix(android-ffi): report an id whose row is already gone
    14b8530553  refactor(android-ffi): restore the explicit re-export list, record lock order
    bf5f64a920  test(android-ffi): cover reviving an exhausted queue and an unresolvable tapped id

## Was offen ist

**Der Refactor lief bei Übergabe noch** — ein Opus-Subagent der alten Session.
Subagenten überleben ein `/clear` nicht zuverlässig. **Erster Schritt: prüfen,
ob er fertig wurde**, sonst neu ansetzen mit `.pipeline-findings.md` als Auftrag.

    git -C <worktree> log --oneline 96389d57f1..HEAD
    git -C <worktree> status --porcelain

Bei Übergabe noch nicht erledigt:

- **CRITICAL — Duplikat-Absturz im Queue-Reiter.** `LibraryTrackRows.kt:138`
  keyt Zeilen nach `"track-${track.uri}"`. Der Kern erlaubt Duplikate
  ausdrücklich; das neue „Als nächstes" auf einem bereits eingereihten Titel
  erzeugt zwei Zeilen mit gleichem Key →
  `IllegalArgumentException: Key … was already used` (vom Reviewer mit einem
  eigenen Robolectric-Test reproduziert). Der Key muss pro Queue-**Platz**
  eindeutig werden, nicht pro Titel. Dabei bewusst entscheiden, ob die
  Bibliothekslisten denselben Rendering-Pfad teilen — dort ist der URI-Key
  richtig. Test gehört dazu.
- **MAJOR — Rückmeldung überlagert die Zeile.** `TrackContextMenu.kt:209` (und
  `:242`) setzen `TransientMessageText` als nacktes Geschwister in die `Box` der
  Zeile; gemessen: Zeile `(0,0)-(412,72)`, Meldung `(0,0)-(15,36)`, also über
  dem Cover. Vorbild ist `FavouriteHeart.kt:35-64` mit einem `Column`-Wrapper.
  Bei Übergabe lagen dazu **30 nicht committete Zeilen** in
  `TrackContextMenu.kt` — angefangene Arbeit desselben Agenten, erst lesen.
- **MINOR — rohe rusqlite/IO-Fehlertexte** erreichen die Oberfläche
  (`TrackContextMenu.kt:292`).

## Danach

1. Phase auf `refactored` setzen
   (`~/.claude/skills/pipeline/scripts/status.sh set <plan> phase refactored`).
2. Diff dem Nutzer vorlegen — das ist sein Prüfpunkt.
3. **Gerätesichtung durch den Nutzer**, zwei Punkte, die kein Test klärt:
   - Das rätselhaft fehlende ✕ im Suchfeld. Im Code ist es vorhanden
     (`LibraryFrame.kt:85-90` tauscht die Lupe gegen `close`), die Ligatur
     `close` steckt nachweislich im Font. Der Nutzer sah es auf einem aktuellen
     Build trotzdem nicht. Ursache unbekannt; das Design hat deshalb drei
     unabhängige Schließwege eingebaut, statt zu raten.
   - Ob fünf Einträge in der `NavigationBar` auf seinem Gerät tragen — fünf ist
     das Maximum von Material 3.

## Fallen, die hier Zeit gekostet haben

- **JDK 21 zwingend** für die Android-Suite (`JAVA_HOME=/usr/lib/jvm/java-21-openjdk`).
  Systemstandard ist 26, darunter stirbt Robolectric im Teardown und es sieht
  wie ein eigener Fehler aus.
- **Gradle meldet grün, ohne zu laufen.** Vor jedem Lauf
  `android/app/build/test-results/testDebugUnitTest` löschen, danach frische
  `TEST-*.xml` zählen und `tests`/`failures`/`errors` aufsummieren. Auch die
  **Suitenzahl** prüfen, nicht nur die Testzahl.
- `ANDROID_HOME=~/.local/share/android-sdk`; `GRADLE_USER_HOME` und
  `XDG_CACHE_HOME` in den Worktree legen. Für den Cross-Build
  `scripts/android-build.sh` benutzen — nacktes `cargo build` scheitert an
  ring/cc-rs.
- **Codex' Zusammenfassungen sind Behauptungen.** Diesmal stimmten die Zahlen
  exakt, geprüft wurde trotzdem selbst — hier lagen sie schon mehrfach daneben.
- **Enge `Files:`-Listen im Plan halten Codex an.** Zwei Neustarts kostete das:
  `AndroidPlaybackSession` liegt privat in `ReprisePlaybackService`, und
  `playTracks` verlangt IDs *und* URIs. Vor einer Dateiliste die
  Besitzverhältnisse greppen und die Liste als Startpunkt deklarieren.
- Ein Wake-Lock `android-queue-refactor` war bei Übergabe gesetzt —
  `wake-lock release android-queue-refactor`, wenn nichts mehr läuft.
