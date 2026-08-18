import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const qualityRoot = fileURLToPath(new URL('.', import.meta.url));
const repositoryRoot = fileURLToPath(new URL('..', import.meta.url));
let targets = process.argv.slice(2);

if (targets.length === 0) {
  const tracked = spawnSync('git', ['ls-files', '*.md'], {
    cwd: repositoryRoot,
    encoding: 'utf8',
  });
  if (tracked.status !== 0) {
    process.stderr.write(tracked.stderr ?? 'Unable to list tracked Markdown files.\n');
    process.exit(1);
  }
  targets = tracked.stdout.split('\n').filter(Boolean);
}

const executable = join(qualityRoot, 'node_modules/.bin/markdownlint-cli2');
const result = spawnSync(
  executable,
  ['--config', '.markdownlint-cli2.jsonc', ...targets],
  {
    cwd: repositoryRoot,
    encoding: 'utf8',
  },
);

process.stdout.write(result.stdout ?? '');
process.stderr.write(result.stderr ?? result.error?.message ?? '');
process.exitCode = result.status ?? 1;
