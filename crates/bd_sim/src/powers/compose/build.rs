//! Composed spec → an ordinary [`MoveDef`] of existing ops (docs/toolbox/COMPOSED_MOVE.md §Kinds).
//!
//! The closed set of verb KINDS lives here as code (vocabulary is data: `assets/verbs.ron`). Every
//! kind has sensible numbers for a fair, satisfying move — short escapable control, a clear chain
//! ending (a teleport ends in the air beside them, an uppercut ends rising, a slam ends grounded) —
//! scaled by the verb's `power` / `reach`, the style (element + rider), the modifiers, the place,
//! the target (an explicit target turns a strike into a homing lunge; "everyone" into the area
//! form), the count and the duration. Pure and deterministic: the host and every client expand a
//! spec identically.

use bd_core::mechanics::{Element, Form, Temperament};

use super::{ComposedMove, Place, Rider, Target, VerbKind, VerbTable};
use crate::powers::{At, MoveDef, MoveJuice, MoveOp, RecipeDef};
use crate::status::StatusKind;

/// Everything a builder reads, resolved from the spec.
#[derive(Clone, Copy, Debug)]
pub struct Knobs {
    pub kind: VerbKind,
    /// × damage and knockback.
    pub power: f32,
    /// × range / distance.
    pub reach: f32,
    /// × cooldown.
    pub cd: f32,
    pub element: Element,
    pub rider: Rider,
    pub place: Place,
    pub target: Target,
    pub count: u8,
    /// Said duration (s), if any.
    pub secs: Option<f32>,
    /// "giant" & co. said.
    pub big: bool,
}

impl Knobs {
    fn targeted(&self) -> bool {
        self.target.is_explicit()
    }
    fn everyone(&self) -> bool {
        self.target == Target::Everyone
    }
    fn secs_or(&self, d: f32, lo: f32, hi: f32) -> f32 {
        self.secs.unwrap_or(d).clamp(lo, hi)
    }
    fn dmg(&self, d: f32) -> f32 {
        (d * self.power).clamp(0.0, 40.0)
    }
    fn kb(&self, k: f32) -> f32 {
        (k * self.power.sqrt()).clamp(0.0, 45.0)
    }
    fn r(&self, d: f32) -> f32 {
        d * self.reach
    }
}

/// The longest a composed move applies each status to another player (s) — the same caps as
/// `ops/afflict.rs` (control stays theirs: stuns are micro, never a freeze).
fn status_cap(s: StatusKind) -> f32 {
    match s {
        StatusKind::Stun => 1.2,
        StatusKind::Float => 2.5,
        StatusKind::Chill => 3.0,
        StatusKind::Shadow => 3.0,
        StatusKind::Burn | StatusKind::Poison => 4.0,
        StatusKind::Wet => 6.0,
    }
}

/// The element a verb itself carries (freeze = ice…), if any.
fn intrinsic(kind: VerbKind) -> Option<Element> {
    Some(match kind {
        VerbKind::Freeze => Element::Ice,
        VerbKind::Burn => Element::Fire,
        VerbKind::Shock => Element::Electric,
        VerbKind::Poison => Element::Poison,
        VerbKind::Blind => Element::Shadow,
        _ => return None,
    })
}

/// Default presentation style of a kind (the style's own `fx` wins).
fn kind_fx(kind: VerbKind) -> &'static str {
    match kind {
        VerbKind::Teleport => "hardlight",
        VerbKind::Dash | VerbKind::Charge => "speed",
        VerbKind::Leap | VerbKind::Fly => "air",
        VerbKind::Phase | VerbKind::Vanish => "ghost",
        VerbKind::Rewind | VerbKind::TimeStop => "glitch",
        VerbKind::Swap => "psychic",
        VerbKind::Pull | VerbKind::Push | VerbKind::Lift | VerbKind::Crush => "psychic",
        VerbKind::Freeze => "ice",
        VerbKind::Burn => "fire",
        VerbKind::Shock => "lightning",
        VerbKind::Poison => "poison",
        VerbKind::Blind => "shadow",
        VerbKind::Heal => "holy",
        VerbKind::Stomp | VerbKind::Smash | VerbKind::Slam => "earth",
        _ => "",
    }
}

