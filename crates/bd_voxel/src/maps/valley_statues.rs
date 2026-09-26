//! The two colossal statues of the Valley of the End (feature `valley`).
//!
//! Each statue is ~55 m of sage stone on its pedestal: sandals, a flared hakama under a long cloak,
//! a skirt of armour plates, a plated cuirass, big layered shoulder guards, a high collar, the head
//! with its hair (a huge spiky mane, or long straight hair with a centre parting), the right hand
//! raised in a two-finger seal inside a hanging kimono sleeve, the left hand resting on the hilt of
//! a sword whose scabbard sticks out behind.
//!
//! Built voxel by voxel from primitives in the statue's local frame — `a` forward (the way it
//! faces), `y` up from the pedestal top, `c` to its right. Everything is breakable (arms, head,
//! sleeves and sword fall away as structural collapse pieces) except a slender indestructible core
//! (legs, spine, a shoulder bar), so a statue never fully vanishes.
//!
//! Gameplay surfaces: the shoulder-guard tops (flat ~6 × 4 m perches), the raised forearm and
//! fingers, the head (small and fully exposed), the sword and scabbard (thin perches), the sleeves
//! and cloak (tether anchors all the way up).

use bd_core::voxel::Material;
use glam::{IVec3, Vec3};

use crate::arena::vcenter;
use crate::material::mat;
use crate::noise::{Rng, dcos, dsin, smoothstep, value2};
use crate::world::VoxelWorld;

/// Statue height above its pedestal top (m).
pub const HEIGHT: f32 = 55.0;
/// Shoulder-guard top (local y, m).
pub const PAULDRON_TOP: f32 = 41.8;
/// Outer half-span of the shoulder guards along the statue's right axis (m).
pub const PAULDRON_C: f32 = 7.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hair {
    /// A huge mane of spikes down the back, bangs over one eye.
    Spiky,
    /// Long straight hair to mid-back, a centre parting, locks framing the face.
    Straight,
}

/// Where a statue stands and which way it faces.
#[derive(Clone, Copy, Debug)]
pub struct StatueSpec {
    /// Feet centre on the pedestal top (m).
    pub base: Vec3,
    /// +1 faces +X (its right is +Z), −1 faces −X (its right is −Z).
    pub facing: f32,
    pub hair: Hair,
}

impl StatueSpec {
    /// World position of a local point (`a` forward, `y` up, `c` right).
    pub fn world(&self, a: f32, y: f32, c: f32) -> Vec3 {
        Vec3::new(self.base.x + self.facing * a, self.base.y + y, self.base.z + self.facing * c)
    }
    /// Local point of a world position.
    pub fn local(&self, p: Vec3) -> Vec3 {
        Vec3::new(self.facing * (p.x - self.base.x), p.y - self.base.y, self.facing * (p.z - self.base.z))
    }
    /// Top of the shoulder guard on the statue's right (`side` = +1) or left (−1): a perch.
    pub fn pauldron_top(&self, side: f32) -> Vec3 {
        self.world(0.0, PAULDRON_TOP, side * PAULDRON_C)
    }
    /// Crown of the head (roughly; the hair adds a little).
    pub fn head_top(&self) -> Vec3 {
        self.world(-0.3, 52.6, 0.0)
    }
    /// The raised two-finger seal (fingertips).
    pub fn seal_hand(&self) -> Vec3 {
        self.world(5.2, 46.0, 2.1)
    }
}

/// A tapered cone / capsule segment (local frame).
#[derive(Clone, Copy, Debug)]
struct Cone {
    a: Vec3,
    b: Vec3,
    r0: f32,
    r1: f32,
}

impl Cone {
    #[inline]
    fn contains(&self, p: Vec3) -> bool {
        let d = self.b - self.a;
        let l2 = d.length_squared();
        let t = if l2 > 0.0 { ((p - self.a).dot(d) / l2).clamp(0.0, 1.0) } else { 0.0 };
        let r = self.r0 + (self.r1 - self.r0) * t;
        (p - (self.a + d * t)).length_squared() < r * r
    }
}

/// Axis-aligned box in the local frame.
#[derive(Clone, Copy, Debug)]
struct Bx {
    lo: Vec3,
    hi: Vec3,
}

impl Bx {
    #[inline]
    fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.lo).all() && p.cmplt(self.hi).all()
    }
}

#[inline]
fn ell(p: Vec3, c: Vec3, r: Vec3) -> f32 {
    let d = (p - c) / r;
    d.length_squared()
}

/// A periodic groove: true on a `width`-wide line every `period` metres of `v`.
#[inline]
fn groove(v: f32, period: f32, width: f32) -> bool {
    (v / period).rem_euclid(1.0) < width / period
}

struct Shape {
    hair: Hair,
    spikes: Vec<Cone>,
    limbs: Vec<Cone>,
    sleeves: Vec<Bx>,
    seed: u32,
}

