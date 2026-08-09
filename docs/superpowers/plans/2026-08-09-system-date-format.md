---
slug: system-date-format
worktree: /home/marvin/Projects/reprise-system-date-format
branch: feature/system-date-format
phase: planned
codex_session:
created: 2026-08-09
---
# One System Date Format Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace all fourteen hand-written date formats with one renderer driven by the system locale, so every displayed date uses a numeric month and an always four-digit year.

**Architecture:** `reprise-core` gains a `DatePattern` value — a numeric-only strftime subset (`%d`, `%m`, `%Y` plus literals) — and renders dates from it, including partial dates that drop fields. `reprise-gnome` reads the pattern once from the C library's locale (`nl_langinfo(D_FMT)`), upgrades a two-digit year to four digits, and falls back to ISO when the locale's pattern is not purely numeric. Every call site then formats through that one value.

**Tech Stack:** Rust, `libc` (new direct dependency of `reprise-gnome` only), existing `chrono` for calendar arithmetic at the call sites, GTK4/libadwaita for the display tests.

Source spec: `docs/superpowers/specs/2026-08-09-table-columns-and-system-dates-design.md` (Part B).

## Global Constraints

- Anchor work against `origin/dev`, not the local checkout — it runs hours behind. Branch from `origin/dev`.
- `reprise-core` must never link `gtk4`, `libadwaita`, `glib`, `gstreamer` or `zbus`. `scripts/check-architecture.sh` enforces it.
- `libc` may become a direct dependency of `reprise-gnome` **only**. It must not appear in `reprise-core`, `reprise-cli`, `reprise-mcp` or `reprise-stems`.
- Every Rust file stays below 800 lines. `window.rs`, `track_list.rs` and `sidebar.rs` stay below 600.
- `scripts/check-frontend-thinness.sh` treats `view_floor` (currently 1782) as ceiling **and** floor. This plan does not add to `reprise-view`; if a step does, raise the floor in the same commit.
- The year is always rendered four digits. This is the whole point of the change — never reintroduce `%y`.
- Machine-readable strings are out of scope and must not change: `%Y-%m` API query keys in `lastfm_stats.rs` and `listenbrainz.rs`, `to_rfc3339` in `concerts_view.rs`, the `%Y%m%d-%H%M%S` debug-dump filename in `row_loss_watchdog.rs`, and every `%Y-%m-%d` that parses an API payload.
- Relative phrasings that name an interval rather than a day stay as they are: `new_releases_updated_ago`, `concerts_updated_ago`.
- Run display tests singly, not as a batch: the display suite is herd-flaky and a batch run reports different failures each time. `xvfb-run -a cargo test -p reprise-gnome <name> -- --ignored --exact --test-threads=1`.
- Check the number before `passed` in every `cargo test` result line. A filter that matches nothing still prints `ok`.

---

### Task 1: The date pattern renderer in core

**Files:**
- Modify: `crates/reprise-core/src/format.rs` (append; `format_unix_timestamp` at line 107 is rewritten in Task 3)

**Interfaces:**
- Consumes: nothing.
- Produces: `pub struct DatePattern`, `DatePattern::ISO: &'static str`, `DatePattern::from_platform(raw: &str) -> DatePattern`, `DatePattern::render(&self, year: Option<i32>, month: Option<u32>, day: Option<u32>) -> String`.

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block at the bottom of `crates/reprise-core/src/format.rs`:

```rust
    #[test]
    fn date_pattern_renders_the_day_first_convention() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "29.05.2026");
        assert_eq!(pattern.render(Some(2026), Some(5), None), "05.2026");
        assert_eq!(pattern.render(Some(2026), None, None), "2026");
        assert_eq!(pattern.render(None, Some(8), Some(15)), "15.08");
    }

    #[test]
    fn date_pattern_renders_the_month_first_convention() {
        let pattern = DatePattern::from_platform("%m/%d/%Y");
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "05/29/2026");
        assert_eq!(pattern.render(Some(2026), Some(5), None), "05/2026");
        assert_eq!(pattern.render(None, Some(8), Some(15)), "08/15");
    }

    #[test]
    fn date_pattern_keeps_non_ascii_unit_markers_when_fields_drop() {
        let pattern = DatePattern::from_platform("%Y年%m月%d日");
        assert_eq!(
            pattern.render(Some(2026), Some(5), Some(29)),
            "2026年05月29日"
        );
        assert_eq!(pattern.render(Some(2026), Some(5), None), "2026年05月");
        assert_eq!(pattern.render(Some(2026), None, None), "2026年");
    }

    #[test]
    fn date_pattern_reproduces_a_complete_date_verbatim() {
        // Hungarian ends a full date with a period; a complete render must
        // not trim it. Only an omitted field licenses trailing trimming.
        let pattern = DatePattern::from_platform("%Y. %m. %d.");
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "2026. 05. 29.");
        assert_eq!(pattern.render(Some(2026), Some(5), None), "2026. 05");
    }

    #[test]
    fn date_pattern_upgrades_a_two_digit_year() {
        // glibc hands out %m/%d/%y for en_US. Four digits, always.
        let pattern = DatePattern::from_platform("%m/%d/%y");
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "05/29/2026");
    }

    #[test]
    fn date_pattern_falls_back_to_iso_for_a_non_numeric_pattern() {
        for raw in ["%a, %b %-d, %Y", "%A %d %B %Y", "", "%d.%m", "nonsense"] {
            assert_eq!(
                DatePattern::from_platform(raw).render(Some(2026), Some(5), Some(29)),
                "2026-05-29",
                "pattern {raw:?} should have fallen back to ISO"
            );
        }
    }

    #[test]
    fn date_pattern_ignores_a_day_without_a_month() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(pattern.render(Some(2026), None, Some(29)), "2026");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p reprise-core date_pattern -- --exact-nothing 2>/dev/null; cargo test -p reprise-core date_pattern`
