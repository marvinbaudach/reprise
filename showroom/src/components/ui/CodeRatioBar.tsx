import { CODE_SEGMENTS } from '../../data/measurements';
import './ui.css';

// The bar is proportional through flex-grow, so the segments carry the ratio
// without anyone computing a percentage that could drift from the numbers.
const formatter = new Intl.NumberFormat('de-CH');

/**
 * One bar across the full width, segmented by where the lines actually sit.
 *
 * The point is the caption, not the bar: the Android frontend cost what it cost
 * because it did not have to write the core a second time.
 */
export function CodeRatioBar() {
  return (
    <figure className="ratio">
      <div
        className="ratio__bar"
        role="img"
        aria-label={CODE_SEGMENTS.map(
          (segment) => `${segment.label}: ${formatter.format(segment.lines)} lines`,
        ).join(', ')}
      >
        {CODE_SEGMENTS.map((segment) => (
          <span
            key={segment.key}
            className={`ratio__segment ratio__segment--${segment.key}`}
            style={{ flexGrow: segment.lines }}
          />
        ))}
      </div>

      <ul className="ratio__legend">
        {CODE_SEGMENTS.map((segment) => (
          <li className="ratio__row" key={segment.key}>
            <span className={`ratio__swatch ratio__segment--${segment.key}`} aria-hidden="true" />
            <span className="ratio__lines">{formatter.format(segment.lines)}</span>
            <span className="ratio__label">{segment.label}</span>
            <span className="ratio__note data">{segment.note}</span>
          </li>
        ))}
      </ul>

      <figcaption className="prose ratio__caption">
        The Android frontend cost <strong>30&apos;922 lines</strong> — {formatter.format(20_677)}{' '}
        Kotlin plus {formatter.format(10_245)} lines of bridge — because it did not have to write
        the {formatter.format(177_661)} lines of core a second time. That is the whole argument for
        a core that carries no interface: the second platform has a price tag, and the third one has
        a price tag instead of a promise.
      </figcaption>
    </figure>
  );
}
