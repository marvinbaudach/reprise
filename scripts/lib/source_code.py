"""Conservative comment removal for source-scanning repository gates."""

from pathlib import Path
import re


_RAW_STRING_START = re.compile(r'(?<![A-Za-z0-9_])(?:br|r)(?P<hashes>#{0,255})"')
_QUOTE_CHAR = "'\"'"
_TRIPLE_QUOTE = '"""'


def _unmodelled_closer(line: str, block_depth: int) -> str | None:
    """Return an opaque construct's closer when this line must stay whole."""
    visible = line
    if block_depth:
        block_end = visible.find("*/")
        if block_end == -1:
            return None
        visible = visible[block_end + 2 :]

    if visible.lstrip().startswith("//"):
        return None

    triple_start = visible.find(_TRIPLE_QUOTE)
    raw_start = _RAW_STRING_START.search(visible)
    char_start = visible.find(_QUOTE_CHAR)
    starts = [
        position
        for position in (
            triple_start,
            raw_start.start() if raw_start else -1,
            char_start,
        )
        if position >= 0
    ]
    if not starts:
        return None

    first = min(starts)
    if first == triple_start:
        if visible.find(_TRIPLE_QUOTE, triple_start + 3) == -1:
            return _TRIPLE_QUOTE
        return ""
    if raw_start and first == raw_start.start():
        closer = '"' + raw_start.group("hashes")
        if visible.find(closer, raw_start.end()) == -1:
            return closer
        return ""
    return ""


def code_of(path: str | Path) -> str:
    """Read source with only comments that can be identified safely removed."""
    try:
        text = Path(path).read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return ""

    code = []
    block_depth = 0
    in_string = False
    escaped = False
    opaque_closer: str | None = None

    for line in text.splitlines(keepends=True):
        if opaque_closer is not None:
            code.append(line)
            if opaque_closer in line:
                opaque_closer = None
            continue

        unmodelled_closer = _unmodelled_closer(line, block_depth)
        if unmodelled_closer is not None:
            code.append(line)
            opaque_closer = unmodelled_closer or None
            continue

        index = 0
        while index < len(line):
            current = line[index]
            following = line[index + 1] if index + 1 < len(line) else ""

            if block_depth:
                if current == "/" and following == "*":
                    block_depth += 1
                    index += 2
                elif current == "*" and following == "/":
                    block_depth -= 1
                    index += 2
                else:
                    if current == "\n":
                        code.append(current)
                    index += 1
                continue

            if in_string:
                code.append(current)
                if escaped:
                    escaped = False
                elif current == "\\":
                    escaped = True
                elif current == '"':
                    in_string = False
                index += 1
                continue

            if current == '"':
                in_string = True
                code.append(current)
                index += 1
            elif current == "/" and following == "/":
                if line.endswith("\n"):
                    code.append("\n")
                break
            elif current == "/" and following == "*":
                block_depth = 1
                index += 2
            else:
                code.append(current)
                index += 1

    return "".join(code)
