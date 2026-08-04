"""Find RefCell borrows that provably outlive a block.

AGENTS.md calls a Ref held across a re-entrant call "the #1 recurring panic
class". Most of the 1,633 borrows in the GTK crate are harmless: a borrow in
its own statement drops at the semicolon. The dangerous shapes are the ones
where the borrow is a *temporary in a scrutinee*, because Rust keeps those
alive until the end of the whole statement — body included:

    if let Some(x) = cell.borrow().get(k) { ...body holds the Ref... }
    match cell.borrow().state { ...arms hold the Ref... }
    for item in cell.borrow().iter() { ...loop body holds the Ref... }
    while let Some(x) = cell.borrow_mut().pop() { ...body holds it... }

Edition 2021 (this workspace) keeps `if let` temporaries alive through the
else block too; edition 2024 shortened that.

Explicitly-bound guards (`let g = cell.borrow_mut();`) are reported separately:
those are legal and often correct, but they are the other way a borrow reaches
a call.

Test code is skipped: a panic in a test is a failing test, not a field crash.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path("crates/reprise-gnome/src")
BORROW = re.compile(r"\.borrow(_mut)?\(\)")
SCRUTINEE = re.compile(r"^\s*(?:\}\s*)?(if let|while let|match|for)\b.*")
LET_GUARD = re.compile(r"^\s*let\s+(?:mut\s+)?(\w+)\s*=\s*[^;]*\.borrow(_mut)?\(\)\s*;")


def production_lines(path: Path):
    """Yield (lineno, text) skipping #[cfg(test)] blocks and comment-only lines."""
    skipping = False
    for n, raw in enumerate(path.read_text(errors="ignore").split("\n"), 1):
        if raw.startswith("#[cfg(test)]"):
            skipping = True
            continue
        if skipping:
            if raw.startswith("}"):
                skipping = False
            continue
        stripped = raw.strip()
        if stripped.startswith(("//", "/*", "*")):
            continue
        yield n, raw


def main():
    scrutinee_hits, guard_hits = [], []
    for path in sorted(ROOT.rglob("*.rs")):
        if path.name.endswith("_tests.rs") or path.name == "test_db.rs":
            continue
        lines = list(production_lines(path))
        for i, (n, text) in enumerate(lines):
            if not BORROW.search(text):
                continue
            m = SCRUTINEE.match(text)
            if m and text.rstrip().endswith("{"):
                # The construct opens a block on this line, so the borrow is a
                # scrutinee temporary and the block body is inside its scope.
                scrutinee_hits.append((path, n, m.group(1), text.strip()))
            elif LET_GUARD.match(text):
                guard_hits.append((path, n, text.strip()))

    rel = str
    print(f"=== scrutinee borrows: Ref alive through the block body ({len(scrutinee_hits)}) ===")
    for p, n, kind, text in scrutinee_hits:
        print(f"{rel(p)}:{n}  [{kind}]")
        print(f"    {text[:110]}")
    print(f"\n=== explicitly bound guards ({len(guard_hits)}) ===")
    by_file = {}
    for p, n, _ in guard_hits:
        by_file.setdefault(rel(p), []).append(n)
    for f, ns in sorted(by_file.items(), key=lambda kv: -len(kv[1]))[:12]:
        print(f"  {len(ns):3d}  {f}")


if __name__ == "__main__":
    main()
