//! Character costumes (DESIGN.md "Character costumes"): any phrase dresses a **fixed humanoid rig**.
//!
//! * Units: costume specs are authored in [`COSTUME_UNIT`] = 1/16 m per unit (not construct units) so
//!   a 1.8 m player has readable faces/ears/snouts. Voxelise with [`costume_model`].
//! * Rig (origin = feet centre, +Y up, facing −Z, ~1.78 m tall, fits the 0.8×1.8×0.8 m hull):
//!   root = torso/pelvis (part 0), [`HEAD`] (pivot at the neck), [`ARM_L`]/[`ARM_R`] (pivot at the
//!   shoulders, hanging), [`LEG_L`]/[`LEG_R`] (pivot at the hips). Roles are the standard
//!   `Head / ArmLeft / ArmRight / LegBackLeft / LegBackRight`, so the whole `anim` set works: `Idle`,
//!   `Locomotion` (run/air), `Slide`, `TetherFire`, `Swing`, `Cast`, `HitReact`, `Tumble`, `Aim` (look).
//! * Dressing: creatures donate their head (+ tail stub / ears / fins / horns), people get headgear
//!   (cosmetic name hints: knight, samurai, wizard…), objects become a mini helmet (held items are
//!   worn on the back), icons with a curated `ShapeSpec` donate their `Head` part (or a mini copy),
//!   mechanics/features become accessories ([`acc`] slots), element adds a small tell.
//! * Tints stay palette slots: Primary = corps colour, Accent = recipe/tweak accent (see
//!   [`costume_accent`]), Dark, Core.
//! * Guardrail: the rest-pose bounding volume stays ≤ 1.2 × the hull volume — oversized accessories
//!   are shrunk toward their anchor, then dropped (largest offender first). Cosmetic only.
//!
//! Name hints are purely cosmetic (they never change mechanics); the lexicon remains the source of
//! meaning. They exist because "wizard"/"pirate"/"cowboy" all resolve to a plain `Biped`.

mod derive;
mod gear;
mod heads;
pub mod look;
pub mod paint;
mod style;

pub use derive::is_character_icon;
pub use paint::{Costume, Paint, PaintedModel, Rgb, Tier, costume_model_painted, mesh_colors};

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Recipe};
use bd_core::shape::{PartId, PartRole, ShapeSpec};
use glam::{Quat, Vec3};

use crate::build::{A, B, C, D, P, v3};
use crate::rng::{Rng, hash_str};
use crate::voxelize::VoxelModel;

/// Metres per costume-spec unit (and costume voxel edge).
pub const COSTUME_UNIT: f32 = 0.0625;
/// Voxel cap for a costume model.
pub const MAX_COSTUME_VOXELS: usize = 8_000;
/// Hull the costume must stay near (m): 0.8 × 1.8 × 0.8.
pub const HULL_M: Vec3 = Vec3::new(0.8, 1.8, 0.8);

pub const HEAD: PartId = 1;
pub const ARM_L: PartId = 2;
pub const ARM_R: PartId = 3;
pub const LEG_L: PartId = 4;
pub const LEG_R: PartId = 5;

/// Accessory slots for [`CostumeTweaks::hide_accessories`].
pub mod acc {
    /// Hats, helmets, hair ornaments, antennae, horns.
    pub const HEAD: u32 = 1;
    /// Beards, masks, eyepatches, goggles, scarves, bells.
    pub const FACE: u32 = 1 << 1;
    /// Capes, wings, backpacks, jetpacks, sheathed weapons, flags.
    pub const BACK: u32 = 1 << 2;
    /// Pauldrons, shoulder gear, cannons, dishes.
    pub const SHOULDERS: u32 = 1 << 3;
    /// Shields / saws on the left arm.
    pub const LEFT_ARM: u32 = 1 << 4;
    /// Holsters, lassos, chains, bomb belts.
    pub const HIP: u32 = 1 << 5;
    /// Tail stubs and back fins.
    pub const TAIL: u32 = 1 << 6;
    /// Boots, skates, tread boots.
    pub const FEET: u32 = 1 << 7;
    /// Gauntlets, claws, drills, arm pipes.
    pub const ARMS: u32 = 1 << 8;
    /// Element tells (flame crest, lightning, ice shards).
    pub const AURA: u32 = 1 << 9;
    pub const ALL: u32 = (1 << 10) - 1;
}

