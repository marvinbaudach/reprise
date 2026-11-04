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
          GNOME conventions on the desktop, Material on the phone. The spectral seek bar is the
          signature they share: the shape of a track, at a glance.
        </p>
        <h3 className="chapter__subhead">Details that make it feel finished</h3>
        <ul className="point-list">
          <li>
            <strong className="point-list__lead">Quick Open</strong> — Ctrl+K jumps to any track,
            album, artist, playlist or show.
          </li>
          <li>
            <strong className="point-list__lead">Sleep timer</strong> — fades out over four seconds,
            then pauses.
          </li>
          <li>
            <strong className="point-list__lead">Mini-player</strong> — a window of its own; the
            library keeps its size and place.
          </li>
          <li>
            <strong className="point-list__lead">Undo</strong> — a removed queue track returns to
            its old place.
          </li>
          <li>
            <strong className="point-list__lead">Android Auto and a widget</strong> — browse in the
            car, play from the home screen.
          </li>
          <li>
            <strong className="point-list__lead">On-device analysis</strong> — waveforms arrive with
            the sync, the rest is computed while music plays.
          </li>
          <li>
            <strong className="point-list__lead">Artist photos</strong> — from Deezer, or credited
            Commons portraits; never a stray album cover.
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
