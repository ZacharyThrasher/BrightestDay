//! `MoveOp::Wave` — a travelling slash / shockwave (docs/toolbox/OPS.md). Getsuga Tensho's
//! crescent, a flying sword slash, a spinning cutting disc, a quake crack racing along the ground.
//!
//! A crescent `width` m wide leaves your chest along the look at `speed` m/s and flies `length` m.
//! Every enemy it passes (within ~1 m of its edge line) is struck ONCE (`damage`, `knockback` along
//! its flight + `up`, `element`) — it passes through players and keeps going. `carve` > 0 = it cuts
//! terrain as it flies (a trench `carve` m in radius every ~1.2 m, across its width) and nothing
//! stops it; `carve` 0 = it breaks on the first wall. `thin`: a razor edge — constructs in its path
//! are cut (Blade damage) instead of merely bumped. Visible and dodge-able (a sidestep / jump / dodge
//! clears it), fully determined by its start event for clients.
//! Deterministic, allocation-free per tick (≤ [`MAX_WAVES`] per player).

use bd_core::Vec3;
use bd_core::mechanics::Element;

use super::super::PowerError;
use super::live::{MAX_WAVES, ToolboxFx, WaveLive};
use crate::combat::{CarveRequest, ConstructHit, HitInfo, HitKind};
use crate::world::World;

/// Sub-step length (m) so a fast wave never skips a player.
const STEP: f32 = 0.5;
/// Metres between carves.
const CARVE_EVERY: f32 = 1.2;
/// How close to the edge line counts as touched (m, plus the hull).
const TOUCH: f32 = 0.6;

impl World {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn op_wave(&mut self, i: usize, length: f32, speed: f32, width: f32, carve: f32, damage: f32, knockback: f32, up: f32, element: Element, thin: bool) -> Result<(), PowerError> {
        let (eye, look) = self.toolbox_eye_look(i);
        let id = self.players[i].id;
        let dir = look.normalize_or(Vec3::Z);
        let start = eye - Vec3::Y * 0.3 + dir * 0.8;
        let w = WaveLive {
            pos: start,
            dir,
            left: length.clamp(2.0, 60.0),
            speed: speed.clamp(5.0, 80.0),
            width: width.clamp(0.4, 12.0),
            carve: carve.clamp(0.0, 3.0),
            since_carve: CARVE_EVERY,
            damage: damage.clamp(0.0, 60.0),
            knockback: knockback.clamp(0.0, 70.0),
            up: up.clamp(0.0, 2.0),
            element,
            thin,
            hit: 0,
            last_c: None,
        };
        let slot = {
            let waves = &mut self.players[i].power.live.waves;
            // A free slot, else replace the one with the least flight left.
            match waves.iter().position(|w| w.is_none()) {
                Some(s) => s,
                None => (0..MAX_WAVES).min_by(|&a, &b| waves[a].map_or(0.0, |w| w.left).total_cmp(&waves[b].map_or(0.0, |w| w.left))).unwrap_or(0),
            }
        };
        self.players[i].power.live.waves[slot] = Some(w);
        let far = start + dir * w.left;
        self.toolbox_fx(id, ToolboxFx::Wave, start, far, w.left / w.speed, w.width);
        Ok(())
    }

    pub(crate) fn step_waves(&mut self, i: usize, dt: f32) {
        for s in 0..MAX_WAVES {
            if let Some(w) = self.players[i].power.live.waves[s] {
                let w = self.step_wave(i, w, dt);
                self.players[i].power.live.waves[s] = w;
            }
        }
    }

    fn step_wave(&mut self, i: usize, mut w: WaveLive, dt: f32) -> Option<WaveLive> {
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        // The crescent's span: level, across the flight direction.
        let side = Vec3::Y.cross(w.dir).normalize_or(Vec3::X);
        let half = w.width * 0.5;
        let mut travel = (w.speed * dt).min(w.left);
        while travel > 1e-4 {
            let step = travel.min(STEP);
            let next = w.pos + w.dir * step;
            // Terrain: carve through, or break on it.
            if w.carve > 0.0 {
                w.since_carve += step;
                if w.since_carve >= CARVE_EVERY {
                    w.since_carve = 0.0;
                    let n = ((w.width / (w.carve * 2.0)).ceil() as i32).clamp(1, 5);
                    for k in 0..n {
                        let f = if n == 1 { 0.0 } else { k as f32 / (n - 1) as f32 * 2.0 - 1.0 };
                        self.request_carve(CarveRequest { center: next + side * (f * (half - w.carve * 0.5).max(0.0)), radius: w.carve, power: 1.5 });
                    }
                }
            } else if self.toolbox_blocked(i, w.pos, next) {
                return None;
            }
            // Players along the edge line.
            let (a, b) = (next - side * half, next + side * half);
            for k in 0..self.players.len() {
                if k == i || k >= 32 || w.hit & (1 << k) != 0 || !self.players[k].alive {
                    continue;
                }
                let hull = crate::geom::inflate(&self.players[k].hull(&base), TOUCH);
                if crate::geom::segment_aabb(a, b, &hull).is_none() {
                    continue;
                }
                w.hit |= 1 << k;
                let tid = self.players[k].id;
                let c = self.players[k].hull(&base).center();
                let hit = HitInfo::new(tid, Some(id), HitKind::Shot, w.damage, w.knockback, w.dir + Vec3::Y * w.up, c).with_element(w.element).with_impact(if w.damage > 12.0 { 1.4 } else { 0.9 });
                self.apply_hit(hit);
            }
            // Constructs it passes (not yours).
            if let Some((cid, _, point)) = self.constructs.segment_query(a, b, 0.4, Some(id)) {
                if w.last_c != Some(cid) && self.constructs.get(cid).is_some_and(|c| c.owner != id && !c.attached()) {
                    w.last_c = Some(cid);
                    self.apply_construct_hit(ConstructHit { target: cid, source: Some(id), damage: w.damage, impulse: w.dir * w.knockback, blade: w.thin, pierce: w.thin, blunt: !w.thin, point });
                }
            }
            w.pos = next;
            w.left -= step;
            travel -= step;
            if !self.in_play(w.pos, 2.0) {
                return None;
            }
        }
        (w.left > 1e-3).then_some(w)
    }
}
