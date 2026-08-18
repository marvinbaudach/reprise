import { useEffect, useRef } from 'react';
import { drawBars } from './bars.ts';
import { BYTES_PER_FRAME, FRAMES_PER_SECOND, VisualizerEngine } from './engine.ts';
import { shouldPlay } from './policy.ts';

const TRACK_URL = `${import.meta.env.BASE_URL}media/showroom/visualizer-track.bin`;
const FRAME_INTERVAL_MS = 1_000 / FRAMES_PER_SECOND;
const MAX_CATCH_UP_FRAMES = 4;

function fitCanvasToPlate(canvas: HTMLCanvasElement): void {
  const width = Math.max(1, Math.round(canvas.clientWidth));
  const height = Math.max(1, Math.round(canvas.clientHeight));
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
}

export function VisualizerPlate() {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext('2d');
    if (!canvas || !context) return undefined;

    const motionPreference = window.matchMedia('(prefers-reduced-motion: reduce)');
    const engine = new VisualizerEngine();
    const abortController = new AbortController();
    let track: Uint8Array | undefined;
    let frameIndex = 0;
    let reducedMotion = motionPreference.matches;
    let intersecting = false;
    let staticFrameDrawn = false;
    let animationFrame: number | undefined;
    let previousTimestamp: number | undefined;
    let accumulatedMs = 0;

    const renderNextFrame = () => {
      if (!track) return;
      fitCanvasToPlate(canvas);
      engine.ingest(track, frameIndex * BYTES_PER_FRAME);
      engine.tick();
      drawBars(context, canvas.width, canvas.height, engine.frame());
      frameIndex = (frameIndex + 1) % (track.length / BYTES_PER_FRAME);
    };

    const stopAnimation = () => {
      if (animationFrame !== undefined) window.cancelAnimationFrame(animationFrame);
      animationFrame = undefined;
      previousTimestamp = undefined;
      accumulatedMs = 0;
    };

    const animate = (timestamp: number) => {
      animationFrame = undefined;
      if (!shouldPlay({ reducedMotion, intersecting })) return;

      if (previousTimestamp === undefined) {
        renderNextFrame();
      } else {
        const elapsed = Math.min(
          timestamp - previousTimestamp,
          FRAME_INTERVAL_MS * MAX_CATCH_UP_FRAMES,
        );
        accumulatedMs += elapsed;
        while (accumulatedMs >= FRAME_INTERVAL_MS) {
          renderNextFrame();
          accumulatedMs -= FRAME_INTERVAL_MS;
        }
      }
      previousTimestamp = timestamp;
      animationFrame = window.requestAnimationFrame(animate);
    };

    const synchronizePlayback = () => {
      stopAnimation();
      if (!track) return;
      if (reducedMotion) {
        if (!staticFrameDrawn) {
          renderNextFrame();
          staticFrameDrawn = true;
        }
        return;
      }
      staticFrameDrawn = false;
      if (intersecting) animationFrame = window.requestAnimationFrame(animate);
    };

    const handleMotionPreference = (event: MediaQueryListEvent) => {
      reducedMotion = event.matches;
      synchronizePlayback();
    };
    motionPreference.addEventListener('change', handleMotionPreference);

    const observer = new IntersectionObserver(([entry]) => {
      intersecting = entry?.isIntersecting ?? false;
      synchronizePlayback();
    });
    observer.observe(canvas);

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
        synchronizePlayback();
      })
      .catch((error: unknown) => {
        if (!abortController.signal.aborted) console.error(error);
      });

    return () => {
      abortController.abort();
      stopAnimation();
      observer.disconnect();
      motionPreference.removeEventListener('change', handleMotionPreference);
    };
  }, []);

  return (
    <span aria-hidden="true">
      <canvas
        ref={canvasRef}
        className="hero-product__visualizer"
        data-showcase="visualizer-plate"
      />
    </span>
  );
}
