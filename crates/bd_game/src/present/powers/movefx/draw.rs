//! The grammar's drawing primitives — every one is GPU particles (bd_fx `FxParticles`: a ring pool
//! written once at spawn) or this-frame immediate cubes. No entities, no meshes, no materials, no
//! allocation: eight players spamming moves cost particle slots only.
//!
//! Every primitive is shaped by the style's [`Body`]: a burst of a Smoke body is lit puffs that
//! linger, of a Bolt body white-hot stretched sparks, of a Grain body falling grit…

use super::table::Body;
use bd_fx::{Emit, FxParticles, PFlags, PKind};
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

/// A resolved style (linear colours), carried by value in every beat.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub style: u16,
    pub body: Body,
    pub main: [f32; 3],
    pub core: [f32; 3],
    pub dark: [f32; 3],
    pub glow: f32,
    pub density: f32,
    pub rise: f32,
    pub jitter: f32,
    pub wobble: f32,
    pub speed: f32,
    pub linger: f32,
    pub size: f32,
    pub afterimages: u8,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            style: 0,
            body: Body::Energy,
            main: [0.4, 0.9, 0.5],
            core: [1.0; 3],
            dark: [0.05; 3],
            glow: 3.0,
            density: 1.0,
            rise: 0.0,
            jitter: 0.0,
            wobble: 0.0,
            speed: 1.0,
            linger: 1.0,
            size: 1.0,
            afterimages: 0,
        }
    }
}

/// The particle pool + this frame's scale knobs.
pub struct Pen<'a> {
    pub fx: &'a mut FxParticles,
    /// Graphics particle budget (0..1) × the table's base mote count.
    pub motes: f32,
    /// Size scale (the local player's own beats are smaller near the camera).
    pub scale: f32,
    /// The camera: nothing blooms in your face (motes shrink away inside ~2 m, flashes and beams
    /// through the lens are dropped / pulled back) — your own blink never whites out your view.
    pub cam: Vec3,
    /// Your own body in first person (where your own move lands: the camera is about to be there
    /// even if it is not yet — prediction / interpolation): nothing spawns inside it.
    pub avoid: Option<Vec3>,
}

