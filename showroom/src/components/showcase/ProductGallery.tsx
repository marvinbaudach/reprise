import { GALLERY_CAPTURES } from '../../data/showcase';
import { ProductShot } from './ProductShot';
import './showcase.css';

export function ProductGallery() {
  const desktopCaptures = GALLERY_CAPTURES.filter((capture) => capture.platform === 'GNOME');
  const phoneCaptures = GALLERY_CAPTURES.filter((capture) => capture.platform === 'Android');

  const renderCapture = (capture: (typeof GALLERY_CAPTURES)[number]) => (
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
  );

  return (
    <div className="product-gallery-stage" data-showcase="product-gallery">
      <div className="frame product-gallery-stage__intro">
        <p className="eyebrow">The surface is part of the proof</p>
        <p className="lead">
          Native where it matters. Shared where it pays. Every frame below is the current app, not a
          mockup.
        </p>
      </div>

      <section
        className="product-gallery"
        data-layout="editorial-grid"
        aria-label="Reprise product surfaces"
      >
        <div className="product-gallery__desktop">{desktopCaptures.map(renderCapture)}</div>

        <section className="product-gallery__phones" aria-labelledby="product-gallery-phones-title">
          <header className="product-gallery__phones-intro">
            <p className="eyebrow">Same core, native mobile</p>
            <h3 id="product-gallery-phones-title">A phone scene, not a shrunken desktop.</h3>
            <p className="data">
              Compose keeps the interaction touch-first while the Rust layer supplies the same
              library and track analysis.
            </p>
          </header>
          <div className="product-gallery__phone-grid">{phoneCaptures.map(renderCapture)}</div>
        </section>
      </section>
    </div>
  );
}
