//! `MoveOp::Rush` — the barrage as a move (docs/toolbox/OPS.md): ORA ORA / MUDA MUDA without typing.
//!
//! Your companion (a stand, a clone at your side) steps out in front of you and throws `hits`
//! punches at `rate` per second, each a small front-cone hit (`damage`, `knockback`, `range`) along
//! your CURRENT look (steer the flurry). No companion: `stand` (a recipe) is summoned for the length
//! of the rush — the stand appears, rushes, and fades — or, with no `stand` either, the punches come
//! from your own fists. The last punch is the send-off (×3 knockback). Being stunned ends it.
//! Deterministic, allocation-free per tick (the temporary stand is one spawn at the start).

use bd_core::mechanics::Element;
use bd_core::{ConstructId, Vec3};

use super::super::{PowerError, PowerEvent, RecipeDef};
use super::live::{RushState, ToolboxFx};
use crate::combat::{HitInfo, HitKind};
use crate::constructs::{SpawnParams, forming_secs};
use crate::events::GameEvent;
use crate::willpower::validate_recipe;
use crate::world::World;

impl World {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn op_rush(&mut self, i: usize, hits: u8, rate: f32, damage: f32, knockback: f32, range: f32, stand: Option<&RecipeDef>) -> Result<(), PowerError> {
        let hits = hits.clamp(1, 40);
        // 0 = defaults: the stand-rush rate, a stand's reach.
        let rate = if rate > 0.0 { rate } else { 16.0 };
        let range = if range > 0.0 { range } else { 2.8 };
        let every = 1.0 / rate.clamp(2.0, 30.0);
        let id = self.players[i].id;
        // A temporary stand when there is no companion out.
        let mut temp = None;
        if self.players[i].power.companion.is_none() {
            if let Some(r) = stand {
                temp = self.rush_spawn_stand(i, r, every * hits as f32 + 0.6);
            }
        }
        if let Some(old) = self.players[i].power.live.rush.and_then(|r| r.stand) {
            self.toolbox_dismiss(old);
        }
        self.players[i].power.live.rush = Some(RushState {
            left: hits,
            every,
            next: 0.0,
            damage: damage.clamp(0.0, 20.0),
            knockback: knockback.clamp(0.0, 30.0),
            range: range.clamp(1.0, 6.0),
            stand: temp,
            n: 0,
        });
        let at = self.players[i].hull(&self.cfg.move_params).center();
        self.toolbox_fx(id, ToolboxFx::Rush, at, at, every * hits as f32, hits as f32);
        self.step_rush(i, 0.0);
        Ok(())
    }

    fn rush_spawn_stand(&mut self, i: usize, r: &RecipeDef, life: f32) -> Option<ConstructId> {
        if self.constructs.len() >= crate::world::MAX_CONSTRUCTS - 8 {
            return None;
        }
        let p = &self.players[i];
        let owner = p.id;
        let look = p.look();
        let fwd = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::Z);
        let pos = p.mv.pos + fwd * 1.0 + Vec3::Y * 0.2;
        let mut rec = r.to_recipe();
        rec.temperament = bd_core::mechanics::Temperament::Protective;
        rec.intensity = rec.intensity.max(0.7);
        let recipe = validate_recipe(rec, &self.cfg.rules);
        let rot = crate::geom::yaw_rotation(fwd).inverse();
        let forming = forming_secs(recipe.form).min(0.15);
        let tick = self.tick;
        let cid = self.constructs.spawn(SpawnParams {
            owner,
            recipe: recipe.clone(),
            shape: None,
            pos,
            rot,
            vel: Vec3::ZERO,
            target: None,
            cost: 0.0,
            integrity: super::super::STAND_INTEGRITY,
            forming,
            life,
            tick,
            colossal: false,
            model: None,
        });
        if let Some(c) = self.constructs.get_mut(cid) {
            // Parented to itself: never targeted by swaps / yanks, like a stand.
            c.parent = Some(cid);
        }
        self.inject_event(GameEvent::ConstructSpawned { id: cid, owner, recipe: Box::new(recipe), pos, rot, forming, cost: 0.0 });
        Some(cid)
    }

    pub(crate) fn step_rush(&mut self, i: usize, dt: f32) {
        let Some(mut r) = self.players[i].power.live.rush else { return };
        if self.players[i].status.stunned() || r.left == 0 {
            if let Some(s) = r.stand {
                self.toolbox_dismiss(s);
            }
            self.players[i].power.live.rush = None;
            return;
        }
        r.next -= dt;
        if r.next > 0.0 {
            self.players[i].power.live.rush = Some(r);
            return;
        }
        r.next += r.every;
        r.left -= 1;
        r.n = r.n.wrapping_add(1);
        let last = r.left == 0;
        let base = self.cfg.move_params.clone();
        let p = &self.players[i];
        let id = p.id;
        let look = p.look();
        let fwd = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::Z);
        let feet = p.mv.pos;
        let eye = p.eye(&base);
        // The stand (companion, else the temporary one) steps out front.
        let stand = p.power.companion.or(r.stand);
        let mut origin = eye;
        if let Some(c) = stand.and_then(|s| self.constructs.get_mut(s)) {
            c.pos = feet + fwd * 1.0 + Vec3::Y * 0.2;
            c.prev_pos = c.pos;
            origin = c.pos + Vec3::Y * c.half.y;
        }
        let side = Vec3::Y.cross(fwd).normalize_or(Vec3::X) * if r.n % 2 == 0 { 0.25 } else { -0.25 };
        let origin = origin + side;
        let kb = if last { r.knockback * 3.0 } else { r.knockback };
        let cos = 55f32.to_radians().cos();
        let mut hits = 0u8;
        for k in 0..self.players.len() {
            let q = &self.players[k];
            if k == i || !q.alive {
                continue;
            }
            let c = q.hull(&base).center();
            let d = c - origin;
            let len = d.length();
            if len > r.range + 0.5 || (len > 0.6 && d.dot(look) / len < cos) {
                continue;
            }
            let tid = q.id;
            let mut hit = HitInfo::new(tid, Some(id), HitKind::Creature, r.damage, kb, look + Vec3::Y * 0.2, c).with_element(Element::Light).with_impact(if last { 1.5 } else { 0.7 });
            hit.construct = stand;
            if self.apply_hit(hit).applied {
                hits = hits.saturating_add(1);
            }
        }
        let point = origin + look * r.range * 0.6;
        self.inject_event(GameEvent::Power(PowerEvent::StandRush { player: id, stand: stand.unwrap_or(ConstructId(0)), point, hits }));
        self.players[i].power.rush_t = 0.35;
        if last {
            if let Some(s) = r.stand {
                self.toolbox_dismiss(s);
            }
            self.players[i].power.live.rush = None;
        } else {
            self.players[i].power.live.rush = Some(r);
        }
    }
}
