import { type MouseEvent as ReactMouseEvent, useEffect, useRef, useState } from 'react';
import { captureUrl, type ProductCapture } from '../../data/showcase';
import './lightbox.css';

interface LightboxProps {
  readonly activeIndex: number;
  readonly captures: readonly ProductCapture[];
  readonly returnFocus: HTMLButtonElement | null;
  readonly onClose: () => void;
  readonly onNext: () => void;
  readonly onPrevious: () => void;
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
  const [zoom, setZoom] = useState<{ readonly origin: string; readonly zoomed: boolean }>({
    origin: 'center',
    zoomed: false,
  });

  useEffect(() => {
    if (activeIndex >= 0) setZoom({ origin: 'center', zoomed: false });
  }, [activeIndex]);

  useEffect(() => {
    const previousOverflow = document.documentElement.style.overflow;
    document.documentElement.style.overflow = 'hidden';
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
          'button:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
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
      if (returnFocus) returnFocus.focus();
    };
  }, [onClose, onNext, onPrevious, returnFocus]);

  if (!capture) return null;

  const handleZoom = (event: ReactMouseEvent<HTMLButtonElement>) => {
    if (zoom.zoomed) {
      setZoom({ origin: 'center', zoomed: false });
      return;
    }
    const bounds = event.currentTarget.getBoundingClientRect();
    const x = ((event.clientX - bounds.left) / bounds.width) * 100;
    const y = ((event.clientY - bounds.top) / bounds.height) * 100;
    setZoom({ origin: `${x.toFixed(1)}% ${y.toFixed(1)}%`, zoomed: true });
  };

  const counter = `${String(activeIndex + 1).padStart(2, '0')} / ${String(captures.length).padStart(2, '0')}`;
  const titleId = `lightbox-title-${capture.id}`;
  const descriptionId = `lightbox-description-${capture.id}`;

  return (
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
          onClick={onClose}
        />
        <button
          className="lightbox__zoom"
          type="button"
          aria-label={zoom.zoomed ? 'Reset screenshot zoom' : 'Zoom screenshot'}
          onClick={handleZoom}
        >
          <img
            className="lightbox__image"
            src={captureUrl(capture)}
            alt={capture.alt}
            width={capture.width}
            height={capture.height}
            data-lb-img=""
            data-zoomed={zoom.zoomed ? 'true' : 'false'}
            style={{
              transform: zoom.zoomed ? 'scale(2.1)' : 'none',
              transformOrigin: zoom.origin,
            }}
            draggable={false}
          />
        </button>
      </div>
      <p id={descriptionId} className="lightbox__description">
        {capture.description}
      </p>
    </div>
  );
}
