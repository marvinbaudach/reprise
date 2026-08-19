import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig, type Plugin } from 'vite';

const GATE_SCRIPT = fileURLToPath(new URL('../scripts/check-merge-readiness.sh', import.meta.url));
const PIPELINE_DOC = fileURLToPath(new URL('../docs/agents/pipeline.md', import.meta.url));

const MERGE_GATES = 'virtual:merge-gates';
const AGENT_PIPELINE = 'virtual:agent-pipeline';

interface PipelineStep {
  readonly step: string;
  readonly phase: string;
  readonly actor: string;
  readonly writes: boolean;
  readonly judges: boolean;
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

/**
 * Two facts the page states are read out of the repository at build time rather
 * than typed next to the words: the checks the merge gate runs, and who runs
 * which step of the pipeline. Changing either source changes the page — or turns
 * a test red, which is the point.
 */
function derivedFacts(): Plugin {
  const resolved = new Map([
    [MERGE_GATES, `\0${MERGE_GATES}`],
    [AGENT_PIPELINE, `\0${AGENT_PIPELINE}`],
  ]);
  return {
    name: 'reprise-derived-facts',
    resolveId(id) {
      return resolved.get(id) ?? null;
    },
    load(id) {
      if (id === resolved.get(MERGE_GATES)) {
        return `export const GATES = ${JSON.stringify(readGates())};\n`;
      }
      if (id === resolved.get(AGENT_PIPELINE)) {
        return `export const PIPELINE = ${JSON.stringify(readPipeline())};\n`;
      }
      return null;
    },
    configureServer(server) {
      // Neither source lives under the Vite root, so the dev server does not
      // watch them on its own.
      server.watcher.add([GATE_SCRIPT, PIPELINE_DOC]);
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
    cssCodeSplit: false,
    assetsInlineLimit: 2048,
  },
});
