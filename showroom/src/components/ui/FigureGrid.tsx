import type { Figure } from '../../data/measurements';
import './ui.css';

/**
 * Figures are set in the display face, not hidden in monospace. A measured
 * number is the loudest thing this page has to say, so it gets the loudest type.
 */
export function FigureGrid({
  figures,
  variant,
}: {
  figures: readonly Figure[];
  variant: 'headline' | 'rulebook';
}) {
  return (
    <dl className={`figure-grid figure-grid--${variant}`}>
      {figures.map((figure) => (
        <div className="figure-grid__cell" data-reveal key={figure.label}>
          <dt className="figure-value">
            <span data-counter={figure.counter ? '' : undefined}>{figure.value}</span>
          </dt>
          <dd className="figure-grid__label">
            <span>{figure.href ? <a href={figure.href}>{figure.label}</a> : figure.label}</span>
            {figure.detail ? (
              <span className="data figure-grid__detail">{figure.detail}</span>
            ) : null}
          </dd>
        </div>
      ))}
    </dl>
  );
}
