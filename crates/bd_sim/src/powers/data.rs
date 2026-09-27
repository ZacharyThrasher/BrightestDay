//! Power definitions (`assets/powers.ron`) — the ONE template of COMBAT.md §5h.
//!
//! Every power is `body` + `graft` + `incantation` + `moves` + `transform` + `companion`, and every
//! piece is closed-set data: `MoveModifiers` numbers, `Element`s, `Mechanic`s, recipes, and a closed
//! list of [`MoveOp`]s that each map onto an existing world operation (cast, impulse, blink,
//! field, portal, link, explosion, carve, status, seat). Nothing here adds a verb.

use std::sync::OnceLock;

use bd_core::mechanics::{Element, Feature, Form, Mechanic, ModelSource, Recipe, Temperament};
use serde::{Deserialize, Serialize};

use crate::movement::MoveModifiers;
use crate::status::StatusKind;

/// Grants key of the "random devil fruit" recipe (lexicon: "devil fruit", "mystery fruit").
pub const RANDOM_FRUIT: &str = "random:devil_fruit";
/// Grants key of the "random stand" recipe (lexicon: "stand arrow", "the arrow").
pub const RANDOM_STAND: &str = "random:stand";
/// Grants key of the "random mutagen" recipe (lexicon: "mutagen", "the ooze"): a random animal
/// hybrid among the `mutagen_*` powers (TMNT pack).
pub const RANDOM_MUTAGEN: &str = "random:mutagen";
/// Grants key of the "random origin" recipe: a random superhero origin kit (`kind: Origin`).
pub const RANDOM_ORIGIN: &str = "random:origin";
/// Id prefix of the mutagen powers — they are Zoan "fruits" mechanically but are never rolled by a
/// random devil fruit, only by [`RANDOM_MUTAGEN`].
pub const MUTAGEN_PREFIX: &str = "mutagen_";

/// Devil fruit class (Logia can scatter; Zoans transform), a JoJo stand, or a superhero origin
/// (radioactive spider, gamma, kryptonian, power ring…). Transforms and companions are data on any
/// kind; the kind only picks the acquisition + weakness rules (COMBAT.md §5h).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PowerKind {
    Paramecia,
    Logia,
    Zoan,
    MythicalZoan,
    Stand,
    /// A pickup like a fruit, but none of the devil-fruit rules: no Water drain, no seastone, no
    /// void sinking, and a second origin simply replaces the first.
    Origin,
    /// A trained technique / form (ultra instinct, super saiyan, sharingan, bankai, nen…): saying
    /// its name makes it yours on the spot until KO — no pickup, no companion needed.
    Technique,
}

impl PowerKind {
    /// A devil fruit (mutagens included): Water drain, seastone, void sink, a second one explodes you.
    pub fn is_fruit(self) -> bool {
        matches!(self, PowerKind::Paramecia | PowerKind::Logia | PowerKind::Zoan | PowerKind::MythicalZoan)
    }

    pub fn is_stand(self) -> bool {
        self == PowerKind::Stand
    }

    pub fn is_origin(self) -> bool {
        self == PowerKind::Origin
    }

    pub fn is_technique(self) -> bool {
        self == PowerKind::Technique
    }

    /// Acquired as a pickup anyone can grab (and dropped where the holder is KO'd): fruits and
    /// origins. Stands are called, held until KO and then gone.
    pub fn is_pickup(self) -> bool {
        self.is_fruit() || self.is_origin()
    }
}

/// How an uneaten fruit / ungrabbed origin looks (§5h): a small inert object. Only the look is data;
/// forms and elements are limited to ones that never act on their own (sanitized on load).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PickupDef {
    /// Construct name ("radioactive spider", "gamma canister"); empty = the power's name.
    pub name: String,
    pub form: Form,
    pub element: Element,
    pub scale: f32,
}

impl Default for PickupDef {
    fn default() -> Self {
        Self { name: String::new(), form: Form::Ball, element: Element::Light, scale: 0.45 }
    }
}

impl PickupDef {
    fn sanitize(&mut self) {
        if !matches!(self.form, Form::Ball | Form::Boulder | Form::Blob0) {
            self.form = Form::Ball;
        }
        // No element that burns, soaks, freezes or blows things around while lying on the ground.
        if !matches!(self.element, Element::Light | Element::Electric | Element::Earth | Element::Poison | Element::Psychic | Element::Shadow | Element::Atomic) {
            self.element = Element::Light;
        }
        self.scale = if self.scale.is_finite() { self.scale.clamp(0.25, 0.9) } else { 0.45 };
    }
}

