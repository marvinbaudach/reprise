# Licensing

Reprise is **GPL-3.0-or-later** throughout — engine, headless surfaces and the native Linux
app alike. The whole workspace inherits the license from `[workspace.package]`, so every
crate carries the same terms and no component can drift.

| Component | Crate(s) / location | License |
|---|---|---|
| Engine (portable, GUI-free) | `crates/reprise-core`, `crates/reprise-platform-linux` | **GPL-3.0-or-later** |
| Headless surfaces & stem backend | `crates/reprise-cli`, `crates/reprise-mcp`, `crates/reprise-stems` | **GPL-3.0-or-later** |
| Native Linux app (GTK4/libadwaita) | `crates/reprise-gnome` | **GPL-3.0-or-later** |
| Runtime, view and FFI crates | `crates/reprise-runtime*`, `crates/reprise-view`, `crates/reprise-android-ffi` | **GPL-3.0-or-later** |

## Why copyleft everywhere
The reference client and everything it stands on stay free: nobody can ship a closed-source
fork, and anyone who redistributes a modified Reprise passes the same rights along. The
distributed binary is GPL-3.0-or-later, with every third-party notice preserved.

This replaces the earlier open-core split — an MIT engine under a GPL frontend, kept
permissive so a proprietary port stayed possible. That option was deliberately given up on
2026-08-11, before the repository became public.

## License text
- GPL-3.0-or-later — [`LICENSE`](LICENSE), applying to the entire workspace.

## Third-party SDK note — rmcp (Apache-2.0)
`reprise-mcp` depends on the official MCP Rust SDK `rmcp` (and `rmcp-macros`), which is
**Apache-2.0**. Apache-2.0 composes one-way with GPL-3.0-or-later: the combined work is
GPL-3.0-or-later and raises no conflict. Apache-2.0 requires preserving the upstream
`NOTICE` and attribution, so keep the `rmcp` license and notice in `reprise-mcp`'s
acknowledgements. The SDK is pinned exactly and guarded by JSON-RPC fixtures, so a version
bump is a deliberate, reviewed commit that re-checks this note.

## Third-party dependency note — symphonia (MPL-2.0)
`reprise-stems`, behind its `ort` feature, decodes arbitrary library formats with the
pure-Rust `symphonia` codecs, which are **MPL-2.0**. MPL-2.0 is a **weak, file-level
copyleft**: the copyleft attaches only to the MPL-covered source files themselves, so
depending on `symphonia` — linking it, unmodified, into a larger work — places no additional
obligation on our own code. It is FSF-listed **GPL-compatible** and links freely into a
larger work. The only obligation is the ordinary one: if we ever modify `symphonia`'s own
files, those modified files stay MPL-2.0 and their source must be made available — which the
unmodified dependency does not trigger. Keep the `symphonia` license notice in
`reprise-stems`' acknowledgements. The remaining `ort`-feature codecs and runtime plumbing
are permissive (`flacenc` Apache-2.0; `rubato` — and the `audioadapter`,
`audioadapter-buffers` and `audioadapter-sample` crates it carries its buffer API on since
4.0 — plus `ndarray` and `ort` itself, all MIT or Apache-2.0), so they raise no additional
copyleft; the `ort`/htdemucs runtime and weights are cleared separately in the
stem-separation section below.

## Third-party DSP note — CAVA (MIT)

`reprise-core` contains an idiomatic Rust port of the frequency-band planning and temporal
signal-processing formulas from CAVA's `cavacore`. CAVA is MIT-licensed, which composes into
a GPL-3.0-or-later work with its notice preserved. Reprise does not copy CAVA's FFTW
integration, audio backends, threading, or renderers. The upstream copyright and permission
notice is preserved in [`LICENSES/CAVA-MIT.txt`](LICENSES/CAVA-MIT.txt).

## Third-party icon note — Phosphor (MIT)

The showroom's screenshot plates carry a zoom cue whose path data is the
`ArrowsOutSimple` icon (regular weight) from Phosphor Icons, taken verbatim from
`phosphor-icons/core` and inlined into
`showroom/src/components/showcase/ShotTile.tsx`. Phosphor is MIT-licensed, which
composes into a GPL-3.0-or-later work with its notice preserved. No other
Phosphor asset is used, and the icon set is not a dependency. The upstream
copyright and permission notice is preserved in
[`LICENSES/PHOSPHOR-MIT.txt`](LICENSES/PHOSPHOR-MIT.txt).

## Ownership / contributions
Marvin Baudach is the sole copyright holder and may relicense his own code — but only for as
long as that stays true. **Before accepting external contributions, add a CLA or DCO**, or
the license becomes fixed by every contributor who has not signed one.

## Audio-analysis and stem-separation dependencies and models

Dependencies, bundled models, and generated model artifacts must permit redistribution,
commercial use, and linking into a GPL-3.0-or-later work. Assets with Non-Commercial or
No-Derivatives terms must not be linked into or distributed with any crate — those terms are
incompatible with the freedoms the GPL passes on to every recipient.

Every future semantic audio model requires a documented license and provenance review before
it enters the repository. Research comparisons do not authorize shipping the compared library
or model.

The same gate governs stem separation (`crates/reprise-stems`, the experimental instrumental
feature). The ML runtime chosen by the package E spike (candle or ort) and — critically — the
**model weights** must clear the same terms. This license clearance is a **precondition**:
the weights license is verified against this gate during the package E spike, **before** any
productive stem-separation code (package G) ships. If a candidate model's weights fail the
gate, the feature is blocked — it is never shipped "somehow" under an incompatible license.
Weights are **not bundled** into the default build or the Flatpak (size and license
exposure); they arrive through a first-use download that verifies a checksum and records the
model's license notice next to the downloaded file, mirroring the existing cover-download
module.
