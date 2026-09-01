import { useCallback, useEffect, useRef, useState } from 'react';
import {
  createSeekRenderer,
  registerSeekRenderer,
  requestSeekFrame,
  type SeekSample,
} from '../../lib/seekRenderer';
import { formatSeekTime, loadSeekTrack, type MeasuredSeekTrack } from '../../lib/seekTrack';
import './measured-seek-track.css';

interface ReadyTrack {
  readonly status: 'ready';
  readonly track: MeasuredSeekTrack;
}

interface PendingTrack {
  readonly status: 'loading' | 'failed';
}

type TrackState = ReadyTrack | PendingTrack;

interface SeekCanvasProps {
  readonly hero?: boolean;
  /** Names the control for anyone who cannot see the bar it draws. */
  readonly label: string;
  readonly onSample: (sample: SeekSample) => void;
  readonly reducedMotion: boolean;
  readonly state: TrackState;
}

function useMeasuredSeekTrack(): TrackState {
  const [state, setState] = useState<TrackState>({ status: 'loading' });
  useEffect(() => {
    let current = true;
    loadSeekTrack().then(
      (track) => {
        if (current) setState({ status: 'ready', track });
      },
      () => {
        if (current) setState({ status: 'failed' });
      },
    );
    return () => {
      current = false;
    };
  }, []);
  return state;
}

/** Arrow keys move by five seconds, Page keys by half a minute. */
const SEEK_STEP_MS = 5_000;
const SEEK_PAGE_MS = 30_000;