/// Multipliers / additions onto [`MoveModifiers`] (1.0 / 0.0 = unchanged).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoveMods {
    pub speed: f32,
    pub gravity: f32,
    pub jump: f32,
    pub air_control: f32,
    pub accel: f32,
    /// Max of this and the current glide.
    pub glide: f32,
    /// Flight (m/s²) and its cap — Phoenix.
    pub thrust: f32,
    pub thrust_max: f32,
    pub slow_fall: f32,
}

impl Default for MoveMods {
    fn default() -> Self {
        Self { speed: 1.0, gravity: 1.0, jump: 1.0, air_control: 1.0, accel: 1.0, glide: 0.0, thrust: 0.0, thrust_max: 0.0, slow_fall: 0.0 }
    }
}

impl MoveMods {
    pub fn apply(&self, m: &mut MoveModifiers) {
        let f = |v: f32| if v.is_finite() { v.clamp(0.0, 4.0) } else { 1.0 };
        m.speed_mult *= f(self.speed);
        m.gravity_scale *= f(self.gravity);
        m.jump_mult *= f(self.jump);
        m.air_control *= f(self.air_control);
        m.accel_mult *= f(self.accel);
        if self.glide.is_finite() {
            m.glide = m.glide.max(self.glide.clamp(0.0, 1.0));
        }
        if self.thrust.is_finite() && self.thrust > m.thrust {
            m.thrust = self.thrust.min(60.0);
            if self.thrust_max.is_finite() && self.thrust_max > 0.0 {
                m.thrust_max_speed = self.thrust_max.min(30.0);
            }
        }
        if self.slow_fall.is_finite() && self.slow_fall > 0.0 {
            m.slow_fall = if m.slow_fall > 0.0 { m.slow_fall.min(self.slow_fall) } else { self.slow_fall };
        }
    }
}

/// Passive traits (§5h.1).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyDef {
    pub moves: MoveMods,
    /// Hits of these elements deal no damage and apply no element status (knockback halved).
    pub immune: Vec<Element>,
    /// Hits of these elements drain the power like Water does.
    pub weak: Vec<Element>,
    /// Logia: dissolve and reform ~1 m away instead of taking a hit (3 s cooldown).
    pub scatter: bool,
    /// Rubber: blunt things boing off (little damage, extra launch).
    pub bounce: bool,
    /// Bara Bara: Blade hits pass through.
    pub blade_immune: bool,
    /// Bomu Bomu: explosions don't hurt you (they still push).
    pub explosion_immune: bool,
}

/// Standing graft merged into every cast (§5h.2), like a party effect.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraftDef {
    pub mechanics: Vec<Mechanic>,
    /// Applied only when the phrase itself is plain Light (the player's own element wins).
    pub element: Option<Element>,
    pub features: Vec<Feature>,
}

/// How an incanted phrase is delivered (§5h.3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Delivery {
    pub add: Vec<Mechanic>,
    pub remove: Vec<Mechanic>,
    /// Overrides the phrase's element.
    pub element: Option<Element>,
    pub scale_mult: f32,
    /// Objects are launched from the caster's fist (never dropped from the sky).
    pub from_fist: bool,
    /// Client: draw a stretching arm from the caster's hand to the construct while it lives.
    pub arm: bool,
}

impl Default for Delivery {
    fn default() -> Self {
        Self { add: vec![], remove: vec![], element: None, scale_mult: 1.0, from_fist: false, arm: false }
    }
}

/// A recipe in data form (compact; converted with [`RecipeDef::to_recipe`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecipeDef {
    pub name: String,
    pub form: Form,
    pub mech: Vec<Mechanic>,
    pub element: Element,
    pub scale: f32,
    pub mass: f32,
    pub count: u8,
    pub features: Vec<Feature>,
    pub temperament: Temperament,
    pub intensity: f32,
    pub payload: Option<Box<RecipeDef>>,
}

impl Default for RecipeDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            form: Form::Ball,
            mech: vec![],
            element: Element::Light,
            scale: 1.0,
            mass: 20.0,
            count: 1,
            features: vec![],
            temperament: Temperament::Calm,
            intensity: 0.0,
            payload: None,
        }
    }
}