Expected: FAIL — `cannot find type DatePattern in this scope`.

- [ ] **Step 3: Implement the renderer**

Append to `crates/reprise-core/src/format.rs`, above the `mod tests` block:

```rust
/// The three numeric fields a locale date pattern may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateField {
    Day,
    Month,
    Year,
}

/// A locale date pattern reduced to what Reprise is willing to render: the
/// three numeric fields and the literals between them.
///
/// Reprise takes the *order and punctuation* from the system and nothing
/// else. A locale that spells the month (`%b`, `%B`) or names the weekday
/// (`%a`, `%A`) is not rendered in its own shape — the whole pattern falls
/// back to ISO — because a month name is exactly what this change exists to
/// remove. A two-digit year (`%y`, which glibc still hands out for `en_US`)
/// is upgraded rather than rejected: the field is right, only its width is
/// wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatePattern {
    /// Literal text before the first field. Usually empty.
    prefix: String,
    /// Each field with the literal run that follows it.
    fields: Vec<(DateField, String)>,
}

impl DatePattern {
    /// The fallback whenever the platform pattern cannot be rendered
    /// numerically. Unambiguous in every locale and already the shape the
    /// library's "Added" column has always used.
    pub const ISO: &'static str = "%Y-%m-%d";

    /// Reduces a platform strftime pattern to a [`DatePattern`], falling back
    /// to [`Self::ISO`] when it carries anything this renderer will not
    /// print.
    pub fn from_platform(raw: &str) -> Self {
        Self::parse(raw).unwrap_or_else(|| {
            Self::parse(Self::ISO).expect("the ISO pattern is renderable by construction")
        })
    }

    fn parse(raw: &str) -> Option<Self> {
        let mut prefix = String::new();
        let mut fields: Vec<(DateField, String)> = Vec::new();
        let mut chars = raw.chars().peekable();

        while let Some(character) = chars.next() {
            if character != '%' {
                push_literal(&mut prefix, &mut fields, character);
                continue;
            }
            // Skip the padding and locale modifiers glibc allows between the
            // percent and the conversion character (`%-d`, `%_d`, `%0e`,
            // `%Ey`), so a padded field is still recognised as its field.
            let mut conversion = chars.next()?;
            while matches!(conversion, '-' | '_' | '0' | '^' | '#' | 'E' | 'O') {
                conversion = chars.next()?;
            }
            let field = match conversion {
                'd' | 'e' => DateField::Day,
                'm' => DateField::Month,
                'Y' | 'y' => DateField::Year,
                '%' => {
                    push_literal(&mut prefix, &mut fields, '%');
                    continue;
                }
                // Month names, weekday names, day-of-year, compound
                // conversions — anything else means this locale's shape is
                // not one Reprise renders.
                _ => return None,
            };
            if fields.iter().any(|(seen, _)| *seen == field) {
                return None; // a repeated field is not a date pattern
            }
            fields.push((field, String::new()));
        }

        // All three fields must be present; a pattern missing one is not a
        // full date and would silently drop information.
        let complete = [DateField::Day, DateField::Month, DateField::Year]
            .iter()
            .all(|field| fields.iter().any(|(seen, _)| seen == field));
        complete.then_some(Self { prefix, fields })
    }

    /// Renders the date, omitting absent fields together with the literal run
    /// that follows them.
    ///
    /// A day without a month is not a date anyone can read, so the day is
    /// dropped in that case. When any field is omitted, a trailing run of
    /// ASCII punctuation or whitespace is trimmed — a dangling `/` or `.`
    /// reads as truncation. Non-ASCII trailing text (the CJK unit markers) is
    /// kept, because there it carries the meaning of the field. A complete
    /// date is reproduced verbatim, trailing punctuation included.
    pub fn render(&self, year: Option<i32>, month: Option<u32>, day: Option<u32>) -> String {
        let day = month.and(day);
        let omitted = year.is_none() || month.is_none() || day.is_none();

        let mut out = self.prefix.clone();
        for (field, suffix) in &self.fields {
            let value = match field {
                DateField::Day => day.map(|day| format!("{day:02}")),
                DateField::Month => month.map(|month| format!("{month:02}")),
                DateField::Year => year.map(|year| format!("{year:04}")),
            };
            if let Some(value) = value {
                out.push_str(&value);
                out.push_str(suffix);
            }
        }

        if omitted {
            let trimmed = out.trim_end_matches(|character: char| {
                character.is_ascii_punctuation() || character.is_whitespace()
            });
            return trimmed.trim_start().to_owned();
        }
        out
    }
}

/// Appends a literal character to whichever run is currently open: the
/// prefix while no field has been seen, otherwise the suffix of the last one.
fn push_literal(prefix: &mut String, fields: &mut [(DateField, String)], character: char) {
    match fields.last_mut() {
        Some((_, suffix)) => suffix.push(character),
        None => prefix.push(character),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p reprise-core date_pattern`
