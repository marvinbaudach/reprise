import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig, type Plugin } from 'vite';
import { census } from './derive/code-census.mjs';

const REPO_ROOT = fileURLToPath(new URL('..', import.meta.url));
const GATE_SCRIPT = fileURLToPath(new URL('../scripts/check-merge-readiness.sh', import.meta.url));
const PIPELINE_DOC = fileURLToPath(new URL('../docs/agents/pipeline.md', import.meta.url));
const TIMELINE_DOC = fileURLToPath(new URL('../docs/showroom/timeline.md', import.meta.url));
const LEDGER_DOC = fileURLToPath(new URL('../docs/measurements/index-rebuild.md', import.meta.url));
const SPECTRAL_SOURCE = fileURLToPath(
  new URL('../crates/reprise-view/src/spectral_colour.rs', import.meta.url),
);

const MERGE_GATES = 'virtual:merge-gates';
const AGENT_PIPELINE = 'virtual:agent-pipeline';
const CODE_CENSUS = 'virtual:code-census';
const BUILD_TIMELINE = 'virtual:build-timeline';
const MEASUREMENTS = 'virtual:measurements';
const SPECTRAL_AXIS = 'virtual:spectral-axis';

interface PipelineStep {
  readonly step: string;
  readonly phase: string;
  readonly actor: string;
  readonly writes: boolean;
  readonly judges: boolean;
}

interface TimelineWeek {
  readonly week: number;
  readonly from: string;
  readonly to: string;
  readonly theme: string;
  readonly landed: string;
}

interface LedgerRow {
  readonly what: string;
  readonly before: string;
  readonly after: string;
  readonly delta: string;
  readonly commit: string;
  readonly date: string;
  readonly method: string;
}

interface Ledger {
  readonly rows: readonly LedgerRow[];
  readonly price: string;
}

/**
 * The gate names, in script order. One `gate "<name>"` call is one check; the
 * preparation steps above them are preconditions and carry no `gate` call, which
 * is what keeps this count honest.
 */
function readGates(): readonly string[] {
  const text = readFileSync(GATE_SCRIPT, 'utf8');
  const names: string[] = [];
  for (const match of text.matchAll(/^gate "([^"]+)"/gm)) {
    const name = match[1];
    if (name !== undefined) names.push(name);
  }
  // An empty derivation is the one failure that would look like success: a wall
  // of zero cells and the number 0, silently agreeing with itself.
  if (names.length === 0) {
    throw new Error(
      `derived no gate names from ${GATE_SCRIPT} — the expression or the script moved`,
    );
  }
  return names;
}

function readPipeline(): readonly PipelineStep[] {
  const text = readFileSync(PIPELINE_DOC, 'utf8');
  const flag = (raw: string, column: string, step: string): boolean => {
    const value = raw.trim();
    if (value === 'yes') return true;
    if (value === 'no') return false;
    throw new Error(
      `step ${step} has "${value}" under ${column} in ${PIPELINE_DOC} — expected yes or no`,
    );
  };

  const steps: PipelineStep[] = [];
  for (const match of text.matchAll(/^\|\s*(\d{2})\s*\|(.+?)\|(.+?)\|(.+?)\|(.+?)\|\s*$/gm)) {
    const [, step, phase, actor, writes, judges] = match;
    if (
      step === undefined ||
      phase === undefined ||
      actor === undefined ||
      writes === undefined ||
      judges === undefined
    ) {
      throw new Error(`a pipeline row in ${PIPELINE_DOC} is missing columns`);
    }
    steps.push({
      step,
      phase: phase.trim(),
      actor: actor.trim(),
      writes: flag(writes, 'Writes', step),
      judges: flag(judges, 'Judges', step),
    });
  }

  if (steps.length === 0) {
    throw new Error(
      `derived no pipeline steps from ${PIPELINE_DOC} — the table or the expression moved`,
    );
  }
  return steps;
}

const DAY_MS = 86_400_000;

/** An ISO day, or an error naming the row it came from. */
function isoDay(raw: string, where: string): number {
  const stamp = Date.parse(`${raw}T00:00:00Z`);
  if (Number.isNaN(stamp)) {
    throw new Error(`${where} in ${TIMELINE_DOC} has "${raw}", which is not an ISO date`);
  }
  if (new Date(stamp).toISOString().slice(0, 10) !== raw) {
    throw new Error(`${where} in ${TIMELINE_DOC} has "${raw}", which is not a day that exists`);
  }
  return stamp;
}

