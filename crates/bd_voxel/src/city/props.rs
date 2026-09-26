//! Street furniture and set dressing: cars (small obstacles to hop), billboards and water towers
//! (tether anchors), shipping containers (cover / parkour blocks), gantry cranes (high anchors over
//! the port), and street lights.

use bd_core::voxel::Material;
use glam::{Vec2, Vec3};

use super::{City, VS};
use crate::material::mat;

#[derive(Clone, Copy, Debug)]
pub struct CarSpec {
    /// Road point under the middle of the car.
    pub base: Vec3,
    pub along_x: bool,
    pub body: Material,
    /// Convertible: no roof (easier to hop into / over).
    pub convertible: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct BillboardSpec {
    /// Point under the middle of the sign (ground or roof).
    pub base: Vec3,
    /// The sign faces along +normal; its panel spans across.
    pub normal: Vec2,
    pub width: f32,
    pub height: f32,
    /// Height of the panel's bottom edge above `base`.
    pub leg_h: f32,
    /// Horizontal colour bands from bottom to top (the art).
    pub bands: [Material; 4],
    pub neon: Material,
}

impl City {
    fn aabb2(&mut self, a: Vec2, b: Vec2, y0: f32, y1: f32, m: Material) {
        let (lo, hi) = (a.min(b), a.max(b));
        self.fill(Vec3::new(lo.x, y0, lo.y), Vec3::new(hi.x, y1, hi.y), m);
    }

    /// A 4.5 × 2 m car (1.5 m tall; a convertible is 1 m): wheels touch the road so it's anchored.
    pub fn car(&mut self, s: &CarSpec) {
        let c = Vec2::new(s.base.x, s.base.z);
        let (e, side) = if s.along_x { (Vec2::X, Vec2::Y) } else { (Vec2::Y, Vec2::X) };
        let y = s.base.y;
        let at = |u: f32, v: f32| c + e * u + side * v;
        // Wheels.
        for (u, v) in [(-1.5, -1.0), (-1.5, 0.5), (1.0, -1.0), (1.0, 0.5)] {
            self.aabb2(at(u, v), at(u + 0.5, v + 0.5), y, y + VS, mat::ROOF_DARK);
        }
        // Body.
        self.aabb2(at(-2.25, -1.0), at(2.25, 1.0), y + VS, y + 1.0, s.body);
        self.aabb2(at(2.0, -0.75), at(2.25, 0.75), y + VS, y + 1.0, mat::NEON_AMBER); // headlights
        if s.convertible {
            self.aabb2(at(-0.5, -1.0), at(0.0, 1.0), y + 1.0, y + 1.5, mat::GLASS_TEAL); // windshield
        } else {
            self.aabb2(at(-1.25, -0.75), at(1.0, 0.75), y + 1.0, y + 1.5, mat::GLASS_BLUE);
            self.aabb2(at(-1.25, -0.75), at(1.0, 0.75), y + 1.5, y + 2.0, s.body);
        }
    }

    /// A billboard on two steel legs with a catwalk, banded art and a neon frame.
    pub fn billboard(&mut self, s: &BillboardSpec) {
        let n = s.normal.normalize_or(Vec2::X);
        let side = Vec2::new(-n.y, n.x);
        let c = Vec2::new(s.base.x, s.base.z);
        let y0 = s.base.y;
        let py = y0 + s.leg_h;
        let hw = s.width * 0.5;
        // Legs (1 m square, from slightly below the base).
        for sg in [-1.0f32, 1.0] {
            let p = c + side * sg * (hw - 2.0);
            self.aabb2(p - n * 0.5 - side * 0.5, p + n * 0.5 + side * 0.5, y0 - VS, py, mat::STEEL);
        }
        // Catwalk (standable).
        self.aabb2(c - side * hw + n * 0.5, c + side * hw + n * 1.5, py - VS, py, mat::STEEL);
        // Panel with art bands and neon frame.
        let a = c - side * hw - n * 0.5;
        let b = c + side * hw + n * 0.5;
        let (lo, hi) = (a.min(b), a.max(b));
        let h = s.height;
        let bands = s.bands;
        let neon = s.neon;
        self.fill_with(Vec3::new(lo.x, py, lo.y), Vec3::new(hi.x, py + h, hi.y), |p, _, _| {
            let q = Vec2::new(p.x, p.z) - c;
            let across = q.dot(side);
            let up = p.y - py;
            if across.abs() > hw - VS || up < VS || up > h - VS {
                return Some(neon);
            }
            let bi = ((up / h) * 4.0).floor().clamp(0.0, 3.0) as usize;
            // A sun disc in the middle of the art.
            let sun = Vec2::new(across * 0.8, up - h * 0.45).length() < h * 0.28 && up > h * 0.3;
            Some(if sun { mat::NEON_AMBER } else { bands[bi] })
        });
    }

    /// A rooftop water tower: four legs, a banded cylindrical tank and a conical cap.
    pub fn water_tower(&mut self, base: Vec3, r: f32, tank_h: f32) {
        let c = Vec2::new(base.x, base.z);
        let leg_h = 3.0;
        for (dx, dz) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            let p = c + Vec2::new(dx, dz) * (r * 0.7);
            self.aabb2(p - 0.25, p + 0.25, base.y - VS, base.y + leg_h, mat::STEEL);
        }
        let y0 = base.y + leg_h;
        self.fill_with(Vec3::new(c.x - r - 0.5, y0, c.y - r - 0.5), Vec3::new(c.x + r + 0.5, y0 + tank_h + r, c.y + r + 0.5), |p, _, _| {
            let d = Vec2::new(p.x - c.x, p.z - c.y).length();
            let up = p.y - y0;
            if up < tank_h {
                if d > r {
                    return None;
                }
                let band = (up / 1.2).floor() as i32 % 2 == 0 && (up.rem_euclid(1.2) < VS);
                return Some(if band { mat::STEEL } else { mat::DARK_WOOD });
            }
            // Cone.
            (d <= r * (1.0 - (up - tank_h) / r)).then_some(mat::ROOF_DARK)
        });
    }

