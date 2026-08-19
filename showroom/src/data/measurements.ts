import { GATES } from 'virtual:merge-gates';

/**
 * Every figure the showroom prints, and where it comes from.
 *
 * These values were counted on `dev@604677322e` with cloc 2.08, product and test
 * code separated by an AST pass over every Rust file, and each one was verified
 * a second time by hand on 2026-08-14 before it was allowed onto this page.
 *
 * They are typed here, not measured by the build. That is the honest state and
 * the footer says so. The measurement strand replaces this module with generated
 * JSON; until then, a figure without a `source` has no business being here.
 */

export const BASELINE = {
  commit: '604677322e',
  countedOn: '2026-08-14',
  repository: 'https://github.com/marvinbaudach/reprise',
} as const;

export function permalink(path: string): string {
  return `${BASELINE.repository}/blob/${BASELINE.commit}/${path}`;
}

export function treelink(path = ''): string {
  return `${BASELINE.repository}/tree/${BASELINE.commit}${path ? `/${path}` : ''}`;
}

export interface Figure {
  readonly value: string;
  readonly label: string;
  readonly detail?: string;
  readonly href?: string;
  readonly counter?: boolean;
}

/** The four numbers that carry chapter one. */
export const HEADLINE_FIGURES: readonly Figure[] = [
  {
    value: "347'842",
    label: 'lines of Rust and Kotlin',
    detail: "327'165 Rust · 20'677 Kotlin",
    counter: true,
  },
  {
    value: '45.8 %',
    label: 'of them are tests',
    detail: "149'504 Rust · 9'884 Kotlin",
    counter: true,
  },
  {
    value: '1 → 4',
    label: 'one core, four frontends',
    detail: 'GNOME · Android · CLI · MCP',
  },
  {
    // Derived, not typed: the wall in chapter two, the tempo band and this
    // figure all count the same `gate` calls in the merge gate script, so the
    // page cannot end up disagreeing with itself.
    value: String(GATES.length),
    label: 'gates before every merge',
    detail: 'all of them, every time',
    counter: true,
  },
];

/** Chapter one, figure B: where the lines actually sit. */
export interface CodeSegment {
  readonly key: string;
  readonly label: string;
  readonly lines: number;
  readonly note: string;
  readonly share: string;
}

export const CODE_SEGMENTS: readonly CodeSegment[] = [
  {
    key: 'rust-product',
    label: 'Rust, product',
    lines: 177_661,
    note: 'the core and everything built on it',
    share: '49.6',
  },
  {
    key: 'rust-test',
    label: 'Rust, tests',
    lines: 149_504,
    note: 'inline `#[cfg(test)]` modules and integration suites',
    share: '41.7',
  },
  {
    key: 'android-bridge',
    label: 'Rust, Android bridge',
    lines: 10_245,
    note: 'crates/reprise-android-ffi',
    share: '2.9',
  },
  {
    key: 'kotlin',
    label: 'Kotlin',
    lines: 20_677,
    note: 'the whole Android frontend, product and tests',
    share: '5.8',
  },
];

/** Chapter three: the two ends of the spectral axis, verbatim from the source. */
export const SPECTRAL_AXIS = {
  coral: '#FF6F5E',
  teal: '#4FDBD4',
  source: 'crates/reprise-view/src/spectral_colour.rs',
} as const;

/** Chapter five: what the index rebuild cost and bought. */
export interface Measurement {
  readonly what: string;
  readonly before: string;
  readonly after: string;
  readonly delta: string;
}

export const PERFORMANCE: readonly Measurement[] = [
  {
    what: "Title window over 100'000 tracks",
    before: "53'605 µs",
    after: "1'333 µs",
    delta: '−97.51 %',
  },
  { what: 'Playback ID projection', before: "8'125 µs", after: '298 µs', delta: '−96.33 %' },
  { what: 'Main-thread CPU while idle', before: '110 ms/s', after: '64 ms/s', delta: '−41.8 %' },
  { what: 'Tag reads on a warm start', before: '419', after: '0', delta: '−100 %' },
];

export const PERFORMANCE_PRICE =
  "The price sits next to it, not in the small print: the title index costs 2'379'776 extra database bytes, up 9.85 %. The track list stays pinned by test to eight cached SQL windows and 1'600 retained rows — unchanged between 10'000 and 100'000 tracks.";
