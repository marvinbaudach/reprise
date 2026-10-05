#!/usr/bin/env bash
set -euo pipefail

emit_routes() {
    local android=false
    local gnome=false
    local core=false
    local display=false
    local path

    if (( $# == 0 )); then
        printf 'android=true\ngnome=false\ncore=true\ndisplay=true\n'
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

    if [[ $gnome == true || $core == true ]]; then
        display=true
    fi

    # The core suite's workspace gate already tests the GNOME crate, so a
    # change that routes core does not run the GNOME suite on top of it.
    if [[ $core == true ]]; then
        gnome=false
    fi

    printf 'android=%s\ngnome=%s\ncore=%s\ndisplay=%s\n' \
        "$android" "$gnome" "$core" "$display"
}

case "${1:-}" in
    --paths)
        shift
        emit_routes "$@"
        ;;
    --diff)
        if (( $# != 5 )); then
            echo "usage: $0 --diff EVENT REF BASE_SHA HEAD_SHA" >&2
            exit 64
        fi
        event=$2
        ref=$3
        base_sha=$4
        head_sha=$5
        if [[ $event == schedule || $event == push && $ref == refs/heads/main ]]; then
            printf 'android=true\ngnome=false\ncore=true\ndisplay=true\n'
            exit 0
        fi
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
    --contain)
        if (( $# != 4 )); then
            echo "usage: $0 --contain EVENT ACTOR SOURCES_STATUS" >&2
            exit 64
        fi
        event=$2
        actor=$3
        sources_status=$4
        # A Dependabot bump whose Flatpak sources are stale would burn the suites
        # on a pull request that is red anyway, and nothing a human reads. It
        # keeps base-contracts, where the same check fails, and loses the suites.
        # This is NOT suite reuse: reuse turns the Quality gate green, and an
        # auto-merge armed pull request would merge with broken sources.
        if [[ $event == pull_request && $actor == "dependabot[bot]" \
            && $sources_status != 0 ]]; then
            echo true
        else
            echo false
        fi
        ;;
    *)
        echo "usage: $0 --paths [PATH ...] | --diff EVENT REF BASE_SHA HEAD_SHA | --suite-skip EVENT REF ACTOR REPOSITORY_OWNER HEAD_SHA DEV_SHA | --contain EVENT ACTOR SOURCES_STATUS" >&2
        exit 64
        ;;
esac
