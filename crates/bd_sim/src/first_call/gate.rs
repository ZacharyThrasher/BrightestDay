//! The world side of FIRST CALL / BURN ON DEATH: read a use's identities, decide ([`super::Verdict`]),
//! substitute the rival / echo version, run it, then commit (claim, life log, marks, events). Every
//! entry point is a no-op returning the input unchanged while both rules are off.
//!
//! Hooks (one line each where they sit):
//! * `powers::chain::World::fire_step` — every toolbox step ([`World::call_step`] / [`World::call_done`]);
//! * `World::cast_plain` — every direct cast command ([`World::call_cast`] / [`World::call_done`]);
//! * `World::power_move_aimed` — held moves (G, shouts of a held power) ([`World::call_power_move`]);
//! * `World::gain_power_at` — holding a power claims it ([`World::call_gain`]);
//! * KO ([`World::call_ko`]), respawn ([`World::call_respawn`]), leaving ([`World::call_leave`]),
//!   a new round ([`super::FirstCallWorld::reset`]), construct pricing ([`World::call_cast_sparks`]).
//!
//! Nested work (a move's casts, a twist's shots, a chain's host follow-ups) runs inside the command
//! that was gated (`FirstCallWorld::depth`) and is never gated again.

use bd_core::mechanics::{Element, ModelSource, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};

use super::keys::{CallKey, CallKind, Ident, Identities, composed_key, thing_key};
use super::{BurnRow, BurnScope, CallData, CallEvent, ClaimRow, MarkRow, Verdict, mark};
use crate::events::GameEvent;
use crate::powers::chain::{self, ToolStep};
use crate::powers::toolbox::MoveRef;
use crate::powers::twist::{self, TwistBase, TwistSide, TwistStep};
use crate::powers::{PowerError, PowerTable};
use crate::world::{Aim, World};

/// The identities one use carries: a fusion has two bases.
pub type Uses = [Option<Ident>; 2];

/// A gated command in flight (returned by the `call_*` gates, handed back to [`World::call_done`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pending {
    pub(crate) on: bool,
    pub(crate) player: Option<PlayerId>,
    pub(crate) uses: Uses,
    pub(crate) verdict: Verdict,
    /// `events.len()` when it started (constructs it spawned get their marks).
    pub(crate) ev0: usize,
}

fn named(k: CallKind) -> bool {
    !matches!(k, CallKind::Thing | CallKind::Move)
}

impl World {
    /// Either rule on?
    pub fn call_rules_on(&self) -> bool {
        self.cfg.rules.first_call || self.cfg.rules.burn.on()
    }

    /// How `player` calling `key` would come out right now (tools, bots, presentation on the host).
    pub fn call_verdict(&self, player: PlayerId, key: CallKey) -> Verdict {
        self.first_call.snap.verdict(player, key, self.cfg.rules.first_call, self.cfg.rules.burn.on())
    }

    /// The identity a burn records for a recipe: the named one, or (BURN everything) the lenient one.
    fn call_recipe_use(&self, r: &Recipe, signature: bool) -> Option<Ident> {
        let ids = Identities::shipped();
        ids.of_recipe(r, signature)
            .or_else(|| (self.cfg.rules.burn == BurnScope::Everything).then(|| Ident { key: thing_key(r), kind: CallKind::Thing }))
    }

    /// Identities of a twist side / toolbox step base.
    fn call_base_use(&self, i: usize, b: &TwistBase) -> Option<Ident> {
        let ids = Identities::shipped();
        match b {
            TwistBase::Move { mv, .. } => ids.of_move(*mv),
            TwistBase::Held(sel) => twist::exec::held_move(&self.players[i].power, *sel).and_then(|(r, _, _)| ids.of_move(r)),
            TwistBase::Cast { recipe, signature, .. } => self.call_recipe_use(recipe, signature.is_some()),
            TwistBase::Composed(spec) => (self.cfg.rules.burn == BurnScope::Everything)
                .then(|| Ident { key: composed_key(&twist::split_composed(*spec).0), kind: CallKind::Move }),
        }
    }