fn shape(hair: Hair, seed: u32) -> Shape {
    let mut rng = Rng::new(seed as u64 ^ 0x7374_6174_7565);
    let mut spikes = Vec::new();
    if hair == Hair::Spiky {
        // The mane: spikes radiating out of a big mass behind the head, drooping with their weight.
        let m = Vec3::new(-2.8, 42.0, 0.0);
        let r = Vec3::new(3.0, 10.0, 5.8);
        for _ in 0..22 {
            let el = rng.range(-0.95, 1.05);
            let az = rng.range(1.75, 4.55);
            let dir = Vec3::new(dcos(el) * dcos(az), dsin(el), dcos(el) * dsin(az));
            let root = m + r * dir * 0.72;
            let len = rng.range(4.0, 7.5);
            let tip = root + (dir + Vec3::new(0.0, -0.35, 0.0)).normalize() * len;
            spikes.push(Cone { a: root, b: tip, r0: rng.range(1.3, 1.9), r1: 0.2 });
        }
        // Crown spikes swept back.
        let cap = Vec3::new(-0.5, 48.6, 0.0);
        for _ in 0..7 {
            let el = rng.range(0.5, 1.25);
            let az = rng.range(1.6, 4.7);
            let dir = Vec3::new(dcos(el) * dcos(az) - 0.35, dsin(el), dcos(el) * dsin(az)).normalize();
            let root = cap + dir * 3.2;
            spikes.push(Cone { a: root, b: root + dir * rng.range(3.0, 5.0), r0: 1.2, r1: 0.2 });
        }
        // Long bangs over the right eye.
        spikes.push(Cone { a: Vec3::new(2.0, 51.0, 1.6), b: Vec3::new(3.5, 44.0, 2.4), r0: 1.0, r1: 0.4 });
        spikes.push(Cone { a: Vec3::new(2.2, 51.2, -0.8), b: Vec3::new(3.3, 46.5, -1.8), r0: 0.8, r1: 0.3 });
    }
    // Arms (capsules) and sword.
    let (rs, re, rw) = (Vec3::new(0.3, 39.3, 6.7), Vec3::new(2.4, 32.6, 6.3), Vec3::new(4.7, 40.4, 2.7));
    let (ls, le, lh) = (Vec3::new(0.3, 39.3, -6.7), Vec3::new(1.2, 31.2, -7.2), Vec3::new(3.9, 26.9, -5.3));
    let limbs = vec![
        Cone { a: rs, b: re, r0: 1.8, r1: 1.7 },
        Cone { a: re, b: rw, r0: 1.5, r1: 1.3 },
        Cone { a: ls, b: le, r0: 1.8, r1: 1.7 },
        Cone { a: le, b: lh, r0: 1.5, r1: 1.3 },
    ];
    // Hanging kimono sleeves: under the raised forearm, and from the left upper arm.
    let mut sleeves = Vec::new();
    for k in 0..=10 {
        let t = k as f32 / 10.0;
        let p = re + (rw - re) * (t * 0.85);
        let drop = 6.8 - 3.5 * t;
        sleeves.push(Bx { lo: Vec3::new(p.x - 1.4, p.y - drop, p.z - 1.7), hi: Vec3::new(p.x + 1.4, p.y + 0.6, p.z + 1.7) });
    }
    for k in 0..=8 {
        let t = k as f32 / 8.0;
        let p = rs + (re - rs) * t;
        sleeves.push(Bx { lo: Vec3::new(p.x - 1.5, (p.y - 4.0).max(28.5), p.z - 1.6), hi: Vec3::new(p.x + 1.5, p.y + 0.5, p.z + 1.8) });
    }
    for k in 0..=10 {
        let t = k as f32 / 10.0;
        let p = ls + (le - ls) * t;
        sleeves.push(Bx { lo: Vec3::new(p.x - 1.6, (p.y - 7.5).max(21.0), p.z - 1.9), hi: Vec3::new(p.x + 1.6, p.y + 0.5, p.z + 1.7) });
    }
    Shape { hair, spikes, limbs, sleeves, seed }
}

/// Indestructible core: two leg columns, the spine, a shoulder bar.
#[inline]
fn core(p: Vec3) -> bool {
    let (a, y, c) = (p.x, p.y, p.z);
    (y < 24.0 && (a * a + (c.abs() - 3.2) * (c.abs() - 3.2)) < 1.4 * 1.4)
        || ((22.0..40.0).contains(&y) && ((a + 0.4) * (a + 0.4) + c * c) < 1.8 * 1.8)
        || ((37.0..39.5).contains(&y) && a.abs() < 1.0 && c.abs() < 5.2)
}

