//! The Grid mode's water surface — a spring-mesh height field driven by the
//! spectrum and jolted by detected beats ("Wasserfläche + Trampolin"). The
//! far row bounces toward the mirrored display bands, neighboring cells pull
//! each other back into shape, and everything damps toward a flat, silent
//! surface once the input stops. Geometry is resolution-independent: heights
//! are unitless and sampled per-cell at draw time via [`WaterGrid::height`].

use crate::playback::SPECTRUM_BAND_COUNT;

use super::dust::xorshift;

pub const WATER_ROWS: usize = 26;
pub const WATER_COLS: usize = 44;
const CELLS: usize = WATER_ROWS * WATER_COLS;

/// Fixed physics step: the visual tick loop always advances by this much,
/// never by a wall-clock delta.
const DT: f32 = 1.0 / 60.0;
/// Below this every cell reads as fully rested (`is_still`).
const REST_EPSILON: f32 = 0.01;

/// Fraction of the remaining distance the far row's *height* closes toward its
/// band target each step. The row is a position-driven boundary, not a
/// force-driven mass: easing the height directly (first order) tracks the music
/// fast with no velocity overshoot, so it snaps to loud hits without the
/// self-oscillating tremor a high force gain produces. ~0.5 ≈ reaches the
/// target in a few frames.
const DRIVE_EASE: f32 = 0.5;
/// Height the far row aims for at full band energy (drive target = band·this).
const DRIVE_AMPLITUDE: f32 = 2.2;
/// Bands below this inject no drive at all — quiet, non-dominant frequencies
/// leave the surface flat instead of trembling it. A noise gate, essentially.
const DRIVE_GATE: f32 = 0.10;
/// Neighbor spring coupling: how fast a disturbance propagates across the mesh
/// toward the viewer. Kept lively so ripples travel rather than crawl.
const SPRING: f32 = 60.0;
/// Restoring pull back toward a flat surface.
const RESTORING: f32 = 1.7;
/// Velocity damping rate; `exp(-DT·this)` per step. Light, so waves linger.
const DAMP_RATE: f32 = 1.0;

/// Height (`h`) and velocity (`v`) field for every grid cell.
pub struct WaterGrid {
    h: [f32; CELLS],
    v: [f32; CELLS],
    rng: u32,
}

impl Default for WaterGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl WaterGrid {
    pub fn new() -> Self {
        Self {
            h: [0.0; CELLS],
            v: [0.0; CELLS],
            rng: 0xa511_e9b3,
        }
    }

    /// One 60 Hz step. Row 0 is a position-driven boundary: its height eases
    /// straight to the (gated) band target with [`DRIVE_EASE`], so it tracks
    /// the music fast without the velocity overshoot that reads as a tremor,
    /// and [`DRIVE_GATE`] keeps quiet bands from stirring it at all. The rows in
    /// front of it are free waves — [`SPRING`] coupling carries the disturbance
    /// toward the viewer, [`RESTORING`] pulls back to flat, and
    /// `exp(-DT·`[`DAMP_RATE`]`)` damps so ripples travel and linger. `h`
    /// clamped to `-1.1..=3.0`.
    pub fn advance(&mut self, bands: &[f32; SPECTRUM_BAND_COUNT]) {
        let half = (WATER_COLS - 1) as f32 / 2.0;
        for col in 0..WATER_COLS {
            let f = (col as f32 - half).abs() / half;
            let band = bands[((f * 0.775 * (SPECTRUM_BAND_COUNT - 1) as f32) as usize)
                .min(SPECTRUM_BAND_COUNT - 1)];
            let target = if band < DRIVE_GATE {
                0.0
            } else {
                band * DRIVE_AMPLITUDE
            };
            self.h[col] += (target - self.h[col]) * DRIVE_EASE;
            self.v[col] = 0.0;
        }
        let damp = (-DT * DAMP_RATE).exp();
        for row in 1..WATER_ROWS {
            for col in 0..WATER_COLS {
                let i = row * WATER_COLS + col;
                let up = self.h[i - WATER_COLS];
                let down = if row < WATER_ROWS - 1 {
                    self.h[i + WATER_COLS]
                } else {
                    self.h[i]
                };
                let left = if col > 0 { self.h[i - 1] } else { self.h[i] };
                let right = if col < WATER_COLS - 1 {
                    self.h[i + 1]
                } else {
                    self.h[i]
                };
                self.v[i] += ((up + down + left + right - 4.0 * self.h[i]) * SPRING
                    - self.h[i] * RESTORING)
                    * DT;
                self.v[i] *= damp;
            }
        }
        for i in WATER_COLS..CELLS {
            self.h[i] = (self.h[i] + self.v[i] * DT).clamp(-1.1, 3.0);
        }
    }

    /// Beat: 2–3 random Gaussian splashes, power `(3.5..6.5)·(0.45+level)`.
    pub fn splash(&mut self, level: f32) {
        let count = 2 + (xorshift(&mut self.rng) * 2.0) as usize;
        for _ in 0..count {
            let col = (WATER_COLS as f32 * (0.2 + xorshift(&mut self.rng) * 0.6)) as usize;
            let row = (WATER_ROWS as f32 * (0.25 + xorshift(&mut self.rng) * 0.55)) as usize;
            let power = (3.5 + xorshift(&mut self.rng) * 3.0) * (0.45 + level);
            for r in row.saturating_sub(3)..(row + 4).min(WATER_ROWS) {
                for c in col.saturating_sub(3)..(col + 4).min(WATER_COLS) {
                    let dr = r as f32 - row as f32;
                    let dc = c as f32 - col as f32;
                    self.v[r * WATER_COLS + c] += (-(dr * dr + dc * dc) / 4.0).exp() * power;
                }
            }
        }
    }

    pub fn height(&self, row: usize, col: usize) -> f32 {
        self.h[row * WATER_COLS + col]
    }

    pub fn reset(&mut self) {
        self.h = [0.0; CELLS];
        self.v = [0.0; CELLS];
    }

    /// All `|h|`, `|v|` below [`REST_EPSILON`] — the surface has settled.
    pub fn is_still(&self) -> bool {
        self.h
            .iter()
            .chain(self.v.iter())
            .all(|value| value.abs() < REST_EPSILON)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_settles_flat_without_input() {
        let mut water = WaterGrid::new();
        water.splash(1.0);
        let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
        for _ in 0..2000 {
            water.advance(&silent);
        }
        assert!(water.is_still(), "waves must damp out");
    }

    #[test]
    fn sub_gate_bands_leave_the_surface_flat() {
        let mut water = WaterGrid::new();
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
        let mut water = WaterGrid::new();
        for _ in 0..8 {
            water.splash(1.0);
        }
        let bands = [1.0_f32; SPECTRUM_BAND_COUNT];
        let mut peak = 0.0_f32;
        for _ in 0..600 {
            water.advance(&bands);
            for row in 0..WATER_ROWS {
                for col in 0..WATER_COLS {
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
