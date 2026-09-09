import { TIMELINE } from 'virtual:build-timeline';
import {
  BASELINE,
  CENSUS_SCOPE,
  group,
  INCIDENT_RECORD,
  PERFORMANCE_RECORD,
  permalink,
} from '../../data/measurements';
import { TempoBand } from './TempoBand';
import './SiteFooter.css';

const MARK = `${import.meta.env.BASE_URL}brand/reprise-mark.svg`;

export function SiteFooter() {
  return (
    <footer className="site-footer" data-ground="oklch(10.5% 0.012 269)">
      <div className="site-footer__frame">
        <section
          id="availability"
          className="availability"
          data-reveal
          aria-labelledby="availability-heading"
        >
          <div>
            <p className="availability__eyebrow">Availability</p>
            <h2 id="availability-heading">Open to work.</h2>
            <p className="availability__copy">
              I’m Marvin Baudach. Reprise brings together my work in product design, native
              development and AI-assisted engineering. I’m interested in bringing that experience to
              a team and its next product challenge.
            </p>
          </div>
          <div className="availability__actions">
            <a className="availability__contact" href="https://github.com/marvinbaudach">
              github.com/marvinbaudach ↗
            </a>
            <p>GPL-3.0-or-later · active alpha</p>
          </div>
        </section>
        <details className="evidence-details">
          <summary>Sources and methodology</summary>
          <p className="site-footer__eyebrow" data-reveal>
            Where the figures come from
          </p>

          <p className="site-footer__honesty" data-reveal>
            Three kinds of number appear on this page and they are not the same kind of claim.{' '}
            <strong>Counted:</strong> the line volumes, their shares, the gate and group counts, and
            the five weeks are read out of this repository while the page is built —{' '}
            <a href={permalink(CENSUS_SCOPE.source)}>
              <code>code-census.mjs</code>
            </a>{' '}
            walked {group(CENSUS_SCOPE.files)} files for the volumes, counting every line with
            something on it. <strong>Quoted:</strong> the index rebuild and the quality case study
            both describe things that happened once and cannot be recounted from the tree, so they
            are read from{' '}
            <a href={permalink(PERFORMANCE_RECORD)}>
              <code>index-rebuild.md</code>
            </a>{' '}
            and{' '}
            <a href={permalink(INCIDENT_RECORD)}>
              <code>queue-anchor-grill-followups.md</code>
            </a>
            , where every row and every claim carries its commit, its date and its method.{' '}
            <strong>Stated:</strong> &ldquo;1 → 4&rdquo; is an architectural claim, not a
            measurement, and the four frontends are its evidence. Every live count is parsed rather
            than copied; the suite is what keeps it that way.
          </p>

          <div className="site-footer__links" data-reveal>
            <img src={MARK} alt="" width={24} height={24} />
            <a href={BASELINE.repository}>Source</a>
            <a href={permalink('docs/ux-rules.md')}>Rulebook</a>
            <a href={permalink('TESTING.md')}>How it is tested</a>
            <a href={permalink(PERFORMANCE_RECORD)}>Measurements</a>
          </div>
        </details>
        <details className="evidence-details">
          <summary>From idea to alpha · {TIMELINE.length} weeks</summary>
          <TempoBand />
        </details>
      </div>
    </footer>
  );
}
