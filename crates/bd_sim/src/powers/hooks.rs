//! Where powers plug into the world: the cast modifier pipeline (step 1, before party effects),
//! the pre-damage hit hook, KO, carve recording and round reset. Each is called from one
//! commented line in `world/**`.

use bd_core::mechanics::{Element, Mechanic, Recipe};
use bd_core::{ConstructId, Vec3};

use super::*;
use crate::combat::{HitInfo, HitKind};
use crate::events::{CastError, GameEvent};
use crate::movement::{CollisionWorld, raycast_world};
use crate::willpower::{MAX_FEATURES, MAX_MECHANICS, validate_recipe};
use crate::world::World;

/// Merge a standing graft into a recipe. Returns true if anything changed.
pub(crate) fn merge_graft(r: &mut Recipe, g: &GraftDef) -> bool {
    let before = (r.mechanics.len(), r.features.len(), r.element);
    for &m in &g.mechanics {
        // A held / worn item can't walk or fly off on its own; flight on it means gliding.
        let m = match (m, r.form.category()) {
            (Mechanic::Walks, bd_core::mechanics::FormCategory::Held | bd_core::mechanics::FormCategory::Worn) => continue,
            (Mechanic::Flies, bd_core::mechanics::FormCategory::Held | bd_core::mechanics::FormCategory::Worn) => Mechanic::Glide,
            (m, _) => m,
        };
        if !r.has(m) && r.mechanics.len() < MAX_MECHANICS {
            r.mechanics.push(m);
            r.mechanics.sort();
        }
    }
    for &f in &g.features {
        if !r.features.contains(&f) && r.features.len() < MAX_FEATURES {
            r.features.push(f);
            r.features.sort();
        }
    }
    if let Some(e) = g.element {
        if r.element == Element::Light {
            r.element = e;
        }
    }
    before != (r.mechanics.len(), r.features.len(), r.element)
}

fn apply_delivery(r: &mut Recipe, d: &Delivery) {
    r.mechanics.retain(|m| !d.remove.contains(m));
    for &m in &d.add {
        if !r.has(m) {
            r.mechanics.push(m);
            r.mechanics.sort();
        }
    }
    // Delivery mechanics win over the phrase's own when the list is full.
    if r.mechanics.len() > MAX_MECHANICS {
        let keep: Vec<Mechanic> = r.mechanics.iter().copied().filter(|m| d.add.contains(m)).collect();
        let mut rest: Vec<Mechanic> = r.mechanics.iter().copied().filter(|m| !d.add.contains(m)).collect();
        rest.truncate(MAX_MECHANICS.saturating_sub(keep.len()));
        r.mechanics = keep;
        r.mechanics.extend(rest);
        r.mechanics.sort();
    }
    if let Some(e) = d.element {
        r.element = e;
    }
    if d.scale_mult.is_finite() && d.scale_mult > 0.0 {
        r.scale *= d.scale_mult.clamp(0.25, 4.0);
    }
}

/// Seastone: a Water-element Link, or a phrase naming seastone.
pub(crate) fn is_seastone(r: &Recipe) -> bool {
    if !r.has(Mechanic::Link) {
        return false;
    }
    if r.element == Element::Water {
        return true;
    }
    let n = norm_words(&r.name);
    PowerTable::get().rules.seastone_words.iter().any(|w| !w.is_empty() && n.contains(w.as_str()))
}

impl World {
    /// Is this power allowed by the lobby rules right now?
    pub(crate) fn power_allowed(&self, def: &PowerDef) -> bool {
        let r = &self.cfg.rules;
        // Origins have no lobby toggle of their own (feature freeze): the powers toggle gates them.
        r.powers_enabled && if def.kind.is_fruit() { r.devil_fruits } else if def.kind.is_stand() { r.stands } else { true }
    }

