//! PRESENCE TIER (pure; moved from bd_game present/presence/tier.rs, which re-exports it so the
//! client, the readability survey and the phrase preview tool share one function): how much gravitas a construct gets, as a pure function of data every construct
//! already carries (recipe scale / count / cost, Heavy / Explodes / DropFromSky / Stomp, the
//! legendary roll, spectacle + signature status). No per-construct authoring.
//!
//! `score = cost_w·log2(cost / cost_ref) + size_w·log2(scale / size_ref) + mechanic bumps + category
//! bias`, then thresholds; overrides lift it (kaiju / colossal / legendary / spectacle ⇒ ≥ Titan,
//! a signature ⇒ Signature). CONTRAST is the point: most casts are Normal; Titan / Signature are rare.

use bd_core::mechanics::{Form, FormCategory, Mechanic, Recipe};
use serde::{Deserialize, Serialize};

/// Presence tiers, weakest → strongest (the index into `PresenceTuning::tiers`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    /// Guns, gauntlets, discs, bugs: a crisp flick.
    Tiny,
    /// Swords, shields, walls, balls: a clean gather + "shing".
    #[default]
    Normal,
    /// Dragons, anvils from the sky, big hammers, vehicles: the world notices.
    Big,
    /// Kaiju, giant mechs, meteors, legendaries: near-Hollow-Purple treatment.
    Titan,
    /// Signature casts (domains, erasing orbs): the top of the scale.
    Signature,
}

impl Tier {
    #[allow(dead_code)]
    pub const ALL: [Tier; 5] = [Tier::Tiny, Tier::Normal, Tier::Big, Tier::Titan, Tier::Signature];

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Everything the tier looks at (built from a recipe + the spawn event).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TierInput {
    pub form: Form,
    pub scale: f32,
    pub count: u8,
    /// The cast's willpower cost (the spawn event's `cost`).
    pub cost: f32,
    pub heavy: bool,
    pub explodes: bool,
    pub drop_from_sky: bool,
    pub stomp: bool,
    /// A held weapon that shoots (its gravitas lives in every shot: muzzle, tracer, impact).
    pub ranged_held: bool,
    /// Golden legendary roll (`cflags::LEGENDARY`).
    pub legendary: bool,
    /// Announced to everyone (`bd_sim::signature::spectacle`).
    pub spectacle: bool,
    /// A signature cast (domain / erasing orb / a spectacle over the signature cost).
    pub signature: bool,
}

impl TierInput {
    pub fn of(recipe: &Recipe, cost: f32, legendary: bool, spectacle: bool, signature: bool) -> Self {
        TierInput {
            form: recipe.form,
            scale: recipe.scale,
            count: recipe.count,
            cost,
            heavy: recipe.has(Mechanic::Heavy),
            explodes: recipe.has(Mechanic::Explodes),
            drop_from_sky: recipe.has(Mechanic::DropFromSky),
            stomp: recipe.has(Mechanic::Stomp),
            ranged_held: recipe.form.category() == FormCategory::Held && (recipe.has(Mechanic::Shoots) || recipe.has(Mechanic::Beam)),
            legendary,
            spectacle,
            signature,
        }
    }
}

/// Tier thresholds + weights (`presence.ron` → `tier`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TierKnobs {
    /// Cost of an ordinary held cast (a sword ≈ 14): log2(cost / cost_ref) · cost_weight.
    pub cost_ref: f32,
    pub cost_weight: f32,
    /// Size of an ordinary construct (m): log2(scale / size_ref) · size_weight.
    pub size_ref: f32,
    pub size_weight: f32,
    pub heavy: f32,
    pub explodes: f32,
    pub drop_from_sky: f32,
    /// Extra when Heavy AND DropFromSky (anvils, meteors: the sky itself is the threat).
    pub heavy_drop: f32,
    pub stomp: f32,
    /// Held guns / beams: the summon is a flick, the shots carry the weight.
    pub ranged_held: f32,
    /// Category bias: structures are big but static; worn items stay out of the way.
    pub structure: f32,
    pub worn: f32,
    pub held: f32,
    /// score < tiny_below ⇒ Tiny; ≥ big_at ⇒ Big; ≥ titan_at ⇒ Titan.
    pub tiny_below: f32,
    pub big_at: f32,
    pub titan_at: f32,
    /// Held / worn items never go above this tier unless legendary / signature (a huge hammer is
    /// Big, the world-shaking treatment is for things that stand on their own).
    pub held_cap: Tier,
    /// A spectacle (announced to everyone) costing at least this is a Signature (domains and orbital
    /// strikes always are).
    pub signature_cost: f32,
}

