import { useEffect, useRef } from 'react';
import { observeSceneActivity } from '../../visualizer/activity';
import { VisualizerPlate } from '../../visualizer/VisualizerPlate';
import './desktop-scene.css';

/** A presentation loop over the supplied capture, not live app playback. */
export function DesktopScene() {
  const sceneRef = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return;
    return observeSceneActivity(scene, (active) => {
      scene.dataset.active = String(active);
    });
  }, []);

  return (
    <span ref={sceneRef} className="desktop-scene" aria-hidden="true" data-active="false">
      <span className="desktop-scene__atmosphere">
        <span className="desktop-scene__disc" />
      </span>
      <span className="desktop-scene__cover" />
      <VisualizerPlate variant="desktop" />
    </span>
  );
}
