# Core-Db-Handle — Abschluss-Handover für Claude Code

Stand: 2026-07-30. Die aktuelle Db-Handle-Stufe ist implementiert. Dieses
Dokument ersetzt das Zwischen-Handover vom 2026-07-29.

## Checkout und Grenze

- Worktree:
  `/home/marvin/Projects/reprise/.worktrees/core-db-handle`
- Branch: `refactor/core-db-handle`
- Integrierte Base: aktuelles `dev` bei `75f2d9cf15`
- Kein Push, PR oder Merge nach `dev` wurde ausgeführt.
- Keine echte Reprise-Datenbank, Musikbibliothek oder Desktop-Session wurde
  verwendet.
- Im Repository ist für diesen Worktree weder eine Koordinationsdatei noch ein
  zu beanspruchender Repository-Lock dokumentiert.

Der Feature-Branch-zu-PR-zu-`dev`-Workflow ist ausdrücklich freigegeben.
`dev` darf nicht nach `main` promoviert und die nächste Roadmap-Stufe nicht
begonnen werden.

## Ergebnis

`reprise_core::db::Db` besitzt die Library-Connection ohne `RefCell` und ohne
`Deref<Target = Connection>`. Die öffentliche Core-API nimmt `&Db`; nur
private Core-Helfer arbeiten noch mit `&Connection`. `Db::conn()` ist
`pub(crate)` und dadurch außerhalb von `reprise-core` nicht erreichbar.

GNOME hält `Rc<Db>`. CLI, MCP und Runtime besitzen ebenfalls `Db` statt einer
rohen Connection. Worker öffnen weiterhin je einen eigenen Handle über den
Datenbankpfad. Tests außerhalb des Cores greifen für Fixtures nicht durch den
Handle, sondern öffnen eine isolierte, dateibasierte Testdatenbank mit einer
separaten Fixture-Connection. Damit bleibt die Produktionsgrenze auch im
Testcode geschlossen.

Die relevanten Strukturinvarianten sind wie folgt abgesichert:

- `pub(crate)` macht externe `.conn()`-Aufrufe zu Compilerfehlern; der
  Frontend-Thinness-Gate verbietet sie im GTK-Produktionscode zusätzlich;
- Frontend-Thinness-Budget `rusqlite = 112`, als exakte Decke und Boden;
- Rust-Dateien bleiben unter den Architekturgrenzen.

Zwei weitere Abschlussinvarianten wurden mit expliziten Repository-Suchen
verifiziert, sind aber nicht als eigene dauerhafte Gates implementiert: Es
gibt weder Datenbank-`Rc<RefCell<Connection>>`/`RefCell<Db>` noch einen
öffentlichen Core-Einstieg mit `Connection` in der Signatur.

## Commits

```text
1988d7a9f6 feat(core): add the Db handle that owns the library connection
0834c15e08 refactor(core): take &Connection where &mut Connection was only for transactions
662e6043b9 refactor(gnome): own the library database through Db
2349bafd71 refactor(adapters): own the library database through Db
559a9d87a6 refactor(core): take Db at public database boundaries
de7690648c refactor(core): seal the Db connection boundary
759611df09 Merge remote-tracking branch 'origin/dev' into refactor/core-db-handle
27982a9ee5 docs(adr): record the core Db handle decision
49bd9a6ae5 test(qa): track the CUA sqlite3 dependency
b31c19bb78 docs(core): link the public Db boundary in rustdoc
53964b0440 docs(agents): record the Db handle refactor stage
4b4f313c12 docs(agents): record the clean Db handle gate
65a64a90c9 docs(agents): distinguish targeted and merge gates
a784b3f389 Merge remote-tracking branch 'origin/dev' into refactor/core-db-handle
```

Der ADR entstand ursprünglich als `298b8dc7c4` auf dem separaten ADR-Branch
und wurde nach der `dev`-Integration als `27982a9ee5` in diesen Branch
cherry-gepickt. Die Merge-Auflösung behielt sowohl die Db-Grenze als auch die
neue MTP-46-Modulgating-Semantik aus `dev`.

## Verifikation

Der endgültige saubere Stand bestand im vollständigen
`MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh
--no-fetch`-Lauf:

- `cargo fmt --check`;
- `cargo clippy --locked --all-targets --workspace -- -D warnings`;
- warning-freies Workspace-Rustdoc;
- die ungekürzte Workspace-Suite, einschließlich der zwei MCP-Radio-
  Sockettests, die ein früherer Sandbox-Lauf nicht binden konnte;
- Architektur, Frontend-Thinness, Device-Sync-GStreamer,
  Runtime-Service-Installation, Accessibility/Input-Parity, UX-Traceability
  und Motion-Tokens;
- Rule-named Displaytests 215/215, Motion-Displaytests 26/26,
  CSS-Displaytests 10/10 und Runtime-D-Bus 25/25;
- `cargo audit` mit ausschließlich dem akzeptierten `RUSTSEC-2024-0436`
  (`paste`).

Zusätzlich liefen auf demselben Code-Stand `cargo fmt --all -- --check`, die
gezielten MTP-46-Suiten (Core 19/19, GNOME 2/2 plus ein display-only
ignorierter Test), der QA-Linter, `git diff --check` sowie die expliziten
Core-Purity- und Abschlussinvarianten-Suchen grün.

Die beiden zuvor an `TcpListener::bind` blockierten Tests liefen im finalen
Clean-tree-Gate grün; hierfür ist kein manueller Nachtrag mehr offen.
`cargo-machete` ist auf diesem Host nicht installiert; der entsprechende
Thinness-Teilschritt wurde daher als `SKIPPED` gemeldet. CUA war nicht
angezeigt, weil die Stufe Besitz und Typgrenzen ändert, aber keine sichtbare
GTK-Semantik.

Der vollständige Gate endete mit Exit 0 und
`Merge-readiness checks passed against origin/dev`.

## Annahmen, Risiken und nächste zulässige Handlung

- Die auf `unchecked_transaction()` migrierten Stellen wurden auf
  verschachtelte Transaktionen geprüft. Der verbleibende Unterschied ist ein
  Laufzeitfehler statt einer Compile-Time-Sperre, falls künftig eine neue
  Verschachtelung eingeführt wird.
- Dateibasierte Test-Fixtures sind absichtlich gewählt: Sie prüfen denselben
  Mehr-Connection-Pfad wie Produktion, ohne auf Nutzerdaten zuzugreifen.
- Der Merge mit `dev` ist lokal und absichtlich als Merge-Commit erhalten; es
  wurde keine Historie umgeschrieben.
- Es sind keine manuellen Produktchecks offen.

Als nächster Schritt ist der freigegebene Push mit anschließendem
Squash-PR-Merge nach `dev` zulässig. `dev` nach `main` und die nächste
Roadmap-Stufe bleiben außerhalb dieser Freigabe.