fn sample(s: &Shape, p: Vec3) -> Option<Material> {
    let (a, y, c) = (p.x, p.y, p.z);
    if !(0.0..57.5).contains(&y) {
        return None;
    }
    let stone = mat::STATUE_STONE;
    let shade = mat::STATUE_SHADE;
    if core(p) {
        return Some(mat::STATUE_CORE);
    }
    // ---- right hand: fist, two raised fingers, thumb -----------------------------------------------
    if (a - 5.2).abs() < 1.15 && (y - 41.4).abs() < 1.45 && (c - 2.1).abs() < 1.2 {
        return Some(stone);
    }
    if (4.5..5.9).contains(&a) && (42.7..46.4).contains(&y) && (1.3..2.9).contains(&c) {
        return Some(if (2.0..2.2).contains(&c) { shade } else { stone });
    }
    if (5.7..6.5).contains(&a) && (41.8..42.6).contains(&y) && (0.9..1.6).contains(&c) {
        return Some(stone);
    }
    // ---- left hand on the hilt; the sword --------------------------------------------------------
    if (a - 4.1).abs() < 1.15 && (y - 26.8).abs() < 1.2 && (c + 5.2).abs() < 1.1 {
        return Some(stone);
    }
    let grip = Cone { a: Vec3::new(6.2, 28.0, -4.9), b: Vec3::new(2.6, 25.7, -5.5), r0: 0.55, r1: 0.55 };
    if grip.contains(p) {
        return Some(shade);
    }
    {
        // Tsuba: a disc across the blade axis.
        let o = Vec3::new(2.4, 25.6, -5.55);
        let ax = (Vec3::new(2.6, 25.7, -5.5) - Vec3::new(-8.3, 16.2, -6.9)).normalize();
        let d = p - o;
        let along = d.dot(ax);
        if along.abs() < 0.3 && (d - ax * along).length_squared() < 1.1 * 1.1 {
            return Some(stone);
        }
    }
    let scabbard = Cone { a: Vec3::new(2.2, 25.4, -5.6), b: Vec3::new(-8.3, 16.2, -6.9), r0: 0.75, r1: 0.65 };
    if scabbard.contains(p) {
        return Some(shade);
    }
    // ---- arms + sleeves ---------------------------------------------------------------------------
    if s.limbs.iter().any(|l| l.contains(p)) {
        return Some(stone);
    }
    if s.sleeves.iter().any(|b| b.contains(p)) {
        return Some(if groove(a + c * 0.3, 1.3, 0.3) { shade } else { stone });
    }
    // ---- head --------------------------------------------------------------------------------------
    // Face details first (they poke out of the head ellipsoid).
    if (2.8..3.6).contains(&a) && (46.3..48.4).contains(&y) && c.abs() < 0.45 {
        return Some(stone);
    }
    if (2.6..3.3).contains(&a) && (48.4..49.0).contains(&y) && c.abs() < 2.3 {
        return Some(shade);
    }
    let jaw = if y < 46.5 { 1.0 - 0.22 * ((46.5 - y) / 2.5).min(1.0) } else { 1.0 };
    let head = ell(p, Vec3::new(0.35, 47.9, 0.0), Vec3::new(3.0 * jaw, 4.0, 2.75 * jaw)) < 1.0;
    // Hair (drawn before the face so it wins at the crown, but never over the face).
    let face = a > 1.4 && y < 50.2 && y > 43.0 && c.abs() < 2.6;
    let hair = match s.hair {
        Hair::Spiky => {
            (!face && ell(p, Vec3::new(-0.5, 48.6, 0.0), Vec3::new(3.5, 4.4, 3.3)) < 1.0)
                || (y > 31.0 && ell(p, Vec3::new(-2.8, 41.5, 0.0), Vec3::new(3.0, 10.0, 5.8)) < 1.0)
                || s.spikes.iter().any(|k| k.contains(p))
        }
        Hair::Straight => {
            let cap = !face && ell(p, Vec3::new(-0.3, 48.5, 0.0), Vec3::new(3.35, 4.7, 3.15)) < 1.0;
            let wide = 3.4 + 0.9 * ((49.5 - y) / 19.5).clamp(0.0, 1.0);
            let sheet = (30.0..49.5).contains(&y) && (-3.6..-1.0).contains(&a) && c.abs() < wide && a > -3.6 + 0.6 * (c / wide) * (c / wide);
            let locks = (2.4..3.4).contains(&c.abs()) && (0.3..2.1).contains(&a) && (37.0..49.0).contains(&y);
            cap || sheet || locks
        }
    };
    if hair {
        let strand = match s.hair {
            Hair::Spiky => groove(c * 0.9 + y * 0.3, 1.3, 0.3),
            Hair::Straight => groove(c + 20.0, 0.9, 0.25) || (c.abs() < 0.35 && y > 51.5),
        };
        return Some(if strand { shade } else { stone });
    }
    if head {
        let eyes = a > 2.2 && (47.6..48.3).contains(&y) && (0.7..1.9).contains(&c.abs());
        let mouth = a > 2.3 && (45.0..45.4).contains(&y) && c.abs() < 1.0;
        return Some(if eyes || mouth { shade } else { stone });
    }
    // ---- neck, collar, shoulder guards -------------------------------------------------------------
    if (40.0..45.0).contains(&y) && a * a + c * c < 1.9 * 1.9 {
        return Some(stone);
    }
    if (39.5..43.0).contains(&y) && ell(Vec3::new(a, 0.0, c), Vec3::ZERO, Vec3::new(3.1, 1.0, 4.3)) < 1.0 {
        return Some(if y > 42.5 { shade } else { stone });
    }
    let cc = c.abs();
    if (31.5..PAULDRON_TOP).contains(&y) && (-3.3..3.3).contains(&a) && cc >= 5.2 && cc < 9.3 + 0.1 * (PAULDRON_TOP - y) {
        return Some(if y < PAULDRON_TOP - 0.5 && groove(PAULDRON_TOP - y, 1.7, 0.35) { shade } else { stone });
    }
    // ---- cuirass, belt, armour skirt ---------------------------------------------------------------
    if (28.0..41.0).contains(&y) {
        let k = if y > 38.0 { 1.0 - 0.05 * (y - 38.0) } else { 1.0 };
        let (ea, ec) = (((a.abs() - 2.5 * k).max(0.0)) / 1.2, ((cc - 4.6 * k).max(0.0)) / 1.2);
        if a.abs() < 3.7 * k && cc < 5.8 * k && ea * ea + ec * ec < 1.0 {
            let plates = a > 0.0 && (groove(y - 28.0, 1.7, 0.3) || groove(c + 0.85, 1.7, 0.3));
            return Some(if plates { shade } else { stone });
        }
    }
    if (26.5..28.6).contains(&y) && ell(Vec3::new(a, 0.0, c), Vec3::ZERO, Vec3::new(4.0, 1.0, 6.0)) < 1.0 {
        return Some(shade);
    }
    if (19.0..27.5).contains(&y) && a > -2.5 {
        let ra = 4.9 - 0.25 * (y - 19.0) / 8.5;
        if ell(Vec3::new(a, 0.0, c), Vec3::ZERO, Vec3::new(ra, 1.0, 7.3)) < 1.0 {
            return Some(if groove(y - 19.0, 1.7, 0.35) || groove(c + 20.0, 2.4, 0.4) { shade } else { stone });
        }
    }
    // ---- cloak (open at the front), hakama, sandals ------------------------------------------------
    if (2.5..40.0).contains(&y) && a < 0.9 {
        let rc = 8.3 - 2.3 * smoothstep(31.0, 40.0, y);
        let hem = 2.5 + 1.2 * (0.5 + 0.5 * value2(c * 0.8, a * 0.8, s.seed));
        if y >= hem && ell(Vec3::new(a, 0.0, c), Vec3::ZERO, Vec3::new(5.1, 1.0, rc)) < 1.0 {
            return Some(if groove(c + 20.0, 2.0, 0.4) { shade } else { stone });
        }
    }
    if (1.0..25.0).contains(&y) {
        let t = (y - 1.0) / 24.0;
        let (ra, rc) = (4.4 - 0.7 * t, 6.9 - 1.2 * t);
        if ell(Vec3::new(a, 0.0, c), Vec3::ZERO, Vec3::new(ra, 1.0, rc)) < 1.0 {
            // The split between the legs at the front.
            if a > ra * 0.55 && cc < 0.55 && y < 10.0 {
                return None;
            }
            return Some(if groove(c + 0.35 * a + 20.0, 1.8, 0.35) { shade } else { stone });
        }
    }
    if y < 1.8 && (-1.6..3.6).contains(&a) && (cc - 3.3).abs() < 1.4 && y < 1.8 - 0.4 * (a - 2.4).max(0.0) {
        return Some(if y < 0.5 { shade } else { stone });
    }
    None
}

/// Carve a statue into `w` (only where there's air, so the pedestal stays).
pub fn build(w: &mut VoxelWorld, s: &StatueSpec, seed: u32) {
    let sh = shape(s.hair, seed);
    // Local bounds → world voxel bounds.
    let (a0, a1, c0, c1) = (-10.5, 8.0, -11.0, 11.0);
    let p0 = s.world(a0, 0.0, c0);
    let p1 = s.world(a1, 57.5, c1);
    let lo = (p0.min(p1) / 0.5).floor().as_ivec3();
    let hi = (p0.max(p1) / 0.5).floor().as_ivec3();
    w.fill_with(lo, hi, |v: IVec3, old| {
        if old != bd_core::voxel::AIR {
            return None;
        }
        sample(&sh, s.local(vcenter(v)))
    });
}
