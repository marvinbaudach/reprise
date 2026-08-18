import {
  type ReactNode,
  type PointerEvent as ReactPointerEvent,
  useEffect,
  useRef,
  useState,
} from 'react';
import type { ProductCapture } from '../../data/showcase';
import { ProductShot } from './ProductShot';
import './shot-tile.css';

const TILT_DEGREES = 8;

function resetTile(tile: HTMLButtonElement | null): void {
  if (!tile) return;
  tile.style.transition = '';
  tile.style.transform = '';
  tile.style.boxShadow = '';
  tile.style.borderColor = '';
}

interface ShotTileProps {
  readonly capture: ProductCapture;
  readonly className?: string;
  readonly eager?: boolean;
  readonly reducedMotion: boolean;
  readonly variant: 'desktop' | 'phone';
  readonly children?: ReactNode;
  readonly onOpen: (trigger: HTMLButtonElement) => void;
}

export function ShotTile({
  capture,
  className = '',
  eager = false,
  reducedMotion,
  variant,
  children,
  onOpen,
}: ShotTileProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (imageRef.current?.complete) setLoading(false);
  }, []);

  useEffect(() => {
    if (reducedMotion) resetTile(buttonRef.current);
  }, [reducedMotion]);

  const handlePointerMove = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const tile = event.currentTarget;
    const bounds = tile.getBoundingClientRect();
    const x = (event.clientX - bounds.left) / bounds.width;
    const y = (event.clientY - bounds.top) / bounds.height;
    tile.style.setProperty('--mx', `${(x * 100).toFixed(1)}%`);
    tile.style.setProperty('--my', `${(y * 100).toFixed(1)}%`);
    if (reducedMotion) return;

    tile.style.transition =
      'transform 110ms linear, box-shadow 300ms ease, border-color 300ms ease';
    tile.style.transform = `perspective(1200px) rotateX(${((0.5 - y) * TILT_DEGREES).toFixed(2)}deg) rotateY(${((x - 0.5) * TILT_DEGREES).toFixed(2)}deg) scale(1.014)`;
    tile.style.boxShadow =
      '0 40px 90px -46px oklch(3% 0.02 269 / 0.98), 0 0 40px -18px oklch(80% 0.12 190 / 0.35)';
    tile.style.borderColor = 'oklch(46% 0.03 195)';
  };

  return (
    <button
      ref={buttonRef}
      className={`shot-tile shot-tile--${variant} ${className}`.trim()}
      type="button"
      data-shot=""
      data-loading={loading ? 'true' : 'false'}
      aria-label={`Open screenshot: ${capture.title}`}
      onClick={(event) => onOpen(event.currentTarget)}
      onPointerMove={handlePointerMove}
      onPointerLeave={(event) => resetTile(event.currentTarget)}
    >
      <ProductShot
        capture={capture}
        eager={eager}
        imageRef={imageRef}
        onLoad={() => setLoading(false)}
        onError={() => setLoading(false)}
      />
      {children}
      <span className="shot-tile__sweep" data-sweep="" aria-hidden="true">
        <span />
      </span>
      <span className="shot-tile__sheen" data-sheen="" aria-hidden="true" />
      <span className="shot-tile__caption">
        <span className="shot-tile__platform" data-p="">
          {capture.platform}
        </span>
        <span className="shot-tile__title" data-t="">
          {capture.title}
        </span>
        <span className="shot-tile__description-wrap" data-dwrap="">
          <span className="shot-tile__description" data-d="">
            {capture.description}
          </span>
        </span>
      </span>
    </button>
  );
}