/**
 * The weeks, out of the record that decided them.
 *
 * Every assertion here throws instead of shrinking. A timeline that quietly
 * loses its last row still looks finished — it just tells a shorter story than
 * the truth, and nothing on the page could contradict it, because the page reads
 * its week count from this same list.
 */
function readTimeline(): readonly TimelineWeek[] {
  const text = readFileSync(TIMELINE_DOC, 'utf8');
  const weeks: TimelineWeek[] = [];
  const row =
    /^\|\s*(\d+)\s*\|\s*(\d{4}-\d{2}-\d{2})\s*(?:…|\.\.\.)\s*(\d{4}-\d{2}-\d{2})\s*\|([^|]+)\|(.+?)\|\s*$/gm;

  // The first table only. The record carries a second one under `## The
  // anchors` with the same column count, and a row of it that ever happened to
  // start with a date range would otherwise be read as a sixth week.
  const firstTable = text.split(/\n#{1,6} /)[0] ?? text;

  for (const match of firstTable.matchAll(row)) {
    const [, week, from, to, theme, landed] = match;
    if (
      week === undefined ||
      from === undefined ||
      to === undefined ||
      theme === undefined ||
      landed === undefined
    ) {
      throw new Error(`a timeline row in ${TIMELINE_DOC} is missing columns`);
    }
    weeks.push({
      week: Number(week),
      from,
      to,
      theme: theme.trim(),
      landed: landed.trim(),
    });
  }

  if (weeks.length === 0) {
    throw new Error(`derived no weeks from ${TIMELINE_DOC} — the table or the expression moved`);
  }

  let previousEnd: number | null = null;
  weeks.forEach((entry, index) => {
    const where = `week ${entry.week}`;
    const from = isoDay(entry.from, `${where} start`);
    const to = isoDay(entry.to, `${where} end`);

    if (entry.week !== index + 1) {
      throw new Error(
        `${where} is row ${index + 1} in ${TIMELINE_DOC} — the weeks are not in order`,
      );
    }
    if (to <= from) {
      throw new Error(`${where} in ${TIMELINE_DOC} ends on or before it starts`);
    }
    if (previousEnd !== null) {
      const expected = new Date(previousEnd + DAY_MS).toISOString().slice(0, 10);
      if (entry.from !== expected) {
        throw new Error(
          `${where} in ${TIMELINE_DOC} starts on ${entry.from}, but the week before it ends the day ` +
            `before ${expected} — the weeks must meet exactly, with no gap and no overlap`,
        );
      }
    }
    previousEnd = to;
  });

  return weeks;
}

/**
 * The index rebuild's before/after figures, out of the record that carries their
 * provenance. These are the one group the build cannot count: they describe a
 * change that happened once, not a state of the tree. Quoting them from a file
 * with a commit, a date and a method beside each row is the closest thing to a
 * measurement the page can honestly offer.
 */
function readLedger(): Ledger {
  const text = readFileSync(LEDGER_DOC, 'utf8');
  const rows: LedgerRow[] = [];
  const row = /^\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|\s*$/gm;

  for (const match of text.matchAll(row)) {
    const cells = match.slice(1).map((cell) => cell.trim());
    const [what, before, after, delta, commit, date, method] = cells;
    if (
      what === undefined ||
      before === undefined ||
      after === undefined ||
      delta === undefined ||
      commit === undefined ||
      date === undefined ||
      method === undefined
    ) {
      throw new Error(`a ledger row in ${LEDGER_DOC} is missing columns`);
    }
    // The header and its separator match the same shape as a row.
    if (what === 'What' || /^-+$/.test(what)) continue;
    rows.push({ what, before, after, delta, commit, date, method });
  }

  if (rows.length === 0) {
    throw new Error(
      `derived no measurements from ${LEDGER_DOC} — the table or the expression moved`,
    );
  }

  // Sliced rather than matched: a lazy pattern with an end-of-input alternative
  // is happy to stop at the first line break under /m, which would quote the
  // price as its first word and look like a formatting choice.
  const heading = '\n## The price\n';
  const at = text.indexOf(heading);
  if (at === -1) {
    throw new Error(`found no "## The price" heading in ${LEDGER_DOC}`);
  }
  const rest = text.slice(at + heading.length);
  const nextHeading = rest.search(/^#{1,6} /m);
  const price = (nextHeading === -1 ? rest : rest.slice(0, nextHeading)).trim();
  if (price.length === 0) {
    throw new Error(`the "## The price" section in ${LEDGER_DOC} is empty`);
  }

  return { rows, price: price.replace(/\s*\n\s*/g, ' ') };
}

/** `pub const CORAL: (u8, u8, u8) = (255, 111, 94);` — the axis, from its function. */
function readSpectralAxis(): { readonly coral: string; readonly teal: string } {
  const text = readFileSync(SPECTRAL_SOURCE, 'utf8');
  const hex = (name: string): string => {
    const match = text.match(
      new RegExp(`pub const ${name}: \\(u8, u8, u8\\) = \\((\\d+), (\\d+), (\\d+)\\);`),
    );
    if (match === null) {
      throw new Error(`found no ${name} constant in ${SPECTRAL_SOURCE} — the axis moved`);
    }
    const channels = match.slice(1, 4).map(Number);
    for (const channel of channels) {
      if (!Number.isInteger(channel) || channel < 0 || channel > 255) {
        throw new Error(`${name} in ${SPECTRAL_SOURCE} has a channel outside 0–255`);
      }
    }
    return `#${channels.map((channel) => channel.toString(16).padStart(2, '0')).join('')}`.toUpperCase();
  };
  return { coral: hex('CORAL'), teal: hex('TEAL') };
}

/**
 * The facts the page states are read out of the repository at build time rather
 * than typed next to the words: the checks the merge gate runs, who runs which
 * step of the pipeline, how many lines of what kind the tree holds, the weeks
 * the work took, the index rebuild's ledger, and the two ends of the spectral
 * axis. Changing any source changes the page — or turns a test red, which is the
 * point.
 */
function derivedFacts(): Plugin {
  const resolved = new Map(
    [MERGE_GATES, AGENT_PIPELINE, CODE_CENSUS, BUILD_TIMELINE, MEASUREMENTS, SPECTRAL_AXIS].map(
      (id) => [id, `\0${id}`],
    ),
  );
  const modules = new Map<string, () => string>([
    [MERGE_GATES, () => `export const GATES = ${JSON.stringify(readGates())};\n`],
    [AGENT_PIPELINE, () => `export const PIPELINE = ${JSON.stringify(readPipeline())};\n`],
    [CODE_CENSUS, () => `export const CENSUS = ${JSON.stringify(census(REPO_ROOT))};\n`],
    [BUILD_TIMELINE, () => `export const TIMELINE = ${JSON.stringify(readTimeline())};\n`],
    [MEASUREMENTS, () => `export const INDEX_REBUILD = ${JSON.stringify(readLedger())};\n`],
    [SPECTRAL_AXIS, () => `export const AXIS = ${JSON.stringify(readSpectralAxis())};\n`],
  ]);

  return {
    name: 'reprise-derived-facts',
    resolveId(id) {
      return resolved.get(id) ?? null;
    },
    load(id) {
      for (const [virtualId, marker] of resolved) {
        if (id === marker) return modules.get(virtualId)?.() ?? null;
      }
      return null;
    },
    configureServer(server) {
      // None of these sources live under the Vite root, so the dev server does
      // not watch them on its own. The census reads the whole tree; watching the
      // two crate roots it counts is what makes an edit there show up.
      server.watcher.add([
        GATE_SCRIPT,
        PIPELINE_DOC,
        TIMELINE_DOC,
        LEDGER_DOC,
        SPECTRAL_SOURCE,
        `${REPO_ROOT}crates`,
        `${REPO_ROOT}android`,
      ]);
    },
  };
}

// Pages serves the site from https://marvinbaudach.github.io/reprise/, so every
// asset URL needs that prefix. Getting this wrong produces a page that loads
// locally and 404s in production — the one failure mode a preview never shows.
export default defineConfig({
  base: '/reprise/',
  plugins: [react(), derivedFacts()],
  build: {
    target: 'es2022',
    // Vite 8 minifies CSS with Lightning CSS against a widely-available baseline,
    // and that baseline still predates `oklch()`: every colour then ships twice,
    // once as a hex approximation and once as `lab()`. The whole palette is
    // authored in oklch, so the site would go out in a downlevelled colour space
    // it never asked for — and the built stylesheet, which the display rules are
    // asserted against, would stop showing the colours the design speaks in.
    // These four are the floor for `oklch()` support; below it there is no site
    // to look at anyway.
    cssTarget: ['chrome120', 'edge120', 'firefox120', 'safari17'],
    cssCodeSplit: false,
    assetsInlineLimit: 2048,
  },
});
