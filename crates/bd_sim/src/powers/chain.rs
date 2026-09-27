//! TOOLBOX execution (docs/toolbox/): borrowed / kit one-shots, per-move cooldowns, spark pricing,
//! chains and the one-slot input buffer. Host-authoritative and deterministic; alloc-free per tick
//! (the chain queue allocates only when a chain command arrives).
//!
//! * [`World::toolbox`] runs a list of [`ToolStep`]s (one step = a single command): the first fires
//!   now — or, if the player is still in a move's ACTIVE PHASE, waits for its end (the input
//!   buffer) — and each next step fires when the previous one's active phase ends
//!   ([`active_phase`]: blink arrival, dash end, slam landing, cast release…), re-aimed along the
//!   player's look at that moment, paying as it fires. A new command, a KO, a stun, or running out
//!   of sparks stops the chain cleanly ([`PowerEvent::ChainStopped`]).
//! * [`World::borrow_move`] = one [`ToolStep::Move`]: ANY move of any power / tier / kit, fired as
//!   a one-shot without holding it. Never transforms, never grants (Transform / Tier ops are
//!   skipped), costs sparks by the move's presence tier ([`move_sparks`]) + 1 per repeat inside the
//!   staleness window, and has its own cooldown in a small per-player table ([`Toolbox::cds`]).

use bd_core::mechanics::{Element, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::{PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use super::toolbox::MoveRef;
use super::{effects, move_sel, MoveDef, MoveOp, PowerDef, PowerError, PowerEvent, PowerTable};
use crate::combat::Explosion;
use crate::events::{CastError, GameEvent};
use crate::presence_tier::Tier;
use crate::signature::SignatureCast;
use crate::willpower::SparkTuning;
use crate::world::{Aim, World};

/// Borrowed-move cooldown slots per player (the oldest ready one is reused when full).
pub const BORROW_SLOTS: usize = 8;
/// Longest a chain waits for a "until you land" active phase (a slam) before moving on.
pub const LAND_WAIT_MAX: f32 = 2.5;

/// One borrowed / kit move's cooldown + repeat count (staleness) for one player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorrowCd {
    /// [`MoveRef::pack`] + 1 (0 = free slot).
    pub mv: u32,
    /// Cooldown left / full (s).
    pub left: f32,
    pub full: f32,
    /// Repeats inside the staleness window (each costs +`sparks.stale`).
    pub uses: u8,
    /// Seconds since it last fired.
    pub since: f32,
}

impl BorrowCd {
    pub fn move_ref(&self) -> Option<MoveRef> {
        self.mv.checked_sub(1).map(MoveRef::unpack)
    }
}

/// One step of a toolbox command / chain (travels in bd_game's `NetCmd::Toolbox`; the host
/// validates each step as it fires, exactly like the single command it stands for).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToolStep {
    /// A held power's move selector ([`move_sel`]: a slot, `MOVE_OF`, `SELECT`, `CYCLE`).
    Held(u8),
    /// A borrowed (`kit: false`) or kit (`kit: true`) move.
    Move { mv: MoveRef, kit: bool },
    /// A construct (the lexicon's resolution of the step's phrase, costume summon applied).
    /// `incantation` = a held power's prefix was said; `signature` = a signature cast.
    Cast { recipe: Box<Recipe>, shape: Option<ShapeSpec>, alt_shape: Option<ShapeSpec>, incantation: bool, signature: Option<SignatureCast> },
    /// VERB GRAMMAR: a composed move ("shadow teleport behind ethan"; docs/toolbox/COMPOSED_MOVE.md),
    /// re-sanitized and expanded by the host as it fires.
    Composed(super::compose::ComposedMove),
    /// MODIFIERS & FUSION: a twisted / fused move ("giant rasengan", "rasengan + amaterasu";
    /// powers/twist): base refs + modifier bytes, rebuilt by the host (`World::fire_twist`).
    /// `shot > 0` = the host's own follow-up (reset to 0 on anything a client sends).
    Twist(Box<super::twist::TwistStep>),
}

/// How a move came to fire ([`PowerEvent::Fired`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FireHow {
    /// A held power's move (G, a shout of a held power).
    #[default]
    Held,
    /// A shout of a power / tier the player doesn't hold.
    Borrowed,
    /// "<kit> <move>".
    Kit,
    /// VERB GRAMMAR: a described move (`PowerEvent::Composed` carries its spec).
    Composed,
}

/// The op a move leads with (presentation picks its grammar by it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpKind {
    #[default]
    Cast,
    Impulse,
    Blink,
    Reel,
    Shove,
    Field,
    Room,
    Swap,
    Portal,
    Link,
    Explode,
    Carve,
    TimeStop,
    Restore,
    Transform,
    SelfStatus,
    Weigh,
    SlamOnLand,
    Yank,
    Singularity,
    Tier,
    /// A generic op from `powers/ops/` (TOOLBOX content ops): the move's data names it.
    Other,
}

