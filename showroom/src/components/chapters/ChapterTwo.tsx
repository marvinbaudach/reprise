import type { CSSProperties } from 'react';
import { MERGE_CHAIN, RULEBOOK_FIGURES, VERIFICATION_RUNGS } from '../../data/measurements';
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
          <ol className="chain" aria-label="What a change walks through before it lands">
            {MERGE_CHAIN.map((step, index) => (
              <li className="chain__step" key={step}>
                <span className="data chain__index">{String(index + 1).padStart(2, '0')}</span>
                <span className="chain__label">{step}</span>
              </li>
            ))}
          </ol>

          <p className="prose chain__note">
            A human takes the plan apart exactly once, before a line of code exists. After that the
            chain runs unattended: an agent writes the code, a reviewer checks it per language, and
            every finding goes to a second agent whose only job is to refute it. Only survivors
            reach the refactor. <strong>The model that writes the code never reviews it.</strong>
          </p>
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
