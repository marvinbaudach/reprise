//! The Grid visual's thin membrane — a spring-mesh height field driven from
//! its center like a bass loudspeaker. Low-band energy moves one underdamped
//! central driver and beat impulses accelerate the same radial speaker cone.
//! The cloth begins moving on the detected frame, builds its crest over a
//! short fluid attack, and then lets the underdamped driver and mesh propagate
//! the motion outward. Signed heights let the surface rise toward the viewer
//! and then fall into depth before damping back to rest.

use crate::playback::SPECTRUM_BAND_COUNT;

pub const MEMBRANE_ROWS: usize = 26;
pub const MEMBRANE_COLS: usize = 44;
const CELLS: usize = MEMBRANE_ROWS * MEMBRANE_COLS;

/// Fixed physics step: the visual tick loop always advances by this much,
/// never by a wall-clock delta.
const DT: f32 = 1.0 / 60.0;
/// Below this every cell reads as fully rested (`is_still`).
const REST_EPSILON: f32 = 0.01;

/// The first display bands are folded into the loudspeaker drive. Keeping the
/// range narrow makes kick and bass move the whole membrane while mids and
/// highs remain visible through the analyzer's overall impact response.
const BASS_BANDS: usize = 8;
/// Low-band energy below this injects no drive — quiet analysis noise leaves
/// the membrane perfectly still.
const DRIVE_GATE: f32 = 0.08;
/// Full-scale bass target of the scalar central driver.
const DRIVER_TARGET_AMPLITUDE: f32 = 0.28;
/// Driver oscillator coefficients. The damping is deliberately below critical
/// (`2*sqrt(DRIVER_SPRING)`), so a beat rises and then falls below the resting
/// plane instead of easing monotonically back to zero.
const DRIVER_SPRING: f32 = 165.0;
const DRIVER_DAMPING: f32 = 11.0;
/// Velocity kick applied by a full-strength detected beat.
const BEAT_IMPULSE: f32 = 8.0;
const BEAT_DRIVER_TARGET: f32 = 1.0;
/// Broad velocity kick applied directly to the cloth. Position is never
/// teleported: the kick produces a short visible attack through normal
/// integration, like a thin loudspeaker membrane accelerating from rest.
const BEAT_SURFACE_IMPULSE: f32 = 25.0;
const BEAT_SURFACE_TARGET: f32 = 1.8;
const BEAT_RADIUS: f32 = 0.42;
const DRIVER_POSITION_LIMIT: f32 = 1.8;
const DRIVER_VELOCITY_LIMIT: f32 = 28.0;
/// Gaussian radius of the central loudspeaker, in normalized membrane space
/// (`x` and `y` each span `-1..=1`).
const DRIVER_RADIUS: f32 = 0.22;
/// How tightly the cloth follows the scalar driver inside that Gaussian.
const DRIVER_COUPLING: f32 = 195.0;
/// Neighbor spring coupling: how fast a disturbance propagates radially across
/// the mesh away from the driver.
const SPRING: f32 = 70.0;
/// Horizontal cells are denser than depth cells on the rectangular physics
/// lattice. Weight their second derivative by the squared spacing ratio so a
/// wave travels the same normalized distance in every direction.
const CELL_SPACING_RATIO: f32 = (MEMBRANE_COLS - 1) as f32 / (MEMBRANE_ROWS - 1) as f32;
const COL_SPRING_SCALE: f32 = CELL_SPACING_RATIO * CELL_SPACING_RATIO;
/// Restoring pull back toward a flat surface.
const RESTORING: f32 = 4.0;
/// Velocity damping rate; light enough for one cloth-like depth stroke to
/// travel, strong enough that its free rebound cannot resemble another beat.
const DAMP_RATE: f32 = 4.2;
const HEIGHT_MIN: f32 = -1.35;
const HEIGHT_MAX: f32 = 2.0;

fn radial_weight(row: usize, col: usize, radius: f32) -> f32 {
    let y = (2 * row as isize - (MEMBRANE_ROWS - 1) as isize) as f32 / (MEMBRANE_ROWS - 1) as f32;
    let x = (2 * col as isize - (MEMBRANE_COLS - 1) as isize) as f32 / (MEMBRANE_COLS - 1) as f32;
    (-(x * x + y * y) / (2.0 * radius * radius)).exp()
}

fn driver_weight(row: usize, col: usize) -> f32 {
    radial_weight(row, col, DRIVER_RADIUS)
}

/// Height (`h`) and velocity (`v`) field for every grid cell plus the one
/// underdamped bass-driver oscillator at its center.
pub struct Membrane {
    h: [f32; CELLS],
    v: [f32; CELLS],
    driver_position: f32,
    driver_velocity: f32,
}

