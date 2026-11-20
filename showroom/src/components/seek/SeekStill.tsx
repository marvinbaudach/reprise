import { useEffect, useRef } from 'react';
import { createSeekRenderer } from '../../lib/seekRenderer';
import { useMeasuredSeekTrack } from './MeasuredSeekTrack';
import './measured-seek-track.css';

/**
 * Where the still rendering holds its playhead. Well past the middle, most bars
 * show the spectral colour they earn once played, and the rest show the unplayed
 * grey the apps draw ahead of the playhead.
 */
const STILL_POSITION = 0.68;

/** What a screen reader hears: the position the drawing holds, said as drawn. */
const PLAYED_PERCENT = Math.round(STILL_POSITION * 100);

/**
 * The spectral seek bar at rest: the measured track drawn once by the same
 * renderer the interactive bar uses, with no clock, no pulse and no pointer.
 * It redraws only when its width or the screen's pixel density changes, and it
 * never joins the page's shared animation frame, so nothing on it moves.
 */
export function SeekStill() {
  const state = useMeasuredSeekTrack();
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || state.status !== 'ready') return undefined;
    const renderer = createSeekRenderer({
      canvas,
      track: state.track,
      hero: false,
      onSample: () => undefined,
    });
    renderer.scrubTo(STILL_POSITION);
    const draw = () => renderer.draw(performance.now(), true);
    draw();
    const resize = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(draw);
    resize?.observe(canvas);

    // Dragging the window to a screen of another density changes the canvas's
    // pixel size without changing its CSS size, which a ResizeObserver never
    // sees. A resolution query matches one density only, so it is re-armed for
    // the new one every time it fires.
    let density: MediaQueryList | null = null;
    const onDensityChange = () => {
      draw();
      watchDensity();
    };
    function watchDensity() {
      density?.removeEventListener('change', onDensityChange);
      density =
        typeof window.matchMedia === 'function'
          ? window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
          : null;
      density?.addEventListener('change', onDensityChange);
    }
    watchDensity();

    return () => {
      resize?.disconnect();
      density?.removeEventListener('change', onDensityChange);
    };
  }, [state]);

  const ready = state.status === 'ready';

  return (
    <figure className="seek-still" data-showcase="seek-still">
      {state.status === 'failed' ? (
        <p className="seek-track__unavailable">Measured track unavailable.</p>
      ) : (
        <canvas
          ref={canvasRef}
          className="seek-still__canvas"
          role="img"
          aria-label={
            ready
              ? `The spectral seek bar for one measured track, ${PLAYED_PERCENT} percent played`
              : 'The spectral seek bar for one measured track, loading'
          }
        />
      )}
      {ready && (
        <figcaption className="seek-still__caption">
          Bar height is the level and colour the frequency, from coral for low and weighty to teal
          for high and airy.
        </figcaption>
      )}
    </figure>
  );
}
