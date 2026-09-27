//! TOTAL LIMIT (owner: "a 'total' constructs/power/etc. limiter … if someone set their lobby to 4,
//! they would be able to fill their hot keys, and if they wanted any more they would have to
//! dismiss"). A host MATCH RULE: [`MatchRules::active_limit`] (0 = off, the shipped default; the
//! lobby offers 2..=8) caps how many ACTIVE things one player has at once.
//!
//! WHAT COUNTS (one each) — [`World::active_count`]:
//! * every live construct you own that stands on its own: summons, creatures, followers, walls,
//!   weapons you hold, things you wear, vehicles, clones (`parent.is_none()`: child parts, swarm
//!   copies, party free copies never count — as for the per-kind caps);
//! * every power you hold: a fruit, a stand, a technique / form, an origin (its stand or transform
//!   body is part of it, not one more).
//!
//! WHAT NEVER COUNTS: one-shot moves (a punch, a blink, a borrowed / kit / described move once it
//! ran), projectiles, the timed zones / fields / portals moves leave behind (they expire on their
//! own), grafts onto something you already have, fruit / origin pickups lying on the map.
//!
//! THE RULE: at the limit, a call that would ADD one more thing does not happen, and costs nothing
//! (checked before any price is paid: `CastError::AtLimit`, a wordless cue on the caller's side). A
//! call that REPLACES is allowed — the total doesn't grow: the per-kind construct caps
//! (`MatchRules::limits`, replace-oldest) are full for that kind, `max_powers` pushes the oldest
//! power out, a second stand replaces the first, a power you already hold is just re-selected.
//! A fruit / origin you touch at the limit stays on the map. Host and prediction run the same code
//! on the replicated rules, so they agree.
//!
//! DISMISS: [`World::drop_power`] lets a held power go by id (bd_game DISMISS BY SLOT: "x → 3").

use crate::constructs::Slot;
use crate::powers::PowerTable;
use crate::world::World;
use bd_core::mechanics::{Form, Recipe};
use bd_core::PlayerId;

/// The lobby's values for [`crate::MatchRules::active_limit`] (0 = off).
pub const LIMIT_VALUES: [u8; 8] = [0, 2, 3, 4, 5, 6, 7, 8];

impl World {
    /// Active things `player` has now (see the module docs).
    pub fn active_count(&self, player: PlayerId) -> u32 {
        let Some(p) = self.player(player) else { return 0 };
        let body = [p.power.companion, p.power.pilot];
        let constructs = self.constructs.owned_by(player).filter(|c| c.parent.is_none() && !body.contains(&Some(c.id))).count();
        (constructs + p.power.count()) as u32
    }

    /// The rule's cap for this match (`None` = off).
    pub fn active_limit(&self) -> Option<u32> {
        self.cfg.rules.active_cap()
    }

    /// At (or over) the limit.
    pub fn at_active_limit(&self, player: PlayerId) -> bool {
        self.active_limit().is_some_and(|l| self.active_count(player) >= l)
    }

    /// Would casting `recipe` as a fresh construct ADD one (no per-kind cap would replace one)?
    pub(crate) fn construct_would_add(&self, player: PlayerId, recipe: &Recipe) -> bool {
        if self.free_cast {
            return false; // a child copy: never counts
        }
        let rules = &self.cfg.rules;
        let slot = Slot::of(recipe.form);
        if self.constructs.in_slot(player, slot).count() >= rules.slot_limit(slot) {
            return false;
        }
        if recipe.form == Form::Kaiju && self.constructs.owned_by(player).filter(|c| c.recipe.form == Form::Kaiju).count() >= rules.kaiju_limit() {
            return false;
        }
        self.constructs.owned_by(player).filter(|c| c.parent.is_none()).count() < rules.total_limit()
    }

    /// TOTAL LIMIT: refuse a fresh construct (before anything is paid).
    pub(crate) fn limit_refuses_construct(&self, player: PlayerId, recipe: &Recipe) -> bool {
        self.at_active_limit(player) && self.construct_would_add(player, recipe)
    }

    /// Would gaining `power` ADD one (not held yet, no `max_powers` / one-stand replacement)?
    pub(crate) fn power_would_add(&self, i: usize, power: u16) -> bool {
        let pw = &self.players[i].power;
        if pw.slot_of(power).is_some() {
            return false;
        }
        let t = PowerTable::get();
        if t.def(power).is_some_and(|d| d.companion.is_some()) && pw.held().any(|h| h.def.companion.is_some()) {
            return false;
        }
        let max = (t.rules.max_powers as usize).clamp(1, 1 + crate::powers::MAX_EXTRA_POWERS);
        pw.count() < max
    }

    /// TOTAL LIMIT: refuse a new power (a said grant, a touched pickup).
    pub(crate) fn limit_refuses_power(&self, i: usize, power: u16) -> bool {
        self.at_active_limit(self.players[i].id) && self.power_would_add(i, power)
    }

