//! Named-target delivery — FREEDOM HOOK, the sim half of bd_lexicon's intent layer ("drop a meteor on
//! Ethan", "missile at Jake", "bees on Sam", "wall around Mia").
//!
//! The client strips the name and sends `TargetHint::Named { player, via }` (the words' delivery). The
//! host validates it ([`World::named_target`]: the player exists, is alive, isn't the caster; range
//! clamped to [`NAMED_RANGE`]) and uses the target **as the caster saw them** (lag-compensated hull
//! history at the caster's view tick). What a delivery does depends on the form category
//! ([`effective`]), never on new numbers:
//!
//! | category  | onto                          | at / hunt                         | near / around                 |
//! |-----------|-------------------------------|-----------------------------------|-------------------------------|
//! | object    | falls from above them (drop telegraph) | thrown / driven at them  | falls from above them         |
//! | creature  | dropped on them if the words dropped it, else next to them | forms here, hunts them | next to them |
//! | structure | dropped on them if dropped, else at their feet | in front of them / at their feet | around them (domes, cages) |
//! | held/worn | in your hand as usual; its homing / autonomy locks onto them                                   |
//!
//! Every explicitly named construct keeps that player as its target (`CompState::named`): homing and
//! creature AI don't drift to whoever the caster hits next. Invalid / out-of-range names fall back to an
//! ordinary cast at the crosshair (the ring never says no).

use bd_core::mechanics::{Form, FormCategory, Mechanic, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::{PlayerId, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::World;
use crate::geom;
use crate::world::TargetHint;

/// Farthest a named delivery reaches (m). Farther targets get it at this distance toward them.
pub const NAMED_RANGE: f32 = 120.0;
/// A creature summoned "near" someone appears this far beyond its own footprint (m).
pub const NEAR_GAP: f32 = 2.5;
/// A wall "at" someone stands this far in front of them (toward the caster, m).
pub const WALL_GAP: f32 = 1.6;

/// How a phrase reaches the named player (the words' preposition / verb; bd_lexicon `Delivery`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Delivery {
    /// From the sky onto them (drop, rain, smite, "on top of").
    Onto,
    /// Thrown / fired / driven toward them (throw, launch, "at").
    At,
    /// Appears next to them (summon, spawn, "near", a creature "on" them).
    Near,
    /// Placed around them (cage, trap, "around", a dome "over" them).
    Around,
    /// Sent after them from here ("after", sic, chase).
    Hunt,
}

/// A validated named target (host truth, see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Named {
    pub player: PlayerId,
    pub via: Delivery,
    /// The target's feet (centre of the hull's floor) as the caster saw them, range-clamped.
    pub feet: Vec3,
    /// The target's hull centre, same view.
    pub center: Vec3,
    /// Level unit vector from the target toward the caster.
    pub back: Vec3,
}

/// What a delivery becomes for this recipe (see the table in the module docs).
pub fn effective(via: Delivery, r: &Recipe) -> Delivery {
    let drops = r.has(Mechanic::DropFromSky) || r.form == Form::Anvil;
    let enclosure = matches!(r.form, Form::Dome | Form::Cage);
    match r.form.category() {
        FormCategory::Held | FormCategory::Worn => Delivery::Hunt,
        FormCategory::Object => match via {
            Delivery::At | Delivery::Hunt if !drops => Delivery::At,
            _ => Delivery::Onto,
        },
        FormCategory::Creature => match via {
            Delivery::Onto if drops => Delivery::Onto,
            Delivery::Onto | Delivery::Near | Delivery::Around => Delivery::Near,
            Delivery::At | Delivery::Hunt => Delivery::Hunt,
        },
        FormCategory::Structure => match via {
            Delivery::Onto if drops => Delivery::Onto,
            _ if enclosure => Delivery::Around,
            Delivery::Around => Delivery::Around,
            _ => Delivery::Near,
        },
    }
}

/// Recipe changes a delivery implies: an object delivered onto someone falls from the sky.
pub fn prepare(recipe: &mut Recipe, via: Delivery) {
    if effective(via, recipe) == Delivery::Onto && recipe.form.category() == FormCategory::Object && !recipe.has(Mechanic::DropFromSky) {
        recipe.mechanics.push(Mechanic::DropFromSky);
        *recipe = recipe.clone().normalized();
    }
}

impl World {
    /// Host validation of a named target for player index `i` casting from `origin` (module docs).
    pub(crate) fn named_target(&self, i: usize, hint: Option<TargetHint>, origin: Vec3) -> Option<Named> {
        let Some(TargetHint::Named { player: t, via }) = hint else { return None };
        let caster = self.players.get(i)?;
        if t == caster.id || !origin.is_finite() {
            return None;
        }
        let tp = self.player(t)?;
        if !tp.alive || tp.eliminated {
            return None;
        }
        let base = &self.cfg.move_params;
        // Lag compensation: where the caster saw them (the same rewind hit tests use).
        let hull = caster.view_tick.and_then(|vt| self.history.hull_at(t, vt)).unwrap_or_else(|| tp.hull(base));
        let mut center = (hull.min + hull.max) * 0.5;
        let mut feet = Vec3::new(center.x, hull.min.y, center.z);
        if !center.is_finite() {
            return None;
        }
        let to = center - origin;
        let d = to.length();
        if d > NAMED_RANGE {
            let shift = to * (NAMED_RANGE / d) - to;
            center += shift;
            feet = self.ground_point_below(center + Vec3::Y * 2.0).unwrap_or(feet + shift);
        }
        let back = Vec3::new(origin.x - feet.x, 0.0, origin.z - feet.z).normalize_or(Vec3::Z);
        Some(Named { player: t, via, feet, center, back })
    }

