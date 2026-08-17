#!/usr/bin/env bash
set -euo pipefail

if (( $# != 9 )); then
    echo "usage: $0 CHANGES_RESULT BASE_RESULT OWNER_SKIP ANDROID_ROUTE ANDROID_RESULT GNOME_ROUTE GNOME_RESULT CORE_ROUTE CORE_RESULT" >&2
    exit 64
fi

changes_result=$1
base_result=$2
owner_skip=$3
android_route=$4
android_result=$5
gnome_route=$6
gnome_result=$7
core_route=$8
core_result=$9

[[ $changes_result == success ]] || {
    echo "changed-path routing did not succeed: $changes_result" >&2
    exit 1
}
case "$owner_skip" in
    true)
        [[ $base_result == skipped ]] || {
            echo "owner skip requested but base contracts were $base_result" >&2
            exit 1
        }
        for route in "$android_route" "$gnome_route" "$core_route"; do
            [[ $route == false ]] || {
                echo "owner skip requested but a route was still selected: $route" >&2
                exit 1
            }
        done
        for result in "$android_result" "$gnome_result" "$core_result"; do
            [[ $result == skipped ]] || {
                echo "owner skip requested but a routed suite was $result" >&2
                exit 1
            }
        done
        echo "CI suites explicitly skipped by the repository owner"
        exit 0
        ;;
    false)
        [[ $base_result == success ]] || {
            echo "base and contract checks did not succeed: $base_result" >&2
            exit 1
        }
        ;;
    *)
        echo "owner skip produced an invalid value: $owner_skip" >&2
        exit 1
        ;;
esac

require_route_result() {
    local surface=$1
    local route=$2
    local result=$3
    case "$route" in
        true)
            [[ $result == success ]] || {
                echo "$surface was selected but its suite result was $result" >&2
                return 1
            }
            ;;
        false)
            [[ $result == skipped ]] || {
                echo "$surface was not selected but its suite result was $result" >&2
                return 1
            }
            ;;
        *)
            echo "$surface produced an invalid route: $route" >&2
            return 1
            ;;
    esac
}

require_route_result Android "$android_route" "$android_result"
require_route_result GNOME "$gnome_route" "$gnome_result"
require_route_result Core "$core_route" "$core_result"
echo "Every selected CI gate succeeded"
