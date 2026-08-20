import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig, type Plugin } from 'vite';
import { census } from './derive/code-census.mjs';

const REPO_ROOT = fileURLToPath(new URL('..', import.meta.url));
const GATE_SCRIPT = fileURLToPath(new URL('../scripts/check-merge-readiness.sh', import.meta.url));
const TIMELINE_DOC = fileURLToPath(new URL('../docs/showroom/timeline.md', import.meta.url));
const LEDGER_DOC = fileURLToPath(new URL('../docs/measurements/index-rebuild.md', import.meta.url));
const INCIDENT_DOC = fileURLToPath(
  new URL('../docs/plans/queue-anchor-grill-followups.md', import.meta.url),
);
const SPECTRAL_SOURCE = fileURLToPath(
  new URL('../crates/reprise-view/src/spectral_colour.rs', import.meta.url),
);

const GATE_COUNT_TOKEN = '%GATE_COUNT%';

const MERGE_GATES = 'virtual:merge-gates';
const CODE_CENSUS = 'virtual:code-census';
const BUILD_TIMELINE = 'virtual:build-timeline';
const MEASUREMENTS = 'virtual:measurements';
const SPECTRAL_AXIS = 'virtual:spectral-axis';
const INCIDENT = 'virtual:incident';

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

interface GateGroup {
  readonly name: string;
  readonly line: string;
  readonly gates: readonly string[];
}

interface GateGroupDefinition {
  readonly name: string;
  readonly line: string;
  readonly checks: readonly string[];
}

/**
 * Coverage categories for the merge gate. The check names are the assignment;
 * their order and the displayed counts still come from the parsed gate calls.
 * Keeping this beside `readGates()` makes a new or renamed check fail the build
 * instead of silently falling out of the public figure.
 */
const GATE_GROUP_ASSIGNMENTS: readonly GateGroupDefinition[] = [
  {
    name: 'Boundaries',
    line: 'The core cannot grow a UI framework.',
    checks: ['Architecture', 'Device-sync GStreamer', 'Frontend thinness', 'GNOME idioms'],
  },
  {
    name: 'Distribution',
    line: 'It installs as a desktop app, not as a demo.',
    checks: [
      'Gettext catalogues',
      'Runtime service install',
      'AppStream',
      'Flatpak manifest',
      'Dependency audit',
    ],
  },
  {
    name: 'Reachable',
    line: 'Every action works without a mouse.',
    checks: ['Accessibility semantics', 'Input parity', 'Motion tokens'],
  },
  {
    name: 'Traceable',
    line: 'A rule without a test fails the build.',
    checks: ['UX traceability', 'AI hygiene', 'Rule-owned display tests'],
  },
  {
    name: 'Green means green',
    line: 'Tests, lints, formatting, documented API.',
    checks: [
      'Project quality',
      'Rust formatting',
      'Rust lint',
      'Rust documentation',
      'Workspace tests',
      'Linux platform tests',
      'Runtime service bus tests',
    ],
  },
  {
    name: 'Toolchain hygiene',
    line: 'The branch, the shell scripts, the worktrees.',
    checks: ['Branch diff', 'Shell', 'Worktree GC', 'Worktree GC schedule', 'Script self-tests'],
  },
];

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

function groupGates(gates: readonly string[]): readonly GateGroup[] {
  const assigned = new Map<string, string>();

  for (const group of GATE_GROUP_ASSIGNMENTS) {
    for (const check of group.checks) {
      if (!gates.includes(check)) {
        throw new Error(`gate group "${group.name}" assigns missing check "${check}"`);
      }
      const previous = assigned.get(check);
      if (previous !== undefined) {
        throw new Error(`gate "${check}" is assigned to both "${previous}" and "${group.name}"`);
      }
      assigned.set(check, group.name);
    }
  }

  const unassigned = gates.filter((gate) => !assigned.has(gate));
  if (unassigned.length > 0) {
    throw new Error(`merge checks have no coverage group: ${unassigned.join(', ')}`);
  }

  return GATE_GROUP_ASSIGNMENTS.map((group) => ({
    name: group.name,
    line: group.line,
    gates: gates.filter((gate) => group.checks.includes(gate)),
  }));
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

/**
 * The incident date, out of the record that decided it.
 *
 * The three heights are deliberately typed in the figure because the record is
 * historical evidence, not a changing data source. The date alone is read here
 * because it also happens to be a timeline boundary, and the timeline contract
 * rightly rejects that date when a component copies it.
 */
function readIncident(): { readonly date: string } {
  const text = readFileSync(INCIDENT_DOC, 'utf8');

  const date = text.match(/^# .*\((\d{4}-\d{2}-\d{2})\)\s*$/m)?.[1];
  if (date === undefined) {
    throw new Error(`found no incident date in the heading of ${INCIDENT_DOC}`);
  }

  return { date };
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
 * than typed next to the words: the checks the merge gate runs, how many lines
 * of what kind the tree holds, the weeks the work took, the index rebuild's
 * ledger, and the two ends of the spectral axis. Changing any source changes the
 * page — or turns a test red, which is the point.
 *
 * The gate count reaches `index.html` the same way. A meta description is the
 * first number a reader sees — in a search result, in every link unfurl — and it
 * used to be the one number on this page that was typed. It said 21 while the
 * page derived 27.
 */
function derivedFacts(): Plugin {
  const resolved = new Map(
    [MERGE_GATES, CODE_CENSUS, BUILD_TIMELINE, MEASUREMENTS, SPECTRAL_AXIS, INCIDENT].map((id) => [
      id,
      `\0${id}`,
    ]),
  );
  const modules = new Map<string, () => string>([
    [
      MERGE_GATES,
      () => {
        const gates = readGates();
        return (
          `export const GATES = ${JSON.stringify(gates)};\n` +
          `export const GATE_GROUPS = ${JSON.stringify(groupGates(gates))};\n`
        );
      },
    ],
    [CODE_CENSUS, () => `export const CENSUS = ${JSON.stringify(census(REPO_ROOT))};\n`],
    [BUILD_TIMELINE, () => `export const TIMELINE = ${JSON.stringify(readTimeline())};\n`],
    [MEASUREMENTS, () => `export const INDEX_REBUILD = ${JSON.stringify(readLedger())};\n`],
    [SPECTRAL_AXIS, () => `export const AXIS = ${JSON.stringify(readSpectralAxis())};\n`],
    [INCIDENT, () => `export const INCIDENT = ${JSON.stringify(readIncident())};\n`],
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
    transformIndexHtml(html) {
      const count = String(readGates().length);
      if (!html.includes(GATE_COUNT_TOKEN)) {
        throw new Error(
          `${GATE_COUNT_TOKEN} is not in index.html — the meta description would ship a stale count`,
        );
      }
      return html.replaceAll(GATE_COUNT_TOKEN, count);
    },
    configureServer(server) {
      // None of these sources live under the Vite root, so the dev server does
      // not watch them on its own. The census reads the whole tree; watching the
      // two crate roots it counts is what makes an edit there show up.
      server.watcher.add([
        GATE_SCRIPT,
        TIMELINE_DOC,
        LEDGER_DOC,
        INCIDENT_DOC,
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