impl Default for TierKnobs {
    fn default() -> Self {
        TierKnobs {
            cost_ref: 14.0,
            cost_weight: 0.4,
            size_ref: 1.5,
            size_weight: 0.6,
            heavy: 0.6,
            explodes: 0.6,
            drop_from_sky: 1.0,
            heavy_drop: 1.5,
            stomp: 0.4,
            ranged_held: -0.3,
            structure: -0.4,
            worn: -1.0,
            held: 0.0,
            tiny_below: -0.4,
            big_at: 1.0,
            titan_at: 2.8,
            held_cap: Tier::Big,
            signature_cost: 80.0,
        }
    }
}

fn l2(x: f32, r: f32) -> f32 {
    if !(x.is_finite() && x > 0.0 && r > 0.0) {
        return 0.0;
    }
    (x / r).max(1e-3).log2()
}

/// The continuous presence score (before thresholds / overrides).
pub fn score(i: &TierInput, k: &TierKnobs) -> f32 {
    let size = i.scale * (i.count.max(1) as f32).cbrt();
    let mut s = k.cost_weight * l2(i.cost.max(1.0), k.cost_ref) + k.size_weight * l2(size.max(0.1), k.size_ref);
    if i.heavy {
        s += k.heavy;
    }
    if i.explodes {
        s += k.explodes;
    }
    if i.drop_from_sky {
        s += k.drop_from_sky;
        if i.heavy {
            s += k.heavy_drop;
        }
    }
    if i.stomp {
        s += k.stomp;
    }
    if i.ranged_held {
        s += k.ranged_held;
    }
    s += match i.form.category() {
        FormCategory::Structure => k.structure,
        FormCategory::Worn => k.worn,
        FormCategory::Held => k.held,
        _ => 0.0,
    };
    if s.is_finite() { s } else { 0.0 }
}

/// THE tier function.
pub fn tier(i: &TierInput, k: &TierKnobs) -> Tier {
    if i.signature {
        return Tier::Signature;
    }
    let s = score(i, k);
    let mut t = if s < k.tiny_below {
        Tier::Tiny
    } else if s < k.big_at {
        Tier::Normal
    } else if s < k.titan_at {
        Tier::Big
    } else {
        Tier::Titan
    };
    if matches!(i.form.category(), FormCategory::Held | FormCategory::Worn) {
        t = t.min(k.held_cap.max(Tier::Tiny));
    }
    let colossal = i.scale >= crate::rules::COLOSSAL_SCALE;
    if i.form == Form::Kaiju || colossal || i.spectacle {
        t = t.max(Tier::Titan);
    }
    if i.legendary {
        // Golden: the rare roll gets the near-Hollow-Purple treatment, whatever it is.
        t = t.max(Tier::Titan);
    }
    t
}

