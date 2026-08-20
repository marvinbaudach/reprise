/**
 * The facts the page derives from the repository at build time. The producing
 * side is the `reprise-derived-facts` plugin in `vite.config.ts`; these
 * declarations are what lets `tsc --noEmit` see them.
 */

declare module 'virtual:merge-gates' {
  /** The merge gate's checks, in the order `scripts/check-merge-readiness.sh` runs them. */
  export const GATES: readonly string[];

  export interface GateGroup {
    readonly name: string;
    readonly line: string;
    /** Checks assigned to this group, retained in script order. */
    readonly gates: readonly string[];
  }

  /** Six exhaustive coverage groups, validated against `GATES` at build time. */
  export const GATE_GROUPS: readonly GateGroup[];
}

declare module 'virtual:code-census' {
  export interface CensusSegment {
    readonly product: number;
    readonly test: number;
  }

  export interface Census {
    readonly rust: CensusSegment;
    readonly bridge: CensusSegment;
    readonly kotlin: CensusSegment;
    readonly total: number;
    readonly test: number;
    readonly files: number;
    readonly testFunctions: number;
  }

  /**
   * Non-blank lines of the tree this build stands in, counted by
   * `derive/code-census.mjs` — the one place the project counts its own lines.
   */
  export const CENSUS: Census;
}

declare module 'virtual:build-timeline' {
  export interface TimelineWeek {
    readonly week: number;
    /** ISO day the week starts on. */
    readonly from: string;
    /** ISO day it ends on. */
    readonly to: string;
    readonly theme: string;
    readonly landed: string;
  }

  /** The weeks, read from the record in `docs/showroom/timeline.md`. */
  export const TIMELINE: readonly TimelineWeek[];
}

declare module 'virtual:measurements' {
  export interface LedgerRow {
    readonly what: string;
    readonly before: string;
    readonly after: string;
    readonly delta: string;
    readonly commit: string;
    readonly date: string;
    readonly method: string;
  }

  /** The index rebuild, quoted from `docs/measurements/index-rebuild.md`. */
  export const INDEX_REBUILD: {
    readonly rows: readonly LedgerRow[];
    readonly price: string;
  };
}

declare module 'virtual:spectral-axis' {
  /** `CORAL` and `TEAL` from `crates/reprise-view/src/spectral_colour.rs`. */
  export const AXIS: {
    readonly coral: string;
    readonly teal: string;
  };
}

declare module 'virtual:incident' {
  export const INCIDENT: {
    readonly date: string;
  };
}