/// Lobby tweaks (sent once at join with the phrase + seed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CostumeTweaks {
    /// Accent colour override (sRGB 0..1); `None` = recipe accent / lighter corps colour.
    pub accent: Option<[f32; 3]>,
    /// Head (and headgear) scale, clamped 0.7..1.35.
    pub head_scale: f32,
    /// Bitmask of [`acc`] slots to hide.
    pub hide_accessories: u32,
}

impl Default for CostumeTweaks {
    fn default() -> Self {
        CostumeTweaks { accent: None, head_scale: 1.0, hide_accessories: 0 }
    }
}

/// A head donor ("knight with a **fox** head"): its recipe and optional curated icon shape.
#[derive(Clone, Copy, Debug)]
pub struct CostumeHead<'a> {
    pub recipe: &'a Recipe,
    pub shape: Option<&'a ShapeSpec>,
}

/// Costume for a phrase's recipe. Pure and deterministic in (recipe, seed, tweaks).
pub fn costume(recipe: &Recipe, seed: u32, tweaks: CostumeTweaks) -> ShapeSpec {
    costume_with(recipe, None, None, seed, tweaks)
}

/// Accent colour to render the costume with: the tweak, else the recipe's accent.
pub fn costume_accent(recipe: &Recipe, tweaks: &CostumeTweaks) -> Option<[f32; 3]> {
    let ok = |a: [f32; 3]| a.iter().all(|c| c.is_finite()).then(|| a.map(|c| c.clamp(0.0, 1.0)));
    tweaks.accent.and_then(ok).or(recipe.accent.and_then(ok))
}

/// Voxelise a costume spec at [`COSTUME_UNIT`] (capped), with the rig derived from its roles.
pub fn costume_model(spec: &ShapeSpec) -> VoxelModel {
    let mut m = crate::voxelize::voxelize_with_unit(spec, COSTUME_UNIT);
    if m.voxel_count > MAX_COSTUME_VOXELS {
        let mut s = spec.clone();
        let f = (MAX_COSTUME_VOXELS as f32 / m.voxel_count as f32).sqrt() * 0.95;
        crate::voxelize::scale_spec(&mut s, f.clamp(0.3, 1.0));
        m = crate::voxelize::voxelize_with_unit(&s, COSTUME_UNIT);
        m.unit_m = COSTUME_UNIT * f.clamp(0.3, 1.0);
    }
    m
}

// ------------------------------------------------------------------------------------ the rig

/// Rig landmarks (units).
pub(crate) mod rig {
    use glam::Vec3;
    pub const HIP_Y: f32 = 10.0;
    pub const NECK_Y: f32 = 19.0;
    pub const SHOULDER: Vec3 = Vec3::new(5.5, 18.0, 0.0);
    pub const ELBOW_Y: f32 = 14.5;
    pub const HAND_Y: f32 = 10.3;
    pub const HIP_X: f32 = 2.2;
    /// Skull centre / full size.
    pub const HC: Vec3 = Vec3::new(0.0, 23.5, -0.5);
    pub const HS: Vec3 = Vec3::new(9.0, 9.0, 8.5);
}

/// Look chosen for the head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Beast {
    Canine,
    Bovine,
    Equine,
    Feline,
    Ursine,
    Rodent,
    Bird,
    Shark,
    Dragon,
    Dino,
    Insect,
    Serpent,
    Octopus,
    Spider,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Headgear {
    Knight,
    Samurai,
    Viking,
    Wizard,
    Pirate,
    Cowboy,
    Astronaut,
    Ninja,
    Robot,
    Skeleton,
    Spiky,
    Chef,
    Crown,
    Cap,
    Tv,
    Toaster,
    Mushroom,
    Cactus,
    Pumpkin,
    Goggles,
}

/// Cosmetic hints from the display name (see module docs).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Hints {
    pub beast: Option<Beast>,
    pub gear: Option<Headgear>,
    pub gear2: Option<Headgear>,
    /// Donkey-style long ears / fox cheeks / unicorn horn / pikachu cheeks.
    pub long_ears: bool,
    pub fox: bool,
    pub horn: bool,
    pub cheeks: bool,
}

