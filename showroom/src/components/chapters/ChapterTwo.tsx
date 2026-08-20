import { INCIDENT } from 'virtual:incident';
import { GATE_GROUPS, GATES } from 'virtual:merge-gates';
import { useState } from 'react';
import {
  INCIDENT_RECORD,
  MERGE_GATE_SOURCE,
  permalink,
  STYLE_SOURCE,
} from '../../data/measurements';
import { displayedReadout, readout, toggle } from '../../lib/mergeGates';
import './ChapterTwo.css';
import './chapters.css';

/**
 * The doc comment this chapter quotes, and the rule it produced. Both are
 * anchored: the comment sits on `app_css_for_test()`, the rule in §4C of the
 * record. GitHub derives that fragment from the heading text, so a renamed
 * heading breaks the link rather than silently pointing at the wrong section.
 */
const QUOTE_LINK = `${permalink(STYLE_SOURCE)}#L41-L45`;
const RULE_LINK = `${permalink(INCIDENT_RECORD)}#c-gate-the-444-claim-on-mutations-not-on-a-green-test`;
const GATE_LINK = permalink(MERGE_GATE_SOURCE);

/**
 * The measured header heights, drawn at 3×. Every number here is reported in §1
 * of the record — 20 and 34 are what the unstyled fixture measured, 36 is the
 * `SECTION_HEADER_MIN_HEIGHT` floor that exists only as CSS. They are not
 * derived and not rounded: this is a change that happened once, so the page
 * quotes it rather than counting it.
 */
const SCALE = 3;
const FLOOR_PX = 36;

interface Bar {
  readonly px: number;
  readonly name: string;
  readonly note: string;
}

const MEASURED: readonly Bar[] = [
  { px: 20, name: 'Now Playing', note: 'bare label' },
  { px: 34, name: 'Play Next', note: 'has a button' },
];

const SHIPPED: readonly Bar[] = [
  { px: FLOOR_PX, name: 'Now Playing', note: 'uniform' },
  { px: FLOOR_PX, name: 'Play Next', note: 'uniform' },
];

/**
 * One panel of the comparison. The value labels sit *below* the bars, in the
 * name column, and that placement is load-bearing rather than stylistic: a
 * label above a bar needs its own line box plus the column gap, so any bar
 * within about 23px of the floor rule drives its label straight through the
 * rule and rasterises as a strike-through. 34px is exactly such a value.
 * Putting every label underneath removes the failure mode for all of them
 * instead of tuning an offset that only holds for today's numbers.
 */
function HeightPanel({
  bars,
  eyebrow,
  title,
  tone,
}: {
  readonly bars: readonly Bar[];
  readonly eyebrow: string;
  readonly title: string;
  readonly tone: 'measured' | 'shipped';
}) {
  return (
    <div className="incident-panel" data-tone={tone}>
      <div className="incident-panel__head">
        <p className="incident-panel__eyebrow">{eyebrow}</p>
        <p className="incident-panel__title">{title}</p>
      </div>

      {/*
       * The bars carry nothing the caption and the prose do not already carry,
       * so the whole chart is hidden from assistive technology and the
       * <figure>/<figcaption> pair owns the text equivalent. Eight anonymous
       * boxes in the accessibility tree would be worse than none.
       */}
      <div className="incident-panel__chart" aria-hidden="true">
        <span className="incident-panel__floor" />
        <p className="incident-panel__floor-label">{FLOOR_PX} px floor</p>
        {bars.map((bar) => (
          <span
            key={bar.name}
            className="incident-panel__bar"
            style={{ height: `${bar.px * SCALE}px` }}
          />
        ))}
      </div>

      <div className="incident-panel__names" aria-hidden="true">
        {bars.map((bar) => (
          <p key={bar.name} className="incident-panel__name">
            <span className="data incident-panel__value">{bar.px} px</span>
            {bar.name}
            <br />
            {bar.note}
          </p>
        ))}
      </div>
    </div>
  );
}

/**
 * The gate strip. One mark per check, no names on the surface — the wall of
 * labels it replaces meant nothing to a reader outside this repository. A name
 * appears on hover, and a click fails that check, so "fail closed" is something
 * a visitor triggers instead of something they are told.
 *
 * The names come out of `check-merge-readiness.sh` at build time. Nothing here
 * is typed beside the words it asserts.
 */
