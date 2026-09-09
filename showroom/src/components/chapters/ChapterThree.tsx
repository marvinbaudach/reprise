import { SpectralSeekTrack } from '../seek/MeasuredSeekTrack';
import './chapters.css';

interface ChapterThreeProps {
  readonly reducedMotion: boolean;
}

export function ChapterThree({ reducedMotion }: ChapterThreeProps) {
  return (
    <section
      id="ch-03"
      className="chapter chapter--design"
      data-ground="oklch(12.5% 0.024 302)"
      aria-labelledby="ch-03-heading"
    >
      <div className="frame">
        <p className="chapter__eyebrow" data-reveal>
          Native design
        </p>
        <h2 id="ch-03-heading" className="chapter__title" data-reveal>
          Two frameworks. One visual signature.
        </h2>

        <p className="chapter__intro" data-reveal>
          GNOME conventions on the desktop, Material on the phone. I designed a shared visual
          language that preserves each platform's navigation and interaction patterns. The spectral
          seek bar makes the structure of a track visible on both.
        </p>
        <details className="evidence-details">
          <summary>Explore the interactive seek bar</summary>
          <SpectralSeekTrack reducedMotion={reducedMotion} />
        </details>
      </div>
    </section>
  );
}
