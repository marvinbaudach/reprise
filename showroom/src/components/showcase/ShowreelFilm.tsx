import { useCallback, useRef, useState } from 'react';
import './showreel.css';

const BASE_URL = import.meta.env?.BASE_URL ?? '/reprise/';
// This is the runtime URL where the encodes must be served when the film is mounted.
const FILM_BASE = `${BASE_URL}media/showreel/`;

/**
 * Below this width the 1080 ladder is wasted on the layout, so the smaller
 * step is offered first. `media` is read once when the element picks a source,
 * which is why the breakpoint matches the layout's own and not a device class.
 * The 900px value must stay equal to showreel.css's responsive breakpoint.
 */
const SMALL_VIEWPORT = '(max-width: 900px)';

/**
 * The film beside the screenshot gallery.
 *
 * It costs nothing until the reader asks it to play, and every start comes from
 * a click — which is what lets it play with sound, since a browser grants an
 * unmuted start only to a user gesture. It runs once and stays on its last
 * frame; the play button then offers it again from the top.
 */
export function ShowreelFilm() {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(false);
  const [ended, setEnded] = useState(false);

  /**
   * Only the autoplay policy is worth a second attempt, and only once: a start
   * the browser refused over the sound is still a start the reader asked for,
   * so it is repeated muted. Any other rejection — no playable source, a
   * torn-down element — would fail again just as quietly.
   */
  const start = useCallback((video: HTMLVideoElement) => {
    // Sitting on the last frame, the only sensible start is from the top.
    if (video.ended) video.currentTime = 0;
    void video.play().catch((error: unknown) => {
      const blockedOverSound = error instanceof DOMException && error.name === 'NotAllowedError';
      if (!blockedOverSound || video.muted) return;
      video.muted = true;
      void video.play().catch(() => undefined);
    });
  }, []);

  const toggle = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    if (video.paused) {
      start(video);
    } else {
      video.pause();
    }
  }, [start]);

  const toggleSound = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    const next = !video.muted;
    video.muted = next;
    // Turning the sound on is the clearest statement of intent there is, so it
    // also starts the film if it happens to be sitting still.
    if (!next && video.paused) start(video);
  }, [start]);

  // Having run out is a state of its own: the button stops offering a Play the
  // reader has already had and offers the film again instead.
  const playIcon = playing ? '❙❙' : ended ? '↻' : '▶';
  const playLabel = playing ? 'Pause' : ended ? 'Watch again' : 'Play';

  return (
    <div className="frame film-frame">
      <header className="film-heading">
        <h3 id="film-heading" data-reveal>
          Fifty-eight seconds. The desktop, the phone, and the sync between them.
        </h3>
        <p data-reveal>Screen recordings of the running apps — nothing rebuilt for the camera</p>
      </header>

      <figure className="film" data-showcase="showreel-film" aria-labelledby="film-heading">
        {/* biome-ignore lint/a11y/useMediaCaption: The film has no dialogue — it is screen
            recordings over music, and the figcaption below is its text equivalent. The WebVTT
            track and its CC toggle went unused and were removed rather than left to rot. */}
        <video
          ref={videoRef}
          className="film__video"
          poster={`${FILM_BASE}showreel-poster.webp`}
          preload="none"
          playsInline
          width={1920}
          height={1080}
          onPlay={() => {
            setPlaying(true);
            setEnded(false);
          }}
          onPause={() => setPlaying(false)}
          onEnded={() => setEnded(true)}
          onVolumeChange={(event) => setMuted(event.currentTarget.muted)}
        >
          <source src={`${FILM_BASE}showreel-720.webm`} type="video/webm" media={SMALL_VIEWPORT} />
          <source src={`${FILM_BASE}showreel-1080.webm`} type="video/webm" />
          <source src={`${FILM_BASE}showreel-720.mp4`} type="video/mp4" media={SMALL_VIEWPORT} />
          <source src={`${FILM_BASE}showreel-1080.mp4`} type="video/mp4" />
        </video>

        <div className="film__controls">
          <button type="button" className="film__control" onClick={toggle}>
            <span aria-hidden="true">{playIcon}</span>
            {playLabel}
          </button>
          <button type="button" className="film__control" onClick={toggleSound}>
            <span aria-hidden="true">{muted ? '🔊' : '🔇'}</span>
            {muted ? 'Sound on' : 'Sound off'}
          </button>
        </div>

        <figcaption className="film__caption">
          Podcasts, YouTube channels, new releases, concerts nearby and your listening counted —
          then the library synced to the phone over MTP, and the same visualizer running on Android.
        </figcaption>
      </figure>
    </div>
  );
}
