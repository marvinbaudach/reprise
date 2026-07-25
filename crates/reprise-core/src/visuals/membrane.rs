//! The Grid visual's thin membrane — a spring-mesh height field driven from
//! its center like a bass loudspeaker. Low-band energy moves one underdamped
//! central driver and beat impulses kick its velocity; a radially symmetric
//! coupling transfers that motion into the cloth and the mesh propagates it
//! outward. Signed heights let the surface rise toward the viewer and then
//! fall into depth before damping back to rest.

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
const DRIVER_TARGET_AMPLITUDE: f32 = 0.95;
/// Driver oscillator coefficients. The damping is deliberately below critical
/// (`2*sqrt(DRIVER_SPRING)`), so a beat rises and then falls below the resting
/// plane instead of easing monotonically back to zero.
const DRIVER_SPRING: f32 = 145.0;
const DRIVER_DAMPING: f32 = 7.5;
/// Velocity kick applied by a full-strength detected beat.
const BEAT_IMPULSE: f32 = 16.0;
const DRIVER_POSITION_LIMIT: f32 = 1.8;
const DRIVER_VELOCITY_LIMIT: f32 = 28.0;
/// Gaussian radius of the central loudspeaker, in normalized membrane space
/// (`x` and `y` each span `-1..=1`).
const DRIVER_RADIUS: f32 = 0.22;
/// How tightly the cloth follows the scalar driver inside that Gaussian.
const DRIVER_COUPLING: f32 = 185.0;
/// Neighbor spring coupling: how fast a disturbance propagates radially across
/// the mesh away from the driver.
const SPRING: f32 = 72.0;
/// Horizontal cells are denser than depth cells on the rectangular physics
/// lattice. Weight their second derivative by the squared spacing ratio so a
/// wave travels the same normalized distance in every direction.
const CELL_SPACING_RATIO: f32 = (MEMBRANE_COLS - 1) as f32 / (MEMBRANE_ROWS - 1) as f32;
const COL_SPRING_SCALE: f32 = CELL_SPACING_RATIO * CELL_SPACING_RATIO;
/// Restoring pull back toward a flat surface.
const RESTORING: f32 = 2.2;
/// Velocity damping rate; light enough for cloth-like rings to travel, strong
/// enough for AC-11 to reach a genuinely static stopped frame.
const DAMP_RATE: f32 = 1.65;
const HEIGHT_MIN: f32 = -1.35;
const HEIGHT_MAX: f32 = 2.0;

fn driver_weight(row: usize, col: usize) -> f32 {
    let y = (2 * row as isize - (MEMBRANE_ROWS - 1) as isize) as f32 / (MEMBRANE_ROWS - 1) as f32;
    let x = (2 * col as isize - (MEMBRANE_COLS - 1) as isize) as f32 / (MEMBRANE_COLS - 1) as f32;
    (-(x * x + y * y) / (2.0 * DRIVER_RADIUS * DRIVER_RADIUS)).exp()
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

    /// Beat impulse, scaled by `strength` (`0..=1`), applied only to the
    /// central driver. The circular coupling makes the resulting vibration
    /// uniform around the center; no random cells or asymmetric splashes are
    /// introduced.
    pub fn splash(&mut self, strength: f32) {
        let strength = strength.clamp(0.0, 1.0);
        self.driver_velocity =
            (self.driver_velocity + strength * BEAT_IMPULSE).min(DRIVER_VELOCITY_LIMIT);
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
    /// Negative cone travel is deliberately zero so the bass glow illuminates
    /// only the outward pressure phase, not the membrane's fall into depth.
    pub fn pressure(&self) -> f32 {
        (self.driver_position.max(0.0) / DRIVER_POSITION_LIMIT).clamp(0.0, 1.0)
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
            center > row_offset + 0.05,
            "the bass driver must be centered: center {center}, ring {row_offset}"
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