    /// Cast modifier pipeline, step 1 (powers; party effects run after). `Err(done)` = the
    /// powers module handled the cast itself (a fruit or a stand); `Ok(recipe)` = continue with
    /// the (possibly grafted / incanted, re-validated) recipe.
    pub(crate) fn powers_on_cast(&mut self, i: usize, recipe: Recipe, origin: Vec3, dir: Vec3) -> Result<Recipe, Result<ConstructId, CastError>> {
        let incanting = std::mem::take(&mut self.powers.incanting);
        self.powers.delivered = None;
        if recipe.grants.is_some() && self.cfg.rules.powers_enabled {
            if let Some(done) = self.cast_grant(i, &recipe, origin, dir) {
                return Err(done);
            }
        }
        let pw = &self.players[i].power;
        let Some(def) = pw.def() else { return Ok(recipe) };
        if !pw.active() || !self.power_allowed(def) {
            return Ok(recipe);
        }
        let mut r = recipe;
        let mut changed = false;
        // Every held power lends its grafts (base, tier, transform): powers MIX on your constructs.
        for h in pw.held() {
            if !self.power_allowed(h.def) {
                continue;
            }
            for g in h.grafts() {
                changed |= merge_graft(&mut r, g);
            }
        }
        // Incantation: the delivery of the active power if it has one, else the first held power
        // that does (a tier may bring its own: Gear Second's JET).
        if incanting {
            let deliv = pw.held().find(|h| !h.def.incantations.is_empty() || h.def.tier(h.tier).is_some_and(|t| !t.incantations.is_empty())).map(|h| {
                h.def.tier(h.tier).and_then(|t| t.delivery.as_ref()).unwrap_or(&h.def.delivery)
            });
            if let Some(d) = deliv {
                apply_delivery(&mut r, d);
                self.powers.delivered = Some((d.from_fist, d.arm));
                changed = true;
            }
        }
        if changed {
            r = validate_recipe(r, &self.cfg.rules);
        }
        Ok(r)
    }

    /// Pre-damage hook: water drain (fruits) / authored weaknesses (any power), seastone marks
    /// (fruits), Logia scatter, immunities, rubber, Bara Bara.
    /// `None` = the hit is cancelled entirely.
    pub(crate) fn powers_on_hit(&mut self, ti: usize, hit: HitInfo) -> Option<HitInfo> {
        // Attacker side first: PowerMult damage / knockback, Black Flash crits.
        let hit = if hit.source.is_some_and(|s| s != hit.target) { self.powers_attacker_mods(hit) } else { hit };
        // GENERIC OPS: guard / counter / deflect stances answer first; hard hits break rams / channels.
        let hit = self.ops_on_hit(ti, hit)?;
        let Some(def) = self.players[ti].power.def() else { return Some(hit) };
        if !self.power_allowed(def) || self.players[ti].shielded() || self.match_state.phase == crate::rules::MatchPhase::Ended {
            return Some(hit);
        }
        let id = self.players[ti].id;
        let delivered_by = hit.construct.and_then(|c| self.constructs.get(c));
        let (blade, pierce, seastone) = delivered_by.map_or((false, false, false), |c| (c.has(Mechanic::Blade), c.has(Mechanic::Pierce), is_seastone(&c.recipe)));
        let rules = &PowerTable::get().rules;
        let fruit = self.players[ti].power.is_fruit();
        if fruit && seastone {
            self.players[ti].power.seastone_mark = 0.5;
        }
        // Water drains devil fruits only; an authored weakness (kryptonite = Poison) drains any power.
        let weak = self.players[ti].power.held().any(|h| h.def.body.weak.contains(&hit.element));
        if ((fruit && hit.element == Element::Water) || weak) && hit.element != Element::Light {
            let p = &mut self.players[ti].power;
            let fresh = p.drained <= 0.0;
            p.drained = p.drained.max(rules.drain_secs);
            if fresh {
                let secs = p.drained;
                self.inject_event(GameEvent::Power(PowerEvent::Drained { player: id, secs }));
            }
        }
        let pw = &self.players[ti].power;
        if !pw.active() {
            return Some(hit);
        }
        // Ultra Instinct: the body may move on its own before anything else.
        if self.powers_auto_dodge(ti, &hit) {
            return None;
        }
        let pw = &self.players[ti].power;
        // Every held power's body counts (a rubber Logia shrugs both ways).
        let (mut scatter, mut immune, mut bounce, mut blade_immune, mut explosion_immune) = (None, false, false, false, false);
        for h in pw.held() {
            let b = h.body();
            if b.scatter && scatter.is_none() {
                scatter = Some(h.def);
            }
            immune |= b.immune.contains(&hit.element);
            bounce |= b.bounce;
            blade_immune |= b.blade_immune;
            explosion_immune |= b.explosion_immune;
        }
        let mut hit = hit;
        // Logia: dissolve and reform beside the hit.
        if let Some(sdef) = scatter
            && pw.scatter_cd <= 0.0
            && hit.kind != HitKind::WallSplat
            && hit.element != Element::Water
            && (hit.damage > 0.0 || hit.knockback > 0.0)
        {
            self.scatter(ti, &hit, sdef);
            return None;
        }
        let mut shrug = None;
        if immune && hit.element != Element::Light {
            hit.damage = 0.0;
            hit.knockback *= 0.5;
            hit.element = Element::Light;
            shrug = Some(Shrug::Immune);
        }
        if explosion_immune && hit.kind == HitKind::Explosion {
            hit.damage = 0.0;
            shrug = Some(Shrug::Blastproof);
        }
        if blade_immune && blade {
            self.inject_event(GameEvent::Power(PowerEvent::Shrugged { player: id, why: Shrug::BladePassed, point: hit.point }));
            return None;
        }
        let blunt_kind = matches!(hit.kind, HitKind::RingBlast | HitKind::Melee | HitKind::HeavyMelee | HitKind::Shot | HitKind::Thrown | HitKind::Creature | HitKind::Crush);
        if bounce && blunt_kind && !blade && !pierce {
            hit.damage *= 0.25;
            hit.knockback *= 1.35;
            shrug = Some(Shrug::Bounced);
        }
        if let Some(why) = shrug {
            self.inject_event(GameEvent::Power(PowerEvent::Shrugged { player: id, why, point: hit.point }));
        }
        Some(hit)
    }

