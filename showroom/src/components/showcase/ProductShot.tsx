import type { ReactEventHandler, Ref } from 'react';
import { captureUrl, type ProductCapture } from '../../data/showcase';

interface ProductShotProps {
  readonly capture: ProductCapture;
  readonly className?: string;
  readonly eager?: boolean;
  readonly imageRef?: Ref<HTMLImageElement>;
  readonly onError?: ReactEventHandler<HTMLImageElement>;
  readonly onLoad?: ReactEventHandler<HTMLImageElement>;
}

export function ProductShot({
  capture,
  className = '',
  eager = false,
  imageRef,
  onError,
  onLoad,
}: ProductShotProps) {
  return (
    <img
      ref={imageRef}
      className={`product-shot ${className}`.trim()}
      src={captureUrl(capture)}
      alt={capture.alt}
      width={capture.width}
      height={capture.height}
      loading={eager ? 'eager' : 'lazy'}
      decoding="async"
      onError={onError}
      onLoad={onLoad}
    />
  );
}
