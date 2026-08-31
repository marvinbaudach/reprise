"""One yardstick for every candidate.

Usage: score-candidates.py SECONDS TRACK...

pick-window.py degenerates on exact-length material: with a 60.0 s window and a
60.024 s file there is exactly one candidate window, and with a 60.000 s file
there is none at all (match -2.000, quietest -99 dB are sentinels, not
measurements). So score every candidate over the same 60 s stretch, and run the
hole check where score.sh does not already fade: 3.0 s to 58.8 s.
"""
import importlib.util, subprocess, sys
import numpy as np

spec = importlib.util.spec_from_file_location(
    'pw', '/home/marvin/Projects/reprise-showreel/scripts/showreel/pick-window.py')
pw = importlib.util.module_from_spec(spec); spec.loader.exec_module(pw)

SR, RATE = pw.SR, pw.SR / pw.FRAME
SECONDS = float(sys.argv[1])
sys.argv = sys.argv[:1] + sys.argv[2:]
arc = pw.target_arc(SECONDS, RATE)

def best_start(path, length):
    if length < SECONDS + 1.0:
        return 0.0
    out = subprocess.run(['python3', '/home/marvin/Projects/reprise-showreel/scripts/showreel/pick-window.py',
                          path, str(SECONDS), '100.0'], capture_output=True, text=True).stdout
    return float([l.split()[1] for l in out.splitlines() if l.startswith('start')][0])

print(f"{'candidate':<32}{'start':>7}{'corr':>8}{f'hole(3-{SECONDS-1.2:.1f}s)':>15}{'head':>7}{'tail':>7}")
for path in sys.argv[1:]:
    x = pw.load_mono(path)
    length = len(x) / SR
    start = best_start(path, length)
    env = pw.envelope(x)[int(start * RATE):][:len(arc)]
    corr = float(np.corrcoef(env, arc)[0, 1])
    blk = int(2 * RATE)
    lo, hi = int(3.0 * RATE), int((SECONDS - 1.2) * RATE)
    inner = env[lo:hi]
    b = inner[:len(inner) // blk * blk].reshape(-1, blk).mean(axis=1)
    med = np.median(env)
    hole = 20 * np.log10((b.min() + 1e-12) / (med + 1e-12))
    head = 20 * np.log10((env[:blk].mean() + 1e-12) / (med + 1e-12))
    tail = 20 * np.log10((env[-blk:].mean() + 1e-12) / (med + 1e-12))
    print(f'{path:<32}{start:>7.1f}{corr:>8.3f}{hole:>13.1f} dB{head:>6.0f}dB{tail:>6.0f}dB')
