#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
bumper="$repo_root/scripts/bump-version.sh"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

fail() {
    printf 'App version routing contract failed: %s\n' "$1" >&2
    exit 1
}

gradle="$repo_root/android/app/build.gradle.kts"
grep -Eq '^[[:space:]]*versionName = "[0-9]+\.[0-9]+\.[0-9]+"$' "$gradle" || \
    fail "Android versionName must be a literal independent app version"
# The Kotlin interpolation must remain literal here.
# shellcheck disable=SC2016
grep -Fq 'buildConfigField("String", "REPRISE_CORE_VERSION", "\"${workspacePackageValue("version")}\"")' "$gradle" || \
    fail "Android must still expose the independently versioned shared Core build"

new_fixture() {
    local name=$1
    local fixture="$scratch/$name"
    mkdir -p "$fixture/scripts" "$fixture/crates/demo" "$fixture/android/app" "$fixture/showroom/src"
    cp "$bumper" "$fixture/scripts/bump-version.sh"
    chmod +x "$fixture/scripts/bump-version.sh"
    printf '%s\n' \
        '[workspace]' \
        'members = ["crates/demo"]' \
        '' \
        '[workspace.package]' \
        'version = "1.2.3"' \
        > "$fixture/Cargo.toml"
    printf '%s\n' \
        '[package]' \
        'name = "demo"' \
        'version.workspace = true' \
        > "$fixture/crates/demo/Cargo.toml"
    printf '%s\n' \
        '[[package]]' \
        'name = "demo"' \
        'version = "1.2.3"' \
        > "$fixture/Cargo.lock"
    printf '%s\n' \
        'android {' \
        '    defaultConfig {' \
        '        versionCode = 41' \
        '        versionName = "2.4.6"' \
        '    }' \
        '}' \
        > "$fixture/android/app/build.gradle.kts"
    printf '# Reprise\n' > "$fixture/README.md"
    printf 'export const page = true;\n' > "$fixture/showroom/src/page.ts"
    git -C "$fixture" init --quiet --initial-branch=test
    git -C "$fixture" config user.name 'Version routing test'
    git -C "$fixture" config user.email 'version-routing@example.invalid'
    git -C "$fixture" add .
    git -C "$fixture" commit --quiet -m base
    printf '%s\n' "$fixture"
}

read_desktop_version() {
    sed -n '/^\[workspace\.package\]$/,/^\[/s/^version = "\([^"]*\)"$/\1/p' "$1/Cargo.toml"
}

read_android_version() {
    sed -n 's/^[[:space:]]*versionName = "\([^"]*\)"$/\1/p' "$1/android/app/build.gradle.kts"
}

read_android_code() {
    sed -n 's/^[[:space:]]*versionCode = \([0-9]*\)$/\1/p' "$1/android/app/build.gradle.kts"
}

fixture=$(new_fixture docs-only)
printf '\nMore documentation.\n' >> "$fixture/README.md"
printf '\nexport const deployed = true;\n' >> "$fixture/showroom/src/page.ts"
mkdir -p "$fixture/.github/workflows"
printf 'name: Pages\n' > "$fixture/.github/workflows/pages.yml"
base=$(git -C "$fixture" rev-parse HEAD)
git -C "$fixture" add README.md showroom/src/page.ts .github/workflows/pages.yml
git -C "$fixture" commit --quiet -m 'docs: update site'
output=$(cd "$fixture" && ./scripts/bump-version.sh --base "$base")
[[ $output == none ]] || fail "README and Showroom changes must report no app bump, got: $output"
[[ $(read_desktop_version "$fixture") == 1.2.3 ]] || fail "README and Showroom changed the desktop version"
[[ $(read_android_version "$fixture") == 2.4.6 ]] || fail "README and Showroom changed the Android version"
[[ $(read_android_code "$fixture") == 41 ]] || fail "README and Showroom changed Android versionCode"

fixture=$(new_fixture desktop-only)
mkdir -p "$fixture/crates/reprise-gnome/src"
printf 'fn desktop_change() {}\n' > "$fixture/crates/reprise-gnome/src/lib.rs"
base=$(git -C "$fixture" rev-parse HEAD)
git -C "$fixture" add crates/reprise-gnome/src/lib.rs
git -C "$fixture" commit --quiet -m 'fix(gnome): adjust desktop'
output=$(cd "$fixture" && ./scripts/bump-version.sh --base "$base")
[[ $output == 'desktop 1.2.4' ]] || fail "desktop-only changes reported the wrong bump: $output"
[[ $(read_desktop_version "$fixture") == 1.2.4 ]] || fail "desktop-only changes did not bump the desktop version"
[[ $(read_android_version "$fixture") == 2.4.6 ]] || fail "desktop-only changes changed the Android version"
[[ $(read_android_code "$fixture") == 41 ]] || fail "desktop-only changes changed Android versionCode"
git -C "$fixture" add Cargo.toml Cargo.lock
git -C "$fixture" commit --quiet -m 'chore: bump version to desktop 1.2.4'
output=$(cd "$fixture" && ./scripts/bump-version.sh --base "$base")
[[ $output == 'desktop 1.2.4' ]] || fail "a desktop bump retry changed its scope: $output"
[[ $(read_android_version "$fixture") == 2.4.6 ]] || fail "a desktop bump retry changed the Android version"
[[ $(read_android_code "$fixture") == 41 ]] || fail "a desktop bump retry changed Android versionCode"

