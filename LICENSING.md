# Licensing

Reprise uses a deliberate **open-core** split: the engine stays freely reusable, the native
Linux app is copyleft, and future commercial ports remain possible.

| Component | Crate(s) / location | License |
|---|---|---|
| Engine (portable, GUI-free) | `crates/reprise-core`, `crates/reprise-platform-linux` | **MIT** |
| Headless surfaces & stem backend (portable, GUI-free) | `crates/reprise-cli`, `crates/reprise-mcp`, `crates/reprise-stems` | **MIT** |
| Native Linux app (GTK4/libadwaita) | `crates/reprise-gnome` | **GPL-3.0-or-later** |
| Future macOS / Windows / mobile frontends | separate, private repositories | **Proprietary / All Rights Reserved** |

The headless CLI (`reprise-cli`), the local MCP server (`reprise-mcp`), and the
stem-separation backend (`reprise-stems`) are **MIT** by decision 9 of the
multi-frontend-core plan: they are first-party frontends and backends over the
MIT engine and must stay redistributable outside the GPL GTK app, exactly like
`reprise-core`.

## Why this split
- The **engine is MIT (permissive)** so *any* frontend may link it — including the author's
  future proprietary macOS/Windows/mobile apps, and free **and** paid mobile editions. A GPL
  engine would legally forbid those.
- The **Linux GUI is GPL-3.0-or-later** so the open reference client stays copyleft: nobody can
  ship a closed-source fork of the Linux app, while the MIT engine underneath stays reusable.
- The **distributed Linux binary** (`reprise`) links the MIT engine into the GPL frontend, so
  per GPL terms the shipped binary is GPL-3.0-or-later, with the MIT components' notices preserved.

## App Store note
Because the mobile/desktop commercial frontends are **proprietary + MIT core** (never GPL), the
classic "GPL app rejected by the Apple App Store" problem does **not** apply to them. Keep the
MIT notice in the app's acknowledgements/licenses screen.

## License texts
- MIT — [`LICENSE`](LICENSE) — applies to `reprise-core`, `reprise-platform-linux`,
  `reprise-cli`, `reprise-mcp`, and `reprise-stems`.
- GPL-3.0-or-later — [`crates/reprise-gnome/LICENSE`](crates/reprise-gnome/LICENSE).

## Third-party SDK note — rmcp (Apache-2.0)
`reprise-mcp` depends on the official MCP Rust SDK `rmcp` (and `rmcp-macros`),
which is **Apache-2.0**. Apache-2.0 is a permissive license, compatible with an
MIT crate: the distributed `reprise-mcp` binary combines our MIT code with the
Apache-2.0 SDK and other permissive dependencies, all of which allow
redistribution and commercial use. Apache-2.0 also composes one-way with
GPL-3.0-or-later, so it raises no conflict for the wider workspace either.
Apache-2.0 requires preserving the upstream `NOTICE` and attribution, so keep
the `rmcp` license and notice in `reprise-mcp`'s acknowledgements. The SDK is
pinned exactly (`=2.2.0`) and guarded by JSON-RPC fixtures, so a version bump is
a deliberate, reviewed commit that re-checks this note.

## Third-party dependency note — symphonia (MPL-2.0)
`reprise-stems`, behind its `ort` feature, decodes arbitrary library formats
with the pure-Rust `symphonia` codecs, which are **MPL-2.0**. MPL-2.0 is a
**weak, file-level copyleft**: the copyleft attaches only to the MPL-covered
source files themselves, so merely depending on `symphonia` — linking it,
unmodified, into a larger work — places **no copyleft obligation on our own MIT
or GPL code**. It permits redistribution and commercial use, is FSF-listed
**GPL-compatible**, and links freely into a larger work, so it satisfies the
audio-analysis/stem-separation gate below for both the GPL Linux client and the
future proprietary frontends. The only obligation is the ordinary one: if we
ever modify `symphonia`'s own files, those modified files stay MPL-2.0 and their
source must be made available — which the first-use, unmodified dependency does
not trigger. Keep the `symphonia` license notice in `reprise-stems`'
acknowledgements. The remaining `ort`-feature codecs and runtime plumbing are
permissive (`flacenc` Apache-2.0; `rubato`, `ndarray`, `ort` itself MIT or
Apache-2.0), so they raise no additional copyleft; the `ort`/htdemucs runtime and
weights are cleared separately in the stem-separation section below.

## Ownership / contributions
Marvin Baudach is the sole copyright holder and may relicense his own code. **Before accepting
external contributions, add a CLA or DCO** so the relicensing option (and the proprietary ports)
stay legally clean.

## Audio-analysis and stem-separation dependencies and models

Local audio analysis is part of the MIT engine path. Dependencies, bundled
models, and generated model artifacts used there must permit redistribution,
commercial use, and linking from both the GPL Linux client and future
proprietary frontends. AGPL libraries and assets with Non-Commercial or
No-Derivatives terms must not be linked into or distributed with
`reprise-core` or `reprise-platform-linux`.

Every future semantic audio model requires a documented license and provenance
review before it enters the repository. Research comparisons do not authorize
shipping the compared library or model.

The same gate governs stem separation (`crates/reprise-stems`, the experimental
instrumental feature). The ML runtime chosen by the package E spike (candle or
ort) and — critically — the **model weights** must permit redistribution,
commercial use, and linking from both the GPL Linux client and future
proprietary frontends. This license clearance is a **precondition**: the
weights license is verified against this gate during the package E spike,
**before** any productive stem-separation code (package G) ships. If a
candidate model's weights fail the gate, the feature is blocked — it is never
shipped "somehow" under an incompatible license. Weights are **not bundled**
into the default build or the Flatpak (Flathub size and license exposure);
they arrive through a first-use download that verifies a checksum and records
the model's license notice next to the downloaded file, mirroring the existing
cover-download module.