/// Default cooldown of a kind (s).
fn kind_cd(kind: VerbKind) -> f32 {
    use VerbKind::*;
    match kind {
        Teleport => 0.9,
        Dash => 0.6,
        Charge => 1.6,
        Leap => 1.2,
        Fly => 4.0,
        Dodge => 1.2,
        Phase => 2.5,
        Swap => 1.5,
        Rewind => 6.0,
        Vanish => 6.0,
        Grow | Shrink => 8.0,
        Punch => 0.55,
        Kick => 0.65,
        Uppercut => 0.9,
        Slam => 1.4,
        Stomp => 1.6,
        Smash => 1.6,
        Slash => 0.8,
        Flurry => 2.5,
        Spin => 1.2,
        Grab => 1.5,
        Pull => 1.4,
        Push => 1.2,
        Lift | Crush => 2.5,
        Explode => 5.0,
        Freeze | Burn | Shock | Poison => 2.0,
        Blind | Stun => 3.0,
        Guard | Counter | Reflect | Absorb | Copy => 3.0,
        Clone => 6.0,
        Heal => 10.0,
        Harden => 10.0,
        TimeStop => 12.0,
    }
}

/// Resolve the knobs of a (sanitized) spec in `t`.
pub fn knobs_in(spec: &ComposedMove, t: &VerbTable) -> Option<Knobs> {
    let v = t.verb(spec.verb)?;
    let style = t.style(spec.style);
    let mut power = v.power;
    let mut reach = v.reach;
    let mut cd = 1.0;
    let mut big = false;
    for id in spec.mods.iter().copied() {
        match t.modifier(id).map(|m| m.name.as_str()) {
            Some("big") => {
                power *= 1.3;
                reach *= 1.2;
                cd *= 1.3;
                big = true;
            }
            Some("quick") => {
                power *= 0.8;
                cd *= 0.6;
            }
            Some("long") => reach *= 1.6,
            _ => {}
        }
    }
    let rider = style.map_or(Rider::None, |s| s.rider);
    match rider {
        Rider::Heavy => {
            power *= 1.2;
            cd *= 1.2;
        }
        Rider::Speed => {
            reach *= 1.35;
            cd *= 0.7;
        }
        _ => {}
    }
    let element = intrinsic(v.kind).or(style.map(|s| s.element)).unwrap_or(Element::Light);
    Some(Knobs {
        kind: v.kind,
        power: power.clamp(0.3, 2.5),
        reach: reach.clamp(0.3, 2.5),
        cd,
        element,
        rider,
        place: spec.place,
        target: spec.target,
        count: spec.count.clamp(1, super::MAX_COUNT),
        secs: (spec.secs > 0).then(|| spec.secs as f32 / 10.0),
        big,
    })
}

/// Expand a spec into a move (the shipped table). `None` = unknown verb.
pub fn build(spec: &ComposedMove) -> Option<MoveDef> {
    build_in(spec, VerbTable::get())
}

pub fn build_in(spec: &ComposedMove, t: &VerbTable) -> Option<MoveDef> {
    let spec = spec.sanitized_in(t)?;
    let k = knobs_in(&spec, t)?;
    let v = t.verb(spec.verb)?;
    let ops = ops_of(&k);
    if ops.is_empty() {
        return None;
    }
    let base_cd = if v.cd > 0.0 { v.cd } else { kind_cd(k.kind) };
    let fx = if !v.fx.is_empty() {
        v.fx.clone()
    } else if let Some(s) = t.style(spec.style) {
        s.fx.clone()
    } else {
        kind_fx(k.kind).to_string()
    };
    let cost = match k.kind {
        VerbKind::TimeStop => 40.0,
        VerbKind::Explode | VerbKind::Rewind | VerbKind::Clone => 8.0,
        _ if k.big => 8.0,
        _ => 0.0,
    };
    let juice = k.big.then(|| MoveJuice { punch: 3.0, shake: 0.15, ..Default::default() });
    Some(MoveDef { name: spec.name(), cooldown: (base_cd * k.cd).clamp(0.2, 20.0), cost, ops, juice, fx, tags: vec![], crit: None })
}

