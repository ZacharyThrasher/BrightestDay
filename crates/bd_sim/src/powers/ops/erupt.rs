//! `MoveOp::Erupt` (docs/toolbox/OPS.md): terrain RISES.
//!
//! The ground itself grows, in one of four shapes (`shape`):
//! * `Line` — along your aim from your feet: `count` spikes (a row of stone spears) or, `count: 0`,
//!   one continuous ridge (a glacier racing forward, a bridge of ice over a gap);
//! * `Wall` — across your aim `dist` m ahead (a wall thrown up in front of you);
//! * `Pillar` — one column under the crosshair (`at: Aim`) or under YOU (`at: Me`, the earth
//!   elevator): whoever stands on it rides it up and is launched off the top — you included;
//! * `Ring` — a prison wall round the aimed point (`length` = its diameter).
//!
//! Material: Ice / Water → ice, Fire / Shadow → magma rock, Earth / Light → whatever the ground under
//! each column is made of (nothing rises from thin air), the other elements their crystal. Enemies
//! on the line / ring / pillar are hit (`damage`) and launched upward (`launch`). Voxels never grow
//! inside a player and are replicated exactly like Crazy Diamond's restore (`PowerEvent::Restored`
//! carries the voxel list), so clients and replays match. Deterministic; one voxel list per cast.

use bd_core::mechanics::Element;
use bd_core::voxel::{Aabb, Material, voxel_center, world_to_voxel};
use bd_core::{IVec3, MAX_PLAYERS, VOXEL_SIZE, Vec3};

use super::super::{At, EruptShape, MAX_RESTORE_VOXELS, PowerError, PowerEvent};
use crate::combat::{HitInfo, HitKind};
use crate::events::GameEvent;
use crate::geom;
use crate::world::World;

/// An eruption's numbers (`MoveOp::Erupt`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct EruptOp {
    pub length: f32,
    pub width: f32,
    pub height: f32,
    pub count: u8,
    pub element: Element,
    pub damage: f32,
    pub launch: f32,
    pub shape: EruptShape,
    pub dist: f32,
    pub at: At,
}

/// The voxel material an element's eruption is made of (`None` = the ground's own material).
pub fn erupt_material(e: Element) -> Option<Material> {
    use bd_voxel::mat;
    Some(match e {
        Element::Ice | Element::Water => mat::ICE,
        Element::Fire | Element::Shadow => mat::BASALT,
        Element::Electric => mat::CRYSTAL_CYAN,
        Element::Psychic => mat::CRYSTAL_VIOLET,
        Element::Poison => mat::JADE,
        Element::Wind => mat::SAND,
        Element::Atomic => mat::CRYSTAL_GOLD,
        Element::Earth | Element::Light => return None,
    })
}

/// One footprint sample: a column centre, its radius, its full height, and whether it tapers to a
/// point (a spike) or keeps a rounded top (a ridge / wall / pillar).
#[derive(Clone, Copy)]
struct Column {
    c: Vec3,
    r: f32,
    h: f32,
    spike: bool,
    /// Ridge columns only fill across `perp` (the next sample covers the rest).
    perp: Option<Vec3>,
}

/// Most footprint samples one eruption walks.
const MAX_SAMPLES: usize = 160;

