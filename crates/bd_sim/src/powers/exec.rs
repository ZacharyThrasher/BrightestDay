//! Power commands (grant, moves, incantations, stand rush) and the per-tick power step. Every
//! operation here is an existing world operation (cast, impulse, field, portal, link, explosion,
//! carve, status, seat) driven by data.

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, ModelSource, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::voxel::{Aabb, voxel_center};
use bd_core::{ConstructId, DT, MAX_PLAYERS, PlayerId, Quat, VOXEL_SIZE, Vec3};

use super::*;
use crate::combat::{CarveRequest, ConstructHit, Explosion, HitInfo, HitKind};
use crate::constructs::{AnimState, Loco, Slot, SpawnParams, forming_secs, lifetime_secs};
use crate::events::{CastError, GameEvent, GoneReason};
use crate::geom;
use crate::movement::{CollisionWorld, Hand, hand_pos, raycast_world};
use crate::space::{LINK_LIFETIME, LINK_MAX_LEN, LinkEnd, PORTAL_LIFETIME, PORTAL_RADIUS, PortalEnd};
use crate::status::StatusKind;
use crate::willpower::{integrity_for_cost, upkeep_per_sec_as, validate_recipe};
use crate::world::{Aim, World};

const INF: f32 = f32::INFINITY;

/// "random:devil_fruit" (also "devil_fruit").
pub fn is_random_fruit(key: &str) -> bool {
    key.eq_ignore_ascii_case(RANDOM_FRUIT) || key.eq_ignore_ascii_case("devil_fruit")
}

/// "random:mutagen" (TMNT: a random animal hybrid among the `mutagen_*` powers).
pub fn is_random_mutagen(key: &str) -> bool {
    key.eq_ignore_ascii_case(RANDOM_MUTAGEN)
}

/// "random:origin" (a random superhero origin kit).
pub fn is_random_origin(key: &str) -> bool {
    key.eq_ignore_ascii_case(RANDOM_ORIGIN)
}

/// "random:stand" (also "stand_arrow").
pub fn is_random_stand(key: &str) -> bool {
    key.eq_ignore_ascii_case(RANDOM_STAND) || key.eq_ignore_ascii_case("stand_arrow")
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z).normalize_or(Vec3::NEG_Z)
}

impl World {
    // ------------------------------------------------------------------------------ queries