// ------------------------------------------------------------------------------------ kinds

fn relocate(k: &Knobs, back: f32, side: f32, up: f32, dist: f32, me: bool) -> MoveOp {
    MoveOp::Relocate { range: k.r(45.0).min(80.0), cone: 20.0, back, side, up, dist, me }
}

/// A small elemental puff where you arrive (styled movement).
fn arrival(k: &Knobs, ops: &mut Vec<MoveOp>) {
    match k.rider {
        Rider::Blind => ops.push(MoveOp::Zone { radius: 3.2, secs: 1.4, status: Some(StatusKind::Shadow), slow: 0.0, dps: 0.0, lockout: false, at: At::Me, dist: 0.0, follow: false }),
        Rider::Gravity => ops.push(MoveOp::Field { strength: 30.0, radius: 5.0, secs: 0.45, at: At::Me, dist: 0.0 }),
        _ if !matches!(k.element, Element::Light | Element::Shadow) => {
            ops.push(MoveOp::Explode { radius: 2.6, damage: k.dmg(3.0), knockback: k.kb(8.0), carve: 0.0, element: k.element, at: At::Me, dist: 0.0 })
        }
        _ => {}
    }
}

/// The target-relative teleport of a place word, if any: (back, side, up).
fn place_offset(place: Place) -> Option<(f32, f32, f32)> {
    Some(match place {
        Place::Behind | Place::Through => (1.8, 0.0, 0.0),
        Place::Front | Place::Toward | Place::Into => (-1.8, 0.0, 0.0),
        Place::Above | Place::Up => (0.0, 0.0, 5.0),
        Place::Onto => (0.0, 0.0, 1.4),
        Place::Beside | Place::Around => (0.0, 2.4, 0.0),
        Place::Below => (-1.2, 0.0, 0.0),
        _ => return None,
    })
}

/// A strike's numbers: (damage, knockback, up, range, angle).
fn strike_numbers(kind: VerbKind) -> (f32, f32, f32, f32, f32) {
    match kind {
        VerbKind::Kick => (9.0, 19.0, 0.45, 3.6, 50.0),
        VerbKind::Uppercut => (9.0, 18.0, 1.6, 3.2, 45.0),
        VerbKind::Smash => (12.0, 22.0, 0.4, 3.4, 55.0),
        _ => (8.0, 16.0, 0.3, 3.2, 40.0),
    }
}

