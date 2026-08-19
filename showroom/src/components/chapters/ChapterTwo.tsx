import { AgentSwimlane } from '../process/AgentSwimlane';
import { GateWall } from '../process/GateWall';
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

        <AgentSwimlane />

        <GateWall />

        <p className="chapter__intro chapter__intro--closing" data-reveal>
          A rule ID leads to a test, the test to a commit, the commit to the decision. The
          traceability is itself a merge gate: every enforceable rule has a test of the same name —
          not as a count anyone tallied, but because a rule without one fails the build, and so does
          a test pointing at a rule that no longer exists.
        </p>
      </div>
    </section>
  );
}