Expected: PASS, `7 passed`. Confirm the count — a filter that matches nothing also prints `ok`.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/format.rs
git commit -m "feat: render dates from a numeric locale pattern"
```

---

### Task 2: The clock convention in core

**Files:**
- Modify: `crates/reprise-core/src/format.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `pub enum ClockConvention { Hours24, Hours12 }`, `ClockConvention::from_platform(t_fmt: &str) -> ClockConvention`, `ClockConvention::render(self, hour: i64, minute: i64) -> String`.

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `crates/reprise-core/src/format.rs`:

```rust
    #[test]
    fn clock_convention_reads_twelve_hours_from_the_locale() {
        for raw in ["%I:%M:%S %p", "%r", "%l:%M %P"] {
            assert_eq!(
                ClockConvention::from_platform(raw),
                ClockConvention::Hours12,
                "pattern {raw:?} is a twelve-hour locale"
            );
        }
        for raw in ["%H:%M:%S", "%T", ""] {
            assert_eq!(
                ClockConvention::from_platform(raw),
                ClockConvention::Hours24,
                "pattern {raw:?} is a twenty-four-hour locale"
            );
        }
    }

    #[test]
    fn clock_convention_renders_minutes_and_never_seconds() {
        assert_eq!(ClockConvention::Hours24.render(14, 3), "14:03");
        assert_eq!(ClockConvention::Hours24.render(0, 0), "00:00");
        assert_eq!(ClockConvention::Hours12.render(14, 3), "2:03 PM");
        assert_eq!(ClockConvention::Hours12.render(0, 5), "12:05 AM");
        assert_eq!(ClockConvention::Hours12.render(12, 0), "12:00 PM");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p reprise-core clock_convention`
Expected: FAIL — `cannot find type ClockConvention in this scope`.

- [ ] **Step 3: Implement**

Append to `crates/reprise-core/src/format.rs`, above `mod tests`:

```rust
/// Whether the locale writes the time on a twelve- or twenty-four-hour dial.
///
/// Reprise takes only that choice from the system and never the locale's full
/// time pattern: `T_FMT` carries seconds in most locales, and a second-precise
/// timestamp in a table cell is noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockConvention {
    Hours24,
    Hours12,
}

impl ClockConvention {
    /// Derives the dial from a platform time pattern. Any twelve-hour
    /// conversion — the hour itself (`%I`, `%l`), the meridiem (`%p`, `%P`)
    /// or the compound twelve-hour time (`%r`) — makes it twelve.
    pub fn from_platform(t_fmt: &str) -> Self {
        let twelve = ["%I", "%l", "%p", "%P", "%r"]
            .iter()
            .any(|marker| t_fmt.contains(marker));
        if twelve {
            Self::Hours12
        } else {
            Self::Hours24
        }
    }

    /// Renders hour and minute. Seconds are never shown.
    pub fn render(self, hour: i64, minute: i64) -> String {
        match self {
            Self::Hours24 => format!("{hour:02}:{minute:02}"),
            Self::Hours12 => {
                let meridiem = if hour < 12 { "AM" } else { "PM" };
                let hour = match hour % 12 {
                    0 => 12,
                    other => other,
                };
                format!("{hour}:{minute:02} {meridiem}")
            }
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p reprise-core clock_convention`
Expected: PASS, `2 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/format.rs
git commit -m "feat: take the twelve/twenty-four-hour dial from the locale"
```

---

### Task 3: `format_unix_timestamp` takes the format

**Files:**
- Modify: `crates/reprise-core/src/format.rs:107-115`
- Modify: `crates/reprise-gnome/src/ui/track_list/column_layout.rs:590`
- Modify: `crates/reprise-gnome/src/ui/track_list/queue_item_presentation.rs:44`
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_columns.rs:74`

**Interfaces:**
- Consumes: `DatePattern` (Task 1), `ClockConvention` (Task 2).
- Produces: `pub struct DateTimeFormat { pub date: DatePattern, pub clock: ClockConvention }` and `pub fn format_unix_timestamp(secs: i64, format: &DateTimeFormat) -> String`. The three GTK call sites are rewired in Task 5 once `date_format::current()` exists; until then they pass `&DateTimeFormat::iso()`.

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `crates/reprise-core/src/format.rs`:

```rust
    #[test]
    fn unix_timestamp_follows_the_supplied_format() {
        let german = DateTimeFormat {
            date: DatePattern::from_platform("%d.%m.%Y"),
            clock: ClockConvention::Hours24,
        };
        assert_eq!(
            format_unix_timestamp(1_000_000_000, &german),
            "09.09.2001 01:46"
        );
        let american = DateTimeFormat {
            date: DatePattern::from_platform("%m/%d/%y"),
            clock: ClockConvention::Hours12,
        };
        assert_eq!(
            format_unix_timestamp(1_000_000_000, &american),
            "09/09/2001 1:46 AM"
        );
    }