impl RecipeDef {
    pub fn to_recipe(&self) -> Recipe {
        Recipe {
            name: self.name.clone(),
            form: self.form,
            mechanics: self.mech.clone(),
            element: self.element,
            scale: self.scale,
            mass: self.mass,
            count: self.count,
            accent: None,
            model: ModelSource::Procedural { seed: crate::rng::word_hash(&self.name) as u32 },
            payload: self.payload.as_ref().map(|p| {
                let mut r = p.to_recipe();
                r.payload = None;
                Box::new(r)
            }),
            features: self.features.clone(),
            grants: None,
            temperament: self.temperament,
            intensity: self.intensity,
            mount: None,
        }
        .normalized()
    }
}

/// Where an area op is centred.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum At {
    /// On the user.
    #[default]
    Me,
    /// Where the crosshair lands (≤ 60 m).
    Aim,
    /// `dist` m in front of the user.
    Front,
}

/// The closed set of things a move may do. Each maps onto an existing world operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MoveOp {
    /// Cast a recipe through the normal pipeline (pays its cost). `modifier` = a graft phrase
    /// (aimed at terrain it animates the terrain, §3c; aimed at an own construct it grafts).
    Cast {
        recipe: RecipeDef,
        #[serde(default)]
        modifier: bool,
        /// Cast it as a signature (the §5j amplifier: hollow purple, getsuga tensho…).
        #[serde(default)]
        signature: Option<crate::signature::SignatureKind>,
        /// The signature's fusion windup (hollow purple: `Some((Water, Fire))`).
        #[serde(default)]
        merge: Option<(Element, Element)>,
    },
    /// Self impulse along the flat look (fwd) and world up.
    Impulse { fwd: f32, up: f32 },
    /// Short teleport along the look (stops before walls). `afterimage`: client ghost.
    Blink {
        dist: f32,
        #[serde(default)]
        afterimage: bool,
    },
    /// Stretch/string to what the crosshair hits and yank yourself there.
    Reel { range: f32, speed: f32 },
    /// Cone hit in front (a big ring shove): the stand's fist, a paw, a neck.
    Shove {
        range: f32,
        angle: f32,
        damage: f32,
        knockback: f32,
        #[serde(default)]
        element: Element,
        #[serde(default)]
        up: f32,
    },
    /// Attract (> 0) / Repel (< 0) force field.
    Field {
        strength: f32,
        radius: f32,
        secs: f32,
        #[serde(default)]
        at: At,
        #[serde(default)]
        dist: f32,
    },
    /// Ope Ope ROOM: a strength-0 field that enables `Swap` inside it.
    Room { radius: f32, secs: f32 },
    /// Shambles: swap places with the aimed player (or own construct) — both must be inside
    /// your ROOM.
    Swap { range: f32 },
    /// Sticky Fingers zipper: a portal on the aimed wall and one `depth` m through it.
    Portal { depth: f32 },
    /// Chain the aimed player to you.
    Link { range: f32 },
    /// Radial blast (+ crater).
    Explode {
        radius: f32,
        damage: f32,
        knockback: f32,
        #[serde(default)]
        carve: f32,
        #[serde(default)]
        element: Element,
        #[serde(default)]
        at: At,
        #[serde(default)]
        dist: f32,
    },
    /// Carve a straight trench along the look (capsule); players in it are hit; `pull` yanks
    /// the first player in it to you (The Hand).
    Carve {
        length: f32,
        radius: f32,
        #[serde(default)]
        damage: f32,
        #[serde(default)]
        pull: bool,
    },
    /// The user alone moves at `speed`× while the world greys (never freezes anyone else).
    TimeStop { secs: f32, speed: f32 },
    /// Crazy Diamond: rebuild recently carved terrain and your own shattered constructs nearby.
    Restore { radius: f32 },
    /// Toggle the power's `transform`.
    Transform,
    /// A status on yourself (Kilo Kilo float = Float).
    SelfStatus { status: StatusKind, secs: f32 },
    /// The aimed player gets a status + a downward shove (Echoes 3 Freeze: heavy → sinks).
    Weigh { range: f32, secs: f32, down: f32 },
    /// When you next touch ground: a slam explosion there (Kilo press).
    SlamOnLand { radius: f32, damage: f32, knockback: f32, carve: f32 },
    /// Yank the aimed player (or construct) toward you along the crosshair — Universal Pull. A
    /// fast pull that breaks their tether and sets up a melee connect. `cone`: aim forgiveness (°).
    Yank {
        range: f32,
        speed: f32,
        #[serde(default)]
        cone: f32,
    },
    /// A gravity core (Planetary Devastation, a black hole): thrown `dist` m along the aim, it
    /// rises `rise` m, then for `secs` pulls enemies, loose constructs and carved terrain chunks
    /// (`chunks` carves) in with `strength` m/s² inside `radius`; enemies within `capture` m are
    /// held in the mass until it breaks (the end blast: `damage`, `knockback`) — or until they
    /// dodge or tether out. Everyone gets a cut-in (it is announced like a signature).
    Singularity {
        dist: f32,
        rise: f32,
        secs: f32,
        radius: f32,
        strength: f32,
        capture: f32,
        chunks: u8,
        #[serde(default)]
        damage: f32,
        #[serde(default)]
        knockback: f32,
    },
    /// Switch the power's tier (0 = base; Gear Second, Super Saiyan Blue…).
    Tier(u8),
}

