//! Firing a twisted / fused move (host-authoritative, deterministic).
//!
//! `ToolStep::Twist` → [`World::fire_twist`]: the host rebuilds the move from the step (move refs /
//! a composed spec / a famous cast + modifier bytes — nothing numeric comes from the client),
//! validates it like a borrowed move (alive, powers on, not stunned / locked, the base moves'
//! cooldowns), prices it ([`price`]), resolves a described move's TARGET (VERB GRAMMAR's rules:
//! "homing punch at ethan" swings at Ethan), runs shot 0 (every simultaneous copy along its fan
//! yaw), pays, cools the BASE move(s) and emits `OpEvent::Twisted` (what it became, for
//! presentation) then `PowerEvent::Fired` (the base ref). Further shots / the fused "after" stage are
//! queued at the FRONT of the player's toolbox queue as host-only follow-ups (`shot > 0`, free), each
//! firing when the previous one's window ends — a new command cuts them short like any chain.

use std::sync::OnceLock;

use bd_core::mechanics::{Element, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::{Quat, Vec3, MAX_PLAYERS};

use super::fuse::fuse;
use super::ops::{gather, twist_gathered, twist_recipe, Twisted};
use super::{split_composed, BaseRef, ModTable, TwistBase, TwistSide, TwistStep};
use crate::events::{CastError, GameEvent};
use crate::powers::chain::{self, FireHow, OpKind, ToolStep, LAND_WAIT_MAX};
use crate::powers::compose::{self, ComposedMove, Forced, Target};
use crate::powers::ops::OpEvent;
use crate::powers::toolbox::MoveRef;
use crate::powers::{effects, move_sel, MoveDef, MoveOp, PlayerPower, PowerDef, PowerError, PowerEvent, PowerTable, RecipeDef};
use crate::presence_tier::Tier;
use crate::signature::SignatureCast;
use crate::willpower::SparkTuning;
use crate::world::{Aim, World};

/// A famous cast twisted as a whole (the full recipe keeps its icon model / shape).
#[derive(Clone, Debug, PartialEq)]
pub struct CastPlan {
    pub recipe: Recipe,
    pub shape: Option<ShapeSpec>,
    pub signature: Option<SignatureCast>,
}

/// A cooldown / staleness slot a twisted move uses and how its `Fired` names it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    /// `Toolbox::cds` key: a base move's `MoveRef::pack() + 1`, or a described move's `cd_key()`.
    pub raw: u32,
    /// The ref `Fired` carries (power 0xFFFF — VERB GRAMMAR's `COMPOSED_REF` — for a described move).
    pub mv: MoveRef,
    pub how: FireHow,
}

/// Everything the host needs to fire a twisted move.
#[derive(Clone, Debug)]
pub struct Plan {
    pub tw: Twisted,
    /// The power whose move it is (ops that need one; a stand-in for casts / described moves).
    pub def: &'static PowerDef,
    /// Base moves whose cooldown / staleness slot it uses (A, then B for a fusion).
    pub keys: [Option<Key>; 2],
    /// A lone famous cast (fired through the cast pipeline with its full recipe).
    pub cast: Option<CastPlan>,
    /// A described move's explicit target ("at ethan", "him", "it"), resolved as it fires.
    pub target: Option<Target>,
    /// What the event names as the bases.
    pub refs: [BaseRef; 2],
}

impl Plan {
    /// Stages: the shots, then the fused "after" stage if any.
    pub fn stages(&self) -> usize {
        self.tw.shots.len() + self.tw.after.is_some() as usize
    }
}

/// `Recipe` → the data form ops carry (drops the icon model: fused casts are procedural).
pub fn recipe_def(r: &Recipe) -> RecipeDef {
    RecipeDef {
        name: r.name.clone(),
        form: r.form,
        mech: r.mechanics.clone(),
        element: r.element,
        scale: r.scale,
        mass: r.mass,
        count: r.count,
        features: r.features.clone(),
        temperament: r.temperament,
        intensity: r.intensity,
        payload: r.payload.as_ref().map(|p| {
            let mut d = recipe_def(p);
            d.payload = None;
            Box::new(d)
        }),
        ..Default::default()
    }
}