pub(crate) fn hints(name: &str) -> Hints {
    let lower = name.to_lowercase();
    let toks: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).collect();
    let has = |w: &[&str]| toks.iter().any(|t| w.iter().any(|x| t == x || (x.len() > 4 && t.starts_with(x))));
    let mut h = Hints::default();
    use Beast::*;
    use Headgear::*;
    let beasts: [(&[&str], Beast); 14] = [
        (&["donkey", "mule", "eeyore", "horse", "pony", "zebra", "unicorn", "burro", "stallion"], Equine),
        (&["fox", "wolf", "dog", "puppy", "coyote", "jackal", "hound", "werewolf", "husky"], Canine),
        (&["cat", "lion", "tiger", "leopard", "panther", "cheetah", "kitten", "lynx"], Feline),
        (&["bear", "panda", "koala", "grizzly"], Ursine),
        (&["cow", "bull", "ox", "bison", "buffalo", "yak", "goat", "sheep", "ram", "minotaur"], Bovine),
        (&["pikachu", "mouse", "rat", "rabbit", "bunny", "hamster", "squirrel", "raichu", "pichu"], Rodent),
        (&["bird", "eagle", "owl", "parrot", "chicken", "duck", "penguin", "hawk", "crow", "raven"], Bird),
        (&["shark", "fish", "piranha", "dolphin", "whale", "orca"], Shark),
        (&["dragon", "wyvern", "drake"], Dragon),
        (&["dinosaur", "rex", "raptor", "godzilla", "lizard", "crocodile", "alligator", "kaiju"], Dino),
        (&["bee", "wasp", "ant", "beetle", "fly", "mantis", "hornet"], Insect),
        (&["snake", "serpent", "cobra", "python", "viper"], Serpent),
        (&["octopus", "squid", "kraken", "cthulhu"], Octopus),
        (&["spider", "tarantula", "scorpion"], Spider),
    ];
    for (w, b) in beasts {
        if has(w) {
            h.beast = Some(b);
            break;
        }
    }
    let gears: [(&[&str], Headgear); 20] = [
        (&["knight", "paladin", "crusader", "templar"], Knight),
        (&["samurai", "shogun", "ronin"], Samurai),
        (&["viking", "barbarian", "norse", "valkyrie"], Viking),
        (&["wizard", "witch", "mage", "sorcerer", "warlock", "merlin", "gandalf"], Wizard),
        (&["pirate", "buccaneer", "captain", "corsair"], Pirate),
        (&["cowboy", "cowgirl", "sheriff", "ranger", "outlaw", "rancher"], Cowboy),
        (&["astronaut", "cosmonaut", "spaceman", "spacesuit"], Astronaut),
        (&["ninja", "assassin", "shinobi", "thief"], Ninja),
        (&["robot", "android", "cyborg", "mech", "droid", "automaton"], Robot),
        (&["skeleton", "skull", "undead", "lich", "zombie"], Skeleton),
        (&["goku", "saiyan", "vegeta", "anime", "punk"], Spiky),
        (&["chef", "cook", "baker"], Chef),
        (&["king", "queen", "prince", "princess", "emperor", "royal"], Crown),
        (&["police", "cop", "soldier", "pilot", "officer", "mailman"], Cap),
        (&["tv", "television", "monitor", "computer", "screen"], Tv),
        (&["toaster"], Toaster),
        (&["mushroom", "toadstool", "fungus"], Mushroom),
        (&["cactus", "cacti"], Cactus),
        (&["pumpkin", "jack", "halloween"], Pumpkin),
        (&["steampunk", "aviator", "engineer", "inventor", "scientist", "mechanic", "tinker"], Goggles),
    ];
    for (w, g) in gears {
        if has(w) {
            if h.gear.is_none() {
                h.gear = Some(g);
            } else if h.gear2.is_none() && h.gear != Some(g) {
                h.gear2 = Some(g);
            }
        }
    }
    h.long_ears = has(&["donkey", "mule", "eeyore", "burro", "rabbit", "bunny", "pikachu", "raichu"]);
    h.fox = has(&["fox", "vulpix", "kitsune"]);
    h.horn = has(&["unicorn", "rhino", "narwhal"]);
    h.cheeks = has(&["pikachu", "raichu", "pichu"]);
    h
}

/// Creature archetype for a form when no hint applies (mirrors the creature generator's first
/// random draw so the costume matches the construct cast from the same phrase).
fn beast_of(recipe: &Recipe) -> Option<Beast> {
    let seed = crate::forms::seed_of(&recipe.model);
    let mut r = Rng::new(seed, recipe.form as u32 + 1);
    Some(match recipe.form {
        Form::Quadruped => match r.pick(5) {
            0 => Beast::Canine,
            1 => Beast::Bovine,
            2 => Beast::Equine,
            3 => Beast::Feline,
            _ => Beast::Ursine,
        },
        Form::Kaiju => Beast::Dino,
        Form::Serpent => Beast::Serpent,
        Form::Bird => Beast::Bird,
        Form::Fish => Beast::Shark,
        Form::Arachnid => Beast::Spider,
        Form::Insect => Beast::Insect,
        Form::Blob => Beast::Octopus,
        _ => return None,
    })
}