```

The three existing tests in that file (`unix_timestamp_formats_the_epoch`,
`unix_timestamp_formats_a_well_known_value`,
`unix_timestamp_clamps_negative_input_to_the_epoch`) keep their expected
strings but now pass `&DateTimeFormat::iso()`:

```rust
        assert_eq!(
            format_unix_timestamp(0, &DateTimeFormat::iso()),
            "1970-01-01 00:00"
        );
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p reprise-core unix_timestamp`
Expected: FAIL — `cannot find type DateTimeFormat in this scope`.

- [ ] **Step 3: Implement**

Replace `crates/reprise-core/src/format.rs:107-115` with:

```rust
/// A complete display format: how this system writes a date, and on which
/// dial it writes the time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTimeFormat {
    pub date: DatePattern,
    pub clock: ClockConvention,
}

impl DateTimeFormat {
    /// The locale-independent fallback, used before a frontend has supplied
    /// the platform's own and by tests that assert an exact string.
    pub fn iso() -> Self {
        Self {
            date: DatePattern::from_platform(DatePattern::ISO),
            clock: ClockConvention::Hours24,
        }
    }
}

pub fn format_unix_timestamp(secs: i64, format: &DateTimeFormat) -> String {
    let secs = secs.max(0);
    let days = secs / 86_400;
    let time_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let date = format
        .date
        .render(Some(year), Some(month as u32), Some(day as u32));
    format!("{date} {}", format.clock.render(hour, minute))
}
```

Check the actual integer types `civil_from_days` returns before writing the
casts; if it already yields `u32` for month and day, drop the `as u32`.

- [ ] **Step 4: Fix the three GTK call sites to compile**

In each of the three files, pass the ISO fallback for now — Task 5 replaces it
with the live format:

```rust
reprise_core::format::format_unix_timestamp(
    track.added_at,
    &reprise_core::format::DateTimeFormat::iso(),
)
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p reprise-core format:: && cargo build -p reprise-gnome`
Expected: PASS and a clean build.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-core/src/format.rs crates/reprise-gnome/src/ui/track_list/
git commit -m "refactor: format_unix_timestamp takes an explicit display format"
```

---

### Task 4: Read the platform's locale in the frontend

**Files:**
- Create: `crates/reprise-gnome/src/ui/date_format.rs`
- Modify: `crates/reprise-gnome/src/ui/mod.rs` (declare the module beside its siblings)
- Modify: `crates/reprise-gnome/Cargo.toml` (add `libc`)
- Modify: `crates/reprise-gnome/src/main.rs` (warm the cache right after `i18n::init`)

**Interfaces:**
- Consumes: `DateTimeFormat`, `DatePattern`, `ClockConvention` (Tasks 1–3).
- Produces: `pub(in crate::ui) fn current() -> &'static DateTimeFormat` and `pub(in crate::ui) const PATTERN_ENV: &str = "REPRISE_DATE_PATTERN"`.

- [ ] **Step 1: Add the dependency**

In `crates/reprise-gnome/Cargo.toml`, under `[dependencies]`:

```toml
libc = "0.2"
```

- [ ] **Step 2: Write the failing test**

Create `crates/reprise-gnome/src/ui/date_format.rs` with only its test module
first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// STYLE-11: the environment override exists so a display test can pin a
    /// locale shape without mutating the process locale, which `setlocale`
    /// makes global and racy across the test harness.
    #[test]
    fn style_11_environment_override_wins_over_the_platform() {
        let pattern = pattern_from(Some("%d.%m.%Y".to_owned()), || "%m/%d/%y".to_owned());
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "29.05.2026");
    }

    #[test]
    fn style_11_platform_pattern_is_used_when_no_override_is_set() {
        let pattern = pattern_from(None, || "%m/%d/%y".to_owned());
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "05/29/2026");
    }

    #[test]
    fn style_11_unreadable_platform_pattern_falls_back_to_iso() {
        let pattern = pattern_from(None, String::new);
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "2026-05-29");
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p reprise-gnome style_11_`
Expected: FAIL — `cannot find function pattern_from`.

- [ ] **Step 4: Implement**

Put this above the test module in `crates/reprise-gnome/src/ui/date_format.rs`:

```rust
//! The one place that decides how Reprise writes a date.
//!
//! `i18n::init` calls `setlocale(LC_ALL, "")` at startup, so the C library
//! already knows the user's `LC_TIME` — nobody was reading it. This module
//! reads it exactly once and hands every call site the same value, which is
//! what makes STYLE-11 enforceable rather than aspirational.

use std::sync::OnceLock;

use reprise_core::format::{ClockConvention, DatePattern, DateTimeFormat};

/// Pins the date pattern for tests and screenshots. Changing the process
/// locale would be the honest alternative, but `setlocale` is global and the
/// test harness runs cases concurrently.
pub(in crate::ui) const PATTERN_ENV: &str = "REPRISE_DATE_PATTERN";

static FORMAT: OnceLock<DateTimeFormat> = OnceLock::new();

/// The display format for this process. Cheap after the first call.
pub(in crate::ui) fn current() -> &'static DateTimeFormat {
    FORMAT.get_or_init(|| DateTimeFormat {
        date: pattern_from(std::env::var(PATTERN_ENV).ok(), platform_date_pattern),
        clock: ClockConvention::from_platform(&platform_time_pattern()),
    })
}

/// Warms the cache. Call once, directly after `i18n::init`, so the pattern is
/// read after `setlocale` and never from a half-initialised locale.
pub(in crate::ui) fn init() {
    let format = current();
    tracing::info!(
        date = ?format.date,
        clock = ?format.clock,
        "date display format resolved"
    );
}

