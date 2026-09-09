import { type CSSProperties, type KeyboardEvent, useCallback, useRef, useState } from 'react';
import { PlayerIcon } from './PlayerIcon';
import './showreel.css';

const BASE_URL = import.meta.env?.BASE_URL ?? '/reprise/';
const FILM_BASE = `${BASE_URL}media/showreel/`;
const SMALL_VIEWPORT = '(max-width: 900px)';
// The encode fades to black after its end card. Rest just before that fade.
const END_CARD_HOLD_SECONDS = 0.7;
const CONTROLS_IDLE_MS = 2400;

function timeLabel(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}

export function ShowreelFilm() {
  const videoRef = useRef<HTMLVideoElement>(null);
  const frameRef = useRef<HTMLDivElement>(null);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [enhanced, setEnhanced] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(false);
  const [volume, setVolume] = useState(1);
  const [ended, setEnded] = useState(false);
  const [duration, setDuration] = useState(0);
  const [position, setPosition] = useState(0);
  const [active, setActive] = useState(true);
  const [fullscreen, setFullscreen] = useState(false);
  const [waiting, setWaiting] = useState(false);
  const [error, setError] = useState('');

  const attachVideo = useCallback((video: HTMLVideoElement | null) => {
    videoRef.current = video;
    if (video) setEnhanced(true);
  }, []);

  const attachFrame = useCallback((frame: HTMLDivElement | null) => {
    frameRef.current = frame;
    if (!frame) return undefined;
    const changed = () => setFullscreen(document.fullscreenElement === frame);
    frame.addEventListener('fullscreenchange', changed);
    return () => {
      clearTimeout(hideTimer.current);
      frame.removeEventListener('fullscreenchange', changed);
    };
  }, []);

  const wakeControls = useCallback(() => {
    clearTimeout(hideTimer.current);
    setActive(true);
    hideTimer.current = setTimeout(() => setActive(false), CONTROLS_IDLE_MS);
  }, []);

  const start = useCallback(async (video: HTMLVideoElement, fromTheTop: boolean) => {
    if (video.error || video.networkState === HTMLMediaElement.NETWORK_NO_SOURCE) video.load();
    if (fromTheTop) video.currentTime = 0;
    setError('');
    setWaiting(true);
    try {
      await video.play();
    } catch (failure: unknown) {
      if (failure instanceof DOMException && failure.name === 'AbortError') return;
      if (failure instanceof DOMException && failure.name === 'NotAllowedError' && !video.muted) {
        video.muted = true;
        try {
          await video.play();
          return;
        } catch {
          /* Show the same recovery route below. */
        }
      }
      setWaiting(false);
      setError('The film could not play. Try again or open the video directly.');
    }
  }, []);

  const unavailable = () => {
    videoRef.current?.pause();
    setPlaying(false);
    setWaiting(false);
    setError('The film could not load. Try again or open the video directly.');
  };

  const toggle = () => {
    const video = videoRef.current;
    if (!video) return;
    if (video.paused) void start(video, ended);
    else video.pause();
    wakeControls();
  };

  const seek = (seconds: number) => {
    const video = videoRef.current;
    if (!video || !duration) return;
    video.currentTime = Math.max(0, Math.min(duration, seconds));
    setPosition(video.currentTime);
    setEnded(false);
    wakeControls();
  };

  const toggleSound = () => {
    const video = videoRef.current;
    if (!video) return;
    video.muted = !video.muted;
    if (!video.muted && video.volume === 0) video.volume = 1;
    wakeControls();
  };

  const toggleFullscreen = async () => {
    try {
      if (document.fullscreenElement === frameRef.current) await document.exitFullscreen();
      else await frameRef.current?.requestFullscreen();
    } catch {
      setError('Full screen is unavailable in this browser.');
    }
    wakeControls();
  };

  const shortcuts = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey || event.target instanceof HTMLInputElement)
      return;
    if (event.key === ' ' && event.target instanceof HTMLButtonElement) return;
    switch (event.key.toLowerCase()) {
      case ' ':
      case 'k':
        event.preventDefault();
        toggle();
        break;
      case 'arrowleft':
        event.preventDefault();
        seek(position - 5);
        break;
      case 'arrowright':
        event.preventDefault();
        seek(position + 5);
        break;
      case 'm':
        event.preventDefault();
        toggleSound();
        break;
      case 'f':
        event.preventDefault();
        void toggleFullscreen();
        break;
    }
  };

  const displayPosition = ended ? duration : position;
  const progress = duration ? (displayPosition / duration) * 100 : 0;
  const playLabel = playing ? 'Pause' : ended ? 'Watch again' : 'Play';

  return (
    <section id="film" className="frame film-frame" aria-labelledby="film-heading">
      <header className="film-heading">
        <div>
          <p className="eyebrow">See it in action</p>
          <h2 id="film-heading">Reprise in 58 seconds.</h2>
        </div>
        <p>The desktop, the phone, and the sync between them. Recorded in the running apps.</p>
      </header>
      <figure className="film" data-showcase="showreel-film" aria-labelledby="film-heading">
        {/* biome-ignore lint/a11y/useSemanticElements: A media control group is not a form fieldset. */}
        <div
          ref={attachFrame}
          className="film__screen"
          role="group"
          aria-label="Film player"
          tabIndex={-1}
          data-active={active || !playing || waiting || Boolean(error)}
          data-playing={playing}
          onPointerMove={wakeControls}
          onPointerDown={wakeControls}
          onKeyDown={shortcuts}
        >
          {/* biome-ignore lint/a11y/useMediaCaption: Screen recordings over music, with the text equivalent in the figcaption. */}
          <video
            ref={attachVideo}
            className="film__video"
            poster={`${FILM_BASE}showreel-poster.webp`}
            preload="none"
            playsInline
            controls={!enhanced}
            width={1920}
            height={1080}
            onLoadedMetadata={(event) => {
              const value = event.currentTarget.duration;
              if (Number.isFinite(value)) setDuration(value);
            }}
            onDurationChange={(event) => {
              const value = event.currentTarget.duration;
              if (Number.isFinite(value)) setDuration(value);
            }}
            onTimeUpdate={(event) => setPosition(event.currentTarget.currentTime)}
            onPlay={() => {
              setPlaying(true);
              setEnded(false);
              wakeControls();
            }}
            onPlaying={() => setWaiting(false)}
            onWaiting={() => setWaiting(true)}
            onCanPlay={() => setWaiting(false)}
            onSeeked={() => setWaiting(false)}
            onPause={() => {
              setPlaying(false);
              setWaiting(false);
            }}
            onEnded={(event) => {
              setEnded(true);
              setPlaying(false);
              setWaiting(false);
              const video = event.currentTarget;
              if (Number.isFinite(video.duration))
                video.currentTime = Math.max(0, video.duration - END_CARD_HOLD_SECONDS);
            }}
            onError={(event) => {
              if (event.currentTarget.error) {
                unavailable();
              }
            }}
            onVolumeChange={(event) => {
              setMuted(event.currentTarget.muted);
              setVolume(event.currentTarget.volume);
            }}
          >
            <source
              src={`${FILM_BASE}showreel-720.webm`}
              type="video/webm"
              media={SMALL_VIEWPORT}
            />
            <source src={`${FILM_BASE}showreel-1080.webm`} type="video/webm" />
            <source src={`${FILM_BASE}showreel-720.mp4`} type="video/mp4" media={SMALL_VIEWPORT} />
            {/* A source ladder can exhaust without rejecting play(). Its final error
                must end the loading state and provide a retry. */}
            <source src={`${FILM_BASE}showreel-1080.mp4`} type="video/mp4" onError={unavailable} />
          </video>
          {enhanced && (
            <>
              <button
                type="button"
                className="film__surface"
                aria-label={playing ? 'Pause film' : ended ? 'Replay film' : 'Play film'}
                onClick={toggle}
              >
                {!playing && (
                  <span className="film__big-play">
                    <PlayerIcon name={ended ? 'replay' : 'play'} />
                  </span>
                )}
              </button>
              {!duration && <span className="film__badge">0:58</span>}
              {waiting && (
                <span className="film__loading" role="status">
                  Loading film…
                </span>
              )}
              <div className="film__controls">
                <input
                  className="film__seek"
                  type="range"
                  aria-label="Seek film"
                  min={0}
                  max={duration || 1}
                  step={0.1}
                  value={displayPosition}
                  disabled={!duration}
                  aria-valuetext={`${timeLabel(displayPosition)} of ${timeLabel(duration)}`}
                  style={{ '--played': `${progress}%` } as CSSProperties}
                  onChange={(event) => seek(Number(event.currentTarget.value))}
                />
                <div className="film__toolbar">
                  <button
                    type="button"
                    className="film__control"
                    aria-label={playLabel}
                    title={playLabel}
                    onClick={toggle}
                  >
                    <PlayerIcon name={playing ? 'pause' : ended ? 'replay' : 'play'} />
                  </button>
                  <button
                    type="button"
                    className="film__control"
                    aria-label={muted || volume === 0 ? 'Unmute' : 'Mute'}
                    title={muted ? 'Unmute' : 'Mute'}
                    onClick={toggleSound}
                  >
                    <PlayerIcon name={muted || volume === 0 ? 'muted' : 'volume'} />
                  </button>
                  <input
                    className="film__volume"
                    type="range"
                    aria-label="Volume"
                    min={0}
                    max={1}
                    step={0.05}
                    value={muted ? 0 : volume}
                    onChange={(event) => {
                      const video = videoRef.current;
                      if (video) {
                        video.volume = Number(event.currentTarget.value);
                        video.muted = video.volume === 0;
                      }
                      wakeControls();
                    }}
                  />
                  <span className="film__time">
                    {timeLabel(displayPosition)}{' '}
                    <span>/ {duration ? timeLabel(duration) : '0:58'}</span>
                  </span>
                  <button
                    type="button"
                    className="film__control film__fullscreen"
                    aria-label={fullscreen ? 'Exit full screen' : 'Full screen'}
                    title="Full screen (F)"
                    onClick={() => void toggleFullscreen()}
                  >
                    <PlayerIcon name={fullscreen ? 'collapse' : 'expand'} />
                  </button>
                </div>
              </div>
            </>
          )}
          {error && (
            <p className="film__error" role="alert">
              {error} <a href={`${FILM_BASE}showreel-720.mp4`}>Open video</a>
            </p>
          )}
        </div>
        <figcaption className="film__caption">
          Browse music and podcasts, discover releases and concerts, then sync the library to
          Android — with the same visualizer on both platforms.
        </figcaption>
      </figure>
    </section>
  );
}
