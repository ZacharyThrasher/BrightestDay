//! `MoveOp::Barrage` — a timed flurry (docs/toolbox/OPS.md).
//!
//! `hits` front-cone strikes (`range`, `angle`, `damage`, `knockback`, `element`, `up`) land every
//! `interval` s; the last one hits with `finish` knockback (0 = the same) — the flurry's closer. Each
//! strike surges you `advance` m/s along your look (a berserker barrage that keeps driving in).
//! `blink: true` is the teleport flurry: between strikes you vanish and reappear on another side of
//! the enemy you're hitting (golden-angle spacing, open air only), striking from there — Nightcrawler's
//! BAMF-BAMF-BAMF, a speedster's hundred-angle beatdown. A stun, a KO or a new flurry ends it. The
//! first strike lands at once (the move "did something"); the rest run from `step_generic_ops`.
//! Deterministic; a fixed per-player table (`OpState::flurries`), allocation-free; nothing runs
//! while no flurry is live.

use bd_core::mechanics::Element;
use bd_core::{MAX_PLAYERS, PlayerId, Vec3};

use super::super::{PowerError, PowerEvent};
use crate::combat::HitKind;
use crate::events::GameEvent;
use crate::world::World;

/// Most strikes in one flurry.
pub const MAX_HITS: u8 = 16;
/// Blink flurry: you reappear this far from the target's centre (m).
pub const BLINK_RING: f32 = 2.1;
/// Blink flurry: a target farther than this is let go (m).
pub const BLINK_LEASH: f32 = 14.0;
const GOLDEN: f32 = 2.399_963;

/// A live flurry (`OpState::flurries`, indexed by the owner's player index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flurry {
    pub owner: PlayerId,
    /// Strikes left.
    pub left: u8,
    /// Seconds to the next strike.
    pub t: f32,
    pub interval: f32,
    pub range: f32,
    pub angle: f32,
    pub damage: f32,
    pub knockback: f32,
    pub finish: f32,
    pub advance: f32,
    pub up: f32,
    pub blink: bool,
    pub element: Element,
    /// Blink flurry: who we're beating on, and how many hops so far.
    pub target: Option<PlayerId>,
    pub hops: u8,
}

/// `MoveOp::Barrage`'s numbers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BarrageArgs {
    pub hits: u8,
    pub interval: f32,
    pub range: f32,
    pub angle: f32,
    pub damage: f32,
    pub knockback: f32,
    pub finish: f32,
    pub advance: f32,
    pub up: f32,
    pub blink: bool,
    pub element: Element,
}

impl World {
    pub(crate) fn op_barrage(&mut self, i: usize, a: BarrageArgs) -> Result<(), PowerError> {
        if i >= MAX_PLAYERS {
            return Err(PowerError::UnknownPlayer);
        }
        let id = self.players[i].id;
        let (eye, look) = self.op_eye_look(i);
        let fin = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { lo };
        let hits = a.hits.clamp(1, MAX_HITS);
        let range = fin(a.range, 0.5, 8.0);
        let target = if a.blink { self.op_aimed_player(i, range + BLINK_RING + 4.0, 20.0).map(|k| self.players[k].id) } else { None };
        let mut f = Flurry {
            owner: id,
            left: hits,
            t: 0.0,
            interval: fin(a.interval, 0.05, 1.0),
            range,
            angle: fin(a.angle, 5.0, 180.0),
            damage: fin(a.damage, 0.0, 40.0),
            knockback: fin(a.knockback, 0.0, 60.0),
            finish: fin(a.finish, 0.0, 60.0),
            advance: fin(a.advance, 0.0, 30.0),
            up: fin(a.up, 0.0, 2.0),
            blink: a.blink,
            element: a.element,
            target,
            hops: 0,
        };
        // A new flurry replaces yours.
        self.powers.ops.flurries[i] = None;
        self.flurry_strike(i, &mut f, Some((eye, look)));
        if f.left > 0 {
            f.t = f.interval;
            self.powers.ops.flurries[i] = Some(f);
        }
        Ok(())
    }

    /// Run the live flurries (from `step_generic_ops`).
    pub(crate) fn step_flurries(&mut self, dt: f32) {
        for i in 0..MAX_PLAYERS {
            let Some(mut f) = self.powers.ops.flurries[i] else { continue };
            let ok = self.players.get(i).is_some_and(|p| p.id == f.owner && p.alive && !p.status.stunned());
            self.powers.ops.flurries[i] = None;
            if !ok {
                continue;
            }
            f.t -= dt;
            if f.t <= 0.0 {
                f.t += f.interval;
                self.flurry_strike(i, &mut f, None);
            }
            if f.left > 0 {
                self.powers.ops.flurries[i] = Some(f);
            }
        }
    }

    fn flurry_strike(&mut self, i: usize, f: &mut Flurry, aim: Option<(Vec3, Vec3)>) {
        let base = self.cfg.move_params.clone();
        let (eye, look) = aim.unwrap_or_else(|| {
            let p = &self.players[i];
            (p.eye(&base), p.look())
        });
        let mut dir = look;
        let mut origin = eye;
        let last = f.left <= 1;
        if f.blink {
            let tgt = f.target.and_then(|t| self.pidx(t)).filter(|&k| {
                let q = &self.players[k];
                q.alive && (q.hull(&base).center() - self.players[i].hull(&base).center()).length() <= BLINK_LEASH
            });
            if let Some(k) = tgt {
                let tc = self.players[k].hull(&base).center();
                let tfeet = self.players[k].mv.pos;
                // Try a few angles around them for open air.
                let from = self.players[i].mv.pos;
                let start = f.hops as f32 * GOLDEN + 1.0;
                for step in 0..4 {
                    let ang = start + step as f32 * 1.3;
                    let feet = tfeet + Vec3::new(ang.cos() * BLINK_RING, 0.3, ang.sin() * BLINK_RING);
                    let free = [0.3, 0.9, 1.6].iter().all(|y| self.voxels.get_at(feet + Vec3::Y * *y) == 0);
                    if free && self.in_play(feet, 0.0) {
                        let p = &mut self.players[i];
                        p.mv.pos = feet;
                        p.mv.vel = Vec3::ZERO;
                        p.mv.grounded = false;
                        let id = p.id;
                        self.inject_event(GameEvent::Power(PowerEvent::Blinked { player: id, from, to: feet, afterimage: true }));
                        break;
                    }
                }
                f.hops = f.hops.wrapping_add(1);
                origin = self.players[i].eye(&base);
                dir = (tc - origin).normalize_or(look);
            }
        } else if f.advance > 0.0 {
            let fl = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
            let mv = &mut self.players[i].mv;
            let along = mv.vel.dot(fl);
            if along < f.advance {
                mv.apply_impulse(fl * (f.advance - along.max(0.0)), 0.0);
            }
        }
        let kb = if last && f.finish > 0.0 { f.finish } else { f.knockback };
        let up = if last { f.up.max(0.35) } else { f.up };
        let hand = origin + dir * 0.4;
        self.cone_hit(i, hand, dir, f.range, f.angle, f.damage, kb, f.element, up, None, HitKind::Melee);
        f.left = f.left.saturating_sub(1);
    }
}