/// Presentation of a move (bd_game `present::ascend`): the juice every big power move shares.
/// Never read by the sim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoveJuice {
    /// A spherical distortion ripple of this radius (m) where the move lands; 0 = none.
    pub ripple: f32,
    pub at: At,
    /// Metres in front for `at: Front`.
    pub dist: f32,
    /// Sub-bass boom + a world duck.
    pub boom: bool,
    /// Caster's camera punch (FOV degrees, comfort-gated).
    pub punch: f32,
    /// Screen shake for players near it (trauma 0..0.4, comfort-gated).
    pub shake: f32,
    /// Title letters over the caster (the move's name in big hard-light words).
    pub title: Option<String>,
    /// sRGB 0..1; `None` = the caster's corps colour.
    pub color: Option<[f32; 3]>,
    /// Loose debris near it is flung (> 0 out, < 0 in) with this speed (m/s).
    pub debris: f32,
}

impl Default for MoveJuice {
    fn default() -> Self {
        Self { ripple: 0.0, at: At::Me, dist: 0.0, boom: false, punch: 0.0, shake: 0.0, title: None, color: None, debris: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoveDef {
    pub name: String,
    pub cooldown: f32,
    /// Willpower (Cast ops additionally pay their own cast cost).
    pub cost: f32,
    pub ops: Vec<MoveOp>,
    /// How it lands (presentation only).
    pub juice: Option<MoveJuice>,
}

impl Default for MoveDef {
    fn default() -> Self {
        Self { name: String::new(), cooldown: 1.0, cost: 0.0, ops: vec![], juice: None }
    }
}

fn one() -> f32 {
    1.0
}

/// How a critical hit looks (presentation).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CritFx {
    /// A white-hot spark.
    #[default]
    Spark,
    /// Black Flash: black-red lightning, space bends around the impact.
    BlackFlash,
}

/// Generic power effects (`powers/effects.rs`). Any power, tier or transform may list them, and
/// they stack across every power a player holds. Deterministic and host-authoritative; the
/// movement ones (`PowerMult::speed`, `AirJumps`) are read by client prediction identically.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PowerEffect {
    /// An incoming dodgeable hit may trigger an automatic dodge (the dodge's i-frames + a short
    /// sidestep) — Ultra Instinct. `chance` 0..1 per hit, `cooldown` s between auto-dodges.
    AutoDodge { chance: f32, cooldown: f32 },
    /// Enemy projectiles and thrown things crawl near you (never hitscan or melee) — Infinity.
    /// Inside `radius` their speed keeps `factor` of itself every 0.1 s, down to a crawl
    /// ([`super::effects::SLOW_FIELD_CRAWL`]) — they still arrive, late.
    ProjectileSlowField { radius: f32, factor: f32 },
    /// A trade (Kaio-ken, forms): your own damage % rises and / or willpower drains while held.
    SelfDrain {
        #[serde(default)]
        damage_pct_per_s: f32,
        #[serde(default)]
        willpower_per_s: f32,
    },
    /// Damage and knockback you deal, your run speed, and your power-move cooldowns (< 1 = faster).
    PowerMult {
        #[serde(default = "one")]
        damage: f32,
        #[serde(default = "one")]
        speed: f32,
        #[serde(default = "one")]
        knockback: f32,
        #[serde(default = "one")]
        cooldown: f32,
    },
    /// Melee connects (weapons, shoves, lunges, power cone hits) may crit: `mult` × damage and
    /// knockback, a deterministic roll — Black Flash.
    CritChance {
        chance: f32,
        mult: f32,
        #[serde(default)]
        fx: CritFx,
    },
    /// Presentation only: attacks coming at you inside `radius` get a brief telegraph glow on
    /// your screen — Sharingan. No time dilation, nothing in the sim.
    ReadSlow { radius: f32 },
    /// `extra` more air jumps per airtime on top of the one everybody has (Gear Fourth's bounce).
    /// Movement: `MoveModifiers::extra_air_jumps`, host and prediction alike.
    AirJumps { extra: u8 },
}

/// Aura look of an Ascend (bd_game `present::ascend`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AscendStyle {
    /// Anime flame aura (super saiyan, kaio-ken).
    #[default]
    Flame,
    /// White steam clouds (Gear Second, Gear Fifth).
    Steam,
    /// Calm wisps and a still ring (ultra instinct, sage mode).
    Calm,
}

