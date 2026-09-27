//! Firing a twisted / fused move (host-authoritative, deterministic).
//!
//! `ToolStep::Twist` → [`World::fire_twist`]: the host rebuilds the move from the step (move refs
//! + modifier ids — nothing numeric comes from the client), validates it like a borrowed move
//! (alive, powers on, not stunned / locked, the base moves' cooldowns), prices it ([`price`]), runs
//! shot 0 (every simultaneous copy along its fan yaw), pays, cools the BASE move(s) and emits
//! `PowerEvent::Fired` (the base move ref) + `OpEvent::Twisted` (what it became, for
//! presentation). Further shots / the fused "after" stage are queued at the FRONT of the player's
//! toolbox queue as host-only follow-ups (`shot > 0`, free), each firing when the previous one's
//! window ends — a new command cuts them short like any chain.

use std::sync::OnceLock;

use bd_core::mechanics::{Element, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::{Quat, Vec3};

use super::fuse::fuse;
use super::ops::{gather, twist_gathered, twist_recipe, Twisted};
use super::{ModTable, TwistBase, TwistSide, TwistStep};
use crate::events::{CastError, GameEvent};
use crate::powers::chain::{self, BorrowCd, FireHow, OpKind, ToolStep, Toolbox, LAND_WAIT_MAX};
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

/// Everything the host needs to fire a twisted move.
#[derive(Clone, Debug)]
pub struct Plan {
    pub tw: Twisted,
    /// The power whose move it is (ops that need one; a synthetic one for casts).
    pub def: &'static PowerDef,
    /// Base moves whose cooldown / staleness slot it uses (A, then B for a fusion).
    pub keys: [Option<(MoveRef, bool)>; 2],
    /// A lone famous cast (fired through the cast pipeline with its full recipe).
    pub cast: Option<CastPlan>,
}

impl Plan {
    /// Stages: the shots, then the fused "after" stage if any.
    pub fn stages(&self) -> usize {
        self.tw.shots.len() + self.tw.after.is_some() as usize
    }
}

/// A stand-in power for moves that aren't a table power's (a famous cast): no transform, nothing.
fn synthetic_power() -> &'static PowerDef {
    static P: OnceLock<PowerDef> = OnceLock::new();
    P.get_or_init(|| ron::from_str::<PowerDef>(r#"(id: "twisted_cast", name: "Twisted Cast", kind: Technique)"#).expect("static stand-in power parses"))
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
        costume: false,
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

fn cd_slot(tb: &mut Toolbox, mv: MoveRef) -> &mut BorrowCd {
    let k = mv.pack() + 1;
    if let Some(i) = tb.cds.iter().position(|c| c.mv == k) {
        return &mut tb.cds[i];
    }
    let i = tb.cds.iter().position(|c| c.mv == 0).unwrap_or_else(|| {
        let mut best = 0;
        for (j, c) in tb.cds.iter().enumerate() {
            let b = &tb.cds[best];
            if c.left < b.left || (c.left == b.left && c.since > b.since) {
                best = j;
            }
        }
        best
    });
    tb.cds[i] = BorrowCd { mv: k, ..Default::default() };
    &mut tb.cds[i]
}

impl World {
    /// One side's move: (power, move, key).
    fn twist_side(&self, i: usize, s: &TwistSide) -> Result<(Option<&'static PowerDef>, MoveDef, Option<(MoveRef, bool)>, Option<CastPlan>), PowerError> {
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
                Ok((Some(def), m.clone(), Some((*mv, *kit)), None))
            }
            TwistBase::Held(sel) => {
                let Some((r, def, m)) = held_move(&self.players[i].power, *sel) else { return Err(PowerError::NoSuchMove) };
                if !crate::powers::toolbox::borrowable(m) {
                    return Err(PowerError::NoSuchMove);
                }
                Ok((Some(def), m.clone(), Some((r, false)), None))
            }
            TwistBase::Cast { recipe, shape, signature } => {
                let sig = signature.map(|s| s.sanitized());
                Ok((None, cast_move(recipe, sig), None, Some(CastPlan { recipe: (**recipe).clone(), shape: shape.clone(), signature: sig })))
            }
        }
    }

    /// Rebuild the plan of a twist step (deterministic: every shot rebuilds the same plan).
    pub fn twist_plan(&self, i: usize, step: &TwistStep) -> Result<Plan, PowerError> {
        let t = ModTable::shipped();
        let (da, ma, ka, ca) = self.twist_side(i, &step.a)?;
        let ga = gather(t, &step.a.mods);
        let mut tw = twist_gathered(t, &ma, &ga);
        let mut cast = ca.map(|mut c| {
            twist_recipe(&mut c.recipe, &ga);
            c
        });
        let mut keys = [ka, None];
        let mut def = da;
        if let Some(b) = &step.b {
            let (db, mb, kb, _) = self.twist_side(i, b)?;
            let gb = gather(t, &b.mods);
            let twb = twist_gathered(t, &mb, &gb);
            tw = fuse(t, &tw, &twb);
            keys[1] = kb.filter(|k| Some(k.0) != keys[0].map(|x| x.0));
            def = def.or(db);
            // a fused cast is a fused move (its orb is procedural)
            cast = None;
        }
        if !step.mods.is_empty() {
            let g = gather(t, &step.mods);
            let whole = twist_gathered(t, &tw.def, &g);
            // keep the fusion's structure, take the whole-twist's ops / look / copies
            let keep = tw.clone();
            tw = Twisted {
                after: keep.after,
                merge: keep.merge.or(whole.merge),
                tier: keep.tier.saturating_add(whole.tier),
                sparks: keep.sparks.saturating_add(whole.sparks),
                size: keep.size * whole.size,
                element: whole.element.or(keep.element),
                ..whole
            };
            if let Some(c) = cast.as_mut() {
                twist_recipe(&mut c.recipe, &g);
            }
        }
        Ok(Plan { tw, def: def.unwrap_or_else(synthetic_power), keys, cast })
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
                if let Some((m, _)) = key
                    && let Some(c) = tb.cd_of(*m)
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
                let (n, extra) = price(&self.cfg.tuning.willpower.sparks, &plan.tw, uses, rules.pool_segments());
                cost = n as f32 * rules.segment_size();
                surcharge = extra as f32 * rules.segment_size();
            }
            if cost > self.players[i].willpower + 1e-4 {
                return Err(PowerError::NotEnoughWillpower);
            }
        }
        let from = self.players[i].mv.pos;
        let (eye, look) = match aim {
            Some(a) => self.resolve_aim(i, &a),
            None => self.eye_look(i),
        };
        // ---- run this stage
        let r = if stage < plan.tw.shots.len() {
            self.twist_shot(i, &plan, stage, eye, look)
        } else {
            self.twist_after(i, &plan, eye, look)
        };
        r?;
        if first {
            let cd = plan.tw.def.cooldown * effects::cooldown_mult(&self.players[i].power);
            let p = &mut self.players[i];
            p.willpower = (p.willpower - cost).max(0.0);
            p.willpower = (p.willpower - surcharge).max(0.0);
            for (m, _) in plan.keys.iter().flatten() {
                let slot = cd_slot(&mut p.power.toolbox, *m);
                let repeat = slot.since <= window && slot.left <= 0.0 && slot.full > 0.0;
                slot.uses = if repeat { slot.uses.saturating_add(1) } else { 1 };
                slot.left = cd;
                slot.full = cd.max(0.05);
                slot.since = 0.0;
            }
            let to = self.players[i].mv.pos;
            let fired_step = self.players[i].power.toolbox.fired;
            let element = plan.tw.element.unwrap_or_else(|| super::ops::element_of(&plan.tw.def));
            if let Some((mr, kit)) = plan.keys[0] {
                let how = if kit { FireHow::Kit } else { FireHow::Borrowed };
                let op = plan.tw.def.ops.iter().find(|o| !matches!(o, MoveOp::Transform | MoveOp::Tier(_))).map_or(OpKind::Other, chain::op_kind);
                let sparks = ((cost + surcharge) / self.cfg.rules.segment_size()).round() as u8;
                self.inject_event(GameEvent::Power(PowerEvent::Fired { player, mv: mr, how, op, element, from, to, dir: look, step: fired_step, sparks }));
            }
            let mut mods = [0u8; 3 * super::MAX_MODS];
            let empty: Vec<u8> = Vec::new();
            let groups = [&step.a.mods, step.b.as_ref().map_or(&empty, |b| &b.mods), &step.mods];
            for (g, list) in groups.iter().enumerate() {
                for (k, n) in list.iter().take(super::MAX_MODS).enumerate() {
                    mods[g * super::MAX_MODS + k] = *n;
                }
            }
            let side_ref = |s: &TwistSide| match &s.base {
                TwistBase::Move { mv, .. } => Some(*mv),
                TwistBase::Held(sel) => held_move(&self.players[i].power, *sel).map(|h| h.0),
                TwistBase::Cast { .. } => None,
            };
            let (ra, rb) = (side_ref(&step.a), step.b.as_ref().and_then(side_ref));
            self.op_event(OpEvent::Twisted {
                player,
                a: ra,
                b: rb,
                mods,
                title: plan.tw.title.clone(),
                element,
                size: plan.tw.size,
                copies: plan.tw.fan.len().min(255) as u8,
                shots: plan.tw.shots.len().min(255) as u8,
                merge: plan.tw.merge.map(|(a, b)| [a, b]),
            });
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

/// Presentation: the twisted move a `Twisted` event names, as a `&'static MoveDef` (built once per
/// distinct twist and kept; at most `MAX_INTERNED`, then `None` = present the base move).
pub fn interned(a: MoveRef, b: Option<MoveRef>, mods: &[u8; 3 * super::MAX_MODS]) -> Option<&'static MoveDef> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    const MAX_INTERNED: usize = 1024;
    type Key = (u32, u32, [u8; 3 * super::MAX_MODS]);
    static CACHE: OnceLock<Mutex<HashMap<Key, &'static MoveDef>>> = OnceLock::new();
    let key: Key = (a.pack() + 1, b.map_or(0, |b| b.pack() + 1), *mods);
    let cache = CACHE.get_or_init(Default::default);
    let mut m = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(d) = m.get(&key) {
        return Some(d);
    }
    if m.len() >= MAX_INTERNED {
        return None;
    }
    let t = ModTable::shipped();
    let table = PowerTable::get();
    let group = |g: usize| -> Vec<u8> { mods[g * super::MAX_MODS..(g + 1) * super::MAX_MODS].iter().copied().filter(|x| *x > 0).collect() };
    let (_, ma) = a.resolve(table)?;
    let mut tw = twist_gathered(t, ma, &gather(t, &group(0)));
    if let Some(b) = b {
        let (_, mb) = b.resolve(table)?;
        let twb = twist_gathered(t, mb, &gather(t, &group(1)));
        tw = fuse(t, &tw, &twb);
    }
    let whole = group(2);
    if !whole.is_empty() {
        tw = twist_gathered(t, &tw.def, &gather(t, &whole));
    }
    let d: &'static MoveDef = Box::leak(Box::new(tw.def));
    m.insert(key, d);
    Some(d)
}

/// The element a twisted move shows (presentation helper).
pub fn shown_element(tw: &Twisted) -> Element {
    tw.element.unwrap_or_else(|| super::ops::element_of(&tw.def))
}
