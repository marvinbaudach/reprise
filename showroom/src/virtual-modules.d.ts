/**
 * Two facts the page derives from the repository at build time. The producing
 * side is the `reprise-derived-facts` plugin in `vite.config.ts`; these
 * declarations are what lets `tsc --noEmit` see them.
 */

declare module 'virtual:merge-gates' {
  /** The merge gate's checks, in the order `scripts/check-merge-readiness.sh` runs them. */
  export const GATES: readonly string[];
}

declare module 'virtual:agent-pipeline' {
  export interface PipelineStep {
    readonly step: string;
    readonly phase: string;
    readonly actor: string;
    readonly writes: boolean;
    readonly judges: boolean;
  }

  /** The pipeline's steps, read from the table in `docs/agents/pipeline.md`. */
  export const PIPELINE: readonly PipelineStep[];
}