    fn call_step_uses(&self, i: usize, step: &ToolStep) -> Uses {
        let ids = Identities::shipped();
        match step {
            ToolStep::Held(sel) => [twist::exec::held_move(&self.players[i].power, *sel).and_then(|(r, _, _)| ids.of_move(r)), None],
            ToolStep::Move { mv, .. } => [ids.of_move(*mv), None],
            ToolStep::Cast { recipe, signature, .. } => [self.call_recipe_use(recipe, signature.is_some()), None],
            ToolStep::Composed(spec) => [self.call_base_use(i, &TwistBase::Composed(*spec)), None],
            ToolStep::Twist(t) => [self.call_base_use(i, &t.a.base), t.b.as_ref().and_then(|b| self.call_base_use(i, &b.base))],
        }
    }

    /// The verdict for `player` using `uses` now (a same-tick claim by a HIGHER id loses the tie: the
    /// caller gets the original and takes the claim on commit).
    fn call_decide(&self, player: PlayerId, uses: &Uses) -> Verdict {
        let fc = self.cfg.rules.first_call;
        let burn = self.cfg.rules.burn.on();
        let mut v = Verdict::default();
        for u in uses.iter().flatten() {
            if fc
                && named(u.kind)
                && v.rival.is_none()
                && let Ok(k) = self.first_call.snap.claims.binary_search_by_key(&u.key, |c| c.key)
            {
                let c = self.first_call.snap.claims[k];
                let tie_won = c.tick == self.tick && player < c.owner;
                if c.owner != player && !tie_won {
                    v.rival = Some(c.owner);
                }
            }
            if burn && self.first_call.snap.burned(player, u.key) {
                v.echo = true;
            }
        }
        v
    }

    /// The modifiers a rival / echo move gets (`element` = the base move's).
    fn call_mods(&self, v: Verdict, element: Element) -> Vec<u8> {
        let d = CallData::shipped();
        let mut m = Vec::new();
        if v.rival.is_some() {
            m.extend(d.rival_mods(element));
        }
        if v.echo {
            m.extend(d.echo_mods());
        }
        m
    }

    /// Rival / echo a construct recipe in place (never pricier: only the element, the costume's dress
    /// and — echo — its size change).
    fn call_recipe(&mut self, r: &mut Recipe, v: Verdict) {
        let d = CallData::shipped();
        if v.rival.is_some() {
            r.element = d.rival_element(r.element);
            let words = d.rival.costume_words.trim();
            if !words.is_empty()
                && let ModelSource::Icon(id) = &r.model
                && let Some(text) = id.strip_prefix(bd_shapes::costume::summon::PREFIX)
                && !text.starts_with(words)
            {
                r.model = ModelSource::Icon(format!("{}{words} {text}", bd_shapes::costume::summon::PREFIX));
            }
        }
        if v.echo {
            r.scale *= d.echo.scale;
            r.mass *= d.echo.mass;
            self.first_call.echo_discount = d.echo.discount;
        }
    }

    fn call_substitute(&mut self, i: usize, step: ToolStep, v: Verdict) -> ToolStep {
        let t = PowerTable::get();
        let side = |base: TwistBase, mods: Vec<u8>| TwistStep { a: TwistSide { base, mods }, b: None, mods: vec![], shot: 0 };
        match step {
            ToolStep::Move { mv, kit } => match mv.resolve(t) {
                Some((_, m)) if crate::powers::toolbox::borrowable(m) => {
                    let mods = self.call_mods(v, chain::move_element(m));
                    ToolStep::Twist(Box::new(side(TwistBase::Move { mv, kit }, mods)))
                }
                _ => ToolStep::Move { mv, kit },
            },
            ToolStep::Held(sel) => match twist::exec::held_move(&self.players[i].power, sel) {
                Some((_, _, m)) if crate::powers::toolbox::borrowable(m) => {
                    let mods = self.call_mods(v, chain::move_element(m));
                    ToolStep::Twist(Box::new(side(TwistBase::Held(sel), mods)))
                }
                _ => ToolStep::Held(sel),
            },
            ToolStep::Composed(spec) => {
                let mods = self.call_mods(v, Element::Light);
                ToolStep::Twist(Box::new(side(TwistBase::Composed(spec), mods)))
            }
            ToolStep::Twist(mut tw) => {
                let element = match &tw.a.base {
                    TwistBase::Move { mv, .. } => mv.resolve(t).map_or(Element::Light, |(_, m)| chain::move_element(m)),
                    TwistBase::Held(sel) => {
                        twist::exec::held_move(&self.players[i].power, *sel).map_or(Element::Light, |(_, _, m)| chain::move_element(m))
                    }
                    TwistBase::Cast { recipe, .. } => recipe.element,
                    TwistBase::Composed(_) => Element::Light,
                };
                let mods = self.call_mods(v, element);
                tw.mods.truncate(twist::MAX_MODS.saturating_sub(mods.len()));
                tw.mods.extend(mods);
                ToolStep::Twist(tw)
            }
            ToolStep::Cast { mut recipe, shape, alt_shape, incantation, signature } => {
                self.call_recipe(&mut recipe, v);
                ToolStep::Cast { recipe, shape, alt_shape, incantation, signature }
            }
        }
    }