fn pattern_from(override_value: Option<String>, platform: impl FnOnce() -> String) -> DatePattern {
    let raw = override_value.unwrap_or_else(platform);
    DatePattern::from_platform(&raw)
}

#[cfg(unix)]
fn platform_date_pattern() -> String {
    langinfo(libc::D_FMT)
}

#[cfg(unix)]
fn platform_time_pattern() -> String {
    langinfo(libc::T_FMT)
}

#[cfg(not(unix))]
fn platform_date_pattern() -> String {
    DatePattern::ISO.to_owned()
}

#[cfg(not(unix))]
fn platform_time_pattern() -> String {
    "%H:%M".to_owned()
}

#[cfg(unix)]
fn langinfo(item: libc::nl_item) -> String {
    // SAFETY: `nl_langinfo` returns a pointer to a NUL-terminated string owned
    // by the C library, valid until the next `setlocale` on this thread. The
    // bytes are copied out immediately and the pointer is never retained, and
    // the only `setlocale` in this process runs once in `i18n::init` before
    // this is ever called.
    unsafe {
        let raw = libc::nl_langinfo(item);
        if raw.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned()
    }
}
```

Declare it in `crates/reprise-gnome/src/ui/mod.rs` alongside the other `ui`
modules (the existing `mod` list around line 199 shows the house style), and
call `crate::ui::date_format::init();` in `main.rs` on the line after
`i18n::init(...)`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p reprise-gnome style_11_`
Expected: PASS, `3 passed`.

- [ ] **Step 6: Verify the dependency boundary**

Run: `bash scripts/check-architecture.sh`
Expected: passes. `libc` must not appear in the default trees of
`reprise-cli`, `reprise-mcp` or `reprise-stems` as a *workspace* edge — it is
an ordinary third-party crate and the gate only bans the GTK/GStreamer/zbus
families, so this should be green. If it fails, read the failing probe before
changing anything.

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/Cargo.toml crates/reprise-gnome/src/ui/date_format.rs crates/reprise-gnome/src/ui/mod.rs crates/reprise-gnome/src/main.rs Cargo.lock
git commit -m "feat: read the system date pattern once at startup"
```

---

### Task 5: Releases — table and Updates popover

**Files:**
- Modify: `crates/reprise-gnome/src/ui/releases/releases_presentation.rs:68-79` and its tests at 186-193
- Modify: `crates/reprise-gnome/src/ui/updates/release_row.rs:82-93`, `:167`, and its tests at 250-282
- Modify: `crates/reprise-gnome/src/ui/track_list/column_layout.rs:590`
- Modify: `crates/reprise-gnome/src/ui/track_list/queue_item_presentation.rs:44`
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_columns.rs:74`

**Interfaces:**
- Consumes: `date_format::current()` (Task 4), `DatePattern::render` (Task 1).
- Produces: `pub(in crate::ui) fn format_partial_date(raw: &str, pattern: &DatePattern) -> String` in `releases_presentation.rs`, re-used by `release_row.rs`. Handles MusicBrainz's three precisions (`YYYY-MM-DD`, `YYYY-MM`, `YYYY`) and returns the raw string unchanged when it parses as none of them.

- [ ] **Step 1: Write the failing tests**

Replace the existing `format_release_date_preserves_musicbrainz_precision`
test in `releases_presentation.rs` with:

```rust
    /// STYLE-11: one pattern, all three MusicBrainz precisions, four-digit
    /// year throughout — the reported case where this very column wrote both
    /// `26` and `2026`.
    #[test]
    fn style_11_release_date_keeps_precision_within_one_pattern() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(format_partial_date("2026-05-29", &pattern), "29.05.2026");
        assert_eq!(format_partial_date("2026-05", &pattern), "05.2026");
        assert_eq!(format_partial_date("2026", &pattern), "2026");
        assert_eq!(format_partial_date("unknown", &pattern), "unknown");
        assert_eq!(format_partial_date("2026-13-40", &pattern), "2026-13-40");
    }
```

Replace all five `format_release_date_*` tests in `release_row.rs` with:

```rust
    /// STYLE-11: the popover used to drop the year inside the current year
    /// and write it two-digit otherwise. Both are gone; it renders exactly
    /// what the table renders.
    #[test]
    fn style_11_popover_release_date_matches_the_table() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(format_partial_date("2026-08-15", &pattern), "15.08.2026");
        assert_eq!(format_partial_date("2025-08-15", &pattern), "15.08.2025");
        assert_eq!(format_partial_date("2026-08", &pattern), "08.2026");
        assert_eq!(format_partial_date("2026", &pattern), "2026");
        assert_eq!(format_partial_date("tba", &pattern), "tba");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p reprise-gnome style_11_release style_11_popover`
Expected: FAIL — `cannot find function format_partial_date`.

- [ ] **Step 3: Implement the shared function**

Replace `format_release_date` in `releases_presentation.rs:68-79` with:

