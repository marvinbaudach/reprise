import type { Figure } from '../../data/measurements';
import './ui.css';

/**
 * Figures are set in the display face, not hidden in monospace. A measured
 * number is the loudest thing this page has to say, so it gets the loudest type.
 */
export function FigureGrid({ figures }: { figures: readonly Figure[] }) {
  return (
    <dl className="figure-grid">
      {figures.map((figure) => (
        <div className="figure-grid__cell" key={figure.label}>
          <dt className="figure-value">
            {figure.href ? (
              <a className="figure-grid__link" href={figure.href}>
                {figure.value}
              </a>
            ) : (
              figure.value
            )}
          </dt>
          <dd className="figure-grid__label">
            <span className="eyebrow">{figure.label}</span>
            {figure.detail ? <span className="data figure-grid__detail">{figure.detail}</span> : null}
          </dd>
        </div>
      ))}
    </dl>
  );
}