pub fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn mul3(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// Two unit vectors perpendicular to `n` (and each other).
pub fn basis(n: Vec3) -> (Vec3, Vec3) {
    let n = n.normalize_or(Vec3::Y);
    let a = if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let u = n.cross(a).normalize_or(Vec3::X);
    (u, n.cross(u))
}

fn hash(k: u32) -> f32 {
    let mut x = k.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

impl Pen<'_> {
    /// 0 (hidden) … 1: how much of a particle at `p` (radius `r`) may show this close to the camera.
    pub fn near(&self, p: Vec3, r: f32) -> f32 {
        let s = |t: f32| {
            let t = t.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let cam = s((p.distance(self.cam) - r - 0.6) / 1.6);
        // Your own arrival point keeps a wider berth (puffs drift and grow toward the lens).
        let own = self.avoid.map_or(1.0, |a| s((p.distance(a) - r - 2.4) / 1.6));
        cam.min(own)
    }

    /// Every emission goes through here (the near-camera guard).
    pub fn put(&mut self, mut e: Emit) -> bool {
        if e.kind == PKind::Beam {
            let (a, b) = (e.pos - e.vel * 0.5, e.pos + e.vel * 0.5);
            let (na, nb) = (self.near(a, 0.0), self.near(b, 0.0));
            if na < 0.05 && nb < 0.05 {
                return false;
            }
            // Pull a lens-side end back along the beam (it still points at you, it never fills you).
            let len = e.vel.length();
            let dir = e.vel / len.max(1e-4);
            let (a, b) = (if na < 0.5 { a + dir * (1.4f32).min(len * 0.5) } else { a }, if nb < 0.5 { b - dir * (1.4f32).min(len * 0.5) } else { b });
            e.pos = (a + b) * 0.5;
            e.vel = b - a;
            return self.fx.emit(e);
        }
        let r = if e.kind == PKind::Flash { e.size * 0.6 } else { e.size * 0.5 };
        let k = self.near(e.pos, r);
        if k < 0.04 {
            return false;
        }
        e.size *= k;
        self.fx.emit(e)
    }

    fn n(&self, base: f32, look: &Look) -> usize {
        (base * self.motes * look.density).round().clamp(0.0, 400.0) as usize
    }

    pub fn flash(&mut self, pos: Vec3, size: f32, color: [f32; 3], glow: f32, life: f32) {
        self.put(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos, size: size * self.scale, color, emissive: glow, life, gravity: 0.0, ..default() });
    }

    /// A timed beam (thick core thinning to a trail).
    pub fn beam(&mut self, a: Vec3, b: Vec3, thick: f32, color: [f32; 3], glow: f32, life: f32) {
        if a.distance_squared(b) < 1e-4 {
            return;
        }
        self.put(Emit { kind: PKind::Beam, flags: PFlags::PLAIN, pos: (a + b) * 0.5, vel: b - a, size: thick, color, emissive: glow, life, gravity: 0.0, ..default() });
    }

    /// One body-shaped mote at `pos` moving with `vel`.
    pub fn mote(&mut self, look: &Look, pos: Vec3, vel: Vec3, life: f32, k: f32) {
        let s = look.size * self.scale;
        let rng = &mut self.fx.rng;
        let r = rng.f();
        let hot = mix3(look.core, look.main, (0.35 + 0.65 * r).min(1.0));
        let e = match look.body {
            Body::Energy | Body::Rubber | Body::Halo => Emit {
                flags: PFlags::WHITE_BIRTH,
                pos,
                vel,
                size: (0.06 + 0.07 * r) * s,
                color: hot,
                emissive: look.glow,
                life: life * (0.7 + 0.5 * k),
                gravity: 0.15 - look.rise * 0.05,
                drag: 3.0,
                spin: 6.0,
                ..default()
            },
            Body::Rigid => Emit {
                flags: PFlags::STRETCH | PFlags::WHITE_BIRTH,
                pos,
                vel: vel * 1.3,
                size: 0.035 * s,
                color: mix3(look.core, look.main, r * 0.5),
                emissive: look.glow * 1.3,
                life: life * 0.8,
                gravity: 1.2,
                drag: 1.5,
                spin: 3.5,
                ..default()
            },
            Body::Smoke => Emit {
                kind: PKind::Rise,
                flags: PFlags::SMOKE | PFlags::GROW,
                pos,
                vel: vel * 0.55,
                size: (0.35 + 0.35 * r) * s,
                color: mix3(look.dark, look.main, 0.25 * r),
                emissive: 0.9,
                life: life * 2.2 * look.linger,
                gravity: 0.35 + look.rise,
                drag: 3.2,
                spin: 0.25,
                ..default()
            },
            Body::Bolt | Body::Streak => Emit {
                flags: PFlags::STRETCH | PFlags::WHITE_BIRTH,
                pos,
                vel: vel * 1.6,
                size: 0.03 * s,
                color: hot,
                emissive: look.glow * 1.5,
                life: life * 0.5,
                gravity: 0.0,
                drag: 4.0,
                spin: if look.body == Body::Streak { 9.0 } else { 5.0 },
                ..default()
            },
            Body::Liquid => Emit {
                flags: PFlags::STRETCH,
                pos,
                vel: vel + Vec3::Y * 2.5,
                size: (0.05 + 0.05 * r) * s,
                color: mix3(look.main, look.core, 0.2 * r),
                emissive: look.glow * 0.4,
                life: life * 1.2,
                gravity: 1.0 - look.rise * 0.1,
                drag: 0.6,
                spin: 2.0,
                ..default()
            },
            Body::Grain => Emit {
                flags: PFlags::SMOKE,
                pos,
                vel: vel * 0.8,
                size: (0.035 + 0.03 * r) * s,
                color: mix3(look.main, look.dark, r * 0.6),
                emissive: 0.7,
                life: life * 1.6,
                gravity: 1.1,
                drag: 1.6,
                spin: 5.0,
                ..default()
            },
            Body::Flame => Emit {
                kind: PKind::Rise,
                flags: PFlags::WHITE_BIRTH,
                pos,
                vel: vel * 0.6,
                size: (0.07 + 0.08 * r) * s,
                color: mix3(look.core, look.main, r),
                emissive: look.glow * 1.2,
                life: life * 1.1,
                gravity: 1.8 + look.rise,
                drag: 3.0,
                spin: 0.5,
                ..default()
            },
            Body::Shard => Emit {
                flags: PFlags::STRETCH | PFlags::WHITE_BIRTH,
                pos,
                vel,
                size: (0.06 + 0.05 * r) * s,
                color: mix3(look.core, look.main, 0.4 + 0.6 * r),
                emissive: look.glow,
                life: life * 1.3,
                gravity: 0.7,
                drag: 2.5,
                spin: 2.2,
                ..default()
            },
            Body::Gust => Emit {
                flags: PFlags::STRETCH | PFlags::PLAIN,
                pos,
                vel: vel + look_swirl(vel, r),
                size: 0.025 * s,
                color: mix3(look.core, look.main, 0.5),
                emissive: look.glow * 0.8,
                life: life * 0.9,
                gravity: -0.05,
                drag: 2.0,
                spin: 7.0,
                ..default()
            },
            Body::Glitch => Emit {
                flags: PFlags::PLAIN,
                pos: pos + vel * 0.08 * r,
                vel: Vec3::ZERO,
                size: (0.06 + 0.1 * r) * s,
                color: if r > 0.7 { look.core } else { look.main },
                emissive: look.glow,
                life: 0.05 + 0.12 * r,
                gravity: 0.0,
                delay: life * 0.8 * k * r,
                ..default()
            },
            Body::Paper => Emit {
                flags: PFlags::SMOKE,
                pos,
                vel: vel * 0.6 + Vec3::Y * 1.5,
                size: (0.1 + 0.06 * r) * s,
                color: mix3(look.main, look.core, 0.3 * r),
                emissive: 0.8,
                life: life * 2.2 * look.linger,
                gravity: 0.12,
                drag: 2.6,
                spin: 9.0,
                ..default()
            },
            Body::Void => Emit {
                flags: PFlags::PLAIN,
                pos,
                vel,
                size: (0.07 + 0.06 * r) * s,
                color: if r > 0.6 { look.main } else { look.dark },
                emissive: if r > 0.6 { look.glow } else { 0.2 },
                life,
                gravity: 0.0,
                drag: 3.0,
                spin: 4.0,
                ..default()
            },
        };
        let e = Emit { seed: rng.next_u32(), ..e };
        self.put(e);
    }

    /// Motes burst out of `pos` (along `dir` ± `spread` radians; `None` = all round).
    pub fn burst(&mut self, look: &Look, pos: Vec3, count: f32, speed: f32, dir: Option<Vec3>, spread: f32, life: f32) {
        let n = self.n(count, look);
        for i in 0..n {
            let d = match dir {
                Some(d) => {
                    let (r1, r2) = (self.fx.rng.f(), self.fx.rng.f());
                    cone_dir(d, spread, r1, r2)
                }
                None => self.fx.rng.dir(),
            };
            let v = d * speed * (0.45 + 0.75 * self.fx.rng.f());
            let k = i as f32 / n.max(1) as f32;
            self.mote(look, pos + d * 0.15, v, life, k);
        }
    }

    /// Motes pulled in from a shell to `c`, all arriving at `secs` (a gather / an implosion).
    pub fn implode(&mut self, look: &Look, c: Vec3, radius: f32, secs: f32, count: f32) {
        let n = self.n(count, look);
        let secs = secs.max(0.04);
        for _ in 0..n {
            let d = self.fx.rng.dir();
            let r = radius * (0.6 + 0.5 * self.fx.rng.f());
            let p = c + d * r;
            let col = mix3(look.core, look.main, 0.3 + 0.7 * self.fx.rng.f());
            let (color, emissive, flags) = match look.body {
                Body::Smoke | Body::Grain | Body::Paper => (mix3(look.dark, look.main, 0.3), 0.8, PFlags::SMOKE | PFlags::STRETCH),
                Body::Void => (look.dark, 0.3, PFlags::STRETCH | PFlags::PLAIN),
                _ => (col, look.glow, PFlags::STRETCH),
            };
            let seed = self.fx.rng.next_u32();
            self.put(Emit {
                flags,
                pos: p,
                vel: (c - p) / secs,
                size: 0.045 * look.size * self.scale,
                color,
                emissive,
                life: secs,
                gravity: 0.0,
                spin: 3.0,
                seed,
                ..default()
            });
        }
    }

    /// A shockwave ring in the plane ⟂ `normal`, racing out from `r0` at `speed`.
    pub fn ring(&mut self, look: &Look, c: Vec3, normal: Vec3, r0: f32, speed: f32, count: f32, life: f32) {
        let n = self.n(count, look).max(6);
        let (u, v) = basis(normal);
        let ph = self.fx.rng.f() * TAU;
        for i in 0..n {
            let a = ph + i as f32 / n as f32 * TAU;
            let d = u * a.cos() + v * a.sin();
            let seed = self.fx.rng.next_u32();
            let (color, emissive, flags) = match look.body {
                Body::Smoke | Body::Grain | Body::Paper => (mix3(look.dark, look.main, 0.35), 0.8, PFlags::SMOKE | PFlags::GROW),
                Body::Void => (look.main, look.glow, PFlags::STRETCH | PFlags::PLAIN),
                _ => (mix3(look.core, look.main, 0.55), look.glow, PFlags::STRETCH | PFlags::PLAIN),
            };
            let big = matches!(look.body, Body::Smoke | Body::Grain | Body::Paper);
            self.put(Emit {
                flags,
                pos: c + d * r0,
                vel: d * speed,
                size: if big { 0.3 } else { 0.06 } * look.size * self.scale,
                color,
                emissive,
                life: life * if big { 1.8 * look.linger } else { 1.0 },
                gravity: 0.0,
                drag: 2.2,
                spin: 3.0,
                seed,
                ..default()
            });
        }
    }

    /// A column of dust / grit rising at a ring (the dust wall of a slam).
    pub fn dust_wall(&mut self, look: &Look, c: Vec3, r: f32, height: f32, count: f32) {
        let n = self.n(count, look);
        for i in 0..n {
            let a = i as f32 / n.max(1) as f32 * TAU + self.fx.rng.f() * 0.3;
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            let seed = self.fx.rng.next_u32();
            let dust = mix3([0.42, 0.36, 0.3], look.dark, 0.3);
            let (r1, r2, r3, r4) = (self.fx.rng.f(), self.fx.rng.f(), self.fx.rng.f(), self.fx.rng.f());
            self.put(Emit {
                kind: PKind::Rise,
                flags: PFlags::SMOKE | PFlags::GROW,
                pos: c + d * r * (0.8 + 0.3 * r1),
                vel: d * (4.0 + 3.0 * r2),
                size: (0.28 + 0.25 * r3) * self.scale,
                color: dust,
                emissive: 0.75,
                life: 0.9 + 0.6 * r4,
                gravity: height,
                drag: 2.4,
                spin: 0.4,
                seed,
                ..default()
            });
        }
    }

    /// A body-shaped trail laid once along a→b (smoke tube, ember line, drops, grit…); the per-frame
    /// bodies (bolt, rubber, rigid) draw theirs in [`Pen::trace_now`].
    pub fn trace(&mut self, look: &Look, a: Vec3, b: Vec3, life: f32) {
        let d = b - a;
        let len = d.length();
        if len < 0.05 {
            return;
        }
        let dir = d / len;
        match look.body {
            Body::Energy | Body::Halo | Body::Rubber | Body::Rigid => {
                self.beam(a, b, 0.14 * look.size, look.main, look.glow, life);
                self.beam(a, b, 0.05 * look.size, look.core, look.glow * 1.5, life * 0.6);
            }
            Body::Streak => {
                let (u, v) = basis(dir);
                for k in 0..4 {
                    let o = (u * (k as f32 - 1.5) * 0.18 + v * ((k % 2) as f32 - 0.5) * 0.3) * look.size;
                    self.beam(a + o, b + o, 0.05, if k % 2 == 0 { look.core } else { look.main }, look.glow * 1.4, life * (0.6 + 0.15 * k as f32));
                }
            }
            Body::Void => {
                self.beam(a, b, 0.22 * look.size, look.dark, 0.1, life);
                self.beam(a, b, 0.06 * look.size, look.main, look.glow, life * 0.7);
            }
            _ => {}
        }
        let per_m = match look.body {
            Body::Smoke => 1.6,
            Body::Liquid | Body::Grain | Body::Flame | Body::Shard | Body::Paper => 3.0,
            Body::Gust | Body::Glitch => 2.5,
            Body::Streak | Body::Bolt => 1.5,
            _ => 1.0,
        };
        let n = ((len * per_m * look.density * self.motes / 22.0).round() as usize).clamp(2, 80);
        for i in 0..n {
            let t = (i as f32 + self.fx.rng.f()) / n as f32;
            let p = a + d * t + self.fx.rng.dir() * 0.15;
            let v = self.fx.rng.dir() * 0.6 + dir * 0.5;
            self.mote(look, p, v, life * 1.4, t);
        }
    }

    /// Per-frame part of a trace (u = 0..1 age): jagged bolt re-struck, and nothing for the rest.
    pub fn trace_now(&mut self, look: &Look, a: Vec3, b: Vec3, u: f32, seed: u32) {
        match look.body {
            Body::Bolt => {
                let fade = (1.0 - u).max(0.0);
                let amp = 0.25 + look.jitter;
                zig(self.fx, a, b, 9, amp, seed, 0.06 * fade + 0.02, look.core, look.glow * 2.0 * fade);
                zig(self.fx, a, b, 7, amp * 1.4, seed ^ 0x55, 0.03 * fade + 0.01, look.main, look.glow * 1.4 * fade);
            }
            Body::Streak if u < 0.6 => {
                zig(self.fx, a, b, 12, 0.3 + look.jitter, seed, 0.02, look.main, look.glow * (1.0 - u));
            }
            _ => {}
        }
    }

    /// First person, your own blink / dash: the world rushes past the lens (speed lines in the
    /// body's shape at the edge of view) — the only part of your own move you can see.
    pub fn rush(&mut self, look: &Look, cam: Vec3, fwd: Vec3, strength: f32, count: f32) {
        let (u, v) = basis(fwd);
        let n = self.n(count * strength, look).max(1);
        for _ in 0..n {
            let a = self.fx.rng.f() * TAU;
            let rad = 1.1 + 1.2 * self.fx.rng.f();
            let ahead = 3.0 + 5.0 * self.fx.rng.f();
            let p = cam + fwd * ahead + (u * a.cos() + v * a.sin()) * rad;
            let seed = self.fx.rng.next_u32();
            let (flags, color, emissive, size, life, spin) = match look.body {
                Body::Smoke | Body::Paper | Body::Grain => (PFlags::SMOKE | PFlags::GROW, mix3(look.dark, look.main, 0.4), 0.9, 0.35, 0.3 * look.linger, 0.3),
                _ => (PFlags::STRETCH | PFlags::PLAIN, mix3(look.core, look.main, 0.4), look.glow, 0.03, 0.2, 9.0),
            };
            self.put(Emit { flags, pos: p, vel: -fwd * (18.0 + 14.0 * strength), size: size * look.size, color, emissive, life, gravity: 0.0, drag: 1.0, spin, seed, ..default() });
        }
    }

    /// A ghost silhouette (afterimage) of a body standing at `feet`, facing `yaw`.
    pub fn ghost(&mut self, feet: Vec3, yaw: f32, size: f32, color: [f32; 3], glow: f32, life: f32) {
        let rot = Quat::from_rotation_y(yaw);
        // (offset, cube size): legs, hips, chest, head, arms.
        const PARTS: [([f32; 3], f32); 8] =
            [([-0.13, 0.35, 0.0], 0.2), ([0.13, 0.35, 0.0], 0.2), ([0.0, 0.8, 0.0], 0.34), ([0.0, 1.15, 0.0], 0.4), ([0.0, 1.62, 0.0], 0.26), ([-0.33, 1.15, 0.0], 0.16), ([0.33, 1.15, 0.0], 0.16), ([0.0, 0.62, 0.0], 0.28)];
        for (o, s) in PARTS {
            let seed = self.fx.rng.next_u32();
            self.put(Emit {
                flags: PFlags::PLAIN,
                pos: feet + rot * (Vec3::from(o) * size),
                vel: Vec3::ZERO,
                size: s * size,
                color,
                emissive: glow,
                life,
                gravity: 0.0,
                spin: 0.0,
                seed: seed & !0xFFFF,
                ..default()
            });
        }
    }

    /// A crack running along the ground from `from` in `dir` for `len` m: dust + dark chips.
    pub fn crack(&mut self, look: &Look, from: Vec3, dir: Vec3, len: f32, seed: u32) {
        let dir = Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::X);
        let side = Vec3::new(-dir.z, 0.0, dir.x);
        let n = ((len * 3.0).round() as usize).clamp(3, 40);
        let mut prev = from;
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let p = from + dir * len * t + side * (hash(seed.wrapping_add(i as u32)) - 0.5) * 0.5;
            let s1 = self.fx.rng.next_u32();
            self.put(Emit {
                flags: PFlags::PLAIN,
                pos: (prev + p) * 0.5 + Vec3::Y * 0.03,
                vel: Vec3::ZERO,
                size: 0.1,
                color: mix3(look.main, look.core, 0.4),
                emissive: look.glow * 0.8,
                life: 0.5 + 0.8 * look.linger,
                gravity: 0.0,
                delay: t * 0.08,
                seed: s1,
                ..default()
            });
            let s2 = self.fx.rng.next_u32();
            self.put(Emit {
                flags: PFlags::SMOKE,
                pos: p + Vec3::Y * 0.1,
                vel: Vec3::Y * (2.0 + 3.0 * hash(s2)) + side * (hash(s2 ^ 7) - 0.5) * 2.0,
                size: 0.08 + 0.06 * hash(s2 ^ 3),
                color: [0.32, 0.28, 0.24],
                emissive: 0.6,
                life: 0.7,
                gravity: 1.0,
                drag: 0.8,
                spin: 6.0,
                delay: t * 0.08,
                seed: s2,
                ..default()
            });
            prev = p;
        }
    }
}

