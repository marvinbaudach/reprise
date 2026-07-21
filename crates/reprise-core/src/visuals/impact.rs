//! Transient impact overlay for the visual engine.
//!
//! Beats and drops spawn short-lived ornaments — expanding shockwaves, radiating
//! spark bursts, a soft full-canvas dynamics glow, a brief accent brightness
//! lift, and a kick envelope for punchy scale/shake — that sit on top of the
//! per-mode geometry and decay independently in the tick loop. This is what
//! turns a measured gauge into something playful and worth watching.
//! Everything is bounded by fixed-capacity pools: a beat storm never
//! allocates or grows without limit.
//!
//! Geometry is stored resolution-independently (shockwaves as an age fraction,
//! particles in polar coordinates from the center) so impacts can be spawned
//! without knowing the canvas size, and scaled at draw time.

use super::dust::xorshift;

/// Fixed capacity for concurrent shockwaves. Overlapping beats stack up to here,
/// then the oldest is recycled.
const MAX_SHOCKWAVES: usize = 6;
/// Fixed capacity for live spark particles.
const MAX_PARTICLES: usize = 56;

/// Shockwave lifetime, in tick frames (~350 ms near 60 fps).
const SHOCKWAVE_LIFE: f32 = 21.0;
/// Particle base lifetime, in tick frames.
const PARTICLE_LIFE: f32 = 34.0;
/// Per-frame velocity retention for sparks (gentle ease-out as they fly).
const PARTICLE_DRAG: f32 = 0.94;
/// Per-frame decay of the dynamics glow and accent lift.
const FLASH_DECAY: f32 = 0.90;
const BOOST_DECAY: f32 = 0.86;
/// Per-frame decay of the kick envelope.
const KICK_DECAY: f32 = 0.90;
/// Below this an envelope is treated as fully rested.
const REST_EPSILON: f32 = 0.01;
/// `dynamics` above this reads as a drop/slam and flashes.
const DROP_THRESHOLD: f32 = 0.35;

#[derive(Clone, Copy)]
struct Shockwave {
    /// Frames elapsed, `0..SHOCKWAVE_LIFE`.
    age: f32,
    /// Impact strength, `0..=1`, scales radius reach and line weight.
    strength: f32,
}

#[derive(Clone, Copy, Default)]
struct Particle {
    angle: f32,
    dist: f32,
    speed: f32,
    life: f32,
    max_life: f32,
}

impl Particle {
    fn alive(&self) -> bool {
        self.life > 0.0
    }
}

/// One live shockwave, resolution-independent: `progress` is `0..=1` over its
/// lifetime, `strength` scales its reach and weight.
#[derive(Clone, Copy)]
pub struct ShockwaveDraw {
    pub progress: f32,
    pub strength: f32,
}

/// One live spark, polar from the canvas center.
#[derive(Clone, Copy)]
pub struct ParticleDraw {
    pub angle: f32,
    pub dist: f32,
    /// Remaining life fraction, `0..=1` (drives fade + size).
    pub life_frac: f32,
}

pub struct ImpactState {
    shockwaves: [Option<Shockwave>; MAX_SHOCKWAVES],
    particles: [Particle; MAX_PARTICLES],
    flash: f32,
    accent_boost: f32,
    /// Punchy scale/shake envelope: jumps on every beat, decays each tick.
    kick: f32,
    next_particle: usize,
    rng: u32,
}

impl Default for ImpactState {
    fn default() -> Self {
        Self::new()
    }
}

impl ImpactState {
    pub fn new() -> Self {
        Self {
            shockwaves: [None; MAX_SHOCKWAVES],
            particles: [Particle::default(); MAX_PARTICLES],
            flash: 0.0,
            accent_boost: 0.0,
            kick: 0.0,
            next_particle: 0,
            rng: 0x2545_f491,
        }
    }

