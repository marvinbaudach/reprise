"""Read the accessibility nodes cua-driver lists but does not index.

`get_window_state` indexes only nodes a client can act on. Everything else the
tree holds - a dialog's title, the two halves of a filter chip, a column header -
appears only in `tree_markdown`, on lines that read

    - label = "Edit 512 Tracks"
    - button = "Save" (disabled)
    - column header = "Title"

An indexed node reads `- [12] button "Save 512" [actions=[click]]`, with its
index in brackets and no equals sign, so the two never share a shape. The audits
need the unindexed ones: the tag dialog title is the only place Reprise states
how many tracks an edit applies to, and a filter chip is two adjacent labels.
"""

from __future__ import annotations

import re

UNINDEXED_NODE = re.compile(
    r'^\s*-\s+(?P<role>[^\[\]"=]+?)\s=\s"(?P<label>.*)"(?:\s\([a-z, ]+\))?\s*$'
)


def unindexed_nodes(tree_markdown: object) -> tuple[tuple[str, str], ...]:
    """Role and label of every unindexed node, in tree (pre-order) order."""
    if not isinstance(tree_markdown, str):
        return ()
    found = []
    for line in tree_markdown.splitlines():
        match = UNINDEXED_NODE.match(line)
        if match is not None and match.group("label"):
            found.append((match.group("role").strip().casefold(), match.group("label")))
    return tuple(found)


def unindexed_labels(tree_markdown: object) -> tuple[str, ...]:
    """The labels alone; adjacent entries keep their tree order."""
    return tuple(label for _role, label in unindexed_nodes(tree_markdown))