/// Accessory group (for the guardrail fit and hide mask).
pub(crate) struct Group {
    #[allow(dead_code)]
    pub slot: u32,
    /// 0 = props (dropped first), 1 = accessories, 2 = signature headgear (shrunk, never dropped).
    pub prio: u8,
    pub prims: std::ops::Range<usize>,
    pub parts: Vec<PartId>,
    pub anchor: Vec3,
}

/// Costume builder: the spec builder + accessory bookkeeping.
pub(crate) struct Cx {
    pub b: B,
    pub groups: Vec<Group>,
    pub hide: u32,
    pub r: Rng,
    pub used: u32,
    /// Paint per prim (character / derived looks; shorter = unpainted).
    pub paint: Vec<Paint>,
    /// Distinct colours seen (tint slot allocation, see `paint::Pn`).
    pub slots: Vec<Rgb>,
    /// Guardrail: bounding volume limit as a multiple of the hull volume.
    pub limit_k: f32,
}

impl Cx {
    /// Build an accessory group in `slot` (skipped when hidden). The closure must leave `b` on
    /// any part (it is reset to the root afterwards).
    pub fn group(&mut self, slot: u32, anchor: Vec3, f: impl FnOnce(&mut B, &mut Rng)) {
        if self.hide & slot != 0 {
            return;
        }
        let p0 = self.b.spec.prims.len();
        let q0 = self.b.spec.parts.len();
        f(&mut self.b, &mut self.r);
        self.b.on(0);
        let parts = self.b.spec.parts[q0..].iter().map(|p| p.id).collect();
        let prio = if slot & (acc::HEAD | acc::FACE) != 0 {
            2
        } else if slot & (acc::BACK | acc::HIP | acc::AURA) != 0 {
            0
        } else {
            1
        };
        self.groups.push(Group { slot, prio, prims: p0..self.b.spec.prims.len(), parts, anchor });
        self.used |= slot;
    }

    fn new(seed: u32, hide: u32) -> Self {
        Cx { b: B::new(), groups: Vec::new(), hide, r: Rng::new(seed, 0xC057), used: 0, paint: Vec::new(), slots: Vec::new(), limit_k: GUARDRAIL_K }
    }

    /// Painting builder on the current part.
    pub fn pn(&mut self) -> paint::Pn<'_> {
        paint::Pn::new(&mut self.b, &mut self.paint, &mut self.slots)
    }

    /// Painted accessory group (see [`Cx::group`]).
    pub fn pgroup(&mut self, slot: u32, anchor: Vec3, f: impl FnOnce(&mut paint::Pn)) {
        if self.hide & slot != 0 {
            return;
        }
        let p0 = self.b.spec.prims.len();
        let q0 = self.b.spec.parts.len();
        {
            let mut p = self.pn();
            f(&mut p);
        }
        self.b.on(0);
        let parts = self.b.spec.parts[q0..].iter().map(|p| p.id).collect();
        let prio = if slot & (acc::HEAD | acc::FACE) != 0 {
            2
        } else if slot & (acc::BACK | acc::HIP | acc::AURA) != 0 {
            0
        } else {
            1
        };
        self.groups.push(Group { slot, prio, prims: p0..self.b.spec.prims.len(), parts, anchor });
        self.used |= slot;
    }

    /// Finish: head scale, guardrail, paint aligned with the prims.
    fn finish(mut self, tweaks: &CostumeTweaks) -> Costume {
        let mut spec = std::mem::take(&mut self.b.spec);
        let mut paint = std::mem::take(&mut self.paint);
        paint.resize(spec.prims.len(), Paint::default());
        derive::resolve_sentinels(&mut spec, &mut paint);
        scale_head(&mut spec, tweaks.head_scale);
        snap_decals(&mut spec, &paint);
        fit_guardrail(&mut spec, &self.groups, &mut paint, self.limit_k);
        Costume { spec, paint, ..Default::default() }
    }
}

