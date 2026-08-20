import { type ReactNode, useEffect, useRef, useState } from 'react';
import type { ProductCapture } from '../../data/showcase';
import { ProductShot } from './ProductShot';
import './shot-tile.css';

interface ShotTileProps {
  readonly capture: ProductCapture;
  readonly className?: string;
  readonly eager?: boolean;
  readonly reveal?: string;
  readonly variant: 'desktop' | 'phone';
  readonly children?: ReactNode;
  readonly onOpen: (trigger: HTMLButtonElement) => void;
}

export function ShotTile({
  capture,
  className = '',
  eager = false,
  reveal,
  variant,
  children,
  onOpen,
}: ShotTileProps) {
  const imageRef = useRef<HTMLImageElement>(null);
  // A tile fades its picture in when the file arrives — a reveal that belongs to
  // the tiles a reader scrolls onto. The two above the fold are loaded eagerly
  // and are the first thing painted: fading them in only delays the largest
  // paint by the length of the fade, so they start settled.
  const [loading, setLoading] = useState(!eager);

  useEffect(() => {
    if (imageRef.current?.complete) setLoading(false);
  }, []);

  return (
    <button
      className={`shot-tile shot-tile--${variant} ${className}`.trim()}
      type="button"
      data-shot=""
      data-reveal={reveal}
      data-loading={loading ? 'true' : 'false'}
      aria-label={`Open screenshot: ${capture.title}`}
      onClick={(event) => onOpen(event.currentTarget)}
    >
      {/*
       * The wrap holds the picture and whatever is laid over it — the hero
       * phone's visualizer canvas among them. The zoom runs on the wrap so the
       * canvas travels with the screenshot instead of standing still on top of a
       * growing image. Caption and cue stay outside: they must not scale.
       */}
      <span className="shot-tile__picture">
        <ProductShot
          capture={capture}
          eager={eager}
          imageRef={imageRef}
          onLoad={() => setLoading(false)}
          onError={() => setLoading(false)}
        />
        {children}
      </span>
      <span className="shot-tile__sweep" data-sweep="" aria-hidden="true">
        <span />
      </span>
      {/*
       * The hover response, in full: a 1px accent line drawn on the picture's
       * own edge. A screenshot is the one image class that must not be scaled —
       * the text inside the capture goes soft and the window edges leave the
       * frame, so the plate stops showing what the app looks like. Decoration
       * over a button that already carries `aria-label="Open screenshot: …"`,
       * so it stays out of the accessibility tree and out of the hit test.
       */}
      <span className="shot-tile__frame" data-frame="" aria-hidden="true" />
      {/* Phosphor `ArrowsOutSimple`, regular weight — see LICENSING.md. */}
      <span className="shot-tile__zoom" data-zoom="" aria-hidden="true">
        <svg viewBox="0 0 256 256" fill="currentColor" focusable="false" aria-hidden="true">
          <path d="M216,48V96a8,8,0,0,1-16,0V67.31l-50.34,50.35a8,8,0,0,1-11.32-11.32L188.69,56H160a8,8,0,0,1,0-16h48A8,8,0,0,1,216,48ZM106.34,138.34,56,188.69V160a8,8,0,0,0-16,0v48a8,8,0,0,0,8,8H96a8,8,0,0,0,0-16H67.31l50.35-50.34a8,8,0,0,0-11.32-11.32Z" />
        </svg>
      </span>
      <span className="shot-tile__caption">
        <span className="shot-tile__platform" data-p="">
          {capture.platform}
        </span>
        <span className="shot-tile__title" data-t="">
          {capture.title}
        </span>
      </span>
    </button>
  );
}
