# Android-Spike — Befunde (2026-08)

Spec: `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`
Plan: `docs/superpowers/plans/2026-08-01-multi-surface-p0-s1.md`

Dieser Bericht beantwortet die Fragen aus Spec §4/S1. Jeder Abschnitt endet
mit einem Urteil: TRÄGT / TRÄGT MIT AUFLAGEN / TRÄGT NICHT.

Stand: Frage 5 ist beantwortet (Recherche, 2026-08-01). Die Fragen 0 bis 4
brauchen den lokalen Prototyp und stehen noch aus.

---

## Frage 5 — Ist die Rust-NDK-Toolchain im F-Droid-Buildserver baubar?

**Urteil: TRÄGT MIT AUFLAGEN.** Die Abbruchbedingung aus B10 ist damit
aufgehoben; die Fragen 0 bis 4 des Spikes werden ausgeführt.

### Präzedenzfall

**Delta Chat** (`com.b44t.messenger`) baut eine Rust-Kernbibliothek über das
NDK aus einem produktiven, laufend gepflegten F-Droid-Rezept. Version 2.57.0
wurde am **2026-07-31** veröffentlicht — der Fall ist aktuell, nicht
historisch. Die Rust-Integration reicht in derselben Metadatei bis Version
1.1.2 (2019) zurück, über NDK r14b → r27.

Der reale Build-Eintrag (`metadata/com.b44t.messenger.yml`, `master`):

```yaml
  - versionName: 2.57.0
    versionCode: 7544
    timeout: 20000
    sudo:
      - apt-get update
      - apt-get install -y make g++ cmake rustup
    prebuild: sed -i -e 's/abiFilters .*/abiFilters "x86_64"/' ...
    build:
      - rustup default $(cat scripts/rust-toolchain)
      - rustup target add x86_64-linux-android
      - PATH=$PATH:$$NDK$$/toolchains/llvm/prebuilt/linux-x86_64/bin/ \
        ANDROID_NDK_ROOT=$$NDK$$ scripts/ndk-make.sh x86_64
    ndk: r27
```

Vier solche Einträge existieren je Version — **einer je ABI, mit eigenem
`versionCode`**. F-Droid unterstützt keine App Bundles oder native
Split-APKs; das ist die etablierte Umgehung.

Ein zweiter, teilweise passender Beleg ist **RiseupVPN**
(`se.leap.riseupvpn`): dasselbe Muster „`sudo:` installiert
Fremdsprachen-Toolchain, `build:` kompiliert nativ über NDK" — dort für Go
statt Rust.

### Die vier Teilfragen

**NDK-Verfügbarkeit.** Die Beschaffung ist **versionsagnostisch**:
`fdroidserver/common.py` (`auto_install_ndk`) ruft
`sdkmanager.build_package_list(use_net=True)` und danach
`sdkmanager.install(f'ndk;{ndk}')` — es gibt keine Whitelist. NDK
29.0.14206865 ist eine offizielle Google-Veröffentlichung und sollte damit
genauso beziehbar sein wie r27. **Bewiesen ist aber nur bis r27.**
→ Auflage: r27 als Rückfallebene einplanen.

**Toolchain-Beschaffung.** `rustup` darf aufgerufen werden und Targets
nachinstallieren; das ist der heute dominante Weg, keine Ausnahme. Debian
liefert inzwischen ein `rustup`-Paket, und die im Repo gepinnte
`rust-toolchain`-Datei bestimmt die Compilerversion — Reprises Rust-1.92-
Anforderung ist damit vom Debian-Basisimage entkoppelt.

**Netzzugang während des Builds.** Vorhanden. F-Droids Blogpost von 2022
behauptet das Gegenteil, wird aber von zwei Primärquellen widerlegt:
`fdroidserver` lädt das NDK selbst mit `use_net=True`, und Delta Chats
aktives Rezept ruft `apt-get install` und `rustup target add` auf. Die
formale Inclusion Policy enthält keine Netzregel für den Build — ihre
Netzregeln betreffen die Laufzeit der fertigen App.
→ **`cargo vendor` ist nicht nötig.** Zur Einordnung: ein Vendor-Lauf über
den heutigen Desktop-Workspace ergab 670 MB, davon ~300 MB reine
Windows-Crates, die für Android nie gezogen würden.

