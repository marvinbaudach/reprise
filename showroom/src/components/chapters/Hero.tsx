import { HeroProduct } from '../showcase/HeroProduct';
import './chapters.css';

export function Hero() {
  return (
    <section
      id="rp-top"
      className="hero"
      aria-labelledby="hero-heading"
      data-ground="oklch(13% 0.014 269)"
      data-showcase="design-hero"
    >
      <div className="hero__grid">
        <div className="hero__copy">
          <p className="hero__eyebrow">A music player for GNOME and Android</p>
          <h1 id="hero-heading" className="hero__headline">
            <span>Two native apps.</span>
            <span>One Rust core.</span>
          </h1>
          <p className="hero__lead">
            Your music library, at home and on the move. Built to feel at home on each platform.
          </p>
          <p className="hero__note">
            An independent project by <strong>Marvin Baudach</strong>.<br />
            Product design, architecture and quality — with AI-assisted development.
          </p>
          <div className="hero__actions">
            <a className="action action--primary" href="#film">
              <span aria-hidden="true">▶</span> Watch the film{' '}
              <span className="action__duration">0:58</span>
            </a>
            <a className="action action--text" href="#ch-01">
              Explore the engineering <span aria-hidden="true">↓</span>
            </a>
          </div>
        </div>
        <HeroProduct />
      </div>
      <div className="hero__byline frame">
        <span>Rust · GTK4 · Kotlin · Compose</span>
        <a href="#availability">
          Open to opportunities <span aria-hidden="true">↗</span>
        </a>
      </div>
    </section>
  );
}