/// Per-player toolbox state (on [`super::PlayerPower::toolbox`]). All zero / empty when unused.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toolbox {
    pub cds: [BorrowCd; BORROW_SLOTS],
    /// Steps still to run (front = next). Empty unless a chain / buffered command is pending.
    pub queue: Vec<ToolStep>,
    /// Active phase of the last move: seconds left, and "until you land" (a slam) with its cap.
    pub active: f32,
    pub land: bool,
    pub land_cap: f32,
    /// Id of the current chain (bumps per command) and steps of it fired so far.
    pub chain: u16,
    pub fired: u8,
    /// The command's crosshair ray relative to the player's look (origin offset from the eye and
    /// direction, both in the look's right / up / forward basis). Later steps re-aim with it at the
    /// player's CURRENT look: in third person the camera sits off the shoulder and the crosshair
    /// isn't re-sent, so the bare look would miss what the crosshair is on (parallax). `None` =
    /// aim along the look.
    pub aim_rel: Option<[f32; 6]>,
}

impl Toolbox {
    /// Anything to tick?
    pub fn live(&self) -> bool {
        self.active > 0.0 || self.land || !self.queue.is_empty() || self.cds.iter().any(|c| c.mv != 0)
    }

    /// In a move's active phase (a new command waits for its end).
    pub fn busy(&self) -> bool {
        self.active > 0.0 || (self.land && self.land_cap > 0.0)
    }

    pub fn cd_of(&self, mv: MoveRef) -> Option<&BorrowCd> {
        self.cd_of_key(mv.pack() + 1)
    }

    /// A cooldown slot by raw key (`MoveRef::pack() + 1`, or a composed move's `cd_key()`).
    pub fn cd_of_key(&self, k: u32) -> Option<&BorrowCd> {
        self.cds.iter().find(|c| c.mv == k)
    }

    fn slot_for(&mut self, mv: MoveRef) -> &mut BorrowCd {
        self.slot_for_key(mv.pack() + 1)
    }

    pub(crate) fn slot_for_key(&mut self, k: u32) -> &mut BorrowCd {
        if let Some(i) = self.cds.iter().position(|c| c.mv == k) {
            return &mut self.cds[i];
        }
        // a free slot, else the one closest to ready (then the stalest)
        let i = self.cds.iter().position(|c| c.mv == 0).unwrap_or_else(|| {
            let mut best = 0;
            for (j, c) in self.cds.iter().enumerate() {
                let b = &self.cds[best];
                if c.left < b.left || (c.left == b.left && c.since > b.since) {
                    best = j;
                }
            }
            best
        });
        self.cds[i] = BorrowCd { mv: k, ..Default::default() };
        &mut self.cds[i]
    }

    fn stop(&mut self) -> bool {
        let had = !self.queue.is_empty();
        self.queue.clear();
        had
    }
}

/// Client view of the local player's toolbox (`OwnState::toolbox`, presentation / HUD).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolboxSnap {
    /// Recent borrowed / kit moves: (packed [`MoveRef`] + 1, cooldown left 0..=255 of full,
    /// repeats inside the staleness window). 0 = empty.
    pub cds: [(u32, u8, u8); BORROW_SLOTS],
    /// Pending chain steps (front first): a borrowed / kit move = packed [`MoveRef`] + 1; a held
    /// selector = `0x8000_0000 | sel`; a cast = `0xC000_0000`. 0 = none.
    pub queue: [u32; MAX_QUEUE_VIEW],
    /// Steps pending in total (may exceed the view).
    pub pending: u8,
    /// Active phase left (1/100 s, saturating).
    pub active: u8,
    /// Current chain id and steps fired of it.
    pub chain: u16,
    pub fired: u8,
}

/// Queue entries shown in [`ToolboxSnap::queue`].
pub const MAX_QUEUE_VIEW: usize = 8;
pub const QUEUE_HELD: u32 = 0x8000_0000;
pub const QUEUE_CAST: u32 = 0xC000_0000;
/// A composed move pending in [`ToolboxSnap::queue`] (VERB GRAMMAR). Its cooldown entry in
/// [`ToolboxSnap::cds`] is `ComposedMove::cd_key()` (bit 31 + power bits 0xFFFF: never resolves).
pub const QUEUE_COMPOSED: u32 = 0xA000_0000;

