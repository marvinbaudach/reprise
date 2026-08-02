---
slug: android-mvp
worktree: /home/marvin/Projects/reprise-android-mvp
branch: feature/android-mvp
phase: planned
codex_session:
created: 2026-08-03
---
# Android-MVP — die Bibliothek auf dem Gerät

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine Android-App, die einen per SAF gewählten Ordner scannt, die
Tracks aus `reprise-core` anzeigt und einen davon abspielt.

**Der eigentliche Zweck ist ein anderer:** Fünf Storage-Pakete sind gegen
Testdoppelgänger bewiesen, die wir selbst geschrieben haben. **Eine echte
SAF-Quelle ist das Erste, was den Entwurf widerlegen kann.** Wenn der
Brücken-Adapter sich natürlich schreiben lässt, war der Schnitt richtig;
verrenkt er sich, sagt er uns, welche Signatur falsch ist.

**Basis:** `dev`, nach Paket 5.

## Was schon steht

- **Werkzeugkette bewiesen** (2026-08-02): NDK unter `/opt/android-ndk`, SDK
  unter `~/Android/Sdk`, drei Rust-Android-Targets installiert,
  `libreprise_android_ffi.so` für `x86_64-linux-android` in 1m50s gebaut,
  Exit 0. Ein Emulator läuft (`emulator-5554`), `adb` und `gradle` sind da.
- **Auf dem Spike-Branch** `spike/android-core-feasibility` liegt eine
  lauffähige App: Manifest, `jniLibs` für arm64 und x86_64, generierte
  UniFFI-Kotlin-Bindings, `MainActivity.kt`, und ein
  `crates/reprise-android-ffi` mit UniFFI 0.32 (130 Zeilen).
- **Auf `dev` liegt davon nichts.** Der Spike-Commit ist als „do not merge"
  markiert.
- Der Kern trägt fünf abstrahierte Storage-Pakete: Aufenthalt, Traversierung,
  Präsenz/Metadaten, Lese-Griff, Tag-Lesung.

## Die Brücke — warum sie einfacher ist, als sie klingt

`LibrarySource` ist ein Rust-Trait mit `&Path`, `Option<LibraryPathMetadata>`,
einem `LibraryReadHandle` (`Box<dyn Read + Seek + Send>`) und einem
Visitor-Callback. **Nichts davon überquert UniFFI** — und das muss es auch
nicht.

Kotlin implementiert eine *andere*, flache Schnittstelle mit vier Methoden:

```
residenceToken(uri: String): Long?
probe(uri: String, follow: Boolean): SourceFacts?
listChildren(uri: String): List<SourceChild>?
openReadFd(uri: String): Int        // wirft bei Fehlschlag
```

Ein Rust-Adapter hält dieses Objekt und implementiert darauf das echte Trait:

- **`open_read`** — `ContentResolver.openFileDescriptor(uri,"r").detachFd()`
  liefert eine **Ganzzahl**. Rust nimmt sie mit `File::from_raw_fd` und hat
  `Read + Seek`. Der Griff überquert nicht, eine Zahl schon.
- **`walk`** — überquert **gar nicht**. Rust baut ihn selbst aus wiederholten
  `listChildren`-Aufrufen: Rekursion, Sortierung, `Stop`-Behandlung,
  Fehlerweitergabe. Kotlin weiß von `walk` nichts. Das ist auch der Grund,
  warum Paket 2 den Baum *streamend* modelliert hat statt als Liste.
- **`probe` / `read_directory`** — flache Records, Zeitstempel als `i64`
  statt `SystemTime`.

**Diese Brücke ändert `LibrarySource` nicht.** Wenn sie es doch verlangt, ist
das ein Befund und gehört in die Commit-Nachricht, nicht stillschweigend
umgesetzt.

## Drei Annahmen, die den Ansatz kippen können

Sie werden **zuerst** geprüft, weil jede für sich alles Weitere wertlos macht.

**A1 — Ist ein SAF-Deskriptor seekbar?** `lofty` braucht `Read + Seek`. Ein
Deskriptor auf eine lokale Datei ist es; einer von einem Netzwerk-Provider
möglicherweise nicht. Wenn nicht, braucht der MVP eine Vorab-Kopie in den
App-Cache — teuer, aber machbar. Das muss man **wissen**, nicht hoffen.

**A2 — Überleben Content-URIs `Path`-Operationen?** Der Kern hält Pfade als
`PathBuf` und vergleicht mit `Path::starts_with`, das **Komponenten**
vergleicht, nicht Zeichen. Eine URI wie
`content://…/tree/primary%3AMusic/document/primary%3AMusic%2Fsong.flac` hat
ihre Trennzeichen teils als `%2F` kodiert. Prüfen: bleibt eine Datei-URI
`starts_with` ihrer Baum-URI? Liefert `extension()` noch `flac`?