fn look_swirl(v: Vec3, r: f32) -> Vec3 {
    let (u, _) = basis(v);
    u * (r - 0.5) * 3.0
}

/// A direction within `spread` radians of `d` (`r1`, `r2` uniform 0..1).
pub fn cone_dir(d: Vec3, spread: f32, r1: f32, r2: f32) -> Vec3 {
    let (u, v) = basis(d);
    let a = r1 * TAU;
    let r = spread.min(1.5) * r2.sqrt();
    (d.normalize_or(Vec3::Z) + (u * a.cos() + v * a.sin()) * r.tan()).normalize_or(d)
}

/// An immediate jagged line a→b (`segs` segments, ±`amp` m), re-struck when `seed` changes.
pub fn zig(fx: &mut FxParticles, a: Vec3, b: Vec3, segs: usize, amp: f32, seed: u32, thick: f32, color: [f32; 3], glow: f32) {
    let d = b - a;
    let (u, v) = basis(d);
    let mut prev = a;
    for i in 1..=segs {
        let t = i as f32 / segs as f32;
        let env = (t * PI).sin();
        let o = if i == segs { Vec3::ZERO } else { (u * (hash(seed.wrapping_mul(31).wrapping_add(i as u32)) - 0.5) + v * (hash(seed.wrapping_mul(17).wrapping_add(i as u32 * 7)) - 0.5)) * 2.0 * amp * env };
        let p = a + d * t + o;
        fx.immediate_segment(prev, p, thick, color, glow, PFlags::PLAIN);
        prev = p;
    }
}

