import { permalink } from '../../data/measurements';
import './process.css';

const ACTIONS = [
  'Read the AT-SPI tree',
  'Choose and perform an action',
  'Measure the main thread',
  'Report an anomaly',
  'Triage into a rule ID',
  'Named test enters the gate',
] as const;

const FINDINGS = [
  { label: '0 × 0 row', detail: 'visible semantics, impossible geometry' },
  { label: 'Escape swallowed', detail: 'the expected exit never reaches the window' },
  { label: 'scroll hitch', detail: 'a main-thread stall crosses the fixed threshold' },
] as const;

export function ExplorationLoop() {
  return (
    <figure
      className="exploration-loop"
      data-showcase="exploration-loop"
      aria-labelledby="exploration-loop-title"
    >
      <header className="process-heading">
        <p className="eyebrow">The unscripted rung</p>
        <h3 id="exploration-loop-title">The bot chooses the next move. The gate keeps the proof.</h3>
      </header>

      <div className="exploration-loop__body">
        <ol className="exploration-loop__actions" aria-label="Autonomous exploration loop">
          {ACTIONS.map((action, index) => (
            <li key={action}>
              <span className="data">{String(index + 1).padStart(2, '0')}</span>
              <strong>{action}</strong>
            </li>
          ))}
        </ol>

        <aside className="exploration-loop__findings" aria-label="Real findings from exploration">
          <p className="eyebrow">Found in real runs</p>
          <ul>
            {FINDINGS.map((finding) => (
              <li key={finding.label}>
                <strong>{finding.label}</strong>
                <span>{finding.detail}</span>
              </li>
            ))}
          </ul>
          <a className="data" href={permalink('scripts/cua-explore/run.sh')}>
            inspect the fail-closed harness ↗
          </a>
        </aside>
      </div>

      <figcaption className="prose">
        The mission supplies a surface and anomaly thresholds, never a click script. An agent reads
        the live accessibility tree, acts, measures, and leaves a rule-named regression behind.
      </figcaption>
    </figure>
  );
}
