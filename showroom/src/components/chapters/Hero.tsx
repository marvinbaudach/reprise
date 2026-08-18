import { HeroSeekTrack } from '../seek/MeasuredSeekTrack';
import { HeroProduct } from '../showcase/HeroProduct';
import './chapters.css';

interface HeroProps {
  readonly reducedMotion: boolean;
}

export function Hero({ reducedMotion }: HeroProps) {
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
          <p className="hero__eyebrow" data-reveal="">
            A music player for GNOME and Android
          </p>

          <h1 id="hero-heading" className="hero__headline" data-reveal="">
            <span>Two native apps.</span>
            <span>One Rust core.</span>
          </h1>

          <p className="hero__lead" data-reveal="">
            Built with AI agents. What gets merged is decided by the gates, not by the agent.
          </p>

          <p className="hero__note" data-reveal="">
            The core carries no interface. That is not an architectural preference, it is the reason
            the second platform had a price tag instead of a rewrite — and the reason a third one
            would have a price tag too.
          </p>

          <div className="hero__scroll-cue" data-reveal="" data-showcase="scroll-cue">
            <span className="hero__scroll-line" aria-hidden="true" />
            <span>Scroll</span>
          </div>
        </div>

        <HeroProduct reducedMotion={reducedMotion} />
      </div>
      <HeroSeekTrack reducedMotion={reducedMotion} />
    </section>
  );
}