function SeekCanvas({ hero = false, label, onSample, reducedMotion, state }: SeekCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const frameRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const frame = frameRef.current;
    if (!canvas || !frame || state.status !== 'ready') return undefined;
    const durationMs = state.track.durationMs;
    let visible = typeof IntersectionObserver === 'undefined';
    let announced = -1;

    // The readout follows the pointer on the wide bar, so what the slider
    // reports is the playhead itself, never the value under the cursor. Only
    // whole seconds are written back: assistive technology is read to a person,
    // and sixty announcements a second is noise, not information.
    const publish = (sample: SeekSample) => {
      const seconds = Math.round(renderer.position() * (durationMs / 1_000));
      if (seconds !== announced) {
        announced = seconds;
        frame.setAttribute('aria-valuenow', String(seconds));
        frame.setAttribute(
          'aria-valuetext',
          `${formatSeekTime(seconds * 1_000)} of ${formatSeekTime(durationMs)}`,
        );
      }
      onSample(sample);
    };

    const renderer = createSeekRenderer({
      canvas,
      track: state.track,
      hero,
      onSample: publish,
    });

    const drawNow = () => renderer.draw(performance.now(), reducedMotion);

    const drawPointer = (event: PointerEvent) => {
      if (hero) return;
      renderer.setHover(renderer.positionAt(event.clientX));
      drawNow();
    };
    const clearPointer = () => {
      if (hero) return;
      renderer.setHover(null);
      drawNow();
    };

    let dragging = false;
    const seekFromPointer = (event: PointerEvent) => {
      renderer.scrubTo(renderer.positionAt(event.clientX));
      requestSeekFrame();
      drawNow();
    };
    const startDrag = (event: PointerEvent) => {
      if (event.button > 0) return;
      // Without this the browser starts a text selection or an image drag as
      // soon as the pointer moves, and the scrub turns into a drag of the page.
      event.preventDefault();
      frame.focus({ preventScroll: true });
      dragging = true;
      frame.dataset.dragging = 'true';
      frame.setPointerCapture(event.pointerId);
      seekFromPointer(event);
    };
    const moveDrag = (event: PointerEvent) => {
      if (dragging) seekFromPointer(event);
    };
    const endDrag = (event: PointerEvent) => {
      if (!dragging) return;
      dragging = false;
      delete frame.dataset.dragging;
      if (frame.hasPointerCapture(event.pointerId)) frame.releasePointerCapture(event.pointerId);
      renderer.releaseScrub();
      requestSeekFrame();
      drawNow();
    };

    const KEY_MOVES: Record<string, number> = {
      ArrowLeft: -SEEK_STEP_MS,
      ArrowDown: -SEEK_STEP_MS,
      ArrowRight: SEEK_STEP_MS,
      ArrowUp: SEEK_STEP_MS,
      PageDown: -SEEK_PAGE_MS,
      PageUp: SEEK_PAGE_MS,
    };
    const onKeyDown = (event: KeyboardEvent) => {
      const move = KEY_MOVES[event.key];
      const target =
        event.key === 'Home'
          ? 0
          : event.key === 'End'
            ? 1
            : move === undefined
              ? null
              : renderer.position() + move / durationMs;
      if (target === null) return;
      event.preventDefault();
      renderer.scrubTo(target);
      renderer.releaseScrub();
      requestSeekFrame();
      drawNow();
    };

    canvas.addEventListener('pointermove', drawPointer, { passive: true });
    canvas.addEventListener('pointerleave', clearPointer, { passive: true });
    frame.addEventListener('pointerdown', startDrag);
    frame.addEventListener('pointermove', moveDrag);
    frame.addEventListener('pointerup', endDrag);
    frame.addEventListener('pointercancel', endDrag);
    frame.addEventListener('keydown', onKeyDown);

    const observer =
      typeof IntersectionObserver === 'undefined'
        ? null
        : new IntersectionObserver((entries) => {
            visible = entries.some((entry) => entry.isIntersecting);
            requestSeekFrame();
          });
    observer?.observe(canvas);
    const unregister = registerSeekRenderer({
      draw: (timestamp, still) => renderer.draw(timestamp, still),
      isVisible: () => visible,
    });
    return () => {
      canvas.removeEventListener('pointermove', drawPointer);
      canvas.removeEventListener('pointerleave', clearPointer);
      frame.removeEventListener('pointerdown', startDrag);
      frame.removeEventListener('pointermove', moveDrag);
      frame.removeEventListener('pointerup', endDrag);
      frame.removeEventListener('pointercancel', endDrag);
      frame.removeEventListener('keydown', onKeyDown);
      observer?.disconnect();
      unregister();
    };
  }, [hero, onSample, reducedMotion, state]);

  if (state.status === 'failed') {
    return <p className="seek-track__unavailable">Measured track unavailable.</p>;
  }

  // The strip only becomes a control once the measured track is in hand. Until
  // then there is nothing to seek through, and a prerendered document that
  // offers a focusable slider which cannot move is worse than one that offers
  // none: it takes a tab stop and answers nothing.
  const durationS = state.status === 'ready' ? Math.round(state.track.durationMs / 1_000) : 0;
  const control =
    state.status === 'ready'
      ? ({
          role: 'slider',
          tabIndex: 0,
          'aria-label': label,
          'aria-orientation': 'horizontal',
          'aria-valuemin': 0,
          'aria-valuemax': durationS,
          'aria-valuenow': 0,
          'aria-valuetext': `0:00 of ${formatSeekTime(durationS * 1_000)}`,
        } as const)
      : {};

  return (
    <span
      ref={frameRef}
      className={`seek-track__canvas-frame${hero ? ' seek-track__canvas-frame--hero' : ''}`}
      {...control}
    >
      {/* `slider` is a leaf role: the canvas under it is never announced, and
          marking it hidden as well would only hide a focusable element. */}
      <canvas ref={canvasRef} className="seek-track__canvas" data-seek-canvas="" />
    </span>
  );
}

function remainingText(state: TrackState): string {
  return state.status === 'ready' ? `−${formatSeekTime(state.track.durationMs)}` : '−—:—';
}

export function HeroSeekTrack({ reducedMotion }: { readonly reducedMotion: boolean }) {
  const state = useMeasuredSeekTrack();
  const elapsedRef = useRef<HTMLSpanElement>(null);
  const remainingRef = useRef<HTMLSpanElement>(null);
  const updateSample = useCallback((sample: SeekSample) => {
    if (elapsedRef.current) elapsedRef.current.textContent = formatSeekTime(sample.elapsedMs);
    if (remainingRef.current) {
      remainingRef.current.textContent = `−${formatSeekTime(sample.remainingMs)}`;
    }
  }, []);

  return (
    <div className="hero-seek" data-reveal="" data-showcase="hero-seek-track">
      <div className="hero-seek__bar">
        <img
          className="hero-seek__mark"
          src={`${import.meta.env.BASE_URL}brand/reprise-mark.svg`}
          alt=""
          width="24"
          height="24"
        />
        <span ref={elapsedRef} className="seek-track__time">
          0:00
        </span>
        <SeekCanvas
          hero
          label="Seek through the measured track"
          onSample={updateSample}
          reducedMotion={reducedMotion}
          state={state}
        />
        <span ref={remainingRef} className="seek-track__time">
          {remainingText(state)}
        </span>
      </div>
      <p className="hero-seek__caption">
        <span>The seek bar, shaped by reprise-view</span>
        <a href="#ch-03">What the colours mean ↓</a>
      </p>
    </div>
  );
}