fn strike(k: &Knobs, ops: &mut Vec<MoveOp>) {
    let (d, kb, mut up, range, angle) = strike_numbers(k.kind);
    let mut kbm = 1.0;
    match k.place {
        Place::Up | Place::Above => up += 1.0,
        Place::Away | Place::Through | Place::Forward => kbm = 1.25,
        _ => {}
    }
    let (mut dmg, mut knock) = (k.dmg(d), k.kb(kb) * kbm);
    if k.rider == Rider::Gravity {
        // a gravity strike drives them DOWN instead of away
        knock *= 0.6;
        dmg *= 1.15;
        up = 0.0;
    }
    let el = k.element;
    let count = if k.rider == Rider::Speed { k.count.max(3) } else { k.count };
    if k.everyone() {
        ops.push(MoveOp::Shove { range: k.r(range + 1.0), angle: 180.0, damage: dmg * 0.8, knockback: knock, element: el, up: up.min(1.2) });
    } else if count > 1 {
        let advance = if k.targeted() { 9.0 } else { 4.0 };
        ops.push(MoveOp::Barrage {
            hits: count.min(super::MAX_COUNT) + 1,
            interval: 0.1,
            range: k.r(range),
            angle,
            damage: (dmg * 0.55).max(1.0),
            knockback: 3.0,
            finish: knock,
            advance,
            up: up.min(2.0),
            blink: false,
            element: el,
        });
    } else if k.targeted() {
        // An explicit target: the strike IS a homing lunge (ends airborne off them: chainable).
        let bounce = if k.kind == VerbKind::Uppercut { 11.0 } else { 6.0 };
        ops.push(MoveOp::Pounce { range: k.r(24.0).min(40.0), speed: 36.0, cone: 22.0, damage: dmg, knockback: knock, element: el, up: up.min(2.0), bounce, secs: 0.8 });
    } else {
        ops.push(MoveOp::Shove { range: k.r(range), angle, damage: dmg, knockback: knock, element: el, up: up.min(2.0) });
        // follow-through: a step in, and an uppercut carries you up with it (ends airborne)
        if k.kind == VerbKind::Uppercut {
            ops.push(MoveOp::Impulse { fwd: 3.0, up: 10.0 });
        } else {
            ops.push(MoveOp::Impulse { fwd: 5.0, up: 0.0 });
        }
    }
    if k.kind == VerbKind::Smash && !k.targeted() && !k.everyone() {
        ops.push(MoveOp::Explode { radius: k.r(2.6), damage: 0.0, knockback: 0.0, carve: 1.4, element: el, at: At::Front, dist: 2.4 });
    }
    if k.rider == Rider::Gravity || k.place == Place::Down {
        ops.push(MoveOp::Weigh { range: k.r(6.0), secs: 0.8, down: 18.0 });
    }
}

fn afflict(k: &Knobs, status: StatusKind, secs: f32, dmg: f32, kb: f32, area_zone: bool, ops: &mut Vec<MoveOp>) {
    let el = k.element;
    let cap = status_cap(status);
    let secs = k.secs.unwrap_or(secs).clamp(0.2, cap);
    if k.everyone() {
        ops.push(MoveOp::Explode { radius: k.r(6.0), damage: k.dmg(dmg * 0.8), knockback: k.kb(kb + 4.0), carve: 0.0, element: el, at: At::Me, dist: 0.0 });
        if area_zone {
            ops.push(MoveOp::Zone { radius: k.r(6.0), secs: secs.max(1.5), status: Some(status), slow: if status == StatusKind::Chill { 0.35 } else { 0.0 }, dps: 0.0, lockout: false, at: At::Me, dist: 0.0, follow: false });
        }
    } else {
        ops.push(MoveOp::Afflict { range: k.r(28.0).min(60.0), cone: 12.0, status: Some(status), secs, damage: k.dmg(dmg), knockback: k.kb(kb), up: 0.0, element: el });
    }
}

