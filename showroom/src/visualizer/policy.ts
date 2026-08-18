export interface VisualizerPlaybackState {
  readonly reducedMotion: boolean;
  readonly intersecting: boolean;
}

export function shouldPlay({ reducedMotion, intersecting }: VisualizerPlaybackState): boolean {
  return !reducedMotion && intersecting;
}
