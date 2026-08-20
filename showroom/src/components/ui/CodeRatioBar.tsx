import { CODE_SEGMENTS, group } from '../../data/measurements';
import './ui.css';

/**
 * One bar across the full width, segmented by where the lines actually sit.
 *
 * Both the lengths and the labels come from the build's own count of this tree,
 * so the bar cannot end up drawn to shares the legend contradicts.
 */
export function CodeRatioBar() {
  return (
    <figure className="ratio" data-reveal>
      <p className="ratio__heading">Where the lines sit</p>
      <div
        className="ratio__bar"
        data-ratio
        role="img"
        aria-label={CODE_SEGMENTS.map(
          (segment) => `${segment.label}: ${group(segment.lines)} lines`,
        ).join(', ')}
      >
        {CODE_SEGMENTS.map((segment) => (
          <span
            key={segment.key}
            className={`ratio__segment ratio__segment--${segment.key}`}
            data-w={segment.share}
            title={segment.label}
          />
        ))}
      </div>

      <ul className="ratio__legend">
        {CODE_SEGMENTS.map((segment) => (
          <li className="ratio__row" key={segment.key}>
            <span className="ratio__label">
              <span className={`ratio__swatch ratio__segment--${segment.key}`} aria-hidden="true" />
              {segment.label} · {group(segment.lines)}
            </span>
            <span className="ratio__note">{segment.note}</span>
          </li>
        ))}
      </ul>
    </figure>
  );
}
