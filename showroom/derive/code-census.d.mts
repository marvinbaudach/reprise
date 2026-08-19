/**
 * Types for `code-census.mjs`, which is plain JavaScript because it also runs by
 * hand (`node showroom/derive/code-census.mjs`) and outside any build. This file
 * is what lets `tsc --noEmit` keep type-checking `vite.config.ts` once the
 * config imports the counter.
 *
 * Only the surface the build uses is declared. The scanner's internals stay
 * where their tests are.
 */

/** Lines of one kind of source, split by what they are. */
export interface CensusSegment {
  /** Non-blank lines that are not part of a `#[cfg(test)]` item. */
  readonly product: number;
  /** Non-blank lines that are. */
  readonly test: number;
}

export interface Census {
  /** Every crate except the Android bridge. */
  readonly rust: CensusSegment;
  /** `crates/reprise-android-ffi` alone. */
  readonly bridge: CensusSegment;
  /** `android/**` — `.kt` and `.kts`. */
  readonly kotlin: CensusSegment;
  /** Every non-blank line the three add up to. */
  readonly total: number;
  /** Every non-blank test line of the three. */
  readonly test: number;
  /** How many files were read. */
  readonly files: number;
  /** Declared `#[test]` and `@Test` functions. */
  readonly testFunctions: number;
}

/** Walks the repository at `repoRoot` and counts it. Throws if it finds nothing. */
export function census(repoRoot: string): Census;

export interface TestRange {
  readonly start: number;
  readonly end: number;
}

export interface ModuleDeclaration {
  readonly name: string;
  /** The file a `#[path = "…"]` attribute names, or null when the name decides. */
  readonly explicit: string | null;
}

export function scanRust(source: string): {
  readonly ranges: readonly TestRange[];
  readonly testMods: readonly string[];
  readonly testPaths: readonly string[];
};
export function rustTestRanges(source: string): readonly TestRange[];
export function declaredModules(source: string): readonly ModuleDeclaration[];
export function countRust(source: string): { readonly product: number; readonly test: number };
export function countTestFunctions(source: string, kotlin: boolean): number;
export function countLines(source: string): number;
export function isRepoRoot(path: string): boolean;
