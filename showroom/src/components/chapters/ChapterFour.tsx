import { CLI_COMMANDS, MCP_CAPABILITIES } from '../../data/headless';
import './ChapterFour.css';

export function ChapterFour() {
  return (
    <section
      id="ch-04"
      className="chapter-four"
      data-ground="oklch(13% 0.016 269)"
      aria-labelledby="ch-04-heading"
    >
      <div className="chapter-four__frame">
        <p className="chapter-four__eyebrow" data-reveal>
          CH.04
        </p>
        <h2 id="ch-04-heading" className="chapter-four__title" data-reveal>
          The other two frontends have no screen at all.
        </h2>
        <p className="chapter-four__intro" data-reveal>
          A core with no interface is only a claim until something without a screen uses it. The CLI
          and the MCP server run as separate processes against the same database as the desktop app,
          and a change-log notifier shows their edits live in a running GTK window without a
          restart. That is the boundary being load-bearing rather than documented.
        </p>

        <div className="headless-grid">
          <figure className="headless-card headless-card--terminal" data-reveal>
            <figcaption className="terminal-card__caption">
              <span className="terminal-card__dots" aria-hidden="true">
                <span />
                <span />
                <span />
              </span>
              <span>reprise-cli · frontend 03</span>
            </figcaption>
            <div className="terminal-card__commands">
              {CLI_COMMANDS.map(({ command, nowrap }) => (
                <p className={nowrap ? 'terminal-card__command--nowrap' : undefined} key={command}>
                  <span aria-hidden="true">$</span> {`reprise-cli ${command}`}
                </p>
              ))}
            </div>
            <p className="headless-card__note terminal-card__note">
              Every command takes <code>--json</code> for machine consumption and <code>--db</code>{' '}
              for a scratch library, so automation never has to touch the real one. Deleting a
              playlist refuses to run without <code>--yes</code>.
            </p>
          </figure>

          <figure className="headless-card headless-card--mcp" data-reveal>
            <figcaption className="mcp-card__caption">
              <span>reprise-mcp · frontend 04</span>
              <span>read is safe, writes are opt-in</span>
            </figcaption>
            <ul className="capability-list">
              {MCP_CAPABILITIES.map((capability) => {
                const state = capability.enabled ? 'on' : 'off';
                return (
                  <li className="capability" key={capability.id}>
                    <span className="capability__id">{capability.id}</span>
                    <span className="capability__description">{capability.description}</span>
                    <span className={`capability__state capability__state--${state}`}>{state}</span>
                  </li>
                );
              })}
            </ul>
            <p className="headless-card__note mcp-card__note">
              Tools over stdio, each behind one of six capability flags read live from the library.
              A revocation takes effect on the next call. Responses never carry filesystem paths,
              cache locations or credentials — and the source resources omit stored URLs, because
              those can hold access tokens.
            </p>
            <p className="mcp-card__link">
              <a href="https://github.com/marvinbaudach/reprise/blob/main/crates/reprise-mcp/README.md">
                the tool contract ↗
              </a>
            </p>
          </figure>
        </div>
      </div>
    </section>
  );
}
