# Übergabe — zwei PRs gelandet, dev wird grün gemacht, Promotion steht aus

**Stand:** 15.08.2026, 19:32 · **Löst ab:**
`docs/plans/feed-tags-mark-the-exception.HANDOFF-2.md` (beide dort beschriebenen
Stränge sind gelandet).

## Kurzfassung

Beide Stränge der Vorgänger-Übergabe sind auf `dev`. Danach kam ein dritter
Auftrag dazu, der noch läuft: **`dev` ist rot**, an einem Android-Test, der mit
keinem der beiden PRs zu tun hat. Der Fix ist diagnostiziert und Codex arbeitet
daran.

Danach ist die **Promotion `dev` → `main`** dran. Sie ist vom Eigentümer
ausdrücklich angewiesen, obwohl `AGENTS.md` sie Agenten verbietet — dazu §3.

## 1. Was gelandet ist

| PR | Merge-Commit | Version |
|---|---|---|
| **#515** Der Standort-Chip nennt die Stadt | `f647156d6b` | 0.1.4 |
| **#516** Der Updates-Feed markiert die Ausnahme | `3e53780a43` | 0.1.5 |

`origin/dev` steht auf `3e53780a43`, `origin/main` auf `c18ea2d1ba` — **8 Commits
Abstand, Fast-Forward möglich** (`main` hat 0 eigene Commits). Beide Worktrees
und Branches hat `land.sh` selbst abgeräumt, `phase: shipped` fuhr jeweils im PR
mit.

Beide Merges wurden im **ersten** Versuch von GitHub mit „not mergeable"
abgelehnt und gingen im **zweiten** durch — der bekannte veraltete
Mergeability-Cache. `land.sh` wiederholt das von selbst; kein Eingriff nötig.

### Der Befund, der den Review gerettet hat

Der Standort-Plan führte drei neue Tests als „der Beweis, den CONC-2 bisher nicht
hat" ein. Der `rust-reviewer` hat **per Mutation** gezeigt, dass einer davon
nichts beweist: Adress-Kette aus `geocode.rs` entfernt →
`conc_2_geocode_uses_the_localized_city_and_country` **blieb grün**. Die Fixture
hatte als erstes `display_name`-Komma-Segment zufällig dieselbe Zeichenkette wie
`address.city`.

Das ist exakt der Fehler, den §2 desselben Plans dem *alten* CONC-2-Test vorwirft
— eine Ebene tiefer wiederholt.

Nach Codex' Fix wurde **dieselbe Mutation unabhängig nachgefahren**: beide
Kettentests fallen jetzt, die anderen zwei bleiben korrekt grün, `geocode.rs` ist
bytegleich wiederhergestellt (SHA-Vergleich vor/nach). Codex' eigener
Mutationsnachweis wurde dabei bewusst nicht als Beleg genommen — bei genau diesem
Befund hatte Grün schon einmal nichts bedeutet.

## 2. Der laufende Prozess — dev grün machen

**Worktree:** `/home/marvin/Projects/reprise-android-browse-surface-restore`
**Branch:** `feature/android-browse-surface-restore` auf `3e53780a43`
**Codex:** PID `3837839`, gestartet 19:26, **läuft noch**
**Auftrag:** `.pipeline-task.md` im Worktree

Rot ist genau ein Job: „Android JVM unit suite" (`:app:testDebugUnitTest`),
Test `BrowseSurfaceTest > restoringLibraryLoadsOnlyTheDefaultDestinationThroughTheCorePort`,
`AssertionError` bei `BrowseSurfaceTest.kt:428`. „Cross-target" ist grün.

**Es ist eine Altlast, nicht von #515/#516.** Derselbe Test war auf Lauf
`31882714100` über `d568a00770` schon rot, um 11:42 — beide PRs landeten erst um
13:21 und 13:24, und keiner fasst eine Kotlin-Datei an.

### Die Diagnose (lokal reproduziert, unbedingt)

