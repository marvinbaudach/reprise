# Handover — Android-Künstlerfotos, was nach der Landung offen ist

**Stand:** 14.08.2026, 20:15 Uhr.
**Mutterplan:** `docs/plans/android-artist-photos.md` (`merge_order: core,ui`).
**Vorgänger-Handover:** `docs/plans/android-artist-photos.HANDOFF.md` — liegt
getrackt auf `dev` und beschreibt die Landung selbst.

## Der Code ist durch

| Strang | Zustand |
| --- | --- |
| `core` (PR #482) | gelandet als `8b87ae8ada` |
| `ui` (PR #486) | gelandet als **`0b7cf509d9`**, 15 Commits |

Beide Stränge des Mutterplans sind damit auf `dev`. Worktrees und Branches sind
weg, `phase: shipped` steht in beiden Strangdateien.

Vor dem Merge unabhängig nachgemessen — nicht aus Codex' Bericht übernommen, und
zwar **nach** dem Rebase auf den aktuellen `dev`:

```
suites=70 tests=356 failures=0 errors=0 skips=0 verdict=fresh
```

`reprise-android-ffi` und `reprise-core` 0×`test result: FAILED`, `cargo fmt`
sauber, `cargo clippy --all-targets --all-features -- -D warnings` sauber.
Nach dem Merge auf `dev`: **`Android JVM unit suite: success`** (Lauf
`31820860877`).

---

## Das eine, was noch offen ist: Aufgabe 10

**Die Sichtprüfung am Gerät.** Sie steht als `Status: OFFEN` in
`docs/plans/android-artist-photos-ui.md` und ist durch das Landen **nicht**
erledigt — `phase: shipped` bezieht sich dort ausdrücklich nur auf den Code.
Sie wurde aus dem headless Codex-Lauf bewusst ausgeschlossen, weil ein solcher
Lauf ein angeschlossenes Gerät kapert, wenn man ihn lässt.

Zu belegen mit Aufnahmen, nicht mit einem Bericht:

1. **Zeile:** die Interpretenliste mit einem geladenen Porträt, einem
   Album-Cover-Rückfall und einer Fallback-Farbe **in derselben Aufnahme**. Der
   Avatar ist rund und liegt auf der Textgrundlinie.
2. **Detailkopf:** zwei Aufnahmen derselben Seite — eine direkt nach dem Öffnen
   (Album-Cover), eine nach dem Eintauschen des Porträts. Die Position der
   Sektion „Albums" muss in beiden identisch sein; das ist der Nachweis, dass
   nichts springt. Auf beiden steht der Interpretenname genau einmal, in der
   Zurück-Zeile.
3. **Scrollen:** eine Bildschirmaufnahme des Durchlaufs, plus Framezeiten aus
   `adb shell dumpsys gfxinfo org.reprise framestats`.

### Drei Fallen, die diesen Lauf sonst kosten

- **Ohne Vorbereitung ist Aufnahme 1 unmöglich.** Die Liste holt nie selbst
  (`allowFetch = false`), ein Porträt landet ausschließlich dadurch im
  Zwischenspeicher, dass jemand den Interpreten geöffnet hat. Auf einer frischen
  Installation zeigt die Liste deshalb *nur* Album-Cover und Fallback-Farben.
  Also: erst drei bis vier Interpreten öffnen und zurück, dann fotografieren.
- **Framezeiten nur aus einem Release-Build auf echter Hardware.** Debug-Build
  und Emulator können die Frage nicht beantworten, das ist gemessen. Der
  Emulator löst 60 Hz ohnehin nicht auf.
- **Der Schalter startet auf Aus.** Ohne „Settings → Online sources →
  Download artist photos" kommt kein einziges Porträt — dann fotografierst du
  korrektes Verhalten, aber nicht das, was Aufgabe 10 verlangt.

### Womit du rechnen solltest

Beim Review wurde ein Punkt bewusst *nicht* geändert, der genau hier sichtbar
werden könnte: der Porträt-Fetch teilt sich die Lane `fullSizeWorker` mit dem
Now-Playing-Cover. Er ist durch 15 s `HTTP_TIMEOUT` gedeckelt und läuft nie auf
dem Hauptthread — er kann also keinen Frame reißen, sondern nur Bilder verspätet
liefern. Wenn Porträts beim Scrollen sichtbar nachhinken oder ein
Now-Playing-Cover ungewöhnlich spät kommt, ist das der erste Verdächtige. Wenn
die Liste selbst stockt, ist es nicht dieser Pfad.

---

## Zweites offenes Ding, das mir nicht gehört

**`dev` ist rot, und zwar seit vor dieser Landung.** Der Lauf auf #484
(`a6a0d11604`) scheitert an genau einem Test:

```
ui::preferences::preference_concerts::tests::concerts_preferences_expose_only_bandsintown_and_link_similar
assertion failed: preferences.inner.rows[0].is::<adw::PasswordEntryRow>()
```

Das ist die Nachwirkung von **#483** („The location is an app setting, not a
Concerts plugin option"): der Test zählt noch die alte Zeilenreihenfolge. Am
14.08. um 18:50 reparierte das kein offener PR. Der lokale Worktree
`reprise-dev-gate-repair` auf `fix/dev-gate-repair` existiert, hat aber keinen
offenen PR — vor einer eigenen Reparatur nachsehen, ob dort schon jemand dran
ist.

**Der `Quality gate` für `0b7cf509d9` hat kein eigenes Urteil**, weil #487 zwei
Minuten nach dem Merge gelandet ist und die Concurrency-Gruppe den Lauf getötet
hat (`cancelled`, nicht rot). Das Urteil fällt im Lauf **`31824510148`** auf
`f24366b269`, der meinen Commit enthält. Wenn der rot ist, zuerst gegen den
obigen Test halten, bevor jemand die Künstlerfotos verdächtigt — sie fassen
`reprise-gnome` nicht an.

---

## Zwei Dinge, die dieser Durchlauf gelernt hat

- **`land.sh` nennt den falschen CI-Lauf.** Es druckt `gh run list --branch dev
  --limit 1`, und das ist auf diesem Repo meist **Cross-target**, ein
  cargo-Cross-Compile-Check, der über Kotlin nichts aussagt. Der Gate heißt `CI`
  (`Android JVM unit suite` + `Quality gate`) und ist ein **eigener Lauf auf
  derselben SHA**. Mit `--limit 8 --json databaseId,workflowName,conclusion,headSha`
  suchen.
- **Ein Codex-Lauf räumt fremde Commits weg.** Er hat hier den Commit fallen
  lassen, der das Vorgänger-Handover angelegt hatte (`97be7d2880`) — samt Datei
  auf der Platte. Zurückgeholt per `git show <sha>:<pfad>`. Wer Codex auf einen
  Branch lässt, in dem fremde Commits liegen, hält danach `git log` gegen den
  Stand davor; ein Commit weniger fällt sonst nicht auf.

---

## Umgebung (gemessen, nicht verhandelbar)

- `JAVA_HOME=/usr/lib/jvm/java-21-openjdk` vor **jedem** Gradle-Aufruf. Der
  Systemstandard ist JDK 26 und killt Robolectric.
- `ANDROID_HOME=/home/marvin/.local/share/android-sdk`. Es ist **nicht** in der
  Shell gesetzt, und `scripts/check-android-suite.sh` bricht ohne es nach einer
  Zeile ab — das sieht wie ein roter Test aus, ist aber gar kein Lauf.
  `/opt/android-sdk` enthält nur das NDK, kein `platforms/`.
- `BUILD SUCCESSFUL` ist kein Beweis. Das Urteil ist die Frische der XML unter
  `android/app/build/test-results/testDebugUnitTest/`, die das Skript gegen den
  Startzeitpunkt prüft (`verdict=fresh` vs. `stale`).
- `TMPDIR=/tmp` für `cargo test -p reprise-android-ffi` (hängt an
  `readdir`-Reihenfolge).
- Cargo-Läufe per `grep -c '^test result: FAILED' <log>` beurteilen, nie an der
  letzten Zeile.
