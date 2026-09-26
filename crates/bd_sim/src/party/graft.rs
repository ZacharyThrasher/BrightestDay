//! The **standing graft** (COMBAT.md §5g / §5h): a recipe fragment merged into *every* recipe a
//! player casts, before validation — the same composition engine as a crosshair graft, applied
//! automatically. Shared by party effects and player powers.
//!
//! Per-form sanity follows the composition engine (§5b): a mechanic that means nothing on a form
//! is skipped (legs on a held hammer) or converted to what the engine would do instead (wings on a
//! held item / worn item → the wielder glides).

use bd_core::mechanics::{Element, Feature, Form, FormCategory, Mechanic, Recipe, Temperament};
use serde::{Deserialize, Serialize};

use crate::willpower::{MAX_FEATURES, MAX_MECHANICS};

/// A recipe fragment applied to every cast. Data (`assets/party_effects.ron`, `powers.ron`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StandingGraft {
    /// Mechanics added (subject to per-form sanity, see [`fit_mechanic`]).
    pub mechanics: Vec<Mechanic>,
    /// Element forced onto the recipe (the most recently acquired effect wins).
    pub element: Option<Element>,
    /// Temperament forced onto the recipe (autonomous constructs act on it).
    pub temperament: Option<Temperament>,
    /// Visible parts added (bells, headlights, wheels…).
    pub features: Vec<Feature>,
    /// Only applies to these form categories (empty = every category).
    pub only: Vec<FormCategory>,
    /// At least this many copies (Clown Car). Skipped where Swarm is skipped.
    pub count: Option<u8>,
    /// Size multiplier on the recipe (Clown Car: tiny). The sim clamps per form afterwards.
    pub scale_mult: Option<f32>,
    /// A temperament that is replaced by Calm (Loud: no sneaking).
    pub forbid_temperament: Option<Temperament>,
}

impl StandingGraft {
    pub fn is_empty(&self) -> bool {
        self.mechanics.is_empty()
            && self.element.is_none()
            && self.temperament.is_none()
            && self.features.is_empty()
            && self.count.is_none()
            && self.scale_mult.is_none()
            && self.forbid_temperament.is_none()
    }
}

/// What mechanic `m` becomes on a construct of `form` (None = skipped: meaningless there).
pub fn fit_mechanic(m: Mechanic, form: Form) -> Option<Mechanic> {
    use FormCategory as C;
    use Mechanic::*;
    let cat = form.category();
    let held_worn = matches!(cat, C::Held | C::Worn);
    match m {
        // Legs, swarming, charging and transforming need a free body.
        Walks | Swarm | Charge | Transforms | Trap | DropFromSky | Piloted if held_worn => None,
        // Wings on something you wield or wear: the wielder glides (§5b SlowFall/Glide row).
        Flies if held_worn => Some(Glide),
        // Things you wear don't get thrown, stuck, bounced or chained.
        Returning | Ricochet | Sticky | Homing | Bounce | Link | Attract | Repel | Explodes | Spin if cat == C::Worn => None,
        // Placed structures don't come back or ricochet.
        Returning | Ricochet if cat == C::Structure => None,
        // Sticky / Grab structures would glue the arena together; keep them to free bodies.
        Sticky if cat == C::Structure => None,
        _ => Some(m),
    }
}

/// Merge `g` into `r` (before validation). Returns true if anything changed. Never exceeds the
/// validation bounds: the recipe's own mechanics are kept first; grafted ones fill the rest.
pub fn apply_graft(r: &mut Recipe, g: &StandingGraft) -> bool {
    let cat = r.form.category();
    if !g.only.is_empty() && !g.only.contains(&cat) {
        return false;
    }
    let mut changed = false;
    for &m in &g.mechanics {
        let Some(m) = fit_mechanic(m, r.form) else { continue };
        if r.mechanics.contains(&m) || r.mechanics.len() >= MAX_MECHANICS {
            continue;
        }
        r.mechanics.push(m);
        changed = true;
    }
    if changed {
        r.mechanics.sort();
        r.mechanics.dedup();
    }
    for &f in &g.features {
        if !r.features.contains(&f) && r.features.len() < MAX_FEATURES {
            r.features.push(f);
            r.features.sort();
            changed = true;
        }
    }
    if let Some(e) = g.element
        && r.element != e
    {
        r.element = e;
        changed = true;
    }
    if let Some(t) = g.temperament
        && r.temperament != t
    {
        r.temperament = t;
        r.intensity = r.intensity.max(0.7);
        changed = true;
    }
    if let Some(t) = g.forbid_temperament
        && r.temperament == t
    {
        r.temperament = Temperament::Calm;
        changed = true;
    }
    let swarm_ok = fit_mechanic(Mechanic::Swarm, r.form).is_some();
    if let Some(n) = g.count.filter(|_| swarm_ok)
        && r.count < n
    {
        r.count = n;
        changed = true;
    }
    // Tiny copies only where copies happen (Clown Car on a held item does nothing).
    if let Some(s) = g.scale_mult.filter(|s| s.is_finite() && *s > 0.0)
        && (g.count.is_none() || swarm_ok)
    {
        r.scale *= s;
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::recipe;

    #[test]
    fn legs_skip_held_items_but_walk_walls() {
        let g = StandingGraft { mechanics: vec![Mechanic::Walks], ..Default::default() };
        let mut hammer = recipe("hammer", Form::Hammer, &[Mechanic::Blunt], Element::Light, 1.0);
        assert!(!apply_graft(&mut hammer, &g));
        let mut wall = recipe("wall", Form::Wall, &[], Element::Light, 6.0);
        assert!(apply_graft(&mut wall, &g));
        assert!(wall.has(Mechanic::Walks));
    }

    #[test]
    fn wings_on_held_become_glide() {
        let g = StandingGraft { mechanics: vec![Mechanic::Flies], ..Default::default() };
        let mut sword = recipe("sword", Form::Sword, &[], Element::Light, 1.0);
        apply_graft(&mut sword, &g);
        assert!(sword.has(Mechanic::Glide) && !sword.has(Mechanic::Flies));
    }

    #[test]
    fn mechanic_cap_keeps_own_mechanics() {
        let mut r = recipe("x", Form::Ball, &crate::fuzz::MECHANICS[..MAX_MECHANICS], Element::Light, 1.0);
        let before = r.mechanics.clone();
        let g = StandingGraft { mechanics: vec![Mechanic::Link], ..Default::default() };
        apply_graft(&mut r, &g);
        assert_eq!(r.mechanics, before);
    }
}
