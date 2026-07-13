# Licensing

Reprise uses a deliberate **open-core** split: the engine stays freely reusable, the native
Linux app is copyleft, and future commercial ports remain possible.

| Component | Crate(s) / location | License |
|---|---|---|
| Engine (portable, GUI-free) | `crates/reprise-core`, `crates/reprise-platform-linux` | **MIT** |
| Native Linux app (GTK4/libadwaita) | `crates/reprise-gnome` | **GPL-3.0-or-later** |
| Future macOS / Windows / mobile frontends | separate, private repositories | **Proprietary / All Rights Reserved** |

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
- MIT — [`LICENSE`](LICENSE) — applies to `reprise-core` and `reprise-platform-linux`.
- GPL-3.0-or-later — [`crates/reprise-gnome/LICENSE`](crates/reprise-gnome/LICENSE).

## Ownership / contributions
Marvin Baudach is the sole copyright holder and may relicense his own code. **Before accepting
external contributions, add a CLA or DCO** so the relicensing option (and the proprietary ports)
stay legally clean.
