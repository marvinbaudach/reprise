import './chrome.css';

/**
 * A two-pixel spectrum across the top edge, filled by the reading position.
 *
 * It quotes the app's seek bar — coral through violet to teal is the same axis
 * `spectral_colour.rs` walks — so the page's own progress is rendered in the
 * language the product uses for a track's progress.
 */
export function ScrollProgress() {
  return <div id="scroll-progress" className="scroll-progress" aria-hidden="true" />;
}
