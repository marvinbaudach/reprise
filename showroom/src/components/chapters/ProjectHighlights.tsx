import { GATES } from 'virtual:merge-gates';
import { PERFORMANCE } from '../../data/measurements';

/** The ledger writes a fall as a signed delta; a sentence says it in words. */
const titleQueryFall = PERFORMANCE[0]?.delta.replace(/^[−-]\s*/, '');

export function ProjectHighlights() {
  return (
    <nav className="highlights frame" aria-label="Project highlights">
      <a href="#ch-01" className="highlight">
        <strong>Desktop, phone, terminal and AI agents share one core.</strong>
        <span>The build keeps GTK, GStreamer and D-Bus out of it.</span>
      </a>
      <a href="#ch-05" className="highlight">
        <strong>Title queries take {titleQueryFall} less time.</strong>
        <span>Measured over 100&apos;000 tracks, with the storage cost on the record.</span>
      </a>
      <a href="#ch-02" className="highlight">
        <strong>{GATES.length} checks guard every merge.</strong>
        <span>They cover architecture, behaviour and packaging.</span>
      </a>
    </nav>
  );
}
