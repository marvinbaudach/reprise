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
        <h2 id="ch-01-heading" className="chapter__title" data-reveal>
          GNOME, Android, a CLI and an MCP server all run on one Rust core.
        </h2>

        <p className="chapter__intro" data-reveal>
          The library and all application logic live in the core. GNOME and Android stay fully
          native; the CLI and the MCP server run the same code without a screen.
        </p>

        <CoreArchitecture />

        <h3 className="chapter__subhead">What the core does once for both apps</h3>
        <ul className="point-list point-list--pairs">
          <li>
            <strong className="point-list__lead">CUE albums</strong> are split once in the core,
            then play track by track on desktop and phone.
          </li>
          <li>
            <strong className="point-list__lead">Loudness</strong> is measured once (EBU R128) and
            played at −18 LUFS at the desk, −14 LUFS on the phone, so navigation prompts no longer
            drown the music in the car.
          </li>
          <li>
            <strong className="point-list__lead">Missing covers</strong> are fetched by the same
            code on both, and only once online sources are switched on.
          </li>
          <li>
            <strong className="point-list__lead">Scripts and agents</strong> reach the same core
            through the <a href="#ch-04">CLI and the MCP server</a>.
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
