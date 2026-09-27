//! FUSION: two moves → one, by what they ARE (op classes, `fusion.classes` in the data), never by
//! name. Rules, first that fits:
//!
//! 1. **Two projectiles** (each has a Cast): ONE orb carrying both — the first's body, both
//!    behaviours, the bigger size × `merge_size`, a Merge windup of both elements (an orb of each at
//!    either hand, pulled together: the Hollow Purple grammar), cast as a signature. Their other ops
//!    ride along.
//! 2. **A movement + an attack**: the attack is delivered at the END of the movement — at once after
//!    a blink; as a landing slam after a leap / hop (a blast or strike becomes `SlamOnLand`); as a
//!    homing lunge-strike for a dash + strike (`Pounce`); otherwise as a last stage fired when the
//!    movement's active phase ends.
//! 3. **A field + anything**: the thing leaves that field where it lands (fields / zones re-placed at
//!    the aim point).
//! 4. **A projectile + a strike**: an IMBUED strike — the strike takes the projectile's element,
//!    damage × `imbue_damage`, and its Explodes / Pierce / Spin; the projectile itself is spent.
//! 5. Anything else: both, in order (a buff first).
//!
//! The fused move cools both base moves, costs the higher tier + `fusion.tier`, and titles itself
//! "A + B".

use bd_core::mechanics::{Element, Mechanic};

use super::ops::{element_of, op_name, Twisted};
use super::{ModTable, OpClass};
use crate::powers::chain;
use crate::powers::{At, MoveDef, MoveJuice, MoveOp};
use crate::signature::SignatureKind;

/// The class of one op (data; an unknown op with a recipe is a Projectile, else a Blast).
pub fn class(t: &ModTable, op: &MoveOp) -> OpClass {
    let name = op_name(op);
    t.class_of(&name).unwrap_or(if matches!(op, MoveOp::Cast { .. } | MoveOp::Seeker { .. }) { OpClass::Projectile } else { OpClass::Blast })
}

/// Which classes a move has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    pub projectile: bool,
    pub melee: bool,
    pub movement: bool,
    pub blast: bool,
    pub field: bool,
    pub buff: bool,
    pub control: bool,
}

impl Profile {
    pub fn of(t: &ModTable, def: &MoveDef) -> Self {
        let mut p = Profile::default();
        for op in &def.ops {
            let c = class(t, op);
            // a cosmetic puff (Nightcrawler's 0-damage smoke burst) is not an attack
            if matches!(c, OpClass::Blast | OpClass::Melee) {
                let f = super::ops::fields(op, &["damage", "knockback"]);
                if f[0] <= 0.0 && f[1] < 5.0 {
                    continue;
                }
            }
            match c {
                OpClass::Projectile => p.projectile = true,
                OpClass::Melee => p.melee = true,
                OpClass::Movement => p.movement = true,
                OpClass::Blast => p.blast = true,
                OpClass::Field => p.field = true,
                OpClass::Buff => p.buff = true,
                OpClass::Control => p.control = true,
            }
        }
        p
    }

    /// Hits someone (a strike, a blast, a projectile, a control).
    pub fn attacks(&self) -> bool {
        self.projectile || self.melee || self.blast || self.control
    }

    /// Only moves you (a blink, a dash) — maybe with a buff.
    pub fn only_moves(&self) -> bool {
        self.movement && !self.attacks() && !self.field
    }

    /// Only a field / zone (maybe with a buff).
    pub fn only_field(&self) -> bool {
        self.field && !self.projectile && !self.melee && !self.blast
    }
}

/// The first Cast of a move: (index, recipe, signature).
fn first_cast(def: &MoveDef) -> Option<(usize, &crate::powers::RecipeDef, Option<SignatureKind>)> {
    def.ops.iter().enumerate().find_map(|(k, o)| match o {
        MoveOp::Cast { recipe, signature, .. } => Some((k, recipe, *signature)),
        _ => None,
    })
}

fn instant_move(op: &MoveOp) -> bool {
    matches!(op, MoveOp::Blink { .. } | MoveOp::BlinkCarry { .. } | MoveOp::WarpTo { .. } | MoveOp::Transpose { .. } | MoveOp::MarkRecall { .. } | MoveOp::Rewind { .. })
}