/// An immediate circle (ground marks, boundaries).
pub fn circle(fx: &mut FxParticles, c: Vec3, normal: Vec3, r: f32, segs: usize, thick: f32, color: [f32; 3], glow: f32, phase: f32) {
    let (u, v) = basis(normal);
    let segs = segs.max(6);
    let mut prev = c + (u * phase.cos() + v * phase.sin()) * r;
    for i in 1..=segs {
        let a = phase + i as f32 / segs as f32 * TAU;
        let p = c + (u * a.cos() + v * a.sin()) * r;
        fx.immediate_segment(prev, p, thick, color, glow, PFlags::PLAIN);
        prev = p;
    }
}

/// Elastic limb from `root` to `tip`: `wob` = wobble amplitude now (m), `ph` its phase. Thins as it
/// stretches (volume kept), a fist blob at the tip. Rubber.
pub fn rubber_limb(fx: &mut FxParticles, root: Vec3, tip: Vec3, thick: f32, wob: f32, ph: f32, color: [f32; 3], core: [f32; 3], glow: f32) {
    let d = tip - root;
    let len = d.length();
    if len < 0.02 {
        return;
    }
    let (u, v) = basis(d);
    // Thinner the further it stretches (rest length ≈ 0.7 m), never a thread.
    let t = thick * (0.7 / len.max(0.7)).sqrt().clamp(0.5, 1.0);
    // Longer = floppier (a 20 m arm swings metres, a 2 m one centimetres).
    let wob = wob * (0.6 + len * 0.12).min(4.0);
    let segs = 14;
    let mut prev = root;
    for i in 1..=segs {
        let s = i as f32 / segs as f32;
        // A standing wave pinned at both ends + a travelling ripple.
        let env = (s * PI).sin();
        let w = wob * env * ((s * 2.2 + ph) * TAU).sin();
        let w2 = wob * 0.5 * env * ((s * 1.3 - ph * 1.7) * TAU).cos();
        let p = root + d * s + u * w + v * w2;
        fx.immediate_segment(prev, p, t, color, glow * 0.55, PFlags::PLAIN);
        // A hot seam down the arm reads as stretched material.
        if i % 2 == 0 {
            fx.immediate_segment(prev, p, t * 0.35, core, glow * 0.9, PFlags::PLAIN);
        }
        prev = p;
    }
    // Fist: a chunky box at the tip, squashed along the stretch.
    let squash = (len / 3.0).clamp(0.0, 0.35);
    let rot = Quat::from_rotation_arc(Vec3::Z, d / len);
    fx.immediate_box(tip, rot, Vec3::new(thick * 2.3 * (1.0 + squash * 0.5), thick * 2.3 * (1.0 + squash * 0.5), thick * 2.3 * (1.0 - squash)), color, glow * 0.8, PFlags::PLAIN);
}