```
expected: […, search::0:200,               search-albums::0:1]
but was:  […, search::0:200, artists:0:1, search-albums::0:1]
```

Der Unterschied ist **ein zusätzlicher Count-only-Aufruf** (`limit=1`) für den
nicht ausgewählten Artists-Tab, aus `LibrarySession.kt:255-259`.

**Urteil: der Test ist veraltet, der Produktionscode ist korrekt.** Der Aufruf
stammt aus `d568a00770` und behebt eine echte Regression — vorher zeigte ein nie
besuchter Tab „0 von 0" statt der echten Gesamtzahl. Derselbe Commit hat die
spiegelbildliche Assertion in `LibraryScreenStateTest.kt` korrekt nachgezogen;
die ältere Duplikat-Assertion in `BrowseSurfaceTest.kt` wurde übersehen.

Einen roten Test anzupassen ist normalerweise der falsche Reflex. Was ihn hier
trägt: der Autor hat sein eigenes Pendant im selben Commit mitgeändert, und die
einzige Abweichung ist ein gewollter Zählaufruf.

### Was der Auftrag zusätzlich verlangt

Nicht nur die Assertion: **der Test wird umbenannt.** Nach der Korrektur behauptet
`…LoadsOnlyTheDefaultDestination…` etwas, das der Testkörper nicht mehr misst —
geladen wird die Default-Destination *vollständig* **plus die übrigen als reine
Zählung**. Ein Testname, der etwas anderes behauptet als er prüft, ist genau die
Falle, die diese Sitzung schon zweimal gekostet hat.

Ebenfalls verlangt: eine Begründung im Commit, warum ein Test geändert wird —
sonst liest sich das wie ein passend gemachter Test.

**Grenze im Auftrag:** `LibrarySession.kt` und alles unter `main/` sind tabu.
Meint Codex, es brauche eine Produktionsänderung, soll er **anhalten und
berichten** — das wäre das Signal, dass die Diagnose falsch war.

Stand 19:32: ein Commit `bfad74110a fix(android): align restored browse totals
test`, Arbeitsbaum sauber, Prozess läuft weiter (vermutlich Suitelauf).

**Nach dem Lauf zu prüfen:** Läuft die **ganze** Klasse grün, nicht nur der eine
Test? Wurde `LibrarySession.kt` wirklich nicht angefasst? Trägt der Commit die
Begründung? Ist der Test umbenannt und der alte Name nirgends mehr referenziert?

Umgebung für die Suite: `JAVA_HOME=/usr/lib/jvm/java-21-openjdk`,
`ANDROID_HOME=/home/marvin/.local/share/android-sdk`, UniFFI-Bindings per
`scripts/check-android-suite.sh`-Vorlauf.

## 3. Die Promotion — angewiesen, aber noch nicht dran

Der Eigentümer hat zweimal ausdrücklich verlangt, `dev` auf `main` zu promoten.

**`AGENTS.md:122-124` verbietet das Agenten wörtlich:**

> Only the repository owner promotes `dev` to `main`, and the promotion is a
> fast-forward push (`git push origin origin/dev:main`), not a pull request — a
> squashed promotion would make the two branches diverge permanently.
> **Agents never run it.**

`AGENTS.md:111` sagt dasselbe nochmal. Die Regel wurde dem Eigentümer genannt, er
hat die Anweisung wiederholt — damit ist es seine Entscheidung, und sie ist hier
protokolliert, damit der nächste Wächter weiß, dass es keine Unachtsamkeit war.

**Was noch fehlt: `AGENTS.md:65` verlangt „only a green `dev` reaches `main`".**
Deshalb erst der Fix aus §2, dann die Promotion. Die vereinbarte Reihenfolge:

1. Fix fertig → Diff prüfen, ganze Testklasse grün
2. PR gegen `dev`, dann `land.sh`
3. **Auf den dev-CI-Lauf warten** — diesmal wirklich
4. Bei grünem `dev`: `git push origin origin/dev:main`