    /// A 6 × 2.5 × 2.5 m shipping container resting at `base` (min corner on the ground).
    pub fn container(&mut self, base: Vec3, along_x: bool, body: Material) {
        let (sx, sz) = if along_x { (6.0, 2.5) } else { (2.5, 6.0) };
        let (lo, hi) = (base, base + Vec3::new(sx, 2.5, sz));
        self.fill_with(lo, hi, |p, v, _| {
            let rib = if along_x { v.x } else { v.z }.rem_euclid(2) == 0;
            let top = p.y > hi.y - VS;
            let end = if along_x { p.x > hi.x - VS } else { p.z > hi.z - VS };
            Some(if end {
                mat::STEEL
            } else if rib && !top {
                body
            } else {
                shade(body)
            })
        });
    }

    /// A gantry crane straddling a quay: four legs on `rect` corners (x0..x1 across, z ± depth), a
    /// portal beam at `top_y`, and a boom reaching out along +X to `boom_x`.
    #[allow(clippy::too_many_arguments)]
    pub fn gantry_crane(&mut self, x0: f32, x1: f32, z: f32, ground_y: f32, top_y: f32, boom_x: f32, color: Material) {
        let dz = 4.0;
        for x in [x0, x1 - 1.0] {
            for zz in [z - dz, z + dz - 1.0] {
                self.fill(Vec3::new(x, ground_y - VS, zz), Vec3::new(x + 1.0, top_y, zz + 1.0), color);
            }
            // Side bracing (diagonals) between the legs of each frame.
            self.line(Vec3::new(x + 0.5, ground_y + 3.0, z - dz + 0.5), Vec3::new(x + 0.5, top_y - 2.0, z + dz - 0.5), mat::STEEL, true);
            self.line(Vec3::new(x + 0.5, ground_y + 3.0, z + dz - 0.5), Vec3::new(x + 0.5, top_y - 2.0, z - dz + 0.5), mat::STEEL, true);
        }
        // Sill beams (the portal) and the boom girder.
        for zz in [z - dz, z + dz - 1.0] {
            self.fill(Vec3::new(x0 - 6.0, top_y, zz), Vec3::new(boom_x, top_y + 1.5, zz + 1.0), color);
        }
        // Cross ties + a walkway deck on the boom.
        let mut x = x0 - 6.0;
        while x < boom_x {
            self.fill(Vec3::new(x, top_y + 1.0, z - dz), Vec3::new(x + 1.0, top_y + 1.5, z + dz), mat::STEEL);
            x += 5.0;
        }
        // Machinery house + A-frame.
        self.fill(Vec3::new(x0 - 5.0, top_y + 1.5, z - 3.0), Vec3::new(x0 + 1.0, top_y + 5.0, z + 3.0), mat::HULL_WHITE);
        self.fill(Vec3::new(x0 - 5.0, top_y + 5.0, z - 3.0), Vec3::new(x0 + 1.0, top_y + 5.5, z + 3.0), color);
        let apex = Vec3::new(x1, top_y + 10.0, z);
        for zz in [z - dz + 0.5, z + dz - 0.5] {
            self.line(Vec3::new(x1 - 2.0, top_y + 1.5, zz), apex, mat::STEEL, true);
            self.line(apex, Vec3::new(boom_x - 1.0, top_y + 1.5, zz), mat::STEEL, true);
        }
        self.fill(apex - Vec3::new(0.5, 0.5, dz), apex + Vec3::new(0.5, 0.5, dz), color);
        // Trolley + hook block hanging below the boom (a low anchor over the ship).
        let tx = (x1 + boom_x) * 0.5 + 2.0;
        self.fill(Vec3::new(tx - 1.5, top_y - 1.0, z - dz), Vec3::new(tx + 1.5, top_y, z + dz), mat::PAINT_YELLOW);
    }

    /// Street light: pole with an arm reaching along `arm` (unit) and an amber lamp head.
    pub fn street_light(&mut self, base: Vec3, arm: Vec2, h: f32) {
        let c = Vec2::new(base.x, base.z);
        self.aabb2(c - 0.25, c + 0.25, base.y - VS, base.y + h, mat::STEEL);
        let tip = c + arm * 2.0;
        self.aabb2(c.min(tip) - 0.25, c.max(tip) + 0.25, base.y + h - VS, base.y + h, mat::STEEL);
        self.aabb2(tip - 0.5, tip + 0.5, base.y + h - 1.0, base.y + h - VS, mat::NEON_AMBER);
    }
}

/// A darker companion colour for ribs/shading within one prop (never emissive).
pub fn shade(m: Material) -> Material {
    match m {
        mat::PAINT_RED => mat::TERRACOTTA,
        mat::HULL_WHITE => mat::CONCRETE,
        mat::STUCCO_WHITE => mat::CONCRETE,
        mat::PAINT_YELLOW => mat::STUCCO_LEMON,
        mat::STUCCO_SKY => mat::STUCCO_WHITE,
        other => other,
    }
}
