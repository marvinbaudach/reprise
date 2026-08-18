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
          CH.01
        </p>
        <h2 id="ch-01-heading" className="chapter__title" data-reveal>
          One core, four frontends.
        </h2>

        <FigureGrid figures={HEADLINE_FIGURES} variant="headline" />

        <p className="chapter__intro" data-reveal>
          A GNOME desktop app in GTK4 and an Android app in Kotlin with Media3 sit on the same Rust
          core. So do a CLI and an MCP server — four frontends over one verified application layer,
          not four codebases that happen to share a name.
        </p>

        <CoreArchitecture />

        <CodeRatioBar />
      </div>
    </section>
  );
}
