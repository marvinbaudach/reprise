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
            .github/* | .superpowers/* | docs/* | scripts/* | showroom/* | \
                AGENTS.md | CODE_OF_CONDUCT.md | CONTEXT.md | LICENSING.md | \
                README* | RELEASING.md | reprise.doap | .editorconfig | .gitignore)
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
    --owner-skip)
        if (( $# != 5 )); then
            echo "usage: $0 --owner-skip EVENT PR_AUTHOR REPOSITORY_OWNER COMMIT_MESSAGE" >&2
            exit 64
        fi
        event=$2
        pr_author=$3
        repository_owner=$4
        commit_message=$5
        if [[ $event == pull_request && -n $repository_owner \
            && $pr_author == "$repository_owner" \
            && $commit_message == *'[owner skip ci]'* ]]; then
            echo true
        else
            echo false
        fi
        ;;
    *)
        echo "usage: $0 --paths [PATH ...] | --diff EVENT BASE_SHA HEAD_SHA | --owner-skip EVENT PR_AUTHOR REPOSITORY_OWNER COMMIT_MESSAGE" >&2
        exit 64
        ;;
esac