    /// Logia scatter: reform ~1 m to the side of the hit (never into terrain).
    fn scatter(&mut self, ti: usize, hit: &HitInfo, def: &PowerDef) {
        let rules = &PowerTable::get().rules;
        let base = self.cfg.move_params.clone();
        let p = &self.players[ti];
        let id = p.id;
        let from = p.mv.pos;
        let center = p.hull(&base).center();
        let d = Vec3::new(hit.impulse_dir.x, 0.0, hit.impulse_dir.z).normalize_or(Vec3::X);
        // Alternate sides by tick parity (deterministic, unpredictable enough to read as fluid).
        let side = if self.tick % 2 == 0 { Vec3::Y.cross(d) } else { d.cross(Vec3::Y) };
        let side = side.normalize_or(Vec3::X);
        let want = rules.scatter_dist.clamp(0.3, 3.0);
        let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(crate::movement::ColliderId::Player(id));
        let mut dist = want;
        for off in [Vec3::ZERO, Vec3::Y * 0.6, Vec3::Y * -0.6] {
            if let Some(h) = raycast_world(&cw, center + off, side, want + 0.45, false) {
                dist = dist.min((h.t - 0.45).max(0.0));
            }
        }
        let to = from + side * dist;
        let p = &mut self.players[ti];
        p.mv.pos = to;
        p.power.scatter_cd = rules.scatter_cooldown;
        let element = scatter_element(def);
        self.inject_event(GameEvent::Power(PowerEvent::Scattered { player: id, from, to, element }));
    }

    /// KO: the power is lost; a fruit / origin respawns where its holder died.
    pub(crate) fn powers_on_ko(&mut self, i: usize) {
        if self.players[i].power.id.is_none() {
            return;
        }
        let p = &self.players[i];
        let pos = if self.in_play(p.mv.pos, 0.0) && p.mv.pos.y > self.arena.void_y + 1.0 {
            p.mv.pos
        } else {
            p.power.last_ground.unwrap_or(self.arena.center + Vec3::Y)
        };
        self.lose_power(i, Some(pos));
    }

    /// Terrain removed by a carve (or its collapse) — remembered for Crazy Diamond.
    pub(crate) fn powers_record_carve(&mut self, out: &bd_voxel::CarveOutcome) {
        for r in &out.removed {
            self.powers.record_removed(r.pos, r.material);
        }
        for c in &out.falling {
            for &(v, m) in &c.voxels {
                self.powers.record_removed(v, m);
            }
        }
    }

    /// New round: fruits, logs and every player's power are cleared.
    pub(crate) fn powers_reset_round(&mut self) {
        self.powers.clear();
        for p in &mut self.players {
            p.power = PlayerPower::default();
        }
    }
}

/// The element a Logia scatters into (its graft element, else its first immunity).
fn scatter_element(def: &PowerDef) -> Element {
    def.graft.as_ref().and_then(|g| g.element).or_else(|| def.body.immune.first().copied()).unwrap_or(Element::Light)
}
