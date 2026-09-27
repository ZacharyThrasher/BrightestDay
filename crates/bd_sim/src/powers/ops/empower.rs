//! `Empower` — a timed super state (docs/toolbox/OPS.md), ≤ 8 s: your hits deal ×`damage`, the
//! knockback you take is ×`armor` (never immunity), and enemies you TOUCH are struck (`contact`
//! damage, `contact_kb`) at most once per `contact_cd` s each. Super Star, a power pellet, Spartan
//! Rage, a Devil Trigger burst, a Mega Mushroom stomp-walk. Movement is untouched (prediction stays
//! exact): pair it with an Impulse / a form for speed.

use bd_core::mechanics::Element;
use bd_core::{MAX_PLAYERS, PlayerId, Vec3};

use super::OpEvent;
use crate::combat::{HitInfo, HitKind};
use crate::powers::PowerError;
use crate::world::World;

pub const MAX_SECS: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EmpowerSpec {
    pub secs: f32,
    pub damage: f32,
    pub armor: f32,
    pub contact: f32,
    pub contact_kb: f32,
    pub contact_cd: f32,
    pub element: Element,
}

/// A live super state (`OpState::empowers`, indexed by player index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Empowered {
    pub owner: PlayerId,
    pub left: f32,
    pub spec: EmpowerSpec,
    /// Per victim (player index): seconds until they can be contact-struck again.
    pub cds: [f32; MAX_PLAYERS],
}

impl World {
    pub(crate) fn op_empower(&mut self, i: usize, s: EmpowerSpec) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let spec = EmpowerSpec {
            secs: f(s.secs, 0.5, MAX_SECS, 4.0),
            damage: f(s.damage, 0.5, 2.5, 1.0),
            armor: f(s.armor, 0.2, 1.0, 1.0),
            contact: f(s.contact, 0.0, 30.0, 0.0),
            contact_kb: f(s.contact_kb, 0.0, 40.0, 0.0),
            contact_cd: f(s.contact_cd, 0.2, 3.0, 0.6),
            element: s.element,
        };
        self.powers.ops.empowers[i] = Some(Empowered { owner: id, left: spec.secs, spec, cds: [0.0; MAX_PLAYERS] });
        self.op_event(OpEvent::Empowered { player: id, secs: spec.secs, on: true });
        Ok(())
    }

    pub(crate) fn step_empowers(&mut self, dt: f32) {
        let base = self.cfg.move_params.clone();
        for i in 0..MAX_PLAYERS.min(self.players.len()) {
            let Some(mut e) = self.powers.ops.empowers[i] else { continue };
            let p = &self.players[i];
            if p.id != e.owner || !p.alive {
                self.powers.ops.empowers[i] = None;
                continue;
            }
            e.left -= dt;
            for c in &mut e.cds {
                *c = (*c - dt).max(0.0);
            }
            if e.left <= 0.0 {
                self.powers.ops.empowers[i] = None;
                self.op_event(OpEvent::Empowered { player: e.owner, secs: 0.0, on: false });
                continue;
            }
            if e.spec.contact > 0.0 || e.spec.contact_kb > 0.0 {
                let my = crate::geom::inflate(&p.hull(&base), 0.35);
                let me = my.center();
                let mut struck: [Option<usize>; MAX_PLAYERS] = [None; MAX_PLAYERS];
                for (k, q) in self.players.iter().enumerate().take(MAX_PLAYERS) {
                    if k == i || !q.alive || e.cds[k] > 0.0 {
                        continue;
                    }
                    let h = q.hull(&base);
                    if crate::geom::closest_point(&h, me).distance(crate::geom::closest_point(&my, h.center())) <= 0.05 {
                        struck[k] = Some(k);
                    }
                }
                for k in struck.iter().flatten().copied() {
                    e.cds[k] = e.spec.contact_cd;
                    let tid = self.players[k].id;
                    let c = self.players[k].hull(&base).center();
                    let dir = Vec3::new(c.x - me.x, 0.0, c.z - me.z).normalize_or(Vec3::X) + Vec3::Y * 0.45;
                    let h = HitInfo::new(tid, Some(e.owner), HitKind::Melee, e.spec.contact, e.spec.contact_kb, dir, c).with_element(e.spec.element).with_impact(1.2);
                    self.apply_hit(h);
                }
            }
            // A hit landed above may have ended it (KO): only keep a still-valid state.
            if self.players[i].alive && self.players[i].id == e.owner {
                self.powers.ops.empowers[i] = Some(e);
            }
        }
    }

    /// Both sides of a hit: the attacker's ×damage, the target's armor.
    pub(crate) fn empower_on_hit(&self, ti: usize, mut hit: HitInfo) -> HitInfo {
        if let Some(si) = hit.source.and_then(|s| self.pidx(s)).filter(|&s| s < MAX_PLAYERS && s != ti)
            && let Some(e) = self.powers.ops.empowers[si].filter(|e| Some(e.owner) == hit.source)
        {
            hit.damage *= e.spec.damage;
        }
        if let Some(e) = self.powers.ops.empowers[ti].filter(|e| e.owner == hit.target)
            && hit.source != Some(hit.target)
        {
            hit.knockback *= e.spec.armor;
        }
        hit
    }
}
