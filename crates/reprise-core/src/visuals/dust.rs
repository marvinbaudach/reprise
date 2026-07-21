//! Ambient dust field for the Particles visual mode — a fixed swarm of
//! softly drifting, twinkling motes in normalized `0.0..=1.0` space. Motes
//! wrap at the edges of the unit box so the field reads as endless, and
//! drift speed responds to the current audio level rather than a wall clock.
//!
//! Also home to [`xorshift`], the tiny deterministic RNG shared with
//! [`super::impact`] — one implementation instead of two copies drifting
//! apart.

/// Number of live dust motes.
pub const DUST_COUNT: usize = 120;

/// Fixed physics step: the visual tick loop always advances by this much,
/// never by a wall-clock delta.
const DT: f32 = 1.0 / 60.0;

/// Base drift speed range, in unit-box widths per second, before the
/// level-driven boost.
const DRIFT_MIN: f32 = 0.01;
const DRIFT_MAX: f32 = 0.035;
/// Twinkle phase advance range, in radians per second.
const TWINKLE_SPEED_MIN: f32 = 0.6;
const TWINKLE_SPEED_MAX: f32 = 1.8;

/// One drifting, twinkling dust mote.
#[derive(Clone, Copy)]
pub struct Dust {
    /// Normalized position, `0.0..=1.0`, wrapping at the edges.
    pub nx: f32,
    pub ny: f32,
    /// Base radius, resolution-independent (scaled at draw time).
    pub r: f32,
    /// Base alpha, `0.0..=1.0`.
    pub a: f32,
    /// Twinkle angular speed, radians per second.
    pub tw: f32,
    /// Current twinkle phase, radians.
    pub ph: f32,
    /// Drift velocity, unit-box widths per second.
    dx: f32,
    dy: f32,
}

/// Build the dust field with a deterministic seed — the same layout every
/// run, so screenshots and tests stay stable.
pub fn make_dust() -> [Dust; DUST_COUNT] {
    let mut rng: u32 = 0x9e37_79b9;
    std::array::from_fn(|_| {
        let angle = xorshift(&mut rng) * std::f32::consts::TAU;
        let speed = DRIFT_MIN + xorshift(&mut rng) * (DRIFT_MAX - DRIFT_MIN);
        Dust {
            nx: xorshift(&mut rng),
            ny: xorshift(&mut rng),
            r: 0.6 + xorshift(&mut rng) * 1.8,
            a: 0.25 + xorshift(&mut rng) * 0.5,
            tw: TWINKLE_SPEED_MIN + xorshift(&mut rng) * (TWINKLE_SPEED_MAX - TWINKLE_SPEED_MIN),
            ph: xorshift(&mut rng) * std::f32::consts::TAU,
            dx: angle.cos() * speed,
            dy: angle.sin() * speed,
        }
    })
}

/// One 60 Hz step: drift every mote, wrapping at the unit-box edges, and
/// advance its twinkle phase. `level` (`0.0..=1.0`, unclamped above) speeds
/// up the drift so the field feels alive with the music.
pub fn advance_dust(dust: &mut [Dust; DUST_COUNT], level: f32) {
    let boost = 1.0 + level.max(0.0) * 1.5;
    for mote in dust.iter_mut() {
        mote.nx = (mote.nx + mote.dx * boost * DT).rem_euclid(1.0);
        mote.ny = (mote.ny + mote.dy * boost * DT).rem_euclid(1.0);
        mote.ph += mote.tw * DT;
    }
}

/// Xorshift32, returning a unit float in `0.0..1.0`. Avoids a `rand`
/// dependency and stays deterministic across runs — variety comes from the
/// sequence, not a random seed. Shared by [`super::water`] and
/// [`super::impact`].
pub(crate) fn xorshift(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x as f32 / u32::MAX as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dust_stays_in_unit_box_forever() {
        let mut dust = make_dust();
        for _ in 0..5000 {
            advance_dust(&mut dust, 1.0);
        }
        assert!(dust
            .iter()
            .all(|p| (-0.05..=1.05).contains(&p.nx) && (-0.05..=1.05).contains(&p.ny)));
    }
}
