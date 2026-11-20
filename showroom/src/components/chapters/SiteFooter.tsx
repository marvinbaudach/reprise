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
        <details className="evidence-details">
          <summary>From idea to alpha · {TIMELINE.length} weeks</summary>
          <TempoBand />
        </details>
        <details className="evidence-details">
          <summary>Sources and methodology</summary>
          <p className="site-footer__honesty" data-reveal>
            Three kinds of number appear on this page. <strong>Counted:</strong> read from this
            repository at build time, where{' '}
            <a href={permalink(CENSUS_SCOPE.source)}>
              <code>code-census.mjs</code>
            </a>{' '}
            walks {group(CENSUS_SCOPE.files)} files. <strong>Quoted:</strong> from records that
            carry commit, date and method, namely{' '}
            <a href={permalink(PERFORMANCE_RECORD)}>
              <code>index-rebuild.md</code>
            </a>{' '}
            and{' '}
            <a href={permalink(INCIDENT_RECORD)}>
              <code>queue-anchor-grill-followups.md</code>
            </a>
            . <strong>Stated:</strong> &ldquo;1 → 4&rdquo; is an architectural claim; the four
            frontends are its evidence.
          </p>

          <div className="site-footer__links" data-reveal>
            <img src={MARK} alt="" width={24} height={24} />
            <a href={BASELINE.repository}>Source</a>
            <a href={permalink('docs/ux-rules.md')}>Rulebook</a>
            <a href={permalink('TESTING.md')}>How it is tested</a>
            <a href={permalink(PERFORMANCE_RECORD)}>Measurements</a>
          </div>
        </details>
        <div className="colophon" data-reveal>
          <a className="colophon__source" href="https://github.com/marvinbaudach">
            github.com/marvinbaudach ↗
          </a>
          <p>GPL-3.0-or-later · beta</p>
        </div>
      </div>
    </footer>
  );
}