    fn call_begin(&mut self, i: usize, uses: Uses, verdict: Verdict) -> Pending {
        self.first_call.depth = self.first_call.depth.saturating_add(1);
        Pending { on: true, player: Some(self.players[i].id), uses, verdict, ev0: self.events.len() }
    }

    /// FIRST CALL HOOK (`fire_step`): gate one toolbox step. Returns the step to fire (the rival /
    /// echo version when it has to be) and the pending state for [`Self::call_done`].
    pub(crate) fn call_step(&mut self, i: usize, step: ToolStep) -> (ToolStep, Pending) {
        if !self.call_rules_on() || self.first_call.depth > 0 || i >= self.players.len() {
            return (step, Pending::default());
        }
        // a twist's host follow-up shot is part of the command that already went through the gate
        if let ToolStep::Twist(t) = &step
            && t.shot > 0
        {
            return (step, Pending::default());
        }
        let uses = self.call_step_uses(i, &step);
        let v = self.call_decide(self.players[i].id, &uses);
        let step = if v.plain() { step } else { self.call_substitute(i, step, v) };
        let p = self.call_begin(i, uses, v);
        (step, p)
    }

    /// FIRST CALL HOOK (`cast_plain`): gate a direct cast command (nested casts pass through).
    pub(crate) fn call_cast(&mut self, player: PlayerId, recipe: Recipe) -> (Recipe, Pending) {
        if !self.call_rules_on() || self.first_call.depth > 0 || self.free_cast {
            return (recipe, Pending::default());
        }
        let Some(i) = self.pidx(player) else { return (recipe, Pending::default()) };
        // A power grant claims when it is HELD (`call_gain`: a fruit is someone's when they eat it).
        let grant = recipe.grants.as_deref().is_some_and(|g| PowerTable::get().index_of(crate::powers::split_tier(g).0).is_some());
        let uses: Uses = if grant { [None, None] } else { [self.call_recipe_use(&recipe, self.signature_casting()), None] };
        let v = self.call_decide(player, &uses);
        let mut recipe = recipe;
        if !v.plain() {
            self.call_recipe(&mut recipe, v);
        }
        let p = self.call_begin(i, uses, v);
        (recipe, p)
    }

    /// End a gated command: commit it if it went off (`ok`).
    pub(crate) fn call_done(&mut self, p: Pending, ok: bool) {
        if !p.on {
            return;
        }
        self.first_call.depth = self.first_call.depth.saturating_sub(1);
        self.first_call.echo_discount = 0;
        if ok && let Some(player) = p.player {
            self.call_commit(player, p.uses, p.verdict, p.ev0);
        }
    }

