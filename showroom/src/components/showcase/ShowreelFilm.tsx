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
 * It costs nothing until the reader asks it to play. It starts muted because a
 * landing page does not make noise at a reader who did not ask for it.
 */
export function ShowreelFilm() {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(true);
  const [captionsShowing, setCaptionsShowing] = useState(false);

  const toggle = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    if (video.paused) {
      void video.play().catch(() => undefined);
    } else {
      video.pause();
    }
  }, []);

  const toggleSound = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    const next = !video.muted;
    video.muted = next;
    // Turning the sound on is the clearest statement of intent there is, so it
    // also starts the film if it happens to be sitting still.
    if (!next && video.paused) void video.play().catch(() => undefined);
  }, []);

  const toggleCaptions = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    const track = video.textTracks[0];
    if (!track) return;
    const next = track.mode !== 'showing';
    track.mode = next ? 'showing' : 'hidden';
    setCaptionsShowing(next);
  }, []);

  return (
    <div className="frame film-frame">
      <header className="film-heading">
        <h3 id="film-heading" data-reveal>
          Fifty-eight seconds. The desktop, the phone, and the sync between them.
        </h3>
        <p data-reveal>Screen recordings of the running apps — nothing rebuilt for the camera</p>
      </header>

      <figure className="film" data-showcase="showreel-film" aria-labelledby="film-heading">
        <video
          ref={videoRef}
          className="film__video"
          poster={`${FILM_BASE}showreel-poster.webp`}
          preload="none"
          playsInline
          loop
          muted
          width={1920}
          height={1080}
          onPlay={() => setPlaying(true)}
          onPause={() => setPlaying(false)}
          onVolumeChange={(event) => setMuted(event.currentTarget.muted)}
        >
          <source src={`${FILM_BASE}showreel-720.webm`} type="video/webm" media={SMALL_VIEWPORT} />
          <source src={`${FILM_BASE}showreel-1080.webm`} type="video/webm" />
          <source src={`${FILM_BASE}showreel-720.mp4`} type="video/mp4" media={SMALL_VIEWPORT} />
          <source src={`${FILM_BASE}showreel-1080.mp4`} type="video/mp4" />
          <track kind="captions" srcLang="en" label="English" src={`${FILM_BASE}showreel.vtt`} />
        </video>

        <div className="film__controls">
          <button type="button" className="film__control" onClick={toggle}>
            <span aria-hidden="true">{playing ? '❙❙' : '▶'}</span>
            {playing ? 'Pause' : 'Play'}
          </button>
          <button type="button" className="film__control" onClick={toggleSound}>
            <span aria-hidden="true">{muted ? '🔊' : '🔇'}</span>
            {muted ? 'Sound on' : 'Sound off'}
          </button>
          <button type="button" className="film__control" onClick={toggleCaptions}>
            <span aria-hidden="true">CC</span>
            {captionsShowing ? 'Captions off' : 'Captions on'}
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