**A3 — Der SQL-Vorfilter.** `candidates_under_root` nutzt `path LIKE '<root>/%'
ESCAPE '\'`, und `%` ist das Jokerzeichen. **Bereits geprüft:**
`playlists::escape_like` escapt `\`, `%` und `_`. Risiko erledigt — trotzdem
mit einem Test festhalten, damit es erledigt bleibt.

## Der bewusste Schnitt: Wiedergabe geht am Kern vorbei

`PlaybackBackend` ist ein sauberes Trait, GStreamer liegt in
`reprise-platform-linux`. Eine Media3-Implementierung wäre eine **zweite
Brücke** — und die braucht der MVP nicht.

**Für den MVP spielt Kotlin die Datei direkt ab** (`MediaPlayer` auf der
Content-URI), ohne den Player-Controller des Kerns. Das ist eine bewusste
Verkürzung, kein Versehen: Der MVP soll die **Bibliotheks**-Abstraktion
beweisen. Queue, Wiedergabezustand und `PlaybackBackend` folgen später und
haben ihre eigene Brücke.

## Global Constraints

- **Gates für den Rust-Anteil vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`,
  `bash scripts/tests/gettext-catalogs.sh`.
- **Exit-Codes einzeln erfassen**, nie durch eine Pipe. Testbilanz nach
  **Schlüsselwort** summieren.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Der Desktop darf sich nicht ändern.** Kein Verhalten, keine Zahl, kein
  Verdikt. Der Android-Anteil ist additiv.
- **`reprise-android-ffi` darf nur von `reprise-core` abhängen** — nicht von
  `reprise-gnome`, nicht von `reprise-platform-linux`.
  `scripts/check-architecture.sh` entsprechend erweitern, nicht umgehen.
- **Keine Vorgabe-Implementierung** auf `LibrarySource` (Paket 3s Regel).
- Kein `#[allow(…)]`, kein Schema-Wechsel.

### Android-Umgebung

```
export ANDROID_NDK_HOME=/opt/android-ndk
export ANDROID_HOME=/home/marvin/Android/Sdk
TC=/opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin
export PATH="$TC:$PATH"
export CC_x86_64_linux_android=$TC/x86_64-linux-android21-clang
export AR_x86_64_linux_android=$TC/llvm-ar
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER=$CC_x86_64_linux_android
```

Der Emulator ist `x86_64`; für ihn genügt dieses eine Target. `arm64-v8a`
erst, wenn ein echtes Gerät drankommt.

---

## Phase 0 — Das Gerüst nach `dev` holen

**Files:**
- Create: `crates/reprise-android-ffi/…`
- Create: `android/…` (aus `android-spike/`, umbenannt)
- Modify: `Cargo.toml`, `scripts/check-architecture.sh`

- [x] **Step 1: Baseline messen**

- [x] **Step 2: Übernehmen, nicht neu erfinden**

Der Spike-Branch `spike/android-core-feasibility` hat das Gerüst. Übernimm es
und lass die Herkunft in der Commit-Nachricht stehen. Vorgebaute `.so` werden
**nicht** übernommen — sie werden gebaut.

- [x] **Step 3: Ein Build-Skript**

`scripts/android-build.sh`, das die Umgebung oben setzt, das Target baut und
die `.so` an die richtige Stelle legt. Damit ist der Build reproduzierbar und
nicht an einer Shell-Historie hängend.

- [x] **Step 4: Volle Gates und Commit**

---

## Phase 1 — Die drei Annahmen

**Files:**
- Create: Tests in `crates/reprise-core`
- Modify: `crates/reprise-android-ffi/src/lib.rs`

- [x] **Step 1: A2 und A3 in Rust-Tests**

Beide sind ohne Gerät prüfbar: eine echte Content-URI als `PathBuf`, dann
`starts_with`, `extension`, und der `LIKE`-Vorfilter gegen eine
In-Memory-Datenbank. **Erst rot beobachten, wo es rot sein muss.**

Fällt A2, halte fest **woran genau** — davon hängt ab, ob der MVP URIs als
Pfade führen kann oder eine Übersetzungsschicht braucht.

- [ ] **Step 2: A1 auf dem Emulator**

Eine FFI-Funktion, die einen Deskriptor entgegennimmt, `File::from_raw_fd`
macht, liest, `seek`t und wieder liest. Von Kotlin mit einem echten
SAF-Deskriptor aufgerufen, Ergebnis über `adb logcat` geprüft.