    fn push_shockwave(&mut self, strength: f32) {
        let wave = Shockwave { age: 0.0, strength };
        if let Some(slot) = self.shockwaves.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(wave);
            return;
        }
        // All full: recycle the oldest (largest age).
        if let Some(slot) = self
            .shockwaves
            .iter_mut()
            .flatten()
            .max_by(|a, b| a.age.total_cmp(&b.age))
        {
            *slot = wave;
        }
    }

    /// A beat landed: emit a shockwave, a spark burst sized by `strength`,
    /// and kick the punch envelope.
    pub fn spawn_beat(&mut self, strength: f32) {
        let strength = strength.clamp(0.0, 1.0);
        self.push_shockwave(strength);
        let count = 3 + (strength * 9.0).round() as usize;
        for _ in 0..count {
            let angle = xorshift(&mut self.rng) * std::f32::consts::TAU;
            let speed = 2.4 + xorshift(&mut self.rng) * 3.0 + strength * 4.5;
            let life = PARTICLE_LIFE * (0.6 + xorshift(&mut self.rng) * 0.6);
            self.particles[self.next_particle] = Particle {
                angle,
                dist: 0.0,
                speed,
                life,
                max_life: life,
            };
            self.next_particle = (self.next_particle + 1) % MAX_PARTICLES;
        }
        self.accent_boost = self.accent_boost.max(strength);
        self.kick = self.kick.max(0.6 + 0.4 * strength);
    }

    /// A drop/slam: soft full-canvas glow plus a big shockwave. No-op below the
    /// dynamics threshold, so ordinary loud passages don't flash.
    pub fn spawn_drop(&mut self, dynamics: f32) {
        if dynamics <= DROP_THRESHOLD {
            return;
        }
        let intensity = (dynamics - DROP_THRESHOLD) / (1.0 - DROP_THRESHOLD);
        self.flash = self.flash.max(intensity);
        self.push_shockwave(1.0);
    }

    /// Advance every impact by one tick frame.
    pub fn advance(&mut self) {
        for slot in &mut self.shockwaves {
            if let Some(wave) = slot {
                wave.age += 1.0;
                if wave.age >= SHOCKWAVE_LIFE {
                    *slot = None;
                }
            }
        }
        for particle in &mut self.particles {
            if particle.alive() {
                particle.dist += particle.speed;
                particle.speed *= PARTICLE_DRAG;
                particle.life -= 1.0;
            }
        }
        self.flash *= FLASH_DECAY;
        self.accent_boost *= BOOST_DECAY;
        self.kick *= KICK_DECAY;
        if self.flash < REST_EPSILON {
            self.flash = 0.0;
        }
        if self.accent_boost < REST_EPSILON {
            self.accent_boost = 0.0;
        }
        if self.kick < REST_EPSILON {
            self.kick = 0.0;
        }
    }

    /// No live ornaments and every envelope rested — the tick loop may stop.
    pub fn is_idle(&self) -> bool {
        self.flash == 0.0
            && self.accent_boost == 0.0
            && self.kick == 0.0
            && self.shockwaves.iter().all(Option::is_none)
            && self.particles.iter().all(|particle| !particle.alive())
    }

    pub fn flash(&self) -> f32 {
        self.flash
    }

    /// Punchy scale/shake envelope, `0..=1`, driven by beats.
    pub fn kick(&self) -> f32 {
        self.kick
    }

    pub fn shockwaves(&self) -> impl Iterator<Item = ShockwaveDraw> + '_ {
        self.shockwaves.iter().flatten().map(|wave| ShockwaveDraw {
            progress: (wave.age / SHOCKWAVE_LIFE).clamp(0.0, 1.0),
            strength: wave.strength,
        })
    }

    pub fn particles(&self) -> impl Iterator<Item = ParticleDraw> + '_ {
        self.particles
            .iter()
            .filter(|p| p.alive())
            .map(|p| ParticleDraw {
                angle: p.angle,
                dist: p.dist,
                life_frac: (p.life / p.max_life).clamp(0.0, 1.0),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impact_beat_storm_stays_within_fixed_capacity() {
        let mut impact = ImpactState::new();
        for _ in 0..200 {
            impact.spawn_beat(1.0);
        }
        // Pools are fixed-capacity: a beat storm never grows without bound.
        assert!(impact.shockwaves().count() <= 6);
        assert!(impact.particles().count() <= 56);
        for spark in impact.particles() {
            assert!(spark.dist.is_finite() && spark.life_frac.is_finite());
            assert!((0.0..=1.0).contains(&spark.life_frac));
        }
        for wave in impact.shockwaves() {
            assert!((0.0..=1.0).contains(&wave.progress));
        }
    }

    #[test]
    fn impact_decays_to_rest_after_a_burst() {
        let mut impact = ImpactState::new();
        assert!(impact.is_idle());
        impact.spawn_beat(1.0);
        impact.spawn_drop(0.9);
        assert!(!impact.is_idle());
        for _ in 0..200 {
            impact.advance();
        }
        assert!(impact.is_idle(), "all ornaments must decay to rest");
    }

    #[test]
    fn impact_drop_below_threshold_is_a_noop() {
        let mut impact = ImpactState::new();
        impact.spawn_drop(0.1);
        assert!(impact.is_idle(), "ordinary loudness must not flash");
        assert_eq!(impact.flash(), 0.0);

        impact.spawn_drop(0.95);
        assert!(!impact.is_idle());
        assert!(impact.flash() > 0.0);
    }

    #[test]
    fn kick_envelope_rises_on_beat_and_decays() {
        let mut impact = ImpactState::new();
        impact.spawn_beat(1.0);
        let peak = impact.kick();
        assert!(peak >= 0.9);
        for _ in 0..60 {
            impact.advance();
        }
        assert!(impact.kick() < 0.05);
    }
}
