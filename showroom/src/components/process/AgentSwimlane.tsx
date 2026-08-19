import { PIPELINE } from 'virtual:agent-pipeline';
import type { CSSProperties } from 'react';
import { permalink, treelink } from '../../data/measurements';
import './process.css';

// What each phase does, in one word, so a mark carries text rather than a
// symbol a screen reader would have to spell out. An unknown phase falls back to
// its own name — visible and harmless, never a blank cell.
const VERBS: Readonly<Record<string, string>> = {
  Plan: 'drafts',
  Checkpoint: 'challenges',
  Implement: 'writes',
  Review: 'reviews',
  Refute: 'refutes',
  Refactor: 'applies',
  Gate: 'decides',
};

const slug = (actor: string) => actor.toLowerCase().replace(/[^a-z0-9]+/g, '-');

// Lane order follows first appearance in the table, so reading top to bottom is
// also reading step 01 onwards. No second hand-kept order to drift.
const LANES = [...new Set(PIPELINE.map((step) => step.actor))];

export function AgentSwimlane() {
  return (
    <figure
      className="swimlane"
      data-showcase="agent-swimlane"
      data-reveal
      aria-labelledby="agent-swimlane-title"
    >
      <header className="process-heading">
        <p className="eyebrow">Authorship is not authority</p>
        <h3 id="agent-swimlane-title">One checkpoint. Independent agents. Mechanical proof.</h3>
      </header>

      {/*
       * A table, not a grid of divs: the assignment of an actor to a step is
       * exactly what a row and column header pair says, and a screen reader can
       * then read "Codex, Implement" instead of buffering a wall of cells.
       */}
      <div className="swimlane__scroll">
        <table className="swimlane__grid">
          <caption>Who performs which step of a change, from plan to merge gate</caption>
          <thead>
            <tr>
              <td className="swimlane__corner" />
              {PIPELINE.map((step) => (
                <th className="swimlane__step" scope="col" key={step.step}>
                  <span className="data swimlane__number">{step.step}</span>
                  <span className="swimlane__phase">{step.phase}</span>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {LANES.map((actor) => (
              <tr className="swimlane__lane" data-role={slug(actor)} key={actor}>
                <th className="swimlane__actor" scope="row">
                  {actor}
                </th>
                {PIPELINE.map((step, index) => (
                  <td className="swimlane__cell" key={step.step}>
                    {step.actor === actor ? (
                      <span
                        className="swimlane__mark"
                        data-mark=""
                        // Marks arrive in step order, so the handovers read left
                        // to right. A delay per column, not a timer.
                        style={{ '--mark-delay': `${index * 90}ms` } as CSSProperties}
                      >
                        {VERBS[step.phase] ?? step.phase}
                      </span>
                    ) : null}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <figcaption className="swimlane__independence">
        <strong>The writer never reviews.</strong>
        <strong>The reviewer never writes.</strong>
        <span>The skeptic cannot apply findings.</span>
      </figcaption>

      <footer className="swimlane__context">
        <p className="data">Plan files · rulebook · handovers · decision records</p>
        <nav aria-label="Workflow context sources">
          <a href={permalink('docs/agents/pipeline.md')}>pipeline ↗</a>
          <a href={treelink('docs/plans')}>plans ↗</a>
          <a href={permalink('docs/ux-rules.md')}>rulebook ↗</a>
          <a href={permalink('.superpowers/sdd/progress.md')}>ledger ↗</a>
        </nav>
      </footer>
    </figure>
  );
}
