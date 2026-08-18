import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const repositoryRoot = fileURLToPath(new URL('..', import.meta.url));
let targets = process.argv.slice(2);

if (targets.length === 0) {
  const tracked = spawnSync('git', ['ls-files', '*.yaml', '*.yml'], {
    cwd: repositoryRoot,
    encoding: 'utf8',
  });
  if (tracked.status !== 0) {
    process.stderr.write(tracked.stderr ?? 'Unable to list tracked YAML files.\n');
    process.exit(1);
  }
  targets = ['.yamllint.yaml', ...tracked.stdout.split('\n').filter(Boolean)];
}

const result = spawnSync(
  'uvx',
  ['yamllint@1.38.0', '--strict', '--config-file', '.yamllint.yaml', ...targets],
  {
    cwd: repositoryRoot,
    encoding: 'utf8',
  },
);

process.stdout.write(result.stdout ?? '');
process.stderr.write(result.stderr ?? '');
process.exitCode = result.status ?? 1;