impl Default for Membrane {
    fn default() -> Self {
        Self::new()
    }
}

impl Membrane {
    pub fn new() -> Self {
        Self {
            h: [0.0; CELLS],
            v: [0.0; CELLS],
            driver_position: 0.0,
            driver_velocity: 0.0,
        }
    }

    /// One fixed 60 Hz step. Low-band energy sets the scalar driver's target;
    /// the driver's underdamped spring and any beat velocity impulse create a
    /// signed speaker-cone motion. A circular Gaussian couples that motion into
    /// the cloth, then the four-neighbor spring mesh carries it toward the
    /// edges. All cells are free to overshoot above and below rest.
    pub fn advance(&mut self, bands: &[f32; SPECTRUM_BAND_COUNT]) {
        let raw_bass = bands[..BASS_BANDS].iter().sum::<f32>() / BASS_BANDS as f32;
        let bass = if raw_bass < DRIVE_GATE {
            0.0
        } else {
            ((raw_bass - DRIVE_GATE) / (1.0 - DRIVE_GATE)).clamp(0.0, 1.0)
        };
        let target = bass * DRIVER_TARGET_AMPLITUDE;
        let driver_acceleration =
            (target - self.driver_position) * DRIVER_SPRING - self.driver_velocity * DRIVER_DAMPING;
        self.driver_velocity = (self.driver_velocity + driver_acceleration * DT)
            .clamp(-DRIVER_VELOCITY_LIMIT, DRIVER_VELOCITY_LIMIT);
        self.driver_position = (self.driver_position + self.driver_velocity * DT)
            .clamp(-DRIVER_POSITION_LIMIT, DRIVER_POSITION_LIMIT);

        let damp = (-DT * DAMP_RATE).exp();
        for row in 0..MEMBRANE_ROWS {
            for col in 0..MEMBRANE_COLS {
                let i = row * MEMBRANE_COLS + col;
                let up = if row > 0 {
                    self.h[i - MEMBRANE_COLS]
                } else {
                    self.h[i]
                };
                let down = if row < MEMBRANE_ROWS - 1 {
                    self.h[i + MEMBRANE_COLS]
                } else {
                    self.h[i]
                };
                let left = if col > 0 { self.h[i - 1] } else { self.h[i] };
                let right = if col < MEMBRANE_COLS - 1 {
                    self.h[i + 1]
                } else {
                    self.h[i]
                };
                let row_laplacian = up + down - 2.0 * self.h[i];
                let col_laplacian = left + right - 2.0 * self.h[i];
                let laplacian = row_laplacian + col_laplacian * COL_SPRING_SCALE;
                let driver_force =
                    driver_weight(row, col) * DRIVER_COUPLING * (self.driver_position - self.h[i]);
                let acceleration = laplacian * SPRING - self.h[i] * RESTORING + driver_force;
                self.v[i] = (self.v[i] + acceleration * DT) * damp;
            }
        }
        for (height, velocity) in self.h.iter_mut().zip(self.v.iter()) {
            *height = (*height + velocity * DT).clamp(HEIGHT_MIN, HEIGHT_MAX);
        }
    }