impl ToolboxSnap {
    /// `None` when idle (nothing on the wire changes for players who never borrow).
    pub fn of(t: &Toolbox) -> Option<Self> {
        if !t.live() && t.chain == 0 {
            return None;
        }
        let mut s = ToolboxSnap::default();
        for (k, c) in t.cds.iter().enumerate() {
            if c.mv != 0 {
                let q = if c.full > 0.0 { ((c.left / c.full).clamp(0.0, 1.0) * 255.0).ceil() as u8 } else { 0 };
                s.cds[k] = (c.mv, q, c.uses);
            }
        }
        for (k, st) in t.queue.iter().take(MAX_QUEUE_VIEW).enumerate() {
            s.queue[k] = match st {
                ToolStep::Held(sel) => QUEUE_HELD | *sel as u32,
                ToolStep::Move { mv, .. } => mv.pack() + 1,
                ToolStep::Cast { .. } => QUEUE_CAST,
                ToolStep::Composed(_) => QUEUE_COMPOSED,
                ToolStep::Twist(t) => match &t.a.base {
                    super::twist::TwistBase::Move { mv, .. } => mv.pack() + 1,
                    super::twist::TwistBase::Held(sel) => QUEUE_HELD | *sel as u32,
                    super::twist::TwistBase::Cast { .. } => QUEUE_CAST,
                    super::twist::TwistBase::Composed(_) => QUEUE_COMPOSED,
                },
            };
        }
        s.pending = t.queue.len().min(255) as u8;
        s.active = (t.active.max(0.0) * 100.0).min(255.0) as u8;
        s.chain = t.chain;
        s.fired = t.fired;
        Some(s)
    }
}

// ------------------------------------------------------------------------------ pure rules

/// The op a move leads with.
pub fn op_kind(op: &MoveOp) -> OpKind {
    match op {
        MoveOp::Cast { .. } => OpKind::Cast,
        MoveOp::Impulse { .. } => OpKind::Impulse,
        MoveOp::Blink { .. } => OpKind::Blink,
        MoveOp::Reel { .. } => OpKind::Reel,
        MoveOp::Shove { .. } => OpKind::Shove,
        MoveOp::Field { .. } => OpKind::Field,
        MoveOp::Room { .. } => OpKind::Room,
        MoveOp::Swap { .. } => OpKind::Swap,
        MoveOp::Portal { .. } => OpKind::Portal,
        MoveOp::Link { .. } => OpKind::Link,
        MoveOp::Explode { .. } => OpKind::Explode,
        MoveOp::Carve { .. } => OpKind::Carve,
        MoveOp::TimeStop { .. } => OpKind::TimeStop,
        MoveOp::Restore { .. } => OpKind::Restore,
        MoveOp::Transform => OpKind::Transform,
        MoveOp::SelfStatus { .. } => OpKind::SelfStatus,
        MoveOp::Weigh { .. } => OpKind::Weigh,
        MoveOp::SlamOnLand { .. } => OpKind::SlamOnLand,
        MoveOp::Yank { .. } => OpKind::Yank,
        MoveOp::Singularity { .. } => OpKind::Singularity,
        MoveOp::Tier(_) => OpKind::Tier,
        #[allow(unreachable_patterns)]
        _ => OpKind::Other,
    }
}

/// The element a move shows (its first op that has one; a cast's recipe element).
pub fn move_element(mv: &MoveDef) -> Element {
    for op in &mv.ops {
        match op {
            MoveOp::Cast { recipe, .. } => return recipe.element,
            MoveOp::Shove { element, .. } | MoveOp::Explode { element, .. } => return *element,
            _ => {}
        }
    }
    Element::Light
}

/// ACTIVE PHASE of one op: (seconds, wait until landing). The next chain step / a buffered command
/// fires when every op of the move is done: a blink on arrival, a dash when its burst is spent, a
/// slam when you land, a cast when it's released, a time stop once it has started…
pub fn op_active(op: &MoveOp) -> (f32, bool) {
    match op {
        MoveOp::Blink { .. } => (0.12, false),
        MoveOp::Impulse { up, .. } => (if *up > 6.0 { 0.45 } else { 0.3 }, false),
        MoveOp::Reel { range, speed } => ((range.clamp(1.0, 80.0) / speed.clamp(1.0, 60.0)).clamp(0.25, 0.9), false),
        MoveOp::Shove { .. } => (0.2, false),
        MoveOp::Cast { signature: Some(_), .. } => (0.7, false),
        MoveOp::Cast { .. } => (0.25, false),
        MoveOp::Field { .. } | MoveOp::Room { .. } => (0.25, false),
        MoveOp::Swap { .. } => (0.15, false),
        MoveOp::Portal { .. } => (0.3, false),
        MoveOp::Link { .. } => (0.2, false),
        MoveOp::Explode { radius, .. } => (if *radius > 6.0 { 0.45 } else { 0.25 }, false),
        MoveOp::Carve { .. } => (0.3, false),
        MoveOp::TimeStop { .. } => (0.3, false),
        MoveOp::Restore { .. } => (0.4, false),
        MoveOp::Transform | MoveOp::Tier(_) => (0.3, false),
        MoveOp::SelfStatus { .. } => (0.1, false),
        MoveOp::Weigh { .. } => (0.25, false),
        MoveOp::SlamOnLand { .. } => (0.15, true),
        MoveOp::Yank { .. } => (0.4, false),
        MoveOp::Singularity { .. } => (0.6, false),
        // generic ops (powers/ops/): their own active phase (a ram's flight, a hook's travel…);
        // `World::op_busy` also holds the chain while one is still running
        #[allow(unreachable_patterns)]
        other => (super::ops::active_secs(other).max(0.05), false),
    }
}