impl World {
    pub(crate) fn op_erupt(&mut self, i: usize, op: EruptOp) -> Result<(), PowerError> {
        let (shape, at) = (op.shape, op.at);
        let id = self.players[i].id;
        let (_, look) = self.op_eye_look(i);
        let base = self.cfg.move_params.clone();
        let feet = self.players[i].mv.pos;
        let fwd = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
        let side = Vec3::Y.cross(fwd).normalize_or(Vec3::X);
        let length = super::fclamp(op.length, 1.0, 36.0);
        let width = super::fclamp(op.width, VOXEL_SIZE, 6.0);
        let height = super::fclamp(op.height, VOXEL_SIZE, 12.0);
        let half_w = width * 0.5;
        // The centre of a pillar / ring: the ground under the crosshair, or under you.
        let centre = match at {
            At::Aim => {
                let p = self.op_at_point(i, At::Aim, 0.0);
                bd_core::voxel::raycast(&self.voxels, p + Vec3::Y * 1.0, Vec3::NEG_Y, 8.0).map_or(p, |h| h.point)
            }
            At::Front => feet + fwd * super::fclamp(op.dist, 1.0, 20.0),
            At::Me => feet,
        };
        let mut cols = [None::<Column>; MAX_SAMPLES];
        let mut n = 0usize;
        let mut push = |c: Column| {
            if n < MAX_SAMPLES {
                cols[n] = Some(c);
                n += 1;
            }
        };
        let spikes = op.count.min(24) as usize;
        // Segment shapes (line / wall).
        let seg = match shape {
            EruptShape::Line => {
                let a = feet + fwd * 1.5;
                Some((a, a + fwd * length))
            }
            EruptShape::Wall => {
                let c = feet + fwd * super::fclamp(op.dist, 1.0, 20.0);
                Some((c - side * length * 0.5, c + side * length * 0.5))
            }
            _ => None,
        };
        match (shape, seg) {
            (EruptShape::Line | EruptShape::Wall, Some((a, b))) => {
                let dir = (b - a).normalize_or(fwd);
                let perp = Vec3::Y.cross(dir).normalize_or(side);
                if spikes > 0 {
                    for s in 0..spikes {
                        let t = (s as f32 + 0.5) / spikes as f32;
                        push(Column { c: a + (b - a) * t, r: half_w, h: height, spike: true, perp: None });
                    }
                } else {
                    let steps = ((length / VOXEL_SIZE).ceil() as usize).clamp(2, MAX_SAMPLES);
                    for s in 0..steps {
                        let t = s as f32 / (steps - 1) as f32;
                        push(Column { c: a + (b - a) * t, r: half_w, h: height, spike: false, perp: Some(perp) });
                    }
                }
            }
            (EruptShape::Pillar, _) => push(Column { c: centre, r: half_w.max(VOXEL_SIZE), h: height, spike: false, perp: None }),
            (EruptShape::Ring, _) => {
                let rr = (length * 0.5).clamp(1.5, 12.0);
                let steps = ((std::f32::consts::TAU * rr / VOXEL_SIZE).ceil() as usize).clamp(8, MAX_SAMPLES);
                for s in 0..steps {
                    let a = s as f32 / steps as f32 * std::f32::consts::TAU;
                    let radial = Vec3::new(a.cos(), 0.0, a.sin());
                    push(Column { c: centre + radial * rr, r: half_w, h: height, spike: false, perp: Some(radial) });
                }
            }
            _ => {}
        }
        // Riders: players standing on a pillar go up with it (onto its top) — before any voxel is
        // placed, so the column grows under them instead of around them.
        let ref_y = match shape {
            EruptShape::Pillar | EruptShape::Ring => centre.y,
            _ => feet.y,
        };
        let floor = world_to_voxel(Vec3::new(feet.x, ref_y, feet.z)).y - 1;
        let fixed = erupt_material(op.element);
        let mut riders = [None::<(usize, f32)>; MAX_PLAYERS];
        let mut nr = 0;
        if shape == EruptShape::Pillar {
            let top_ground = bd_core::voxel::raycast(&self.voxels, centre + Vec3::Y * 3.0, Vec3::NEG_Y, 10.0).map_or(floor, |h| h.voxel.y);
            let top = voxel_center(IVec3::new(0, top_ground, 0)).y + VOXEL_SIZE * 0.5 + (height / VOXEL_SIZE).round().max(1.0) * VOXEL_SIZE;
            for (k, q) in self.players.iter().enumerate() {
                if !q.alive || nr >= riders.len() {
                    continue;
                }
                let d = Vec3::new(q.mv.pos.x - centre.x, 0.0, q.mv.pos.z - centre.z).length();
                if d <= half_w + 0.45 && (q.mv.pos.y - centre.y).abs() <= 1.6 {
                    riders[nr] = Some((k, top));
                    nr += 1;
                }
            }
            for (k, top) in riders.iter().flatten().copied() {
                let p = &mut self.players[k];
                p.mv.pos.y = p.mv.pos.y.max(top + 0.05);
                p.mv.grounded = false;
            }
        }
        let mut hulls = [None::<Aabb>; MAX_PLAYERS];
        for (k, q) in self.players.iter().enumerate().take(MAX_PLAYERS) {
            if q.alive {
                hulls[k] = Some(geom::inflate(&q.hull(&base), 0.1));
            }
        }
        let mut list: Vec<(IVec3, u8)> = Vec::new();
        let cells = ((width / VOXEL_SIZE).ceil() as i32).max(1);
        'outer: for col in cols.iter().flatten().copied() {
            for dx in -cells..=cells {
                for dz in -cells..=cells {
                    let off = match col.perp {
                        // A ridge: only across the line (the next sample covers along it).
                        Some(perp) => {
                            if dz != 0 {
                                continue;
                            }
                            perp * (dx as f32 * VOXEL_SIZE)
                        }
                        None => Vec3::new(dx as f32, 0.0, dz as f32) * VOXEL_SIZE,
                    };
                    let r = Vec3::new(off.x, 0.0, off.z).length();
                    if r > col.r + 1e-3 {
                        continue;
                    }
                    let p = col.c + off;
                    // The ground under this column (a line follows the terrain; over a gap it bridges
                    // from your own footing).
                    let hit = bd_core::voxel::raycast(&self.voxels, Vec3::new(p.x, ref_y + 3.0, p.z), Vec3::NEG_Y, 10.0);
                    let ground = hit.map_or(floor, |h| h.voxel.y);
                    let mat = fixed.or_else(|| hit.map(|h| self.voxels.get(h.voxel)).filter(|m| *m != 0)).unwrap_or(bd_voxel::mat::STONE);
                    // Taper: spikes narrow to a point, the rest round their top edge.
                    let frac = if col.r > 0.0 { r / col.r } else { 0.0 };
                    let h = if col.spike { col.h * (1.0 - frac * 0.85) } else { col.h * (1.0 - frac * frac * 0.3) };
                    let rows = ((h / VOXEL_SIZE).round() as i32).max(1);
                    let cv = world_to_voxel(p);
                    for y in 1..=rows {
                        let v = IVec3::new(cv.x, ground + y, cv.z);
                        if !self.voxels.in_bounds(v) || self.voxels.get(v) != 0 {
                            continue;
                        }
                        let vc = voxel_center(v);
                        let halfv = Vec3::splat(VOXEL_SIZE * 0.5);
                        let vb = Aabb { min: vc - halfv, max: vc + halfv };
                        if hulls.iter().flatten().any(|h| h.intersects(&vb)) {
                            continue;
                        }
                        if list.len() >= MAX_RESTORE_VOXELS {
                            break 'outer;
                        }
                        // Set now: later columns see it and never duplicate it.
                        self.voxels.set(v, mat);
                        list.push((v, mat));
                    }
                }
            }
        }
        // Who gets launched: enemies on the line / wall, on the ring's rim, or riding the pillar
        // (a rider who is YOU is launched too — the earth elevator — but never hit).
        let mut victims = [None::<usize>; MAX_PLAYERS];
        let mut m = 0;
        for (k, q) in self.players.iter().enumerate() {
            if !q.alive || m >= victims.len() {
                continue;
            }
            let p = q.mv.pos;
            let on = match (shape, seg) {
                (EruptShape::Line | EruptShape::Wall, Some((a, b))) => {
                    let s = b - a;
                    let t = ((p - a).dot(s) / s.length_squared().max(1e-6)).clamp(0.0, 1.0);
                    let o = a + s * t;
                    Vec3::new(p.x - o.x, 0.0, p.z - o.z).length() <= half_w + 0.7 && (p.y - o.y).abs() <= height + 2.0
                }
                (EruptShape::Ring, _) => {
                    let d = Vec3::new(p.x - centre.x, 0.0, p.z - centre.z).length();
                    (d - (length * 0.5).clamp(1.5, 12.0)).abs() <= half_w + 0.7 && (p.y - centre.y).abs() <= height + 2.0
                }
                (EruptShape::Pillar, _) => riders.iter().flatten().any(|(r, _)| *r == k),
                _ => false,
            };
            if on && (k != i || shape == EruptShape::Pillar) {
                victims[m] = Some(k);
                m += 1;
            }
        }
        if list.is_empty() && m == 0 {
            return Err(PowerError::NoTarget);
        }
        let launch = super::fclamp(op.launch, 0.0, 40.0);
        for k in victims.iter().flatten().copied() {
            if k == i {
                // The elevator throws you up off the top.
                let mv = &mut self.players[i].mv;
                mv.vel.y = mv.vel.y.max(launch);
                mv.grounded = false;
                continue;
            }
            let tid = self.players[k].id;
            let c = self.players[k].hull(&base).center();
            let h = HitInfo::new(tid, Some(id), HitKind::Crush, super::fclamp(op.damage, 0.0, 40.0), launch, Vec3::Y + fwd * 0.2, c)
                .with_element(op.element)
                .with_impact(1.2);
            self.apply_hit(h);
        }
        let (center, radius) = match (shape, seg) {
            (EruptShape::Line | EruptShape::Wall, Some((a, b))) => ((a + b) * 0.5 + Vec3::Y * height * 0.5, (length * 0.5).max(height)),
            _ => (centre + Vec3::Y * height * 0.5, (length * 0.5).max(height)),
        };
        self.inject_event(GameEvent::Power(PowerEvent::Restored { player: id, center, radius, voxels: list, constructs: 0 }));
        Ok(())
    }
}
