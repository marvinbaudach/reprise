import { HEADLINE_FIGURES } from '../../data/measurements';
import { CoreArchitecture } from '../architecture/CoreArchitecture';
import { CodeRatioBar } from '../ui/CodeRatioBar';
import { FigureGrid } from '../ui/FigureGrid';
import './chapters.css';

export function ChapterOne() {
  return (
    <section
      id="ch-01"
      className="chapter chapter--design"
      data-ground="oklch(14.5% 0.016 258)"
      aria-labelledby="ch-01-heading"
    >
      <div className="frame">
        <p className="chapter__eyebrow" data-reveal>
          Architecture
        </p>
        <h2 id="ch-01-heading" className="chapter__title" data-reveal>
          One core, four frontends.
        </h2>

        <p className="chapter__intro" data-reveal>
          I separated the music library and application logic from platform code. GNOME and Android
          keep their native interfaces; the CLI and MCP server use the same core without a screen.
          Adding a platform builds on existing behaviour.
        </p>

        <CoreArchitecture />

        <p className="case-result">
          The boundary is enforced in the build: the core cannot depend on a UI framework.{' '}
          <a href="#ch-04">Explore CLI and MCP</a>.
        </p>
        <details className="evidence-details">
          <summary>Code breakdown</summary>
          <FigureGrid figures={HEADLINE_FIGURES} variant="headline" />
          <CodeRatioBar />
        </details>
      </div>
    </section>
  );
}
