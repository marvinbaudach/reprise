//! Tiny deterministic RNG for the transient impact overlay.

/// Xorshift32, returning a unit float in `0.0..1.0`. Avoids a `rand`
/// dependency and stays deterministic across runs — variety comes from the
/// sequence, not a random seed.
pub(crate) fn xorshift(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x as f32 / u32::MAX as f32
}