    /// The power `player` holds (if any).
    pub fn player_power(&self, player: PlayerId) -> Option<&'static PowerDef> {
        self.player(player).and_then(|p| p.power.def())
    }

    /// [`parse_phrase`] for `player`'s current power.
    pub fn parse_power_phrase(&self, player: PlayerId, text: &str) -> PowerPhrase {
        match self.player(player).filter(|_| self.cfg.rules.powers_enabled) {
            Some(p) => p.power.parse_phrase(text),
            None => PowerPhrase::Plain,
        }
    }

    // ------------------------------------------------------------------------------ commands

    /// Give `player` the power `key` directly (tools, tests, a lobby "random powers" toggle).
    /// A stand calls its companion; a fruit / origin is taken as if grabbed (one power slot: the
    /// new power replaces the old, except a second fruit explodes you).
    pub fn grant_power(&mut self, player: PlayerId, key: &str) -> bool {
        let Some(i) = self.pidx(player) else { return false };
        if !self.players[i].alive {
            return false;
        }
        let Some(id) = self.resolve_power_key(key) else { return false };
        let Some(def) = PowerTable::get().def(id) else { return false };
        if !self.power_allowed(def) {
            return false;
        }
        if def.kind.is_pickup() {
            self.take_pickup(i, id);
        } else {
            self.gain_power_at(i, id, GainHow::Granted, split_tier(key).1.unwrap_or(0));
        }
        true
    }

    /// Cast `recipe` (the lexicon's resolution of the phrase *after* the incantation prefix, see
    /// [`parse_phrase`]) with the held power's delivery ("gomu gomu no anvil" → an anvil with
    /// Returning + Bounce launched from the fist on a stretching arm). Without the power it is a
    /// plain cast.
    pub fn cast_incantation(&mut self, player: PlayerId, recipe: Recipe, shape: Option<ShapeSpec>, aim: Aim) -> Result<ConstructId, CastError> {
        self.cast_incantation_shaped(player, recipe, shape, None, aim)
    }

    /// [`Self::cast_incantation`] passing a two-mode icon's authored robot `alt_shape` through.
    pub fn cast_incantation_shaped(&mut self, player: PlayerId, recipe: Recipe, shape: Option<ShapeSpec>, alt_shape: Option<ShapeSpec>, aim: Aim) -> Result<ConstructId, CastError> {
        self.powers.incanting = true;
        self.powers.delivered = None;
        let res = self.cast_shaped(player, recipe, shape, alt_shape, aim);
        self.powers.incanting = false;
        if let (Ok(cid), Some((from_fist, arm))) = (res, self.powers.delivered.take()) {
            let base = self.cfg.move_params.clone();
            let hand = self.player(player).map(|p| {
                let params = p.params(&base);
                (hand_pos(&p.mv, &params, p.input.yaw, p.input.pitch, Hand::Right), p.look())
            });
            if let (Some(c), Some((hand, look))) = (self.constructs.get_mut(cid), hand) {
                if from_fist && c.age == 0.0 && c.category == FormCategory::Object {
                    // Never from the sky: it forms in the fist and the object controller throws it.
                    c.comp.eff.0 &= !(1u64 << (Mechanic::DropFromSky as u32 & 63));
                    c.comp.dropped = true;
                    c.pos = hand + look * (c.half.max_element() + 0.3);
                    c.prev_pos = c.pos;
                    c.vel = Vec3::ZERO;
                }
            }
            self.inject_event(GameEvent::Power(PowerEvent::Delivered { player, construct: cid, arm, from_fist }));
        }
        res
    }

    /// G tap (slot 0) / G hold (slot 1): the power's signature move.
    pub fn power_move(&mut self, player: PlayerId, slot: u8) -> Result<(), PowerError> {
        self.power_move_aimed(player, slot, None)
    }

    /// [`Self::power_move`] along a client aim (CAMERA: third person sends the converged
    /// crosshair ray, like casts). The aim is host-validated ([`World::resolve_aim`]: near the eye,
    /// clamped to the look when the rules forbid third person); every op that reads the eye / look
    /// (casts, dashes, blinks, "at aim" placements) uses it for this move. `None` = the look.
    pub fn power_move_aimed(&mut self, player: PlayerId, slot: u8, aim: Option<Aim>) -> Result<(), PowerError> {
        let Some(i) = self.pidx(player) else { return Err(PowerError::UnknownPlayer) };
        self.powers.move_aim = aim.map(|a| {
            let (o, d) = self.resolve_aim(i, &a);
            (i, o, d)
        });
        let r = self.power_move_inner(i, player, slot);
        self.powers.move_aim = None;
        r
    }

    fn power_move_inner(&mut self, i: usize, player: PlayerId, slot: u8) -> Result<(), PowerError> {
        let p = &self.players[i];
        if !p.alive {
            return Err(PowerError::Dead);
        }
        let Some(def) = p.power.def() else { return Err(PowerError::NoPower) };
        if !self.power_allowed(def) {
            return Err(PowerError::Disabled);
        }
        // Selectors: swap the active power / pick a tier / fire another held power's move.
        if slot & move_sel::SELECT != 0 {
            let held = ((slot >> 4) & 3) as usize;
            let tier = slot & move_sel::KEEP_TIER;
            return self.select_power(i, held, if tier == move_sel::KEEP_TIER { None } else { Some(tier) });
        }
        if slot & move_sel::MOVE_OF != 0 {
            let (held, s) = move_sel::split_move_of(slot);
            if held == 0 {
                return self.power_move_inner(i, player, s);
            }
            // Run it as the active power for one move, then swap straight back (cooldowns stay
            // with their own power: moves from different powers chain).
            if !self.players[i].power.swap_active(held) {
                return Err(PowerError::NoSuchMove);
            }
            let r = self.power_move_inner(i, player, s);
            if self.players[i].power.others[held - 1].is_some() {
                self.players[i].power.swap_active(held);
            }
            return r;
        }
        if slot == move_sel::CYCLE {
            return self.cycle_power(i);
        }
        if slot as usize >= move_sel::MAX_MOVES {
            return Err(PowerError::NoSuchMove);
        }
        let p = &self.players[i];
        if !p.power.active() {
            return Err(PowerError::Drained);
        }
        let tier = p.power.tier;
        let Some(mv) = def.moves_at(tier).get(slot as usize) else { return Err(PowerError::NoSuchMove) };
        let s = slot as usize;
        if p.power.move_cd[s] > 0.0 {
            return Err(PowerError::Cooldown);
        }
        let only_transform = mv.ops.iter().all(|o| matches!(o, MoveOp::Transform | MoveOp::Tier(_)));
        if p.status.stunned() || p.oath.reciting() || p.charge.active || (p.seat.is_some_and(|s| !s.suit) && !only_transform) {
            return Err(PowerError::Busy);
        }
        // GENERIC OPS: a Lockout (hack / silence / EMP zone) takes power moves away for a moment.
        if self.locked_at(i) && !only_transform {
            return Err(PowerError::Busy);
        }
        let cost = if self.cfg.rules.infinite_willpower() { 0.0 } else { mv.cost };
        if cost > p.willpower + 1e-4 {
            return Err(PowerError::NotEnoughWillpower);
        }
        // Run the ops; the move only counts (cost + cooldown) if the first op did something.
        let (from, fire_dir) = (p.mv.pos, self.eye_look(i).1); // TOOLBOX: for the `Fired` event
        self.phase_break(i); // SHONEN CLASSICS (ops/phase.rs): striking makes you solid
        let mut first: Option<Result<(), PowerError>> = None;
        // IMPACT FRAMES: `MoveDef::crit` — every hit these ops land crits (`effects`).
        let outer_crit = self.powers.move_crit.take();
        self.powers.move_crit = mv.crit.map(|(m, fx)| (i, m, fx)).or(outer_crit);
        for op in &mv.ops {
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
        self.powers.move_crit = outer_crit;
        if let Some(Err(e)) = first {
            return Err(e);
        }
        let cd = mv.cooldown * effects::cooldown_mult(&self.players[i].power);
        let p = &mut self.players[i];
        p.willpower = (p.willpower - cost).max(0.0);
        // A Tier / Transform op may have swapped nothing, but the slot is still this power's.
        p.power.move_cd[s] = cd;
        let power = p.power.id.unwrap_or(0);
        self.note_seen_move(i, power, tier, slot); // SHONEN CLASSICS (ops/copy_last.rs): enemies nearby saw it
        self.inject_event(GameEvent::Power(PowerEvent::Move { player, power, slot }));
        self.emit_held_fired(i, slot, from, fire_dir); // TOOLBOX: one `Fired` event for every move
        Ok(())
    }

    /// G double-tap: the next held power becomes the active one.
    fn cycle_power(&mut self, i: usize) -> Result<(), PowerError> {
        let pw = &self.players[i].power;
        let Some(k) = pw.others.iter().position(|o| o.is_some()) else { return Err(PowerError::NoSuchMove) };
        // Rotate: active → back of the line, the first other → active.
        let pw = &mut self.players[i].power;
        pw.swap_active(k + 1);
        // Keep held order stable: move the old active (now at k) to the end.
        let old = pw.others[k].take();
        let mut rest: [Option<HeldPower>; MAX_EXTRA_POWERS] = [None; MAX_EXTRA_POWERS];
        let mut n = 0;
        for o in pw.others.iter().flatten() {
            rest[n] = Some(*o);
            n += 1;
        }
        if n < MAX_EXTRA_POWERS {
            rest[n] = old;
        }
        pw.others = rest;
        self.announce_selected(i);
        Ok(())
    }

    /// Make held power `held` (0 = active) active and optionally switch its tier.
    fn select_power(&mut self, i: usize, held: usize, tier: Option<u8>) -> Result<(), PowerError> {
        if held > 0 && !self.players[i].power.swap_active(held) {
            return Err(PowerError::NoSuchMove);
        }
        match tier {
            Some(t) if t != self.players[i].power.tier => self.set_tier(i, t)?,
            _ if held > 0 => self.announce_selected(i),
            Some(_) => self.announce_selected(i),
            None => {}
        }
        Ok(())
    }

    fn announce_selected(&mut self, i: usize) {
        let p = &self.players[i];
        if let Some(power) = p.power.id {
            let (player, tier) = (p.id, p.power.tier);
            self.inject_event(GameEvent::Power(PowerEvent::Selected { player, power, tier }));
        }
    }

    /// Switch the active power's tier (0 = base). A tier with an Ascend plays it (the first time
    /// this round: the awakening).
    pub(crate) fn set_tier(&mut self, i: usize, tier: u8) -> Result<(), PowerError> {
        let Some(def) = self.players[i].power.def() else { return Err(PowerError::NoPower) };
        let tier = tier.min(def.tiers.len().min(15) as u8);
        if tier > 0 && def.tier(tier).is_none() {
            return Err(PowerError::NoSuchMove);
        }
        let old = self.players[i].power.tier;
        // POWER-UPS HOOK: top tiers need an Awakening pickup (Locked: the highest free tier / a plain flare).
        let (tier, spend) = crate::pickups::gate_tier(self, i, old, tier);
        if old == tier {
            return Ok(());
        }
        if spend {
            let pid = self.players[i].id;
            crate::pickups::spend(self, pid, crate::pickups::GateKind::Awakening);
        }
        let pw = &mut self.players[i].power;
        pw.tier = tier;
        // A tier brings its own moves: their cooldowns start fresh.
        if def.tier(tier).is_some_and(|t| !t.moves.is_empty()) || def.tier(old).is_some_and(|t| !t.moves.is_empty()) {
            pw.move_cd = [0.0; move_sel::MAX_MOVES];
        }
        self.announce_selected(i);
        if tier > 0 || def.ascend.is_some() {
            let id = self.players[i].power.id.unwrap_or(0);
            self.ascend(i, Some(id), tier, false);
        }
        Ok(())
    }

    /// Emit the transformation moment (awakening = the first time this round).
    pub(crate) fn ascend(&mut self, i: usize, power: Option<u16>, tier: u8, transform: bool) {
        let player = self.players[i].id;
        let awakening = match power {
            Some(id) => {
                let key = (player, id, if transform { 0x80 | tier } else { tier });
                let first = !self.powers.awakened.contains(&key);
                if first && self.powers.awakened.len() < 1024 {
                    self.powers.awakened.push(key);
                }
                first
            }
            None => false,
        };
        self.inject_event(GameEvent::Power(PowerEvent::Ascend { player, power, tier, transform, awakening }));
    }

    /// One typed rush token (ORA / MUDA / DORA / ARI / WRYY…) while the ring console is open =
    /// one punch from the stand (typing speed = rush speed). Returns true if it was accepted.
    pub fn stand_rush_token(&mut self, player: PlayerId) -> bool {
        self.stand_rush(player, 1) > 0
    }

    /// `count` rush tokens at once ("ORAORAORA" = 3, the lexicon's `stand_rush` count): the first
    /// punch lands now, the rest queue at the max rush rate. Returns how many were accepted.
    pub fn stand_rush(&mut self, player: PlayerId, count: u16) -> u16 {
        let Some(i) = self.pidx(player) else { return 0 };
        let p = &self.players[i];
        let Some(def) = p.power.def() else { return 0 };
        if p.power.companion_def().is_none() || !p.alive || !self.power_allowed(def) || p.status.stunned() || !p.input.held(bd_core::input::buttons::TYPING) {
            return 0;
        }
        let mut accepted = 0u16;
        for _ in 0..count.min(MAX_RUSH_QUEUE as u16 + 1) {
            let pw = &mut self.players[i].power;
            if pw.rush_cd <= 0.0 && pw.rush_queue == 0 {
                self.rush_punch(i);
            } else if pw.rush_queue < MAX_RUSH_QUEUE {
                pw.rush_queue += 1;
            } else {
                break;
            }
            accepted += 1;
        }
        accepted
    }

    /// One stand rush punch (a small cone hit from the stand, which steps out front).
    fn rush_punch(&mut self, i: usize) {
        let Some(comp) = self.players[i].power.companion_def() else { return };
        let rules = &PowerTable::get().rules;
        let base = self.cfg.move_params.clone();
        let p = &self.players[i];
        let player = p.id;
        let look = p.look();
        let fwd = flat(look);
        let feet = p.mv.pos;
        let eye = p.eye(&base);
        let stand = p.power.companion;
        {
            let p = &mut self.players[i];
            p.power.rush_cd = 1.0 / rules.rush_max_rate.clamp(2.0, 60.0);
            p.power.rush_t = 0.35;
            p.power.rushes = p.power.rushes.wrapping_add(1);
        }
        // The stand steps out front for the rush.
        let mut origin = eye;
        if let Some(c) = stand.and_then(|s| self.constructs.get_mut(s)) {
            c.pos = feet + fwd * 1.0 + Vec3::Y * 0.2;
            c.prev_pos = c.pos;
            origin = c.pos + Vec3::Y * c.half.y;
        }
        // Alternate fists: a tiny sideways jitter so the rush reads as a flurry.
        let n = self.players[i].power.rushes;
        let side = Vec3::Y.cross(fwd).normalize_or(Vec3::X) * if n % 2 == 0 { 0.25 } else { -0.25 };
        let hits = self.cone_hit(i, origin + side, look, comp.rush_range, 55.0, comp.rush_damage, comp.rush_knockback, Element::Light, 0.5, stand, HitKind::Creature);
        let point = origin + look * comp.rush_range * 0.6;
        self.inject_event(GameEvent::Power(PowerEvent::StandRush { player, stand: stand.unwrap_or(ConstructId(0)), point, hits }));
    }

    // ------------------------------------------------------------------------------ grants

    fn resolve_power_key(&mut self, key: &str) -> Option<u16> {
        let key = split_tier(key).0;
        let t = PowerTable::get();
        if is_random_mutagen(key) {
            let n = t.mutagen_ids().count() as u32;
            if n == 0 {
                return None;
            }
            let k = self.rng.below(n) as usize;
            return t.mutagen_ids().nth(k);
        }
        if is_random_origin(key) {
            let n = t.origin_ids().count() as u32;
            if n == 0 {
                return None;
            }
            let k = self.rng.below(n) as usize;
            return t.origin_ids().nth(k);
        }
        let stand = if is_random_fruit(key) {
            false
        } else if is_random_stand(key) {
            true
        } else {
            return t.index_of(key);
        };
        let n = t.ids_of(stand).count() as u32;
        if n == 0 {
            return None;
        }
        let k = self.rng.below(n) as usize;
        t.ids_of(stand).nth(k)
    }

    fn power_pay(&mut self, i: usize, cost: f32) -> Result<(), CastError> {
        if self.cfg.rules.infinite_willpower() {
            return Ok(());
        }
        let p = &mut self.players[i];
        if cost > p.willpower + 1e-4 {
            return Err(CastError::NotEnoughWillpower { cost, have: p.willpower });
        }
        p.willpower = (p.willpower - cost).max(0.0);
        p.regen_pause = crate::willpower::REGEN_PAUSE_AFTER_CAST;
        Ok(())
    }

    /// A recipe with `grants`: a fruit construct, or the stand itself. `None` = not a known /
    /// allowed power (the cast continues as a normal construct — the ring never says no).
    pub(crate) fn cast_grant(&mut self, i: usize, recipe: &Recipe, origin: Vec3, dir: Vec3) -> Option<Result<ConstructId, CastError>> {
        let full = recipe.grants.as_deref()?;
        // A plain "power up" / "full power": the Ascend moment in your corps colour. Cosmetic only.
        if full.eq_ignore_ascii_case(ASCEND_KEY) {
            let pw = &mut self.players[i].power;
            if pw.ascend_cd <= 0.0 {
                pw.ascend_cd = 2.5;
                self.ascend(i, None, 0, false);
            }
            return Some(Ok(NO_CONSTRUCT));
        }
        let (key, tier) = split_tier(full);
        let t = PowerTable::get();
        let is_random_stand = is_random_stand(key);
        // A random mutagen is a fruit-like pickup (Zoan) gated by the devil-fruit toggle.
        let is_random_fruit = is_random_fruit(key) || is_random_mutagen(key);
        // A random origin is a pickup gated only by the powers toggle.
        let is_random_origin = is_random_origin(key);
        let def = if is_random_stand || is_random_fruit || is_random_origin {
            if (is_random_stand && !self.cfg.rules.stands) || (is_random_fruit && !self.cfg.rules.devil_fruits) {
                return None;
            }
            None
        } else {
            Some(t.def(t.index_of(key)?)?)
        };
        if def.is_some_and(|d| !self.power_allowed(d)) {
            return None;
        }
        // A FORM name ("gear 5", "super saiyan blue": a grants key with a tier) makes YOU that form —
        // for any kind: a fruit's form comes with the fruit (no pickup to go and eat).
        let form = tier.is_some() && def.is_some();
        // Saying a power you already hold makes it the active one (a tier phrase picks the tier):
        // free and instant, the toolkit is yours to mix.
        if let Some(d) = def
            && (!d.kind.is_pickup() || form)
            && let Some(id) = t.index_of(key)
            && let Some(slot) = self.players[i].power.slot_of(id)
        {
            let tier = tier.or(if d.tiers.is_empty() { None } else { Some(0) });
            let _ = self.select_power(i, slot, tier);
            return Some(Ok(self.players[i].power.companion.filter(|_| d.companion.is_some()).unwrap_or(NO_CONSTRUCT)));
        }
        let direct = form || def.map_or(is_random_stand, |d| !d.kind.is_pickup());
        let technique = def.is_some_and(|d| d.kind.is_technique());
        let pickup_kind = def.is_some_and(|d| d.kind.is_pickup());
        let cost = if technique {
            t.rules.technique_cost
        } else if pickup_kind {
            t.rules.fruit_cost
        } else if direct {
            t.rules.stand_cost
        } else {
            t.rules.fruit_cost
        };
        // POWER-UPS HOOK: a form at an awakening tier needs the Awakening pickup; without it the
        // phrase is the plain "power up" flare (never a no, nothing paid).
        let mut spend_awakening = false;
        if form && tier.unwrap_or(0) >= self.cfg.tuning.pickups.awakening_min_tier {
            match crate::pickups::gate(self, self.players[i].id, crate::pickups::GateKind::Awakening) {
                crate::pickups::Gate::Free => {}
                crate::pickups::Gate::Loaded => spend_awakening = true,
                crate::pickups::Gate::Locked => {
                    self.power_up_flare(i);
                    return Some(Ok(NO_CONSTRUCT));
                }
            }
        }
        // FREEDOM (never refuse): a power pickup you can not afford is not refused: the phrase
        // casts as the ordinary construct it describes (which itself sputters to fit what is left).
        // A power is permanent until KO, so a "weaker" power would be a new system; the construct
        // is the charming fallback and the power stays a real purchase. A FORM you can't afford is
        // the plain "power up" flare (the moment, not the form).
        if self.power_pay(i, cost).is_err() {
            if form {
                self.power_up_flare(i);
                return Some(Ok(NO_CONSTRUCT));
            }
            return None;
        }
        let name = recipe.name.clone();
        let tick = self.tick;
        self.players[i].casts.record(&name, tick);
        let Some(id) = self.resolve_power_key(key) else { return Some(Err(CastError::Busy)) };
        let owner = self.players[i].id;
        if direct {
            let how = if technique || form { GainHow::Learned } else { GainHow::Stand };
            // One devil fruit per body: a spoken fruit form sets any other held fruit down at your
            // feet (a pickup again) — no overdose for words.
            if form && t.def(id).is_some_and(|d| d.kind.is_fruit()) {
                let feet = self.players[i].mv.pos + Vec3::Y * 0.5;
                loop {
                    let slot = self.players[i].power.held().find(|h| h.id != id && h.def.kind.is_fruit()).map(|h| h.slot);
                    let Some(slot) = slot else { break };
                    self.drop_held(i, slot, Some(feet));
                }
            }
            self.gain_power_at(i, id, how, tier.unwrap_or(0));
            if spend_awakening {
                crate::pickups::spend(self, owner, crate::pickups::GateKind::Awakening);
            }
            let has_companion = t.def(id).is_some_and(|d| d.companion.is_some());
            return Some(Ok(self.players[i].power.companion.filter(|_| has_companion).unwrap_or(NO_CONSTRUCT)));
        }
        // The fruit / origin pickup pops out where you aim (<= 8 m) or just in front of you.
        let cw = CollisionWorld::new(&self.voxels, self.colliders());
        let at = match raycast_world(&cw, origin, dir, 8.0, false) {
            Some(h) => h.point + h.normal * 0.45,
            None => origin + dir * 3.0,
        };
        Some(self.spawn_fruit(owner, id, at, Vec3::Y * 3.0, false).ok_or(CastError::Busy))
    }

    /// The plain "power up" flare (your corps colour, no form), rate-limited like the phrase.
    fn power_up_flare(&mut self, i: usize) {
        let pw = &mut self.players[i].power;
        if pw.ascend_cd <= 0.0 {
            pw.ascend_cd = 2.5;
            self.ascend(i, None, 0, false);
        }
    }

    /// A hard-light fruit (or origin pickup: a radioactive spider, a gamma canister...) anyone can
    /// take by touching it (conserved: if it is destroyed it reappears).
    pub(crate) fn spawn_fruit(&mut self, owner: PlayerId, power: u16, pos: Vec3, vel: Vec3, respawn: bool) -> Option<ConstructId> {
        let def = PowerTable::get().def(power)?;
        if self.constructs.len() >= crate::world::MAX_CONSTRUCTS - 8 || self.pidx(owner).is_none() || !pos.is_finite() {
            return None;
        }
        if self.powers.fruits.len() >= MAX_FRUITS {
            let old = self.powers.fruits.remove(0);
            if let Some(c) = self.constructs.get_mut(old.construct) {
                c.gone = Some(GoneReason::Expired);
            }
        }
        let look = def.pickup.as_ref();
        let recipe = validate_recipe(
            Recipe {
                name: look.filter(|k| !k.name.is_empty()).map_or_else(|| def.name.clone(), |k| k.name.clone()),
                form: look.map_or(Form::Ball, |k| k.form),
                mechanics: vec![],
                element: look.map_or(Element::Light, |k| k.element),
                scale: look.map_or(0.45, |k| k.scale),
                mass: 2.0,
                count: 1,
                accent: None,
                model: ModelSource::Procedural { seed: crate::rng::word_hash(&def.id) as u32 },
                payload: None,
                features: vec![],
                grants: Some(def.id.clone()),
                temperament: Default::default(),
                intensity: 0.0,
                mount: None,
                opening: None,
            },
            &self.cfg.rules,
        );
        let tick = self.tick;
        let cid = self.constructs.spawn(SpawnParams {
            owner,
            recipe: recipe.clone(),
            shape: None,
            pos,
            rot: Quat::IDENTITY,
            vel,
            target: None,
            cost: 0.0,
            integrity: 80.0,
            forming: 0.0,
            life: INF,
            tick,
            colossal: false,
            model: None,
        });
        if let Some(c) = self.constructs.get_mut(cid) {
            // Its own parent: never counts toward (or gets replaced from) the object slot.
            c.parent = Some(cid);
            c.comp.launched = true;
            c.body.loco = Loco::Ballistic;
            c.set_anim(AnimState::Idle, tick);
        }
        self.powers.fruits.push(Fruit { construct: cid, power, owner, last_pos: pos, age: 0.0 });
        self.inject_event(GameEvent::ConstructSpawned { id: cid, owner, recipe: Box::new(recipe), pos, rot: Quat::IDENTITY, forming: 0.0, cost: 0.0 });
        self.inject_event(GameEvent::Power(PowerEvent::FruitSpawned { construct: cid, power, pos, respawn }));
        Some(cid)
    }

    /// Take a pickup (eat a fruit / grab an origin). One power slot: the new power replaces the
    /// old one (a replaced fruit / origin drops at your feet, a stand is gone) - except a second
    /// devil fruit, which is an overdose (a big blast, self-KO, no credit).
    fn take_pickup(&mut self, i: usize, power: u16) {
        let p = &self.players[i];
        let id = p.id;
        let fruit = PowerTable::get().def(power).is_some_and(|d| d.kind.is_fruit());
        if fruit && p.power.is_fruit() {
            let base = self.cfg.move_params.clone();
            let pos = p.hull(&base).center();
            let r = &PowerTable::get().rules;
            self.inject_event(GameEvent::Power(PowerEvent::Overdose { player: id, pos }));
            self.explode(Explosion {
                pos,
                radius: r.overdose_radius,
                damage: r.overdose_damage,
                knockback: r.overdose_knockback,
                source: Some(id),
                construct: None,
                element: Element::Atomic,
                carve_radius: 2.5,
                self_damage: false,
            });
            if self.players[i].alive {
                self.ko(i, None, crate::events::KoCause::Damage);
            }
            return;
        }
        self.gain_power(i, power, GainHow::Ate);
    }

    /// Take power `power` at its base tier (see [`Self::gain_power_at`]).
    pub(crate) fn gain_power(&mut self, i: usize, power: u16, how: GainHow) {
        self.gain_power_at(i, power, how, 0);
    }

    /// Take power `power` (at `tier`) as the ACTIVE power. Powers mix: up to
    /// `PowerRules::max_powers` are held (the oldest goes beyond it; a pickup drops at your
    /// feet); a second stand replaces the first (one companion); an already-held power just
    /// becomes active.
    pub(crate) fn gain_power_at(&mut self, i: usize, power: u16, how: GainHow, tier: u8) {
        let t = PowerTable::get();
        let Some(def) = t.def(power) else { return };
        if let Some(slot) = self.players[i].power.slot_of(power) {
            let _ = self.select_power(i, slot, Some(tier));
            return;
        }
        let feet = self.players[i].mv.pos + Vec3::Y * 0.5;
        // One companion at a time: a second stand (or companion technique) replaces the first.
        if def.companion.is_some() {
            let old = self.players[i].power.held().find(|h| h.def.companion.is_some()).map(|h| h.slot);
            if let Some(slot) = old {
                self.drop_held(i, slot, None);
            }
        }
        let max = (t.rules.max_powers as usize).clamp(1, 1 + MAX_EXTRA_POWERS);
        while self.players[i].power.count() >= max {
            let pw = &self.players[i].power;
            let active = pw.id.map(|_| (0usize, pw.since));
            let oldest = active.into_iter().chain(pw.others.iter().enumerate().filter_map(|(k, o)| o.map(|o| (k + 1, o.since)))).min_by_key(|x| x.1).map(|x| x.0);
            let Some(slot) = oldest else { break };
            self.drop_held(i, slot, Some(feet));
        }
        let pw = &mut self.players[i].power;
        pw.gains = pw.gains.wrapping_add(1);
        if let Some(old) = pw.id {
            let held = HeldPower { id: old, move_cd: pw.move_cd, transformed: pw.transformed, tier: pw.tier, since: pw.since };
            if let Some(slot) = pw.others.iter_mut().find(|o| o.is_none()) {
                *slot = Some(held);
            }
        }
        pw.id = Some(power);
        pw.move_cd = [0.0; move_sel::MAX_MOVES];
        pw.transformed = false;
        pw.tier = tier.min(def.tiers.len().min(15) as u8);
        pw.since = pw.gains;
        pw.time_speed = time_stop_speed(def);
        let tier = pw.tier;
        let id = self.players[i].id;
        self.inject_event(GameEvent::Power(PowerEvent::Gained { player: id, power, how }));
        if def.companion.is_some() && self.players[i].power.companion.is_none() {
            self.spawn_companion(i);
        }
        if def.ascend_at(tier).is_some() {
            self.ascend(i, Some(power), tier, false);
        }
    }

    /// Drop the held power in `slot` (0 = active: the next held one becomes active). A fruit /
    /// origin reappears at `fruit_at`.
    pub(crate) fn drop_held(&mut self, i: usize, slot: usize, fruit_at: Option<Vec3>) {
        let pw = &mut self.players[i].power;
        let (power, transformed) = if slot == 0 {
            let Some(id) = pw.id else { return };
            let transformed = pw.transformed;
            // Promote the first other (if any) to active.
            match pw.others.iter().position(|o| o.is_some()) {
                Some(k) => {
                    pw.swap_active(k + 1);
                    pw.others[k] = None;
                }
                None => {
                    pw.id = None;
                    pw.move_cd = [0.0; move_sel::MAX_MOVES];
                    pw.transformed = false;
                    pw.tier = 0;
                }
            }
            (id, transformed)
        } else {
            let Some(o) = pw.others.get_mut(slot - 1).and_then(|o| o.take()) else { return };
            (o.id, o.transformed)
        };
        // Compact the others (held order).
        let mut rest: [Option<HeldPower>; MAX_EXTRA_POWERS] = [None; MAX_EXTRA_POWERS];
        for (n, o) in pw.others.iter().flatten().enumerate() {
            rest[n] = Some(*o);
        }
        pw.others = rest;
        let id = self.players[i].id;
        let def = PowerTable::get().def(power);
        if def.is_some_and(|d| d.companion.is_some()) {
            if let Some(cid) = self.players[i].power.companion.take() {
                if let Some(c) = self.constructs.get_mut(cid) {
                    if c.gone.is_none() {
                        c.gone = Some(GoneReason::OwnerGone);
                    }
                }
            }
        }
        if transformed && def.is_some_and(|d| d.transform.as_ref().is_some_and(|t| t.pilot.is_some())) {
            if let Some(cid) = self.players[i].power.pilot.take() {
                if self.players[i].seat.is_some_and(|s| s.construct == cid) {
                    self.eject_with(id, cid, Vec3::Y * 8.0, false);
                }
                if let Some(c) = self.constructs.get_mut(cid) {
                    if c.gone.is_none() {
                        c.gone = Some(GoneReason::OwnerGone);
                    }
                }
            }
        }
        self.inject_event(GameEvent::Power(PowerEvent::Lost { player: id, power }));
        if let (Some(at), Some(def)) = (fruit_at, def) {
            if def.kind.is_pickup() {
                self.spawn_fruit(id, power, at, Vec3::Y * 2.0, true);
            }
        }
    }

    /// Drop EVERY held power (KO / round). Fruits / origins reappear around `fruit_at`.
    pub(crate) fn lose_power(&mut self, i: usize, fruit_at: Option<Vec3>) {
        let mut n = 0u32;
        while self.players[i].power.id.is_some() && n < 8 {
            // Several pickups fan out a little so they do not stack in one spot.
            let off = if n == 0 { Vec3::ZERO } else { Vec3::new((n as f32 * 2.1).cos(), 0.0, (n as f32 * 2.1).sin()) * 0.8 };
            self.drop_held(i, 0, fruit_at.map(|p| p + off));
            n += 1;
        }
        let keep_ground = self.players[i].power.last_ground;
        let companion = self.players[i].power.companion;
        let pilot = self.players[i].power.pilot;
        for cid in [companion, pilot].into_iter().flatten() {
            if let Some(c) = self.constructs.get_mut(cid) {
                if c.gone.is_none() {
                    c.gone = Some(GoneReason::OwnerGone);
                }
            }
        }
        self.players[i].power = PlayerPower { last_ground: keep_ground, ..Default::default() };
    }

    fn spawn_companion(&mut self, i: usize) {
        let Some(comp) = self.players[i].power.companion_def() else { return };
        if self.constructs.len() >= crate::world::MAX_CONSTRUCTS - 8 {
            self.players[i].power.resummon = 1.0;
            return;
        }
        let p = &self.players[i];
        let owner = p.id;
        let fwd = flat(p.look());
        let pos = p.mv.pos - fwd * 0.9 + Vec3::Y * 0.5;
        let mut r = comp.recipe.to_recipe();
        r.temperament = bd_core::mechanics::Temperament::Protective;
        r.intensity = r.intensity.max(0.7);
        let recipe = validate_recipe(r, &self.cfg.rules);
        let rot = geom::yaw_rotation(fwd).inverse();
        let forming = forming_secs(recipe.form).min(0.6);
        let tick = self.tick;
        let cid = self.constructs.spawn(SpawnParams {
            owner,
            recipe: recipe.clone(),
            shape: None,
            pos,
            rot,
            vel: Vec3::ZERO,
            target: None,
            cost: 0.0,
            integrity: STAND_INTEGRITY,
            forming,
            life: INF,
            tick,
            colossal: false,
            model: None,
        });
        if let Some(c) = self.constructs.get_mut(cid) {
            c.parent = Some(cid);
        }
        self.players[i].power.companion = Some(cid);
        self.inject_event(GameEvent::ConstructSpawned { id: cid, owner, recipe: Box::new(recipe), pos, rot, forming, cost: 0.0 });
    }

    // ------------------------------------------------------------------------------ move ops

    pub(super) fn eye_look(&self, i: usize) -> (Vec3, Vec3) {
        // CAMERA: a power move's validated aim (third person: the converged crosshair ray).
        if let Some((k, eye, dir)) = self.powers.move_aim
            && k == i
        {
            return (eye, dir);
        }
        let p = &self.players[i];
        (p.eye(&self.cfg.move_params), p.look())
    }

    fn at_point(&self, i: usize, at: At, dist: f32) -> Vec3 {
        let (eye, look) = self.eye_look(i);
        match at {
            At::Me => self.players[i].hull(&self.cfg.move_params).center(),
            At::Front => eye + look * dist.clamp(0.0, 30.0),
            At::Aim => {
                let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(crate::movement::ColliderId::Player(self.players[i].id));
                raycast_world(&cw, eye, look, 60.0, false).map_or(eye + look * 20.0, |h| h.point + h.normal * 0.3)
            }
        }
    }

    /// The living enemy under the crosshair within `range` (terrain blocks).
    fn aimed_player(&self, i: usize, range: f32) -> Option<usize> {
        let (eye, look) = self.eye_look(i);
        let base = &self.cfg.move_params;
        let end = eye + look * range;
        let wall = bd_core::voxel::raycast(&self.voxels, eye, look, range).map_or(INF, |h| h.t);
        let mut best: Option<(usize, f32)> = None;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive {
                continue;
            }
            let hull = geom::inflate(&q.hull(base), 0.45);
            if let Some((t, _)) = geom::segment_aabb(eye, end, &hull) {
                let d = t * range;
                if d <= wall && best.is_none_or(|b| d < b.1) {
                    best = Some((k, d));
                }
            }
        }
        best.map(|b| b.0)
    }

    /// Cone hit (players + enemy constructs) from `origin` along `dir`. Returns how many things
    /// were struck. No allocation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn cone_hit(&mut self, i: usize, origin: Vec3, dir: Vec3, range: f32, angle_deg: f32, damage: f32, knockback: f32, element: Element, up: f32, construct: Option<ConstructId>, kind: HitKind) -> u8 {
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        let cos = angle_deg.clamp(1.0, 179.0).to_radians().cos();
        let in_cone = |p: Vec3| {
            let d = p - origin;
            let len = d.length();
            len <= range && (len < 0.6 || d.dot(dir) / len >= cos)
        };
        let mut targets = [None; MAX_PLAYERS];
        let mut n = 0;
        for q in &self.players {
            if q.id == id || !q.alive || n >= targets.len() {
                continue;
            }
            let h = q.hull(&base);
            let c = geom::closest_point(&h, origin + dir * range.min(1.5));
            if in_cone(c) {
                targets[n] = Some((q.id, h.center()));
                n += 1;
            }
        }
        let mut hits = 0u8;
        for (tid, c) in targets.iter().flatten().copied() {
            let push = (dir + (c - origin).normalize_or_zero()) * 0.5 + Vec3::Y * up;
            let mut hit = HitInfo::new(tid, Some(id), kind, damage, knockback, push, c).with_element(element).with_impact(if damage > 10.0 { 1.3 } else { 0.8 });
            hit.construct = construct;
            if self.apply_hit(hit).applied {
                hits = hits.saturating_add(1);
            }
        }
        let mut chits = [None; 8];
        let mut m = 0;
        for c in self.constructs.iter() {
            if m >= chits.len() {
                break;
            }
            if c.owner == id || !c.active() || c.attached() || Some(c.id) == construct {
                continue;
            }
            if in_cone(c.center()) {
                chits[m] = Some(c.id);
                m += 1;
            }
        }
        for cid in chits.iter().flatten().copied() {
            if damage > 0.0 && self.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: cid, source: Some(id), damage, impulse: dir * knockback, blade: false, point: origin + dir }) {
                hits = hits.saturating_add(1);
            } else if damage > 0.0 {
                hits = hits.saturating_add(1);
            }
        }
        hits
    }

    pub(super) fn run_op(&mut self, i: usize, def: &'static PowerDef, mv: &MoveDef, op: &MoveOp) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let (eye, look) = self.eye_look(i);
        match op {
            MoveOp::Cast { recipe, modifier, signature, merge } => {
                let mut r = recipe.to_recipe();
                // SHONEN CLASSICS: a costumed summon (a kit's clones wear the character's look).
                let mut shape = None;
                if recipe.costume {
                    bd_shapes::costume::summon::apply(&recipe.name, &mut r, &mut shape);
                }
                let aim = Aim { origin: eye, dir: look, target_hint: None, modifier: *modifier };
                let res = match signature {
                    Some(kind) => {
                        let mut sig = crate::signature::SignatureCast::new(*kind, 0.8);
                        if let Some((a, b)) = merge {
                            sig = sig.with_merge(*a, *b);
                        }
                        self.cast_signature(id, r, None, aim, sig)
                    }
                    None => self.cast(id, r, shape, aim),
                };
                match res {
                    Ok(_) => Ok(()),
                    Err(CastError::NotEnoughWillpower { .. }) => Err(PowerError::NotEnoughWillpower),
                    Err(_) => Err(PowerError::Busy),
                }
            }
            MoveOp::Impulse { fwd, up } => {
                let dv = flat(look) * fwd.clamp(-60.0, 60.0) + Vec3::Y * up.clamp(-60.0, 60.0);
                let mv = &mut self.players[i].mv;
                if *up < 0.0 {
                    mv.vel.y = mv.vel.y.min(0.0);
                }
                mv.apply_impulse(dv, 0.0);
                if *up < 0.0 {
                    mv.grounded = false;
                }
                Ok(())
            }
            MoveOp::Blink { dist, afterimage } => {
                let base = self.cfg.move_params.clone();
                let p = &self.players[i];
                let hull = p.hull(&base);
                let from = p.mv.pos;
                let dir = Vec3::new(look.x, look.y * 0.5, look.z).normalize_or(flat(look));
                let want = dist.clamp(0.5, 40.0);
                let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(crate::movement::ColliderId::Player(id));
                let mut d = want;
                let h = hull.half().y;
                for off in [Vec3::ZERO, Vec3::Y * (h - 0.15), Vec3::Y * -(h - 0.15)] {
                    if let Some(hit) = raycast_world(&cw, hull.center() + off, dir, want + 0.5, false) {
                        d = d.min((hit.t - 0.5).max(0.0));
                    }
                }
                if d < 0.25 {
                    return Err(PowerError::NoTarget);
                }
                let to = from + dir * d;
                if !self.in_play(to, 0.0) {
                    return Err(PowerError::NoTarget);
                }
                let p = &mut self.players[i];
                p.mv.pos = to;
                p.mv.grounded = false;
                self.inject_event(GameEvent::Power(PowerEvent::Blinked { player: id, from, to, afterimage: *afterimage }));
                Ok(())
            }
            MoveOp::Reel { range, speed } => {
                let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(crate::movement::ColliderId::Player(id));
                let Some(hit) = raycast_world(&cw, eye, look, range.clamp(1.0, 80.0), true) else { return Err(PowerError::NoTarget) };
                let to = (hit.point - eye).normalize_or(look);
                let mv = &mut self.players[i].mv;
                mv.vel = to * speed.clamp(1.0, 60.0) + Vec3::Y * 2.0;
                mv.grounded = false;
                mv.tumble = 0.0;
                Ok(())
            }
            MoveOp::Shove { range, angle, damage, knockback, element, up } => {
                let hand = eye + look * 0.4;
                self.cone_hit(i, hand, look, range.clamp(0.5, 20.0), *angle, damage.clamp(0.0, 100.0), knockback.clamp(0.0, 80.0), *element, up.clamp(0.0, 2.0), None, HitKind::Melee);
                Ok(())
            }
            MoveOp::Field { strength, radius, secs, at, dist } => {
                let pos = self.at_point(i, *at, *dist);
                self.create_field(id, pos, *radius, *strength, *secs, None).map(|_| ()).ok_or(PowerError::Busy)
            }
            MoveOp::Room { radius, secs } => {
                let pos = self.players[i].hull(&self.cfg.move_params).center();
                let f = self.create_field(id, pos, *radius, 0.0, *secs, None).ok_or(PowerError::Busy)?;
                self.players[i].power.room = Some(f);
                Ok(())
            }
            MoveOp::Swap { range } => self.op_swap(i, *range),
            MoveOp::Portal { depth } => {
                let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(crate::movement::ColliderId::Player(id));
                let Some(hit) = raycast_world(&cw, eye, look, 12.0, false) else { return Err(PowerError::NoTarget) };
                let n = hit.normal.normalize_or(-look);
                // Find open air on the far side of the wall.
                let mut exit = None;
                let mut d = 1.0;
                while d <= depth.clamp(1.0, 16.0) {
                    let q = hit.point - n * d;
                    if self.voxels.get_at(q) == 0 && self.voxels.get_at(q - n * 0.6) == 0 && self.voxels.get_at(q + Vec3::Y * 0.8) == 0 {
                        exit = Some(q - n * 0.3);
                        break;
                    }
                    d += 0.5;
                }
                let Some(exit) = exit else { return Err(PowerError::NoTarget) };
                let a = crate::space::fit_portal(&self.voxels, PortalEnd::new(hit.point + n * 0.05, n, Vec3::Y, PORTAL_RADIUS));
                let b = crate::space::fit_portal(&self.voxels, PortalEnd::new(exit, -n, Vec3::Y, PORTAL_RADIUS));
                self.create_portal(id, a, b, PORTAL_LIFETIME, None).map(|_| ()).ok_or(PowerError::Busy)
            }
            MoveOp::Link { range } => {
                let end = match self.aimed_player(i, *range) {
                    Some(k) => LinkEnd::Player(self.players[k].id),
                    None => {
                        let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(crate::movement::ColliderId::Player(id));
                        match raycast_world(&cw, eye, look, range.clamp(1.0, 30.0), true) {
                            Some(h) => LinkEnd::Point(h.point),
                            None => return Err(PowerError::NoTarget),
                        }
                    }
                };
                self.create_link(Some(id), LinkEnd::Player(id), end, LINK_MAX_LEN, LINK_LIFETIME).map(|_| ()).ok_or(PowerError::NoTarget)
            }
            MoveOp::Explode { radius, damage, knockback, carve, element, at, dist } => {
                let pos = self.at_point(i, *at, *dist);
                // A blast from your own body (Almighty Push) never rocket-jumps you: the ordinary
                // own-blast push would launch the caster straight up (~34 m/s) into whatever is above.
                let keep = (*at == At::Me).then(|| self.players[i].mv);
                self.explode(Explosion {
                    pos,
                    radius: *radius,
                    damage: *damage,
                    knockback: *knockback,
                    source: Some(id),
                    construct: None,
                    element: *element,
                    carve_radius: *carve,
                    self_damage: false,
                });
                if let Some(mv) = keep {
                    self.players[i].mv = mv;
                }
                Ok(())
            }
            MoveOp::Carve { length, radius, damage, pull } => {
                self.op_carve(i, length.clamp(1.0, 40.0), radius.clamp(0.3, 4.0), *damage, *pull);
                Ok(())
            }
            MoveOp::TimeStop { secs, speed } => {
                let p = &mut self.players[i];
                p.power.time_stop = secs.clamp(0.1, 3.0);
                p.power.time_speed = speed.clamp(1.0, 3.0);
                let secs = p.power.time_stop;
                self.inject_event(GameEvent::Power(PowerEvent::TimeStop { player: id, secs }));
                Ok(())
            }
            MoveOp::Restore { radius } => {
                if self.op_restore(i, radius.clamp(1.0, 20.0)) {
                    Ok(())
                } else {
                    Err(PowerError::NoTarget)
                }
            }
            MoveOp::Transform => self.op_transform(i, def),
            MoveOp::SelfStatus { status, secs } => {
                self.apply_status(i, *status, *secs, Some(id));
                Ok(())
            }
            MoveOp::Weigh { range, secs, down } => {
                let Some(k) = self.aimed_player(i, *range) else { return Err(PowerError::NoTarget) };
                let tid = self.players[k].id;
                let c = self.players[k].hull(&self.cfg.move_params).center();
                // A zero-damage hit: credit + interrupts, no knockback.
                self.apply_hit(HitInfo::new(tid, Some(id), HitKind::RingShove, 0.0, 0.0, Vec3::NEG_Y, c));
                self.apply_status(k, StatusKind::Chill, *secs, Some(id));
                let q = &mut self.players[k];
                if q.alive {
                    q.mv.vel.y = q.mv.vel.y.min(0.0) - down.clamp(0.0, 40.0);
                    q.mv.grounded = false;
                }
                Ok(())
            }
            MoveOp::SlamOnLand { radius, damage, knockback, carve } => {
                self.players[i].power.slam = Some([*radius, *damage, *knockback, *carve]);
                Ok(())
            }
            MoveOp::Yank { range, speed, cone } => self.op_yank(i, range.clamp(1.0, 60.0), speed.clamp(1.0, 60.0), cone.clamp(0.0, 20.0)),
            MoveOp::Singularity { dist, rise, secs, radius, strength, capture, chunks, damage, knockback } => {
                self.op_singularity(i, eye, look, &mv.name, mv.cost, (*dist, *rise, *secs, *radius, *strength, *capture, *chunks, *damage, *knockback))
            }
            MoveOp::Tier(t) => {
                let t = if *t == self.players[i].power.tier { 0 } else { *t };
                self.set_tier(i, t)
            }
            // MYTHOLOGY & FOLKLORE: run one rolled pick (ops/roulette.rs); nested roulettes are skipped.
            MoveOp::Roulette { picks } => {
                let k = if picks.is_empty() { return Err(PowerError::NoSuchMove) } else { self.op_roulette_pick(i, picks.len()) };
                let mut res = Ok(());
                for (n, op) in picks[k].iter().filter(|o| !matches!(o, MoveOp::Roulette { .. })).enumerate() {
                    let r = self.run_op(i, def, mv, op);
                    if n == 0 && r.is_err() {
                        return r;
                    }
                    res = res.and(Ok(()));
                }
                res
            }
            // GENERIC OPS (powers/ops/*.rs): everything else is one of them.
            #[allow(unreachable_patterns)]
            op => self.run_generic_op(i, op).unwrap_or(Err(PowerError::NoSuchMove)),
        }
    }

    /// Universal Pull: the aimed enemy (generous cone) or construct flies at you; their tether
    /// breaks.
    fn op_yank(&mut self, i: usize, range: f32, speed: f32, cone: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let (eye, look) = self.eye_look(i);
        let base = self.cfg.move_params.clone();
        let me = self.players[i].hull(&base).center();
        // Aimed player: the crosshair ray, else the best one inside the cone (line of sight).
        let target = self.aimed_player(i, range).or_else(|| {
            let cos = cone.to_radians().cos();
            let mut best: Option<(usize, f32)> = None;
            for (k, q) in self.players.iter().enumerate() {
                if k == i || !q.alive || cone <= 0.0 {
                    continue;
                }
                let c = q.hull(&base).center();
                let d = c - eye;
                let len = d.length();
                if len > range || len < 0.5 {
                    continue;
                }
                let dot = d.dot(look) / len;
                if dot < cos || bd_core::voxel::raycast(&self.voxels, eye, d / len, len - 0.5).is_some() {
                    continue;
                }
                if best.is_none_or(|b| dot > b.1) {
                    best = Some((k, dot));
                }
            }
            best.map(|b| b.0)
        });
        if let Some(k) = target {
            let q = &mut self.players[k];
            let from = q.hull(&base).center();
            let to = (me + look.normalize_or_zero() * 1.6 - from).normalize_or(-look);
            q.mv.release_tethers();
            q.mv.extra.cancel_dodge(&mut q.mv.vel);
            q.mv.vel = to * speed + Vec3::Y * 4.0;
            q.mv.grounded = false;
            q.mv.tumble = q.mv.tumble.max(0.25);
            let tid = q.id;
            // A zero-damage hit: credit + interrupts (no knockback, the yank is the motion).
            self.apply_hit(HitInfo::new(tid, Some(id), HitKind::RingShove, 0.0, 0.0, to, from));
            self.inject_event(GameEvent::Power(PowerEvent::Yanked { player: id, target: Some(tid), construct: None, from }));
            return Ok(());
        }
        // Else the first construct (not yours, not worn / held by someone) on the aim line.
        let hit = self.constructs.segment_query(eye, eye + look * range, 0.6, None).filter(|(c, _, _)| self.constructs.get(*c).is_some_and(|c| c.owner != id && !c.attached() && c.parent != Some(c.id)));
        let Some((cid, _, _)) = hit else { return Err(PowerError::NoTarget) };
        let Some(c) = self.constructs.get_mut(cid) else { return Err(PowerError::NoTarget) };
        let from = c.center();
        let to = (me - from).normalize_or(-look);
        c.vel = to * speed * 0.8 + Vec3::Y * 2.0;
        if matches!(c.body.loco, Loco::Static | Loco::Kinematic) && c.category != FormCategory::Structure {
            c.body.loco = Loco::Ballistic;
        }
        self.inject_event(GameEvent::Power(PowerEvent::Yanked { player: id, target: None, construct: Some(cid), from }));
        Ok(())
    }

    /// Shambles: swap places with the aimed player / own construct — both inside your ROOM.
    fn op_swap(&mut self, i: usize, range: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let Some(room) = self.players[i].power.room.and_then(|r| self.fields.iter().find(|f| f.id == r)).copied() else { return Err(PowerError::NoTarget) };
        let inside = |p: Vec3| (p - room.pos).length() <= room.radius;
        let me = self.players[i].mv.pos;
        if !inside(me) {
            return Err(PowerError::NoTarget);
        }
        if let Some(k) = self.aimed_player(i, range) {
            let other = self.players[k].mv.pos;
            if !inside(other) || self.players[k].seat.is_some() {
                return Err(PowerError::NoTarget);
            }
            self.players[i].mv.pos = other;
            self.players[k].mv.pos = me;
            self.players[i].mv.grounded = false;
            self.players[k].mv.grounded = false;
            let b = self.players[k].id;
            self.inject_event(GameEvent::Power(PowerEvent::Swapped { a: id, b_player: Some(b), b_construct: None, a_pos: other, b_pos: me }));
            return Ok(());
        }
        let (eye, look) = self.eye_look(i);
        let hit = self.constructs.segment_query(eye, eye + look * range, 0.3, None).filter(|(c, _, _)| self.constructs.get(*c).is_some_and(|c| c.owner == id && !c.attached() && c.parent != Some(c.id)));
        let Some((cid, _, _)) = hit else { return Err(PowerError::NoTarget) };
        let Some(c) = self.constructs.get_mut(cid) else { return Err(PowerError::NoTarget) };
        let other = c.pos;
        if !inside(other) {
            return Err(PowerError::NoTarget);
        }
        c.pos = me;
        c.prev_pos = me;
        self.players[i].mv.pos = other + Vec3::Y * (c.half.y * 2.0 + 0.1);
        self.players[i].mv.grounded = false;
        let to = self.players[i].mv.pos;
        self.inject_event(GameEvent::Power(PowerEvent::Swapped { a: id, b_player: None, b_construct: Some(cid), a_pos: to, b_pos: me }));
        Ok(())
    }

    /// A straight trench along the look; players in it are hit, `pull` yanks the nearest in.
    fn op_carve(&mut self, i: usize, length: f32, radius: f32, damage: f32, pull: bool) {
        let id = self.players[i].id;
        let (eye, look) = self.eye_look(i);
        let a = eye + look * 1.0;
        let b = eye + look * length;
        let n = ((length / (radius * 1.5)).ceil() as usize).clamp(1, 16);
        for k in 0..=n {
            let t = k as f32 / n as f32;
            self.request_carve(CarveRequest { center: a + (b - a) * t, radius, power: 1.5 });
        }
        let base = self.cfg.move_params.clone();
        let mut victims = [None; MAX_PLAYERS];
        let mut m = 0;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive || m >= victims.len() {
                continue;
            }
            let c = q.hull(&base).center();
            let t = ((c - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            if (a + (b - a) * t - c).length() <= radius + 0.6 {
                victims[m] = Some((k, t));
                m += 1;
            }
        }
        let nearest = victims.iter().flatten().min_by(|x, y| x.1.total_cmp(&y.1)).map(|v| v.0);
        for (k, _) in victims.iter().flatten().copied() {
            let tid = self.players[k].id;
            let c = self.players[k].hull(&base).center();
            if damage > 0.0 {
                self.apply_hit(HitInfo::new(tid, Some(id), HitKind::Melee, damage, 4.0, look, c));
            }
        }
        if pull {
            if let Some(k) = nearest {
                let q = &mut self.players[k];
                if q.alive {
                    let to = (eye - q.mv.pos).normalize_or(-look);
                    q.mv.vel = to * 22.0 + Vec3::Y * 3.0;
                    q.mv.grounded = false;
                }
            }
        }
    }

    /// Crazy Diamond: rebuild remembered terrain within `radius` (never inside a player) and own
    /// shattered constructs nearby. Replicated as one `Restored` event with the exact voxel list.
    fn op_restore(&mut self, i: usize, radius: f32) -> bool {
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        let center = self.players[i].hull(&base).center();
        let mut hulls = [None::<Aabb>; MAX_PLAYERS];
        for (k, q) in self.players.iter().enumerate().take(MAX_PLAYERS) {
            if q.alive {
                hulls[k] = Some(geom::inflate(&q.hull(&base), 0.1));
            }
        }
        let r2 = radius * radius;
        let mut list: Vec<(bd_core::IVec3, u8)> = Vec::new();
        {
            let voxels = &mut self.voxels;
            self.powers.restore_log.retain(|&(v, m)| {
                if list.len() >= MAX_RESTORE_VOXELS {
                    return true;
                }
                let p = voxel_center(v);
                if (p - center).length_squared() > r2 {
                    return true;
                }
                if voxels.get(v) != 0 {
                    return false;
                }
                let half = Vec3::splat(VOXEL_SIZE * 0.5);
                let vb = Aabb { min: p - half, max: p + half };
                if hulls.iter().flatten().any(|h| h.intersects(&vb)) {
                    return true;
                }
                voxels.set(v, m);
                list.push((v, m));
                false
            });
        }
        // Own shattered constructs nearby come back together.
        let mut rebuilt = 0u8;
        let mut k = 0;
        while k < self.powers.shattered.len() {
            let s = &self.powers.shattered[k];
            if s.owner != id || (s.pos - center).length() > radius + 2.0 {
                k += 1;
                continue;
            }
            let slot = Slot::of(s.recipe.form);
            let total = self.constructs.owned_by(id).filter(|c| c.parent.is_none()).count();
            if matches!(slot, Slot::Held | Slot::Worn) || self.constructs.in_slot(id, slot).count() >= self.cfg.rules.slot_limit(slot) || total >= self.cfg.rules.total_limit() || self.constructs.len() >= crate::world::MAX_CONSTRUCTS - 8 {
                k += 1;
                continue;
            }
            let s = self.powers.shattered.remove(k);
            let forming = forming_secs(s.recipe.form);
            let tick = self.tick;
            let cid = self.constructs.spawn(SpawnParams {
                owner: id,
                recipe: s.recipe.clone(),
                shape: None,
                pos: s.pos,
                rot: s.rot,
                vel: Vec3::ZERO,
                target: self.players[i].last_target,
                cost: 0.0,
                integrity: integrity_for_cost(6.0),
                forming,
                life: lifetime_secs(s.recipe.form),
                tick,
                colossal: false,
                model: None,
            });
            self.inject_event(GameEvent::ConstructSpawned { id: cid, owner: id, recipe: Box::new(s.recipe), pos: s.pos, rot: s.rot, forming, cost: 0.0 });
            rebuilt = rebuilt.saturating_add(1);
        }
        if list.is_empty() && rebuilt == 0 {
            return false;
        }
        self.inject_event(GameEvent::Power(PowerEvent::Restored { player: id, center, radius, voxels: list, constructs: rebuilt }));
        true
    }

    fn op_transform(&mut self, i: usize, def: &'static PowerDef) -> Result<(), PowerError> {
        let Some(t) = def.transform.as_ref() else {
            // A tiered power without a transform: toggle base <-> its first tier.
            if def.tiers.is_empty() {
                return Err(PowerError::NoSuchMove);
            }
            let to = if self.players[i].power.tier == 0 { 1 } else { 0 };
            return self.set_tier(i, to);
        };
        let id = self.players[i].id;
        let on = !self.players[i].power.transformed;
        // POWER-UPS HOOK: an awakening needs its pickup (Locked: the plain power-up flare, no form).
        let Some(spend) = crate::pickups::gate_awakening(self, i, on) else { return Ok(()) };
        if let Some(pilot) = &t.pilot {
            if on {
                if self.players[i].seat.is_some() {
                    return Err(PowerError::Busy);
                }
                let (eye, look) = self.eye_look(i);
                let mut r = pilot.to_recipe();
                if !r.has(Mechanic::Piloted) {
                    r.mechanics.push(Mechanic::Piloted);
                    r.mechanics.sort();
                }
                let cid = match self.cast(id, r, None, Aim::new(eye, look)) {
                    Ok(c) => c,
                    Err(CastError::NotEnoughWillpower { .. }) => return Err(PowerError::NotEnoughWillpower),
                    Err(_) => return Err(PowerError::Busy),
                };
                let p = &mut self.players[i];
                p.power.pilot = Some(cid);
                p.power.transformed = true;
            } else {
                let pilot = self.players[i].power.pilot.take();
                self.players[i].power.transformed = false;
                if let Some(cid) = pilot {
                    self.eject(id);
                    if let Some(c) = self.constructs.get_mut(cid) {
                        c.gone = Some(GoneReason::Dismissed);
                    }
                }
            }
        } else {
            self.players[i].power.transformed = on;
        }
        self.inject_event(GameEvent::Power(PowerEvent::Transformed { player: id, on }));
        if on && t.ascend.is_some() {
            let power = self.players[i].power.id;
            self.ascend(i, power, 0, true);
        }
        if spend {
            crate::pickups::spend(self, id, crate::pickups::GateKind::Awakening); // POWER-UPS HOOK
        }
        Ok(())
    }

    // ------------------------------------------------------------------------------ tick

    /// Per-tick power upkeep (after space, before willpower): timers, seastone, void sinking,
    /// landing slams, the stand (follow, mirror damage to the user, resummon), fruits (keep,
    /// conserve, eat), shattered-construct memory. Does nothing (and allocates nothing) when
    /// nobody holds a power and no fruit exists.
    pub(crate) fn step_powers(&mut self, dt: f32) {
        self.step_generic_ops(dt); // GENERIC OPS: rams, hooks, stances, zones… (no-op when unused)
        self.step_toolbox(dt); // TOOLBOX: borrowed cooldowns, active phases, chains (no-op when unused)
        let any = !self.powers.fruits.is_empty() || !self.powers.cores.is_empty() || self.players.iter().any(|p| p.power.id.is_some() || p.power.ascend_cd > 0.0);
        if !any {
            return;
        }
        let base = self.cfg.move_params.clone();
        let mut restorer = false;
        let mut stand_hits: [Option<(usize, Option<PlayerId>, f32, Vec3)>; 16] = [None; 16];
        let mut nh = 0;
        // Damage dealt to stands this tick (from the event list) is mirrored to their users.
        if self.players.iter().any(|p| p.power.companion.is_some()) {
            for e in self.events() {
                if let GameEvent::ConstructHit { id, source, damage, point } = e {
                    if let Some(k) = self.players.iter().position(|p| p.power.companion == Some(*id)) {
                        if *source != Some(self.players[k].id) && nh < stand_hits.len() && damage.is_finite() && *damage > 0.0 {
                            stand_hits[nh] = Some((k, *source, *damage, *point));
                            nh += 1;
                        }
                    }
                }
            }
        }
        for i in 0..self.players.len() {
            if self.players[i].power.id.is_none() {
                continue;
            }
            if !self.players[i].alive {
                continue;
            }
            if self.players[i].power.def().is_none() {
                continue;
            }
            restorer |= self.players[i].power.held().any(|h| h.def.moves.iter().any(|m| m.ops.iter().any(|o| matches!(o, MoveOp::Restore { .. }))));
            let id = self.players[i].id;
            // Timers.
            {
                let pw = &mut self.players[i].power;
                pw.drained = (pw.drained - dt).max(0.0);
                pw.scatter_cd = (pw.scatter_cd - dt).max(0.0);
                for cd in &mut pw.move_cd {
                    *cd = (*cd - dt).max(0.0);
                }
                pw.time_stop = (pw.time_stop - dt).max(0.0);
                pw.rush_t = (pw.rush_t - dt).max(0.0);
                pw.rush_cd = (pw.rush_cd - dt).max(0.0);
                pw.resummon = (pw.resummon - dt).max(0.0);
                pw.seastone_mark = (pw.seastone_mark - dt).max(0.0);
                for o in pw.others.iter_mut().flatten() {
                    for cd in &mut o.move_cd {
                        *cd = (*cd - dt).max(0.0);
                    }
                }
            }
            let (pos, grounded) = (self.players[i].mv.pos, self.players[i].mv.grounded);
            if grounded {
                self.players[i].power.last_ground = Some(pos);
            }
            // Fruit users over the void sink faster.
            if self.players[i].power.is_fruit() {
                let over = !grounded && bd_core::voxel::raycast(&self.voxels, pos + Vec3::Y * 0.1, Vec3::NEG_Y, 80.0).is_none();
                self.players[i].power.over_void = over;
                // Seastone: linked by a seastone chain (or to a seastone construct).
                let mark = self.players[i].power.seastone_mark > 0.0;
                let linked = self.links.iter().any(|l| {
                    let mine = matches!(l.a, LinkEnd::Player(p) if p == id) || matches!(l.b, LinkEnd::Player(p) if p == id);
                    let stone_end = [l.a, l.b].iter().any(|e| matches!(e, LinkEnd::Construct(c) if self.constructs.get(*c).is_some_and(|c| hooks::is_seastone(&c.recipe))));
                    mine && (stone_end || mark || self.players[i].power.seastone)
                });
                if linked != self.players[i].power.seastone {
                    self.players[i].power.seastone = linked;
                    self.inject_event(GameEvent::Power(PowerEvent::Seastone { player: id, on: linked }));
                }
            }
            // Landing slam.
            if grounded {
                if let Some([radius, damage, knockback, carve]) = self.players[i].power.slam.take() {
                    self.explode(Explosion { pos, radius, damage, knockback, source: Some(id), construct: None, element: Element::Earth, carve_radius: carve, self_damage: false });
                }
            }
            if !self.players[i].alive {
                continue;
            }
            // Piloted transform ends when the seat does.
            if let Some(cid) = self.players[i].power.pilot {
                if self.players[i].seat.is_none_or(|s| s.construct != cid) {
                    let pw = &mut self.players[i].power;
                    pw.pilot = None;
                    let pilots = |id: u16| PowerTable::get().def(id).is_some_and(|d| d.transform.as_ref().is_some_and(|t| t.pilot.is_some()));
                    if pw.id.is_some_and(pilots) {
                        pw.transformed = false;
                    }
                    for o in pw.others.iter_mut().flatten() {
                        if pilots(o.id) {
                            o.transformed = false;
                        }
                    }
                    self.inject_event(GameEvent::Power(PowerEvent::Transformed { player: id, on: false }));
                }
            }
            // The stand (and queued rush punches).
            if self.players[i].power.companion_def().is_some() {
                self.step_companion(i, &base, dt);
                let pw = &mut self.players[i].power;
                if pw.rush_queue > 0 && pw.rush_cd <= 0.0 {
                    pw.rush_queue -= 1;
                    self.rush_punch(i);
                }
            }
        }
        for (k, source, damage, point) in stand_hits.iter().flatten().copied() {
            if let Some(p) = self.players.get(k).filter(|p| p.alive) {
                let tid = p.id;
                let dir = p.hull(&base).center() - point;
                self.apply_hit(HitInfo::new(tid, source, HitKind::Creature, damage, 0.0, dir, point).with_impact(0.8));
            }
        }
        self.step_power_effects(dt);
        self.step_fruits(&base, dt);
        if restorer {
            self.record_shattered(dt);
        }
    }

    fn step_companion(&mut self, i: usize, base: &crate::movement::MoveParams, dt: f32) {
        let _ = base;
        let p = &self.players[i];
        let id = p.id;
        let fwd = flat(p.look());
        let side = Vec3::Y.cross(fwd).normalize_or(Vec3::X);
        let (feet, vel, rushing) = (p.mv.pos, p.mv.vel, p.power.rush_t > 0.0);
        let infinite = self.cfg.rules.infinite_willpower();
        let wmax = self.cfg.rules.willpower_max();
        let cid = self.players[i].power.companion;
        let alive_stand = match cid.and_then(|c| self.constructs.get_mut(c)) {
            Some(c) => {
                if c.gone.is_some() && c.shatter.is_some() {
                    // Stands don't shatter — the damage went to the user.
                    c.gone = None;
                    c.shatter = None;
                    c.comp.frozen = 0.0;
                }
                if c.gone.is_none() {
                    c.integrity = c.max_integrity;
                    c.life = INF;
                    let want = if rushing { feet + fwd * 1.0 + Vec3::Y * 0.2 } else { feet - fwd * 0.9 + side * 0.35 + Vec3::Y * 0.5 };
                    c.pos = if c.pos.is_finite() { c.pos + (want - c.pos) * (18.0 * dt).min(1.0) } else { want };
                    c.vel = vel;
                    c.rot = geom::yaw_rotation(fwd).inverse();
                    Some(upkeep_per_sec_as(&c.recipe, false, false)) // stands: never autonomy-priced
                } else {
                    None
                }
            }
            None => None,
        };
        match alive_stand {
            Some(upkeep) => {
                // The stand is the power, not a cast: no upkeep.
                if !infinite {
                    let p = &mut self.players[i];
                    p.willpower = (p.willpower + upkeep * dt).min(wmax);
                }
            }
            None => {
                let pw = &mut self.players[i].power;
                if pw.companion.is_some() {
                    pw.companion = None;
                    pw.resummon = 1.5;
                } else if pw.resummon <= 0.0 {
                    self.spawn_companion(i);
                }
            }
        }
        let _ = id;
    }

    fn step_fruits(&mut self, base: &crate::movement::MoveParams, dt: f32) {
        let void_y = self.arena.void_y;
        let mut k = 0;
        while k < self.powers.fruits.len() {
            let f = self.powers.fruits[k];
            let state = self.constructs.get_mut(f.construct).map(|c| {
                if c.gone.is_none() {
                    c.life = INF;
                }
                (c.gone.is_none(), c.center())
            });
            match state {
                Some((true, pos)) => {
                    let fr = &mut self.powers.fruits[k];
                    fr.age += dt;
                    if pos.is_finite() && pos.y > void_y + 1.0 {
                        fr.last_pos = pos;
                    }
                    k += 1;
                }
                _ => {
                    // Destroyed / lost: a fruit is never lost — it reappears (where it was, or
                    // on a random spawn if it fell into the void).
                    self.powers.fruits.remove(k);
                    let fell = state.is_none_or(|(_, p)| !p.is_finite() || p.y < void_y + 1.0) && f.last_pos.y < void_y + 3.0;
                    let at = if fell || !self.in_play(f.last_pos, 0.0) {
                        let n = self.arena.spawns.len().max(1) as u32;
                        let s = self.rng.below(n) as usize;
                        self.arena.spawns.get(s).copied().unwrap_or(self.arena.center) + Vec3::Y
                    } else {
                        f.last_pos + Vec3::Y * 0.3
                    };
                    let owner = if self.pidx(f.owner).is_some() { Some(f.owner) } else { self.players.first().map(|p| p.id) };
                    if let Some(owner) = owner {
                        self.spawn_fruit(owner, f.power, at, Vec3::ZERO, true);
                    }
                }
            }
        }
        if !self.cfg.rules.powers_enabled {
            return;
        }
        // Taking: whoever touches a ripe pickup (lowest id first) - only kinds the lobby allows
        // (devil fruits can be off while origins stay on).
        let t = PowerTable::get();
        for _ in 0..MAX_FRUITS {
            let mut found = None;
            'outer: for (k, f) in self.powers.fruits.iter().enumerate() {
                if f.age < FRUIT_EAT_DELAY || !t.def(f.power).is_some_and(|d| self.power_allowed(d)) {
                    continue;
                }
                let Some(c) = self.constructs.get(f.construct).filter(|c| c.gone.is_none()) else { continue };
                let b = geom::inflate(&c.bounds(), 0.35);
                for (pi, p) in self.players.iter().enumerate() {
                    if p.alive && p.seat.is_none_or(|s| s.suit) && p.hull(base).intersects(&b) {
                        found = Some((k, pi));
                        break 'outer;
                    }
                }
            }
            let Some((k, pi)) = found else { break };
            let f = self.powers.fruits.remove(k);
            if let Some(c) = self.constructs.get_mut(f.construct) {
                c.gone = Some(GoneReason::Spent);
            }
            self.take_pickup(pi, f.power);
        }
    }

    /// Remember own constructs that shattered (for Crazy Diamond users).
    fn record_shattered(&mut self, dt: f32) {
        for s in &mut self.powers.shattered {
            s.age += dt;
        }
        self.powers.shattered.retain(|s| s.age <= SHATTER_RESTORE_SECS);
        let mut new = [None::<ConstructId>; 4];
        let mut n = 0;
        for c in self.constructs.iter() {
            if n >= new.len() {
                break;
            }
            if c.gone.is_none() || c.shatter.is_none() || c.parent.is_some() || c.model.is_some() || c.recipe.grants.is_some() {
                continue;
            }
            let owner_restores = self.player(c.owner).is_some_and(|p| p.power.held().any(|h| h.def.moves.iter().any(|m| m.ops.iter().any(|o| matches!(o, MoveOp::Restore { .. })))));
            if owner_restores && !self.powers.shattered.iter().any(|s| s.owner == c.owner && s.pos == c.pos) {
                new[n] = Some(c.id);
                n += 1;
            }
        }
        for cid in new.iter().flatten().copied() {
            let Some(c) = self.constructs.get(cid) else { continue };
            if self.powers.shattered.len() >= SHATTER_LOG {
                self.powers.shattered.remove(0);
            }
            let rec = Shattered { owner: c.owner, recipe: c.recipe.clone(), pos: c.pos, rot: c.rot, age: 0.0 };
            self.powers.shattered.push(rec);
        }
        let _ = DT;
    }
}