/// Rigid telescoping pole from `root` to `tip`: `n` nested segments (each thinner), bright bands at
/// every joint, dead straight. `ext` 0..1 = how far the segments have slid out (they deploy in order).
pub fn rigid_pole(fx: &mut FxParticles, root: Vec3, tip: Vec3, thick: f32, ext: f32, color: [f32; 3], band: [f32; 3], glow: f32) {
    let d = tip - root;
    let len = d.length();
    if len < 0.02 {
        return;
    }
    let dir = d / len;
    let rot = Quat::from_rotation_arc(Vec3::Z, dir);
    let n = 4;
    let seg = len / n as f32;
    let mut at = root;
    for k in 0..n {
        // Segment k slides out once the ones before it are out (staggered).
        let e = ((ext * n as f32) - k as f32).clamp(0.0, 1.0);
        let l = seg * (0.25 + 0.75 * e);
        let r = thick * 0.92f32.powi(k as i32);
        let end = at + dir * l;
        fx.immediate_segment(at, end, r, color, glow * 0.35, PFlags::PLAIN);
        fx.immediate_box(end, rot, Vec3::new(r * 1.6, r * 1.6, r * 1.1), band, glow * 1.2, PFlags::PLAIN);
        at = end;
    }
    // A thin gold glint down the shaft: readable against sky and shadow alike.
    fx.immediate_segment(root + dir * (len * 0.05), at, thick * 0.3, band, glow * 0.5, PFlags::PLAIN);
}

