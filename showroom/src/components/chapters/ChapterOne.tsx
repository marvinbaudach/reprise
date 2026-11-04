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
          The library and all application logic live in one Rust core. GNOME and Android stay fully
          native; the CLI and MCP server run the same core without a screen.
        </p>

        <CoreArchitecture />

        <h3 className="chapter__subhead">Written once, shipped twice</h3>
        <ul className="point-list">
          <li>
            <strong className="point-list__lead">CUE albums</strong> split once in the core, then
            play track by track on desktop and phone.
          </li>
          <li>
            <strong className="point-list__lead">Loudness</strong> measured once (EBU R128): −18
            LUFS at the desk, −14 LUFS on the phone, so navigation prompts no longer drown the music
            in the car.
          </li>
          <li>
            <strong className="point-list__lead">Missing covers</strong> fetched by the same code on
            both, and only once online sources are on.
          </li>
          <li>
            <strong className="point-list__lead">Scripts and agents</strong> —{' '}
            <a href="#ch-04">CLI and MCP</a> on the same core.
          </li>
        </ul>
        <details className="evidence-details">
          <summary>Code breakdown</summary>
          <FigureGrid figures={HEADLINE_FIGURES} variant="headline" />
          <CodeRatioBar />
        </details>
      </div>
    </section>
  );
}