fixture=$(new_fixture android-only)
mkdir -p "$fixture/android/app/src/main"
printf 'class AndroidChange\n' > "$fixture/android/app/src/main/AndroidChange.kt"
base=$(git -C "$fixture" rev-parse HEAD)
git -C "$fixture" add android/app/src/main/AndroidChange.kt
git -C "$fixture" commit --quiet -m 'fix(android): adjust mobile'
output=$(cd "$fixture" && ./scripts/bump-version.sh --base "$base")
[[ $output == 'android 2.4.7' ]] || fail "Android-only changes reported the wrong bump: $output"
[[ $(read_desktop_version "$fixture") == 1.2.3 ]] || fail "Android-only changes changed the desktop version"
[[ $(read_android_version "$fixture") == 2.4.7 ]] || fail "Android-only changes did not bump the Android version"
[[ $(read_android_code "$fixture") == 42 ]] || fail "Android-only changes did not bump Android versionCode"

fixture=$(new_fixture core-shared)
mkdir -p "$fixture/crates/reprise-core/src"
printf 'pub fn shared_change() {}\n' > "$fixture/crates/reprise-core/src/lib.rs"
base=$(git -C "$fixture" rev-parse HEAD)
git -C "$fixture" add crates/reprise-core/src/lib.rs
git -C "$fixture" commit --quiet -m 'fix(core): adjust shared behavior'
output=$(cd "$fixture" && ./scripts/bump-version.sh --base "$base")
[[ $output == 'desktop 1.2.4, android 2.4.7' ]] || fail "Core changes reported the wrong bumps: $output"
[[ $(read_desktop_version "$fixture") == 1.2.4 ]] || fail "Core changes did not bump the desktop version"
[[ $(read_android_version "$fixture") == 2.4.7 ]] || fail "Core changes did not bump the Android version"
[[ $(read_android_code "$fixture") == 42 ]] || fail "Core changes did not bump Android versionCode"

fixture=$(new_fixture tooling-only)
printf '#!/usr/bin/env bash\ntrue\n' > "$fixture/scripts/tooling.sh"
base=$(git -C "$fixture" rev-parse HEAD)
git -C "$fixture" add scripts/tooling.sh
git -C "$fixture" commit --quiet -m 'ci: adjust tooling'
output=$(cd "$fixture" && ./scripts/bump-version.sh --base "$base")
[[ $output == none ]] || fail "tooling-only changes must report no app bump, got: $output"
[[ $(read_desktop_version "$fixture") == 1.2.3 ]] || fail "tooling-only changes changed the desktop version"
[[ $(read_android_version "$fixture") == 2.4.6 ]] || fail "tooling-only changes changed the Android version"
[[ $(read_android_code "$fixture") == 41 ]] || fail "tooling-only changes changed Android versionCode"

fixture=$(new_fixture desktop-packaging)
printf 'app-id: io.github.marvinbaudach.Reprise\n' > \
    "$fixture/io.github.marvinbaudach.Reprise.yml"
base=$(git -C "$fixture" rev-parse HEAD)
git -C "$fixture" add io.github.marvinbaudach.Reprise.yml
git -C "$fixture" commit --quiet -m 'build(flatpak): adjust the desktop package'
output=$(cd "$fixture" && ./scripts/bump-version.sh --base "$base")
[[ $output == 'desktop 1.2.4' ]] || fail "desktop packaging changes reported the wrong bump: $output"
[[ $(read_desktop_version "$fixture") == 1.2.4 ]] || fail "desktop packaging changes did not bump the desktop version"
[[ $(read_android_version "$fixture") == 2.4.6 ]] || fail "desktop packaging changes changed the Android version"
[[ $(read_android_code "$fixture") == 41 ]] || fail "desktop packaging changes changed Android versionCode"

fixture=$(new_fixture coupled-base-migration)
sed -i 's/versionName = "2.4.6"/versionName = workspacePackageValue("version")/' \
    "$fixture/android/app/build.gradle.kts"
git -C "$fixture" add android/app/build.gradle.kts
git -C "$fixture" commit --quiet --amend --no-edit
base=$(git -C "$fixture" rev-parse HEAD)
sed -i 's/versionName = workspacePackageValue("version")/versionName = "1.2.3"/' \
    "$fixture/android/app/build.gradle.kts"
git -C "$fixture" add android/app/build.gradle.kts
git -C "$fixture" commit --quiet -m 'build(android): separate the mobile version'
output=$(cd "$fixture" && ./scripts/bump-version.sh --base "$base")
[[ $output == 'android 1.2.4' ]] || fail "the one-time coupled base migration reported the wrong bump: $output"
[[ $(read_desktop_version "$fixture") == 1.2.3 ]] || fail "the Android version migration changed the desktop version"
[[ $(read_android_version "$fixture") == 1.2.4 ]] || fail "the Android version migration did not bump from the coupled base"
[[ $(read_android_code "$fixture") == 42 ]] || fail "the Android version migration did not bump versionCode"

echo "App version routing contracts passed"