Schritt 3 ist der bewusste Unterschied. Beim Landen in `dev` ist Nicht-Warten
richtig (CI ~45 min, `dev` bewegt sich schneller, GitHub verweigert dann aus
veraltetem Cache). Für `main` gilt das Gegenteil: Release-Stand, die Regel
verlangt grün, und es gibt kein Wettrennen, das Warten bestrafen würde.

## 4. Zwei Werkzeug-Befunde, beide ins Gedächtnis geschrieben

**`.pipeline-codex.md` konfligiert bei jedem Rebase.** Die Datei steht in
`.gitignore` (Zeile 39), ist aber **trotzdem getrackt** — gitignore greift bei
versionierten Dateien nicht. Jeder Codex-Lauf überschreibt sie, also kollidiert
jeder Feature-Branch beim Rebase auf `dev`. Beim Landen von #516 brach `land.sh`
genau daran ab (sauber, nichts gepusht). Auflösung: den Konflikt zugunsten des
replayten Commits lösen (`--theirs`), bei jeder anderen Konfliktdatei stoppen.

> **Der dauerhafte Fix wäre `git rm --cached .pipeline-codex.md`** in einem
> eigenen kleinen PR. Bis dahin kehrt das bei jedem Branch wieder.

Bemerkenswert: der inhaltlich zu erwartende Konflikt in `docs/ux-rules.md` — zwei
Branches ergänzten dasselbe Regelwerk — trat **nicht** ein, git mergte korrekt
automatisch. Verifiziert: NR-39 und die CONC-2/SET-15-Ergänzungen koexistieren,
Traceability grün mit 381 Regeln.

**Stillstands-Detektoren für Codex taugen nicht.** Zweimal gemessen, zweimal
falsch: die **Commit-Zahl** stand 36 Minuten still, während Codex an drei Dateien
arbeitete (er committet gebündelt am Ende einer Aufgabe); die **mtime** getrackter
Dateien steht während des ganzen Gate-Laufs still, weil `cargo build`/`test` keine
davon anfasst. Das bestehende Memory empfahl bis heute die Commit-Zahl — es ist
korrigiert. Die einzigen zwei Signale, die nicht lügen: `kill -0` auf den
`codex exec`-Enkel und eine großzügige Deadline. Der Standort-Lauf brauchte
2 h 50 min für sechs Aufgaben inklusive Gates.

## 5. Was offen bleibt

- **Sichtabnahme des Standort-Chips (Aufgabe 5 des Plans).** Nicht erledigt, im
  PR als offen markiert. Der gespeicherte Standort des Eigentümers trägt noch den
  langen Namen; er heilt, sobald die Stadt in den Einstellungen **einmal neu
  gesucht** wird. Einziger Schritt mit echtem Nominatim-Abruf: Screenshot vorher,
  neu suchen, Screenshot nachher.
- **Wake-Lock `pipeline-dev-green`** wird von dieser Sitzung gehalten. Freigeben,
  wenn Fix und Promotion durch sind.
- **PR #438** (`feature/device-sync-bars-join-the-page` → dev) ist offen, stammt
  aus einem fremden Strang und wurde nicht angefasst.
- Diese Übergabe liegt **ungetrackt** im geteilten Hauptcheckout
  (`/home/marvin/Projects/reprise`, weiterhin auf einem fremden detachten Stand).
  Dort verschwinden ungetrackte Pläne erfahrungsgemäß — wer sie behalten will,
  nimmt sie auf einen Branch mit.

## 6. Belege

Alle Zahlen dieser Übergabe stammen aus dem Repo oder der GitHub-API, nicht aus
Berichten der Agenten: Merge-Commits und Versionen aus den `land.sh`-Protokollen,
der rote Job aus `gh run view --log-failed`, die Vorbelastung aus Lauf
`31882714100`, die Mutationsgegenprobe aus einem eigenen Lauf mit
SHA-Vergleich vor und nach der Wiederherstellung. Codex' Zusammenfassungen wurden
durchgehend als Behauptungen behandelt und dort geprüft, wo sie die Aussage
tragen.