function GateStrip() {
  const [failed, setFailed] = useState<ReadonlySet<string>>(() => new Set<string>());
  const [peek, setPeek] = useState<number>(-1);
  const status = readout(failed, GATES.length);
  const peeked = peek > -1 ? GATES[peek] : undefined;

  const message = displayedReadout(status, peek, peeked);

  return (
    <div className="gate-strip" data-blocked={status.blocked ? 'true' : 'false'}>
      <div className="gate-strip__row">
        <p className="gate-strip__end">one change</p>

        <div className="gate-strip__ticks">
          {GATES.map((name, index) => (
            <button
              key={name}
              type="button"
              className="gate-strip__tick"
              aria-label={`${String(index + 1).padStart(2, '0')} · ${name}`}
              aria-pressed={failed.has(name)}
              data-gate={name}
              data-broken={failed.has(name) ? 'true' : 'false'}
              onClick={() => setFailed((current) => toggle(current, name))}
              onMouseEnter={() => setPeek(index)}
              onMouseLeave={() => setPeek(-1)}
              onFocus={() => setPeek(index)}
              onBlur={() => setPeek(-1)}
            >
              <span />
            </button>
          ))}
        </div>

        <span className="gate-strip__rail" aria-hidden="true" />
        <p className="gate-strip__verdict">{status.blocked ? 'blocked' : 'merge'}</p>
      </div>

      <p className="gate-strip__readout" role="status">
        {message}
      </p>
    </div>
  );
}

function GateGroups() {
  return (
    <div className="gate-groups">
      {GATE_GROUPS.map((group) => (
        <article
          key={group.name}
          className="gate-group"
          data-gate-count={group.gates.length}
          data-gates={group.gates.join('|')}
        >
          <h4 className="gate-group__title">
            <span className="data gate-group__count">
              {String(group.gates.length).padStart(2, '0')}
            </span>
            {group.name}
          </h4>
          <p>{group.line}</p>
        </article>
      ))}
    </div>
  );
}

export function ChapterTwo() {
  return (
    <section
      id="ch-02"
      className="chapter chapter--design"
      data-ground="oklch(13.5% 0.02 205)"
      aria-labelledby="ch-02-heading"
    >
      <div className="frame">
        <p className="chapter__eyebrow" data-reveal>
          CH.02
        </p>
        <h2 id="ch-02-heading" className="chapter__title" data-reveal>
          Nobody judges their own writing.
        </h2>

        <p className="chapter__intro" data-reveal>
          An agent will tell you its work is finished. So will a green test. Neither counts here.
        </p>

        <div className="incident" data-reveal>
          <p className="incident__eyebrow">One incident · {INCIDENT.date}</p>
          <h3 className="incident__title">A test was measuring an app that never ships.</h3>
        </div>

        <p className="chapter__intro" data-reveal>
          A queue test failed on header heights. The headers were fine. The fixture never installs
          the app stylesheet, so it was measuring widgets the app never renders.
        </p>

        <figure className="incident-figure" data-reveal aria-labelledby="ch-02-figure-caption">
          <div className="incident-figure__panels">
            <HeightPanel
              bars={MEASURED}
              eyebrow="what the test measured"
              title="Fixture, no stylesheet"
              tone="measured"
            />
            <HeightPanel
              bars={SHIPPED}
              eyebrow="what ships"
              title="The app, with its stylesheet"
              tone="shipped"
            />
          </div>
          <figcaption id="ch-02-figure-caption" className="incident-figure__caption">
            Section header heights, drawn at {SCALE}×. The unstyled fixture measured 20 px for Now
            Playing as a bare label and 34 px for Play Next with a button. The app stylesheet makes
            both uniform at the {FLOOR_PX} px floor.
          </figcaption>
        </figure>

        <blockquote className="incident-quote" data-reveal>
          <p>
            “A geometry assertion against unstyled widgets passes while the shipped button is a
            different size.”
          </p>
          <footer>
            the doc comment on <a href={QUOTE_LINK}>app_css_for_test()</a> — written before the
            incident, naming the trap that produced it
          </footer>
        </blockquote>

        <p className="chapter__intro" data-reveal>
          Since then no pull request may claim <a href={RULE_LINK}>Fixes #444</a> until three
          mutations turn the suite red. If one leaves it green, the claim does not go in.
        </p>

        <div className="incident" data-reveal>
          <p className="incident__eyebrow incident__eyebrow--accent">Fail closed</p>
          <h3 className="incident__title">There is no partial merge.</h3>
        </div>

        <figure className="gate-figure" data-reveal aria-labelledby="ch-02-gate-caption">
          <GateStrip />
          <figcaption id="ch-02-gate-caption" className="gate-figure__caption">
            {GATES.length} checks from <a href={GATE_LINK}>check-merge-readiness.sh</a>. Hover one
            to see what it is; click one to fail it. A red check does not stop the report. It stops
            the merge.
          </figcaption>
        </figure>

        <div className="incident" data-reveal>
          <p className="incident__eyebrow incident__eyebrow--accent">What the checks refuse</p>
          <h3 className="incident__title">Six ways a change can stop short of the branch.</h3>
        </div>

        <div data-reveal>
          <GateGroups />
        </div>

        <p className="chapter__intro chapter__intro--closing" data-reveal>
          A rule ID leads to a test, the test to a commit, the commit to the decision. None of that
          makes an agent trustworthy. It makes trust unnecessary.
        </p>
      </div>
    </section>
  );
}
