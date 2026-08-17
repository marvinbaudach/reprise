import { captureUrl, type ProductCapture } from '../../data/showcase';

interface ProductShotProps {
  readonly capture: ProductCapture;
  readonly className?: string;
  readonly eager?: boolean;
}

export function ProductShot({ capture, className = '', eager = false }: ProductShotProps) {
  return (
    <img
      className={`product-shot ${className}`.trim()}
      src={captureUrl(capture)}
      alt={capture.alt}
      width={capture.width}
      height={capture.height}
      loading={eager ? 'eager' : 'lazy'}
      decoding="async"
    />
  );
}
