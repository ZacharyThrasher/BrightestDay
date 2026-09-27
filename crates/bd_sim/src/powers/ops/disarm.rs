//! `MoveOp::Disarm` — knock the weapon out of their hands (docs/toolbox/OPS.md).
//!
//! The aimed enemy (`range`, aim-forgiving `cone`) loses what they hold and wear: each of their
//! held / worn constructs (never a piloted suit or a companion) is struck hard enough to burst
//! out of their grip — its pieces fly toward you (`pull`, Expelliarmus) or up and away. They also
//! take a small jolt (`knockback`, toward you when pulling). Empty-handed = just the jolt: the
//! spell still lands. An `OpEvent::Zap` draws it hand → target. Deterministic, allocation-free;
//! does nothing unless a move uses it.

use bd_core::mechanics::Element;
use bd_core::{ConstructId, Vec3};

use super::OpEvent;
use super::afflict::fin_range;
use super::super::PowerError;
use crate::combat::{ConstructHit, HitInfo, HitKind};
use crate::world::World;

/// Most items one disarm knocks loose.
pub const MAX_DISARM: usize = 4;
/// Integrity damage that bursts an item out of the hand.
const DISARM_DAMAGE: f32 = 10_000.0;

impl World {
    pub(crate) fn op_disarm(&mut self, i: usize, range: f32, cone: f32, pull: bool, knockback: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let (eye, look) = self.op_eye_look(i);
        let Some(k) = self.op_aimed_player(i, fin_range(range), cone.clamp(0.0, 30.0)) else { return Err(PowerError::NoTarget) };
        let base = self.cfg.move_params.clone();
        let tid = self.players[k].id;
        let c = self.players[k].hull(&base).center();
        let pilot = self.players[k].seat.map(|s| s.construct);
        let companion = self.players[k].power.companion;
        let mut items: [Option<ConstructId>; MAX_DISARM] = [None; MAX_DISARM];
        let mut n = 0;
        for con in self.constructs.iter() {
            if n >= MAX_DISARM {
                break;
            }
            if con.owner != tid || !con.attached() || con.suit || con.parent.is_some() || Some(con.id) == pilot || Some(con.id) == companion {
                continue;
            }
            items[n] = Some(con.id);
            n += 1;
        }
        self.op_event(OpEvent::Zap { player: id, from: eye + look * 0.5, to: c, element: Element::Light, target: Some(tid) });
        let to_me = (eye - c).normalize_or(-look);
        let fling = if pull { to_me * 14.0 + Vec3::Y * 6.0 } else { Vec3::Y * 12.0 - to_me * 4.0 };
        for cid in items.iter().flatten().copied() {
            let point = self.constructs.get(cid).map_or(c, |x| x.center());
            self.apply_construct_hit(ConstructHit { pierce: false, blunt: true, target: cid, source: Some(id), damage: DISARM_DAMAGE, impulse: fling, blade: false, point });
        }
        let kb = if knockback.is_finite() { knockback.clamp(0.0, 30.0) } else { 0.0 };
        let push = if pull { to_me + Vec3::Y * 0.3 } else { -to_me + Vec3::Y * 0.3 };
        self.apply_hit(HitInfo::new(tid, Some(id), HitKind::RingBlast, 0.0, kb, push, c).with_impact(0.9));
        Ok(())
    }
}
