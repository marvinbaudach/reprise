import { RULEBOOK_FIGURES, VERIFICATION_RUNGS } from '../../data/measurements';
import { AgentWorkflow } from '../process/AgentWorkflow';
import { ExplorationLoop } from '../process/ExplorationLoop';
import { FigureGrid } from '../ui/FigureGrid';
import './chapters.css';

export function ChapterTwo() {
  return (
    <section
      id="ch-02"
      className="chapter chapter--design"
      data-ground="oklch(13.5% 0.02 205)"
      aria-labelledby="ch-02-heading"
    >
      <div className="frame">
        <p className="chapter__eyebrow" data-reveal>
          CH.02
        </p>
        <h2 id="ch-02-heading" className="chapter__title" data-reveal>
          Built by agents. Merged by gates.
        </h2>

        <AgentWorkflow />

        <div className="evidence-heading" data-reveal>
          <p>Evidence, weakest first</p>
          <h3>Five levels of proof. The top two are agents nobody scripted.</h3>
          <p>
            Each level proves something the one below it cannot. The counts fall as the evidence
            gets harder to fake.
          </p>
        </div>

        <ol className="rungs">
          {VERIFICATION_RUNGS.map((rung) => (
            <li
              className={`rungs__rung${rung.agentDriven ? ' rungs__rung--agent' : ''}`}
              data-reveal
              key={rung.name}
            >
              <span className="rungs__count" data-counter>
                {rung.count}
              </span>
              <strong className="rungs__name">{rung.name}</strong>
              <span className="rungs__proves">
                <span className="rungs__marker rungs__marker--can">can prove</span>
                {rung.proves}
              </span>
              <span className="rungs__cannot">
                <span className="rungs__marker rungs__marker--cannot">cannot</span>
                {rung.cannotProve}
              </span>
            </li>
          ))}
        </ol>

        <p className="chapter__intro chapter__intro--after-rungs" data-reveal>
          The top two rungs are not scripts with expected values: an agent reads the accessibility
          tree, decides where to click on its own, and reports what is not in order — judged against
          deterministic anomaly classes with fixed thresholds. That is how defects nobody scripted
          are found.
        </p>

        <ExplorationLoop />

        <h3 className="chapter__subhead" data-reveal>
          The rulebook, and how much of it is traceable
        </h3>

        <FigureGrid figures={RULEBOOK_FIGURES} variant="rulebook" />

        <p className="chapter__intro chapter__intro--closing" data-reveal>
          A rule ID leads to a test, the test to a commit, the commit to the decision. The
          traceability is itself a merge gate: a rule without a test fails the build — and so does a
          test that points at a rule that no longer exists.
        </p>
      </div>
    </section>
  );
}
