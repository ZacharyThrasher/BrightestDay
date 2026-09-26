//! Punch-through (COMBAT.md §1a): a launched, tumbling player hitting destructible terrain faster
//! than `punch_threshold(hardness)` plows *through* it. Before movement each tick the world probes
//! the path ahead; if it is solid and the player is fast enough, a capsule is carved along it (a
//! replicated `CarveOp`, counted against the per-tick carve budget) so the swept movement passes,
//! and the player bleeds speed per metre of solid by hardness (leaves / glass ≪ wood ≪ stone).
//! Too slow (or out of carve budget) → the ordinary collision stops them → wall splat. Indestructible
//! voxels are never carved: they always stop you. Constructs in the path take integrity damage and
//! slow the player by their mass; one that shatters stops blocking immediately.
//! `PunchThrough { entry, exit }` fires when the player bursts back out into air.

use bd_core::voxel::world_to_voxel;
use bd_core::Vec3;
use bd_voxel::{CarveOp, CarveShape, material_def};

use super::World;
use crate::combat::ConstructHit;
use crate::events::GameEvent;
use crate::smash::{PunchState, punch_threshold};

/// Path sampling step (m).
const STEP: f32 = 0.25;

impl World {
    pub(crate) fn punch_through(&mut self, dt: f32) {
        let t = self.cfg.tuning.smash.clone();
        let base = self.cfg.move_params.clone();
        let mut ops = 0u16;
        for i in 0..self.players.len() {
            let p = &self.players[i];
            let punching = p.punch;
            if !p.alive || p.seat.is_some() || (p.launch.is_none() && punching.is_none()) {
                continue;
            }
            let launched = p.launch.is_some() && p.mv.tumble > 0.0;
            let id = p.id;
            let v = p.mv.vel;
            let speed = v.length();
            if !(speed > 1e-3) || !v.is_finite() {
                continue;
            }
            let hull = p.hull(&base);
            let c0 = hull.center();
            let half = hull.half();
            let dir = v / speed;
            let reach = speed * dt + half.max_element() + 0.3;
            // Tunnel already carved ahead of us (centreline distance).
            let already = punching.map_or(f32::NEG_INFINITY, |ps| (ps.carved_to - c0).dot(dir));
            let radius = t.punch_radius.max(half.length() + 0.1);
            // Probe the hull's cross-section along the path (centre + 4 offsets, so off-axis rock
            // that would catch the hull's corners counts): first material, indestructible stop.
            // Hardness-weighted metres of *new* solid are billed along the centreline, out to where
            // the capsule will reach (each metre once).
            // Off-axis samples only once inside a tunnel: starting a punch needs rock dead ahead
            // (so skimming a floor never trenches it).
            let (u, w2) = dir.any_orthonormal_pair();
            let lanes = if punching.is_some() { 5 } else { 1 };
            let off = [Vec3::ZERO, u * half.x * 0.9, -u * half.x * 0.9, w2 * half.y * 0.9, -w2 * half.y * 0.9];
            let mut new_m = 0.0f32;
            let mut first: Option<(Vec3, u8, f32)> = None;
            let mut blocked: Option<f32> = None;
            let n = (reach / STEP).ceil() as i32;
            'probe: for k in 0..=n {
                let s = k as f32 * STEP;
                for (j, o) in off.iter().enumerate().take(lanes) {
                    let at = c0 + dir * s + *o;
                    let m = self.voxels.get(world_to_voxel(at));
                    if m == 0 {
                        continue;
                    }
                    let def = material_def(m);
                    if def.indestructible {
                        blocked = Some(s);
                        break 'probe;
                    }
                    let h = def.hardness.max(t.punch_min_hardness);
                    if j == 0 && s > already {
                        new_m += h * STEP;
                    }
                    if first.is_none() {
                        first = Some((c0 + dir * s, m, h));
                    }
                }
            }
            // Centreline metres in the capsule's end cap (carved now, billed now).
            let cap_n = (radius / STEP).ceil() as i32;
            for k in 1..=cap_n {
                let s = reach + k as f32 * STEP;
                if blocked.is_some() || s <= already {
                    continue;
                }
                let m = self.voxels.get(world_to_voxel(c0 + dir * s));
                if m != 0 && !material_def(m).indestructible {
                    new_m += material_def(m).hardness.max(t.punch_min_hardness) * STEP;
                }
            }
            let Some((fpos, fmat, fh)) = first else {
                // Open air ahead: out once past the far end of the tunnel we carved.
                if let Some(ps) = punching.filter(|ps| (c0 - ps.carved_to).dot(dir) >= 0.0) {
                    self.set_punch(i, None);
                    self.events.push(GameEvent::PunchThrough { player: id, entry: ps.entry, exit: c0 + dir * half.max_element(), material: ps.material });
                }
                continue;
            };
            if !launched || speed < punch_threshold(&t, fh) || ops >= t.punch_max_carves_per_tick {
                // Too slow (or out of budget): the rock wins — a splat right here, as normal.
                let stalled = punching.is_some() && launched && speed >= self.cfg.tuning.hit.splat_min_speed;
                self.set_punch(i, None);
                if stalled {
                    if let Some(p) = self.players.get_mut(i) {
                        p.mv.vel = Vec3::ZERO;
                    }
                    self.splat_player(i, crate::movement::WallHit { speed, normal: -dir });
                }
                continue;
            }
            let end = match blocked {
                Some(b) => (b - 0.3).max(0.0),
                None => reach,
            };
            if end <= 0.05 {
                self.set_punch(i, None);
                continue;
            }
            let seed = self.carve_seed;
            self.carve_seed = self.carve_seed.wrapping_add(1).max(1);
            let op = CarveOp { shape: CarveShape::Capsule { a: c0, b: c0 + dir * end, radius }, power: t.punch_carve_power, seed };
            let _ = super::apply_carve_op(&mut self.voxels, &op);
            self.events.push(GameEvent::Carve { op });
            ops += 1;
            self.carve_ops_used += 1;
            // Bleed speed by the new material plowed through (hardness-weighted metres).
            let mut new_speed = (speed - new_m * t.punch_loss_per_m_per_hardness).max(0.0);
            // Constructs in the way take the hit and slow the player by their mass.
            if let Some((cid, _, _)) = self.constructs.segment_query(c0, c0 + dir * reach, 0.5, None) {
                let mass = self.constructs.get(cid).map_or(0.0, |c| c.mass());
                let shattered = self.apply_construct_hit(ConstructHit {
                    pierce: false,
                    blunt: true,
                    target: cid,
                    source: Some(id),
                    damage: speed * 0.5,
                    impulse: dir * speed,
                    blade: false,
                    point: c0 + dir * half.max_element(),
                });
                new_speed = (new_speed - mass * t.punch_construct_loss_per_kg).max(0.0);
                if shattered {
                    // It no longer blocks this tick's movement.
                    self.colliders.retain(|c| !matches!(c.id, crate::movement::ColliderId::Construct { id, .. } if id == cid));
                }
            }
            if let Some(p) = self.players.get_mut(i) {
                p.mv.vel = dir * new_speed;
            }
            let carved_to = c0 + dir * (end + radius);
            let ps = match punching {
                Some(ps) => PunchState { carved_to, ..ps },
                None => PunchState { entry: fpos, material: fmat, carved_to },
            };
            self.set_punch(i, Some(ps));
        }
    }

    fn set_punch(&mut self, i: usize, ps: Option<PunchState>) {
        if let Some(p) = self.players.get_mut(i) {
            p.punch = ps;
        }
    }
}
