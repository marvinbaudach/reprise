import { permalink, SPECTRAL_AXIS } from '../../data/measurements';
import './ui.css';

/**
 * The seek bar's colour ramp.
 *
 * `spectral_colour.rs` walks OKLCH with a falling hue, which is why the ramp runs
 * CORAL → magenta → violet → blue → TEAL and not the short way round through
 * green. `in oklch longer hue` walks the same arc, so the bar is the function's
 * path rather than a decorative gradient between two brand colours.
 */
export function SpectralAxis() {
  return (
    <figure className="axis">
      <div
        className="axis__ramp"
        role="img"
        aria-label="The seek bar ramp, coral through violet to teal"
      />
      <div className="axis__legend">
        <span className="data axis__end">
          <span className="axis__dot axis__dot--coral" aria-hidden="true" />
          CORAL {SPECTRAL_AXIS.coral}
        </span>
        <figcaption className="data axis__caption">
          Reprise tints the position bar by the spectral centroid of the playing track. The axis is
          that ramp, walked in OKLCH with a falling hue — the path{' '}
          <a href={permalink(SPECTRAL_AXIS.source)}>spectral_colour.rs</a> takes, not a gradient
          between two brand colours.
        </figcaption>
        <span className="data axis__end axis__end--right">
          {SPECTRAL_AXIS.teal} TEAL
          <span className="axis__dot axis__dot--teal" aria-hidden="true" />
        </span>
      </div>
    </figure>
  );
}