/// The tier a CAST reads as — ONE function for the client's presence (what players see and hear)
/// and the sim's SPARKS price (`willpower::cast_sparks`), so the price is learnable by feel.
/// `weight` = the cast's presence weight (`GameEvent::ConstructSpawned::cost`: its plain willpower
/// cost without balance knobs / autonomy / staleness, times the signature / orbital multipliers).
/// `legendary` = the hidden golden roll (client only: never priced); a famous held icon counts as
/// legendary. `signature` = cast as a signature (`cflags::SIGNATURE`); orbital strikes always are.
/// The DANGER alert (`signature::spectacle`) is a separate announcement and does not lift the tier:
/// a fire dragon is Big, a meteor Titan, a gun Tiny (the proof set below).
pub fn cast_tier(recipe: &Recipe, weight: f32, legendary: bool, signature: bool, k: &TierKnobs) -> Tier {
    if signature || crate::signature::is_orbital(recipe) {
        return Tier::Signature;
    }
    let famous = crate::constructs::legend::famous(recipe);
    tier(&TierInput::of(recipe, weight, legendary || famous, false, false), k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::mechanics::Element;

    /// A recipe as the sim would keep it (clamped by the default match rules), like the lexicon
    /// resolves the proof phrases (`cargo run -p bd_lexicon --example probe`).
    fn rec(form: Form, mechs: &[Mechanic], scale: f32) -> Recipe {
        let mut r = Recipe::unknown("x");
        r.form = form;
        r.mechanics = mechs.to_vec();
        r.scale = scale;
        r.element = Element::Light;
        crate::willpower::validate_recipe(r.normalized(), &crate::MatchRules::default())
    }

    fn input(r: &Recipe) -> TierInput {
        TierInput::of(r, crate::willpower::base_cost(r), false, false, false)
    }

    fn t(r: &Recipe) -> Tier {
        tier(&input(r), &TierKnobs::default())
    }

    #[test]
    fn the_proof_set_spreads_across_tiers() {
        let k = TierKnobs::default();
        use Mechanic::*;
        let cases = [
            ("gun", rec(Form::Gun, &[Shoots], 0.8), Tier::Tiny),
            ("sword", rec(Form::Sword, &[Blade], 1.1), Tier::Normal),
            ("ice wall", rec(Form::Wall, &[Barrier], 3.0), Tier::Normal),
            ("lightning spear", rec(Form::Spear, &[Pierce], 2.2), Tier::Normal),
            ("fire dragon", rec(Form::Quadruped, &[ArticulatedTail, Flies, Breath], 8.0), Tier::Big),
            ("anvil from the sky", rec(Form::Boulder, &[Ricochet, Flies, DropFromSky, Heavy], 1.5), Tier::Titan),
            ("meteor", rec(Form::Boulder, &[DropFromSky, Heavy], 1.5), Tier::Titan),
            ("giant mech", rec(Form::Kaiju, &[Shoots, Stomp, Heavy], 20.0), Tier::Titan),
            ("bomb", rec(Form::Bomb, &[Explodes], 0.7), Tier::Normal),
        ];
        for (name, r, want) in cases {
            assert_eq!(t(&r), want, "{name}: score {:.2}", score(&input(&r), &k));
        }
    }

    #[test]
    fn overrides_lift_and_cap() {
        let k = TierKnobs::default();
        let sword = rec(Form::Sword, &[Mechanic::Blade], 1.1);
        let cost = crate::willpower::base_cost(&sword);
        assert_eq!(tier(&TierInput::of(&sword, cost, true, false, false), &k), Tier::Titan, "legendary");
        assert_eq!(tier(&TierInput::of(&sword, cost, false, false, true), &k), Tier::Signature);
        // A huge expensive hammer stays Big (held cap) unless legendary.
        let hammer = rec(Form::Hammer, &[Mechanic::Blunt, Mechanic::Heavy, Mechanic::Returning], 3.0);
        assert_eq!(tier(&TierInput::of(&hammer, 200.0, false, false, false), &k), Tier::Big);
        // Spectacles are at least Titan.
        assert_eq!(tier(&TierInput::of(&sword, cost, false, true, false), &k), Tier::Titan);
    }

    #[test]
    fn garbage_in_is_bounded() {
        let k = TierKnobs::default();
        let mut i = input(&rec(Form::Ball, &[], 0.8));
        i.cost = f32::NAN;
        i.scale = f32::INFINITY;
        assert!(score(&i, &k).is_finite());
        i.scale = f32::NAN;
        assert!(tier(&i, &k) <= Tier::Normal);
    }
}