/// A spectral limb (every other body): a glowing beam with a hot core and a fist of light.
pub fn light_limb(fx: &mut FxParticles, root: Vec3, tip: Vec3, thick: f32, color: [f32; 3], core: [f32; 3], glow: f32) {
    let d = tip - root;
    let len = d.length();
    if len < 0.02 {
        return;
    }
    fx.immediate_segment(root, tip, thick * 0.8, color, glow * 0.6, PFlags::PLAIN);
    fx.immediate_segment(root, tip, thick * 0.3, core, glow * 1.3, PFlags::PLAIN);
    let rot = Quat::from_rotation_arc(Vec3::Z, d / len);
    fx.immediate_box(tip, rot, Vec3::splat(thick * 2.2), mul3(color, 1.0), glow, PFlags::PLAIN);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_is_orthonormal() {
        for n in [Vec3::Y, Vec3::X, Vec3::new(0.3, -0.8, 0.2)] {
            let (u, v) = basis(n);
            let n = n.normalize();
            assert!(u.dot(n).abs() < 1e-4 && v.dot(n).abs() < 1e-4 && u.dot(v).abs() < 1e-4);
            assert!((u.length() - 1.0).abs() < 1e-4 && (v.length() - 1.0).abs() < 1e-4);
        }
    }
}