    /// Where a named delivery puts a fresh construct (replaces the crosshair / visible placement).
    /// `default` = the ordinary placement computed with `aim_point = feet` and the look toward them
    /// (objects in the hand or 12 m above them, drops at their feet, ramps / pillars under them).
    pub(crate) fn deliver_place(&self, r: &Recipe, shape: Option<&ShapeSpec>, n: &Named, default: (Vec3, Quat)) -> (Vec3, Quat) {
        // the ordinary facing convention: yaw of the caster's look (toward the target)
        let face = geom::yaw_rotation(-n.back);
        let (half, _) = crate::constructs::body_box(r, shape);
        let foot = Vec3::new(half.x, 0.0, half.z).length();
        let ground = |p: Vec3| self.ground_point_below(p + Vec3::Y * 2.0).unwrap_or(p);
        match (r.form.category(), effective(n.via, r)) {
            (FormCategory::Creature, Delivery::Near) => {
                let flier = r.has(Mechanic::Flies) || matches!(r.form, Form::Bird | Form::Insect);
                let at = n.feet + n.back * (foot + NEAR_GAP);
                let pos = if flier { at + Vec3::Y * (half.y + 1.5) } else { ground(at) };
                // creatures use the inverse yaw (actions.rs); this one looks at the target
                (pos, face.inverse())
            }
            // domes / cages close around them
            (FormCategory::Structure, Delivery::Around) => (ground(n.feet), face),
            // a wall stands in front of them, between you
            (FormCategory::Structure, Delivery::Near) if r.form == Form::Wall => (ground(n.feet + n.back * (WALL_GAP + half.z)), face),
            _ => default,
        }
    }
}

/// Launch direction for a construct thrown at its named target: a ballistic lob at `speed` from `from`
/// to the target's centre (`gravity` m/s², 0 for fliers / thrusters), or `None` without a live target.
pub fn lob(from: Vec3, target: Vec3, target_vel: Vec3, speed: f32, gravity: f32) -> Option<Vec3> {
    let to = target - from;
    let d = to.length();
    if !d.is_finite() || d < 0.5 || speed <= 0.1 {
        return None;
    }
    let t = (d / speed).min(3.0);
    // a little lead (half of the time of flight: players dodge; this is a throw, not a hitscan)
    let aim = target + target_vel * (t * 0.5);
    let flat = aim - from;
    let v = flat / t + Vec3::Y * (0.5 * gravity * t);
    Some(v.clamp_length_max(speed * 1.6))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::mechanics::Recipe;

    fn r(form: Form, mech: &[Mechanic]) -> Recipe {
        let mut x = Recipe::unknown("t");
        x.form = form;
        x.mechanics = mech.to_vec();
        x.normalized()
    }

    #[test]
    fn deliveries_by_category() {
        assert_eq!(effective(Delivery::Onto, &r(Form::Boulder, &[Mechanic::DropFromSky])), Delivery::Onto);
        assert_eq!(effective(Delivery::At, &r(Form::Rocket, &[Mechanic::Homing])), Delivery::At);
        assert_eq!(effective(Delivery::At, &r(Form::Anvil, &[])), Delivery::Onto, "an anvil always drops");
        assert_eq!(effective(Delivery::Near, &r(Form::Insect, &[Mechanic::Swarm])), Delivery::Near);
        assert_eq!(effective(Delivery::Onto, &r(Form::Insect, &[])), Delivery::Near, "bees on sam appear next to him");
        assert_eq!(effective(Delivery::Onto, &r(Form::Kaiju, &[Mechanic::DropFromSky])), Delivery::Onto);
        assert_eq!(effective(Delivery::Hunt, &r(Form::Quadruped, &[])), Delivery::Hunt);
        assert_eq!(effective(Delivery::Onto, &r(Form::Dome, &[])), Delivery::Around);
        assert_eq!(effective(Delivery::At, &r(Form::Cage, &[])), Delivery::Around);
        assert_eq!(effective(Delivery::Onto, &r(Form::Sword, &[])), Delivery::Hunt);
        let mut car = r(Form::Vehicle, &[]);
        prepare(&mut car, Delivery::Onto);
        assert!(car.has(Mechanic::DropFromSky), "a car dropped on someone falls from the sky");
        let mut rock = r(Form::Boulder, &[]);
        prepare(&mut rock, Delivery::At);
        assert!(!rock.has(Mechanic::DropFromSky));
    }

    #[test]
    fn lob_reaches() {
        let v = lob(Vec3::ZERO, Vec3::new(0.0, 0.0, -30.0), Vec3::ZERO, 20.0, 20.0).unwrap();
        // integrate
        let (mut p, mut vel) = (Vec3::ZERO, v);
        for _ in 0..(1.5 / 0.01) as usize {
            vel.y -= 20.0 * 0.01;
            p += vel * 0.01;
        }
        assert!((p - Vec3::new(0.0, 0.0, -30.0)).length() < 1.0, "{p:?}");
        assert!(lob(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 20.0, 20.0).is_none());
    }
}
