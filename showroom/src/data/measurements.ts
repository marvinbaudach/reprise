import { CENSUS } from 'virtual:code-census';
import { INDEX_REBUILD } from 'virtual:measurements';
import { GATES } from 'virtual:merge-gates';
import { AXIS } from 'virtual:spectral-axis';

/**
 * Every figure the showroom prints, and where it comes from.
 *
 * Three kinds of number appear on this page and they are not interchangeable:
 *
 * - **Counted.** The line volumes and their shares come from
 *   `derive/code-census.mjs`, which walks this very tree at build time. Nobody
 *   types them; changing the code changes the page.
 * - **Quoted.** The index rebuild's before/after figures describe a change that
 *   happened once and cannot be re-counted from the tree. They are read from
 *   `docs/measurements/index-rebuild.md`, where each row carries its commit, its
 *   date and its method.
 * - **Stated.** "1 → 4" is a claim about architecture, not a measurement. It is
 *   typed, and it says so.
 *
 * The footer tells the reader which is which. A figure that fits none of the
 * three has no business being here.
 */

export const BASELINE = {
  /**
   * The commit every permalink on this page points at — and nothing else. The
   * volumes used to be counted here too; now they are counted at build time, so
   * this is a link target, not a provenance claim.
   *
   * It has to be a commit that carries every path the page cites, and it has to
   * be reachable from a published branch. Both have been wrong here. The first
   * pin predated `index-rebuild.md`, `timeline.md` and `code-census.mjs`, so
   * four links resolved to a 404. The second, `a776f8a963`, was rewritten out
   * of the history: the commit stayed in local object stores, so the page kept
   * building here while every link on it 404ed and CI could not resolve the
   * commit at all. `e9ceec3645` is its rewritten twin — same tree, byte for
   * byte — and it sits on `main`. `permalinks-resolve.test.mjs` fails the build
   * on a missing path, which is the only reason this stays true.
   *
   * It follows the promotions rather than standing still. `49c2807a42` is the
   * merge that released 0.1.139; before it the pin sat two weeks behind the
   * figures beside it, so a reader who clicked a source link left the tree the
   * counts had been taken from. Moving it is safe exactly while it is a commit
   * on `main` — a tag would read better and resolve worse, because the page is
   * rebuilt between releases and the pin has to carry the paths that build
   * cites.
   */
  commit: '49c2807a42',
  repository: 'https://github.com/marvinbaudach/reprise',
} as const;

export function permalink(path: string): string {
  return `${BASELINE.repository}/blob/${BASELINE.commit}/${path}`;
}

export function treelink(path = ''): string {
  return `${BASELINE.repository}/tree/${BASELINE.commit}${path ? `/${path}` : ''}`;
}

/**
 * Thousands grouped the way the whole page groups them. `Intl` would do this
 * too, but its separator for `de-CH` has changed character between ICU versions
 * — and a figure whose punctuation depends on the Node build it was rendered
 * with is not a figure anyone can assert against.
 */
export function group(value: number): string {
  return Math.round(value)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, "'");
}

/** One decimal, which is as fine as a line count deserves to be read. */
function share(part: number, whole: number): string {
  return `${((part / whole) * 100).toFixed(1)} %`;
}

const RUST_TOTAL =
  CENSUS.rust.product + CENSUS.rust.test + CENSUS.bridge.product + CENSUS.bridge.test;
const KOTLIN_TOTAL = CENSUS.kotlin.product + CENSUS.kotlin.test;
const RUST_TESTS = CENSUS.rust.test + CENSUS.bridge.test;

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
    // Counted: every non-blank line under `crates/` and `android/`.
    value: group(CENSUS.total),
    label: 'lines of Rust and Kotlin',
    detail: `${group(RUST_TOTAL)} Rust · ${group(KOTLIN_TOTAL)} Kotlin`,
    counter: true,
  },
  {
    value: share(CENSUS.test, CENSUS.total),
    label: 'of them are tests',
    detail: `${group(RUST_TESTS)} Rust · ${group(CENSUS.kotlin.test)} Kotlin`,
    counter: true,
  },
  {
    // Stated, not counted: four frontends over one core is an architectural
    // claim, and the four names are the evidence for it.
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

function segment(key: string, label: string, lines: number, note: string): CodeSegment {
  return { key, label, lines, note, share: ((lines / CENSUS.total) * 100).toFixed(1) };
}

export const CODE_SEGMENTS: readonly CodeSegment[] = [
  segment(
    'rust-product',
    'Rust, product',
    CENSUS.rust.product,
    'the core and everything built on it',
  ),
  segment(
    'rust-test',
    'Rust, tests',
    CENSUS.rust.test,
    'inline `#[cfg(test)]` items and the files they pull in',
  ),
  segment(
    'android-bridge',
    'Rust, Android bridge',
    CENSUS.bridge.product + CENSUS.bridge.test,
    'crates/reprise-android-ffi',
  ),
  segment('kotlin', 'Kotlin', KOTLIN_TOTAL, 'the whole Android frontend, product and tests'),
];

/** How many files the count read, and how many tests it found declared. */
export const CENSUS_SCOPE = {
  files: CENSUS.files,
  testFunctions: CENSUS.testFunctions,
  source: 'showroom/derive/code-census.mjs',
} as const;

/** Chapter three: the two ends of the spectral axis, read from the function. */
export const SPECTRAL_AXIS = {
  coral: AXIS.coral,
  teal: AXIS.teal,
  source: 'crates/reprise-view/src/spectral_colour.rs',
} as const;

/** Chapter five: what the index rebuild cost and bought. */
export interface Measurement {
  readonly what: string;
  readonly before: string;
  readonly after: string;
  readonly delta: string;
  readonly commit: string;
  readonly date: string;
  readonly method: string;
}

export const PERFORMANCE: readonly Measurement[] = INDEX_REBUILD.rows;

export const PERFORMANCE_PRICE = INDEX_REBUILD.price;

export const PERFORMANCE_RECORD = 'docs/measurements/index-rebuild.md';

/**
 * The incident CH.02 quotes. Like the ledger above it is *quoted*, not counted:
 * it happened once and cannot be recovered from the tree.
 */
export const INCIDENT_RECORD = 'docs/plans/queue-anchor-grill-followups.md';

/** The doc comment CH.02 quotes, and the trap it named before it was sprung. */
export const STYLE_SOURCE = 'crates/reprise-gnome/src/ui/style/mod.rs';

/** The script whose parsed gate calls supply CH.02's strip and coverage groups. */
export const MERGE_GATE_SOURCE = 'scripts/check-merge-readiness.sh';
