# Handover — Showroom-Relaunch

**Stand:** 2026-08-14 14:10 CEST · Konzept vollständig abgestimmt und als Spec
committet. **Noch nichts umgesetzt**, nichts gepusht, `reprise-showcase`
unangetastet.

**Die Spec ist das maßgebliche Dokument:**
`docs/superpowers/specs/2026-08-14-showroom-relaunch-design.md`
Dieses Handover erklärt nur, wie man dort weiterarbeitet.

## Wo die Arbeit liegt

| | |
|---|---|
| Worktree | `/home/marvin/Projects/reprise-showroom-relaunch` |
| Branch | `showroom-relaunch`, von `origin/dev` |
| Commit | `f1a180791e` — nur die Spec |
| Basis | `a7febd7d92` (Stand 14.08. mittags) |

Der Hauptcheckout `/home/marvin/Projects/reprise` steht auf **detached HEAD**
und wird von anderen Sessions geteilt — dort nicht committen.

## Was dieses Vorhaben vom alten unterscheidet

Es ersetzt `docs/plans/showcase-und-bewerbung-relaunch.md` und dessen HANDOFF
(beide liegen **untracked** im Hauptcheckout und können dort verschwinden —
siehe die Erfahrung aus #459/#464). Vier Entscheidungen kippen den alten Plan:

1. **Eine Bühne statt zwei.** `reprise-showcase` wird archiviert; alles zieht
   nach `marvinbaudach/reprise`, die Seite lebt unter
   `marvinbaudach.github.io/reprise/`. Der alte Plan pflegte zwei Repos.
2. **Zahlen werden gemessen, nicht gepflegt.** Der Pages-Build misst gegen den
   Commit, den er baut. Der alte Plan wollte Zahlen einmal festziehen — sie
   waren nach drei Tagen wieder falsch.
3. **Die Seite wird eine React/TS-Anwendung**, nicht handgeschriebenes HTML.
   Damit belegt das Projekt drei Stacks statt zwei; React/TS ist der berufliche
   Hintergrund des Bewerbers und bisher unbelegt.
4. **Kein Projektsteckbrief-PDF.** Die Seite ist der Steckbrief; die CV-Karte
   führt hin.

Zielrollen sind **Agenten-Engineering** und **Native/Mobile Engineering** —
diese Gewichtung steckt in jedem Abschnitt der Spec und war im alten Plan nie
festgelegt.

## Was als Nächstes zu tun ist

Der Umsetzungsplan fehlt noch. Die Spec nennt fünf Stränge; **Strang 1
(Messung) blockiert 2 und 5**, die Stränge 3 (Aufnahmen) und 4
(Release-Pipeline) laufen unabhängig.

Beginne mit der Messung — ohne sie ist jede Textfläche Spekulation:

- Analyzer um Gruppierung pro Crate erweitern
  (`bewerbung/.skill-staging/update-reprise-code-stats/scripts/reprise-stats/src/main.rs`,
  215 Zeilen; er archiviert alle `.rs` des Commits, projiziert per `syn` die
  Testanteile in einen zweiten Baum und lässt `cloc` beide zählen).
- Aktive UX-Regeln **exakt auszählen**. `docs/ux-rules.md` hat 606
  Listeneinträge, ein Teil trägt Ersetzt-/Withdrawn-Marker, teils mitten in
  Erklärtexten. Die aktive Zahl ist daraus nicht mechanisch ableitbar.
- Gate-Stufen gegen `dev` nachzählen. Das alte README nennt 18; `ci.yml` hat
  drei Jobs, die Stufenzahl kommt aus der dokumentierten Zählung, und #471 hat
  Android-Builds und -Tests ergänzt.

## Fallen in dieser Umgebung

**Die Zahlen-Gates brechen bei der Umstellung.** `readme-evidence.sh` im
Showcase prüft wörtlich auf `5,541`, `172` und `More than 340 active UX rules`.
Zahlenumstellung und Gate-Anpassung müssen **dieselbe** Änderung sein.

**Im Showcase liegt uncommittete Arbeit.** Tauri 2 wurde vollständig entfernt —
README EN/DE, beide Architektur-SVGs (die zwei verbleibenden Frontend-Pillen
wurden neu vermessen, 600 px statt 270/300), dazu die Gate-Zeilen in
`check-showcase.sh` und `readme-evidence.sh`. Beide Gates laufen darauf grün.
**Diese Arbeit portieren, nicht wiederholen.** Ebenfalls entfernt: ein
`Impact-Site-Verification`-Meta-Tag samt Gate-Zeile.

**Der CV zeigt auf das alte Repo und verspricht Tauri.**
`bewerbung/src/lebenslauf.html` verlinkt `github.com/marvinbaudach/reprise-showcase`
und führt Badges „KDE · Tauri 2" / „Windows · Tauri 2". In
`bewerbung/src/shared/profile.js` stehen die Zahlen als Literale mit
Quellkommentar auf `18000adcbe`. Alles drei muss mit.

**`reprise-showcase` nicht löschen und nicht umbenennen.** In der bereits
verschickten Bewerbung `bewerbung/src/bewerbungen/cortec-fullstack.html` steht
die alte URL. Vor dem Archivieren eine weiterleitende `index.html` einsetzen und
die Repo-Beschreibung korrigieren — sie behauptet bis heute *„Public showcase;
the source is private"*, was widerlegbar falsch ist.

**Alle sieben Screenshots sind tot** (Stand 09.08.). Seither wurden genau die
fotografierten Oberflächen umgebaut: Geräte-Karten neu gebaut (#431), Sort-Chip
entfernt (#442), Kopfband-Text von der Grafik genommen (#440), pausierter
Visualizer (#432), Seitenleiste viermal (#441, #422, #423, #450), Android Now
Playing mit neuem Visualizer.

**Kein Android-Video.** Die Emulator-Aufnahme löst hier keine 60 Hz auf und die
App ist auf 60 Hz gedeckelt — das Video zeigte ein Ruckeln, das nicht existiert.

**`origin/dev` bewegt sich schnell.** Während des zweistündigen Brainstormings
sind zwei Commits gelandet, in den drei Tagen davor 118. Jede handgepflegte Zahl
ist veraltet, bevor sie veröffentlicht ist.

**Der Load-Governor-Hook blockt Bash-Kommandos**, die nach schweren
Einstiegspunkten aussehen — etwa der bloße Dateiname eines Gate-Skripts in einem
`grep`. Umformulieren oder `HEAVY_RUN_DISABLE=1` setzen, wenn es wirklich nur
eine Textsuche ist.

**Nicht ohne Freigabe pushen.** Weder Reprise noch Showcase noch Bewerbung.

## Visuelle Entwürfe

Die Mockups aus dem Brainstorming liegen unter
`.superpowers/brainstorm/` im Scratchpad der Session
(`visual-direction`, `protokoll-rhythmus`, `ux-kapitel`, `galerie`,
`schaerfung`, `seek-zurueckgenommen`). Sie sind Wireframes in Farbe, keine
fertigen Entwürfe — die Richtung steht in der Spec, die Dateien sind nur
Anschauungsmaterial und überleben die Session nicht zwingend.

Gewählt wurden: Messprotokoll als Rahmen mit Produktbühne als Bruch · Galerie
als randabfallendes Band, darunter jede Oberfläche einzeln · Seek-Signatur als
zwei Pixel hohe Haarlinie, die nur bei Berührung zum Spektrum wächst.
