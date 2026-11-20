import { SpectralSeekTrack } from '../seek/MeasuredSeekTrack';
import { SeekStill } from '../seek/SeekStill';
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
        <h2 id="ch-03-heading" className="chapter__title" data-reveal>
          The desktop follows GNOME and the phone follows Material, and both draw the same seek bar.
        </h2>

        <p className="chapter__intro" data-reveal>
          The spectral seek bar is the signature they share: the shape of a track, at a glance.
        </p>

        <SeekStill />

        <h3 className="chapter__subhead">Details that make it feel finished</h3>
        <ul className="point-list point-list--flow">
          <li>
            <strong className="point-list__lead">Quick Open</strong> (Ctrl+K) jumps to any track,
            album, artist, playlist or show.
          </li>
          <li>
            The <strong className="point-list__lead">sleep timer</strong> fades out over four
            seconds, then pauses.
          </li>
          <li>
            The <strong className="point-list__lead">mini-player</strong> is a window of its own, so
            the library keeps its size and place.
          </li>
          <li>
            <strong className="point-list__lead">Undo</strong> returns a removed queue track to its
            old place.
          </li>
          <li>
            <strong className="point-list__lead">Android Auto</strong> lets you browse in the car,
            and a widget plays from the home screen.
          </li>
          <li>
            <strong className="point-list__lead">Waveforms</strong> arrive with the sync; the rest
            of the analysis runs on the device while music plays.
          </li>
          <li>
            <strong className="point-list__lead">Artist photos</strong> come from Deezer or from
            credited Commons portraits.
          </li>
        </ul>
        <details className="evidence-details">
          <summary>Explore the interactive seek bar</summary>
          <SpectralSeekTrack reducedMotion={reducedMotion} />
        </details>
      </div>
    </section>
  );
}