**Zeitgrenzen.** Das `timeout:`-Feld ist dokumentiert, Vorgabe 7200 s,
`0` = unbegrenzt. Delta Chat setzt 20000 s **je ABI-Eintrag**. Das eigentliche
Risiko liegt woanders: die **GitLab-CI-Pipeline**, die jede
`fdroiddata`-Merge-Request testweise baut, hat bei vergleichbaren
From-Source-Projekten (Godot, `com.controlloid`) bereits manuelles
Nachjustieren durch Maintainer gebraucht.

### Auflagen für P8

1. **Ein Build-Eintrag je ABI** mit eigenem `versionCode` (arm64-v8a,
   armeabi-v7a, x86_64) statt einer Fat-APK.
2. **Großzügiges `timeout:`**, an Delta Chats 20000 s orientiert.
3. **NDK zuerst mit der aktuellen Version versuchen, r27 als Rückfall.**
4. **Die Erstaufnahme-Pipeline früh mit den F-Droid-Maintainern klären**,
   nicht erst beim Einreichen.

### Entwurf eines Builds-Eintrags

Angelehnt an Delta Chat, aber mit `cargo-ndk` (lokal bereits im Einsatz)
statt handgeschriebener Linker-Konfiguration. Ein Eintrag je ABI:

```yaml
  - versionName: '1.0.0'
    versionCode: 1001
    commit: <tag>
    subdir: android/app
    timeout: 21600
    sudo:
      - apt-get update
      - apt-get install -y make g++ cmake rustup pkg-config
    prebuild: sed -i 's/abiFilters .*/abiFilters "arm64-v8a"/' build.gradle
    build:
      - rustup default $(cat rust-toolchain)
      - rustup target add aarch64-linux-android
      - cargo install cargo-ndk --locked --version 4.1.2
      - cd ../../<bindings-crate>
      - cargo ndk -t arm64-v8a -o ../android/app/src/main/jniLibs build --release
    ndk: r27
    gradle:
      - foss
```

`rusqlite`/`bundled` braucht nur einen funktionierenden C-Compiler fürs
Ziel; `cargo-ndk` setzt `CC_<target>`/`AR_<target>` aus der NDK-Toolchain,
und `cmake`/`make`/`g++` kommen wie bei Delta Chat über `sudo:`.

### Restunsicherheiten

1. **NDK 29.0.14206865 ist konkret ungetestet.** Der Mechanismus spricht
   dafür, bewiesen ist r27.
2. **Die Erstaufnahme-Pipeline** ist eine vom `timeout:`-Feld unabhängige
   Hürde und für ein Workspace dieser Größe ungetestet.
3. **Reale Laufzeit und Hardware** der Produktionsflotte sind nicht
   dokumentiert; ein aktueller Diskussionsfaden deutet auf ältere
   Build-Maschinen hin. Ohne echten Lauf bleibt die Dauer eine Hochrechnung.

### Quellen

- `https://gitlab.com/fdroid/fdroiddata/-/raw/master/metadata/com.b44t.messenger.yml`
- `https://f-droid.org/en/packages/com.b44t.messenger/`
- `https://raw.githubusercontent.com/f-droid/fdroidserver/master/fdroidserver/common.py`
- `https://gitlab.com/fdroid/fdroid-website/-/raw/master/_docs/Build_Metadata_Reference.md`
- `https://gitlab.com/fdroid/fdroid-website/-/raw/master/_docs/Inclusion_Policy.md`
- `https://forum.f-droid.org/t/pipeline-timeout-building-a-new-app/32809`
- `https://forum.f-droid.org/t/build-timeout-for-com-controlloid/7833`

---

## Frage 0 — Baut der Rust-Baum überhaupt für Android?

**Urteil: TRÄGT.** Ohne Auflagen, ohne Nacharbeit an einer einzigen Zeile
Rust.

Umgebung: NDK **29.0.14206865** (`/opt/android-sdk/ndk`), `cargo-ndk` 4.1.2,
Rust 1.92, Targets `aarch64-linux-android`, `armv7-linux-androideabi`,
`x86_64-linux-android`.

| Crate | ABI | Release-Build |
| --- | --- | --- |
| `reprise-core` | arm64-v8a | **OK**, 43 s |
| `reprise-core` | armeabi-v7a | **OK**, 42 s |
| `reprise-core` | x86_64 | **OK**, 45 s |
| `reprise-runtime` | arm64-v8a | **OK**, 7 s |

Drei Befunde:

1. **`rusqlite` mit `bundled` übersetzt SQLite aus C für alle drei
   Android-Architekturen**, ohne dass an der Toolchain etwas eingerichtet
   werden musste. `cargo-ndk` setzt `CC_<target>`/`AR_<target>` aus dem NDK
   selbst. Das war das größte Einzelrisiko dieser Frage und es ist keins.
2. **`reprise-runtime` baut in 7 Sekunden auf einem fremden Target.** Damit
   ist Spec §2.1 („die Runtime ist transportfrei") nicht mehr nur eine
   Behauptung über den Dependency-Baum, sondern auf Android bewiesen — sie
   zieht nichts Linux-Spezifisches nach.
3. **Erzeugt werden `.rlib`-Dateien, nicht `.so`.** Das ist erwartet: beide
   sind reine Bibliotheks-Crates. Die gemeinsame Bibliothek entsteht später
   aus einer Bindings-Crate mit `crate-type = ["cdylib"]` — das ist Teil von
   Frage 3 (UniFFI), nicht dieser Frage.

Zur Einordnung für P8: knapp 45 s je ABI im Release-Build auf einem
Entwicklungsrechner mit warmem Cache. Der F-Droid-Buildserver baut kalt und
auf schwächerer Hardware, aber die Größenordnung liegt weit unter dem
Timeout-Budget, das Delta Chat dort verwendet.

## Frage 3 — Trägt UniFFI die Typen von reprise-view?

**Urteil: TRÄGT MIT EINER AUFLAGE**, und die Auflage prägt P1a.

Prüfobjekt war `ui/track_list/queue_sections.rs`, nachgebaut in
`spikes/uniffi-shapes` gegen UniFFI 0.29 (Proc-Macro-Modus, plus ein
`uniffi-bindgen`-Binärziel).

### Was trägt

Verschachtelte Records, Listen, `u32`/`i64`/`String` und — bemerkenswert —
**Enums mit Datenvariante**. `QueueSectionKind::UpNext { source_label }`
wird auf der Kotlin-Seite zu einer `sealed class`, also genau der
idiomatischen Entsprechung:

```kotlin
data class QueueRow(...)
data class QueueSection(...)
data class QueueViewModel(...)
sealed class QueueSectionKind { ... }
fun queueViewModel(rows: UInt): QueueViewModel
fun queueWindow(start: UInt, len: UInt): List<Long>
```

### Was nicht trägt — und das ist der eigentliche Befund

`QueueViewModel` hält heute über `VirtualContextTail` einen **boxed
Closure**:

```rust
window: Rc<dyn Fn(usize, usize) -> Vec<i64>>
```

Das ist die Naht, an der die GTK-Liste beim Scrollen in Rust zurückruft, um
den Kontext-Schwanz fensterweise nachzuladen. Über eine FFI-Grenze ist sie
nicht darstellbar. Gegenprobe gefahren, drei Fehler:

```text
the trait `uniffi::TypeId<UniFfiTag>` is not implemented for
  `Rc<dyn Fn(usize, usize) -> Vec<i64>>`
the trait `uniffi::Lower<UniFfiTag>` is not implemented for ...
the trait `Lift<UniFfiTag>` is not implemented for ...
```

Die geteilte Form kompiliert dagegen sauber: das ViewModel trägt nur noch
`context_len`, und das Fenster holt der Aufrufer über einen **expliziten**
`queue_window(start, len)`.

### Konsequenz für P1a

**ViewModels in `reprise-view` dürfen keine Closures halten.** Faules
Nachladen wird von Rückruf zu Anfrage/Antwort. Das betrifft **auch GTK** —
beide Oberflächen konsumieren dieselbe Schicht, also verliert das
GTK-Frontend an dieser Stelle seinen Closure-Rückruf und bekommt denselben
expliziten Fensteraufruf. Das ist kein Nebenschauplatz von P1a, sondern eine
seiner Kernumbauten.

### Noch nicht gemessen

Die Kosten für eine lange Liste über die Grenze (Plan Task 6 Step 4: 10.000
Zeilen). Dafür braucht es eine Kotlin-Laufzeit, nicht nur die Generierung.
Die Frage ist offen, ob das ViewModel am Stück oder seitenweise geholt
werden muss — die Antwort ändert die Auflage oben nicht, nur ihre
Dimensionierung.

## Frage 1 — Erfüllt Media3 den playback-Vertrag?

Offen. Braucht den lokalen Prototyp (Plan Task 7).

## Frage 2 — Kann ein MediaSessionService die Runtime beherbergen?

Offen. Braucht ein echtes Gerät (Plan Task 8).

## Frage 4 — Trägt SAF den Scanner?

**Zwischenstand aus Code-Analyse (2026-08-01): TRÄGT NICHT ohne
Storage-Abstraktion.** Der Prototyp aus Plan Task 9 steht noch aus und muss
das Ausmaß bestätigen — die Richtung ist aber bereits klar, und sie ist die
teuerste Nachricht dieses Spikes.

Vier Mechanismen in `reprise-core` hängen strukturell an einem echten
Dateipfad:

1. **Der Scanner selbst.** `walkdir::WalkDir::new(root)` in
   `library/scanner.rs:264` und `library/scanner_progress.rs:15` — beide über
   `&Path`.
2. **Die Unmounted-gegen-Deleted-Klassifikation.** `library/mounts.rs` ist der
   heikelste Punkt: sein Modulkommentar hält ausdrücklich fest, der
   Mechanismus komme *„without any platform trait, GVolumeMonitor, or
   `/proc/mounts` parsing"* aus. Er ruht ganz auf `tracks.device` — dem
   `st_dev` der Datei aus Schema v2. Auf SAF gibt es kein `st_dev`. Das ist
   keine Lücke, sondern eine **bewusste Architekturentscheidung, die Android
   zunichtemacht**.
3. **Geschwisterdateien und alle drei Writeback-Pfade** (Cover, Tags,
   `.lrc`-Sidecars). Gemessen: **49 Produktivdateien** in
   `crates/reprise-core/src` nutzen `std::fs` direkt (90 inklusive Tests).
4. **Der `notify`-Watcher.** Auf SAF gibt es keine Entsprechung — das ist
   zugleich die Antwort auf offenen Punkt O3.

### Der Tag-Pfad ist handle-fähig — gemessen, nicht vermutet

Die teuerste Teilfrage war, ob `lofty` überhaupt ohne Pfad arbeiten kann.
SAF liefert `content://`-URIs und daraus Dateideskriptoren, nie Pfade;
`reprise-core` ruft heute ausnahmslos `read_from_path`/`save_to_path`.

Gemessen am 2026-08-01 gegen `lofty` 0.24 mit
`crates/reprise-core/tests/fixtures/sine.flac`:

```text
LESEN AUS HANDLE: OK    Format: Flac, Tags: 1
SCHREIBEN IN HANDLE: OK
```

`lofty::read_from(&mut File)` und `AudioFile::save_to(&mut File,
WriteOptions)` existieren beide und funktionieren. Damit ist der gesamte
Tag-Lese- und Schreibpfad über einen Dateideskriptor bedienbar — der Umbau
ist ein Signaturwechsel, kein Ersatz der Bibliothek.

Das verkleinert den Befund deutlich. Übrig bleiben drei Stellen, die
wirklich neu gedacht werden müssen: das **Auflisten** eines Baums (SAF hat
mit `DocumentsContract` eine eigene API dafür), die
**Unmounted-Klassifikation** über `st_dev`, und der **Watcher**.

### Was daraus folgt

Eine Android-Oberfläche über unverändertem Core reicht nicht. Es braucht eine
echte **Storage-Abstraktion** — ein `LibrarySource`-artiges Trait mit
Auflisten, Lesen, Schreiben und Erreichbarkeitsprüfung — mit zwei
Implementierungen, durchgezogen durch Scanner, Mount-Klassifikation und alle
Writeback-Pfade. Dazu ein Ersatz für die `tracks.device`-Spalte, weil SAF
kein `st_dev` kennt.

**Rückwirkung auf die Planung:** Das ist Arbeit an `reprise-core`, nicht an
Android — sie gehört damit vor P4a und berührt Code, den auch P1a anfasst.
Die Größenordnung ist noch nicht seriös schätzbar; Task 9 muss den
Übergabemechanismus (Dateideskriptoren nach Rust gegen Pfad-Trait) klären,
bevor daraus ein Paket wird.

**Was dabei nicht verlorengeht:** Die Zweiteilung „vorübergehend
unerreichbar" gegen „endgültig weg" ist auf dem Desktop bereits als Konzept
vorhanden (`MissingReason::Unmounted` / `Deleted` / `Unknown`). Sie trägt auf
Android weiter, nur die Feststellung braucht einen anderen Weg als `st_dev`.