```rust
/// Renders a MusicBrainz date string at whatever precision it carries, in the
/// system pattern. MusicBrainz supplies `YYYY-MM-DD`, `YYYY-MM` or `YYYY`;
/// anything else is passed through untouched rather than guessed at.
pub(in crate::ui) fn format_partial_date(raw: &str, pattern: &DatePattern) -> String {
    let mut parts = raw.split('-');
    let Some(year) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
        return raw.to_owned();
    };
    if raw.len() == 4 {
        return pattern.render(Some(year), None, None);
    }
    let month = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|month| (1..=12).contains(month));
    let Some(month) = month else {
        return raw.to_owned();
    };
    if raw.len() == 7 {
        return pattern.render(Some(year), Some(month), None);
    }
    let day = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|day| (1..=31).contains(day));
    let Some(day) = day else {
        return raw.to_owned();
    };
    if parts.next().is_some() {
        return raw.to_owned();
    }
    pattern.render(Some(year), Some(month), Some(day))
}
```

Add `use reprise_core::format::DatePattern;` to both files. In
`release_row.rs`, delete its own `format_release_date`, the
`RELEASE_DATE_*_LEN` constants it used, and the now-unused `show_year`
argument; import the shared function
(`use crate::ui::releases::releases_presentation::format_partial_date;` —
check the exact module path and widen the visibility of the `releases` module
if the compiler asks for it).

- [ ] **Step 4: Rewire the call sites**

In `releases_columns.rs`, the date column renders
`format_partial_date(&entry.first_release_date, &crate::ui::date_format::current().date)`.
In `release_row.rs:167` the same. In the three `format_unix_timestamp` call
sites from Task 3, replace `&DateTimeFormat::iso()` with
`crate::ui::date_format::current()`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p reprise-gnome style_11_ && cargo test -p reprise-gnome releases`
Expected: PASS. Check the counts.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-gnome/src/ui/releases/ crates/reprise-gnome/src/ui/updates/release_row.rs crates/reprise-gnome/src/ui/track_list/
git commit -m "fix: releases write one date, in the system pattern"
```

---

### Task 6: Concerts — table and Updates section

**Files:**
- Modify: `crates/reprise-gnome/src/ui/concerts/concerts_presentation.rs:21-29` and its test at 130-139
- Modify: `crates/reprise-gnome/src/ui/updates/concerts_section.rs:29-40`

**Interfaces:**
- Consumes: `format_partial_date` (Task 5), `date_format::current()` (Task 4).
- Produces: nothing new. `format_event_date` keeps its name and its
  `(date_key: &str, today: NaiveDate)` signature so its callers are untouched;
  `today` becomes unused and is renamed `_today`, matching how
  `format_release_date` already carried an unused `_today`.

- [ ] **Step 1: Write the failing test**

Replace the existing date test in `concerts_presentation.rs` with:

```rust
    /// STYLE-11: the weekday and the current-year abbreviation are gone. A
    /// concert date reads exactly like a release date.
    #[test]
    fn style_11_event_date_is_the_system_pattern() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(format_event_date_with("2026-10-17", &pattern), "17.10.2026");
        assert_eq!(format_event_date_with("2027-01-02", &pattern), "02.01.2027");
        assert_eq!(format_event_date_with("broken", &pattern), "broken");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reprise-gnome style_11_event_date`
Expected: FAIL — `cannot find function format_event_date_with`.

- [ ] **Step 3: Implement**

Replace `concerts_presentation.rs:21-29` with:

```rust
pub(super) fn format_event_date(date_key: &str, _today: NaiveDate) -> String {
    format_event_date_with(date_key, &crate::ui::date_format::current().date)
}

/// The pattern-taking form, so the rule can be tested without reaching for
/// the process-wide format.
pub(super) fn format_event_date_with(date_key: &str, pattern: &DatePattern) -> String {
    crate::ui::releases::releases_presentation::format_partial_date(date_key, pattern)
}
```

Delete the `Datelike` import if it becomes unused. Apply the same replacement
in `updates/concerts_section.rs:29-40` — it holds a second copy of the same
logic with the day and month in the opposite order, which is the drift this
task removes; it calls `format_event_date` instead of formatting anything
itself.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p reprise-gnome concerts`
Expected: PASS. Check the count.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/concerts/ crates/reprise-gnome/src/ui/updates/concerts_section.rs
git commit -m "fix: concerts write one date, in the system pattern"
```

---

### Task 7: The remaining five surfaces

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_presentation.rs:189-194`
- Modify: `crates/reprise-gnome/src/ui/podcasts/add_dialog_results.rs:37`
- Modify: `crates/reprise-gnome/src/ui/library_doctor/start_page.rs:227`
- Modify: `crates/reprise-gnome/src/ui/issues/missing_view.rs:157`
- Modify: `crates/reprise-gnome/src/ui/device_sync/device_sync_page_copy.rs:57,71`

**Interfaces:**
- Consumes: `date_format::current()` (Task 4), `format_partial_date` (Task 5),
  `format_unix_timestamp` (Task 3).
- Produces: nothing new.

- [ ] **Step 1: Write the failing test**

In `podcasts_presentation.rs`, replace whichever test asserts the episode date
with:

```rust
    /// STYLE-11: the episode line carried its own current-year year-omission,
    /// the third copy of that rule in the app. One pattern now, year always.
    #[test]
    fn style_11_episode_date_is_the_system_pattern() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(episode_date_with("2026-08-15", &pattern), "15.08.2026");
        assert_eq!(episode_date_with("2025-08-15", &pattern), "15.08.2025");
    }
```

Read the file first: the episode date arrives as a unix timestamp or as a
string depending on the source. Match the test to the actual input type — if
it is a timestamp, assert through `format_unix_timestamp` with a pinned
`DateTimeFormat` instead, and drop the time portion only if the current line
already omits it.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reprise-gnome style_11_episode_date`
Expected: FAIL.

