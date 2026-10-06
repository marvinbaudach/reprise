"""How a mission reaches a sidebar section that has no entry of its own name.

`Playlists` is a heading above the playlists, and a heading is not operable: the
only accessible handle into the section is a playlist. A hover-sweep workload
therefore maps such a section to a fixture token, whose value is the accessible
name of the entry that opens it. Every other section is reached by its own name.
"""

from __future__ import annotations

from typing import Any, Mapping


def section_handle(
    workload: Mapping[str, Any],
    fixture_tokens: Mapping[str, str],
    section: str,
) -> str:
    """The accessible name that opens `section`."""
    token = workload.get("section_handles", {}).get(section)
    return str(fixture_tokens[token]) if token is not None else section
