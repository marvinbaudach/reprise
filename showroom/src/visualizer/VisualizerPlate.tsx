import { useEffect, useRef } from 'react';
import { observeSceneActivity } from './activity.ts';
import { drawBars } from './bars.ts';
import { BYTES_PER_FRAME, FRAMES_PER_SECOND, VisualizerEngine } from './engine.ts';

const TRACK_URL = `${import.meta.env.BASE_URL}media/showroom/visualizer-track.bin`;
const FRAME_INTERVAL_MS = 1_000 / FRAMES_PER_SECOND;
const MAX_CATCH_UP_FRAMES = 4;

function fitCanvasToPlate(canvas: HTMLCanvasElement): void {
  // Keep the segmented geometry legible in tiny hero plates; bound zoom cost.
  const height = Math.max(128, Math.min(360, Math.round(canvas.clientHeight)));
  const width = Math.max(
    1,
    Math.round((height * canvas.clientWidth) / Math.max(1, canvas.clientHeight)),
  );
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
}

export function VisualizerPlate({ variant = 'phone' }: { readonly variant?: 'phone' | 'desktop' }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext('2d');
    if (!canvas || !context) return undefined;
    const engine = new VisualizerEngine();
    const abortController = new AbortController();
    let track: Uint8Array | undefined;
    let frameIndex = 0;
    let active = false;
    let animationFrame: number | undefined;
    let previousTimestamp: number | undefined;
    let accumulatedMs = 0;

    const advance = () => {
      if (!track) return;
      engine.ingest(track, frameIndex * BYTES_PER_FRAME);
      engine.tick();
      frameIndex = (frameIndex + 1) % (track.length / BYTES_PER_FRAME);
    };
    const draw = () => {
      fitCanvasToPlate(canvas);
      drawBars(
        context,
        canvas.width,
        canvas.height,
        engine.frame(),
        variant === 'desktop' ? '#242a2f' : '#000',
      );
    };
    const stopAnimation = () => {
      if (animationFrame !== undefined) window.cancelAnimationFrame(animationFrame);
      animationFrame = undefined;
      previousTimestamp = undefined;
      accumulatedMs = 0;
    };
    const animate = (timestamp: number) => {
      animationFrame = undefined;
      if (!active || !track) return;
      accumulatedMs +=
        previousTimestamp === undefined
          ? FRAME_INTERVAL_MS
          : Math.min(timestamp - previousTimestamp, FRAME_INTERVAL_MS * MAX_CATCH_UP_FRAMES);
      let changed = false;
      while (accumulatedMs >= FRAME_INTERVAL_MS) {
        advance();
        accumulatedMs -= FRAME_INTERVAL_MS;
        changed = true;
      }
      // Catch up the simulation without painting several times in one browser frame.
      if (changed) draw();
      previousTimestamp = timestamp;
      animationFrame = window.requestAnimationFrame(animate);
    };
    const synchronizePlayback = () => {
      stopAnimation();
      if (track && active) animationFrame = window.requestAnimationFrame(animate);
    };
    const stopObserving = observeSceneActivity(canvas, (playing) => {
      if (active === playing) return;
      active = playing;
      synchronizePlayback();
    });
    void fetch(TRACK_URL, { signal: abortController.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(`visualizer track request failed: ${response.status}`);
        return new Uint8Array(await response.arrayBuffer());
      })
      .then((loadedTrack) => {
        if (loadedTrack.length === 0 || loadedTrack.length % BYTES_PER_FRAME !== 0) {
          throw new Error('visualizer track has an incomplete frame');
        }
        track = loadedTrack;
        advance();
        draw();
        synchronizePlayback();
      })
      .catch((error: unknown) => {
        if (!abortController.signal.aborted) console.error(error);
      });
    return () => {
      abortController.abort();
      stopAnimation();
      stopObserving();
    };
  }, [variant]);

  return (
    <span aria-hidden="true">
      <canvas
        ref={canvasRef}
        className={variant === 'desktop' ? 'desktop-scene__visualizer' : 'hero-product__visualizer'}
        data-showcase="visualizer-plate"
      />
    </span>
  );
}
