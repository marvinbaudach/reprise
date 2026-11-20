import type { Figure } from '../../data/measurements';
import './ui.css';

/**
 * The census figures sit side by side, so they are set in Martian Mono at
 * display size: tabular digits line up across the cells, and the size, not the
 * face, makes them the loudest thing in the disclosure.
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