- [ ] **Step 3: Replace all five sites**

Each one loses its `chrono` `.format("…")` call and renders through the shared
pattern instead:

- `podcasts_presentation.rs` — `%-d. %b` / `%-d. %b %Y` → the pattern, year always.
- `add_dialog_results.rs` — `%b %Y` → `pattern.render(Some(year), Some(month), None)`.
- `start_page.rs` — `%Y-%m-%d %H:%M` → `format_unix_timestamp(secs, date_format::current())`. If the value there is a `NaiveDateTime` rather than a timestamp, render the date through the pattern and the time through `current().clock.render(hour, minute)`.
- `missing_view.rs` — `%b %-d` → the pattern with the year included; this line has room for it.
- `device_sync_page_copy.rs` (both lines) — `%b %-d, %Y at %H:%M` → the pattern, then the translated "at" string that is already there, then `current().clock.render(...)`. Keep the surrounding sentence exactly as it is; only the two rendered fragments change.

- [ ] **Step 4: Prove no formatter survives**

Run:

```bash
git grep -nE '\.format\("%[^"]*[bBaAeyj]' -- crates/reprise-gnome/src crates/reprise-view/src
```

Expected: no output. Any hit is either a site this task missed or one of the
machine strings named in the Global Constraints — check which before deciding.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p reprise-gnome`
Expected: PASS. Display-gated tests stay ignored here; run them singly in Task 9.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/ crates/reprise-gnome/src/ui/library_doctor/start_page.rs crates/reprise-gnome/src/ui/issues/missing_view.rs crates/reprise-gnome/src/ui/device_sync/device_sync_page_copy.rs
git commit -m "fix: podcasts, doctor, missing and device sync write one date"
```

---

### Task 8: The My Stats axis

**Files:**
- Modify: `crates/reprise-core/src/library/stats_period.rs:353-378`
- Modify: whichever caller builds the buckets — find it with
  `git grep -n "fn buckets\|stats_period::" -- crates`

**Interfaces:**
- Consumes: `DatePattern` (Task 1).
- Produces: the bucket-building function takes an extra `pattern: &DatePattern`
  parameter. Its GTK caller passes `&crate::ui::date_format::current().date`;
  its tests pass `&DatePattern::from_platform("%d.%m.%Y")`.

- [ ] **Step 1: Write the failing test**

In `stats_period.rs`'s test module:

```rust
    /// STYLE-11: an axis label names a bucket, so it may show fewer fields
    /// than the pattern holds — the period selector above the chart already
    /// says which span is on screen. It may not show a different pattern.
    #[test]
    fn style_11_axis_labels_follow_the_pattern_at_bucket_precision() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(day_label(2026, 8, 15, &pattern), "15.08");
        assert_eq!(week_label(2026, 8, 15, &pattern), "Week of 15.08");
        assert_eq!(month_label(2026, 8, &pattern), "08.2026");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reprise-core style_11_axis`
Expected: FAIL — `cannot find function day_label`.

- [ ] **Step 3: Implement**

Extract the three label shapes out of the `match granularity` block into named
functions, then call them from it:

```rust
fn day_label(year: i32, month: u32, day: u32, pattern: &DatePattern) -> String {
    pattern.render(None, Some(month), Some(day))
}

fn week_label(year: i32, month: u32, day: u32, pattern: &DatePattern) -> String {
    // "Week of" is a translated string in the GTK layer today; keep whatever
    // this file already produces and only replace the date fragment.
    format!("Week of {}", day_label(year, month, day, pattern))
}

fn month_label(year: i32, month: u32, pattern: &DatePattern) -> String {
    pattern.render(Some(year), Some(month), None)
}
```

Thread `pattern` through the bucket builder's signature to its caller.
`year` is unused in `day_label` and `week_label` — take it anyway so all three
read alike, and silence the warning with a leading underscore if clippy
objects.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p reprise-core stats_period && cargo build -p reprise-gnome`
Expected: PASS and a clean build.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/library/stats_period.rs crates/reprise-gnome/src
git commit -m "fix: the stats axis follows the system pattern at bucket precision"
```

---

### Task 9: The rule, the amendment, and the display proof

**Files:**
- Modify: `docs/ux-rules.md` (section U, after STYLE-9 at line 2820; BROWSE-9 at line 4183)
- Create: `crates/reprise-gnome/src/ui/date_format_display_tests.rs`
- Modify: `crates/reprise-gnome/src/ui/mod.rs` (declare the test module behind `#[cfg(test)]`)

**Interfaces:**
- Consumes: everything above.
- Produces: nothing further.

- [ ] **Step 1: Write the rule**

Insert into `docs/ux-rules.md` directly after STYLE-9:

```markdown
- **STYLE-11** [active] [core] [gtk] — **A date looks the same everywhere.**
  Every displayed calendar date follows the system locale's date pattern,
  with a numeric month and an always four-digit year; a locale pattern the
  app cannot render numerically falls back to ISO. Incomplete dates shorten
  within that same pattern instead of switching to a different one. Times
  show minutes and never seconds, on the system's twelve- or twenty-four-hour
  dial. No call site formats dates itself and no surface keeps a month name.
  A label may show fewer fields than the pattern holds — a chart axis whose
  period is already named on screen omits the year — but never a different
  pattern. Machine-readable strings (API query keys, stored timestamps,
  filenames) and relative phrasings that name an interval rather than a day
  are not dates in this sense and are unaffected. **Test rule:** the pattern
  renderer is unit-tested against the day-first, month-first, year-first and
  suffixed conventions; one display test renders the affected surfaces under
  a pinned pattern.
```

