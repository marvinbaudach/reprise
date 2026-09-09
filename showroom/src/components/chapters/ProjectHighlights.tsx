import { GATES } from 'virtual:merge-gates';
import { PERFORMANCE } from '../../data/measurements';

export function ProjectHighlights() {
  return (
    <nav className="highlights frame" aria-label="Project highlights">
      <a href="#ch-01" className="highlight">
        <span className="highlight__label">Architecture</span>
        <strong>One core. Four frontends.</strong>
        <span>Desktop, Android, CLI and MCP reuse the same application logic.</span>
      </a>
      <a href="#ch-05" className="highlight">
        <span className="highlight__label">Performance</span>
        <strong>{PERFORMANCE[0]?.delta} query time</strong>
        <span>A measured title-list optimisation, with its storage cost reported.</span>
      </a>
      <a href="#ch-02" className="highlight">
        <span className="highlight__label">Quality</span>
        <strong>{GATES.length} checks before merge.</strong>
        <span>Automated checks for architecture, behaviour and distribution.</span>
      </a>
    </nav>
  );
}
