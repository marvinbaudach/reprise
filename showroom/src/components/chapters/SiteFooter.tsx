import { BASELINE, treelink } from '../../data/measurements';
import './SiteFooter.css';

const MARK = `${import.meta.env.BASE_URL}brand/reprise-mark.svg`;

export function SiteFooter() {
  return (
    <footer className="site-footer" data-ground="oklch(10.5% 0.012 269)">
      <div className="site-footer__frame">
        <p className="site-footer__eyebrow" data-reveal>
          Where the figures come from
        </p>

        <p className="site-footer__honesty" data-reveal>
          The test and rule counts are the ones the repository states on{' '}
          <a href={BASELINE.repository}>
            <code>main</code>
          </a>
          . The code volumes were counted on{' '}
          <a href={treelink()}>
            <code>{`dev@${BASELINE.commit}`}</code>
          </a>{' '}
          with cloc 2.08, product and test code separated by an AST pass over every Rust file, and
          verified a second time by hand on {BASELINE.countedOn}.{' '}
          <strong>This build does not measure them yet</strong> — the figures are typed into one
          module and the measurement runs in CI next. Until it does, that sentence stays here
          instead of a claim that would be nicer and untrue.
        </p>

        <div className="site-footer__links" data-reveal>
          <img src={MARK} alt="" width={24} height={24} />
          <a href={BASELINE.repository}>Source</a>
          <a href={`${BASELINE.repository}/blob/${BASELINE.commit}/docs/ux-rules.md`}>Rulebook</a>
          <a href={`${BASELINE.repository}/blob/${BASELINE.commit}/TESTING.md`}>How it is tested</a>
        </div>

        <section className="availability" data-reveal aria-labelledby="availability-heading">
          <div>
            <p className="availability__eyebrow">Availability</p>
            <h2 id="availability-heading">Open to work.</h2>
            <p className="availability__copy">
              This project is the skill demonstration: architecture, the gates that hold agent
              output to it, and four shipped surfaces in four weeks. The same method applies to a
              codebase that is not mine.
            </p>
          </div>
          <div className="availability__actions">
            <a className="availability__contact" href="https://github.com/marvinbaudach">
              github.com/marvinbaudach ↗
            </a>
            <p>GPL-3.0-or-later · active alpha</p>
          </div>
        </section>
      </div>
    </footer>
  );
}
