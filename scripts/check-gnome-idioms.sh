#!/usr/bin/env bash
# GP-2/GP-3/GP-4/FB-11: gtk4-rs idioms in the GTK frontend.
#
# This gate greps. It is a tripwire, not a proof: it catches the shapes that
# reviewers reject, and it reports counts so a rule can be switched to
# [active] once the count reaches zero.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/lib/rulebook.sh

ui=crates/reprise-gnome/src
[[ -d $ui ]] || { echo "ERROR: $ui does not exist" >&2; exit 1; }

production_rust_lines() {
  python3 - "$1" <<'PY'
import os
import re
import sys

root = sys.argv[1]
cfg_test = re.compile(r'^\s*#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]')
quoted = re.compile(r'r\#*".*?"\#*|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'')
module = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z0-9_]+)\s*;')
path_attr = re.compile(r'^\s*#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]')

all_files = [
    os.path.join(directory, name)
    for directory, _, files in os.walk(root)
    for name in files
    if name.endswith(".rs")
]
test_modules = set()
for path in all_files:
    lines = open(path, encoding="utf-8").readlines()
    for index, line in enumerate(lines):
        if not cfg_test.match(line):
            continue
        explicit_path = None
        for declaration in lines[index + 1:index + 6]:
            if match := path_attr.match(declaration):
                explicit_path = match.group(1)
                continue
            if match := module.match(declaration):
                if explicit_path:
                    test_modules.add(os.path.normpath(os.path.join(os.path.dirname(path), explicit_path)))
                else:
                    base = os.path.dirname(path)
                    if os.path.basename(path) != "mod.rs":
                        base = os.path.join(base, os.path.splitext(os.path.basename(path))[0])
                    name = match.group(1)
                    for candidate in (os.path.join(base, f"{name}.rs"), os.path.join(base, name, "mod.rs")):
                        if os.path.exists(candidate):
                            test_modules.add(os.path.normpath(candidate))
                break

for directory, dirs, files in os.walk(root):
    dirs[:] = [name for name in dirs if name not in {"tests", "examples"}]
    for name in sorted(files):
        if (not name.endswith(".rs") or name == "tests.rs" or
                name.endswith(("_tests.rs", "_fixture.rs"))):
            continue
        path = os.path.join(directory, name)
        if os.path.normpath(path) in test_modules:
            continue
        skipping = False
        depth = None
        with open(path, encoding="utf-8") as source:
            for number, line in enumerate(source, 1):
                if not skipping and cfg_test.match(line):
                    skipping = True
                    depth = None
                    continue
                if skipping:
                    code = quoted.sub("", line.split("//", 1)[0])
                    if depth is None:
                        if ";" in code and "{" not in code:
                            skipping = False
                            continue
                        if "{" not in code:
                            continue
                        depth = code.count("{") - code.count("}")
                    else:
                        depth += code.count("{") - code.count("}")
                    if depth <= 0:
                        skipping = False
                    continue
                print(f"{path}:{number}:{line}", end="")
PY
}

count_matches() {
  { production_rust_lines "$ui" | grep -E "$1" || true; } \
    | { grep -vcE '^[^:]+:[0-9]+:[[:space:]]*//' || true; }
}
list_matches() {
  { production_rust_lines "$ui" | grep -E "$1" || true; } \
    | { grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true; } | sed -n '1,10p'
}

# FB-11 — every toast title is plain text. Direct construction silently keeps
# libadwaita's markup default and can discard messages containing &, < or >.
toast_construction_pattern='(^|[^[:alnum:]_])Toast::(new|builder)|(^|[^[:alnum:]_])Toast[[:space:]]+as[[:space:]]+[[:alnum:]_]+|(^|[^[:alnum:]_])Toast(::)?[[:space:]]*$'
toast_construction=$({
  production_rust_lines "$ui" | grep -E "$toast_construction_pattern" || true
} | { grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true; } \
  | { grep -v "^$ui/ui/toasts\.rs:" || true; })
[[ -z $toast_construction ]] || report_violation FB-11 \
  "direct toast construction leaves plain text in the default markup slot; use crate::ui::toasts::plain:
$toast_construction"

# GP-2 — blocking calls that must not sit on the main loop.
blocking='(std::thread::sleep|\.blocking_recv\(\)|\.blocking_send\(|block_on\()'
n=$(count_matches "$blocking")
(( n == 0 )) || report_violation GP-2 "$n blocking call(s) in $ui:
$(list_matches "$blocking")"

# GP-3 — explicit #[strong] captures. The rulebook documents the grep limit.
n=$({ production_rust_lines "$ui" | grep -A2 'clone!(' || true; } \
  | { grep -E '#\[strong\]' || true; } | wc -l)
(( n == 0 )) || report_violation GP-3 "$n clone! block(s) capture strongly:
$(production_rust_lines "$ui" | { grep -A2 'clone!(' || true; } \
  | { grep -E '#\[strong\]' || true; } | sed -n '1,10p')"

# GP-4 — unwrap() in the frontend.
n=$(count_matches '\.unwrap\(\)')
(( n == 0 )) || report_violation GP-4 "$n unwrap() call(s) in $ui:
$(list_matches '\.unwrap\(\)')"

rulebook_exit