    /// DISMISS BY SLOT: let the held power `power` go (its stand / body with it; a fruit / origin
    /// pops back out at your feet, as when it is replaced). `false` = not held.
    pub fn drop_power(&mut self, player: PlayerId, power: u16) -> bool {
        let Some(i) = self.pidx(player) else { return false };
        let Some(slot) = self.players[i].power.slot_of(power) else { return false };
        let feet = self.players[i].mv.pos + bd_core::Vec3::Y * 0.5;
        // (`drop_held` sends `PowerEvent::Lost`: the clean exit presentation)
        self.drop_held(i, slot, Some(feet));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::CastError;
    use crate::world::{Aim, ArenaSource, SimConfig};
    use bd_core::corps::Corps;
    use bd_core::Vec3;

    const ME: PlayerId = PlayerId(1);

    fn world(limit: u8) -> World {
        let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, ..SimConfig::default() };
        cfg.rules.active_limit = limit;
        cfg.rules.willpower = crate::rules::WillpowerMode::Infinite;
        let mut w = World::new(cfg);
        w.add_player(ME, "me".into(), Corps::from_index(1)).unwrap();
        w.add_player(PlayerId(2), "you".into(), Corps::from_index(2)).unwrap();
        w.start_live();
        w.tick(&[]);
        let p = w.player_mut(ME).unwrap();
        p.spawn_shield = 0.0;
        p.mv.pos = Vec3::new(0.0, 8.0, 0.0);
        w.tick(&[]);
        w
    }

    fn cast(w: &mut World, form: Form, name: &str) -> Result<bd_core::ConstructId, CastError> {
        let r = crate::bots::recipe(name, form, &[], bd_core::mechanics::Element::Light, 1.5);
        let eye = w.player(ME).unwrap().eye(w.params());
        w.cast(ME, r, None, Aim::new(eye, Vec3::new(0.3, -0.4, -1.0).normalize()))
    }

    fn pick_power(pred: impl Fn(&crate::powers::PowerDef) -> bool) -> String {
        PowerTable::get().powers.iter().find(|p| pred(p)).map(|p| p.id.clone()).expect("a power")
    }

    #[test]
    fn counts_constructs_and_powers_not_parts() {
        let mut w = world(0);
        assert_eq!(w.active_count(ME), 0);
        cast(&mut w, Form::Wall, "wall").unwrap();
        cast(&mut w, Form::Quadruped, "wolf").unwrap();
        assert_eq!(w.active_count(ME), 2);
        let stand = pick_power(|p| p.companion.is_some() && p.kind.is_stand());
        assert!(w.grant_power(ME, &stand));
        // the stand's body is part of the power: +1, not +2
        assert_eq!(w.active_count(ME), 3);
        let n = w.dismiss(ME);
        assert!(n >= 2);
        w.tick(&[]);
        assert_eq!(w.active_count(ME), 1, "only the power is left (its stand body re-forms, uncounted)");
    }

    #[test]
    fn at_the_limit_a_new_thing_is_refused_for_free_but_replacing_is_allowed() {
        let mut w = world(2);
        w.cfg.rules.willpower = crate::rules::WillpowerMode::Normal;
        w.player_mut(ME).unwrap().willpower = 100.0;
        cast(&mut w, Form::Wall, "wall").unwrap();
        cast(&mut w, Form::Quadruped, "wolf").unwrap();
        assert_eq!(w.active_count(ME), 2);
        let before = w.player(ME).unwrap().willpower;
        // a third, different kind: refused, nothing paid
        let r = cast(&mut w, Form::Sword, "sword");
        assert_eq!(r, Err(CastError::AtLimit));
        assert_eq!(w.player(ME).unwrap().willpower, before, "the attempt costs nothing");
        assert_eq!(w.active_count(ME), 2);
        // a second wall while the wall slot has room: refused; fill the kind's cap → replace-oldest ok
        w.cfg.rules.limits.structures = 1;
        cast(&mut w, Form::Wall, "another wall").expect("replaces the old wall: the total doesn't grow");
        w.tick(&[]);
        assert_eq!(w.active_count(ME), 2);
        // a new power at the limit: refused (the phrase path answers AtLimit)
        let tech = pick_power(|p| p.kind.is_technique() && p.companion.is_none());
        let i = w.pidx(ME).unwrap();
        let id = PowerTable::get().index_of(&tech).unwrap();
        assert!(w.limit_refuses_power(i, id));
        // dismiss one: room again
        let wolf = w.constructs.owned_by(ME).find(|c| c.recipe.form == Form::Quadruped).map(|c| c.id).unwrap();
        assert!(w.dismiss_one(ME, wolf));
        w.tick(&[]);
        assert!(!w.limit_refuses_power(i, id));
        assert!(w.grant_power(ME, &tech));
        assert_eq!(w.active_count(ME), 2);
        // and drop it by id (DISMISS BY SLOT)
        assert!(w.drop_power(ME, id));
        assert_eq!(w.active_count(ME), 1);
        assert!(!w.drop_power(ME, id), "not held any more");
    }

    #[test]
    fn off_never_refuses() {
        let mut w = world(0);
        for k in 0..6 {
            cast(&mut w, [Form::Wall, Form::Quadruped, Form::Sword, Form::Ball][k % 4], "x").unwrap();
        }
        assert!(!w.at_active_limit(ME));
    }
}
