import { ARCHITECTURE_LINKS, ARCHITECTURE_SURFACES } from '../../data/architecture';
import './architecture.css';

export function CoreArchitecture() {
  return (
    <figure
      className="architecture"
      data-showcase="core-architecture"
      data-reveal
      aria-labelledby="architecture-title"
      aria-describedby="architecture-caption"
    >
      <header className="architecture__header">
        <div>
          <p className="eyebrow">Core and edges</p>
          <h3 id="architecture-title" className="architecture__title">
            Four surfaces. Dependency only points inward.
          </h3>
        </div>
        <a className="data architecture__gate-link" href={ARCHITECTURE_LINKS.gate}>
          machine-enforced ↗
        </a>
      </header>

      <ol className="architecture__surfaces" aria-label="Reprise frontends">
        {ARCHITECTURE_SURFACES.map((surface, index) => (
          <li className="architecture__surface" data-reveal key={surface.id}>
            <a
              className={`architecture__surface-link architecture__surface-link--${surface.id}`}
              href={surface.href}
              data-surface={surface.id}
            >
              <span className="data architecture__index">0{index + 1}</span>
              <strong>{surface.name}</strong>
              <span>{surface.stack}</span>
              <span className="data">{surface.role}</span>
            </a>
          </li>
        ))}
      </ol>

      <div className="architecture__flow" aria-hidden="true">
        <span />
        <span />
        <span />
        <span />
      </div>

      <div className="architecture__shared">
        <a
          className="architecture__layer architecture__layer--view"
          data-reveal
          href={ARCHITECTURE_LINKS.view}
        >
          <span className="eyebrow">portable presentation semantics</span>
          <strong>reprise-view</strong>
          <span className="data">geometry · colour · scene · surface models</span>
        </a>
        <a
          className="architecture__layer architecture__layer--core"
          data-reveal
          href={ARCHITECTURE_LINKS.core}
        >
          <span className="eyebrow">application and domain layer</span>
          <strong>reprise-core</strong>
          <span className="data">19 dependencies · 0 UI frameworks</span>
        </a>
      </div>

      <figcaption id="architecture-caption" className="architecture__caption" data-reveal>
        The dependency arrows only point inward. GNOME and Android keep their native toolkit and
        interaction model; CLI and MCP reuse the same application layer without pretending to be
        screens. The build rejects GTK, libadwaita, GStreamer and D-Bus in the core.
      </figcaption>
    </figure>
  );
}
