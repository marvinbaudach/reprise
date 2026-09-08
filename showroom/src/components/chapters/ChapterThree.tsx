import { SpectralSeekTrack } from '../seek/MeasuredSeekTrack';
import { ShowreelFilm } from '../showcase/ShowreelFilm';
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
          CH.03
        </p>
        <h2 id="ch-03-heading" className="chapter__title" data-reveal>
          Two frameworks. One visual signature.
        </h2>

        <p className="chapter__intro" data-reveal>
          <strong>The two apps look different on purpose.</strong> GNOME conventions on the desktop,
          Material on the phone. Making them match would not show craft, it would show missing
          platform UX. What is shared is the signature — and that is the harder half: separate
          rendering stacks, GSK against Skia, different layout systems, different languages, the
          same visualisation and the same physics. Not a shared component. A shared specification.
        </p>

        <p className="chapter__intro chapter__intro--closing" data-reveal>
          The seek bar is the case in point. The decision: show the structure of the track instead
          of an empty gutter. The implementation: a portable visuals layer that neither frontend
          owns. The result: physics that were measured afterwards rather than asserted.
        </p>

        <SpectralSeekTrack reducedMotion={reducedMotion} />
      </div>

      <ShowreelFilm />
    </section>
  );
}
