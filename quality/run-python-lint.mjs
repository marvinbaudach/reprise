import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const repositoryRoot = fileURLToPath(new URL('..', import.meta.url));
const result = spawnSync(
  'uvx',
  ['ruff@0.16.0', 'check', '--config', 'ruff.toml', '.'],
  {
    cwd: repositoryRoot,
    encoding: 'utf8',
  },
);

process.stdout.write(result.stdout ?? '');
process.stderr.write(result.stderr ?? '');
process.exitCode = result.status ?? 1;