export function SpectralSeekTrack({ reducedMotion }: { readonly reducedMotion: boolean }) {
  const state = useMeasuredSeekTrack();
  const elapsedRef = useRef<HTMLSpanElement>(null);
  const remainingRef = useRef<HTMLSpanElement>(null);
  const colourRef = useRef<HTMLSpanElement>(null);
  const levelRef = useRef<HTMLSpanElement>(null);
  const swatchRef = useRef<HTMLSpanElement>(null);
  const updateSample = useCallback((sample: SeekSample) => {
    if (elapsedRef.current) elapsedRef.current.textContent = formatSeekTime(sample.elapsedMs);
    if (remainingRef.current) {
      remainingRef.current.textContent = `−${formatSeekTime(sample.remainingMs)}`;
    }
    if (colourRef.current) colourRef.current.textContent = `centroid ${sample.centroid.toFixed(2)}`;
    if (levelRef.current) levelRef.current.textContent = `level ${sample.level.toFixed(2)}`;
    if (swatchRef.current) {
      const [red, green, blue] = sample.colour;
      swatchRef.current.style.backgroundColor = `rgb(${red * 255} ${green * 255} ${blue * 255})`;
    }
  }, []);

  return (
    <figure className="seek-card" data-reveal="" data-showcase="spectral-seek-track">
      <div className="seek-card__heading">
        <div>
          <p>The spectral seek bar, live</p>
          <h3>Height is the level. Colour is the frequency.</h3>
        </div>
      </div>

      <div className="seek-card__canvas">
        <SeekCanvas
          label="Seek through the spectral track"
          onSample={updateSample}
          reducedMotion={reducedMotion}
          state={state}
        />
      </div>

      <div className="seek-readout" aria-live="off">
        <span ref={elapsedRef}>0:00</span>
        <span className="seek-readout__measurements">
          <span>
            <span ref={swatchRef} className="seek-readout__swatch" aria-hidden="true" />
            <span ref={colourRef}>centroid 0.00</span>
          </span>
          <span ref={levelRef}>level 0.00</span>
        </span>
        <span ref={remainingRef}>{remainingText(state)}</span>
      </div>

      <p className="seek-card__note" data-reveal="">
        Move across the measured track to inspect its values. The bars are shaped by the same
        functions the apps use —{' '}
        <a href="https://github.com/marvinbaudach/reprise/blob/main/crates/reprise-core/src/visuals/modes/bars.rs">
          bars.rs
        </a>
        ,{' '}
        <a href="https://github.com/marvinbaudach/reprise/blob/main/crates/reprise-view/src/waveform.rs">
          waveform.rs
        </a>{' '}
        and{' '}
        <a href="https://github.com/marvinbaudach/reprise/blob/main/crates/reprise-view/src/spectral_colour.rs">
          spectral_colour.rs
        </a>{' '}
        — with only the band values standing in for live PCM.
      </p>

      <div className="seek-legends">
        <article data-reveal="" data-seek-legend="height">
          <h4>Height — the body</h4>
          <p>
            Every bar is the RMS of its slice, mapped through the track's own p10–p95 window and
            smoothed against flicker. A compressed master still shows verse against chorus instead
            of one loud wall.
          </p>
        </article>
        <article data-reveal="" data-seek-legend="colour">
          <h4>Colour — the frequency</h4>
          <p>The tint is the spectral centroid: coral is low and weighty, teal high and airy.</p>
          <div className="seek-legends__axis" aria-hidden="true" />
          <div className="seek-legends__ends">
            <span>#FF6F5E · low</span>
            <span>high · #4FDBD4</span>
          </div>
        </article>
      </div>
    </figure>
  );
}