Then amend BROWSE-9 at line 4183: replace "The ISO-formatted time is hidden
by default" with "The time is rendered per STYLE-11 and the column is hidden
by default".

- [ ] **Step 2: Write the failing display test**

Create `crates/reprise-gnome/src/ui/date_format_display_tests.rs`:

```rust
//! STYLE-11 across the real tables, under a pinned pattern.

use gtk4::prelude::*;

/// STYLE-11: four surfaces, one pattern. Renders the releases and concerts
/// tables with `REPRISE_DATE_PATTERN` pinned and asserts that every date-like
/// label matches the day-first shape — measured against the widgets that
/// actually render, not against the formatting functions, because the drift
/// this rule removes lived in the call sites rather than in the formatter.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_11_every_surface_renders_the_pinned_pattern() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    std::env::set_var(crate::ui::date_format::PATTERN_ENV, "%d.%m.%Y");
    gtk4::init().unwrap();

    // Build the releases table exactly as `releases_columns::append_columns`
    // does in `releases_view.rs`, with one entry dated 2026-05-29, and the
    // concerts table with one row dated 2026-10-17. Follow the existing
    // fixtures in `releases_columns.rs`'s and `concerts_columns.rs`'s test
    // modules — they already construct a `HistoryEntry` and a `ConcertRow`.
    // Then collect every descendant label and assert:
    let expected = ["29.05.2026", "17.10.2026"];
    for text in expected {
        // assert some label reads exactly `text`
        let _ = text;
    }
}
```

Fill in the body using the fixture helpers that already exist in
`releases_columns.rs` (`entry`, `descendant_labels`) and `concerts_columns.rs`
(`row`, `descendant_labels`); do not invent new ones.

- [ ] **Step 3: Run it to verify it fails**

Run: `xvfb-run -a cargo test -p reprise-gnome style_11_every_surface -- --ignored --exact --test-threads=1`
Expected: FAIL while the body is still a stub.

- [ ] **Step 4: Complete the test body and run it**

Run: `xvfb-run -a cargo test -p reprise-gnome style_11_every_surface -- --ignored --exact --test-threads=1`
Expected: PASS, `1 passed`.

- [ ] **Step 5: Check the base before believing a red result**

Three display tests are already red on `origin/dev` itself. If anything
unrelated fails, run the same test in a checkout of `origin/dev` before
treating it as caused by this branch.

Run: `bash scripts/check-ux-traceability.sh`
Expected: passes — STYLE-11 now has a rule-named test.

- [ ] **Step 6: Commit**

```bash
git add docs/ux-rules.md crates/reprise-gnome/src/ui/date_format_display_tests.rs crates/reprise-gnome/src/ui/mod.rs
git commit -m "docs: STYLE-11 binds one date format app-wide"
```

---

## Parallelism and file ownership

Tasks 1–4 are a chain: each needs the previous one's types. One agent, in
order.

Tasks 5, 6, 7 and 8 are independent once Task 4 has landed and own disjoint
files. Four agents may run them concurrently. The ownership below is binding —
record it in `AGENTS.md` in the worktree before dispatching, not only here, so
an agent that never reads this plan still cannot stray:

| Task | Owns |
|---|---|
| 5 | `ui/releases/`, `ui/updates/release_row.rs`, `ui/track_list/` |
| 6 | `ui/concerts/`, `ui/updates/concerts_section.rs` |
| 7 | `ui/podcasts/`, `ui/library_doctor/`, `ui/issues/`, `ui/device_sync/` |
| 8 | `core/library/stats_period.rs` and its single GTK caller |

Task 9 runs last, alone: it touches `docs/ux-rules.md`, which every parallel
agent would otherwise conflict on.

## Self-review

- **Spec coverage.** Part B §B.1 → Task 4. §B.2 pattern and partial precision
  → Tasks 1 and 5. §B.3 clock → Tasks 2 and 3. §B.4 call sites → Tasks 5–8,
  all twelve files from the spec's table appear in a task's Files block. The
  chart-label carve-in → Task 8. STYLE-11 and the BROWSE-9 amendment → Task 9.
- **Not covered here by design.** Part A of the spec (column editing) is a
  separate plan, `2026-08-09-table-column-editing.md`. The NR-25 amendment
  belongs to that plan because it is about the cover column, not about dates.
- **Type consistency.** `DatePattern::render(Option<i32>, Option<u32>,
  Option<u32>)` is used with that signature in Tasks 1, 5, 6, 7 and 8.
  `DateTimeFormat { date, clock }` — field named `date`, not `pattern` — is
  used consistently in Tasks 3, 4, 5 and 7. `format_partial_date(&str,
  &DatePattern)` is defined in Task 5 and consumed in Tasks 6 and 7.
- **Known soft spot.** Task 7's podcast step depends on an input type this
  plan did not read (timestamp versus string). The step says so and tells the
  implementer to read the file first rather than guessing — that is a
  deliberate instruction, not a placeholder.
