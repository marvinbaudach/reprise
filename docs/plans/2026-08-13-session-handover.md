# Übergabe 13.08.2026 — Artwork-Schalter und ein Datenverlust

> Geschrieben am 13.08.2026, 21:30. Zwei unabhängige Vorgänge in einer Sitzung:
> die Review- und Refactor-Phase für `artwork-toggle-starts-the-scan`, und ein
> Verlust von 32 Planungsdokumenten im geteilten Hauptcheckout, der mitten
> hinein platzte. Beide sind unten getrennt beschrieben. **Der Datenverlust ist
> der dringendere Teil** — seine Ursache ist nicht behoben.

## Teil 1 — Der Artwork-Strang

### Wo er steht

| | |
|---|---|
| Plan | `docs/plans/artwork-toggle-starts-the-scan.md` |
| Befunde | `docs/plans/artwork-toggle-starts-the-scan.FINDINGS.md` |
| Worktree | `/home/marvin/Projects/reprise-artwork-toggle-starts-the-scan` |
| Branch | `feature/artwork-toggle-starts-the-scan` |
| Phase | `refactored` |
| Merge-Basis | `4912275130` (= `origin/dev` @ #455) |
| PR | **existiert nicht** |

Acht Commits: vier aus der Implementierung (`0ee37dca9a`, `48561f9487`,
`d0f0142d6d`, `95974b56b8`), vier aus der Refactor-Phase (`3b987f35e4`,
`996ec82353`, `afbd486f2e`, `e9f0d6e85e`). Die vier Refactor-Commits umfassen
30 Dateien, +858/−130.

### Was passiert ist

Der Zweig macht, was sein Titel sagt: Artwork einschalten startet den
Cover-Durchlauf sofort, statt auf einen Anlass zu warten, der nie kommt. Die
Check-Phase fuhr vier Reviewer (drei `rust-reviewer` nach Zuständigkeit, ein
Spec-Abgleich) und fand 13 Befunde, darunter einen kritischen: eine kaputte
MusicBrainz-Antwort brannte ein Album weiterhin permanent als „nichts zu holen"
fest — dieselbe Bibliothekskorruption, die der Plan eigentlich abstellen sollte,
nur über den Sucheingang statt den Abrufeingang.

Alle 13 wurden vom Nutzer angenommen, Codex hat sie abgearbeitet, und ein
Verifikationsagent hat jede einzelne Behauptung gegen den Code gehalten:
**13 von 13 bestätigt**, Testrümpfe gelesen, auf Hohlheit geprüft. Die
Abgrenzung des Plans hält vollständig — Startpfad unberührt,
`PreferencesContext::new` weiter bei 21 Argumenten, kein neuer
`NetworkMonitor`-Leser, kein Vorwärmen, kein Nachholen bei Netzrückkehr.

Eine Entscheidung des Nutzers steckt darin (Befund 10): der Live-Netz-Riegel
gilt **nicht** für Online Lyrics. Der Plan begründet die Netzvorbedingung
ausschließlich mit dem Cover-Pfad und `remember_download_unavailable`; Lyrics
kennt keinen zerstörerischen Merker, ein Fehlversuch kostet dort nichts. Als
Test festgeschrieben in
`lyr_6_the_production_module_transition_starts_lyrics_once_even_offline`.

Neu in `docs/ux-rules.md`: **NET-5** (Sofortstart beim Einschalten) und
**NET-6** (bereits gezeichnete Flächen fordern nach), beide `[active] [gtk]`.

### Was noch offen ist

1. **Der Worktree ist unsauber.** `.pipeline-codex.md` ist geändert (getrackt),
   `artwork-toggle-starts-the-scan.FINDINGS.md` liegt ungetrackt darin.
   `land.sh` verweigert bei einem unsauberen Worktree. Vor dem Landen
   entscheiden: committen oder verwerfen.
2. **Kein eigener Testlauf.** Codex behauptet Core 2413 grün, GNOME 1796 grün
   bei 662 ignorierten, volle Workspace-Suite seriell, `cargo fmt`, striktes
   Clippy, Core-Purity und UX-Traceability bestanden. Verifiziert ist der
   *Code* — dass die Reparaturen echt und die Tests nicht hohl sind. Dass die
   Suite auch *läuft*, hat niemand gemessen.
3. **Die visuelle Abnahme steht aus.** Verifikationspunkt 5 des Plans. Die
   beiden einschlägigen Tests tragen `#[ignore]`, und der CUA-Versuch scheiterte
   an einer degradierten AT-SPI-Bridge. Das war vor dem Review offen und ist es
   weiterhin.
4. **Kein PR.** Landen heißt hier: rebasen, pushen, sofort mergen — nicht auf CI
   warten (`land.sh <pr-nr>`).

### Eine Nebenwirkung, die man kennen muss

Die LYR-6-Abdeckung ist **langsamer** geworden. Vorher lief der Regeltest bei
jedem `cargo test`; jetzt ist er ein `#[ignore]`-Display-Test. Dafür prüft er
den echten Produktionspfad statt einer toten `#[cfg(test)]`-Methode — inhaltlich
ein Gewinn. Geprüft: `check-display-tests.sh` leitet im `--rule-named`-Modus die
Präfixe aus den Regel-IDs ab und matcht `^(net|lyr|…)_[0-9]+[a-z]?_`; beide
neuen Tests (`net_5_…`, `lyr_6_…`) werden also ausgewählt. Die Abdeckung hängt
damit daran, dass das Display-Gate tatsächlich läuft.

## Teil 2 — Der Datenverlust

### Was geschehen ist

Gegen 20:45 verschwanden aus `/home/marvin/Projects/reprise/docs/plans/`
**32 ungetrackte Planungsdokumente**. Der Hauptcheckout stand danach in
**detached HEAD** auf `c0df688a19` (#451) — vier Commits *hinter* `origin/dev`.
Eine fremde Sitzung hatte sich rückwärts durch die Historie gecheckt. Getrackte
Dateien wurden dabei auf den Commit-Stand zurückgesetzt, ungetrackte gelöscht.

Aufgefallen ist es nur, weil `status.sh set … phase refactored` mit
`no such plan file` abbrach. **Diese Meldung ist ein Datenverlust, kein
Tippfehler.**

Betroffen war unter anderem der komplette achtteilige `cua-explore`-Strang, alle
vier Gerätesync-Dokumente, neun Android-Dokumente, die
Barrierefreiheits-Notizen und die Übergabe vom 12.08.

### Was gerettet wurde

**Alle 32.** Die Dokumente stehen vollständig als `Write`-Aufrufe im
Konversationsarchiv (6015 JSONL-Dateien unter `~/.claude/projects` und
`~/.config/superpowers/conversation-archive`). Ein Skript nimmt je Datei den
jüngsten `Write` und schreibt ihn zurück:

```
/tmp/claude-1000/-home-marvin-Projects-reprise/6b2faf99-…/scratchpad/recover-plans.py
```

Zwei fehlten zunächst, weil sie seinerzeit unter dem Pfad des Worktrees
`reprise-android-desktop-visualizer` geschrieben wurden — der Pfad im Skript ist
der Suchschlüssel, nicht der Dateiname.

Kopien liegen in `/home/marvin/Projects/reprise-plan-backup/`:
`20260813-2111/` (die Überlebenden) und `recovered/` (die 32).

### Drei Dinge, die daraus offen bleiben

1. **Die Ursache ist nicht behoben.** Der Hauptcheckout steht weiterhin in
   detached HEAD und ist weiterhin geteilt. In `docs/plans/` liegen jetzt
   **37 ungetrackte Dateien** — durch das Zurücklegen ist die Angriffsfläche
   größer als vorher, nicht kleiner. Dasselbe kann jederzeit wieder passieren.
   Sauber wäre, die Dokumente zu committen; das im detachten Checkout zu tun,
   während dort eine fremde Sitzung arbeitet, ist allerdings heikel.
2. **Die Phasenvermerke lügen.** Alle 16 wiederhergestellten Dokumente mit
   Frontmatter stehen auf `phase: planned`. Die Wiederherstellung nimmt den
   letzten *vollständigen* `Write`; spätere Phasenwechsel schrieb `status.sh`
   per Edit und sind so nicht rekonstruierbar. **Der Planindex ist damit
   unbrauchbar** — wer nach `phase: shipped` sucht, hält erledigte Arbeit für
   offen. Die echte Phase steht am Branch oder am gemergten PR, nicht im
   Frontmatter. Reparierbar je Slug, aber Fleißarbeit.
3. **Zwei „Überlebende" sind beschädigt.**
   `about-dialog-libadwaita-standard.md` und `android-live-cava-visualizer.md`
   waren getrackt; ihre ungetrackte Arbeitsfassung wurde durch den Commit-Stand
   ersetzt. Sie zeigen jetzt `phase: planned` beziehungsweise `phase: reviewed`
   — ob das dem letzten echten Stand entspricht, ist ungeprüft.

## Nächste Schritte, in der Reihenfolge

1. Die 32 wiederhergestellten Pläne in Sicherheit bringen (committen oder in
   einen eigenen Worktree ziehen), bevor die nächste fremde Sitzung den
   Hauptcheckout bewegt.
2. Den Artwork-Worktree säubern und den Strang landen — oder vorher einen
   eigenen Testlauf fahren, falls Codex' grüne Zahlen nicht genügen.
3. Die visuelle Abnahme nachholen, sobald die AT-SPI-Bridge wieder trägt.
4. Bei Gelegenheit die Phasenvermerke der wiederhergestellten Pläne gegen die
   Branches richtigstellen.
