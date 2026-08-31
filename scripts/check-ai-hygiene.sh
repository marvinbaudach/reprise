#!/usr/bin/env bash
# GP-19/GP-20: the four shapes the GNOME Circle committee names as rejection
# reasons for AI-generated submissions.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/lib/rulebook.sh

src=crates

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

# GP-19a — comments phrased as instructions to a model.
prompt_like='^\s*//+\s*(Step [0-9]|First,|Now (we|I)|Let'"'"'s |You (should|must|can)|Note that you|As an AI|I will now|Here'"'"'s )'
prompt_hit=':[0-9]+:[[:space:]]*//+[[:space:]]*(Step [0-9]|First,|Now (we|I)|Let'"'"'s |You (should|must|can)|Note that you|As an AI|I will now|Here'"'"'s )'
n=$({ production_rust_lines "$src" | cut -d: -f3- | grep -E "$prompt_like" || true; } | wc -l)
(( n == 0 )) || report_violation GP-19 "$n comment(s) read as model instructions:
$(production_rust_lines "$src" | { grep -E "$prompt_hit" || true; } | sed -n '1,10p')"

# GP-19b — banner comment blocks.
banner='^\s*//\s*[=-]{10,}\s*$'
n=$({ production_rust_lines "$src" | cut -d: -f3- | grep -E "$banner" || true; } | wc -l)
(( n == 0 )) || report_violation GP-19 "$n banner comment block(s):
$(production_rust_lines "$src" | { grep -E ':[0-9]+:[[:space:]]*//+' || true; } \
  | { grep -E ':[[:space:]]*//[[:space:]]*[=-]{10,}[[:space:]]*$' || true; } | sed -n '1,10p')"

# GP-19c — decorative emoji in comments. The listed characters document real
# UI symbols and are not decorative prose.
emoji='^\s*//.*(?:(?![★☆✓✕⏏🗑✦])[\x{1F300}-\x{1FAFF}\x{2600}-\x{27BF}])'
n=$({ production_rust_lines "$src" | cut -d: -f3- | grep -Pc "$emoji" || true; } \
  | awk '{s+=$1} END {print s+0}')
(( n == 0 )) || report_violation GP-19 "$n comment(s) contain emoji:
$(production_rust_lines "$src" | { grep -P ':[0-9]+:\s*//.*(?:(?![★☆✓✕⏏🗑✦])[\x{1F300}-\x{1FAFF}\x{2600}-\x{27BF}])' || true; } \
  | sed -n '1,10p')"

# GP-20 — dead-code allowances without a stated reason. A reason is a comment
# on the same line or on the line directly above.
allows=$(production_rust_lines "$src" | grep -F '#[allow(dead_code)]' || true)
unexplained=0
while IFS= read -r hit; do
  [[ -n $hit ]] || continue
  file=${hit%%:*}
  rest=${hit#*:}
  line=${rest%%:*}
  same_line=${hit#*:*:}
  above=""
  if (( line > 1 )); then
    above=$(sed -n "$((line - 1))p" "$file")
  fi
  if [[ $same_line != *"//"* && $above != *"//"* ]]; then
    unexplained=$((unexplained + 1))
    printf '  %s:%s\n' "$file" "$line" >&2
  fi
done <<< "$allows"
(( unexplained == 0 )) || report_violation GP-20 \
  "$unexplained #[allow(dead_code)] without a stated reason (listed above)"

rulebook_exit