- [x] **Step 3: Festhalten, dann Commit**

Die drei Antworten in `docs/research/android-spike-2026-08.md`. **Wenn A1 oder
A2 fällt, endet die Phase hier** und der Plan wird neu geschnitten — nicht
drumherum gebaut.

---

## Phase 2 — Die Brücke

**Files:**
- Modify: `crates/reprise-android-ffi/src/lib.rs`
- Create: `crates/reprise-android-ffi/src/source.rs`

- [ ] **Step 1: Die UniFFI-Schnittstelle**

Vier Methoden, flache Records. Kein `Path`, kein Griff, kein Visitor.

- [ ] **Step 2: Der Adapter**

`impl LibrarySource for BridgedSource`. `walk` wird aus `listChildren`
abgeleitet — Rekursion, `LibraryWalkOrder`, `LibraryWalkControl::Stop`,
Fehler als `LibraryWalkItem::Error`.

**Beachte, was Paket 3 gelernt hat:** `probe` liefert `None` für „nicht da",
und zwei Aufrufstellen machen daraus einen `missing_since`-Eintrag. Eine
SAF-Quelle, die bei einem gescheiterten Binder-Rundlauf `None` liefert, würde
lebende Tracks als gelöscht markieren. **Der Adapter muss zwischen „der
Provider sagt: gibt es nicht" und „der Aufruf ist gescheitert"
unterscheiden.** Kann die Kotlin-Seite das nicht liefern, ist das der
wichtigste Befund des ganzen MVP und gehört sofort festgehalten.

- [ ] **Step 3: Der Adapter gegen einen Fake**

Ein Rust-Test mit einem Fake-`SafSource` (In-Memory-Baum), der beweist, dass
der abgeleitete `walk` dieselbe Reihenfolge- und Filterzusicherung liefert wie
`UnixLibrarySource`.

- [ ] **Step 4: Volle Gates und Commit**

---

## Phase 3 — Scannen auf dem Gerät

**Files:**
- Modify: `crates/reprise-android-ffi/src/lib.rs`
- Modify: `android/app/src/main/java/…`

- [ ] **Step 1: Die FFI-Oberfläche**

Datenbank öffnen (App-privates Verzeichnis), Baum-URI setzen, scannen,
Tracks auflisten. Fortschritt als Rückruf — `ScanProgress::Scanning::total`
ist seit Paket 2 `Option<u64>`, und der Erstscan hat **keine** Schätzung: die
Oberfläche zeigt einen unbestimmten Balken, keine erfundene Prozentzahl.

- [ ] **Step 2: Die Kotlin-Quelle**

`ContentResolver`, `DocumentsContract.buildChildDocumentsUriUsingTree`,
`openFileDescriptor`. Die vier Methoden, mehr nicht.

- [ ] **Step 3: Auf dem Emulator scannen**

Fixtures nach `/sdcard/Music` schieben, Ordner per SAF wählen, scannen. Der
Beweis ist eine Zeile in `adb logcat` mit der Trackzahl **und** eine Abfrage
der Datenbank, die dieselbe Zahl nennt.

- [ ] **Step 4: Commit mit dem Emulator-Beleg in der Nachricht**

---

## Phase 4 — Die Oberfläche

**Files:**
- Modify: `android/app/src/main/java/…`

- [ ] **Step 1: Ordner wählen, scannen, Liste zeigen**

Compose. Drei Zustände: kein Ordner, scannt, Liste. Kein Design-Anspruch —
lesbar und ehrlich reicht.

- [ ] **Step 2: Einen Track abspielen**

`MediaPlayer` auf der Content-URI, direkt aus Kotlin. Siehe „bewusster
Schnitt" oben.

- [ ] **Step 3: Auf dem Emulator prüfen, Screenshot, Commit**

---

## Phase 5 — Festhalten

- [ ] **Step 1: Was der MVP über die Abstraktion gesagt hat**

Die ehrliche Bilanz: Welche der fünf Pakete haben getragen, welche Signatur
hat sich verrenkt, was würde man anders schneiden. **Das ist der Ertrag**, den
kein weiteres Paket gegen Doppelgänger liefern kann.

- [ ] **Step 2: Ledger, Gates, Commit**

---

## Was der MVP ausdrücklich nicht ist

Keine Schreibseite (Tags, Cover), kein `mount_point_of`, kein Watcher, keine
Queue, keine Wiedergabe über den Kern, keine Übersetzungen, kein Design.
Jedes davon ist ein eigenes Paket, und keines davon steht zwischen hier und
„die Bibliothek erscheint auf dem Telefon".
