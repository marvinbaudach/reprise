import { GATES } from 'virtual:merge-gates';
import './chapters.css';

/**
 * The tempo band between the hero and the first chapter: how long the whole
 * thing took, next to the three figures that make the number mean something.
 *
 * It sits on its own ground colour and between two hairlines, so it reads as a
 * caesura rather than as a chapter of its own.
 */
export function TempoBand() {
  return (
    <section className="tempo" data-ground="oklch(14% 0.03 195)">
      <div className="tempo__frame frame">
        <div>
          <p className="eyebrow tempo__label" data-reveal>
            Idea to alpha
          </p>
          <p className="tempo__figure" data-reveal>
            <span data-counter>4</span>
            <span className="tempo__unit">weeks</span>
          </p>
          <p className="tempo__note" data-reveal>
            From the first idea to a running alpha on all four frontends — desktop, phone, terminal
            and agent surface.
          </p>
        </div>

        <div className="tempo__stats" data-reveal>
          <p>
            <span data-counter>347'842</span>
            <span>lines of Rust and Kotlin</span>
          </p>
          <p>
            <span data-counter>45.8 %</span>
            <span>of the code is tests</span>
          </p>
          <p>
            <span className="tempo__accent" data-counter>
              {GATES.length}
            </span>
            <span>gates on every merge</span>
          </p>
          <p>
            <span>0</span>
            <span>merges on an agent's word</span>
          </p>
        </div>
      </div>
    </section>
  );
}