/// A famous cast as a one-op move (its title = its name).
pub fn cast_move(r: &Recipe, sig: Option<SignatureCast>) -> MoveDef {
    MoveDef {
        name: r.name.to_lowercase(),
        cooldown: 0.05,
        ops: vec![MoveOp::Cast { recipe: recipe_def(r), modifier: false, signature: sig.map(|s| s.kind), merge: sig.and_then(|s| s.merge).map(|[a, b]| (a, b)) }],
        juice: Some(crate::powers::MoveJuice { title: Some(r.name.to_uppercase()), ..Default::default() }),
        ..Default::default()
    }
}

/// The held move a selector names: (ref, power, move).
pub fn held_move(pw: &PlayerPower, sel: u8) -> Option<(MoveRef, &'static PowerDef, &'static MoveDef)> {
    if sel & move_sel::SELECT != 0 || sel == move_sel::CYCLE {
        return None;
    }
    let (held, slot) = move_sel::split_move_of(sel);
    let h = pw.held().nth(held)?;
    let mv = h.def.moves_at(h.tier).get(slot as usize)?;
    let own = h.def.tier(h.tier).is_some_and(|t| !t.moves.is_empty());
    Some((MoveRef::new(h.id, if own { h.tier } else { 0 }, slot), h.def, mv))
}

/// SPARKS of a twisted move: (the ops' price, the surcharge on top of its casts' own prices).
/// The ops' price is the twisted move's own presence tier (bigger ops read bigger) lifted by the
/// modifiers' tier bumps (never into Signature), + extra sparks + the crowd rule for copies, +
/// staleness of the base move; a cast-only move pays its casts plus the bumps and extras.
pub fn price(sp: &SparkTuning, tw: &Twisted, uses: u32, bar: u32) -> (u32, u32) {
    let crowd = if tw.total() >= sp.crowd_count.max(2) as u32 || tw.hands >= sp.crowd_count.max(2) { sp.crowd as u32 } else { 0 };
    let extra = tw.sparks.max(0) as u32 + crowd;
    let bump = tw.tier as i32;
    match chain::move_tier(&tw.def) {
        Some(t) => {
            let k = (t.index() as i32 + bump).clamp(Tier::Tiny.index() as i32, Tier::Titan.index() as i32) as usize;
            let base = sp.tier.get(k).copied().unwrap_or(2) as u32;
            let n = (base + extra + uses.min(64) * sp.stale as u32).clamp(1, bar.max(1));
            (n, 0)
        }
        None => (0, (bump.max(0) as u32 + extra).min(bar)),
    }
}

/// Rotate a look direction about world up by `deg`.
fn yawed(dir: Vec3, deg: f32) -> Vec3 {
    if deg.abs() < 1e-3 {
        return dir;
    }
    (Quat::from_rotation_y(deg.to_radians()) * dir).normalize_or(dir)
}

