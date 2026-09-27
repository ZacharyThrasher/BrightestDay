//! `MoveOp::Drill` — a hand-held orb (or lightning) lunge (docs/toolbox/OPS.md).
//!
//! You dash along the look for `secs` at `speed` with the thing held out in your palm. The first
//! enemy your palm meets is LATCHED: you both stop, they are ground against it for `grind` s
//! (ticking damage, no knockback — damage over time so the release isn't juggle-decayed), then the
//! release blasts them away along your look with `knockback` + `up` (the spin-away). `grind: 0` is a
//! single piercing strike on contact (a lightning blade through the chest). Terrain met mid-lunge is
//! bored out (a crater at the palm) and ends it; a whiff just ends. Escapable: a dodge during the
//! grind (the tick hit doesn't land) frees the victim; stunning the driller ends it.
//! Deterministic, allocation-free; nothing happens unless a move uses it.

use bd_core::Vec3;
use bd_core::mechanics::Element;

use super::super::PowerError;
use super::live::{DrillState, ToolboxFx};
use crate::combat::{CarveRequest, HitInfo, HitKind};
use crate::world::World;

/// Grind ticks every this many seconds.
const GRIND_TICK: f32 = 0.1;
/// Share of `damage` dealt over the grind (the rest lands on release).
const GRIND_SHARE: f32 = 0.4;
/// How far in front of your centre the palm reaches (m).
const REACH: f32 = 1.3;
/// Longest grind (escapable, never a lock).
const MAX_GRIND: f32 = 0.9;

impl World {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn op_drill(&mut self, i: usize, speed: f32, secs: f32, grind: f32, damage: f32, knockback: f32, element: Element, up: f32) -> Result<(), PowerError> {
        let (eye, look) = self.toolbox_eye_look(i);
        // Mostly along the look; a steep dive / climb is allowed but softened.
        let dir = Vec3::new(look.x, look.y.clamp(-0.8, 0.6), look.z).normalize_or(Vec3::Z);
        let speed = speed.clamp(4.0, 50.0);
        let st = DrillState {
            t: secs.clamp(0.05, 1.2),
            dir,
            speed,
            grind: grind.clamp(0.0, MAX_GRIND),
            grind_t: 0.0,
            target: None,
            tick_t: 0.0,
            damage: damage.clamp(0.0, 60.0),
            knockback: knockback.clamp(0.0, 70.0),
            up: up.clamp(0.0, 2.0),
            element,
        };
        let id = self.players[i].id;
        {
            let p = &mut self.players[i];
            p.power.live.drill = Some(st);
            p.mv.vel = dir * speed;
            if dir.y > 0.05 {
                p.mv.grounded = false;
            }
        }
        self.toolbox_fx(id, ToolboxFx::DrillStart, eye + look * 0.6, eye + dir * (speed * st.t), st.t, st.grind);
        // Already touching someone: latch now.
        self.drill_contact(i);
        Ok(())
    }

