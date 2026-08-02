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

**Urteil: TRÄGT**, mit einer benannten Lücke.

Gemessen am 2026-08-01 auf einem **Pixel 10 Pro XL, Android 17 (API 37)** —
also am strengsten verfügbaren API-Level — mit Media3 1.10.1, gegen zwei
fünfminütige FLAC-Proben.

| Vertragsteil | Beleg |
| --- | --- |
| Laden | `existiert=true bytes=10066662`, dann `BUFFERING → READY` |
| Dauer | `dur=300000` — korrekt aus der Datei, nicht geraten |
| Abspielen | `isPlaying=true`, `pos` wächst |
| Position lesen | `pos=4667`, später `pos=38325` |
| Suchen | `BUFFERING pos=35989 → READY pos=36048`, Wiedergabe läuft weiter |
| Pausieren | `isPlaying=false`, `pos=39512` eingefroren |
| **Gapless** | `itemTransition reason=1` (`AUTO`) bei `pos=283873`, direkt gefolgt von `pos=20164` im nächsten Titel — **ohne** `BUFFERING` und **ohne** `ENDED` dazwischen |

Der Gapless-Beleg ist der wertvollste: Media3 wechselt automatisch und ohne
Pufferstillstand. Ein Zwischenzustand hätte sich im Log gezeigt.

**Die Lücke: Crossfade.** ExoPlayer bringt kein Überblenden mit; das
GTK-Frontend kann es. Für P4a heißt das entweder Eigenbau über zwei
Player-Instanzen oder ein bewusster Verzicht auf Android. **Nicht gemessen**,
weil es nichts zu messen gab — die Funktion existiert schlicht nicht.

**Ebenfalls nicht gemessen:** `STATE_ENDED` am Ende der letzten Queue-Position
(der Lauf endete vorher).

## Frage 2 — Kann ein MediaSessionService die Runtime beherbergen?

**Urteil: TRÄGT** für alles, was ohne Handgriff am Gerät prüfbar war.

Ein Befund vorweg, der B7 stützt: **Service und Activity laufen im selben
Prozess** (dieselbe PID im Log). Der Service ist damit ein tragfähiger Wirt
für eine eingebettete Runtime — sie müsste nicht über eine Prozessgrenze
angesprochen werden.

| Prüfung | Ergebnis |
| --- | --- |
| Foreground-Service-Typ (AUD-8) | `isForeground=true`, `types=0x00000002` = `MEDIA_PLAYBACK`. Auf API 37 ist das die Pflichtangabe, ohne die `startForeground()` abstürzt |
| Medienbenachrichtigung | `category=transport`, `actions=2`, `NO_CLEAR｜FOREGROUND_SERVICE`, `vis=PUBLIC` |
| Audio-Focus angefordert | Im System-Focus-Stack: `pack: dev.reprise.spike`, `gain: GAIN`, `loss: none`, `usage=USAGE_MEDIA`, angefordert durch `androidx.media3.common.audio.AudioFocusManager` |
| Hintergrundwiedergabe | 60 s Bildschirm aus: **null Ereignisse**, Prozess lebt, Wiedergabe ununterbrochen |
| `POST_NOTIFICATIONS` | erteilt; die Wiedergabe hing zu keinem Zeitpunkt daran (AUD-13) |

`setAudioAttributes(…, handleAudioFocus = true)` und
`setHandleAudioBecomingNoisy(true)` genügen also, um die Anforderung
überhaupt zu stellen — nachgewiesen im Focus-Stack des Systems, nicht nur im
App-Code.

### Die drei Handgriffe am Gerät — nachgeholt

**Focus-Verlust: die Runtime erfährt davon.** Eine fremde App mit Ton
gestartet, und der Listener feuerte:

```text
19:14:44.288  EVENT isPlaying=false
```

**Das ist der wertvollste Einzelbefund dieses Spikes.** Media3 pausiert nicht
still hinter dem Rücken der Anwendung — der Zustandswechsel läuft über
`onIsPlayingChanged`, also über denselben Rückruf, den `reprise-runtime`
ohnehin abonnieren wird. Die befürchtete Konsequenz aus B7 (ein neu zu
erfindender Zustandsübergang) **entfällt**.

