//! **Grafting** (COMBAT.md §0.2), **animated terrain** (§3c) and **command phrases** — construct
//! engineer.
//!
//! * [`merge`]: casting while the crosshair is on one of your own live constructs grafts the new
//!   phrase onto it: mechanics and features are unioned (so `wings` on a thrown boulder makes it
//!   fly — the Flies component simply starts running; `legs` on your wall makes it a Walker; `fire`
//!   makes it burn), a non-Light element replaces the old one, a payload is added/replaced, a
//!   temperament taken if set. The world handles cost, Shrink/Grow resizing and fields.
//! * [`step_terrain`]: the controller of a terrain piece lifted by `world::animate_terrain`
//!   (`c.model` is the extracted voxels, original colours). A tree with legs walks at your enemy
//!   (Walker behaviour), a boulder with wings flies (Flies component), a pillar with rockets
//!   launches (Thrust lifts it), anything else falls and rolls as a physical body. When it
//!   shatters, clients burst it into terrain-coloured debris (`ConstructShattered` + the model).
//! * Commands (COMBAT.md §5d-bis): "roll out" transforms every Transforms construct you own;
//!   "transform" toggles (grafted onto a transformer) or adds the mechanic; "combine" /
//!   "devastator" / "form devastator" merges ≥ 3 of your transformers into one giant. A fresh
//!   cast whose name is a command acts and dissolves ([`run_commands`]); a graft whose name is a
//!   command acts on the owner's constructs ([`graft_command`]). "board" / "get in" / "ride it"
//!   ([`Command::Board`]) seats the caster in their own construct — the world handles it
//!   (`World::board_command`) before grafting or spawning.

use bd_core::mechanics::{Element, Mechanic, Recipe, Temperament};
use bd_core::{PlayerId, Vec3};

use super::components::{self, eff_has};
use super::{Construct, ConstructOut, Driver, Env, Loco};
use crate::events::GoneReason;

/// Merge a grafted phrase into `c` (see module docs).
pub fn merge(c: &mut Construct, add: &Recipe) {
    let r = &mut c.recipe;
    r.mechanics.extend_from_slice(&add.mechanics);
    r.mechanics.sort();
    r.mechanics.dedup();
    r.mechanics.truncate(crate::willpower::MAX_MECHANICS);
    r.features.extend_from_slice(&add.features);
    r.features.sort();
    r.features.dedup();
    r.features.truncate(crate::willpower::MAX_FEATURES);
    if add.element != Element::Light {
        r.element = add.element;
    }
    if add.payload.is_some() {
        let mut p = add.payload.clone();
        if let Some(p) = p.as_deref_mut() {
            p.payload = None; // depth 1
        }
        r.payload = p;
    }
    if add.temperament != Temperament::Calm {
        r.temperament = add.temperament;
        r.intensity = add.intensity;
    }
}

/// A command phrase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    /// Transform every owned Transforms construct.
    RollOut,
    /// Toggle / add Transforms.
    Transform,
    /// Merge ≥ 3 owned transformers into one giant.
    Combine,
    /// "get in", "hop in", "ride it", "take the wheel" (bd_lexicon `GraftAction::Board`, recipe named
    /// "board"): the caster takes the wheel of their aimed (else nearest) boardable construct. The
    /// world handles it before any graft / spawn (`World::board_command`); never builds anything.
    Board,
}

/// Is this recipe a command phrase?
pub fn command_of(r: &Recipe) -> Option<Command> {
    let n = r.name.trim();
    let eq = |s: &str| n.eq_ignore_ascii_case(s);
    if eq("roll out") || eq("rollout") || eq("autobots roll out") || eq("transform and roll out") {
        Some(Command::RollOut)
    } else if eq("combine") || eq("devastator") || eq("form devastator") || eq("combiners combine") || eq("merge") {
        Some(Command::Combine)
    } else if eq("transform") || eq("transforms") {
        Some(Command::Transform)
    } else if eq("board") || eq("get in") || eq("hop in") || eq("take the wheel") || eq("mount up") {
        Some(Command::Board)
    } else {
        None
    }
}

/// Toggle every Transforms construct of `owner`. Returns how many started transforming.
pub fn roll_out(items: &mut [Construct], owner: PlayerId) -> usize {
    let mut n = 0;
    for c in items.iter_mut() {
        if c.owner == owner && c.gone.is_none() && c.active() && components::request_transform(c) {
            n += 1;
        }
    }
    n
}

/// Fresh casts whose name is a command act on the owner's constructs and dissolve. Runs at the
/// start of each update (the cast arrives between ticks).
pub fn run_commands(items: &mut [Construct]) {
    for i in 0..items.len() {
        if items[i].comp.cmd_checked || items[i].gone.is_some() {
            continue;
        }
        items[i].comp.cmd_checked = true;
        let Some(cmd) = command_of(&items[i].recipe) else { continue };
        let owner = items[i].owner;
        let me = items[i].id;
        let others_transform = items.iter().any(|c| c.id != me && c.owner == owner && c.gone.is_none() && eff_has(c, Mechanic::Transforms));
        match cmd {
            Command::RollOut => {
                items[i].gone = Some(GoneReason::Spent);
                roll_out(items, owner);
            }
            Command::Combine => {
                items[i].gone = Some(GoneReason::Spent);
                components::start_combine(items, owner);
            }
            Command::Transform if others_transform => {
                items[i].gone = Some(GoneReason::Spent);
                roll_out(items, owner);
            }
            Command::Transform => {}
            // The world boards before spawning; a stray copy (a party echo…) just dissolves.
            Command::Board => items[i].gone = Some(GoneReason::Spent),
        }
    }
}

/// A graft whose phrase is a command ("roll out" / "combine" aimed at one of your constructs).
pub fn graft_command(items: &mut [Construct], owner: PlayerId, add: &Recipe) {
    match command_of(add) {
        Some(Command::RollOut) => {
            roll_out(items, owner);
        }
        Some(Command::Combine) => {
            components::start_combine(items, owner);
        }
        _ => {}
    }
}

/// [`super::StepFn`] for [`super::Controller::Terrain`] (§3c).
pub fn step_terrain(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    if !c.active() {
        // Lifting out of the island (forming): hang where it stood.
        c.body.loco = Loco::Kinematic;
        c.vel = Vec3::ZERO;
        return;
    }
    if !c.comp.launched {
        c.comp.launched = true;
    }
    if eff_has(c, Mechanic::Walks) {
        components::step_walker(c, env, out);
        return;
    }
    if eff_has(c, Mechanic::Transforms) {
        return;
    }
    // No legs: a physical lump of island (wings / rockets / homing are components on top).
    if c.body.free(Driver::Steer) && matches!(c.body.loco, Loco::Kinematic | Loco::Static | Loco::Walk) && c.comp.attached_to.is_none() && c.stuck_to.is_none() {
        c.body.loco = Loco::Ballistic;
    }
}