/// The ONE transformation moment (presentation only; the sim never reads it): windup pose →
/// aura ignites in `color` → burst → title letters → cosmetic tints → a lingering aura while the
/// form lasts. Every form / tier / power-up in powers.ron drives it with data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AscendDef {
    /// sRGB 0..1; `None` = the player's corps colour.
    pub color: Option<[f32; 3]>,
    /// 1 = a base form; tiers escalate (≈ 0.6 … 2.5).
    pub intensity: f32,
    /// Title letters ("SUPER SAIYAN BLUE").
    pub title: Option<String>,
    /// `false` = flicker and crackle (Kaio-ken, risky forms).
    pub stable: bool,
    pub style: AscendStyle,
    /// Cosmetic tint hooks the costume system can read (sRGB 0..1).
    pub hair_tint: Option<[f32; 3]>,
    pub eye_glow: Option<[f32; 3]>,
    pub skin_tint: Option<[f32; 3]>,
    /// Keep a low aura while the form lasts.
    pub linger: bool,
    /// A music layer while the form is active ("drums"): the audio side maps it.
    pub music: Option<String>,
    /// "The world goes quiet" for the caster: audio dip + everything but you desaturates a beat.
    pub quiet: bool,
    /// Toon physics: squash-and-stretch body + cartoon eyes hook (Gear Fifth).
    pub toon: bool,
}

impl Default for AscendDef {
    fn default() -> Self {
        Self {
            color: None,
            intensity: 1.0,
            title: None,
            stable: true,
            style: AscendStyle::Flame,
            hair_tint: None,
            eye_glow: None,
            skin_tint: None,
            linger: true,
            music: None,
            quiet: false,
            toon: false,
        }
    }
}

