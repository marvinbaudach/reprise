import { GATES } from 'virtual:merge-gates';
import { PERFORMANCE } from '../../data/measurements';

export function ProjectHighlights() {
  return (
    <nav className="highlights frame" aria-label="Project highlights">
      <a href="#ch-01" className="highlight">
        <span className="highlight__label">Architecture</span>
        <strong>One core. Four frontends.</strong>
        <span>Desktop, phone, terminal and AI agents share one tested core.</span>
      </a>
      <a href="#ch-05" className="highlight">
        <span className="highlight__label">Performance</span>
        <strong>{PERFORMANCE[0]?.delta} query time</strong>
        <span>The title list over 100&apos;000 tracks. Storage cost on the record.</span>
      </a>
      <a href="#ch-02" className="highlight">
        <span className="highlight__label">Quality</span>
        <strong>{GATES.length} checks before merge.</strong>
        <span>Architecture, behaviour and packaging — checked on every change.</span>
      </a>
    </nav>
  );
}