/// A move's active phase: the longest op, and "until landing" if any op waits for it.
pub fn active_phase(mv: &MoveDef) -> (f32, bool) {
    mv.ops.iter().map(op_active).fold((0.1f32, false), |(a, l), (b, m)| (a.max(b), l || m))
}

/// PRESENCE TIER of a move, by what it does on screen (the same five tiers casts use): a blink /
/// dash / status is Tiny, a shove / small blast / field Normal, a big blast / trench / restore Big,
/// a time stop / gravity core Titan. Cast ops are priced by the cast pipeline itself (not here).
pub fn move_tier(mv: &MoveDef) -> Option<Tier> {
    let mut t: Option<Tier> = None;
    let mut bump = |x: Tier| t = Some(t.map_or(x, |y| y.max(x)));
    for op in &mv.ops {
        match op {
            MoveOp::Cast { .. } | MoveOp::Transform | MoveOp::Tier(_) => {}
            MoveOp::Impulse { .. } | MoveOp::Reel { .. } | MoveOp::SelfStatus { .. } | MoveOp::Link { .. } => bump(Tier::Tiny),
            MoveOp::Blink { dist, .. } => bump(if *dist > 14.0 { Tier::Normal } else { Tier::Tiny }),
            MoveOp::Shove { damage, range, .. } => bump(if *damage > 18.0 || *range > 8.0 { Tier::Big } else if *damage > 6.0 { Tier::Normal } else { Tier::Tiny }),
            MoveOp::Yank { .. } | MoveOp::Weigh { .. } | MoveOp::Swap { .. } | MoveOp::Room { .. } | MoveOp::Portal { .. } => bump(Tier::Normal),
            MoveOp::Field { radius, strength, .. } => bump(if *radius > 10.0 || strength.abs() > 40.0 { Tier::Big } else { Tier::Normal }),
            MoveOp::Explode { radius, damage, .. } => bump(if *radius >= 9.0 || *damage >= 35.0 {
                Tier::Titan
            } else if *radius > 4.5 || *damage >= 18.0 {
                Tier::Big
            } else {
                Tier::Normal
            }),
            MoveOp::Carve { length, .. } => bump(if *length > 15.0 { Tier::Big } else { Tier::Normal }),
            MoveOp::SlamOnLand { radius, .. } => bump(if *radius > 5.0 { Tier::Big } else { Tier::Normal }),
            MoveOp::Restore { .. } => bump(Tier::Big),
            MoveOp::TimeStop { .. } | MoveOp::Singularity { .. } => bump(Tier::Titan),
            #[allow(unreachable_patterns)]
            _ => bump(Tier::Normal),
        }
    }
    // An authored willpower weight lifts it too (a 40-willpower move is not Tiny).
    if let Some(x) = t {
        let floor = if mv.cost >= 40.0 {
            Tier::Titan
        } else if mv.cost >= 20.0 {
            Tier::Big
        } else if mv.cost >= 8.0 {
            Tier::Normal
        } else {
            Tier::Tiny
        };
        t = Some(x.max(floor));
    }
    t
}

/// SPARKS a borrowed / kit move costs: its tier's base price + `stale` per repeat inside the window,
/// within 1..=bar (0 for a cast-only move: each Cast op pays the cast pipeline's own price).
pub fn move_sparks(t: &SparkTuning, mv: &MoveDef, uses: u32, bar: u32) -> u32 {
    let Some(tier) = move_tier(mv) else { return 0 };
    let base = t.tier.get(tier.index()).copied().unwrap_or(2) as u32;
    (base + uses.min(64) * t.stale as u32).clamp(1, bar.max(1))
}

/// The WILLPOWER a power move costs — held, borrowed or kit, one price model: its presence tier in
/// sparks ([`move_sparks`], + `uses` staleness steps) × the spark size; 0 with infinite willpower.
/// The HUD's gauge preview calls this too, so the bite it shows is the bite you pay.
pub fn move_cost(t: &SparkTuning, rules: &crate::rules::MatchRules, mv: &MoveDef, uses: u32) -> f32 {
    if rules.infinite_willpower() {
        return 0.0;
    }
    move_sparks(&rules.sparks(t), mv, uses, rules.pool_segments()) as f32 * rules.segment_size()
}

/// The move a held selector fires (for its active phase / event), without firing it.
fn held_move(pw: &super::PlayerPower, sel: u8) -> Option<(MoveRef, &'static MoveDef)> {
    if sel & move_sel::SELECT != 0 || sel == move_sel::CYCLE {
        return None;
    }
    let (held, slot) = move_sel::split_move_of(sel);
    let h = pw.held().nth(held)?;
    let mv = h.def.moves_at(h.tier).get(slot as usize)?;
    let has_own = h.def.tier(h.tier).is_some_and(|t| !t.moves.is_empty());
    Some((MoveRef::new(h.id, if has_own { h.tier } else { 0 }, slot), mv))
}

// ------------------------------------------------------------------------------ the world side