    /// Latch onto (or strike) the first enemy the palm touches this tick. True = contact.
    fn drill_contact(&mut self, i: usize) -> bool {
        let Some(st) = self.players[i].power.live.drill else { return false };
        if st.target.is_some() {
            return false;
        }
        let base = self.cfg.move_params.clone();
        let me = self.players[i].hull(&base).center();
        let a = me;
        let b = me + st.dir * REACH;
        let mut best: Option<(usize, f32)> = None;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive {
                continue;
            }
            let hull = crate::geom::inflate(&q.hull(&base), 0.35);
            if let Some((t, _)) = crate::geom::segment_aabb(a, b, &hull) {
                if best.is_none_or(|x| t < x.1) {
                    best = Some((k, t));
                }
            }
        }
        let Some((k, _)) = best else { return false };
        let id = self.players[i].id;
        let tid = self.players[k].id;
        let point = self.players[k].hull(&base).center();
        if st.grind <= 0.0 {
            // One piercing strike.
            let hit = HitInfo::new(tid, Some(id), HitKind::HeavyMelee, st.damage, st.knockback, st.dir + Vec3::Y * st.up, point).with_element(st.element).with_impact(1.6);
            self.apply_hit(hit);
            self.drill_end(i, point, ToolboxFx::DrillRelease);
            return true;
        }
        // Latch: a credit touch (a dodge slips it).
        if !self.toolbox_touch(i, k, HitKind::Melee) {
            return false;
        }
        if let Some(d) = self.players[i].power.live.drill.as_mut() {
            d.target = Some(tid);
            d.grind_t = d.grind;
            d.tick_t = 0.0;
        }
        let p = &mut self.players[i];
        p.mv.vel = Vec3::ZERO;
        self.toolbox_fx(id, ToolboxFx::DrillLatch, me, point, st.grind, 0.0);
        true
    }

    fn drill_end(&mut self, i: usize, at: Vec3, fx: ToolboxFx) {
        let id = self.players[i].id;
        let from = self.players[i].hull(&self.cfg.move_params).center();
        self.players[i].power.live.drill = None;
        self.toolbox_fx(id, fx, from, at, 0.0, 0.0);
    }

    pub(crate) fn step_drill(&mut self, i: usize, dt: f32) {
        let Some(st) = self.players[i].power.live.drill else { return };
        let base = self.cfg.move_params.clone();
        if self.players[i].status.stunned() {
            let at = self.players[i].hull(&base).center();
            self.drill_end(i, at, ToolboxFx::DrillEnd);
            return;
        }
        let id = self.players[i].id;
        match st.target {
            None => {
                // The lunge: committed along its line.
                let t = st.t - dt;
                let me = self.players[i].hull(&base).center();
                // Terrain in the palm's way: bore it and stop.
                if self.toolbox_blocked(i, me, me + st.dir * (REACH + st.speed * dt)) {
                    let at = me + st.dir * REACH;
                    self.request_carve(CarveRequest { center: at, radius: 1.0 + st.damage * 0.03, power: 1.5 });
                    self.drill_end(i, at, ToolboxFx::DrillEnd);
                    return;
                }
                if self.drill_contact(i) {
                    return;
                }
                if t <= 0.0 {
                    let at = me + st.dir * REACH;
                    self.players[i].mv.vel *= 0.35;
                    self.drill_end(i, at, ToolboxFx::DrillEnd);
                    return;
                }
                if let Some(d) = self.players[i].power.live.drill.as_mut() {
                    d.t = t;
                }
                self.players[i].mv.vel = st.dir * st.speed;
            }
            Some(tid) => {
                let Some(k) = self.pidx(tid).filter(|&k| self.players[k].alive) else {
                    let at = self.players[i].hull(&base).center();
                    self.drill_end(i, at, ToolboxFx::DrillEnd);
                    return;
                };
                let me = self.players[i].hull(&base).center();
                let pin = me + st.dir * REACH;
                // Both hang where they met: the driller hovers, the victim is held at the palm.
                self.players[i].mv.vel = Vec3::ZERO;
                {
                    let q = &mut self.players[k];
                    let c = q.hull(&base).center();
                    q.mv.vel = (pin - c) * 12.0;
                    q.mv.grounded = false;
                }
                let mut d = st;
                d.grind_t -= dt;
                d.tick_t -= dt;
                if d.tick_t <= 0.0 {
                    d.tick_t += GRIND_TICK;
                    let ticks = (d.grind / GRIND_TICK).max(1.0);
                    let dmg = d.damage * GRIND_SHARE / ticks;
                    // Escapable: a dodge (the touch doesn't land) slips out.
                    if !self.toolbox_touch(i, k, HitKind::Melee) {
                        self.drill_end(i, pin, ToolboxFx::DrillEnd);
                        return;
                    }
                    self.damage_over_time(k, dmg, Some(id));
                }
                if d.grind_t <= 0.0 {
                    let point = self.players[k].hull(&base).center();
                    let hit = HitInfo::new(tid, Some(id), HitKind::HeavyMelee, d.damage * (1.0 - GRIND_SHARE), d.knockback, d.dir + Vec3::Y * d.up, point)
                        .with_element(d.element)
                        .with_impact(1.8);
                    self.apply_hit(hit);
                    // The release craters what's behind them a little.
                    self.request_carve(CarveRequest { center: point + d.dir * 1.2, radius: 0.8 + d.damage * 0.02, power: 1.2 });
                    self.drill_end(i, point, ToolboxFx::DrillRelease);
                    return;
                }
                self.players[i].power.live.drill = Some(d);
            }
        }
    }
}
