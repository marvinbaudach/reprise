import {
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  useEffect,
  useRef,
  useState,
} from 'react';
import { createPortal } from 'react-dom';
import {
  captureSrcSet,
  captureUrl,
  LIGHTBOX_SIZES,
  type ProductCapture,
} from '../../data/showcase';
import { VisualizerPlate } from '../../visualizer/VisualizerPlate';
import './lightbox.css';

// Ten seconds leaves a slow but healthy download ample time to preserve the
// atomic ratio-and-bitmap swap, while still letting a wedged dialog recover.
const IMAGE_PRELOAD_TIMEOUT_MS = 10_000;

interface LightboxProps {
  readonly activeIndex: number;
  readonly captures: readonly ProductCapture[];
  readonly returnFocus: HTMLButtonElement | null;
  readonly onClose: () => void;
  readonly onNext: () => void;
  readonly onPrevious: () => void;
}

interface ZoomState {
  readonly index: number;
  readonly origin: string;
  // Kept while the picture travels back to 1x: dropping the state outright
  // would snap the origin to the centre mid-flight and swing the picture
  // across the viewport instead of letting it settle where it grew from.
  readonly zoomed: boolean;
}

export function Lightbox({
  activeIndex,
  captures,
  returnFocus,
  onClose,
  onNext,
  onPrevious,
}: LightboxProps) {
  /*
   * The picture the frame is currently built around, which lags the requested
   * index until the next file has decoded.
   *
   * The frame carries the capture's aspect ratio and the image inside it is
   * `object-fit: contain`, so advancing the ratio the moment the index changes
   * re-letterboxes the picture that is still on screen: the outgoing shot
   * visibly rescales — a desktop capture at 1.6 into a phone's 0.46 — and only
   * then does the new one arrive. Warm, the two happen in the same frame and
   * nobody sees it; cold, on a phone, that is the reported flash. Holding the
   * ratio and the source together removes it: nothing moves until the picture
   * that belongs to the new ratio is ready to be painted.
   */
  const [shownIndex, setShownIndex] = useState(activeIndex);
  const capture = captures[shownIndex];
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  // Display follows the requested index, so an arrow press settles the outgoing
  // picture before the decoded incoming picture replaces it at 1x.
  const [zoom, setZoom] = useState<ZoomState | null>(null);
  const frameZoom = zoom && zoom.index === activeIndex ? zoom : null;
  const activeZoom = frameZoom?.zoomed ? frameZoom : null;
  const swapping = activeIndex !== shownIndex;

  useEffect(() => {
    if (activeIndex === shownIndex) return undefined;
    const incoming = captures[activeIndex];
    if (!incoming) {
      setShownIndex(activeIndex);
      return undefined;
    }

    let superseded = false;
    const commit = () => {
      window.clearTimeout(timeout);
      // A later press starts its own preload and this one must not land on top
      // of it — the reader would be sent back a picture.
      if (!superseded) setShownIndex(activeIndex);
    };

    const preload = new Image();
    preload.sizes = LIGHTBOX_SIZES;
    preload.srcset = captureSrcSet(incoming);
    preload.src = captureUrl(incoming);
    // `decode()` waits for the pixels, not just the bytes, which is the whole
    // point here — but it rejects on an aborted or broken file, and it is not
    // everywhere. Either way the swap has to happen: a picture that cannot be
    // decoded is still the picture the reader asked for, and the `<img>` below
    // carries its own error handling.
    if (typeof preload.decode === 'function') {
      preload.decode().then(commit, commit);
    } else {
      preload.onload = commit;
      preload.onerror = commit;
    }
    const timeout = window.setTimeout(commit, IMAGE_PRELOAD_TIMEOUT_MS);

    return () => {
      superseded = true;
      window.clearTimeout(timeout);
      preload.src = '';
      preload.srcset = '';
    };
  }, [activeIndex, shownIndex, captures]);

  useEffect(() => {
    // The dialog lives in a portal on <body>, so the page behind it can be made
    // inert wholesale: the keyboard trap below only stops Tab, while a screen
    // reader's browse cursor would still walk the header and the footer that
    // aria-modal declares hidden.
    const page = document.getElementById('showroom-root');
    const previousOverflow = document.documentElement.style.overflow;
    document.documentElement.style.overflow = 'hidden';
    page?.setAttribute('inert', '');
    page?.setAttribute('aria-hidden', 'true');
    closeRef.current?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key === 'ArrowRight') {
        event.preventDefault();
        onNext();
        return;
      }
      if (event.key === 'ArrowLeft') {
        event.preventDefault();
        onPrevious();
        return;
      }
      if (event.key !== 'Tab') return;

      const focusable = Array.from(
        dialogRef.current?.querySelectorAll<HTMLElement>(
          'button:not([disabled]):not([tabindex="-1"]), [href], [tabindex]:not([tabindex="-1"])',
        ) ?? [],
      );
      const first = focusable.at(0);
      const last = focusable.at(-1);
      if (!first || !last) {
        event.preventDefault();
        dialogRef.current?.focus();
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      document.documentElement.style.overflow = previousOverflow;
      page?.removeAttribute('inert');
      page?.removeAttribute('aria-hidden');
      // Only reachable again once the page is no longer inert.
      if (returnFocus) returnFocus.focus();
    };
  }, [onClose, onNext, onPrevious, returnFocus]);

  if (!capture) return null;

  const handleZoom = (event: ReactMouseEvent<HTMLButtonElement>) => {
    if (activeZoom) {
      setZoom({ ...activeZoom, zoomed: false });
      return;
    }
    const bounds = event.currentTarget.getBoundingClientRect();
    const x = ((event.clientX - bounds.left) / bounds.width) * 100;
    const y = ((event.clientY - bounds.top) / bounds.height) * 100;
    setZoom({
      index: shownIndex,
      origin: `${x.toFixed(1)}% ${y.toFixed(1)}%`,
      zoomed: true,
    });
  };

  const counter = `${String(shownIndex + 1).padStart(2, '0')} / ${String(captures.length).padStart(2, '0')}`;
  const titleId = `lightbox-title-${capture.id}`;
  const descriptionId = `lightbox-description-${capture.id}`;

  return createPortal(
    <div
      ref={dialogRef}
      className="lightbox"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      aria-describedby={descriptionId}
      aria-busy={swapping ? 'true' : undefined}
      tabIndex={-1}
      data-lb=""
      data-swapping={swapping ? 'true' : 'false'}
    >
      <header className="lightbox__header">
        <div className="lightbox__heading">
          <p className="lightbox__platform">{capture.platform}</p>
          <h2 id={titleId}>{capture.title}</h2>
        </div>
        <div className="lightbox__controls">
          <span className="lightbox__counter" aria-live="polite">
            {counter}
          </span>
          <button type="button" onClick={onPrevious} aria-label="Previous screenshot">
            ←
          </button>
          <button type="button" onClick={onNext} aria-label="Next screenshot">
            →
          </button>
          <button ref={closeRef} type="button" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>
      </header>
      <div className="lightbox__viewport">
        <button
          className="lightbox__backdrop"
          type="button"
          aria-label="Close lightbox"
          // A mouse shortcut, not a tab stop: the header already carries Close.
          tabIndex={-1}
          onClick={onClose}
        />
        <button
          className="lightbox__zoom"
          type="button"
          aria-label={activeZoom ? 'Reset screenshot zoom' : 'Zoom screenshot'}
          onClick={handleZoom}
        >
          {/* The frame carries the aspect ratio, so the plate can sit on the
              picture in its own percentages and zoom along with it. */}
          <span
            className="lightbox__frame"
            style={
              {
                '--lb-ratio': capture.width / capture.height,
                transform: activeZoom ? 'scale(2.1)' : 'none',
                transformOrigin: frameZoom?.origin ?? 'center',
              } as CSSProperties
            }
          >
            <img
              className="lightbox__image"
              src={captureUrl(capture)}
              srcSet={captureSrcSet(capture)}
              sizes={LIGHTBOX_SIZES}
              alt={capture.alt}
              width={capture.width}
              height={capture.height}
              data-lb-img=""
              data-zoomed={activeZoom ? 'true' : 'false'}
              draggable={false}
            />
            {capture.visualizer && <VisualizerPlate />}
          </span>
        </button>
      </div>
      <p id={descriptionId} className="lightbox__description">
        {capture.description}
      </p>
    </div>,
    document.body,
  );
}
