"""Whole-line comment filtering for source-scanning repository gates."""

from pathlib import Path


def code_of(path: str | Path) -> str:
    """Read source with whole-line comments removed and every other line intact."""
    try:
        text = Path(path).read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return ""

    return "".join(
        line
        for line in text.splitlines(keepends=True)
        if not line.lstrip().startswith(("//", "/*", "*"))
    )
