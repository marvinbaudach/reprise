import { useCallback, useEffect, useRef, useState } from 'react';
import {
  createSeekRenderer,
  registerSeekRenderer,
  requestSeekFrame,
  type SeekCanvasRenderer,
  type SeekMode,
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
  readonly mode: SeekMode;
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

function SeekCanvas({ hero = false, mode, onSample, reducedMotion, state }: SeekCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<SeekCanvasRenderer | null>(null);
  const modeRef = useRef(mode);
  modeRef.current = mode;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || state.status !== 'ready') return undefined;
    let visible = typeof IntersectionObserver === 'undefined';
    const renderer = createSeekRenderer({
      canvas,
      track: state.track,
      mode: modeRef.current,
      hero,
      onSample,
    });
    rendererRef.current = renderer;
    const drawPointer = (event: PointerEvent) => {
      if (hero) return;
      const bounds = canvas.getBoundingClientRect();
      renderer.setHover((event.clientX - bounds.left) / bounds.width);
      renderer.draw(performance.now(), reducedMotion);
    };
    const clearPointer = () => {
      if (hero) return;
      renderer.setHover(null);
      renderer.draw(performance.now(), reducedMotion);
    };
    canvas.addEventListener('pointermove', drawPointer, { passive: true });
    canvas.addEventListener('pointerleave', clearPointer, { passive: true });
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
      observer?.disconnect();
      unregister();
      rendererRef.current = null;
    };
  }, [hero, onSample, reducedMotion, state]);

  useEffect(() => {
    rendererRef.current?.setMode(mode);
    requestSeekFrame();
  }, [mode]);

  if (state.status === 'failed') {
    return <p className="seek-track__unavailable">Measured track unavailable.</p>;
  }

  return (
    <span
      className={`seek-track__canvas-frame${hero ? ' seek-track__canvas-frame--hero' : ''}`}
      aria-hidden="true"
    >
      <canvas
        ref={canvasRef}
        className="seek-track__canvas"
        data-seek-canvas=""
        data-seek-mode={hero ? 'fill' : mode}
      />
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
    <div className="hero-seek" data-reveal="" data-showcase="hero-seek-track" data-seek-mode="fill">
      <div className="hero-seek__bar">
        <img
          className="hero-seek__mark"
          src={`${import.meta.env.BASE_URL}brand/reprise-mark.svg`}
          alt=""
          width="26"
          height="26"
        />
        <span ref={elapsedRef} className="seek-track__time">
          0:00
        </span>
        <SeekCanvas
          hero
          mode="fill"
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
  const [mode, setMode] = useState<SeekMode>('fill');
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
        <fieldset className="seek-modes">
          <legend>Seek bar rendering</legend>
          <button
            type="button"
            data-mode="fill"
            aria-pressed={mode === 'fill'}
            onClick={() => setMode('fill')}
          >
            Spectral fill
          </button>
          <button
            type="button"
            data-mode="marks"
            aria-pressed={mode === 'marks'}
            onClick={() => setMode('marks')}
          >
            One colour + marks
          </button>
        </fieldset>
      </div>

      <div className="seek-card__canvas">
        <SeekCanvas
          mode={mode}
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
        Move across the measured track to inspect its values. The phone in the hero runs the same
        treatment for its Now Playing scene: 64 CAVA columns, 16 segments, peaks and reflections
        from{' '}
        <a href="https://github.com/marvinbaudach/reprise/blob/main/crates/reprise-core/src/visuals/modes/bars.rs">
          bars.rs
        </a>
        , with only the band values standing in for live PCM. The bars below are shaped by the same
        functions the apps use —{' '}
        <a href="https://github.com/marvinbaudach/reprise/blob/main/crates/reprise-view/src/waveform.rs">
          waveform.rs
        </a>{' '}
        for the heights and{' '}
        <a href="https://github.com/marvinbaudach/reprise/blob/main/crates/reprise-view/src/spectral_colour.rs">
          spectral_colour.rs
        </a>{' '}
        for the ramp.
      </p>

      <div className="seek-legends">
        <article data-reveal="" data-seek-legend="height">
          <h4>Height — the body</h4>
          <p>
            Every bar is the RMS of its slice of the track, mapped through the track's own p10–p95
            window and a γ1.6 curve, then smoothed 25/50/25 against flicker. A compressed master
            still shows verse against chorus instead of one loud wall. Anything below −50 dB of the
            track's own maximum renders as a fixed 2 px dot.
          </p>
        </article>
        <article data-reveal="" data-seek-legend="colour">
          <h4>Colour — the frequency</h4>
          <p>
            The tint is the spectral centroid: coral is low and weighty; teal is high and airy. The
            ramp walks OKLCH the long way round, hue falling through magenta, violet and blue.
          </p>
          <div className="seek-legends__axis" aria-hidden="true" />
          <div className="seek-legends__ends">
            <span>#FF6F5E · low</span>
            <span>high · #4FDBD4</span>
          </div>
        </article>
        <article data-reveal="" data-seek-legend="marks">
          <h4>Marks — the sections</h4>
          <p>
            The centroid is averaged over eight seconds first. In one-colour mode, a step of 26 of
            255 across four seconds becomes a hairline, never closer than 20 s to the next one. A
            track with no transitions correctly gets none.
          </p>
        </article>
      </div>
    </figure>
  );
}