fn airborne_move(op: &MoveOp) -> bool {
    matches!(op, MoveOp::Leap { .. }) || matches!(op, MoveOp::Impulse { up, .. } if *up >= 5.0)
}

fn dash_speed(op: &MoveOp) -> Option<f32> {
    match *op {
        MoveOp::Impulse { fwd, .. } if fwd >= 6.0 => Some(fwd),
        MoveOp::Ram { speed, .. } | MoveOp::Drill { speed, .. } | MoveOp::Pounce { speed, .. } => Some(speed),
        _ => None,
    }
}

/// (damage, knockback, element, up) of the strongest strike / blast of a move, and a radius for it.
fn hit_of(def: &MoveDef) -> (f32, f32, Element, f32, f32) {
    let mut best = (0.0f32, 0.0f32, element_of(def), 0.0f32, 3.5f32);
    for op in &def.ops {
        let h = match *op {
            MoveOp::Shove { range, damage, knockback, element, up, .. } => Some((damage, knockback, element, up, range.max(3.0))),
            MoveOp::Explode { radius, damage, knockback, element, .. } => Some((damage, knockback, element, 0.0, radius)),
            MoveOp::Finisher { range, damage, knockback, element, up, .. } => Some((damage, knockback, element, up, range.max(3.0))),
            MoveOp::Salvo { radius, damage, knockback, element, up, .. } => Some((damage, knockback, element, up, radius)),
            MoveOp::SlamOnLand { radius, damage, knockback, .. } => Some((damage, knockback, best.2, 0.0, radius)),
            MoveOp::Ram { damage, knockback, element, up, .. } | MoveOp::Drill { damage, knockback, element, up, .. } => Some((damage, knockback, element, up, 3.0)),
            MoveOp::Barrage { range, damage, knockback, element, up, hits, .. } => Some((damage * hits.max(1) as f32 * 0.5, knockback, element, up, range.max(3.0))),
            _ => None,
        };
        if let Some(h) = h
            && h.0 > best.0
        {
            best = h;
        }
    }
    best
}

fn mix(a: Option<[f32; 3]>, b: Option<[f32; 3]>) -> Option<[f32; 3]> {
    match (a, b) {
        (Some(a), Some(b)) => Some([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]),
        (a, b) => a.or(b),
    }
}

fn place_at_aim(op: MoveOp) -> MoveOp {
    match op {
        MoveOp::Field { strength, radius, secs, .. } => MoveOp::Field { strength, radius, secs, at: At::Aim, dist: 0.0 },
        MoveOp::Zone { radius, secs, status, slow, dps, lockout, .. } => MoveOp::Zone { radius, secs, status, slow, dps, lockout, at: At::Aim, dist: 0.0, follow: false },
        MoveOp::Room { radius, secs } => MoveOp::Field { strength: 0.0, radius, secs, at: At::Aim, dist: 0.0 },
        other => other,
    }
}

