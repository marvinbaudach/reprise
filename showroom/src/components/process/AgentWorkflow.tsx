import { permalink, treelink } from '../../data/measurements';
import './process.css';

const WORKFLOW = [
  { label: 'Plan', detail: 'The claim and its acceptance proof' },
  {
    label: 'Human checkpoint',
    detail: 'The plan is challenged before code exists',
    role: 'human-checkpoint',
  },
  { label: 'Implement', detail: 'One agent writes code and tests', role: 'implementer' },
  { label: 'Review', detail: 'A fresh agent reviews each language', role: 'reviewer' },
  { label: 'Refute', detail: 'A skeptic tries to disprove every finding', role: 'skeptic' },
  { label: 'Refactor', detail: 'Only surviving findings change the patch' },
  { label: 'Gate', detail: '21 checks decide whether it can land' },
] as const;

export function AgentWorkflow() {
  return (
    <figure
      className="agent-workflow"
      data-showcase="agent-workflow"
      data-reveal
      aria-labelledby="agent-workflow-title"
    >
      <header className="process-heading">
        <p className="eyebrow">Authorship is not authority</p>
        <h3 id="agent-workflow-title">One checkpoint. Independent agents. Mechanical proof.</h3>
      </header>

      <ol className="agent-workflow__path" aria-label="A change from plan to merge gate">
        {WORKFLOW.map((step, index) => (
          <li
            className="agent-workflow__step"
            data-reveal
            data-role={'role' in step ? step.role : undefined}
            key={step.label}
          >
            <span className="data agent-workflow__index">{String(index + 1).padStart(2, '0')}</span>
            <strong>{step.label}</strong>
            <span>{step.detail}</span>
          </li>
        ))}
      </ol>

      <div className="agent-workflow__independence">
        <strong>The writer never reviews.</strong>
        <strong>The reviewer never writes.</strong>
        <span>The skeptic cannot apply findings.</span>
      </div>

      <footer className="agent-workflow__context">
        <p className="data">Plan files · rulebook · handovers · decision records</p>
        <nav aria-label="Workflow context sources">
          <a href={treelink('docs/plans')}>plans ↗</a>
          <a href={permalink('docs/ux-rules.md')}>rulebook ↗</a>
          <a href={permalink('.superpowers/sdd/progress.md')}>ledger ↗</a>
        </nav>
      </footer>
    </figure>
  );
}
