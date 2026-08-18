import { type MouseEvent as ReactMouseEvent, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  captureSrcSet,
  captureUrl,
  LIGHTBOX_SIZES,
  type ProductCapture,
} from '../../data/showcase';
import { VisualizerPlate } from '../../visualizer/VisualizerPlate';
import './lightbox.css';

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
}

export function Lightbox({
  activeIndex,
  captures,
  returnFocus,
  onClose,
  onNext,
  onPrevious,
}: LightboxProps) {
  const capture = captures[activeIndex];
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  // Tied to the picture it was measured on, so the next arrow key cannot flash
  // the following screenshot at 2.1x around the previous one's origin.
  const [zoom, setZoom] = useState<ZoomState | null>(null);
  const activeZoom = zoom && zoom.index === activeIndex ? zoom : null;

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
      setZoom(null);
      return;
    }
    const bounds = event.currentTarget.getBoundingClientRect();
    const x = ((event.clientX - bounds.left) / bounds.width) * 100;
    const y = ((event.clientY - bounds.top) / bounds.height) * 100;
    setZoom({ index: activeIndex, origin: `${x.toFixed(1)}% ${y.toFixed(1)}%` });
  };

  const counter = `${String(activeIndex + 1).padStart(2, '0')} / ${String(captures.length).padStart(2, '0')}`;
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
      tabIndex={-1}
      data-lb=""
    >
      <header className="lightbox__header">
        <div className="lightbox__heading">
          <p className="lightbox__platform">{capture.platform}</p>
          <h2 id={titleId}>{capture.title}</h2>
        </div>
        <div className="lightbox__controls">
          <span className="lightbox__counter">{counter}</span>
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
            style={{
              aspectRatio: `${capture.width} / ${capture.height}`,
              transform: activeZoom ? 'scale(2.1)' : 'none',
              transformOrigin: activeZoom?.origin ?? 'center',
            }}
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
