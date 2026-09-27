//! `MoveOp::Relocate` — teleport to a spot RELATIVE TO A TARGET (docs/toolbox/OPS.md, VERB GRAMMAR).
//!
//! "shadow teleport behind ethan", "blink above him", "teleport in front of the dragon", "warp
//! beside her", "bamf onto the tower", "teleport back". The anchor is, in order: the composed
//! move's resolved target (`World::compose_now`: a named player, "him", "it", "the dragon", "me"),
//! else the enemy / construct nearest the crosshair (`range`, `cone`; you sense them — no line of
//! sight needed for the aim ray), else the terrain point under the crosshair; `me: true` = always
//! yourself. You appear `back` m behind it (< 0 = in front, face to face), `side` m to its side and
//! `up` m above its top (a player: behind = their BACK, from their own look; a construct / point /
//! yourself: behind = its far side from you / behind your look). The spot must be open air in
//! play (else the other side, then the side, then fail — nothing spent). Nothing to anchor on =
//! a plain `dist` m blink (0 = fail). Ends airborne (a beat to strike, or a fall onto them from
//! above: the slam chain). Deterministic, allocation-free; nothing runs unless a move uses it.

use bd_core::Vec3;

use super::super::{PowerError, PowerEvent};
use super::flat;
use crate::events::GameEvent;
use crate::movement::{ColliderId, CollisionWorld, raycast_world};
use crate::powers::compose::Forced;
use crate::world::World;

/// Farthest a relocation reaches (m).
pub const MAX_RANGE: f32 = 90.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RelocateSpec {
    pub range: f32,
    pub cone: f32,
    pub back: f32,
    pub side: f32,
    pub up: f32,
    pub dist: f32,
    pub me: bool,
}

/// An anchor: its feet / base point, the way it faces, its height, its half width.
#[derive(Clone, Copy, Debug)]
struct Anchor {
    base: Vec3,
    face: Vec3,
    height: f32,
    width: f32,
}

fn fin(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { 0.0 }
}