/// A side's move: (power, move, key, a cast plan, a described target, the event ref, modifier bytes
/// the base carried itself).
type SideMove = (Option<&'static PowerDef>, MoveDef, Option<Key>, Option<CastPlan>, Option<Target>, BaseRef, Vec<u8>);

/// A twisted move built from nothing but data (presentation, tools): the twist of `a` (fused with
/// `b`), each with its own modifier bytes, then the whole's. Deterministic.
pub fn build(t: &ModTable, a: (&MoveDef, &[u8]), b: Option<(&MoveDef, &[u8])>, whole: &[u8]) -> Twisted {
    let mut tw = twist_gathered(t, a.0, &gather(t, a.1));
    if let Some((db, mb)) = b {
        let twb = twist_gathered(t, db, &gather(t, mb));
        tw = fuse(t, &tw, &twb);
    }
    if !whole.is_empty() {
        let w = twist_gathered(t, &tw.def, &gather(t, whole));
        let keep = tw;
        tw = Twisted {
            after: keep.after,
            how: keep.how,
            merge: keep.merge.or(w.merge),
            tier: keep.tier.saturating_add(w.tier),
            sparks: keep.sparks.saturating_add(w.sparks),
            size: keep.size * w.size,
            element: w.element.or(keep.element),
            ..w
        };
    }
    tw
}

impl World {
    fn twist_side(&self, i: usize, s: &TwistSide) -> Result<SideMove, PowerError> {
        match &s.base {
            TwistBase::Move { mv, kit } => {
                let table = PowerTable::get();
                let Some((def, m)) = mv.resolve(table) else { return Err(PowerError::NoSuchMove) };
                if !self.power_allowed(def) {
                    return Err(PowerError::Disabled);
                }
                if !crate::powers::toolbox::borrowable(m) {
                    return Err(PowerError::NoSuchMove);
                }
                let key = Key { raw: mv.pack() + 1, mv: *mv, how: if *kit { FireHow::Kit } else { FireHow::Borrowed } };
                Ok((Some(def), m.clone(), Some(key), None, None, BaseRef::Move(*mv), vec![]))
            }
            TwistBase::Held(sel) => {
                let Some((r, def, m)) = held_move(&self.players[i].power, *sel) else { return Err(PowerError::NoSuchMove) };
                if !crate::powers::toolbox::borrowable(m) {
                    return Err(PowerError::NoSuchMove);
                }
                Ok((Some(def), m.clone(), Some(Key { raw: r.pack() + 1, mv: r, how: FireHow::Borrowed }), None, None, BaseRef::Move(r), vec![]))
            }
            TwistBase::Cast { recipe, shape, signature } => {
                let sig = signature.map(|s| s.sanitized());
                Ok((None, cast_move(recipe, sig), None, Some(CastPlan { recipe: (**recipe).clone(), shape: shape.clone(), signature: sig }), None, BaseRef::None, vec![]))
            }
            TwistBase::Composed(spec) => {
                // its own modifier bytes belong to the twist: applied once
                let (spec, extra) = split_composed(*spec);
                let Some(spec) = spec.sanitized() else { return Err(PowerError::NoSuchMove) };
                let Some(m) = spec.expand() else { return Err(PowerError::NoSuchMove) };
                // compose::exec::COMPOSED_REF (power 0xFFFF never resolves: presentation reads the twist)
                let key = Key { raw: spec.cd_key(), mv: MoveRef::new(u16::MAX, 0, 0), how: FireHow::Composed };
                let target = (spec.target != Target::Aim).then_some(spec.target);
                Ok((Some(compose::composed_power()), m, Some(key), None, target, BaseRef::Composed(spec), extra))
            }
        }
    }

    /// Rebuild the plan of a twist step (deterministic: every shot rebuilds the same plan).
    pub fn twist_plan(&self, i: usize, step: &TwistStep) -> Result<Plan, PowerError> {
        let t = ModTable::shipped();
        let (da, ma, ka, ca, ta, ra, xa) = self.twist_side(i, &step.a)?;
        let mods_a: Vec<u8> = xa.into_iter().chain(step.a.mods.iter().copied()).take(super::MAX_MODS).collect();
        let ga = gather(t, &mods_a);
        let mut cast = ca.map(|mut c| {
            twist_recipe(&mut c.recipe, &ga);
            c
        });
        let mut keys = [ka, None];
        let mut refs = [ra, BaseRef::None];
        let mut def = da;
        let mut target = ta;
        let tw = match &step.b {
            None => build(t, (&ma, &mods_a), None, &step.mods),
            Some(b) => {
                let (db, mb, kb, _, tb, rb, xb) = self.twist_side(i, b)?;
                let mods_b: Vec<u8> = xb.into_iter().chain(b.mods.iter().copied()).take(super::MAX_MODS).collect();
                keys[1] = kb.filter(|k| Some(k.raw) != keys[0].map(|x| x.raw));
                refs[1] = rb;
                def = def.or(db);
                target = target.or(tb);
                // a fused cast is a fused move (its orb is procedural)
                cast = None;
                build(t, (&ma, &mods_a), Some((&mb, &mods_b)), &step.mods)
            }
        };
        if !step.mods.is_empty()
            && let Some(c) = cast.as_mut()
        {
            twist_recipe(&mut c.recipe, &gather(t, &step.mods));
        }
        Ok(Plan { tw, def: def.unwrap_or_else(compose::composed_power), keys, cast, target, refs })
    }

    /// Fire a twisted move (see the module docs).
    pub(crate) fn fire_twist(&mut self, i: usize, step: TwistStep, aim: Option<Aim>) -> Result<(), PowerError> {
        let player = self.players[i].id;
        let p = &self.players[i];
        if !p.alive {
            return Err(PowerError::Dead);
        }
        if !self.cfg.rules.powers_enabled {
            return Err(PowerError::Disabled);
        }
        if p.status.stunned() || p.oath.reciting() || p.charge.active || p.seat.is_some_and(|s| !s.suit) || self.locked_at(i) {
            return Err(PowerError::Busy);
        }
        let plan = self.twist_plan(i, &step)?;
        let stage = step.shot as usize;
        if stage >= plan.stages() {
            return Ok(());
        }
        let first = stage == 0;
        // ---- price + cooldown (the command only)
        let mut cost = 0.0f32;
        let mut surcharge = 0.0f32;
        let window = self.cfg.tuning.willpower.stale_window;
        if first {
            let tb = &self.players[i].power.toolbox;
            let mut uses = 0u32;
            for (k, key) in plan.keys.iter().enumerate() {
                if let Some(key) = key
                    && let Some(c) = tb.cd_of_key(key.raw)
                {
                    if c.left > 0.0 {
                        return Err(PowerError::Cooldown);
                    }
                    if k == 0 && c.since <= window {
                        uses = c.uses as u32;
                    }
                }
            }
            let rules = &self.cfg.rules;
            if !rules.infinite_willpower() {
                let (n, extra) = price(&rules.sparks(&self.cfg.tuning.willpower.sparks), &plan.tw, uses, rules.pool_segments());
                cost = n as f32 * rules.segment_size();
                surcharge = extra as f32 * rules.segment_size();
            }
            if cost > self.players[i].willpower + 1e-4 {
                return Err(PowerError::NotEnoughWillpower);
            }
        }
        let from = self.players[i].mv.pos;
        let (eye, mut look) = match aim {
            Some(a) => self.resolve_aim(i, &a),
            None => self.eye_look(i),
        };
        // ---- a described move's target: every copy swings at it (VERB GRAMMAR's rules)
        let forced = plan.target.and_then(|tg| self.compose_resolve(i, tg));
        if let Some(pt) = forced.and_then(|f| self.forced_point(f)) {
            let d = pt - eye;
            if d.length_squared() > 0.01 {
                look = d.normalize();
            }
        }
        if i < MAX_PLAYERS && plan.target.is_some() {
            let m = &mut self.powers.ops.compose[i];
            if m.owner != Some(player) {
                *m = compose::ComposeMemo { owner: Some(player), ..Default::default() };
            }
            m.now = forced;
        }
        // ---- run this stage
        let r = if stage < plan.tw.shots.len() {
            self.twist_shot(i, &plan, stage, eye, look)
        } else {
            self.twist_after(i, &plan, eye, look)
        };
        if i < MAX_PLAYERS && plan.target.is_some() {
            self.powers.ops.compose[i].now = None;
        }
        r?;
        // remember who / what it went for ("him" next time, `Same` in a chain)
        if i < MAX_PLAYERS && matches!(forced, Some(Forced::Player(_) | Forced::Construct(_))) {
            let tick = self.tick;
            let m = &mut self.powers.ops.compose[i];
            m.last = forced;
            m.last_tick = tick;
        }
        if first {
            let cd = plan.tw.def.cooldown * effects::cooldown_mult(&self.players[i].power);
            let p = &mut self.players[i];
            p.willpower = (p.willpower - cost).max(0.0);
            p.willpower = (p.willpower - surcharge).max(0.0);
            for key in plan.keys.iter().flatten() {
                let slot = p.power.toolbox.slot_for_key(key.raw);
                let repeat = slot.since <= window && slot.left <= 0.0 && slot.full > 0.0;
                slot.uses = if repeat { slot.uses.saturating_add(1) } else { 1 };
                slot.left = cd;
                slot.full = cd.max(0.05);
                slot.since = 0.0;
            }
            let to = self.players[i].mv.pos;
            let fired_step = self.players[i].power.toolbox.fired;
            let element = plan.tw.element.unwrap_or_else(|| super::ops::element_of(&plan.tw.def));
            let mut mods = [0u8; 3 * super::MAX_MODS];
            let (xa, xb) = (composed_mods(&step.a), step.b.as_ref().map(composed_mods).unwrap_or_default());
            let empty: Vec<u8> = Vec::new();
            let ga: Vec<u8> = xa.iter().chain(step.a.mods.iter()).copied().collect();
            let gb: Vec<u8> = xb.iter().chain(step.b.as_ref().map_or(&empty, |b| &b.mods).iter()).copied().collect();
            for (g, list) in [&ga, &gb, &step.mods].iter().enumerate() {
                for (k, n) in list.iter().take(super::MAX_MODS).enumerate() {
                    mods[g * super::MAX_MODS + k] = *n;
                }
            }
            self.op_event(OpEvent::Twisted {
                player,
                a: plan.refs[0],
                b: plan.refs[1],
                mods,
                title: plan.tw.title.clone(),
                element,
                size: plan.tw.size,
                copies: plan.tw.fan.len().min(255) as u8,
                shots: plan.tw.shots.len().min(255) as u8,
                merge: plan.tw.merge.map(|(a, b)| [a, b]),
            });
            // `Fired` right AFTER `Twisted`: presentation reads the twist, then plays the beat
            if let Some(key) = plan.keys[0] {
                let op = plan.tw.def.ops.iter().find(|o| !matches!(o, MoveOp::Transform | MoveOp::Tier(_))).map_or(OpKind::Other, chain::op_kind);
                let sparks = ((cost + surcharge) / self.cfg.rules.segment_size()).round() as u8;
                self.inject_event(GameEvent::Power(PowerEvent::Fired { player, mv: key.mv, how: key.how, op, element, from, to, dir: look, step: fired_step, sparks }));
            }
        }
        // ---- the next stage (host-only follow-up at the front of the queue)
        let next = stage + 1;
        if next < plan.stages() {
            let wait = if next < plan.tw.shots.len() { plan.tw.interval } else { plan.tw.after.as_ref().map_or(0.1, |a| a.0) };
            let mut s = step;
            s.shot = next.min(255) as u8;
            self.players[i].power.toolbox.queue.insert(0, ToolStep::Twist(Box::new(s)));
            self.twist_active(i, wait.max(0.05), false);
        } else {
            let (secs, land) = chain::active_phase(&plan.tw.def);
            self.twist_active(i, secs, land);
        }
        Ok(())
    }

    fn twist_active(&mut self, i: usize, secs: f32, land: bool) {
        let grounded = self.players[i].mv.grounded;
        let tb = &mut self.players[i].power.toolbox;
        tb.active = tb.active.max(secs);
        tb.land = land && !grounded;
        tb.land_cap = if tb.land { LAND_WAIT_MAX } else { 0.0 };
    }

    /// One sequential shot: every simultaneous copy along its fan yaw. Only the command's very
    /// first copy pays for casts (the rest are free copies).
    fn twist_shot(&mut self, i: usize, plan: &Plan, shot: usize, eye: Vec3, look: Vec3) -> Result<(), PowerError> {
        let player = self.players[i].id;
        let prev_free = self.free_cast;
        let mut result = Ok(());
        let shot_yaw = plan.tw.shots.get(shot).copied().unwrap_or(0.0);
        for (c, fan_yaw) in plan.tw.fan.iter().enumerate() {
            let dir = yawed(look, shot_yaw + fan_yaw);
            self.powers.move_aim = Some((i, eye, dir));
            self.free_cast = prev_free || shot > 0 || c > 0;
            let r = match &plan.cast {
                Some(cp) => {
                    let a = Aim::new(eye, dir);
                    let res = match cp.signature {
                        Some(sig) => self.cast_signature(player, cp.recipe.clone(), cp.shape.clone(), a, sig),
                        None => self.cast_shaped(player, cp.recipe.clone(), cp.shape.clone(), None, a),
                    };
                    match res {
                        Ok(_) => Ok(()),
                        Err(CastError::NotEnoughWillpower { .. }) => Err(PowerError::NotEnoughWillpower),
                        Err(_) => Err(PowerError::NoTarget),
                    }
                }
                None => {
                    let mut first: Option<Result<(), PowerError>> = None;
                    for op in plan.tw.def.ops.iter().filter(|o| !matches!(o, MoveOp::Transform | MoveOp::Tier(_))) {
                        let r = self.run_op(i, plan.def, &plan.tw.def, op);
                        if first.is_none() {
                            first = Some(r);
                            if r.is_err() {
                                break;
                            }
                        }
                        if !self.players[i].alive {
                            break;
                        }
                    }
                    first.unwrap_or(Ok(()))
                }
            };
            if c == 0 {
                result = r;
                if result.is_err() {
                    break;
                }
            }
            if !self.players[i].alive {
                break;
            }
        }
        self.free_cast = prev_free;
        self.powers.move_aim = None;
        // a follow-up that fizzled (nothing to hit) is not an error for the chain
        if shot > 0 && matches!(result, Err(PowerError::NoTarget) | Err(PowerError::Busy)) {
            return Ok(());
        }
        result
    }

    /// The fused move's last stage (the attack delivered at the end of the movement).
    fn twist_after(&mut self, i: usize, plan: &Plan, eye: Vec3, look: Vec3) -> Result<(), PowerError> {
        let Some((_, ops)) = &plan.tw.after else { return Ok(()) };
        let prev_free = self.free_cast;
        self.free_cast = true;
        self.powers.move_aim = Some((i, eye, look));
        for op in ops.iter().filter(|o| !matches!(o, MoveOp::Transform | MoveOp::Tier(_))) {
            let _ = self.run_op(i, plan.def, &plan.tw.def, op);
            if !self.players[i].alive {
                break;
            }
        }
        self.powers.move_aim = None;
        self.free_cast = prev_free;
        Ok(())
    }
}

/// The modifier bytes a side's composed base carried itself (moved to the twist).
fn composed_mods(s: &TwistSide) -> Vec<u8> {
    match &s.base {
        TwistBase::Composed(spec) => split_composed(*spec).1,
        _ => vec![],
    }
}

/// A base ref's untwisted move (presentation / tools).
pub fn base_move(r: BaseRef) -> Option<MoveDef> {
    match r {
        BaseRef::None => None,
        BaseRef::Move(m) => m.resolve(PowerTable::get()).map(|(_, d)| d.clone()),
        BaseRef::Composed(spec) => split_composed(spec).0.sanitized().and_then(|s| s.expand()),
    }
}

/// Presentation: the twisted move a `Twisted` event names, as a `&'static MoveDef` (built once per
/// distinct twist and kept; at most `MAX_INTERNED`, then `None` = present the base move).
pub fn interned(a: BaseRef, b: BaseRef, mods: &[u8; 3 * super::MAX_MODS]) -> Option<&'static MoveDef> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    const MAX_INTERNED: usize = 1024;
    type K = (BaseRef, BaseRef, [u8; 3 * super::MAX_MODS]);
    static CACHE: OnceLock<Mutex<HashMap<K, &'static MoveDef>>> = OnceLock::new();
    let key: K = (a, b, *mods);
    let cache = CACHE.get_or_init(Default::default);
    let mut m = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(d) = m.get(&key) {
        return Some(d);
    }
    if m.len() >= MAX_INTERNED {
        return None;
    }
    let t = ModTable::shipped();
    let group = |g: usize| -> Vec<u8> { mods[g * super::MAX_MODS..(g + 1) * super::MAX_MODS].iter().copied().filter(|x| *x > 0).collect() };
    let da = base_move(a)?;
    let db = match b {
        BaseRef::None => None,
        r => Some(base_move(r)?),
    };
    let (ga, gb) = (group(0), group(1));
    let tw = build(t, (&da, &ga), db.as_ref().map(|d| (d, gb.as_slice())), &group(2));
    let d: &'static MoveDef = Box::leak(Box::new(tw.def));
    m.insert(key, d);
    Some(d)
}

/// The element a twisted move shows (presentation helper).
pub fn shown_element(tw: &Twisted) -> Element {
    tw.element.unwrap_or_else(|| super::ops::element_of(&tw.def))
}

/// The composed spec a side carries (tests / tools).
pub fn side_spec(s: &TwistSide) -> Option<ComposedMove> {
    match &s.base {
        TwistBase::Composed(spec) => Some(split_composed(*spec).0),
        _ => None,
    }
}
