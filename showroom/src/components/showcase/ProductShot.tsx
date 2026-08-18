import type { ReactEventHandler, Ref } from 'react';
import { captureSrcSet, captureUrl, type ProductCapture } from '../../data/showcase';

interface ProductShotProps {
  readonly capture: ProductCapture;
  readonly className?: string;
  readonly eager?: boolean;
  readonly imageRef?: Ref<HTMLImageElement>;
  readonly onError?: ReactEventHandler<HTMLImageElement>;
  readonly onLoad?: ReactEventHandler<HTMLImageElement>;
  /** Overrides the capture's own layout width, for surfaces that differ. */
  readonly sizes?: string;
}

export function ProductShot({
  capture,
  className = '',
  eager = false,
  imageRef,
  onError,
  onLoad,
  sizes,
}: ProductShotProps) {
  return (
    <img
      ref={imageRef}
      className={`product-shot ${className}`.trim()}
      src={captureUrl(capture)}
      srcSet={captureSrcSet(capture)}
      sizes={sizes ?? capture.sizes}
      alt={capture.alt}
      width={capture.width}
      height={capture.height}
      loading={eager ? 'eager' : 'lazy'}
      fetchPriority={eager ? 'high' : undefined}
      decoding="async"
      onError={onError}
      onLoad={onLoad}
    />
  );
}
