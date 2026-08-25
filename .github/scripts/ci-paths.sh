#!/usr/bin/env bash
set -euo pipefail

emit_routes() {
    local android=false
    local gnome=false
    local core=false
    local path

    if (( $# == 0 )); then
        printf 'android=true\ngnome=false\ncore=true\n'
        return
    fi

    for path in "$@"; do
        case "$path" in
            android/* | crates/reprise-android-ffi/*)
                android=true
                ;;
            crates/reprise-core/* | Cargo.toml | Cargo.lock)
                android=true
                core=true
                ;;
            crates/reprise-view/*)
                android=true
                gnome=true
                ;;
            crates/reprise-gnome/* | crates/reprise-platform-linux/* | \
                assets/* | data/* | flatpak/* | po/* | meson.build)
                gnome=true
                ;;
            crates/*)
                core=true
                ;;
            .github/* | .superpowers/* | docs/* | quality/* | scripts/* | showroom/* | \
                AGENTS.md | CODE_OF_CONDUCT.md | CONTEXT.md | LICENSING.md | \
                README* | RELEASING.md | reprise.doap | .editorconfig | .gitignore)
                ;;
            .markdownlint-cli2.jsonc | .yamllint.yaml | ruff.toml)
                ;;
            *)
                # An unclassified product path is expensive, but skipping a
                # dependent surface silently is worse. New roots fail closed.
                android=true
                core=true
                ;;
        esac
    done

    printf 'android=%s\ngnome=%s\ncore=%s\n' "$android" "$gnome" "$core"
}

case "${1:-}" in
    --paths)
        shift
        emit_routes "$@"
        ;;
    --diff)
        if (( $# != 4 )); then
            echo "usage: $0 --diff EVENT BASE_SHA HEAD_SHA" >&2
            exit 64
        fi
        event=$2
        base_sha=$3
        head_sha=$4
        if [[ $event == workflow_dispatch || -z $base_sha || $base_sha =~ ^0+$ ]] || \
            ! git cat-file -e "$base_sha^{commit}" 2>/dev/null || \
            ! git cat-file -e "$head_sha^{commit}" 2>/dev/null; then
            emit_routes
            exit 0
        fi
        mapfile -d '' -t changed_paths < <(
            git diff --name-only --diff-filter=ACDMRTUXB -z "$base_sha" "$head_sha"
        )
        emit_routes "${changed_paths[@]}"
        ;;
    --suite-skip)
        if (( $# != 7 )); then
            echo "usage: $0 --suite-skip EVENT REF ACTOR REPOSITORY_OWNER HEAD_SHA DEV_SHA" >&2
            exit 64
        fi
        event=$2
        ref=$3
        actor=$4
        repository_owner=$5
        head_sha=$6
        dev_sha=$7
        # A pull request skips the expensive suites so review stays cheap;
        # the real verification happens on the push to dev. Dependabot is the
        # one author that never reaches that push consciously: its pull
        # requests merge themselves as soon as the required check turns green,
        # so for it the pull request IS the only opportunity to test the diff.
        if [[ $event == pull_request && $actor != "dependabot[bot]" ]]; then
            echo true
        elif [[ $event == push && $ref == refs/heads/main \
            && -n $repository_owner && $actor == "$repository_owner" \
            && -n $dev_sha && $head_sha == "$dev_sha" ]]; then
            echo true
        else
            echo false
        fi
        ;;
    *)
        echo "usage: $0 --paths [PATH ...] | --diff EVENT BASE_SHA HEAD_SHA | --suite-skip EVENT REF ACTOR REPOSITORY_OWNER HEAD_SHA DEV_SHA" >&2
        exit 64
        ;;
esac
