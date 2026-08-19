import { TIMELINE } from 'virtual:build-timeline';
import { BASELINE, CENSUS_SCOPE, PERFORMANCE_RECORD, permalink } from '../../data/measurements';
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
          Three kinds of number appear on this page and they are not the same kind of claim.{' '}
          <strong>Counted:</strong> the line volumes, their shares, the gate count, the pipeline
          table and the five weeks are read out of this repository while the page is built —{' '}
          <a href={permalink(CENSUS_SCOPE.source)}>
            <code>code-census.mjs</code>
          </a>{' '}
          walked {CENSUS_SCOPE.files} files for the volumes, counting every line with something on
          it. <strong>Quoted:</strong> the index rebuild describes a change that happened once and
          cannot be recounted from the tree, so it is read from{' '}
          <a href={permalink(PERFORMANCE_RECORD)}>
            <code>index-rebuild.md</code>
          </a>
          , where every row carries its commit, its date and its method. <strong>Stated:</strong>{' '}
          &ldquo;1 → 4&rdquo; is an architectural claim, not a measurement, and the four frontends
          are its evidence. Nothing on this page is typed next to the words it would be asserting; a
          test in the suite is what keeps it that way.
        </p>

        <div className="site-footer__links" data-reveal>
          <img src={MARK} alt="" width={24} height={24} />
          <a href={BASELINE.repository}>Source</a>
          <a href={permalink('docs/ux-rules.md')}>Rulebook</a>
          <a href={permalink('TESTING.md')}>How it is tested</a>
          <a href={permalink(PERFORMANCE_RECORD)}>Measurements</a>
        </div>

        <section className="availability" data-reveal aria-labelledby="availability-heading">
          <div>
            <p className="availability__eyebrow">Availability</p>
            <h2 id="availability-heading">Open to work.</h2>
            <p className="availability__copy">
              This project is the skill demonstration: architecture, the gates that hold agent
              output to it, and four shipped surfaces in {TIMELINE.length} weeks. The same method
              applies to a codebase that is not mine.
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