    /// Beat impact, scaled by `strength` (`0..=1`), applied as radial momentum
    /// plus a velocity kick to the central driver. Both use the
    /// same circular speaker profile, so the vibration stays uniform around
    /// the center; no position jump, random cells, or asymmetric splashes are
    /// introduced.
    pub fn splash(&mut self, strength: f32) {
        let strength = strength.clamp(0.0, 1.0);
        // A fresh beat establishes its own positive speaker stroke instead of
        // inheriting the oscillator phase. Additive velocity made a
        // kick landing in a trough look weak, then let stored spring energy
        // create a larger peak after the music had already moved on.
        let driver_target = strength * BEAT_DRIVER_TARGET;
        let driver_headroom = if driver_target > 0.0 {
            ((driver_target - self.driver_position) / driver_target).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.driver_velocity = strength * BEAT_IMPULSE * driver_headroom;
        for row in 0..MEMBRANE_ROWS {
            for col in 0..MEMBRANE_COLS {
                let index = row * MEMBRANE_COLS + col;
                let weight = radial_weight(row, col, BEAT_RADIUS);
                let beat_target = weight * strength * BEAT_SURFACE_TARGET;
                let headroom = if beat_target > 0.0 {
                    ((beat_target - self.h[index]) / beat_target).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let beat_velocity = weight * strength * BEAT_SURFACE_IMPULSE * headroom;
                self.v[index] = self.v[index].max(beat_velocity);
            }
        }
    }

    pub fn height(&self, row: usize, col: usize) -> f32 {
        self.h[row * MEMBRANE_COLS + col]
    }

    /// Bilinear height at normalized membrane coordinates (`0..=1`). This
    /// keeps the physics lattice modest while letting renderers draw a much
    /// denser wire mesh without adding simulation work.
    pub fn sample(&self, depth: f32, across: f32) -> f32 {
        let row = depth.clamp(0.0, 1.0) * (MEMBRANE_ROWS - 1) as f32;
        let col = across.clamp(0.0, 1.0) * (MEMBRANE_COLS - 1) as f32;
        let row0 = row.floor() as usize;
        let col0 = col.floor() as usize;
        let row1 = (row0 + 1).min(MEMBRANE_ROWS - 1);
        let col1 = (col0 + 1).min(MEMBRANE_COLS - 1);
        let row_mix = row - row0 as f32;
        let col_mix = col - col0 as f32;
        let top =
            self.height(row0, col0) + (self.height(row0, col1) - self.height(row0, col0)) * col_mix;
        let bottom =
            self.height(row1, col0) + (self.height(row1, col1) - self.height(row1, col0)) * col_mix;
        top + (bottom - top) * row_mix
    }

    /// Positive normalized loudspeaker push for the renderer (`0..=1`).
    /// This follows the visible cloth center rather than the hidden scalar
    /// driver so the glow lands in the same frame as the rendered hit and
    /// turns off as soon as the membrane falls into depth.
    pub fn pressure(&self) -> f32 {
        (self.sample(0.5, 0.5).max(0.0) / HEIGHT_MAX).clamp(0.0, 1.0)
    }

    pub fn reset(&mut self) {
        self.h = [0.0; CELLS];
        self.v = [0.0; CELLS];
        self.driver_position = 0.0;
        self.driver_velocity = 0.0;
    }

    /// All `|h|`, `|v|` below `REST_EPSILON` — the surface has settled.
    pub fn is_still(&self) -> bool {
        self.driver_position.abs() < REST_EPSILON
            && self.driver_velocity.abs() < REST_EPSILON
            && self
                .h
                .iter()
                .chain(self.v.iter())
                .all(|value| value.abs() < REST_EPSILON)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn center_height(membrane: &Membrane) -> f32 {
        let upper = MEMBRANE_ROWS / 2 - 1;
        let lower = MEMBRANE_ROWS / 2;
        let left = MEMBRANE_COLS / 2 - 1;
        let right = MEMBRANE_COLS / 2;
        (membrane.height(upper, left)
            + membrane.height(upper, right)
            + membrane.height(lower, left)
            + membrane.height(lower, right))
            / 4.0
    }

    #[test]
    fn beat_impulse_is_symmetric_around_the_membrane_center() {
        let mut water = Membrane::new();
        water.splash(1.0);
        let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
        for _ in 0..24 {
            water.advance(&silent);
        }

        for row in 0..MEMBRANE_ROWS {
            for col in 0..MEMBRANE_COLS {
                let opposite_row = MEMBRANE_ROWS - 1 - row;
                let opposite_col = MEMBRANE_COLS - 1 - col;
                let delta =
                    (water.height(row, col) - water.height(opposite_row, opposite_col)).abs();
                assert!(
                    delta < 1.0e-5,
                    "radial impulse diverged at ({row}, {col}) by {delta}"
                );
            }
        }
    }

    #[test]
    fn normalized_sampling_matches_every_exact_physics_cell() {
        let mut membrane = Membrane::new();
        membrane.splash(1.0);
        let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
        for _ in 0..18 {
            membrane.advance(&silent);
        }

        for row in [0, MEMBRANE_ROWS / 2, MEMBRANE_ROWS - 1] {
            for col in [0, MEMBRANE_COLS / 2, MEMBRANE_COLS - 1] {
                let depth = row as f32 / (MEMBRANE_ROWS - 1) as f32;
                let across = col as f32 / (MEMBRANE_COLS - 1) as f32;
                assert!(
                    (membrane.sample(depth, across) - membrane.height(row, col)).abs() < 1.0e-5
                );
            }
        }
    }

    #[test]
    fn central_driver_overshoots_below_rest_after_a_positive_beat() {
        let mut water = Membrane::new();
        water.splash(1.0);
        let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
        let mut peak = 0.0_f32;
        let mut trough = 0.0_f32;
        for _ in 0..240 {
            water.advance(&silent);
            let height = center_height(&water);
            peak = peak.max(height);
            trough = trough.min(height);
        }

        assert!(peak > 0.35, "driver must first rise, peaked at {peak}");
        assert!(
            trough < -0.05,
            "underdamped driver must fall into depth, trough was {trough}"
        );
    }

    #[test]
    fn strong_beat_rises_fluidly_without_teleporting_the_surface() {
        let mut membrane = Membrane::new();
        let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
        let before = center_height(&membrane);

        membrane.splash(1.0);
        let scheduled = center_height(&membrane);
        assert!(
            (scheduled - before).abs() < 1.0e-5,
            "splash must schedule momentum instead of teleporting the cloth: before={before}, after={scheduled}"
        );

        let mut heights = Vec::new();
        for _ in 0..120 {
            membrane.advance(&silent);
            heights.push(center_height(&membrane));
        }
        let peak = heights
            .iter()
            .copied()
            .max_by(f32::total_cmp)
            .expect("the fixture advances at least once");
        let peak_index = heights
            .iter()
            .position(|height| (*height - peak).abs() < 1.0e-5)
            .expect("the measured peak must occur in the trace");
        let max_step = heights
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold((heights[0] - scheduled).abs(), f32::max);

        assert!(
            peak > 1.5,
            "a full-strength beat must retain a large dome, peaked at {peak}"
        );
        assert!(
            (4..=8).contains(&peak_index),
            "the dome must peak after a fluid 5-9 frame attack, peaked at frame {}",
            peak_index + 1,
        );
        assert!(
            max_step < 0.45,
            "the cloth must not jump between adjacent frames, largest step was {max_step}"
        );
    }

    #[test]
    fn strong_beat_overrides_the_negative_phase_and_remains_the_largest_peak() {
        let mut membrane = Membrane::new();
        let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
        membrane.splash(1.0);
        let mut reached_trough = false;
        for _ in 0..180 {
            membrane.advance(&silent);
            if center_height(&membrane) < -0.2 {
                reached_trough = true;
                break;
            }
        }
        assert!(
            reached_trough,
            "fixture must reach the negative cloth phase"
        );

        let trough_height = center_height(&membrane);
        membrane.splash(1.0);
        assert!(
            (center_height(&membrane) - trough_height).abs() < 1.0e-5,
            "a new beat must not teleport the cloth out of its trough"
        );
        let mut impact_peak = f32::NEG_INFINITY;
        let mut previous_height = trough_height;
        let mut max_step = 0.0_f32;
        for _ in 0..9 {
            membrane.advance(&silent);
            let height = center_height(&membrane);
            impact_peak = impact_peak.max(height);
            max_step = max_step.max((height - previous_height).abs());
            previous_height = height;
        }
        assert!(
            impact_peak > 1.5,
            "a strong beat must build a large dome even from a trough, got {impact_peak}"
        );
        assert!(
            max_step < 0.45,
            "a beat landing in the depth phase must still transition fluidly, largest step was {max_step}"
        );
        let mut tail_peak = 0.0_f32;
        for _ in 0..120 {
            membrane.advance(&silent);
            tail_peak = tail_peak.max(center_height(&membrane));
        }
        assert!(
            tail_peak <= impact_peak * 1.05,
            "the cloth tail must not invent a larger peak later: hit={impact_peak}, tail={tail_peak}"
        );
    }

    #[test]
    fn ac_20_single_hit_does_not_generate_another_breakdown_sized_rebound() {
        let mut membrane = Membrane::new();
        let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
        membrane.splash(1.0);

        let mut heights = Vec::new();
        for _ in 0..240 {
            membrane.advance(&silent);
            heights.push(center_height(&membrane));
        }

        let first_peak_index = heights
            .windows(2)
            .position(|pair| pair[0] > pair[1])
            .expect("a strong hit must form a positive crest");
        let first_peak = heights[first_peak_index];
        let first_depth_index = heights[first_peak_index..]
            .iter()
            .position(|height| *height < 0.0)
            .map(|index| first_peak_index + index)
            .expect("the speaker stroke must continue into depth");
        let rebound = heights[first_depth_index..]
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);

        assert!(
            rebound < first_peak * 0.20,
            "one musical hit must not create a second hit-sized crest: first={first_peak}, rebound={rebound}"
        );
    }

    #[test]
    fn strong_beat_dome_is_broad_like_the_reference_speaker() {
        let mut membrane = Membrane::new();
        membrane.splash(1.0);
        let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
        for _ in 0..3 {
            membrane.advance(&silent);
        }
        let center = center_height(&membrane);
        let side = membrane.height(MEMBRANE_ROWS / 2 - 1, MEMBRANE_COLS / 2 - 9);
        let depth = membrane.height(MEMBRANE_ROWS / 2 - 6, MEMBRANE_COLS / 2 - 1);
        assert!(
            side > center * 0.5 && depth > center * 0.5,
            "beat dome must stay broad in both axes: center={center}, side={side}, depth={depth}"
        );
    }

    #[test]
    fn rapid_beats_do_not_flatten_the_membrane_against_its_height_limit() {
        let mut membrane = Membrane::new();
        let mut bass_sustain = [0.0_f32; SPECTRUM_BAND_COUNT];
        bass_sustain[..BASS_BANDS].fill(0.75);
        let mut peak = 0.0_f32;
        let mut saturated_frames = 0;

        for frame in 0..180 {
            if frame % 3 == 0 {
                membrane.splash(1.0);
            }
            membrane.advance(&bass_sustain);
            let height = center_height(&membrane);
            peak = peak.max(height);
            saturated_frames += usize::from(height >= HEIGHT_MAX - 1.0e-3);
        }

        assert!(
            peak > 1.5,
            "rapid strong beats must still produce a large dome, peaked at {peak}"
        );
        assert_eq!(
            saturated_frames, 0,
            "rapid beats must preserve a rounded cloth crest instead of pinning it flat"
        );
    }

    #[test]
    fn pressure_exposes_only_the_positive_bass_push_for_rendering() {
        let mut water = Membrane::new();
        assert_eq!(water.pressure(), 0.0);
        water.splash(1.0);
        let silent = [0.0_f32; SPECTRUM_BAND_COUNT];

        let mut positive_pressure = 0.0_f32;
        let mut saw_negative_center = false;
        for _ in 0..240 {
            water.advance(&silent);
            positive_pressure = positive_pressure.max(water.pressure());
            if center_height(&water) < -0.05 {
                saw_negative_center = true;
                assert_eq!(
                    water.pressure(),
                    0.0,
                    "depth phase must not keep the bass-push glow lit"
                );
                break;
            }
        }
        assert!(positive_pressure > 0.25);
        assert!(saw_negative_center);
    }

    #[test]
    fn bass_energy_drives_a_round_central_dome() {
        let mut water = Membrane::new();
        let mut bass = [0.0_f32; SPECTRUM_BAND_COUNT];
        bass[..8].fill(1.0);
        for _ in 0..300 {
            water.advance(&bass);
        }

        let center = center_height(&water);
        let row_offset = water.height(MEMBRANE_ROWS / 2 - 5, MEMBRANE_COLS / 2 - 1);
        let col_offset = water.height(MEMBRANE_ROWS / 2 - 1, MEMBRANE_COLS / 2 - 8);
        assert!(center > 0.25, "bass must lift the center, got {center}");
        assert!(
            (row_offset - col_offset).abs() < 0.08,
            "equal radial offsets must move together: row {row_offset}, col {col_offset}"
        );
        assert!(
            center > row_offset + 0.02,
            "the broad speaker dome must remain centered: center {center}, ring {row_offset}"
        );
    }

    #[test]
    fn membrane_settles_flat_without_input() {
        let mut water = Membrane::new();
        water.splash(1.0);
        let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
        for _ in 0..2000 {
            water.advance(&silent);
        }
        assert!(water.is_still(), "waves must damp out");
    }

    #[test]
    fn sub_gate_bands_leave_the_surface_flat() {
        let mut water = Membrane::new();
        // Every band faint but non-zero, below the noise gate: the "Grundrauschen"
        // case — must not stir the surface into a tremor.
        let faint = [DRIVE_GATE * 0.5; SPECTRUM_BAND_COUNT];
        for _ in 0..400 {
            water.advance(&faint);
        }
        assert!(
            water.is_still(),
            "faint sub-gate input must leave the surface at rest"
        );
    }

    #[test]
    fn splash_raises_the_surface_and_stays_bounded() {
        let mut water = Membrane::new();
        for _ in 0..8 {
            water.splash(1.0);
        }
        let bands = [1.0_f32; SPECTRUM_BAND_COUNT];
        let mut peak = 0.0_f32;
        for _ in 0..600 {
            water.advance(&bands);
            for row in 0..MEMBRANE_ROWS {
                for col in 0..MEMBRANE_COLS {
                    let height = water.height(row, col);
                    assert!(height.is_finite() && (-1.1..=3.0).contains(&height));
                    peak = peak.max(height);
                }
            }
        }
        assert!(
            peak > 0.5,
            "driven surface must actually move, peaked at {peak}"
        );
    }
}