Bemerkenswert und regelkonform: **kein Auto-Resume** nach dem Ende der
Störung. Das entspricht `AUD-2` — bei dauerhaftem Verlust ist
Nicht-Fortsetzen das richtige Verhalten, kein Versäumnis.

**`ACTION_AUDIO_BECOMING_NOISY`: greift.** Manuell beobachtet (Kopfhörer im
USB-C-Port, deshalb ohne adb): „beim Abziehen stoppt der Ton". Für diese
Frage ist die Wahrnehmung das Kriterium; `setHandleAudioBecomingNoisy(true)`
tut, was es soll.

**Aus Recents gewischt:**

```text
19:16:28.570  LIFECYCLE onTaskRemoved
19:16:28.620  LIFECYCLE onDestroy
```

Der Service wurde beendet — **weil die Wiedergabe zu diesem Zeitpunkt
pausiert war**. Media3s Standard ist genau diese Unterscheidung: laufende
Wiedergabe überlebt das Wischen, pausierte nicht.

Das ist dieselbe Frage, die `RUN-6` auf dem Desktop stellt („Fensterschließen
beendet die Wiedergabe, die dieses Fenster gestartet hat"). Androids
Standardantwort deckt sich mit der Regel — P4a muss hier nichts erfinden,
nur bewusst bestätigen.

### Weiterhin offen

**Doze.** Braucht eine lange Leerlaufphase; nicht gemessen.

### Betriebsnotiz für künftige Gerätearbeit

Drahtloses adb war in diesem Netz **nicht** herstellbar. Ursache ist nicht
das VPN des Telefons — mit abgeschaltetem NordVPN blieb der Ping bei 100 %
Verlust —, sondern die **Client-Isolation des WLAN-Routers**. Zwei
WLAN-Geräte dürfen dort nicht miteinander sprechen. Für Tests, die den
USB-C-Port brauchen (Kopfhörer), heißt das: entweder ein anderes Netz oder
manuelle Beobachtung.

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

## Frage 6 — Läuft gettext auf Android? (nachgetragen 2026-08-02)

**Urteil: JA, statisch, für 24 kB je ABI.** Der Befund gehört zu Spec-Punkt
O2 und wurde beim Ausmessen von Welle 1 nötig, weil die Strings an
`crate::i18n::gettext` hängen.

Ausgangslage: `reprise-gnome` bindet `gettext-rs 0.7.7` mit
`features = ["gettext-system"]`, also gegen die **System**-libintl. Bionic hat
keine — die Annahme „Strings ziehen einfach nach `reprise-view`" trägt so
nicht.

Gemessen an einer `cdylib`-Probe gegen `aarch64-linux-android`, NDK r29,
`cargo-ndk`, `gettext-rs` mit `default-features = false`:

| Prüfung | Ergebnis |
| --- | --- |
| `gettext-sys` baut libintl aus Quellen | **OK** — `libintl.a` mit NDK-Clang, ~28 s |
| Link einer `cdylib` | **OK**, Exit 0 |
| `NEEDED` der erzeugten `.so` | nur `libdl.so`, `libc.so` — **kein externes libintl** |
| undefinierte `gettext`/`intl`-Symbole | **keine**, statisch aufgelöst |
| `libintl_gettext`, `_libintl_gettext_extract_plural`, `_nl_expand_alias` | im Binary vorhanden; Referenzprobe ohne gettext: null Treffer |
| Größe gestrippt, release | 331.664 B mit gettext gegen 307.584 B ohne → **+24.080 B je ABI** |

**Messfalle, die zweimal zuschlug:** Eine `rlib` wird nicht gelinkt, beweist
also nichts über fehlende Symbole. Und eine `cdylib` exportiert nur
`#[no_mangle] extern "C"`-Symbole — ein bloßes `pub fn` wird samt gettext
wegoptimiert, worauf beide Proben byte-identisch herauskamen. Erst der echte
C-Export macht die Messung gültig.

**Was damit noch nicht entschieden ist:** nur die Bauseite. Wie Compose und
Tauri die `po`-Kataloge konsumieren — Katalog in `reprise-view` gegen
`strings.xml`-Erzeugung aus `po` — bleibt offen. Der Unterschied ist ab jetzt
eine Werkzeug- und Übersetzer-Workflow-Frage, kein Build-Hindernis.

**Auflage für die Welle, die `strings.rs` bewegt:** `reprise-view` darf
`gettext-rs` nur ohne `gettext-system` führen, sonst bricht der Android-Build.
Das Architektur-Gate fängt das nicht ab — es verbietet `gtk4|libadwaita|glib|
gstreamer|zbus` und fremde `reprise-*`-Kanten, und `gettext-rs` ist keins von
beidem.

## Frage 0b — Läuft der Spike auf dem Emulator? (nachgetragen 2026-08-02)

**Urteil: JA — und das war vorher nicht bewiesen.** Frage 0 hat gezeigt, dass
`reprise-core` für alle drei Android-ABIs *baut*. Was danach tatsächlich lief,
war die arm64-Variante auf dem angeschlossenen Pixel. Der headless-Emulator
aus der Betriebsnotiz ist `x86_64` — der Spike trug diese ABI gar nicht
(`abiFilters += "arm64-v8a"`, eine ABI, um die APK-Größe ehrlich zu messen).
Ein Start dort wäre am `UnsatisfiedLinkError` gescheitert.

Behoben und gemessen auf `emulator-5554` (`pixel10xl_api37`, Android 17,
x86_64, headless, Software-Rendering):

| Schritt | Ergebnis |
| --- | --- |
| `cargo ndk -t x86_64 build --release -p reprise-android-ffi` | **OK**, 3 min 59 s kalt |
| `libreprise_android_ffi.so` x86_64 | 5.272.992 B (arm64 zum Vergleich: 5.508.168 B) |
| APK mit beiden ABIs | 22.730.147 B debug; `lib/x86_64/` und `lib/arm64-v8a/` beide enthalten |
| Installation und Start | **OK**, Prozess lebt, **kein `UnsatisfiedLinkError`, kein FATAL** |
| `reprise-core` im App-Sandbox | DB öffnen + migrieren **157 ms**, Scan 4 ms, Count 1 ms, Fenster 1 ms |
| Scan-Ergebnis | `added=3 errors=0`, Bibliothek hält 3 Tracks |
| UniFFI → Compose | Titelliste als **Text** im Accessibility-Baum gelesen, nicht als Pixel |

Damit ist die Kette lückenlos: Rust-Kern über NDK gebaut, per UniFFI nach
Kotlin, von Compose gerendert, auf einem Gerät, das niemand bereithalten muss.
Die 157 ms für Öffnen und Migrieren sind unter Software-Rendering gemessen und
damit eine Obergrenze, keine Zielgröße.

**Betriebsfolge:** Der Emulator ist ab jetzt ein vollwertiges Testziel für
Android-Arbeit an Reprise, nicht nur für Fremd-Apps. Wer eine ABI hinzufügt,
muss `abiFilters` mitziehen — der Build schlägt sonst nicht fehl, die App
stürzt erst beim Start ab.

## Frage 7 — Wie groß ist die Storage-Abstraktion? (nachgetragen 2026-08-02)

Die Spec sagt an dieser Stelle ausdrücklich **„Nicht geschätzt"** und nennt
die Frage, an der die Größe hängt: wie viele der Dateisystem-Stellen in
`reprise-core` müssen wirklich über `LibrarySource`, und wie viele nicht.

Vermessen wurden alle Produktivdateien mit Dateisystem-Zugriff, je Bereich von
einem Agenten eingeordnet; anschließend hat ein zweiter Durchgang jeden
**Freispruch** angegriffen — die Richtung ist bewusst so gewählt, weil beim
Schätzen das Unterzählen der teure Fehler ist.

| Urteil | Stellen |
| --- | --- |
| **must-abstract** | 21 behauptet → **27 nach der Gegenprüfung** |
| nur ein Dateihandle nötig | 6 → 3 |
| app-eigener Pfad (Cache/Config) | 3 |
| nur in Tests | 11 |

**6 von 20 Freisprüchen wurden zurückgenommen** — eine Fehlerquote von 30 % in
genau der Richtung, die zählt. Alle sechs lagen im Tag-Schreiben
(`scanner_meta.rs`, `tag_edit.rs`, `tag_mutation.rs`, `tag_mutation_guarded.rs`,
`tag_edit_write.rs`, `trash_tracks.rs`): sie galten als „arbeitet auf einem
Handle", öffnen die Nutzerdatei aber selbst per Pfad.

### Was das Paket kleiner macht

1. **Das Tag-Schreiben bündelt sich bereits in einer Naht.**
   `tag_mutation.rs` nennt `apply_tag_patch_to_file` im eigenen Doc-Kommentar
   „the sole production Lofty tag-save path". Tag-Editor, geschützter Editor
   und der Batch-Job laufen alle dort hindurch. Da `lofty` laut Spike auf
   Handles arbeitet, deckt **ein** umgestelltes Modul die gesamte
   Tag-Schreib-Oberfläche ab.
2. **Zwei Muster für den Schnitt existieren bereits im Code.**
   `trash_tracks.rs` nimmt die plattformspezifische Löschung schon als
   injizierten `Fn(&Path)`-Closure entgegen, und `device_sync/machine.rs` gibt
   einen `Effect`-Enum aus, den eine Plattformschicht ausführt. Beides sind
   funktionierende Präzedenzfälle — die Abstraktion muss nicht erfunden,
   sondern verallgemeinert werden.
3. **Die echten Caches sind app-privat.** `artist_portrait/cache.rs` und
   `remote_image` liegen im App-Verzeichnis, für das Android einen echten
   Dateisystempfad vergibt. SAF greift dort nicht, dort ändert sich nichts.
4. **Viel weniger Dateien als vermutet fassen überhaupt an.** In `concerts`
   und `radio` tut es außer `http.rs` keine einzige; in `lyrics` nur 2 von 11;
   in `queries`/`device_sync` nur 5 von 10 — der Rest manipuliert `PathBuf`
   als Zeichenkette, was nach der eigenen Regel des Crates keine
   Dateisystemstelle ist.

### Was das Paket größer macht

1. **`mounts.rs` hat auf Android kein Gegenstück — das ist der teure Befund.**
   Die Unterscheidung „Laufwerk nicht eingehängt" gegen „Datei gelöscht" steht
   vollständig auf POSIX-`st_dev` und `lstat` über die Pfad-Vorfahren. Unter
   SAF gibt es kein `/proc/mounts`, keine stabile Geräte-ID unter einem
   `content://`-Baum und kein Vorfahren-`lstat`. **Das ist kein
   Signaturwechsel.** Entweder bekommt Android einen anderen Mechanismus
   (etwa: ist die Berechtigung für diesen Tree-URI noch erteilt?), oder die
   Unterscheidung wird dort bewusst aufgegeben.
   `mounts.rs`, `scanner_mount.rs`, die Identitätsfelder in `scanner.rs` und
   `relink.rs` bilden dabei **eine zusammenhängende Einheit**, keine
   unabhängigen Stellen.
2. **Drei Bereiche haben keine Bündelung.** `walkdir::WalkDir::new(root)` läuft
   an vier Stellen unabhängig über denselben Bibliotheksbaum (`scanner.rs`,
   `scanner_progress.rs` für den Fortschrittsbalken, `relink.rs` zweimal).
   Bei den Podcasts gibt es **keinen** gemeinsamen Weg für „einen geladenen
   Download abschließen" — drei eigenständige Implementierungen. Und in
   `lyrics` leiten `local.rs` und `sidecar_write.rs` jeweils selbst
   `track_path.with_extension("lrc")` ab.
3. **Die `.exists()`-Prüfungen sind verstreut.** „Ist dieser Track noch da?"
   steht als Einzeiler in `scanner_vanish.rs`, `scanner_move.rs`, dreimal in
   `relink.rs` und in `rhythmbox_import.rs` — jeweils trivial, aber ohne
   gemeinsame Stelle.

### Urteil

**Das Paket ist zweigeteilt, und nur eine Hälfte ist Fleißarbeit.**

Die 27 Stellen zerfallen in rund 20 mechanische — `.exists()`,
`metadata()`, `walk`, Handle statt Pfad — die über das Trait der Spec
laufen, sobald es existiert, und die dank der Tag-Naht und der beiden
vorhandenen Präzedenzfälle weniger Aufwand sind, als ihre Zahl vermuten lässt.

Der Rest ist **ein einziger Entwurf**: der Ersatz für `st_dev`. Er entscheidet,
ob `MissingReason::Unmounted` auf Android eine Bedeutung hat, und er berührt
Scanner, Relink und die Mount-Klassifikation gemeinsam. Diese eine Frage sollte
beantwortet sein, bevor das Paket geschnitten wird — alles andere daran ist
absehbar.

### Korrektur zu Frage 7 (2026-08-02, nach dem Lesen von `classify_missing`)

Oben steht, `mounts.rs` habe „auf Android kein Gegenstück" und sei „ein
Neuentwurf". **Das war zu scharf**, und der Fehler ist meiner: Ich habe die
Einordnung des messenden Durchgangs übernommen, ohne den Klassifikator selbst
zu lesen. Er ist zehn Zeilen lang und bereits plattformneutral:

```rust
pub(crate) fn classify_missing(stored_device: Option<i64>, path: &Path) -> MissingReason {
    let Some(stored_device) = stored_device else { return MissingReason::Unknown };
    match nearest_existing_ancestor_dev(path) {
        Some(current) if current == stored_device as u64 => MissingReason::Deleted,
        Some(_) => MissingReason::Unmounted,
        None => MissingReason::Unknown,
    }
}
```

Die Logik fragt nicht, **was** das Merkmal ist — nur, ob es mit dem
gespeicherten übereinstimmt. Das ist wörtlich `residence_token` aus dem
Spec-Trait, und die Spec hat es richtig vorhergesehen: die Spalte wird von
„`st_dev`" zu einem generischen Aufenthaltsmerkmal umgedeutet, ohne Migration.

**Der tatsächliche Umfang, nachgezählt:**

| | |
| --- | --- |
| Unix-spezifische Produktivstellen (`MetadataExt`, `.dev()`, `.ino()`, `symlink_metadata`) | **15**, in vier Dateien |
| davon in `mounts.rs` | 9 |
| Aufrufstellen von `classify_missing` | **2** (`scanner_vanish.rs:148`, `queries/maintenance.rs:365`) |
| plattformabhängige Funktion | **eine**: `nearest_existing_ancestor_dev` |

**Unter SAF wird die Feststellung sogar einfacher, nicht schwerer.** Der
Vorfahren-Aufstieg existiert dort nicht — er muss auch nicht: die Wurzel des
gewählten Baums **ist** der feste Vorfahr. Ist sie abfragbar, steht das Volume
und eine fehlende Datei ist `Deleted`; ist sie es nicht — Berechtigung
entzogen, Karte gezogen —, dann `Unmounted`. Dieselbe Dreiteilung, ein
Vergleich weniger.

**Die Fehlerrichtung stimmt bereits.** `classify_missing` läuft nur für
Dateien, die ohnehin schon als fehlend gelten. Ein veraltetes Merkmal — unter
Linux können sich Gerätenummern über einen Neustart ändern — führt zu
`Unmounted` statt `Deleted`, also zur datenerhaltenden Antwort. Diese
Großzügigkeit trägt unverändert nach SAF.

**Was offen bleibt, ist eine andere Frage als die, die ich oben gestellt habe:**
nicht die Erreichbarkeit, sondern die **Umzugserkennung**. `scanner.rs`
erkennt eine verschobene Datei an `(device, inode)`. SAF kennt keine Inodes,
sondern Dokument-IDs, und deren Stabilität über einen Umzug hinweg garantiert
der DocumentsProvider, nicht die Plattform. Das ist der Punkt, der einen
eigenen Entwurf braucht — nicht `classify_missing`.

### Paket 1 umgesetzt: Aufenthalt und Erreichbarkeit (2026-08-02)

`LibrarySource: Send + Sync` besitzt jetzt genau die zwei zuerst benötigten
Operationen: `residence_token` und die darauf aufgebaute
Vorgabe-Implementierung `reachability`. `UnixLibrarySource` liefert weiterhin
den `st_dev` des mit `lstat` gefundenen nächsten existierenden Vorfahren; eine
Quelle ohne stabiles Merkmal liefert ehrlich `None`. Die zwei bisherigen
`classify_missing`-Aufrufe und die Root-Guard-Prüfung laufen über dieses Trait.
Der Linux-Pfad bleibt dabei identisch: der aktuelle `u64`-Gerätewert und der
gespeicherte SQLite-`i64`-Wert werden wie zuvor über dasselbe Bitmuster
verglichen. Die alten Mount-Tests bleiben grün; eine zweite Testquelle leitet
ihr Merkmal stattdessen aus einer Provider-Tree-ID ab und liefert dieselbe
Dreiteilung `Deleted` / `Unmounted` / `Unknown`.

**Gewonnene Zuführungsform: ein `&dyn LibrarySource`-Parameter.** Die heutigen
öffentlichen Scan- und Query-Grenzen behalten schmale Unix-Vorgaben, die den
Parameter an die drei betroffenen Helfer reichen. Ein Closure hätte die
zusammengehörigen Operationen `residence_token` und `reachability` wieder
auseinandergerissen; ein `Effect`-Enum hätte für drei synchrone Abfragen die
Scanner-Transaktion und die Query unnötig in einen Plattform-Rundlauf zerlegt.
Die Folgepakete reichen deshalb dieselbe Quelle als Parameter weiter, statt
pro Operation neue Closures oder Effects einzuführen.

Die ursprüngliche **27er-Messung zählte Abstraktionsstellen und Nähte, nicht
Rohaufrufe**. Damit Paket 2 nicht neu messen muss, ist der verbleibende
Live-Bestand nach Paket 1:

| Cluster | Verbleibende Stellen |
| --- | --- |
| Baum und Präsenz (Paket 2) | `library/scanner.rs` (`metadata`, Root-Präsenz, ein `WalkDir`), `scanner_progress.rs` (ein `WalkDir`), `scanner_vanish.rs`, `scanner_move.rs`, `relink.rs` (drei Präsenzprüfungen, zwei `WalkDir`), `rhythmbox_import.rs`, `queries/maintenance.rs`, `queries/issues.rs`, `device_sync/snapshot.rs`, `device_sync/podcasts.rs`, `cover.rs`, `cover_writeback.rs`, `lyrics/local.rs`, `lyrics/sidecar_write.rs`, `podcasts/download_state.rs`, `podcasts/downloads.rs`, `podcasts/ytdlp.rs` |
| Quellnahe Lese-/Schreib-Handles (Paket 3) | `library/scanner_meta.rs`, `tag_edit.rs`, `tag_mutation.rs` (weiterhin die einzige produktive Lofty-Speichernaht), `tag_mutation_guarded.rs`, `tag_edit_write.rs`, `trash_tracks.rs`, `cover.rs`, `cover_writeback.rs`, `writeback_publish.rs`, `provenance.rs`, `lyrics/local.rs`, `lyrics/sidecar_write.rs`, `podcasts/downloads.rs`, `podcasts/ytdlp.rs`, `podcasts/ytdlp_download.rs`, `podcasts/pipeline.rs`, `podcasts/episode_tags.rs` |
| Bereits vorhandene Zuführungsnaht | `queries/import_errors.rs` nimmt seine Metadata-Abfrage schon als Closure entgegen; beim Durchziehen des Parameters wird sie adaptiert, nicht neu entworfen. |
| Separater späterer Vertrag | `library/watcher.rs`: `notify` hat unter SAF kein gleichwertiges Gegenstück und bleibt die ausdrücklich optionale Watcher-Fähigkeit, nicht Teil der mechanischen Pakete 2 oder 3. |

Einige Dateien stehen bewusst in beiden ersten Zeilen: etwa `lyrics/local.rs`
prüft zuerst die Präsenz und öffnet danach dieselbe Quelle. Paket 2 ersetzt
nur die Status-/Metadaten-/Walk-Operation; Paket 3 ersetzt anschließend das
Öffnen und Publizieren über Handles. App-private Cache-, Konfigurations- und
Staging-Pfade bleiben weiterhin außerhalb von `LibrarySource`.

## Frage 8 — Die Umzugserkennung, und was Tauri daran ändert (umgesetzt 2026-08-02)

**Status: umgesetzt.** `file_stat` liefert jetzt die echte Größe getrennt von
`Option<(device, inode)>`; `MoveLookup` trägt dieselbe Identität als ein
einziges optionales Tupel. Ohne Identität entfällt nur Strategie 1, während
der Fingerabdruck aus Strategie 2 weiterläuft. Der Nicht-Unix-Zweig erfindet
deshalb kein `(0, 0)` mehr. Die bestehenden Linux-Fälle bleiben unverändert:
bei vorhandener Identität läuft weiterhin zuerst die Inode-Strategie, danach
erst der Fingerabdruck.

Die Korrektur zu Frage 7 lässt eine Frage übrig: `scanner.rs` erkennt eine
verschobene Datei an `(device, inode)`, und SAF kennt keine Inodes. Nachgelesen
ist die Lage besser **und** schlechter als erwartet.

### Es gab bereits einen zweiten Weg — er war nur verriegelt

`find_move_candidate` versucht zwei Strategien: erst `(device, inode)` für ein
`rename` auf demselben Dateisystem, dann einen **Fingerabdruck aus Titel,
Interpret, Album, Dauer und Dateigröße** für den Fall „kopiert und gelöscht",
bei dem sich der Inode ändert, Inhalt und Tags aber nicht.

Die zweite Strategie braucht nichts, was SAF fehlt: Tags und Dauer liefert
`lofty` über ein Handle, die Größe gibt der DocumentsProvider als Spalte.

Vor dem Umbau war sie aber nicht einzeln erreichbar. Die Aufrufstelle
(`scanner.rs:444`) lautete
`match (device, inode) { (Some(device), Some(inode)) => …, _ => None }` —
**fehlte die Identität, entfielen beide Strategien.** Der Kommentar darüber
begründete das, und die Begründung war für einen fehlgeschlagenen `stat`
richtig: Dann war auch `file_size` ein Platzhalter-`0`, und der Fingerabdruck
hätte gegen Müll verglichen.

**Für SAF galt diese Begründung nicht.** Dort ist die Größe echt, nur die
Identität fehlt. Die Bedingung, die das Gate motivierte, traf also nicht zu —
und damit war der Umbau klein und klar umrissen: Das damalige
`Option<(size, device, inode)>` musste sich in „Größe bekannt" und „Identität
bekannt" trennen, sodass eine Quelle mit Größe, aber ohne Identität, Strategie
2 allein benutzen kann. Die Spec hat das im Trait bereits so modelliert —
`residence_token()` gibt `Option<i64>` zurück, getrennt von allem anderen.

### Derselbe Umbau räumte eine Annahme weg, die Tauri kippte

`file_stat`s damaliger Doc-Kommentar sagte über den Nicht-Unix-Zweig:

> off Unix identity degrades to `(0, 0)` — **never reached at runtime**

Das stimmte, solange die App nur auf Linux lief. **Ein Tauri-Desktop auf
Windows hätte die Aussage falsch gemacht.** Dort hätte `stat` nicht
fehlgeschlagen, sondern `Some((size, 0, 0))` geliefert: Das Gate hätte
geöffnet, und Strategie 1 hätte mit `WHERE device = 0 AND inode = 0` jede auf
Windows gescannte Zeile getroffen.

Genau diesen Fall beschrieb derselbe Kommentar als den Fehler, den Stage 3
Task 1 beseitigt hatte („could have coincidentally matched an unrelated
`(device, inode)` of `(0, 0)`"). Der damalige Fix griff jedoch nur, wenn
`stat` **fehlschlug**. Auf Windows wäre das nicht passiert.

**Wie weit hätte der Schaden getragen?** Nicht weit, aber er wäre echt
gewesen. Der Kandidatenfilter verlangt, dass der alte Pfad verschwunden oder
die Zeile bereits als fehlend markiert ist, und bei mehreren Treffern greift
die Mehrdeutigkeitssperre („not guessing"). Falsch wäre es also erst geworden,
wenn **genau eine** solche Zeile existiert hätte — dann hätte die Historie
eines fremden Titels am neuen gehangen. Selten, still und nicht rückgängig zu
machen.

### Folge

Der gemeinsame Core-Umbau für beide Plattformen ist umgesetzt: Die bekannte
Größe öffnet die Umzugserkennung, die optionale Identität nur deren erste
Strategie. Damit

- kann ein späterer SAF-Adapter Strategie 2 ohne Identität benutzen; die
  `LibrarySource`-/Handle-Anbindung selbst ist weiterhin nicht Teil dieses
  Umbaus,
- benutzt Windows Strategie 2 statt einer Abfrage auf `(0, 0)`,
- bleibt Linux samt Strategie-Reihenfolge unverändert.

Auf Android fehlt weiterhin die `rename`-Erkennung. Dafür braucht es einen
eigenen Entwurf für den Umgang mit stabilen
DocumentsProvider-Dokument-IDs; der hier freigeschaltete Fingerabdruck erkennt
nur den vorhandenen Kopieren-und-Löschen-Fall.
