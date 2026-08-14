import './chrome.css';

const MARKS: readonly { id: string; label: string }[] = [
  { id: 'ch-01', label: 'CH.01' },
  { id: 'ch-02', label: 'CH.02' },
  { id: 'ch-03', label: 'CH.03' },
];

/**
 * The signature gesture: a two-pixel line that fills as the page scrolls and
 * carries a glowing head. Touch it and it grows into a spectrum with the chapter
 * marks, then retreats.
 *
 * It quotes the app's seek bar, which shows a track's structure instead of an
 * empty gutter. The fill runs on a CSS scroll timeline, so it lives in the
 * compositor and never touches the main thread — the same argument the app makes
 * about its own idle frame clock.
 */
export function ScrollHairline() {
  return (
    <div className="hairline" aria-hidden="false">
      <nav className="hairline__marks" aria-label="Chapters">
        {MARKS.map((mark) => (
          <a key={mark.id} className="hairline__mark" href={`#${mark.id}`}>
            {mark.label}
          </a>
        ))}
      </nav>
      <div className="hairline__track">
        <div className="hairline__fill" />
        <div className="hairline__head" />
      </div>
    </div>
  );
}