/// Keep one-voxel decals on whole voxels after a head rescale (faces stay crisp).
fn snap_decals(spec: &mut ShapeSpec, paint: &[Paint]) {
    for (p, pt) in spec.prims.iter_mut().zip(paint) {
        if pt.decal && p.rot_deg == Vec3::ZERO {
            let small = |s: f32| s <= 1.6;
            let c = p.center;
            p.center = Vec3::new(if small(p.size.x) { c.x.round() } else { c.x }, if small(p.size.y) { c.y.round() } else { c.y }, if small(p.size.z) { c.z.round() } else { c.z });
            p.size = Vec3::new(if small(p.size.x) { 1.0 } else { p.size.x }, if small(p.size.y) { 1.0 } else { p.size.y }, if small(p.size.z) { 1.0 } else { p.size.z });
        }
    }
}

/// Full resolution for a costume phrase (DESIGN.md "Character costumes"):
/// 1. the phrase names a hand-authored **character** (assets/costumes/characters.ron: aliases,
///    optionally + variant words) -> that look; `seed` cycles its variants;
/// 2. the phrase resolved to a curated **humanoid icon** -> a look derived from the icon;
/// 3. otherwise the **vocabulary** costume ([`costume_with`]).
///
/// `recipe` / `icon` are the lexicon's resolution of the phrase (tier 1 never needs them).
pub fn dress(phrase: &str, recipe: &Recipe, icon: Option<&ShapeSpec>, head: Option<CostumeHead>, seed: u32, tweaks: CostumeTweaks) -> Costume {
    if head.is_none()
        && let Some(f) = look::find(phrase)
    {
        return character_costume(f.index, f.base_variant + seed as usize, tweaks);
    }
    if head.is_none()
        && let Some(ic) = icon
        && derive::is_character_icon(recipe, ic)
    {
        return derived_costume(recipe, ic, tweaks);
    }
    let mut c = vocab(recipe, icon, head, seed, tweaks);
    c.tier = Tier::Vocabulary;
    c
}

/// Character `index` of the compiled-in table, variant `variant` (wraps).
pub fn character_costume(index: usize, variant: usize, tweaks: CostumeTweaks) -> Costume {
    let t = look::table();
    let Some(ch) = t.chars.get(index) else { return Costume::default() };
    let n = ch.variant_count();
    let v = variant % n;
    let lk = ch.look_of(v);
    let mut cx = Cx::new(hash_str(&ch.names[0]), tweaks.hide_accessories);
    cx.limit_k = CHARACTER_GUARDRAIL_K;
    style::dress(&mut cx, &lk, costume_accent_tweak(&tweaks), true);
    let mut c = cx.finish(&tweaks);
    c.character = ch.names.first().cloned();
    c.variant = Some((v, n));
    c.tier = Tier::Character;
    c
}

/// Tier 2 (see [`dress`]).
pub fn derived_costume(recipe: &Recipe, icon: &ShapeSpec, tweaks: CostumeTweaks) -> Costume {
    let lk = derive::derive_look(recipe, icon);
    let mut cx = Cx::new(hash_str(&recipe.name.to_lowercase()), tweaks.hide_accessories);
    cx.limit_k = CHARACTER_GUARDRAIL_K;
    style::dress(&mut cx, &lk, costume_accent_tweak(&tweaks), false);
    derive::icon_head_painted(&mut cx, recipe, icon);
    let mut c = cx.finish(&tweaks);
    c.character = Some(recipe.name.clone());
    c.tier = Tier::Derived;
    c
}

fn costume_accent_tweak(t: &CostumeTweaks) -> Option<Rgb> {
    t.accent.filter(|a| a.iter().all(|c| c.is_finite())).map(|a| a.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8))
}

/// Full costume entry point: `icon` = curated shape for an `Icon` recipe, `head` = head donor.
pub fn costume_with(recipe: &Recipe, icon: Option<&ShapeSpec>, head: Option<CostumeHead>, seed: u32, tweaks: CostumeTweaks) -> ShapeSpec {
    vocab(recipe, icon, head, seed, tweaks).spec
}

