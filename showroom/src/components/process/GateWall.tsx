import { GATES } from 'virtual:merge-gates';
import { type CSSProperties, useState } from 'react';
import { permalink } from '../../data/measurements';
import { readout, toggle } from '../../lib/mergeGates';
import './process.css';

export function GateWall() {
  const [failed, setFailed] = useState<ReadonlySet<string>>(() => new Set<string>());
  const status = readout(failed, GATES.length);

  return (
    <figure
      className="gate-wall"
      data-showcase="gate-wall"
      data-reveal
      aria-labelledby="gate-wall-title"
    >
      <header className="process-heading">
        <p className="eyebrow">Fail closed</p>
        <h3 id="gate-wall-title">
          {GATES.length} checks decide. Fail one and the change does not land.
        </h3>
      </header>

      {/*
       * The cells are buttons because the claim is testable: fail a check and
       * the readout blocks. Believing it is optional.
       */}
      <ul className="gate-wall__grid" data-gates={GATES.length}>
        {GATES.map((name, index) => {
          const broken = failed.has(name);
          return (
            <li key={name}>
              <button
                type="button"
                className="gate-wall__cell"
                data-gate={name}
                aria-pressed={broken}
                style={{ '--cell-delay': `${index * 28}ms` } as CSSProperties}
                onClick={() => setFailed((current) => toggle(current, name))}
              >
                <span className="data gate-wall__index">{String(index + 1).padStart(2, '0')}</span>
                <span className="gate-wall__name">{name}</span>
              </button>
            </li>
          );
        })}
      </ul>

      <p
        className="gate-wall__readout"
        data-blocked={status.blocked ? 'true' : 'false'}
        role="status"
        aria-live="polite"
      >
        {status.message}
      </p>

      <figcaption className="gate-wall__source">
        Every cell is one <code>gate</code> call in{' '}
        <a href={permalink('scripts/check-merge-readiness.sh')}>check-merge-readiness.sh ↗</a>, read
        out of the script when this page is built.
      </figcaption>
    </figure>
  );
}
