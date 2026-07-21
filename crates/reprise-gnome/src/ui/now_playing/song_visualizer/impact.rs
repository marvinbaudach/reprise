//! Transient impact overlay for the song visualizer.
//!
//! Beats and drops spawn short-lived ornaments — expanding shockwaves, radiating
//! spark bursts, a soft full-canvas dynamics glow, and a brief accent brightness
//! lift — that sit on top of the per-mode geometry and decay independently in
//! the tick loop. This is what turns a measured gauge into something playful and
//! worth watching. Everything is bounded by fixed-capacity pools: a beat storm
//! never allocates or grows without limit.
//!
//! Geometry is stored resolution-independently (shockwaves as an age fraction,
//! particles in polar coordinates from the center) so impacts can be spawned
//! without knowing the canvas size, and scaled at draw time.

/// Fixed capacity for concurrent shockwaves. Overlapping beats stack up to here,
/// then the oldest is recycled.
const MAX_SHOCKWAVES: usize = 6;
/// Fixed capacity for live spark particles.
const MAX_PARTICLES: usize = 56;

/// Shockwave lifetime, in tick frames (~350 ms near 60 fps).
const SHOCKWAVE_LIFE: f64 = 21.0;
/// Particle base lifetime, in tick frames.
const PARTICLE_LIFE: f64 = 34.0;
/// Per-frame velocity retention for sparks (gentle ease-out as they fly).
const PARTICLE_DRAG: f64 = 0.94;
/// Per-frame decay of the dynamics glow and accent lift.
const FLASH_DECAY: f64 = 0.90;
const BOOST_DECAY: f64 = 0.86;
/// Below this an envelope is treated as fully rested.
const REST_EPSILON: f64 = 0.01;
/// `dynamics` above this reads as a drop/slam and flashes.
const DROP_THRESHOLD: f32 = 0.35;

#[derive(Clone, Copy)]
struct Shockwave {
    /// Frames elapsed, `0..SHOCKWAVE_LIFE`.
    age: f64,
    /// Impact strength, `0..=1`, scales radius reach and line weight.
    strength: f64,
}

#[derive(Clone, Copy, Default)]
struct Particle {
    angle: f64,
    dist: f64,
    speed: f64,
    life: f64,
    max_life: f64,
}

impl Particle {
    fn alive(&self) -> bool {
        self.life > 0.0
    }
}

/// One live shockwave, resolution-independent: `progress` is `0..=1` over its
/// lifetime, `strength` scales its reach and weight.
#[derive(Clone, Copy)]
pub(super) struct ShockwaveDraw {
    pub progress: f64,
    pub strength: f64,
}

/// One live spark, polar from the canvas center.
#[derive(Clone, Copy)]
pub(super) struct ParticleDraw {
    pub angle: f64,
    pub dist: f64,
    /// Remaining life fraction, `0..=1` (drives fade + size).
    pub life_frac: f64,
}

pub(super) struct ImpactState {
    shockwaves: [Option<Shockwave>; MAX_SHOCKWAVES],
    particles: [Particle; MAX_PARTICLES],
    flash: f64,
    accent_boost: f64,
    next_particle: usize,
    rng: u32,
}

impl ImpactState {
    pub(super) fn new() -> Self {
        Self {
            shockwaves: [None; MAX_SHOCKWAVES],
            particles: [Particle::default(); MAX_PARTICLES],
            flash: 0.0,
            accent_boost: 0.0,
            next_particle: 0,
            rng: 0x2545_f491,
        }
    }

    /// Xorshift-based unit float in `0.0..1.0`. Avoids a dependency and stays
    /// deterministic across runs — variety comes from the sequence, not a seed.
    fn rand_unit(&mut self) -> f64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        f64::from(x) / f64::from(u32::MAX)
    }

    fn push_shockwave(&mut self, strength: f64) {
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

    /// A beat landed: emit a shockwave and a spark burst sized by `strength`.
    pub(super) fn spawn_beat(&mut self, strength: f32) {
        let strength = f64::from(strength.clamp(0.0, 1.0));
        self.push_shockwave(strength);
        let count = 3 + (strength * 9.0).round() as usize;
        for _ in 0..count {
            let angle = self.rand_unit() * std::f64::consts::TAU;
            let speed = 2.4 + self.rand_unit() * 3.0 + strength * 4.5;
            let life = PARTICLE_LIFE * (0.6 + self.rand_unit() * 0.6);
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
    }

    /// A drop/slam: soft full-canvas glow plus a big shockwave. No-op below the
    /// dynamics threshold, so ordinary loud passages don't flash.
    pub(super) fn spawn_drop(&mut self, dynamics: f32) {
        if dynamics <= DROP_THRESHOLD {
            return;
        }
        let intensity = f64::from((dynamics - DROP_THRESHOLD) / (1.0 - DROP_THRESHOLD));
        self.flash = self.flash.max(intensity);
        self.push_shockwave(1.0);
    }

    /// Advance every impact by one tick frame.
    pub(super) fn advance(&mut self) {
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
        if self.flash < REST_EPSILON {
            self.flash = 0.0;
        }
        if self.accent_boost < REST_EPSILON {
            self.accent_boost = 0.0;
        }
    }

    /// No live ornaments and both envelopes rested — the tick loop may stop.
    pub(super) fn is_idle(&self) -> bool {
        self.flash == 0.0
            && self.accent_boost == 0.0
            && self.shockwaves.iter().all(Option::is_none)
            && self.particles.iter().all(|particle| !particle.alive())
    }

    pub(super) fn flash(&self) -> f64 {
        self.flash
    }

    pub(super) fn accent_boost(&self) -> f64 {
        self.accent_boost
    }

    pub(super) fn shockwaves(&self) -> impl Iterator<Item = ShockwaveDraw> + '_ {
        self.shockwaves.iter().flatten().map(|wave| ShockwaveDraw {
            progress: (wave.age / SHOCKWAVE_LIFE).clamp(0.0, 1.0),
            strength: wave.strength,
        })
    }

    pub(super) fn particles(&self) -> impl Iterator<Item = ParticleDraw> + '_ {
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