/// The vocabulary costume (tier 3) as an unpainted [`Costume`].
fn vocab(recipe: &Recipe, icon: Option<&ShapeSpec>, head: Option<CostumeHead>, seed: u32, tweaks: CostumeTweaks) -> Costume {
    let name_seed = hash_str(&recipe.name.to_lowercase());
    let mut cx = Cx::new(seed ^ name_seed, tweaks.hide_accessories);
    let h = hints(&recipe.name);
    let hh = head.map(|d| hints(&d.recipe.name));
    // What wears the head?
    let donor = head.map(|d| d.recipe).unwrap_or(recipe);
    let donor_icon = match head {
        Some(d) => d.shape,
        None => icon,
    };
    let dh = hh.unwrap_or(h);
    let icon_head = donor_icon.filter(|s| s.parts.iter().any(|p| p.role == PartRole::Head));
    let beast = if icon_head.is_some() {
        None
    } else {
        dh.beast.or_else(|| if dh.gear.is_some() { None } else { beast_of(donor) })
    };
    let robot = h.gear == Some(Headgear::Robot) || h.gear2 == Some(Headgear::Robot);
    let skeleton = h.gear == Some(Headgear::Skeleton);

    // Body (root, arms, legs) + bare head part.
    base_body(&mut cx, recipe, robot, skeleton, h.beast == Some(Beast::Insect));
    // Head.
    if let Some(spec) = icon_head {
        heads::icon_head(&mut cx, spec);
    } else if let Some(bst) = beast {
        heads::beast_head(&mut cx, bst, &dh, donor);
        heads::beast_extras(&mut cx, bst, &dh, donor);
    } else {
        let gear = if head.is_some() { dh.gear } else { h.gear };
        heads::human_head(&mut cx, gear, h.gear2.filter(|_| head.is_none()), recipe);
        if head.is_none() && gear.is_none() {
            // Objects become a mini helmet / back prop.
            match recipe.form.category() {
                FormCategory::Object | FormCategory::Structure | FormCategory::Held if recipe.form != Form::Biped => {
                    heads::mini_construct(&mut cx, recipe, icon);
                }
                _ => {}
            }
        }
    }
    // Outfit accessories from the body recipe (mechanics, features, element).
    gear::accessories(&mut cx, recipe, &h, beast.is_some());
    if let Some(d) = head {
        // Donor extras that read at costume scale (horns from Charge, element tells).
        if d.recipe.element != Element::Light && recipe.element == Element::Light {
            gear::element_tell(&mut cx, d.recipe.element);
        }
    }

    cx.finish(&tweaks)
}

/// Pelvis/torso, arms (upper + forearm + glove, ring on the right hand), legs with boots, bare head.
fn base_body(cx: &mut Cx, recipe: &Recipe, robot: bool, skeleton: bool, stripes: bool) {
    use rig::*;
    let b = &mut cx.b;
    let heavy = recipe.has(Mechanic::Heavy) || recipe.form == Form::Kaiju;
    let tw = if heavy { 9.0 } else { 8.0 };
    // Head part first so the ids match HEAD / ARM_* / LEG_*.
    let hid = b.part(0, v3(0.0, NECK_Y, 0.0), PartRole::Head);
    debug_assert_eq!(hid, HEAD);
    b.bx(v3(0.0, NECK_Y + 0.5, 0.0), v3(3.6, 1.6, 3.2), D);
    b.on(0);
    // Torso (root).
    b.bx(v3(0.0, 14.5, 0.0), v3(tw, 9.0, 5.0), P);
    b.bx(v3(0.0, 17.8, 0.0), v3(tw + 0.4, 2.4, 5.4), P);
    b.bx(v3(0.0, 10.8, 0.0), v3(tw - 0.6, 2.0, 5.2), D);
    b.bx(v3(0.0, 10.8, -2.7), v3(2.0, 1.4, 0.6), A);
    b.bx(v3(0.0, 18.8, 0.0), v3(4.0, 1.0, 3.6), A);
    // Lantern emblem on the chest.
    b.bx(v3(0.0, 15.8, -2.6), v3(3.0, 3.0, 0.6), D);
    b.bx(v3(0.0, 15.8, -2.8), v3(1.4, 1.4, 0.6), C);
    if robot {
        b.bx(v3(0.0, 13.5, -2.6), v3(5.0, 1.0, 0.5), A);
        for x in [-2.0f32, 0.0, 2.0] {
            b.bx(v3(x, 13.0, -2.7), Vec3::ONE, C);
        }
    }
    if skeleton {
        for y in [12.8f32, 14.4, 16.0] {
            b.bx(v3(0.0, y, -2.6), v3(tw - 2.0, 0.8, 0.6), C);
        }
        b.bx(v3(0.0, 14.5, -2.7), v3(1.0, 6.0, 0.6), C);
    }
    if stripes {
        for y in [12.5f32, 15.5] {
            b.bx(v3(0.0, y, 0.0), v3(tw + 0.3, 1.2, 5.3), D);
        }
    }
    // Arms.
    for (id_expect, s) in [(ARM_L, -1.0f32), (ARM_R, 1.0)] {
        let role = if s < 0.0 { PartRole::ArmLeft } else { PartRole::ArmRight };
        let sh = v3(s * SHOULDER.x, SHOULDER.y, 0.0);
        let id = b.part(0, sh, role);
        debug_assert_eq!(id, id_expect);
        let ax = s * 5.5;
        b.bx(v3(ax, 16.4, 0.0), v3(3.0, 4.2, 3.0), P);
        b.bx(v3(ax, 12.9, 0.0), v3(2.8, 3.4, 2.8), P);
        b.bx(v3(ax, 18.0, 0.0), v3(3.6, 1.6, 3.6), A);
        // Glove + cuff.
        b.bx(v3(ax, HAND_Y, -0.1), v3(3.4, 3.2, 3.4), D);
        b.bx(v3(ax, 11.9, 0.0), v3(3.6, 0.8, 3.6), A);
        if s > 0.0 {
            // Lantern ring on the ring hand.
            b.bx(v3(ax - 0.6, HAND_Y - 0.4, -1.9), v3(1.6, 1.2, 0.8), A);
            b.bx(v3(ax - 0.6, HAND_Y - 0.4, -2.3), v3(1.0, 1.0, 0.6), C);
        }
        b.on(0);
    }
    // Legs + boots.
    for (id_expect, s) in [(LEG_L, -1.0f32), (LEG_R, 1.0)] {
        let role = if s < 0.0 { PartRole::LegBackLeft } else { PartRole::LegBackRight };
        let id = b.part(0, v3(s * HIP_X, HIP_Y, 0.0), role);
        debug_assert_eq!(id, id_expect);
        let x = s * HIP_X;
        b.bx(v3(x, 6.3, 0.0), v3(3.6, 7.6, 3.8), P);
        let bw = if heavy { 4.6 } else { 4.0 };
        b.bx(v3(x, 1.6, -0.5), v3(bw, 3.2, 5.6), D);
        b.bx(v3(x, 3.4, -0.2), v3(bw + 0.2, 0.8, 5.2), A);
        b.on(0);
    }
}

