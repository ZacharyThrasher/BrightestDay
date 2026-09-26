//! Corps colour → 4-slot construct palette (bd_core `Tint`: Primary, Accent, Dark, Core).

use bd_core::corps::Corps;

/// Linear RGB palette, index = bd_shapes palette slot (0 Primary, 1 Accent, 2 Dark, 3 Core).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette(pub [[f32; 3]; 4]);

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn sanitize(c: [f32; 3]) -> [f32; 3] {
    let f = |v: f32| if v.is_finite() { v.clamp(0.0, 4.0) } else { 0.5 };
    [f(c[0]), f(c[1]), f(c[2])]
}

impl Palette {
    /// Derive the palette from a corps colour (+ optional recipe accent).
    pub fn new(corps: [f32; 3], accent: Option<[f32; 3]>) -> Self {
        let c = sanitize(corps);
        let accent = accent.map(sanitize).unwrap_or_else(|| mix(c, [1.0, 1.0, 1.0], 0.42));
        let dark = [c[0] * 0.30, c[1] * 0.30, c[2] * 0.30];
        let core = mix(c, [1.0, 1.0, 1.0], 0.6);
        Palette([c, accent, dark, core])
    }

    pub fn of_corps(corps: Corps) -> Self {
        Self::new(corps.color(), None)
    }

    #[inline]
    pub fn slot(&self, i: usize) -> [f32; 3] {
        self.0[i & 3]
    }
}

/// Element tints (linear RGB) used by trails.
pub fn element_color(e: bd_core::mechanics::Element) -> [f32; 3] {
    use bd_core::mechanics::Element::*;
    match e {
        Light => [1.0, 1.0, 0.95],
        Fire => [1.0, 0.38, 0.06],
        Electric => [0.55, 0.8, 1.0],
        Ice => [0.6, 0.9, 1.0],
        Water => [0.12, 0.45, 1.0],
        Wind => [0.8, 1.0, 0.95],
        Earth => [0.45, 0.3, 0.16],
        Poison => [0.55, 1.0, 0.2],
        Psychic => [1.0, 0.35, 0.9],
        Shadow => [0.25, 0.05, 0.4],
        Atomic => [0.3, 1.0, 0.7],
    }
}
