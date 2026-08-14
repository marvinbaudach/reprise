import { HEADLINE_FIGURES } from '../../data/measurements';
import { CodeRatioBar } from '../ui/CodeRatioBar';
import { FigureGrid } from '../ui/FigureGrid';
import './chapters.css';

export function ChapterOne() {
  return (
    <section id="ch-01" className="chapter" aria-labelledby="ch-01-heading">
      <div className="frame">
        <p className="rule eyebrow">CH.01</p>
        {/* The design names this chapter "Two native apps. One core." — but the
            hero says almost exactly that four hundred pixels higher up. The
            chapter keeps its job and takes the half the hero does not carry. */}
        <h2 id="ch-01-heading" className="display chapter-title">
          One core, four frontends.
        </h2>
      </div>

      <div className="stage">
        <div className="frame">
          <FigureGrid figures={HEADLINE_FIGURES} />
        </div>
      </div>

      <div className="frame chapter__body">
        <p className="prose">
          A GNOME desktop app in GTK4 and an Android app in Kotlin with Media3 sit on the same Rust
          core. So do a CLI and an MCP server — four frontends over one verified application layer,
          not four codebases that happen to share a name.
        </p>

        <CodeRatioBar />
      </div>
    </section>
  );
}