/// Scale the head and everything on it about the neck.
fn scale_head(spec: &mut ShapeSpec, k: f32) {
    let k = if k.is_finite() { k.clamp(0.7, 1.35) } else { 1.0 };
    if (k - 1.0).abs() < 1e-4 {
        return;
    }
    let neck = v3(0.0, rig::NECK_Y, 0.0);
    // Head part + its descendants.
    let mut ids = vec![HEAD];
    loop {
        let before = ids.len();
        for p in &spec.parts {
            if ids.contains(&p.parent) && !ids.contains(&p.id) {
                ids.push(p.id);
            }
        }
        if ids.len() == before {
            break;
        }
    }
    for p in spec.prims.iter_mut().filter(|p| ids.contains(&p.part)) {
        p.center = neck + (p.center - neck) * k;
        p.size *= k;
    }
    for p in spec.parts.iter_mut().filter(|p| ids.contains(&p.id) && p.id != HEAD) {
        p.pivot = neck + (p.pivot - neck) * k;
    }
}

fn volume_excluding(spec: &ShapeSpec, groups: &[Group], skip: Option<usize>, dropped: &[bool]) -> f32 {
    let mut mn = Vec3::splat(f32::MAX);
    let mut mx = Vec3::splat(f32::MIN);
    for (i, p) in spec.prims.iter().enumerate() {
        if p.subtract || !p.center.is_finite() || !p.size.is_finite() {
            continue;
        }
        let gi = groups.iter().position(|g| g.prims.contains(&i));
        if let Some(g) = gi
            && (dropped[g] || Some(g) == skip)
        {
            continue;
        }
        let e = crate::voxelize::PrimEval::new(p.kind, p.center, p.size, p.rot_deg);
        mn = mn.min(e.lo.as_vec3());
        mx = mx.max(e.hi.as_vec3());
    }
    if mn.x > mx.x {
        return 0.0;
    }
    let s = mx - mn + Vec3::ONE;
    s.x * s.y * s.z
}

/// Hull volume × 1.2 in units³.
/// Default guardrail: rest-pose bounding volume <= 1.2 x the hull.
pub const GUARDRAIL_K: f32 = 1.2;
/// Character looks: signature silhouettes (Goku's spikes, a straw-hat brim, a cape) are thin, so the
/// *bounding box* grows much faster than the visual mass: they get a looser box (still cosmetic;
/// hitboxes never depend on the costume).
pub const CHARACTER_GUARDRAIL_K: f32 = 1.75;