/// Fuse two (already twisted) moves into one.
pub fn fuse(t: &ModTable, a: &Twisted, b: &Twisted) -> Twisted {
    let f = &t.fusion;
    let (pa, pb) = (Profile::of(t, &a.def), Profile::of(t, &b.def));
    let (ea, eb) = (a.element.unwrap_or_else(|| element_of(&a.def)), b.element.unwrap_or_else(|| element_of(&b.def)));
    let mut ops: Vec<MoveOp> = Vec::new();
    let mut after: Option<(f32, Vec<MoveOp>)> = None;
    let mut merged_orb = false;
    let how: &'static str;
    let others = |d: &MoveDef, skip: Option<usize>| -> Vec<MoveOp> { d.ops.iter().enumerate().filter(|(k, _)| Some(*k) != skip).map(|(_, o)| o.clone()).collect() };
    if let (Some((ca, ra, sa)), Some((cb, rb, sb))) = (first_cast(&a.def), first_cast(&b.def)) {
        // 1. two projectiles → one orb, both behaviours, a Merge windup
        let mut r = ra.clone();
        for m in &rb.mech {
            if !r.mech.contains(m) {
                r.mech.push(*m);
            }
        }
        r.mech.retain(|m| !matches!(m, Mechanic::Swarm));
        r.mech.truncate(8);
        r.scale = ra.scale.max(rb.scale) * f.merge_size;
        r.mass = ra.mass.max(rb.mass) * f.merge_size;
        r.count = 1;
        r.name = format!("{} {}", ra.name, rb.name).trim().to_string();
        let piercing = r.mech.contains(&Mechanic::Pierce) || r.mech.contains(&Mechanic::Beam);
        let sig = sa.or(sb).or(Some(if piercing { SignatureKind::Beam } else { SignatureKind::Summon }));
        ops.push(MoveOp::Cast { recipe: r, modifier: false, signature: sig, merge: Some((ea, eb)) });
        ops.extend(others(&a.def, Some(ca)));
        ops.extend(others(&b.def, Some(cb)));
        merged_orb = true;
        how = "one orb, both behaviours, a merge windup";
    } else if pa.only_moves() != pb.only_moves() && (pa.attacks() || pb.attacks()) {
        // 2. a movement + an attack: the attack arrives at the end of the movement
        let (m, x) = if pa.only_moves() { (&a.def, &b.def) } else { (&b.def, &a.def) };
        let xp = Profile::of(t, x);
        let (dmg, kb, el, up, radius) = hit_of(x);
        if m.ops.iter().any(instant_move) {
            ops.extend(m.ops.iter().cloned());
            ops.extend(x.ops.iter().cloned());
            how = "the attack arrives with you";
        } else if m.ops.iter().any(airborne_move) && (xp.blast || xp.melee) && !xp.projectile {
            ops.extend(m.ops.iter().filter(|o| !matches!(o, MoveOp::SlamOnLand { .. })).cloned());
            ops.push(MoveOp::SlamOnLand { radius: (radius * 1.1).clamp(2.5, 9.0), damage: dmg, knockback: kb, carve: 1.2 });
            // the rest of the attack (a status, a buff) rides along
            ops.extend(x.ops.iter().filter(|o| !matches!(o, MoveOp::Shove { .. } | MoveOp::Explode { .. } | MoveOp::Salvo { .. } | MoveOp::SlamOnLand { .. })).cloned());
            how = "the attack lands as a slam where you come down";
        } else if let (Some(speed), true) = (m.ops.iter().find_map(dash_speed), xp.melee && !xp.projectile) {
            ops.push(MoveOp::Pounce { range: 28.0, speed: speed.clamp(12.0, 40.0), cone: 20.0, damage: dmg, knockback: kb, element: el, up, bounce: 3.0, secs: 0.8 });
            ops.extend(m.ops.iter().filter(|o| dash_speed(o).is_none()).cloned());
            ops.extend(x.ops.iter().filter(|o| !matches!(o, MoveOp::Shove { .. } | MoveOp::Finisher { .. } | MoveOp::Barrage { .. })).cloned());
            how = "a homing lunge that strikes on contact";
        } else {
            ops.extend(m.ops.iter().cloned());
            let (secs, _) = chain::active_phase(m);
            after = Some((secs.clamp(0.1, 2.0), x.ops.clone()));
            how = "the attack fires when the movement ends";
        }
    } else if pa.only_field() != pb.only_field() {
        // 3. a field + anything: the thing leaves the field where it lands
        let (fd, x) = if pa.only_field() { (&a.def, &b.def) } else { (&b.def, &a.def) };
        ops.extend(x.ops.iter().cloned());
        ops.extend(fd.ops.iter().cloned().map(place_at_aim));
        how = "it leaves the field where it lands";
    } else if ((pa.projectile || pa.control) && !pa.melee && pb.melee) || ((pb.projectile || pb.control) && !pb.melee && pa.melee) {
        // 4. a projectile / an affliction + a strike: an imbued strike (the strike takes its
        //    element and behaviours; an affliction rides along onto whoever it hits)
        let (p, m) = if (pa.projectile || pa.control) && !pa.melee { (&a.def, &b.def) } else { (&b.def, &a.def) };
        let (pel, pmech) = match p.ops.iter().find_map(|o| match o {
            MoveOp::Cast { recipe, .. } => Some((recipe.element, recipe.mech.clone())),
            _ => None,
        }) {
            Some(x) => x,
            None => (element_of(p), vec![]),
        };
        let pel = if pel == Element::Light { element_of(p) } else { pel };
        let k = f.imbue_damage;
        for op in &m.ops {
            ops.push(match op.clone() {
                MoveOp::Shove { range, angle, damage, knockback, up, .. } => MoveOp::Shove {
                    range: range * if pmech.contains(&Mechanic::Pierce) { 1.4 } else { 1.15 },
                    angle: if pmech.contains(&Mechanic::Spin) { 179.0 } else { angle },
                    damage: damage * k,
                    knockback,
                    element: pel,
                    up,
                },
                MoveOp::Finisher { lunge, range, angle, damage, knockback, up, window, mult, fx, .. } => {
                    MoveOp::Finisher { lunge, range, angle, damage: damage * k, knockback, element: pel, up, window, mult, fx }
                }
                MoveOp::Ram { speed, secs, damage, knockback, up, pin, through, wall, pitch, steer, keep, .. } => MoveOp::Ram {
                    speed,
                    secs,
                    damage: damage * k,
                    knockback,
                    up,
                    pin,
                    through: through || pmech.contains(&Mechanic::Pierce),
                    wall,
                    pitch,
                    steer,
                    keep,
                    element: pel,
                },
                MoveOp::Drill { speed, secs, grind, damage, knockback, up, .. } => MoveOp::Drill { speed, secs, grind, damage: damage * k, knockback, element: pel, up },
                MoveOp::Pounce { range, speed, cone, damage, knockback, up, bounce, secs, .. } => {
                    MoveOp::Pounce { range, speed, cone, damage: damage * k, knockback, element: pel, up, bounce, secs }
                }
                MoveOp::Barrage { hits, interval, range, angle, damage, knockback, finish, advance, up, blink, .. } => {
                    MoveOp::Barrage { hits, interval, range, angle, damage: damage * k, knockback, finish, advance, up, blink, element: pel }
                }
                other => other,
            });
        }
        if pmech.contains(&Mechanic::Explodes) {
            let (d, kb, _, _, _) = hit_of(m);
            ops.push(MoveOp::Explode { radius: 3.2, damage: (d * 0.45).max(6.0), knockback: kb.max(12.0), carve: 1.0, element: pel, at: At::Front, dist: 3.0 });
        }
        ops.extend(p.ops.iter().filter(|o| !matches!(o, MoveOp::Cast { .. })).cloned());
        how = "an imbued strike";
    } else {
        // 5. both, a buff first
        let (first, second) = if pb.buff && !pb.attacks() && !pa.buff { (&b.def, &a.def) } else { (&a.def, &b.def) };
        ops.extend(first.ops.iter().cloned());
        ops.extend(second.ops.iter().cloned());
        how = "both, in order";
    }
    ops.truncate(12);

    let mut def = a.def.clone();
    def.name = format!("{} {}", a.def.name, b.def.name);
    def.ops = ops;
    def.cooldown = (a.def.cooldown.max(b.def.cooldown) * f.cooldown).clamp(0.05, 120.0);
    def.cost = a.def.cost.max(b.def.cost);
    def.tags.clear();
    if def.fx.is_empty() {
        def.fx = b.def.fx.clone();
    }
    let crit = a.def.crit.or(b.def.crit);
    def.crit = crit;
    let title = format!("{}{}{}", a.title, f.title_join, b.title);
    let (ja, jb) = (a.def.juice.clone().unwrap_or_default(), b.def.juice.clone().unwrap_or_default());
    def.juice = Some(MoveJuice {
        ripple: ja.ripple.max(jb.ripple).max(3.0),
        at: ja.at,
        dist: ja.dist,
        boom: true,
        punch: (ja.punch.max(jb.punch) + 1.0).min(8.0),
        shake: (ja.shake.max(jb.shake) + 0.05).min(0.4),
        title: Some(title.clone()),
        color: mix(ja.color, jb.color),
        debris: ja.debris.max(jb.debris),
        impact_frame: ja.impact_frame.or(jb.impact_frame),
    });
    Twisted {
        def,
        fan: if a.fan.len() >= b.fan.len() { a.fan.clone() } else { b.fan.clone() },
        shots: if a.shots.len() >= b.shots.len() { a.shots.clone() } else { b.shots.clone() },
        interval: a.interval.max(b.interval),
        after,
        tier: a.tier.max(b.tier).saturating_add(f.tier),
        sparks: a.sparks.saturating_add(b.sparks).saturating_add(f.sparks),
        element: Some(ea),
        size: a.size.max(b.size) * if merged_orb { f.merge_size } else { 1.0 },
        merge: Some((ea, eb)),
        title,
        hands: a.hands.max(b.hands),
        how,
    }
}
