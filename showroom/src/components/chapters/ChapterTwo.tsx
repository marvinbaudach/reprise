import type { CSSProperties } from 'react';
import { RULEBOOK_FIGURES, VERIFICATION_RUNGS } from '../../data/measurements';
import { AgentWorkflow } from '../process/AgentWorkflow';
import { ExplorationLoop } from '../process/ExplorationLoop';
import { FigureGrid } from '../ui/FigureGrid';
import './chapters.css';

export function ChapterTwo() {
  return (
    <section id="ch-02" className="chapter" aria-labelledby="ch-02-heading">
      <div className="frame">
        <p className="rule eyebrow">CH.02</p>
        <h2 id="ch-02-heading" className="display chapter-title">
          Built by agents. Merged by gates.
        </h2>
      </div>

      <div className="stage stage--second">
        <div className="frame">
          <AgentWorkflow />
        </div>
      </div>

      <div className="frame chapter__body">
        <h3 className="rule eyebrow">The five rungs, weakest evidence first</h3>

        <ol className="rungs">
          {VERIFICATION_RUNGS.map((rung, index) => (
            <li
              className={`rungs__rung${rung.agentDriven ? ' rungs__rung--agent' : ''}`}
              key={rung.name}
              style={{ '--rung-index': index } as CSSProperties}
            >
              <span className="figure-value rungs__count">{rung.count}</span>
              <span className="rungs__name">{rung.name}</span>
              <span className="data rungs__proves">
                <span className="rungs__marker rungs__marker--can">can prove</span> {rung.proves}
              </span>
              <span className="data rungs__cannot">
                <span className="rungs__marker rungs__marker--cannot">cannot</span>{' '}
                {rung.cannotProve}
              </span>
            </li>
          ))}
        </ol>

        <p className="prose">
          The top two rungs are not scripts with expected values: an agent reads the accessibility
          tree, decides where to click on its own, and reports what is not in order — judged against
          deterministic anomaly classes with fixed thresholds. That is how defects nobody scripted
          are found.
        </p>

        <ExplorationLoop />

        <h3 className="rule eyebrow">The rulebook, and how much of it is traceable</h3>

        <FigureGrid figures={RULEBOOK_FIGURES} />

        <p className="prose">
          A rule ID leads to a test, the test to a commit, the commit to the decision. The
          traceability is itself a merge gate: a rule without a test fails the build — and so does a
          test that points at a rule that no longer exists.
        </p>
      </div>
    </section>
  );
}
