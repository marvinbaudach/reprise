import { useCallback, useEffect, useRef, useState } from 'react';
import './showreel.css';

const BASE_URL = import.meta.env?.BASE_URL ?? '/reprise/';
const FILM_BASE = `${BASE_URL}media/showreel/`;

/**
 * Below this width the 1080 ladder is wasted on the layout, so the smaller
 * step is offered first. `media` is read once when the element picks a source,
 * which is why the breakpoint matches the layout's own and not a device class.
 */
const SMALL_VIEWPORT = '(max-width: 900px)';

/** Enough of the plate on screen to be worth starting. */
const PLAY_THRESHOLD = 0.45;

interface ShowreelFilmProps {
  readonly reducedMotion: boolean;
}

/**
 * The film, in the slot the screenshot mosaic used to hold.
 *
 * It starts itself when it scrolls into view and stops when it leaves, because
 * a page that loads ten megabytes for a reader who never reaches the section
 * has spent someone else's bandwidth. The moment the reader touches a control
 * that hand-off ends: from then on the film is theirs, and scrolling does not
 * overrule it.
 *
 * It starts muted and says so. Every browser blocks an unmuted autoplay anyway,
 * but that is not the reason — a landing page that makes noise at a reader who
 * did not ask for it has already lost them.
 */
export function ShowreelFilm({ reducedMotion }: ShowreelFilmProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const claimed = useRef(false);
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(true);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    // The preference can flip while the page is open, so this is a reaction to
    // the current value rather than a decision taken once at mount.
    if (reducedMotion) {
      video.pause();
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[0];
        if (!entry || claimed.current) return;
        if (entry.isIntersecting) {
          // A rejected play() is the browser exercising its autoplay policy,
          // not a fault: the poster and the control are already in place.
          void video.play().catch(() => undefined);
        } else {
          video.pause();
        }
      },
      { threshold: PLAY_THRESHOLD },
    );
    observer.observe(video);
    return () => observer.disconnect();
  }, [reducedMotion]);

  const toggle = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    claimed.current = true;
    if (video.paused) {
      void video.play().catch(() => undefined);
    } else {
      video.pause();
    }
  }, []);

  const toggleSound = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    claimed.current = true;
    const next = !video.muted;
    video.muted = next;
    setMuted(next);
    // Turning the sound on is the clearest statement of intent there is, so it
    // also starts the film if it happens to be sitting still.
    if (!next && video.paused) void video.play().catch(() => undefined);
  }, []);

  return (
    <div className="frame film-frame">
      <header className="film-heading">
        <h3 id="film-heading" data-reveal>
          Sixty seconds. Both apps, and an agent writing into the library.
        </h3>
        <p data-reveal>Screen recordings of the running apps — nothing rebuilt for the camera</p>
      </header>

      <figure className="film" data-showcase="showreel-film" aria-labelledby="film-heading">
        <video
          ref={videoRef}
          className="film__video"
          poster={`${FILM_BASE}showreel-poster.webp`}
          preload="metadata"
          playsInline
          loop
          muted
          width={1920}
          height={1080}
          onPlay={() => setPlaying(true)}
          onPause={() => setPlaying(false)}
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
            <span aria-hidden="true">{muted ? '🔇' : '🔊'}</span>
            {muted ? 'Sound on' : 'Sound off'}
          </button>
        </div>

        <figcaption className="film__caption">
          Instant search, lyrics, podcasts, the Library Doctor, listening statistics — then the same
          library and the same visualizer on Android.
        </figcaption>
      </figure>
    </div>
  );
}