/// A tier of a power (Gear Second…Fifth, Super Saiyan 2 / 3 / God / Blue, Kaio-ken ×10…): picked
/// by saying one of its `names` while you hold the power (a technique's tier names also grant it).
/// Everything is optional and overrides / adds to the base power while the tier is on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TierDef {
    pub name: String,
    pub names: Vec<String>,
    /// Client costume id (look), like a transform.
    pub costume: String,
    /// Replaces the base body.
    pub body: Option<BodyDef>,
    /// Added to the base graft.
    pub graft: Option<GraftDef>,
    /// Replace the base effects while on (empty = keep the base's): tier numbers are totals.
    pub effects: Vec<PowerEffect>,
    /// Replace the base moves while on (empty = keep).
    pub moves: Vec<MoveDef>,
    /// Extra shouts while on.
    pub shouts: Vec<(String, u8)>,
    /// Extra incantation prefixes while on ("gomu gomu no jet") and their delivery.
    pub incantations: Vec<String>,
    pub delivery: Option<Delivery>,
    pub ascend: Option<AscendDef>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransformDef {
    /// Client costume id for the Transforms panel system (beast / hybrid form).
    pub costume: String,
    /// Replaces the base body while transformed.
    pub body: BodyDef,
    /// Extra standing graft while transformed.
    pub graft: Option<GraftDef>,
    /// Become a piloted construct instead (Kaido's dragon): cast + seat; eject = transform back.
    pub pilot: Option<RecipeDef>,
    /// Added to the base effects while transformed.
    pub effects: Vec<PowerEffect>,
    /// The transformation moment (presentation).
    pub ascend: Option<AscendDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompanionDef {
    pub recipe: RecipeDef,
    /// One rush punch (per typed token).
    pub rush_damage: f32,
    pub rush_knockback: f32,
    pub rush_range: f32,
}

impl Default for CompanionDef {
    fn default() -> Self {
        Self { recipe: RecipeDef::default(), rush_damage: 3.0, rush_knockback: 3.0, rush_range: 2.6 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PowerDef {
    /// Stable id (`Recipe::grants`). Field names `id` / `name` / `names` / `incantations` / `kind`
    /// / `companion` are also read by the lexicon's scanner (bd_lexicon/src/powers.rs).
    pub id: String,
    pub name: String,
    pub kind: PowerKind,
    /// Names the lexicon maps to this id ("gum gum fruit", "star platinum").
    #[serde(default)]
    pub names: Vec<String>,
    /// Names the lexicon keeps for this power even when they are very common words ("infinity").
    #[serde(default)]
    pub claim: Vec<String>,
    #[serde(default)]
    pub body: BodyDef,
    #[serde(default)]
    pub graft: Option<GraftDef>,
    /// Lower-case word sequences ("gomu gomu no"): `<prefix> <phrase>` is an incantation
    /// (whole words, punctuation ignored). Empty = no incantation.
    #[serde(default)]
    pub incantations: Vec<String>,
    /// How an incanted phrase is delivered (§5h.3).
    #[serde(default)]
    pub delivery: Delivery,
    /// Whole phrases that trigger a move slot ("za warudo" → 1; slots 2 / 3 exist only as shouts).
    /// Checked before incantations.
    #[serde(default)]
    pub shouts: Vec<(String, u8)>,
    /// ≤ 4: [G tap, G hold, shout-only, shout-only].
    #[serde(default)]
    pub moves: Vec<MoveDef>,
    #[serde(default)]
    pub transform: Option<TransformDef>,
    #[serde(default)]
    pub companion: Option<CompanionDef>,
    /// Look of the pickup (fruits / origins); `None` = a plain glowing orb named after the power.
    #[serde(default)]
    pub pickup: Option<PickupDef>,
    /// Generic effects while held (`powers/effects.rs`).
    #[serde(default)]
    pub effects: Vec<PowerEffect>,
    /// The moment you gain it (presentation; techniques / forms).
    #[serde(default)]
    pub ascend: Option<AscendDef>,
    /// Tiers picked by phrase (index + 1 = tier; 0 = base).
    #[serde(default)]
    pub tiers: Vec<TierDef>,
}

impl PowerDef {
    /// Tier `t` (1-based; 0 = base → `None`).
    pub fn tier(&self, t: u8) -> Option<&TierDef> {
        t.checked_sub(1).and_then(|k| self.tiers.get(k as usize))
    }

    /// The tier a spoken phrase names (1-based), if any.
    pub fn tier_named(&self, phrase: &str) -> Option<u8> {
        let n = norm_words(phrase);
        self.tiers.iter().position(|t| t.names.iter().any(|a| *a == n)).map(|k| (k + 1).min(15) as u8)
    }

    /// Is `phrase` one of this power's own names?
    pub fn is_named(&self, phrase: &str) -> bool {
        let n = norm_words(phrase);
        !n.is_empty() && (norm_words(&self.name) == n || self.names.iter().any(|a| *a == n) || self.id.replace('_', " ") == n)
    }

    /// Moves in effect at tier `t` (a tier's own moves replace the base ones).
    pub fn moves_at(&self, t: u8) -> &[MoveDef] {
        match self.tier(t) {
            Some(td) if !td.moves.is_empty() => &td.moves,
            _ => &self.moves,
        }
    }

    /// The Ascend of tier `t` (falls back to the power's own).
    pub fn ascend_at(&self, t: u8) -> Option<&AscendDef> {
        self.tier(t).and_then(|td| td.ascend.as_ref()).or(self.ascend.as_ref())
    }
}

/// Rules shared by every fruit / stand / origin (tuning lives here in code; powers.ron is only entries).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerRules {
    /// Willpower to cast a fruit or an origin pickup.
    pub fruit_cost: f32,
    /// Willpower to call a stand.
    pub stand_cost: f32,
    /// Seconds a Water (or weakness) hit drains a fruit.
    pub drain_secs: f32,
    pub scatter_cooldown: f32,
    pub scatter_dist: f32,
    /// Gravity multiplier for fruit users falling over the void.
    pub void_sink: f32,
    /// Second fruit: blast radius / damage / knockback.
    pub overdose_radius: f32,
    pub overdose_damage: f32,
    pub overdose_knockback: f32,
    /// Typed rush tokens (upper-case; the client counts them while the console is open).
    pub rush_tokens: Vec<String>,
    /// Fastest accepted rush rate (punches / s).
    pub rush_max_rate: f32,
    /// Phrase words that make a Link seastone (a Water-element Link also is).
    pub seastone_words: Vec<String>,
    /// Willpower to learn a technique (saying its name).
    pub technique_cost: f32,
    /// Powers one player can hold at once (the oldest is replaced beyond it).
    pub max_powers: u8,
}

impl Default for PowerRules {
    fn default() -> Self {
        Self {
            fruit_cost: 40.0,
            stand_cost: 35.0,
            drain_secs: 3.0,
            scatter_cooldown: 3.0,
            scatter_dist: 1.1,
            void_sink: 1.6,
            overdose_radius: 6.0,
            overdose_damage: 35.0,
            overdose_knockback: 28.0,
            rush_tokens: vec!["ORA".into(), "MUDA".into(), "DORA".into(), "ARI".into(), "WRYY".into()],
            rush_max_rate: 20.0,
            seastone_words: vec!["seastone".into(), "kairoseki".into(), "sea prism".into()],
            technique_cost: 30.0,
            max_powers: 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PowerTable {
    #[serde(default)]
    pub rules: PowerRules,
    pub powers: Vec<PowerDef>,
}

/// The shipped table (compiled in; deterministic on every peer).
pub const SHIPPED_RON: &str = include_str!("../../../../assets/powers.ron");

impl PowerTable {
    pub fn parse(src: &str) -> Result<PowerTable, String> {
        // The file is a plain list of entries (the lexicon's scanner reads the same list).
        let powers: Vec<PowerDef> = ron::from_str(src).map_err(|e| e.to_string())?;
        let mut t = PowerTable { rules: PowerRules::default(), powers };
        t.sanitize();
        Ok(t)
    }

    fn sanitize(&mut self) {
        self.powers.truncate(250);
        for p in &mut self.powers {
            p.moves.truncate(super::move_sel::MAX_MOVES);
            p.id = p.id.to_ascii_lowercase();
            for a in &mut p.names {
                *a = norm_words(a);
            }
            for pre in &mut p.incantations {
                *pre = norm_words(pre);
            }
            p.incantations.retain(|p| !p.is_empty());
            if let Some(k) = &mut p.pickup {
                k.sanitize();
            }
            for (s, _) in &mut p.shouts {
                *s = norm_words(s);
            }
            p.tiers.truncate(15);
            for t in &mut p.tiers {
                t.moves.truncate(super::move_sel::MAX_MOVES);
                for a in &mut t.names {
                    *a = norm_words(a);
                }
                t.names.retain(|a| !a.is_empty());
                for pre in &mut t.incantations {
                    *pre = norm_words(pre);
                }
                t.incantations.retain(|p| !p.is_empty());
                for (s, _) in &mut t.shouts {
                    *s = norm_words(s);
                }
            }
            let fix_move = |m: &mut MoveDef| {
                if !m.cooldown.is_finite() || m.cooldown < 0.05 {
                    m.cooldown = 0.05;
                }
                if !m.cost.is_finite() || m.cost < 0.0 {
                    m.cost = 0.0;
                }
            };
            p.moves.iter_mut().for_each(fix_move);
            p.tiers.iter_mut().flat_map(|t| t.moves.iter_mut()).for_each(fix_move);
            super::effects::sanitize_effects(&mut p.effects);
            if let Some(t) = &mut p.transform {
                super::effects::sanitize_effects(&mut t.effects);
            }
            for t in &mut p.tiers {
                super::effects::sanitize_effects(&mut t.effects);
            }
        }
        self.rules.max_powers = self.rules.max_powers.clamp(1, 1 + super::MAX_EXTRA_POWERS as u8);
        for t in &mut self.rules.rush_tokens {
            *t = t.to_ascii_uppercase();
        }
        for w in &mut self.rules.seastone_words {
            *w = norm_words(w);
        }
    }

    /// The table every world uses (parsed once from [`SHIPPED_RON`]; an empty table if it fails
    /// to parse, so a bad data edit disables powers instead of crashing).
    pub fn get() -> &'static PowerTable {
        static T: OnceLock<PowerTable> = OnceLock::new();
        T.get_or_init(|| PowerTable::parse(SHIPPED_RON).unwrap_or(PowerTable { rules: PowerRules::default(), powers: vec![] }))
    }

    pub fn index_of(&self, key: &str) -> Option<u16> {
        self.powers.iter().position(|p| p.id.eq_ignore_ascii_case(key)).map(|i| i as u16)
    }

    pub fn def(&self, id: u16) -> Option<&PowerDef> {
        self.powers.get(id as usize)
    }

    /// Power id for a spoken / typed name ("gum gum fruit", "star platinum").
    pub fn id_for_name(&self, name: &str) -> Option<&str> {
        let n = norm_words(name);
        if n.is_empty() {
            return None;
        }
        self.powers.iter().find(|p| p.id == n.replace(' ', "_") || norm_words(&p.name) == n || p.names.iter().any(|a| *a == n)).map(|p| p.id.as_str())
    }

    /// Every (name, id) pair.
    pub fn names(&self) -> impl Iterator<Item = (&str, &str)> {
        self.powers.iter().flat_map(|p| p.names.iter().map(move |a| (a.as_str(), p.id.as_str())))
    }

    /// Random pools, in table order: `false` = devil fruits ("devil fruit": never an origin or a
    /// mutagen), `true` = stands ("stand arrow": only stands).
    pub fn ids_of(&self, stand: bool) -> impl Iterator<Item = u16> + '_ {
        self.powers
            .iter()
            .enumerate()
            .filter(move |(_, p)| if stand { p.kind.is_stand() } else { p.kind.is_fruit() && !p.id.starts_with(MUTAGEN_PREFIX) })
            .map(|(i, _)| i as u16)
    }

    /// Ids of every origin kit (for "random:origin"), in table order.
    pub fn origin_ids(&self) -> impl Iterator<Item = u16> + '_ {
        self.powers.iter().enumerate().filter(|(_, p)| p.kind.is_origin()).map(|(i, _)| i as u16)
    }

    /// Ids of every mutagen power (for "random:mutagen"), in table order.
    pub fn mutagen_ids(&self) -> impl Iterator<Item = u16> + '_ {
        self.powers.iter().enumerate().filter(|(_, p)| p.kind.is_fruit() && p.id.starts_with(MUTAGEN_PREFIX)).map(|(i, _)| i as u16)
    }

    pub fn is_rush_token(&self, token: &str) -> bool {
        let t = token.trim().trim_matches(|c: char| !c.is_ascii_alphabetic()).to_ascii_uppercase();
        !t.is_empty() && self.rules.rush_tokens.iter().any(|r| t == *r || (t.len() > r.len() && t.starts_with(r.as_str()) && t[r.len()..].chars().all(|c| c == r.chars().last().unwrap_or('A'))))
    }
}

/// Lower-case, punctuation stripped, single spaces ("Gomu-Gomu no… PISTOL!!" → "gomu gomu no pistol").
pub fn norm_words(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for w in s.split(|c: char| c.is_whitespace() || c == '-' || c == '_').filter(|w| !w.is_empty()) {
        let w: String = w.chars().filter(|c| c.is_alphanumeric() || *c == '\'').flat_map(|c| c.to_lowercase()).collect();
        if w.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&w);
    }
    out
}

/// What a typed/spoken phrase means for a player holding power `power` (client + lexicon contract).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PowerPhrase {
    /// Exactly a shout of the held power → [`crate::World::power_move`] with this slot.
    Move(u8),
    /// `<prefix> <rest>`: resolve `rest` with the lexicon, then call
    /// [`crate::World::cast_incantation`].
    Incantation { rest: String },
    /// Not a power phrase (cast it normally).
    Plain,
}

/// Parse a phrase against a single held power (`None` = no power → always `Plain`). An incantation
/// prefix of a power you don't hold is `Plain` (the whole phrase goes to the lexicon). Prefer
/// [`super::PlayerPower::parse_phrase`], which knows every power the player holds.
pub fn parse_phrase(power: Option<u16>, text: &str) -> PowerPhrase {
    let held = super::PlayerPower { id: power, ..Default::default() };
    held.parse_phrase(text)
}
