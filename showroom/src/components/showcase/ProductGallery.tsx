import { GALLERY_CAPTURES } from '../../data/showcase';
import { ProductShot } from './ProductShot';
import './showcase.css';

export function ProductGallery() {
  return (
    <div className="product-gallery-stage" data-showcase="product-gallery">
      <div className="frame product-gallery-stage__intro">
        <p className="eyebrow">The surface is part of the proof</p>
        <p className="lead">
          Native where it matters. Shared where it pays. Every frame below is the current app, not a
          mockup.
        </p>
      </div>

      <div className="product-gallery" role="region" aria-label="Reprise product surfaces" tabIndex={0}>
        {GALLERY_CAPTURES.map((capture) => (
          <figure
            className={`product-gallery__item product-gallery__item--${capture.platform.toLowerCase()}`}
            key={capture.id}
          >
            <div className="product-gallery__media">
              <ProductShot capture={capture} />
            </div>
            <figcaption className="product-gallery__caption">
              <span className="eyebrow">{capture.platform}</span>
              <strong>{capture.title}</strong>
              <span className="data">{capture.description}</span>
            </figcaption>
          </figure>
        ))}
      </div>

      <p className="frame data product-gallery-stage__hint">Drag or scroll the evidence strip →</p>
    </div>
  );
}
