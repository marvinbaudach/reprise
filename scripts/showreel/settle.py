"""When does the shot's own page actually arrive?

The window title is the witness: take the strip from the end of the shot, where
the page is right, and walk forward from the in-point until it matches. The
strip is cropped tight around the title and NOT downscaled — a downscaled strip
makes 'Music' and 'Releases' differ by less than the noise floor, which is how
the first version of this reported that every page was already settled.
"""
import subprocess
import sys
import numpy as np

CROP = 'crop=2880:1747:0:53,crop=560:44:1160:4'

def strip(path, t):
    raw = subprocess.run(
        ['ffmpeg', '-v', 'error', '-ss', f'{t:.2f}', '-i', path, '-frames:v', '1',
         '-vf', CROP, '-f', 'rawvideo', '-pix_fmt', 'gray', '-'],
        check=True, stdout=subprocess.PIPE).stdout
    return np.frombuffer(raw, dtype=np.uint8).astype(float)

for spec in sys.argv[1:]:
    name, path, start, dur = spec.split(',')
    start, dur = float(start), float(dur)
    ref = strip(path, start + dur - 0.3)
    ts = np.arange(start - 1.0, start + dur + 8.0, 0.2)
    diff = [np.abs(strip(path, t) - ref).mean() for t in ts]
    floor = min(diff)
    thresh = floor + 1.0
    settle = next((t for t, d in zip(ts, diff) if d <= thresh and t >= start - 1.0), None)
    print(f'{name:<12} in={start:6.1f} dur={dur:4.1f} floor={floor:5.2f} '
          f'settles={settle:6.1f}  diff@in={diff[5]:5.2f}')