/// The ops of a kind (empty = nothing sensible).
pub fn ops_of(k: &Knobs) -> Vec<MoveOp> {
    use VerbKind::*;
    let mut ops: Vec<MoveOp> = Vec::with_capacity(4);
    let el = k.element;
    match k.kind {
        // ------------------------------------------------------------------ movement
        Teleport => {
            let untargeted = !k.targeted() && k.target != Target::Ground;
            match k.place {
                Place::Through if untargeted || k.target == Target::Ground => ops.push(MoveOp::PassThrough { depth: k.r(10.0).min(16.0) }),
                Place::Away => ops.push(relocate(k, k.r(9.0).min(24.0), 0.0, 0.0, 0.0, true)),
                Place::Up if untargeted => ops.push(relocate(k, 0.0, 0.0, k.r(7.0).min(14.0), 0.0, true)),
                Place::Down if untargeted => ops.push(MoveOp::Impulse { fwd: 0.0, up: -30.0 }),
                Place::Forward | Place::None if untargeted => {
                    let d = (k.r(12.0) * (1.0 + 0.6 * (k.count as f32 - 1.0))).min(30.0);
                    ops.push(MoveOp::Blink { dist: d, afterimage: true })
                }
                p => {
                    // target-relative (explicit target, terrain point, or the aimed thing)
                    let (back, side, up) = place_offset(p).unwrap_or((-1.8, 0.0, 0.0));
                    ops.push(relocate(k, back, side, up, if untargeted { k.r(12.0).min(30.0) } else { 0.0 }, false));
                }
            }
            arrival(k, &mut ops);
        }
        Dash => {
            let fwd = (k.r(24.0) * (1.0 + 0.5 * (k.count as f32 - 1.0))).min(42.0);
            match k.place {
                Place::Through if k.targeted() || k.target == Target::Aim => {
                    ops.push(MoveOp::Ram { speed: 34.0, secs: 0.35 * k.reach.min(1.6), damage: k.dmg(6.0), knockback: k.kb(12.0), up: 0.2, pin: false, through: true, wall: 0.0, pitch: 0.0, steer: 0.0, keep: 0.4, element: el })
                }
                Place::Behind | Place::Around | Place::Beside if k.targeted() => {
                    let (back, side, up) = place_offset(k.place).unwrap_or((1.8, 0.0, 0.0));
                    ops.push(relocate(k, back, side, up, 0.0, false));
                }
                Place::Away => ops.push(MoveOp::Impulse { fwd: -fwd, up: 5.0 }),
                Place::Up | Place::Above if !k.targeted() => ops.push(MoveOp::Impulse { fwd: 6.0, up: 18.0 }),
                Place::Down => ops.push(MoveOp::Impulse { fwd: 10.0, up: -26.0 }),
                _ if k.targeted() || matches!(k.place, Place::Toward | Place::Into) && k.target != Target::Aim => {
                    ops.push(MoveOp::Pounce { range: k.r(26.0).min(40.0), speed: 34.0, cone: 18.0, damage: k.dmg(6.0), knockback: k.kb(14.0), element: el, up: 0.3, bounce: 7.0, secs: 0.8 })
                }
                _ => ops.push(MoveOp::Impulse { fwd, up: 3.0 }),
            }
            if !matches!(el, Element::Light) && matches!(ops.first(), Some(MoveOp::Impulse { .. })) {
                // an elemental dash scorches / shocks whoever you brush past on the way out
                ops.push(MoveOp::Shove { range: 2.6, angle: 70.0, damage: k.dmg(3.0), knockback: k.kb(6.0), element: el, up: 0.2 });
            }
        }
        Charge => ops.push(MoveOp::Ram {
            speed: 24.0 * k.reach.clamp(0.8, 1.4),
            secs: 0.55,
            damage: k.dmg(9.0),
            knockback: k.kb(20.0),
            up: if k.place == Place::Up { 0.9 } else { 0.3 },
            pin: k.place == Place::Into,
            through: k.place == Place::Through || k.everyone(),
            wall: 8.0,
            pitch: 0.0,
            steer: 0.5,
            keep: 0.3,
            element: el,
        }),
        Leap => {
            match k.place {
                Place::Up if !k.targeted() && k.target != Target::Ground => ops.push(MoveOp::Impulse { fwd: 3.0, up: 20.0 }),
                Place::Away => ops.push(MoveOp::Impulse { fwd: -14.0, up: 14.0 }),
                _ => ops.push(MoveOp::Leap { range: k.r(30.0).min(80.0), apex: 6.0 }),
            }
            if k.rider == Rider::Heavy || k.rider == Rider::Gravity || k.big || k.place == Place::Onto && k.targeted() {
                ops.push(MoveOp::SlamOnLand { radius: k.r(3.5), damage: k.dmg(8.0), knockback: k.kb(16.0), carve: 1.0 });
            }
        }
        Fly => {
            ops.push(MoveOp::Impulse { fwd: 4.0, up: 13.0 });
            ops.push(MoveOp::SelfStatus { status: StatusKind::Float, secs: k.secs_or(2.5, 0.5, 4.0) });
        }
        Dodge => {
            ops.push(MoveOp::Evade { secs: k.secs_or(0.45, 0.2, 0.6), slow: 0.5, speed: 1.5, blink: 0.0 });
            match k.place {
                Place::Away => ops.push(MoveOp::Impulse { fwd: -12.0, up: 3.0 }),
                Place::Forward | Place::Toward | Place::Through => ops.push(MoveOp::Impulse { fwd: 12.0, up: 2.0 }),
                Place::Up | Place::Above => ops.push(MoveOp::Impulse { fwd: 0.0, up: 12.0 }),
                _ => {}
            }
        }
        Phase => match k.place {
            _ if k.target == Target::Ground => ops.push(MoveOp::PassThrough { depth: k.r(10.0).min(16.0) }),
            Place::Through | Place::Into if k.target == Target::Aim => ops.push(MoveOp::PassThrough { depth: k.r(10.0).min(16.0) }),
            Place::Through if k.targeted() => {
                ops.push(MoveOp::Ram { speed: 30.0, secs: 0.3, damage: 0.0, knockback: 0.0, up: 0.0, pin: false, through: true, wall: 0.0, pitch: 0.0, steer: 0.0, keep: 0.6, element: el });
                ops.push(MoveOp::Evade { secs: 0.4, slow: 0.0, speed: 1.0, blink: 0.0 });
            }
            // intangible: hits pass through you (firing a move ends it — come out swinging)
            _ => ops.push(MoveOp::Phase { secs: k.secs_or(1.2, 0.3, 2.5) }),
        },
        Swap => ops.push(MoveOp::Transpose { range: k.r(45.0).min(60.0), cone: 14.0, others: false }),
        Rewind => ops.push(MoveOp::Rewind { secs: k.secs_or(2.0, 0.5, 3.0) }),
        Vanish => {
            ops.push(MoveOp::Zone { radius: 4.5, secs: k.secs_or(2.5, 1.0, 4.0), status: Some(StatusKind::Shadow), slow: 0.0, dps: 0.0, lockout: false, at: At::Me, dist: 0.0, follow: false });
            ops.push(relocate(k, 7.0, 0.0, 0.0, 0.0, true));
        }
        Grow => ops.push(MoveOp::SelfResize { scale: if k.big { 1.6 } else { 1.5 }, secs: k.secs_or(6.0, 2.0, 10.0) }),
        Shrink => ops.push(MoveOp::SelfResize { scale: 0.6, secs: k.secs_or(6.0, 2.0, 10.0) }),
        // ------------------------------------------------------------------ strikes
        Punch | Kick | Uppercut | Smash => strike(k, &mut ops),
        Slam => {
            let (r, d, kb) = (k.r(if matches!(k.rider, Rider::Heavy | Rider::Gravity) { 5.0 } else { 4.0 }), k.dmg(11.0), k.kb(22.0));
            if k.targeted() && matches!(k.place, Place::Onto | Place::Above) {
                ops.push(relocate(k, 0.0, 0.0, 5.0, 0.0, false));
                ops.push(MoveOp::Impulse { fwd: 0.0, up: -34.0 });
            } else if k.targeted() {
                ops.push(MoveOp::Pounce { range: k.r(22.0).min(40.0), speed: 36.0, cone: 20.0, damage: k.dmg(6.0), knockback: k.kb(8.0), element: el, up: 0.0, bounce: 9.0, secs: 0.8 });
            } else {
                ops.push(MoveOp::Impulse { fwd: 2.0, up: -34.0 });
            }
            ops.push(MoveOp::SlamOnLand { radius: r, damage: d, knockback: kb, carve: 1.2 });
        }
        Stomp => ops.push(MoveOp::Explode {
            radius: k.r(if k.everyone() { 6.0 } else { 4.5 }),
            damage: k.dmg(8.0),
            knockback: k.kb(18.0),
            carve: 0.8,
            element: if el == Element::Light { Element::Earth } else { el },
            at: At::Me,
            dist: 0.0,
        }),
        Slash => {
            if k.everyone() {
                ops.push(MoveOp::Shove { range: k.r(4.5), angle: 180.0, damage: k.dmg(8.0), knockback: k.kb(14.0), element: el, up: 0.3 });
            } else if k.count > 1 {
                ops.push(MoveOp::Barrage { hits: k.count + 1, interval: 0.1, range: k.r(3.6), angle: 70.0, damage: k.dmg(4.5), knockback: 3.0, finish: k.kb(14.0), advance: 4.0, up: 0.2, blink: false, element: el });
            } else {
                // a close cut + a travelling crescent (the slash flies on)
                ops.push(MoveOp::Shove { range: k.r(3.8), angle: 70.0, damage: k.dmg(6.0), knockback: k.kb(10.0), element: el, up: 0.2 });
                ops.push(MoveOp::Wave { length: k.r(18.0).min(40.0), speed: 42.0, width: 3.0, carve: 0.35, damage: k.dmg(6.0), knockback: k.kb(12.0), up: 0.2, element: el, thin: false });
            }
        }
        Flurry => {
            let hits = if k.count > 1 { (k.count * 2).min(12) } else { 6 };
            ops.push(MoveOp::Barrage {
                hits,
                interval: if k.rider == Rider::Speed { 0.06 } else { 0.08 },
                range: k.r(3.2),
                angle: 50.0,
                damage: k.dmg(2.2),
                knockback: 3.0,
                finish: k.kb(16.0),
                advance: if k.targeted() { 8.0 } else { 5.0 },
                up: 0.0,
                blink: false,
                element: el,
            });
        }
        Spin => {
            ops.push(MoveOp::Shove { range: k.r(4.2), angle: 180.0, damage: k.dmg(7.0), knockback: k.kb(16.0), element: el, up: 0.35 });
            ops.push(MoveOp::Impulse { fwd: 0.0, up: 4.0 });
        }
        Grab => {
            if k.targeted() && k.power < 1.1 {
                // "grab him" from afar: a hook that reels them in (then chain a throw)
                ops.push(MoveOp::Hook { range: k.r(28.0).min(45.0), speed: 48.0, pull: 24.0, damage: k.dmg(3.0), status: None, secs: 0.0, anchor: false });
            } else {
                let up = match k.place {
                    Place::Up | Place::Above => 1.3,
                    Place::Down | Place::Into => -0.2,
                    _ => 0.5,
                };
                ops.push(MoveOp::Grab { range: k.r(3.6), hold: 0.3, damage: k.dmg(8.0), knockback: k.kb(22.0), up, lift: 0.0, back: matches!(k.place, Place::Behind | Place::Away) });
            }
        }
        // ------------------------------------------------------------------ force
        Pull => {
            if k.everyone() {
                ops.push(MoveOp::Field { strength: 45.0 * k.power.min(1.5), radius: k.r(14.0).min(24.0), secs: 0.7, at: At::Me, dist: 0.0 });
            } else if k.target == Target::Me {
                ops.push(MoveOp::Reel { range: k.r(45.0).min(80.0), speed: 32.0 });
            } else if k.target == Target::Ground {
                ops.push(MoveOp::Haul { range: k.r(30.0).min(60.0), radius: 1.6, speed: 22.0, damage: k.dmg(8.0) });
            } else {
                ops.push(MoveOp::Yank { range: k.r(32.0).min(60.0), speed: 28.0, cone: 10.0 });
            }
        }
        Push => {
            let up = match k.place {
                Place::Up | Place::Above => 1.2,
                _ => 0.35,
            };
            if k.everyone() {
                ops.push(MoveOp::Explode { radius: k.r(7.0).min(14.0), damage: k.dmg(2.0), knockback: k.kb(28.0), carve: 0.0, element: el, at: At::Me, dist: 0.0 });
            } else if k.targeted() {
                ops.push(MoveOp::Afflict { range: k.r(26.0).min(60.0), cone: 12.0, status: None, secs: 0.0, damage: k.dmg(3.0), knockback: k.kb(26.0), up: up * 6.0, element: el });
            } else {
                ops.push(MoveOp::Shove { range: k.r(6.0), angle: 55.0, damage: k.dmg(3.0), knockback: k.kb(26.0), element: el, up });
            }
            if k.place == Place::Down || k.rider == Rider::Gravity {
                ops.push(MoveOp::Weigh { range: k.r(8.0), secs: 0.6, down: 16.0 });
            }
        }
        Lift => {
            if k.everyone() {
                ops.push(MoveOp::Zone { radius: k.r(7.0), secs: k.secs_or(1.5, 0.5, 2.5), status: Some(StatusKind::Float), slow: 0.0, dps: 0.0, lockout: false, at: At::Me, dist: 0.0, follow: false });
            } else {
                ops.push(MoveOp::Afflict { range: k.r(28.0).min(60.0), cone: 12.0, status: Some(StatusKind::Float), secs: k.secs_or(1.4, 0.3, 2.5), damage: 0.0, knockback: 0.0, up: 9.0, element: el });
            }
        }
        Crush => {
            if k.everyone() {
                ops.push(MoveOp::Zone { radius: k.r(7.0), secs: k.secs_or(1.5, 0.5, 3.0), status: Some(StatusKind::Chill), slow: 0.4, dps: 0.0, lockout: false, at: At::Me, dist: 0.0, follow: false });
            } else {
                ops.push(MoveOp::Weigh { range: k.r(28.0).min(60.0), secs: k.secs_or(1.2, 0.3, 2.0), down: 20.0 });
            }
        }
        Explode => ops.push(MoveOp::Explode { radius: k.r(6.0).min(12.0), damage: k.dmg(11.0), knockback: k.kb(26.0), carve: 1.2, element: el, at: At::Me, dist: 0.0 }),
        // ------------------------------------------------------------------ afflict
        Freeze => afflict(k, StatusKind::Chill, 1.8, 4.0, 3.0, true, &mut ops),
        Burn => afflict(k, StatusKind::Burn, 2.5, 5.0, 4.0, false, &mut ops),
        Shock => afflict(k, StatusKind::Stun, 0.5, 6.0, 6.0, false, &mut ops),
        Poison => afflict(k, StatusKind::Poison, 3.0, 3.0, 2.0, true, &mut ops),
        Blind => afflict(k, StatusKind::Shadow, 2.5, 0.0, 0.0, true, &mut ops),
        Stun => afflict(k, StatusKind::Stun, 0.7, 2.0, 2.0, false, &mut ops),
        // ------------------------------------------------------------------ defense / self
        Guard => ops.push(MoveOp::Counter { secs: k.secs_or(0.8, 0.2, 1.5), damage: 0.0, knockback: 0.0, up: 0.0, element: el }),
        Copy => ops.push(MoveOp::CopyLast { range: k.r(45.0).min(60.0), secs: 6.0 }),
        Counter => ops.push(MoveOp::Counter { secs: 0.5, damage: k.dmg(10.0), knockback: k.kb(20.0), up: 0.4, element: el }),
        Absorb => ops.push(MoveOp::Counter { secs: 0.7, damage: k.dmg(5.0), knockback: k.kb(10.0), up: 0.2, element: el }),
        Reflect => ops.push(MoveOp::Deflect { secs: k.secs_or(1.0, 0.3, 2.0), angle: 75.0 }),
        Clone => {
            ops.push(MoveOp::Cast {
                recipe: RecipeDef {
                    name: "clone".into(),
                    form: Form::Biped,
                    element: el,
                    scale: 1.0,
                    mass: 60.0,
                    count: k.count.min(3),
                    temperament: Temperament::Aggressive,
                    ..Default::default()
                },
                modifier: false,
                signature: None,
                merge: None,
            });
            ops.push(relocate(k, 0.0, 3.0, 0.0, 0.0, true));
        }
        Heal => ops.push(MoveOp::Mend { pct: (12.0 * k.power).min(20.0), per_s: 3.0, secs: k.secs_or(3.0, 1.0, 5.0) }),
        Harden => ops.push(MoveOp::Empower { secs: k.secs_or(4.0, 2.0, 6.0), damage: 1.1, armor: 0.6, contact: 0.0, contact_kb: 0.0, contact_cd: 0.0, element: el }),
        TimeStop => ops.push(MoveOp::TimeStop { secs: k.secs_or(1.5, 0.5, 3.0), speed: 1.8 }),
    }
    ops
}
