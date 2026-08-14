import { BASELINE, treelink } from '../../data/measurements';
import { ThemeToggle } from '../theme/ThemeToggle';
import './chrome.css';

export function SiteHeader() {
  return (
    <header className="site-header frame">
      <div className="site-header__id">
        <span className="site-header__wordmark">Reprise</span>
        <span className="eyebrow site-header__state">Alpha</span>
      </div>

      <div className="site-header__meta">
        <a className="data" href={treelink()}>
          counted on {BASELINE.commit}
        </a>
        <ThemeToggle />
      </div>
    </header>
  );
}
