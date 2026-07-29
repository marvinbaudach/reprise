# ADR 002: reprise-core reicht einen `Db`-Handle heraus, keine `rusqlite::Connection`

## Status

Angenommen am 2026-07-29.

## Context

`crates/reprise-core/src/db.rs` gibt mit `open_migrated` eine nackte
`rusqlite::Connection` heraus. Das GTK-Frontend wickelt sie in
`Rc<RefCell<Connection>>` (76 Dateien) und borgt sie an 575 Stellen in 130
Dateien. Einen Core-seitigen Handle gibt es nicht.

`scripts/check-frontend-thinness.sh` deckelt die Kategorie `rusqlite` bei 538 —
eine Zahl, deren Name in die Irre führt. Mit der Gate-Semantik gemessen
(`#[cfg(test)]` übersprungen, Kommentarzeilen aus) steht `params!` bei 0,
`.prepare(` bei 0 und `.query_row(` bei 3, während `Connection` 370-mal und
`rusqlite::` 165-mal vorkommt. Das Frontend schreibt kein SQL; es reicht nur
Connections durch seine eigenen Signaturen. Das Budget misst damit nicht, was
sein Name behauptet, und jede künftige echte Verletzung geht im Grundpegel
unter.

Schwerer wiegt die zweite Folge. `AGENTS.md` nennt RefCell-Disziplin die „#1
recurring panic class". Genau diese Klasse sind die 575 `borrow()`: Ein
Ausdruck wie `get_color_scheme(&context.conn.borrow())` hält ein temporäres
`Ref` über den gesamten Aufruf; löst die gerufene Funktion ein GTK-Callback
aus, das erneut auf die Connection zugreift, ist das ein `BorrowMutError` — ein
Absturz, der nur unter bestimmtem Timing auftritt.

Die Portabilität von `reprise-core` — ein zweites Frontend soll die Fachlogik
erben — bleibt außerdem eine Behauptung, solange die Grenze zwischen Frontend
und Core eine Konvention ist und kein Typ.

## Decision

`reprise-core` besitzt einen Handle-Typ `Db`, der die `Connection` privat hält,
und nimmt ihn in seiner **öffentlichen** API überall dort, wo bisher
`&Connection` stand. Der Handle lebt in `crates/reprise-core/src/db/handle.rs`
(nicht in `db.rs` — die steht bei 779 Zeilen).

```rust
pub struct Db { conn: Connection }

impl Db {
    pub fn open_migrated(path: Option<&Path>) -> Result<Self, DbError>;
    pub fn open_in_memory() -> Result<Self, DbError>;
    pub fn path(&self) -> Option<PathBuf>;
    pub(crate) fn conn(&self) -> &Connection;
}
```

Drei Festlegungen tragen den Entwurf:

**Kein `Deref<Target = Connection>`.** Das würde die Connection durch die
Hintertür wieder exponieren und den Zweck aufheben.

**Keine interior mutability.** `Db` hält die `Connection` ohne `RefCell`. Die 62
Core-Funktionen, die heute `&mut Connection` nehmen, tun das ausschließlich
wegen `conn.transaction()`; sie stellen auf `unchecked_transaction()` um, das
mit `&Connection` auskommt und im Core bereits an 13 Stellen benutzt wird
(`concerts/pipeline.rs`, `podcasts/store.rs`, `scrobbling/queue.rs` u. a.).
Damit verschwinden die 575 `borrow()` ersatzlos, statt sich hinter einer
Handle-Methode zu verstecken — ein Handle, der die `RefCell` bloß kapselt,
würde die Panik-Klasse unsichtbar machen statt beseitigen und wäre schlechter
als der Ausgangszustand.

**Nur die öffentliche Ebene wandert.** 386 der 587 Connection-nehmenden
Core-Funktionen sind `pub`; die übrigen ~200 privaten bleiben auf
`&Connection`. Eine `pub fn` holt sich in ihrer ersten Zeile
`let conn = db.conn();` und ruft die private Ebene unverändert.

Während der Umstellung ist `Db::conn()` vorübergehend `pub`, damit jede Etappe
kompiliert und testbar bleibt; die Umstellung gilt erst als abgeschlossen, wenn
`conn()` auf `pub(crate)` heruntergestuft ist. Ab da findet der Compiler jeden
Aufrufer außerhalb des Cores, der die Connection noch anfassen will.

## Consequences

- Das Frontend nennt `rusqlite` nicht mehr. Die Budget-Zahl in
  `check-frontend-thinness.sh` fällt auf einen Wert nahe null und wird damit zu
  einem echten Signal statt zu einem Grundpegel. Das Budget ist Decke **und**
  Boden; der neue Wert wird gemessen eingetragen, nicht gerechnet.
- Die 575 `borrow()`-Aufrufe entfallen. Die häufigste Panik-Klasse des Projekts
  ist für den Datenbankpfad strukturell ausgeschlossen, nicht nur seltener.
- `unchecked_transaction()` gibt Rusts Compile-Zeit-Schutz gegen verschachtelte
  Transaktionen auf: Eine Verschachtelung wird zum Laufzeitfehler. Die 25
  betroffenen Stellen werden bei der Umstellung einzeln daraufhin geprüft, ob
  sie eine andere transaktionale Core-Funktion aufrufen.
- Ein Teilumbau ist kein möglicher Endzustand. Solange `conn()` `pub` ist, ist
  der Zustand sichtbar unfertig; sobald es `pub(crate)` ist, kompiliert ein
  halber Umbau nicht. Zwei Idiome nebeneinander — ein Teil auf `Db`, der Rest
  auf `&Connection` — können nicht versehentlich gemergt werden.
- Worker-Threads öffnen weiterhin ihre eigene `Db` über den Pfad, statt eine
  über Threads zu teilen. Das ändert sich durch den Handle nicht.
- `reprise-cli`, `reprise-mcp` und `reprise-runtime` hängen mit an der
  Core-Oberfläche und stellen mit um.

## Alternatives considered

- **Fassade: der Handle bietet Methoden statt einer Connection.** Verworfen
  nach Messung. Eine erste Schätzung lag bei 58 nötigen Methoden; tatsächlich
  ruft das Frontend **172** distinkte öffentliche Core-Funktionen mit einer
  Connection auf. Eine Fassade dieser Breite wäre eine reine Durchreiche-
  Attrappe, und die Namen sind generisch und über Module verteilt (`load`
  allein aus 11 Frontend-Dateien, dazu `list`, `get_setting`, `is_enabled`),
  sodass eine flache Fassade Namenskollisionen hätte.
- **Closure-Zugriff `db.with(|conn| …)`.** Verworfen, weil es das gemessene
  Problem nicht löst: Das Frontend nennt `Connection` weiter, nur jetzt im
  Closure-Parameter. Die Zahl sinkt kaum, und die Grenze bleibt eine
  Konvention.
- **Alles lassen und nur das Budget senken.** Verworfen, weil die Zahl dann
  weiter etwas anderes misst als ihr Name sagt und die Panik-Klasse bleibt.
