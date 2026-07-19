def p95:
  (.samples_us | sort) as $samples
  | $samples[((($samples | length) * 95 / 100) | ceil) - 1];
def maximum: (.samples_us | max);

($baseline[0] | p95) as $baseline_p95
| ($glass[0] | p95) as $glass_p95
| ($glass[0] | maximum) as $glass_max
| ([($glass_p95 - $baseline_p95), 0] | max) as $overhead
| {
    schema_version: 1,
    duration_kind: "before-paint-to-after-paint-wall-us",
    baseline_renderer: $baseline[0].renderer,
    glass_renderer: $glass[0].renderer,
    baseline_frames: ($baseline[0].samples_us | length),
    glass_frames: ($glass[0].samples_us | length),
    baseline_p95_us: $baseline_p95,
    glass_p95_us: $glass_p95,
    glass_max_us: $glass_max,
    overhead_p95_us: $overhead,
    budgets_us: {glass_p95: 20000, glass_max: 50000, overhead_p95: 3000},
    pass: (($baseline[0].samples_us | length) >= 120
      and ($glass[0].samples_us | length) >= 120
      and $glass_p95 <= 20000
      and $glass_max <= 50000
      and $overhead <= 3000)
  }
