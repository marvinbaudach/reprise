---
slug: lofty-0-25-migration
worktree: /home/marvin/Projects/reprise/.worktrees/lofty-025
branch: fix/lofty-0-25-migration
phase: planned
codex_session:
created: 2026-08-25
---

# lofty 0.25 hat sein Fehlermodell aufgelöst

## Warum

Dependabot #667 hat `lofty` von 0.24.0 auf 0.25.1 gehoben. 0.25 hat
`lofty::error::LoftyError`, `lofty::error::ErrorKind` und `lofty::error::Result`
ersatzlos gestrichen und durch eine Familie getippter Einzelfehler ersetzt;
`VorbisComments` ist nicht mehr unter `lofty::ogg::` re-exportiert. `reprise-core`
compiliert seitdem nicht: 14 Fehler in der Bibliothek, 28 mit Tests.

Der Bump kam ungeprüft durch, weil zwei kaputte Gates davor (`flatpak-cargo-sources`
und der `setup-uv`-Contract) die Stufe *base and contract checks* zum Scheitern
brachten — der Quality gate lief dahinter gar nicht erst, und auto-merge hat auf
einem Lauf gemerged, der nichts geprüft hat. Beide Gates sind repariert (#680,
#681); dies hier ist der dritte und letzte Grund, warum `dev` rot ist.

## Was 0.25 anbietet

Keine gemeinsame Fehlerklasse mehr. Die Aufrufe, die dieses Repo benutzt, teilen
sich sauber entlang Lesen/Schreiben:

| Aufruf | Fehlertyp in 0.25 |
|---|---|
| `read_from`, `read_from_path`, `Probe::open`, `Probe::read`, `AudioFile::read_from` | `lofty::error::FileParseError` |
| `AudioFile::save_to{,_path}`, `TagExt::save_to{,_path}`, `TagExt::remove_from{,_path}` | `lofty::error::FileEncodingError` |

`FileParseError` kennt `From` für `std::io::Error`, `TagParseError`,
`UnknownFormatError`, `SizeMismatchError` — aber **nicht** für
`UnsupportedTagError` oder `TooMuchDataError`; die beiden können nur in einem
`FileEncodingError` landen. Das ist der Grund, warum ein einzelner `#[from]`-Zweig
nicht mehr reicht.

`VorbisComments` liegt jetzt unter `lofty::ogg::tag::VorbisComments`. Es gibt
**kein** Feature-Flag, das den alten Pfad zurückholt.

## Aufgaben

### T1 — `import_errors.rs`: Klassifikation auf Downcast umstellen

`classify_lofty` matcht heute auf `ErrorKind`. Diesen Weg gibt es nicht mehr.
Neue Signatur nimmt ein Trait-Objekt, damit beide Fehlertypen hindurchpassen, und
läuft die `source()`-Kette ab — genau das Idiom, das lofty intern in
`FileParseError::is_unknown_format()` selbst benutzt.

```rust
pub(crate) fn find_source<'a, T: std::error::Error + 'static>(
    mut e: &'a (dyn std::error::Error + 'static),
) -> Option<&'a T> {
    loop {
        if let Some(found) = e.downcast_ref::<T>() { return Some(found); }
        e = e.source()?;
    }
}
```

`classify_lofty(e: &(dyn std::error::Error + 'static))`: `UnknownFormatError` in
der Kette → `UnsupportedFormat`; sonst `std::io::Error` in der Kette →
`PermissionDenied` bzw. `Io`; sonst `UnreadableTags`. Alle bestehenden Aufrufe
`classify_lofty(&e)` compilieren unverändert weiter, weil beide konkreten Typen
zu `&dyn Error` coercen.

### T2 — Fehlerdetail nicht verlieren (Verhaltensregression)

`FileParseError`/`FileEncodingError` drucken in `Display` **nicht** mehr die
Ursache — nur noch „failed to parse {ty} file". `Debug` ist
`finish_non_exhaustive()` und verschweigt sie ebenfalls. Jedes heutige
`detail = e.to_string()` liefert nach der Migration also sichtbar unschärferen
Text an den Nutzer (`TagMutationFailure.error`, `PromotionError::Tag`), ohne dass
ein Test das merkt — der vorhandene Test prüft nur `!detail.is_empty()`.

Baue `detail` deshalb aus der `source()`-Kette: die Meldung des Fehlers selbst
plus die aller Ursachen, mit `": "` verbunden. Sichere das mit einem Test ab, der
einen `FileParseError` aus einem `io::Error` mit erkennbarem Text baut und
verlangt, dass dieser Text im `detail` vorkommt — nicht bloß, dass `detail`
nichtleer ist.

### T3 — `tag_probe.rs`: Rückgabetyp von `open_probe`

`lofty::error::Result<T>` gibt es nicht mehr. Neue Signatur:
`Result<lofty::probe::Probe<LibraryReadHandle>, lofty::error::FileParseError>`.
`source.open_read(path)?` konvertiert weiterhin automatisch, jetzt über
`FileParseError: From<std::io::Error>`.

### T4 — `tag_edit.rs` und `provenance.rs`: Fehler-Enums aufspalten

Je ein `#[from]`-Zweig pro konkretem Typ — `thiserror` erlaubt `#[from]` nur 1:1:

```rust
#[error("tag operation failed: {0}")]
Lofty(#[from] lofty::error::FileParseError),
#[error("tag write failed: {0}")]
LoftyWrite(#[from] lofty::error::FileEncodingError),
```

In `provenance.rs` zusätzlich der Import:
`use lofty::ogg::tag::VorbisComments;`. Die `?`-Stellen ändern sich nicht.

### T5 — `tag_mutation.rs`: `classify_write_error` neu

`ErrorKind`-Import raus. Die Klassifikation läuft jetzt über die beiden Zweige.
Jede heutige Regel bleibt erhalten:

- `NoWritableTag` → `UnsupportedFormat`
- beide Zweige, `io::ErrorKind::NotFound` in der Kette → `NotFound`
- `LoftyWrite` mit `UnsupportedTagError` in der Kette → `UnsupportedFormat`
- `LoftyWrite` mit `TooMuchDataError` in der Kette → `Io`
- sonst → `WriteErrorKind::from_import_kind(classify_lofty(e).0)`

`UnsupportedTagError`/`TooMuchDataError` müssen auf dem Lese-Zweig nicht geprüft
werden: sie können strukturell nicht in einem `FileParseError` stecken.

Die vier `.map_err(lofty::error::LoftyError::from)` auf rohen
`std::fs::read`/`std::fs::write`-Aufrufen werden zu `FileParseError::from`.

### T6 — `scanner_meta.rs` und `episode_tags.rs`: Typen nachziehen

`scanner_meta.rs`: der `classify`-Closure nimmt `FileParseError`.
`episode_tags.rs`: `Unreadable(FileParseError)`, `Write(FileEncodingError)`.
Beides reine Typänderungen, die Aufrufstellen bleiben.

### T7 — Testfixtures reparieren

Drei Testdateien bauen `LoftyError`/`ErrorKind` direkt und tauchen deshalb erst
mit `--tests` auf:

- `import_errors_tests.rs` — `FileParseError::from(UnknownFormatError)` bzw.
  `let err: FileParseError = io_err.into();`
- `tag_edit_write_tests.rs` — die Fixtures auf die passenden Zweige umbauen.
  Für 0.24s `ErrorKind::NotAPicture` gibt es keinen öffentlich konstruierbaren
  Ersatz (`PictureParseError`s Konstruktor ist `pub(super)`); nimm
  `SizeMismatchError` als anderen Container-Schaden, der ebenfalls als
  `UnreadableTags` klassifizieren muss, und **schreib in einen Kommentar, warum**.
- `scanner_source_tests.rs` — der `TagEditError`-Match wird durch den neuen Zweig
  nicht mehr erschöpfend und braucht einen dritten Arm.

## Was diese Aufgabe nicht tut

Die alte `ErrorKind`-Prüfung war `#[non_exhaustive]` plus Pflicht-Wildcard, damit
ein künftiger lofty-Release mit neuer Fehlerart einen Compilerfehler auslöst und
jemand hinschaut. Diese Zwangsstelle geht ersatzlos verloren: die
Downcast-Kaskade fällt still auf `UnreadableTags` zurück. Das ist ein bewusster
Verlust an Beobachtbarkeit, keine Nachlässigkeit — ein Ersatz dafür ist eine
eigene Aufgabe und gehört nicht in diese.

Kein `Cargo.toml`-Eingriff: `lofty = "0.25"` und der Lock auf 0.25.1 stehen schon.

## Abnahme

```
cargo check -p reprise-core --tests     # 0 Fehler
cargo test  -p reprise-core --lib       # grün, insbesondere classify_lofty_maps_*,
                                        # write_error_classification_*,
                                        # unknown_extension_keeps_the_path_read_unknown_format_verdict
cargo check --workspace                 # kein anderes Crate betroffen
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Zusätzlich der Nachweis zu T2: der neue Detailtest muss fehlschlagen, wenn man
`detail` wieder auf `e.to_string()` zurückdreht.
