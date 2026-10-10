"""A named observation the agent keeps for the maintainer; it is never a verdict."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping


@dataclass(frozen=True)
class Note:
    code: str
    summary: str
    evidence: Mapping[str, Any]