impl World {
    /// Run a toolbox command for `player`: one step or a chain (see the module docs). Stops any
    /// chain already pending. The first step fires now unless a move's active phase is still
    /// running (then it waits for it: the input buffer). `aim` (the client's crosshair) aims the
    /// FIRST step if it fires now; later steps re-aim when they fire: the player's look then, with
    /// the command's crosshair offset kept ([`Toolbox::aim_rel`], so third person stays on target).
    pub fn toolbox(&mut self, player: PlayerId, steps: Vec<ToolStep>, aim: Option<Aim>) -> Result<(), PowerError> {
        let Some(i) = self.pidx(player) else { return Err(PowerError::UnknownPlayer) };
        if !self.players[i].alive {
            return Err(PowerError::Dead);
        }
        if steps.is_empty() {
            return Ok(());
        }
        let tb = &mut self.players[i].power.toolbox;
        let stopped = tb.stop();
        tb.chain = tb.chain.wrapping_add(1).max(1);
        tb.fired = 0;
        if stopped {
            self.inject_event(GameEvent::Power(PowerEvent::ChainStopped { player, why: PowerError::Busy }));
        }
        let mut steps = steps;
        steps.truncate(super::toolbox::MAX_CHAIN);
        // MODIFIERS & FUSION: follow-up shots are the host's own, never a command's
        for s in steps.iter_mut() {
            if let ToolStep::Twist(t) = s {
                t.shot = 0;
            }
        }
        // 3P re-aim: remember the crosshair ray relative to the look for the steps that fire later
        let rel = aim.map(|a| self.aim_relative(i, &a));
        self.players[i].power.toolbox.aim_rel = rel;
        if self.players[i].power.toolbox.busy() || self.op_busy(player) {
            // The input buffer: wait for the active phase to end.
            self.players[i].power.toolbox.queue = steps;
            return Ok(());
        }
        let first = steps.remove(0);
        self.players[i].power.toolbox.queue = steps;
        let r = self.fire_step(i, first, aim);
        if let Err(e) = r {
            if stops_chain(e) {
                self.stop_chain(i, e);
            }
        }
        r
    }

    /// Fire ANY move of any power / tier / kit as a one-shot (no chain). See [`Self::toolbox`].
    pub fn borrow_move(&mut self, player: PlayerId, mv: MoveRef, kit: bool, aim: Option<Aim>) -> Result<(), PowerError> {
        self.toolbox(player, vec![ToolStep::Move { mv, kit }], aim)
    }

    /// A new (non-toolbox) command from `player` stops their pending chain (bd_game calls this for
    /// every command it applies, except the toolbox's own).
    pub fn toolbox_interrupt(&mut self, player: PlayerId) {
        if let Some(i) = self.pidx(player) {
            if !self.players[i].power.toolbox.queue.is_empty() {
                self.stop_chain(i, PowerError::Busy);
            }
        }
    }

    /// Read-only: a player's toolbox view (for UI in practice / host; clients read `OwnState`).
    pub fn toolbox_view(&self, player: PlayerId) -> Option<ToolboxSnap> {
        self.player(player).and_then(|p| ToolboxSnap::of(&p.power.toolbox))
    }

    /// The look basis of player `i`: (right, up, forward).
    fn look_basis(&self, i: usize) -> (Vec3, Vec3, Vec3) {
        let f = self.players[i].look().normalize_or(Vec3::NEG_Z);
        let r = f.cross(Vec3::Y).normalize_or(Vec3::X);
        (r, r.cross(f), f)
    }

    /// A (validated) crosshair ray as offsets in the look basis ([`Toolbox::aim_rel`]).
    fn aim_relative(&self, i: usize, a: &Aim) -> [f32; 6] {
        let (o, d) = self.resolve_aim(i, a);
        let eye = self.players[i].eye(&self.cfg.move_params);
        let (r, u, f) = self.look_basis(i);
        let off = o - eye;
        [off.dot(r), off.dot(u), off.dot(f), d.dot(r), d.dot(u), d.dot(f)]
    }

    /// The aim a later chain step fires with: the command's crosshair ray re-applied to the
    /// player's current look (3P parallax stays solved), or `None` (the bare look).
    fn chain_aim(&self, i: usize) -> Option<Aim> {
        let rel = self.players[i].power.toolbox.aim_rel?;
        if rel.iter().any(|x| !x.is_finite()) {
            return None;
        }
        let eye = self.players[i].eye(&self.cfg.move_params);
        let (r, u, f) = self.look_basis(i);
        let origin = eye + r * rel[0] + u * rel[1] + f * rel[2];
        let dir = (r * rel[3] + u * rel[4] + f * rel[5]).normalize_or(f);
        Some(Aim::new(origin, dir))
    }

    fn stop_chain(&mut self, i: usize, why: PowerError) {
        if self.players[i].power.toolbox.stop() {
            let player = self.players[i].id;
            self.inject_event(GameEvent::Power(PowerEvent::ChainStopped { player, why }));
        }
    }