impl World {
    pub(crate) fn op_relocate(&mut self, i: usize, s: RelocateSpec) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        let hull = self.players[i].hull(&base);
        let h = hull.half().y;
        let me_pos = self.players[i].mv.pos;
        let (eye, look) = self.op_eye_look(i);
        let my_face = flat(look, Vec3::NEG_Z);
        let range = fin(s.range, 1.0, MAX_RANGE);
        let (back, side, up) = (fin(s.back, -30.0, 30.0), fin(s.side, -12.0, 12.0), fin(s.up, -4.0, 16.0));
        let self_anchor = Anchor { base: me_pos, face: my_face, height: h * 2.0, width: 0.0 };
        let player_anchor = |w: &World, k: usize| {
            let q = &w.players[k];
            let qh = q.hull(&base);
            Anchor { base: q.mv.pos, face: flat(q.look(), my_face), height: qh.half().y * 2.0, width: qh.half().x }
        };
        let construct_anchor = |_w: &World, c: &crate::constructs::Construct| {
            let to_me = flat(me_pos - c.pos, -my_face);
            Anchor { base: c.pos - Vec3::Y * c.half.y, face: to_me, height: c.half.y * 2.0, width: c.half.x.max(c.half.z) }
        };
        let anchor: Option<Anchor> = if s.me {
            Some(self_anchor)
        } else {
            match self.compose_now(i) {
                Some(Forced::Me) => Some(self_anchor),
                Some(Forced::Player(pid)) => self.pidx(pid).filter(|&k| self.players[k].alive).map(|k| player_anchor(self, k)),
                Some(Forced::Construct(cid)) => self.constructs.get(cid).filter(|c| c.gone.is_none()).map(|c| construct_anchor(self, c)),
                None => {
                    if let Some(k) = self.op_aimed_player(i, range, fin(s.cone, 0.0, 45.0)) {
                        Some(player_anchor(self, k))
                    } else {
                        // a construct near the crosshair, else the terrain under it
                        let cos = fin(s.cone, 0.0, 45.0).to_radians().cos();
                        let mut best: Option<(f32, Anchor)> = None;
                        for c in self.constructs.iter() {
                            if c.gone.is_some() || c.parent.is_some() {
                                continue;
                            }
                            let d = c.pos - eye;
                            let len = d.length();
                            if len < 0.5 || len > range {
                                continue;
                            }
                            let dot = d.dot(look) / len;
                            if dot >= cos && best.as_ref().is_none_or(|b| dot > b.0) {
                                best = Some((dot, construct_anchor(self, c)));
                            }
                        }
                        best.map(|b| b.1).or_else(|| {
                            let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(ColliderId::Player(id));
                            raycast_world(&cw, eye, look, range, false).map(|hit| Anchor { base: hit.point + hit.normal * 0.05, face: -my_face, height: 0.0, width: 0.0 })
                        })
                    }
                }
            }
        };
        let Some(a) = anchor else {
            // nothing to anchor on: a plain blink (or nothing)
            let d = fin(s.dist, 0.0, 40.0);
            if d < 0.5 {
                return Err(PowerError::NoTarget);
            }
            return self.relocate_blink(i, d);
        };
        if !s.me && (a.base - me_pos).length() > range + a.width + 2.0 {
            return Err(PowerError::NoTarget);
        }
        let right = Vec3::new(-a.face.z, 0.0, a.face.x);
        let gap = |v: f32| if v == 0.0 { 0.0 } else { v.signum() * (v.abs() + a.width) };
        let lift = if up > 0.0 { a.height + up } else { up.max(0.0) };
        let want = a.base - a.face * gap(back) + right * gap(side) + Vec3::Y * lift;
        let alt = [
            want,
            want + Vec3::Y * 1.0,
            want + Vec3::Y * 2.2,
            a.base + a.face * gap(back) + right * gap(side) + Vec3::Y * lift,
            a.base + right * gap(if side == 0.0 { 1.8 } else { -side }) + Vec3::Y * lift,
            a.base - right * gap(if side == 0.0 { 1.8 } else { side }) + Vec3::Y * lift,
        ];
        let free = |w: &World, feet: Vec3| [0.3, h, h * 2.0 - 0.2].iter().all(|y| w.voxels.get_at(feet + Vec3::Y * *y) == 0) && w.in_play(feet, 0.0);
        let Some(to) = alt.iter().copied().find(|p| p.is_finite() && free(self, *p)) else { return Err(PowerError::NoTarget) };
        if (to - me_pos).length() < 0.3 {
            return Err(PowerError::NoTarget);
        }
        let from = me_pos;
        let p = &mut self.players[i];
        p.mv.pos = to;
        p.mv.grounded = false;
        // arrive composed: most of the momentum bleeds off (from above you drop straight onto them)
        p.mv.vel = if lift > 0.5 { Vec3::ZERO } else { Vec3::new(p.mv.vel.x * 0.25, p.mv.vel.y.max(0.0) * 0.25, p.mv.vel.z * 0.25) };
        self.inject_event(GameEvent::Power(PowerEvent::Blinked { player: id, from, to, afterimage: true }));
        Ok(())
    }

    /// A plain blink along the aim (walls stop it), `Blink`'s rule.
    fn relocate_blink(&mut self, i: usize, want: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        let hull = self.players[i].hull(&base);
        let (_, look) = self.op_eye_look(i);
        let dir = Vec3::new(look.x, look.y * 0.5, look.z).normalize_or(flat(look, Vec3::NEG_Z));
        let from = self.players[i].mv.pos;
        let h = hull.half().y;
        let mut d = want;
        {
            let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(ColliderId::Player(id));
            for off in [Vec3::ZERO, Vec3::Y * (h - 0.15), Vec3::Y * -(h - 0.15)] {
                if let Some(hit) = raycast_world(&cw, hull.center() + off, dir, want + 0.5, false) {
                    d = d.min((hit.t - 0.5).max(0.0));
                }
            }
        }
        let to = from + dir * d;
        if d < 0.25 || !self.in_play(to, 0.0) {
            return Err(PowerError::NoTarget);
        }
        let p = &mut self.players[i];
        p.mv.pos = to;
        p.mv.grounded = false;
        self.inject_event(GameEvent::Power(PowerEvent::Blinked { player: id, from, to, afterimage: true }));
        Ok(())
    }
}