    /// Record a use that went off: claims (FIRST CALL), the life log (BURN), marks, events.
    fn call_commit(&mut self, player: PlayerId, uses: Uses, v: Verdict, ev0: usize) {
        let Some(i) = self.pidx(player) else { return };
        // where it shows: the construct it made (a rival summon), else the caller
        let made_at = self.events.get(ev0..).unwrap_or(&[]).iter().find_map(|e| match e {
            GameEvent::ConstructSpawned { owner, pos, recipe, .. } if *owner == player => {
                Some(*pos + Vec3::Y * (recipe.scale * 0.6).clamp(0.8, 6.0))
            }
            _ => None,
        });
        let pos = made_at.unwrap_or(self.players[i].mv.pos + Vec3::Y * 1.2);
        let d = CallData::shipped();
        let tick = self.tick;
        let mut first_key: Option<Ident> = None;
        let mut announced_rival = false;
        let mut fresh = false;
        let mut echoed = false;
        for u in uses.iter().flatten().copied() {
            first_key.get_or_insert(u);
            if self.cfg.rules.first_call && named(u.kind) {
                let claims = &mut self.first_call.snap.claims;
                match claims.binary_search_by_key(&u.key, |c| c.key) {
                    Err(k) => {
                        if claims.len() < d.max_claims as usize {
                            claims.insert(k, ClaimRow { key: u.key, owner: player, kind: u.kind, tick });
                            self.events.push(GameEvent::FirstCall(CallEvent::Claimed { player, key: u.key, kind: u.kind }));
                        }
                    }
                    Ok(k) if claims[k].owner == player => {}
                    Ok(k) if claims[k].tick == tick && player < claims[k].owner => {
                        // same tick: the lower player id wins; the other's use becomes the rival one
                        let loser = claims[k].owner;
                        claims[k].owner = player;
                        self.events.push(GameEvent::FirstCall(CallEvent::Claimed { player, key: u.key, kind: u.kind }));
                        let lpos = self.player(loser).map_or(pos, |p| p.mv.pos + Vec3::Y * 1.2);
                        self.events.push(GameEvent::FirstCall(CallEvent::Rival {
                            player: loser,
                            owner: player,
                            key: u.key,
                            kind: u.kind,
                            pos: lpos,
                        }));
                        for m in self.first_call.snap.marks.iter_mut().filter(|m| m.key == u.key) {
                            if self.constructs.get(ConstructId(m.construct)).is_some_and(|c| c.owner == loser) {
                                m.mark |= mark::RIVAL;
                            }
                        }
                    }
                    Ok(k) => {
                        let owner = claims[k].owner;
                        if !announced_rival {
                            announced_rival = true;
                            self.events.push(GameEvent::FirstCall(CallEvent::Rival { player, owner, key: u.key, kind: u.kind, pos }));
                        }
                    }
                }
            }
            let burn = self.cfg.rules.burn;
            if burn.on() && (named(u.kind) || burn == BurnScope::Everything) {
                if self.first_call.snap.burned(player, u.key) {
                    if !echoed {
                        echoed = true;
                        self.events.push(GameEvent::FirstCall(CallEvent::Echo { player, key: u.key, kind: u.kind }));
                    }
                } else if !self.first_call.life.iter().any(|(p, k, _)| *p == player && *k == u.key) {
                    let mine = self.first_call.life.iter().filter(|(p, _, _)| *p == player).count();
                    if mine < d.max_life as usize {
                        self.first_call.life.push((player, u.key, u.kind));
                    }
                    if !fresh {
                        fresh = true;
                        self.events.push(GameEvent::FirstCall(CallEvent::Fresh { player, key: u.key, kind: u.kind }));
                    }
                }
            }
        }
        // marks on what it built
        let bits = if v.rival.is_some() { mark::RIVAL } else { 0 } | if v.echo { mark::ECHO } else { 0 };
        if bits != 0
            && let Some(u) = first_key
        {
            let mut made: Vec<u32> = Vec::new();
            for e in self.events.get(ev0..).unwrap_or(&[]) {
                if let GameEvent::ConstructSpawned { id, owner, .. } = e
                    && *owner == player
                {
                    made.push(id.0);
                }
            }
            let marks = &mut self.first_call.snap.marks;
            // forget marks of constructs that are gone (bounded list)
            let store = &self.constructs;
            marks.retain(|m| store.get(ConstructId(m.construct)).is_some());
            for id in made {
                match marks.binary_search_by_key(&id, |m| m.construct) {
                    Ok(k) => marks[k].mark |= bits,
                    Err(k) if marks.len() < 256 => marks.insert(k, MarkRow { construct: id, mark: bits, key: u.key }),
                    Err(_) => {}
                }
            }
        }
    }

    /// FIRST CALL HOOK (`power_move_aimed`): a held move of a power that is a rival's (claimed by
    /// someone else) or burned for the caller fires as its rival / echo version; any other held move
    /// runs as ever inside the gate (its casts aren't gated again). `None` = rules off / nested: the
    /// caller runs it plainly.
    pub(crate) fn call_power_move(&mut self, player: PlayerId, slot: u8, aim: Option<Aim>) -> Option<Result<(), PowerError>> {
        if !self.call_rules_on() || self.first_call.depth > 0 {
            return None;
        }
        let i = self.pidx(player)?;
        let held = twist::exec::held_move(&self.players[i].power, slot);
        let uses: Uses = [held.and_then(|(r, _, _)| Identities::shipped().of_move(r)), None];
        let v = self.call_decide(player, &uses);
        let p = self.call_begin(i, uses, v);
        let r = match held {
            Some((_, _, m)) if !v.plain() && crate::powers::toolbox::borrowable(m) => {
                let mods = self.call_mods(v, chain::move_element(m));
                let step = TwistStep { a: TwistSide { base: TwistBase::Held(slot), mods }, b: None, mods: vec![], shot: 0 };
                self.fire_twist(i, step, aim)
            }
            _ => self.power_move_plain(player, slot, aim),
        };
        self.call_done(p, r.is_ok());
        Some(r)
    }