pub fn guardrail_volume_units() -> f32 {
    let u3 = COSTUME_UNIT * COSTUME_UNIT * COSTUME_UNIT;
    HULL_M.x * HULL_M.y * HULL_M.z * 1.2 / u3
}

/// Shrink (then drop) accessory groups until the bounding volume fits the guardrail.
fn fit_guardrail(spec: &mut ShapeSpec, groups: &[Group], paint: &mut Vec<Paint>, limit_k: f32) {
    let limit = guardrail_volume_units() * limit_k / GUARDRAIL_K;
    let mut dropped = vec![false; groups.len()];
    let mut shrinks = vec![0u8; groups.len()];
    for _ in 0..64 {
        let v = volume_excluding(spec, groups, None, &dropped);
        if v <= limit {
            break;
        }
        // Offender: lowest priority first, then the group whose absence shrinks the volume most.
        let mut best: Option<(usize, u8, f32)> = None;
        for g in 0..groups.len() {
            if dropped[g] || groups[g].prims.is_empty() || (groups[g].prio == 2 && shrinks[g] >= 2) {
                continue;
            }
            let gain = v - volume_excluding(spec, groups, Some(g), &dropped);
            if gain > 0.0 && best.is_none_or(|(_, p, b)| groups[g].prio < p || (groups[g].prio == p && gain > b)) {
                best = Some((g, groups[g].prio, gain));
            }
        }
        let Some((g, _, _)) = best else { break };
        if shrinks[g] >= 3 {
            dropped[g] = true;
            continue;
        }
        shrinks[g] += 1;
        let grp = &groups[g];
        let k = 0.8;
        for p in &mut spec.prims[grp.prims.clone()] {
            p.center = grp.anchor + (p.center - grp.anchor) * k;
            p.size *= k;
        }
        for p in spec.parts.iter_mut().filter(|p| grp.parts.contains(&p.id)) {
            p.pivot = grp.anchor + (p.pivot - grp.anchor) * k;
        }
    }
    // Still too big (tall ears, long snouts): shrink the head, then the whole figure.
    let mut hk = 1.0;
    while volume_excluding(spec, groups, None, &dropped) > limit && hk > 0.75 {
        scale_head(spec, 0.93);
        hk *= 0.93;
    }
    for _ in 0..8 {
        let v = volume_excluding(spec, groups, None, &dropped);
        if v <= limit {
            break;
        }
        let k = ((limit / v).cbrt() * 0.99).min(0.98);
        crate::voxelize::scale_spec(spec, k);
    }
    if dropped.iter().any(|d| *d) {
        let mut drop_parts: Vec<PartId> = Vec::new();
        let mut keep = vec![true; spec.prims.len()];
        for (g, grp) in groups.iter().enumerate() {
            if dropped[g] {
                for i in grp.prims.clone() {
                    keep[i] = false;
                }
                drop_parts.extend(&grp.parts);
            }
        }
        let mut i = 0;
        spec.prims.retain(|_| {
            i += 1;
            keep[i - 1]
        });
        let mut i = 0;
        paint.retain(|_| {
            i += 1;
            keep.get(i - 1).copied().unwrap_or(true)
        });
        spec.parts.retain(|p| !drop_parts.contains(&p.id));
    }
}

/// First-person hands: the costume's forearms + gloves (+ ring on the right hand), as parts
/// `ArmLeft`/`ArmRight` pivoting at the elbow and pointing forward (−Z), costume units. Place each
/// part's pivot where the view model wants the elbow.
pub fn costume_first_person_hands(spec: &ShapeSpec) -> ShapeSpec {
    let mut out = ShapeSpec::default();
    let q = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for (id, role) in [(ARM_L, PartRole::ArmLeft), (ARM_R, PartRole::ArmRight)] {
        let Some(part) = spec.parts.iter().find(|p| p.id == id && p.role == role) else { continue };
        let elbow = v3(part.pivot.x, rig::ELBOW_Y, 0.0);
        let mut any = false;
        for p in spec.prims.iter().filter(|p| p.part == id && p.center.is_finite() && p.center.y < rig::ELBOW_Y + 0.5) {
            let mut np = p.clone();
            np.center = elbow + q * (p.center - elbow);
            let pq = crate::build::quat_from_rot_deg(p.rot_deg);
            np.rot_deg = crate::build::rot_deg_from_quat(q * pq);
            out.prims.push(np);
            any = true;
        }
        if any {
            out.parts.push(bd_core::shape::Part { id, parent: 0, pivot: elbow, role });
        }
    }
    out
}