    /// A chain step failed in a way that ends the chain (even if it was the last one).
    fn chain_step_failed(&mut self, i: usize, why: PowerError) {
        self.players[i].power.toolbox.stop();
        let player = self.players[i].id;
        self.inject_event(GameEvent::Power(PowerEvent::ChainStopped { player, why }));
    }

    fn fire_step(&mut self, i: usize, step: ToolStep, aim: Option<Aim>) -> Result<(), PowerError> {
        // FIRST CALL HOOK: a claimed / burned thing fires as its rival / echo version (no-op while off)
        let (step, gate) = self.call_step(i, step);
        let r = self.fire_step_plain(i, step, aim);
        self.call_done(gate, r.is_ok());
        r
    }

    fn fire_step_plain(&mut self, i: usize, step: ToolStep, aim: Option<Aim>) -> Result<(), PowerError> {
        let player = self.players[i].id;
        let r = match step {
            ToolStep::Held(sel) => {
                let mv = held_move(&self.players[i].power, sel);
                let r = self.power_move_aimed(player, sel, aim);
                // `power_move_aimed` emits `Fired` itself; the chain only needs the active phase.
                if r.is_ok() {
                    let (secs, land) = mv.map_or((0.1, false), |(_, m)| active_phase(m));
                    self.set_active(i, secs, land);
                }
                r
            }
            ToolStep::Move { mv, kit } => {
                self.powers.move_aim = aim.map(|a| {
                    let (o, d) = self.resolve_aim(i, &a);
                    (i, o, d)
                });
                let r = self.fire_borrowed(i, mv, kit);
                self.powers.move_aim = None;
                r
            }
            // VERB GRAMMAR (powers/compose/exec.rs): same aim rule; the target may re-aim it.
            ToolStep::Composed(spec) => {
                self.powers.move_aim = aim.map(|a| {
                    let (o, d) = self.resolve_aim(i, &a);
                    (i, o, d)
                });
                let r = self.fire_composed(i, spec);
                self.powers.move_aim = None;
                r
            }
            // MODIFIERS & FUSION (powers/twist/exec.rs): prices, cools and aims itself.
            ToolStep::Twist(t) => self.fire_twist(i, *t, aim),
            ToolStep::Cast { recipe, shape, alt_shape, incantation, signature } => {
                let a = aim.unwrap_or_else(|| {
                    let p = &self.players[i];
                    Aim::new(p.eye(&self.cfg.move_params), p.look())
                });
                let res = match signature {
                    Some(sig) => self.cast_signature(player, *recipe, shape, a, sig),
                    None if incantation => self.cast_incantation_shaped(player, *recipe, shape, alt_shape, a),
                    None => self.cast_shaped(player, *recipe, shape, alt_shape, a),
                };
                match res {
                    Ok(_) => {
                        self.set_active(i, if signature.is_some() { 0.7 } else { 0.25 }, false);
                        Ok(())
                    }
                    Err(CastError::NotEnoughWillpower { .. }) => Err(PowerError::NotEnoughWillpower),
                    Err(_) => Err(PowerError::NoTarget),
                }
            }
        };
        if r.is_ok() {
            let tb = &mut self.players[i].power.toolbox;
            tb.fired = tb.fired.saturating_add(1);
        }
        r
    }

    pub(crate) fn set_active(&mut self, i: usize, secs: f32, land: bool) {
        let grounded = self.players[i].mv.grounded;
        let tb = &mut self.players[i].power.toolbox;
        tb.active = tb.active.max(secs);
        // a slam from the ground has nothing to wait for
        tb.land = land && !grounded;
        tb.land_cap = if tb.land { LAND_WAIT_MAX } else { 0.0 };
    }