    /// FIRST CALL HOOK (`gain_power_at`): holding a power is using it (a fruit is the eater's).
    pub(crate) fn call_gain(&mut self, i: usize, power: u16) {
        if !self.call_rules_on() || i >= self.players.len() {
            return;
        }
        let Some(u) = Identities::shipped().of_power(power) else { return };
        let player = self.players[i].id;
        let v = self.call_decide(player, &[Some(u), None]);
        let ev0 = self.events.len();
        self.call_commit(player, [Some(u), None], v, ev0);
    }

    /// SPARKS HOOK (`cast_sparks_of`): an echo construct is cheaper (never below one spark).
    pub(crate) fn call_cast_sparks(&self, n: u32) -> u32 {
        let off = self.first_call.echo_discount as u32;
        if off == 0 { n } else { n.saturating_sub(off).max(1) }
    }

    /// FIRST CALL HOOK (KO): BURN ON DEATH burns what the player used this life.
    pub(crate) fn call_ko(&mut self, i: usize) {
        if !self.cfg.rules.burn.on() || i >= self.players.len() {
            return;
        }
        let player = self.players[i].id;
        let max = CallData::shipped().max_burns as usize;
        let mut n = 0u16;
        let life = std::mem::take(&mut self.first_call.life);
        let (mine, rest): (Vec<_>, Vec<_>) = life.into_iter().partition(|(p, _, _)| *p == player);
        self.first_call.life = rest;
        let burns = &mut self.first_call.snap.burns;
        for (_, key, kind) in mine {
            if let Err(k) = burns.binary_search_by_key(&(player, key), |b| (b.player, b.key)) {
                if burns.len() >= max {
                    continue;
                }
                burns.insert(k, BurnRow { key, player, kind });
                n += 1;
            }
        }
        if n > 0 {
            self.events.push(GameEvent::FirstCall(CallEvent::Burned { player, n }));
            match self.first_call.pending_crumble.iter_mut().find(|(p, _)| *p == player) {
                Some(c) => c.1 = c.1.saturating_add(n),
                None => self.first_call.pending_crumble.push((player, n)),
            }
        }
    }

    /// FIRST CALL HOOK (respawn): the ring sheds the ash of what burned last life.
    pub(crate) fn call_respawn(&mut self, i: usize) {
        if self.first_call.pending_crumble.is_empty() || i >= self.players.len() {
            return;
        }
        let player = self.players[i].id;
        if let Some(k) = self.first_call.pending_crumble.iter().position(|(p, _)| *p == player) {
            let (_, n) = self.first_call.pending_crumble.remove(k);
            self.events.push(GameEvent::FirstCall(CallEvent::Crumble { player, n }));
        }
    }

    /// FIRST CALL HOOK (leaving): their claims are free again; their burns and life log go.
    pub(crate) fn call_leave(&mut self, player: PlayerId) {
        if self.first_call.is_empty() {
            return;
        }
        let fc = &mut self.first_call;
        fc.snap.claims.retain(|c| c.owner != player);
        fc.snap.burns.retain(|b| b.player != player);
        fc.life.retain(|(p, _, _)| *p != player);
        fc.pending_crumble.retain(|(p, _)| *p != player);
    }

    /// Is a signature cast in progress (its name is a named identity)?
    fn signature_casting(&self) -> bool {
        self.signature.cast.is_some()
    }
}

/// The identity of a move ref (presentation helper; the same index the host uses).
pub fn move_ident(mv: MoveRef) -> Option<Ident> {
    Identities::shipped().of_move(mv)
}

/// The NAMED identity of a recipe (presentation helper).
pub fn recipe_ident(r: &Recipe, signature: bool) -> Option<Ident> {
    Identities::shipped().of_recipe(r, signature)
}