    /// A borrowed / kit move (validation, price, cooldown, ops, event, active phase).
    fn fire_borrowed(&mut self, i: usize, mr: MoveRef, kit: bool) -> Result<(), PowerError> {
        let player = self.players[i].id;
        let p = &self.players[i];
        if !p.alive {
            return Err(PowerError::Dead);
        }
        if !self.cfg.rules.powers_enabled {
            return Err(PowerError::Disabled);
        }
        let table = PowerTable::get();
        let Some((def, mv)) = mr.resolve(table) else { return Err(PowerError::NoSuchMove) };
        let def: &'static PowerDef = def;
        let mv: &'static MoveDef = mv;
        if !self.power_allowed(def) {
            return Err(PowerError::Disabled);
        }
        // Never transforms, never switches tiers: those ops are the holder's.
        if mv.ops.iter().all(|o| matches!(o, MoveOp::Transform | MoveOp::Tier(_))) {
            return Err(PowerError::NoSuchMove);
        }
        // TRANSFORMERS FEEL: seated in a transformer, the move is the ROBOT's ("optimus prime combo",
        // "ion blaster": `constructs::brawl::order_for_move`).
        if let Some(r) = self.pilot_move(i, mr, mv, kit) {
            return r;
        }
        let p = &self.players[i];
        // a Lockout (hack / silence) locks borrowed moves too
        if p.status.stunned() || p.oath.reciting() || p.charge.active || p.seat.is_some_and(|s| !s.suit) || self.locked_at(i) {
            return Err(PowerError::Busy);
        }
        let window = self.cfg.tuning.willpower.stale_window;
        let (uses, cooling) = match p.power.toolbox.cd_of(mr) {
            Some(c) => (if c.since <= window { c.uses as u32 } else { 0 }, c.left > 0.0),
            None => (0, false),
        };
        if cooling {
            return Err(PowerError::Cooldown);
        }
        let cost = move_cost(&self.cfg.tuning.willpower.sparks, &self.cfg.rules, mv, uses);
        if cost > p.willpower + 1e-4 {
            return Err(PowerError::NotEnoughWillpower);
        }
        let from = p.mv.pos;
        let (_, dir) = self.eye_look(i);
        // Run the ops (Transform / Tier skipped); the move only counts if the first op did something.
        self.phase_break(i); // SHONEN CLASSICS (ops/phase.rs): striking makes you solid
        let mut first: Option<Result<(), PowerError>> = None;
        for op in mv.ops.iter().filter(|o| !matches!(o, MoveOp::Transform | MoveOp::Tier(_))) {
            let r = self.run_op(i, def, mv, op);
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
        if let Some(Err(e)) = first {
            return Err(e);
        }
        self.note_seen_move(i, mr.power, mr.tier, mr.slot); // SHONEN CLASSICS (ops/copy_last.rs): enemies nearby saw it
        let cd = mv.cooldown * effects::cooldown_mult(&self.players[i].power);
        let p = &mut self.players[i];
        p.willpower = (p.willpower - cost).max(0.0);
        let slot = p.power.toolbox.slot_for(mr);
        let repeat = slot.since <= window && slot.left <= 0.0 && slot.full > 0.0;
        slot.uses = if repeat { slot.uses.saturating_add(1) } else { 1 };
        slot.left = cd;
        slot.full = cd.max(0.05);
        slot.since = 0.0;
        let to = p.mv.pos;
        let step = p.power.toolbox.fired;
        let how = if kit { FireHow::Kit } else { FireHow::Borrowed };
        let op = mv.ops.iter().find(|o| !matches!(o, MoveOp::Transform | MoveOp::Tier(_))).map_or(OpKind::Other, op_kind);
        let element = move_element(mv);
        self.inject_event(GameEvent::Power(PowerEvent::Fired { player, mv: mr, how, op, element, from, to, dir, step, sparks: (cost / self.cfg.rules.segment_size()).round() as u8 }));
        let (secs, land) = active_phase(mv);
        self.set_active(i, secs, land);
        Ok(())
    }

    /// TRANSFORMERS FEEL: a move said while seated in a transformer becomes a robot order
    /// (`constructs::brawl::order`): a cockpit pilot's every move; a suit wearer's combos and
    /// ultimates (the rest fire from your own body — you ARE the transformer). Free (the robot's own
    /// moveset), but the move's cooldown applies; `Fired` still plays its title / juice. None = not
    /// seated in a transformer (the normal rules apply).
    fn pilot_move(&mut self, i: usize, mr: MoveRef, mv: &'static MoveDef, kit: bool) -> Option<Result<(), PowerError>> {
        use crate::constructs::brawl::{self, Order};
        let p = &self.players[i];
        let seat = p.seat?;
        let o = brawl::order_for_move(mv);
        if seat.suit && !matches!(o, Order::Combo | Order::Super) {
            return None;
        }
        if !self.constructs.get(seat.construct).is_some_and(|c| c.gone.is_none() && c.has(bd_core::mechanics::Mechanic::Transforms)) {
            return None;
        }
        if p.status.stunned() || self.locked_at(i) {
            return Some(Err(PowerError::Busy));
        }
        if p.power.toolbox.cd_of(mr).is_some_and(|c| c.left > 0.0) {
            return Some(Err(PowerError::Cooldown));
        }
        let c = self.constructs.get_mut(seat.construct)?;
        if !brawl::order(c, o) {
            return None;
        }
        let busy = brawl::order_secs(c, o, &self.cfg.tuning.transformer);
        let from = c.center();
        let player = self.players[i].id;
        let (_, dir) = self.eye_look(i);
        let cd = mv.cooldown * effects::cooldown_mult(&self.players[i].power);
        let p = &mut self.players[i];
        let slot = p.power.toolbox.slot_for(mr);
        slot.uses = 1;
        slot.left = cd;
        slot.full = cd.max(0.05);
        slot.since = 0.0;
        let step = p.power.toolbox.fired;
        let how = if kit { FireHow::Kit } else { FireHow::Borrowed };
        let op = mv.ops.iter().find(|o| !matches!(o, MoveOp::Transform | MoveOp::Tier(_))).map_or(OpKind::Other, op_kind);
        let element = move_element(mv);
        self.inject_event(GameEvent::Power(PowerEvent::Fired { player, mv: mr, how, op, element, from, to: from, dir, step, sparks: 0 }));
        // A chain's next step waits for the robot to finish this one.
        self.set_active(i, busy, false);
        Some(Ok(()))
    }

    /// The `Fired` event of a held move (called by `power_move_inner` after it ran).
    pub(crate) fn emit_held_fired(&mut self, i: usize, sel_slot: u8, from: Vec3, dir: Vec3) {
        let pw = &self.players[i].power;
        let Some(id) = pw.id else { return };
        let Some(def) = PowerTable::get().def(id) else { return };
        let tier = pw.tier;
        let Some(mv) = def.moves_at(tier).get(sel_slot as usize) else { return };
        let has_own = def.tier(tier).is_some_and(|t| !t.moves.is_empty());
        let mr = MoveRef::new(id, if has_own { tier } else { 0 }, sel_slot);
        let op = mv.ops.first().map_or(OpKind::Other, op_kind);
        let element = move_element(mv);
        let player = self.players[i].id;
        let to = self.players[i].mv.pos;
        let step = pw.toolbox.fired;
        // (the price it just paid: the same spark model as a borrowed move, `move_cost`)
        let sparks = (move_cost(&self.cfg.tuning.willpower.sparks, &self.cfg.rules, mv, 0) / self.cfg.rules.segment_size().max(1e-3)).round() as u8;
        self.inject_event(GameEvent::Power(PowerEvent::Fired { player, mv: mr, how: FireHow::Held, op, element, from, to, dir, step, sparks }));
    }

    /// Per tick (from `step_powers`, before its no-power early return): borrowed cooldowns, active
    /// phases, chains; plus the bits of borrowed moves that a powerless player needs ticked (time
    /// stop, a pending landing slam). Nothing (and no allocation) when nobody has toolbox state.
    pub(crate) fn step_toolbox(&mut self, dt: f32) {
        let any = self.players.iter().any(|p| p.power.toolbox.live() || (p.power.id.is_none() && (p.power.time_stop > 0.0 || p.power.slam.is_some())));
        if !any {
            return;
        }
        let window = self.cfg.tuning.willpower.stale_window;
        for i in 0..self.players.len() {
            // powerless players: the timers step_powers would tick for a holder
            if self.players[i].power.id.is_none() {
                let (pos, grounded, alive, id) = (self.players[i].mv.pos, self.players[i].mv.grounded, self.players[i].alive, self.players[i].id);
                let pw = &mut self.players[i].power;
                pw.time_stop = (pw.time_stop - dt).max(0.0);
                if !alive {
                    pw.slam = None;
                } else if grounded {
                    if let Some([radius, damage, knockback, carve]) = pw.slam.take() {
                        self.explode(Explosion {
                            pos,
                            radius,
                            damage,
                            knockback,
                            source: Some(id),
                            construct: None,
                            element: Element::Earth,
                            carve_radius: carve,
                            self_damage: false,
                        });
                    }
                }
            }
            if !self.players[i].power.toolbox.live() {
                continue;
            }
            let grounded = self.players[i].mv.grounded;
            let slam_pending = self.players[i].power.slam.is_some();
            {
                let tb = &mut self.players[i].power.toolbox;
                for c in tb.cds.iter_mut().filter(|c| c.mv != 0) {
                    c.left = (c.left - dt).max(0.0);
                    c.since += dt;
                    if c.left <= 0.0 && c.since > window {
                        *c = BorrowCd::default();
                    }
                }
                tb.active = (tb.active - dt).max(0.0);
                if tb.land {
                    tb.land_cap = (tb.land_cap - dt).max(0.0);
                    // landed (the slam went off) or waited long enough
                    if (grounded && !slam_pending) || tb.land_cap <= 0.0 {
                        tb.land = false;
                    }
                }
            }
            if self.players[i].power.toolbox.queue.is_empty() {
                continue;
            }
            // KO / stun stop the chain; otherwise the next step fires when the phase ends.
            if !self.players[i].alive {
                self.stop_chain(i, PowerError::Dead);
                continue;
            }
            if self.players[i].status.stunned() {
                self.stop_chain(i, PowerError::Busy);
                continue;
            }
            if self.players[i].power.toolbox.busy() || self.op_busy(self.players[i].id) {
                continue;
            }
            let step = self.players[i].power.toolbox.queue.remove(0);
            // re-aimed at fire time: the player's current look + the command's crosshair offset
            let aim = self.chain_aim(i);
            if let Err(e) = self.fire_step(i, step, aim) {
                if stops_chain(e) {
                    self.chain_step_failed(i, e);
                }
                // a step that fizzled (cooldown, nothing to hit) is skipped: the next one fires
                // on the next tick
            }
        }
    }
}

/// Errors that end a chain (the rest just skip that step).
fn stops_chain(e: PowerError) -> bool {
    matches!(e, PowerError::NotEnoughWillpower | PowerError::Dead | PowerError::Busy | PowerError::UnknownPlayer)
}
