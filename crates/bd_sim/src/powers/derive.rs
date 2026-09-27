//! AUTO-KITS (docs/toolbox/AUTO_KITS.md): COMPACT generated character kits.
//!
//! `assets/powers/zz_derived_kits.ron` (written by `tools/derive_kits/derive_kits.py`) spells a character kit
//! as a handful of TEMPLATE ids + a few character params instead of full moves:
//!
//! ```ron
//! (id: "dk_heat_wave", name: "Heat Wave", kind: Kit, fx: "fire", names: ["heat wave", "mick rory"],
//!  derived: Some((el: Fire, tm: Aggressive, sz: 1.1, col: Some((1.0, 0.45, 0.1)), fr: Some("comic"),
//!   mv: [(Bolt(Fireball), "flame bolt"), (Evasive(Sprint), "sprint"), (Ward, "flame ward"),
//!        (Volley, "blazing volley"), (Storm, "inferno"), (Bomb, "blazing bomb")]))),
//! ```
//!
//! [`Expander::expand`] turns that into ordinary [`MoveDef`]s when the table is built
//! (`PowerTable::sanitize`), so derived kits run the exact hand-kit path afterwards. Pure and
//! deterministic (every peer expands the same). Moves are in slot order: Primary, Movement,
//! Defense + Counter, Combo, Ultimate, then untagged bonus moves.
//!
//! The ops are written as RON text and parsed (memoised by text), so ops that gain fields (with serde
//! defaults) never break this file. A template whose ops fail to parse is dropped (tests catch it).

use std::collections::HashMap;

use bd_core::mechanics::{Element, Form, Mechanic, Temperament};
use serde::{Deserialize, Serialize};

use super::{CritFx, MoveDef, MoveJuice, MoveOp, MoveTag};

/// One generated character kit, compact (see the module docs).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DerivedKit {
    /// Effective element (`Light` = physical / flavour only).
    pub el: Element,
    pub tm: Temperament,
    /// Size factor (0.85 small … 1.25 kaiju): damage, knockback and radii scale a little with it.
    pub sz: f32,
    /// The character's weapon, when a template cares (bolt: gun pellets / arrows / thrown knives).
    pub wp: Option<Wp>,
    /// Light-element flavour: `true` = tech (pellet guns, hard-light guards).
    pub tech: bool,
    /// Accent / costume colour for the ultimate's juice.
    pub col: Option<[f32; 3]>,
    /// Impact-frame style for the ultimate ("comic", "ink", "glitch", "holy", "void").
    pub fr: Option<String>,
    /// (template, move name) in slot order.
    pub mv: Vec<(Tpl, String)>,
}

impl Default for DerivedKit {
    fn default() -> Self {
        Self { el: Element::Light, tm: Temperament::Calm, sz: 1.0, wp: None, tech: false, col: None, fr: None, mv: vec![] }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Wp {
    Sword,
    Axe,
    Hammer,
    Spear,
    Bow,
    Gun,
    Whip,
    Claws,
    Staff,
    Shield,
    Knives,
    Fists,
}

/// Every move shape the generator can pick (tools/derive_kits/derive_kits.py mirrors the names).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tpl {
    // ---- primary
    Slash,
    Cleave,
    Thrust,
    Smash,
    Stomp,
    Stretch,
    Brawl(Brawl),
    Bolt(Bolt),
    Beam,
    BeamBounce,
    Breath,
    Bite,
    Lash,
    Spin,
    Hex,
    // ---- movement
    Blur,
    Streak,
    Teleport,
    Vanish,
    Ambush,
    Fly,
    Swoop,
    Boost,
    Leap,
    Charge,
    Pounce,
    Reel,
    Burrow,
    Surf,
    RollOut,
    Bounce,
    PhaseDash,
    ElemStep,
    Evasive(Evasive),
    // ---- defense (+ counter)
    Parry,
    Guard,
    Deflect,
    Ward,
    Brace,
    Evade,
    Smoke,
    Heal,
    Shell,
    Intangible,
    // ---- combo
    Flurry,
    FistBarrage,
    Rush,
    Blitz,
    Launcher,
    Grab(GrabKind),
    Volley,
    Sweep,
    Maul(MaulKind),
    TailLash,
    Curse,
    Crescent,
    Drill,
    FollowThrough,
    Dogpile,
    // ---- ultimate
    Storm,
    Awaken,
    Meteor,
    Blades,
    Cannon,
    DeadAim,
    Bombard,
    Summon(Form, bool, Option<Mechanic>),
    Stampede,
    Overdrive,
    Gamble,
    Guardian,
    Piledriver,
    Assassinate,
    Strafe,
    Orbital,
    Courage,
    Knockout,
    Beatdown,
    TopRope,
    Rage,
    Nap,
    FreakOut,
    Showstopper,
    Throttle,
    TimeStop,
    // ---- bonus (untagged; Breath / Beam / Stomp / Bolt reuse the primary shapes)
    Grow,
    Shrink,
    Yank,
    WebSnare,
    Seek,
    Bomb,
    Gravity,
    Repulse,
    Trap,
    Rewind,
    Portal,
    Drop,
    Roar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Brawl {
    Jab,
    Haymaker,
    Kick,
    FlyingKick,
    Palm,
    Headbutt,
    Slap,
    Elbow,
    Punch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Bolt {
    Fireball,
    Thunder,
    Lift,
    Ray,
    Shard,
    Cutter,
    Rock,
    Spit,
    Orb,
    Pellets,
    Arrow,
    HomingArrow,
    Knives,
    Energy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Evasive {
    Roll,
    Slide,
    Vault,
    Springback,
    Sprint,
    Sidestep,
    Flee,
    Cartwheel,
    Flop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GrabKind {
    Constrict,
    Toss,
    Suplex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MaulKind {
    Maul,
    Frenzy,
    Rend,
    PackHunt,
}

/// A built move before its RON text is parsed.
struct Shape {
    ops: String,
    cd: f32,
    cost: f32,
    fx: &'static str,
    juice: String,
    crit: Option<(f32, CritFx)>,
}

fn shape(ops: impl Into<String>, cd: f32, cost: f32) -> Shape {
    Shape { ops: ops.into(), cd, cost, fx: "", juice: String::new(), crit: None }
}

impl Shape {
    fn fx(mut self, fx: &'static str) -> Self {
        self.fx = fx;
        self
    }
    fn juice(mut self, j: String) -> Self {
        self.juice = j;
        self
    }
}

/// Character numbers (mirrors derive_kits.py's old Ctx: size and temperament shade the numbers).
struct K<'a> {
    d: &'a DerivedKit,
    agg: f32,
    kbm: f32,
}

impl K<'_> {
    fn dmg(&self, x: f32) -> f32 {
        x * self.agg * (0.92 + 0.08 * self.d.sz)
    }
    fn kb(&self, x: f32) -> f32 {
        x * self.d.sz * self.kbm
    }
    fn rad(&self, x: f32) -> f32 {
        x * (0.85 + 0.15 * self.d.sz)
    }
    fn light(&self) -> bool {
        self.d.el == Element::Light
    }
    /// `, element: Fire` (nothing for Light).
    fn el(&self) -> String {
        if self.light() {
            String::new()
        } else {
            format!(", element: {:?}", self.d.el)
        }
    }
    fn status(&self) -> Option<&'static str> {
        Some(match self.d.el {
            Element::Fire | Element::Atomic => "Burn",
            Element::Electric => "Stun",
            Element::Ice => "Chill",
            Element::Water => "Wet",
            Element::Poison => "Poison",
            Element::Psychic => "Float",
            Element::Shadow => "Shadow",
            _ => return None,
        })
    }
    fn col(&self) -> String {
        match self.d.col {
            Some([r, g, b]) => format!(", color: Some(({r:.2}, {g:.2}, {b:.2}))"),
            None => String::new(),
        }
    }
    /// The ultimate's landing: title letters, boom, colour, impact frame.
    fn ult(&self, title: &str, extra: &str, punch: f32, shake: f32) -> String {
        let fr = self.d.fr.as_deref().map(|f| format!(", impact_frame: Some({f:?})")).unwrap_or_default();
        format!("(title: Some({:?}), boom: true, punch: {punch:.1}, shake: {shake:.2}{extra}{}{fr})", title.to_uppercase(), self.col())
    }
}

fn f(x: f32) -> String {
    format!("{x:.2}")
}

/// Expands compact kits, memoising parsed op / juice text (many characters share numbers).
#[derive(Default)]
pub struct Expander {
    ops: HashMap<String, Option<Vec<MoveOp>>>,
    juice: HashMap<String, Option<MoveJuice>>,
}

impl Expander {
    /// The full moves of `d` (the kit's own `fx` is `kit_fx`: a move's same style is left implicit).
    pub fn expand(&mut self, d: &DerivedKit, kit_name: &str, kit_fx: &str) -> Vec<MoveDef> {
        let agg = match d.tm {
            Temperament::Aggressive => 1.1,
            Temperament::Chaotic | Temperament::Lazy => 1.05,
            Temperament::Protective | Temperament::Playful => 0.95,
            Temperament::Fearful => 0.9,
            _ => 1.0,
        };
        let cdm = match d.tm {
            Temperament::Aggressive => 0.9,
            Temperament::Lazy => 1.2,
            Temperament::Fearful => 1.05,
            Temperament::Playful => 0.95,
            _ => 1.0,
        };
        let sz = if d.sz.is_finite() { d.sz.clamp(0.7, 1.4) } else { 1.0 };
        let d2 = DerivedKit { sz, ..d.clone() };
        let k = K { d: &d2, agg, kbm: if d.tm == Temperament::Aggressive { 1.05 } else { 1.0 } };
        let mut out = Vec::with_capacity(d.mv.len());
        for (slot, (t, name)) in d.mv.iter().enumerate().take(super::move_sel::MAX_MOVES) {
            let Some(s) = build(&k, *t, slot, kit_name) else { continue };
            let s = if slot == 4 { ult_juice(&k, *t, s, name) } else { s };
            let ops = self.ops.entry(s.ops.clone()).or_insert_with(|| ron::from_str::<Vec<MoveOp>>(&s.ops).ok()).clone();
            let Some(ops) = ops.filter(|o| !o.is_empty()) else { continue };
            let juice = if s.juice.is_empty() {
                None
            } else {
                self.juice.entry(s.juice.clone()).or_insert_with(|| ron::from_str::<MoveJuice>(&s.juice).ok()).clone()
            };
            let tags = match slot {
                0 => vec![MoveTag::Primary],
                1 => vec![MoveTag::Movement],
                2 => vec![MoveTag::Defense, MoveTag::Counter],
                3 => vec![MoveTag::Combo],
                4 => vec![MoveTag::Ultimate],
                _ => vec![],
            };
            let cooldown = (s.cd * if slot == 4 { 1.0 } else { cdm } * 100.0).round() / 100.0;
            out.push(MoveDef {
                name: name.clone(),
                cooldown,
                cost: s.cost,
                ops,
                juice,
                fx: if s.fx == kit_fx { String::new() } else { s.fx.to_string() },
                tags,
                crit: s.crit,
                ..Default::default()
            });
        }
        out
    }
}

/// The move shape of template `t` in `slot` (≥ 5 = a bonus move: longer cooldowns).
fn build(k: &K, t: Tpl, slot: usize, kit_name: &str) -> Option<Shape> {
    use Tpl::*;
    let bonus = slot >= 5;
    let el = k.el();
    let light = k.light();
    let s = match t {
        // ------------------------------------------------------------------ primary
        Slash | Cleave => {
            let heavy = t == Cleave;
            shape(
                format!(
                    "[Impulse(fwd: {}, up: 0.0), Shove(range: {}, angle: {}, damage: {}, knockback: {}{el})]",
                    f(if heavy { 3.0 } else { 5.0 }),
                    f(k.rad(if heavy { 4.2 } else { 3.6 })),
                    f(if heavy { 110.0 } else { 80.0 }),
                    f(k.dmg(if heavy { 13.0 } else { 10.0 })),
                    f(k.kb(if heavy { 15.0 } else { 10.0 }))
                ),
                if heavy { 1.1 } else { 0.8 },
                3.0,
            )
        }
        Thrust => shape(
            format!("[Impulse(fwd: 9.0, up: 0.0), Shove(range: {}, angle: 22.0, damage: {}, knockback: {}{el})]", f(k.rad(5.4)), f(k.dmg(12.0)), f(k.kb(14.0))),
            0.9,
            3.0,
        ),
        Smash => shape(
            format!("[Impulse(fwd: 4.0, up: 0.0), Shove(range: {}, angle: 90.0, damage: {}, knockback: {}, up: 0.25{el})]", f(k.rad(3.8)), f(k.dmg(12.0)), f(k.kb(16.0))),
            1.0,
            3.0,
        ),
        Stomp => shape(format!("[Explode(radius: {}, damage: {}, knockback: {}, carve: 0.8{el})]", f(k.rad(4.2)), f(k.dmg(10.0)), f(k.kb(14.0))), if bonus { 4.0 } else { 1.3 }, 3.0)
            .fx(if light { "earth" } else { "" })
            .juice("(shake: 0.15)".into()),
        Stretch => shape(format!("[Shove(range: 7.5, angle: 16.0, damage: {}, knockback: {}{el})]", f(k.dmg(10.0)), f(k.kb(14.0))), 0.8, 3.0).fx("rubber"),
        Brawl(b) => {
            // (lunge fwd, lunge up, range, angle, damage, knockback, up, cooldown)
            let (fwd, up, rng, ang, dmg, kb, lift, cd) = match b {
                self::Brawl::Jab => (5.0, 0.0, 3.0, 50.0, 7.0, 9.0, 0.0, 0.5),
                self::Brawl::Haymaker => (6.0, 0.0, 3.4, 70.0, 12.0, 18.0, 0.15, 1.1),
                self::Brawl::Kick => (4.0, 0.0, 3.8, 40.0, 10.0, 13.0, 0.25, 0.8),
                self::Brawl::FlyingKick => (9.0, 3.0, 3.4, 40.0, 10.0, 14.0, 0.1, 0.9),
                self::Brawl::Palm => (3.0, 0.0, 3.4, 40.0, 9.0, 20.0, 0.1, 0.9),
                self::Brawl::Headbutt => (6.0, 0.0, 2.8, 40.0, 11.0, 14.0, 0.2, 1.0),
                self::Brawl::Slap => (4.0, 0.0, 3.0, 80.0, 6.0, 14.0, 0.1, 0.6),
                self::Brawl::Elbow => (5.0, 0.0, 2.8, 60.0, 11.0, 12.0, 0.0, 0.8),
                self::Brawl::Punch => (5.0, 0.0, 3.2, 60.0, 9.0, 11.0, 0.0, 0.7),
            };
            shape(
                format!(
                    "[Impulse(fwd: {}, up: {}), Shove(range: {}, angle: {}, damage: {}, knockback: {}, up: {}{el})]",
                    f(fwd),
                    f(up),
                    f(k.rad(rng)),
                    f(ang),
                    f(k.dmg(dmg)),
                    f(k.kb(kb)),
                    f(lift)
                ),
                cd,
                2.0,
            )
        }
        Bolt(b) => {
            let e = k.d.el;
            let recipe = |name: &str, form: &str, mech: &str, scale: f32, mass: f32, count: u8| {
                let elp = if e == Element::Light { String::new() } else { format!(", element: {e:?}") };
                format!("[Cast(recipe: (name: {name:?}, form: {form}, mech: [{mech}]{elp}, scale: {}, mass: {}, count: {count}))]", f(scale), f(mass))
            };
            let ops = match b {
                self::Bolt::Thunder => format!("[Explode(radius: 2.6, damage: {}, knockback: {}, element: Electric, at: Aim)]", f(k.dmg(10.0)), f(k.kb(12.0))),
                self::Bolt::Lift => format!("[Afflict(range: 30.0, cone: 10.0, status: Some(Float), secs: 1.0, damage: {}, up: 5.0, element: Psychic)]", f(k.dmg(8.0))),
                self::Bolt::Ray => format!("[BounceBeam(length: 28.0, radius: 0.45, damage: {}, knockback: {}{el})]", f(k.dmg(11.0)), f(k.kb(14.0))),
                self::Bolt::Fireball => recipe("fireball", "Ball", "Explodes, Fast", 0.55, 6.0, 1),
                self::Bolt::Shard => recipe("ice shard", "Spear", "Pierce, Fast", 0.6, 5.0, 1),
                self::Bolt::Cutter => recipe("wind cutter", "Disc", "Blade, Fast", 0.6, 4.0, 1),
                self::Bolt::Rock => recipe("rock", "Boulder", "Heavy", 0.8, 40.0, 1),
                self::Bolt::Spit => recipe("venom spit", "Blob", "Sticky", 0.4, 4.0, 1),
                self::Bolt::Orb => recipe("dark orb", "Ball", "Homing", 0.5, 5.0, 1),
                self::Bolt::Pellets => recipe("pellets", "Ball", "Fast, Swarm", 0.25, 3.0, 3),
                self::Bolt::Arrow => recipe("arrow", "Spear", "Pierce, Fast", 0.55, 3.0, 1),
                self::Bolt::HomingArrow => recipe("arrow", "Spear", "Pierce, Fast, Homing", 0.55, 3.0, 1),
                self::Bolt::Knives => recipe("knives", "Disc", "Blade, Fast", 0.3, 1.0, 3),
                self::Bolt::Energy => recipe("energy bolt", "Ball", "Fast", 0.45, 5.0, 1),
            };
            shape(ops, if bonus { 2.0 } else { 0.8 }, 2.0)
        }
        Beam | BeamBounce => shape(
            format!(
                "[BounceBeam(length: 34.0, bounces: {}, radius: 0.45, damage: {}, knockback: {}{el})]",
                if t == BeamBounce { 2 } else { 0 },
                f(k.dmg(12.0)),
                f(k.kb(12.0))
            ),
            if bonus { 4.0 } else { 1.1 },
            if bonus { 4.0 } else { 3.0 },
        ),
        Breath => {
            let zone = k
                .status()
                .map(|st| format!(", Zone(radius: {}, secs: 2.0, status: Some({st}), at: Front, dist: 5.0)", f(k.rad(3.5))))
                .unwrap_or_default();
            shape(
                format!("[Shove(range: {}, angle: 32.0, damage: {}, knockback: {}{el}){zone}]", f(k.rad(8.0)), f(k.dmg(9.0)), f(k.kb(11.0))),
                if bonus { 5.0 } else { 1.3 },
                if bonus { 4.0 } else { 3.0 },
            )
        }
        Bite => shape(
            format!("[Pounce(range: 12.0, speed: 36.0, cone: 12.0, damage: {}, knockback: {}, bounce: 3.0{el})]", f(k.dmg(9.0)), f(k.kb(10.0))),
            0.9,
            3.0,
        ),
        Lash => shape(format!("[Hook(range: 16.0, speed: 70.0, damage: {}, status: Some(Stun), secs: 0.25)]", f(k.dmg(8.0))), 1.0, 3.0),
        Spin => shape(format!("[Shove(range: {}, angle: 179.0, damage: {}, knockback: {}{el})]", f(k.rad(3.6)), f(k.dmg(9.0)), f(k.kb(12.0))), 0.9, 3.0),
        Hex => {
            let st = k.status().unwrap_or("Stun");
            let secs = if st == "Stun" { 0.5 } else { 1.4 };
            shape(format!("[Afflict(range: 30.0, cone: 10.0, status: Some({st}), secs: {}, damage: {}{el})]", f(secs), f(k.dmg(8.0))), 1.0, 3.0)
        }
        // ------------------------------------------------------------------ movement
        Blur => shape("[Blink(dist: 12.0, afterimage: true)]", 2.5, 3.0).fx(if matches!(k.d.el, Element::Light | Element::Electric) { "speedforce" } else { "" }),
        Streak => shape(format!("[Ram(speed: 36.0, secs: 0.35, damage: {}, knockback: {}, through: true, keep: 0.4{el})]", f(k.dmg(6.0)), f(k.kb(10.0))), 3.0, 4.0)
            .fx(if matches!(k.d.el, Element::Light | Element::Electric) { "speedforce" } else { "" }),
        Teleport => shape("[Blink(dist: 14.0)]", 3.0, 3.0),
        Vanish => shape("[Blink(dist: 14.0)]", 3.0, 3.0).fx("smoke"),
        Ambush => shape("[WarpTo(range: 30.0, cone: 12.0, behind: 2.0)]", 5.0, 4.0).fx(if light { "smoke" } else { "" }),
        Fly => shape("[Impulse(fwd: 14.0, up: 12.0), SelfStatus(status: Float, secs: 1.2)]", 3.0, 3.0),
        Swoop => shape("[Impulse(fwd: 18.0, up: 6.0), SelfStatus(status: Float, secs: 0.8)]", 3.0, 3.0),
        Boost => shape("[Impulse(fwd: 20.0, up: 5.0)]", 2.5, 3.0).fx(if light { "hardlight" } else { "" }),
        Leap => shape(
            format!("[Leap(range: 24.0, apex: 8.0), SlamOnLand(radius: {}, damage: {}, knockback: {}, carve: 1.0)]", f(k.rad(4.2)), f(k.dmg(10.0)), f(k.kb(14.0))),
            4.0,
            4.0,
        )
        .fx(if light { "earth" } else { "" }),
        Charge => shape(
            format!("[Ram(speed: 24.0, secs: 0.7, damage: {}, knockback: {}, up: 0.2, through: true, keep: 0.5{el})]", f(k.dmg(9.0)), f(k.kb(16.0))),
            3.5,
            4.0,
        ),
        Pounce => shape(format!("[Pounce(range: 20.0, speed: 40.0, cone: 12.0, damage: {}, knockback: {}, bounce: 7.0)]", f(k.dmg(7.0)), f(k.kb(8.0))), 3.0, 3.0),
        Reel => shape("[Reel(range: 26.0, speed: 30.0)]", 2.5, 3.0),
        Burrow => shape(format!("[Blink(dist: 11.0), Explode(radius: 2.5, damage: {}, knockback: {}, carve: 0.6, element: Earth)]", f(k.dmg(6.0)), f(k.kb(10.0))), 3.5, 4.0).fx("earth"),
        Surf => shape(format!("[Ram(speed: 26.0, secs: 0.6, damage: {}, knockback: {}, through: true, keep: 0.5, element: Water)]", f(k.dmg(6.0)), f(k.kb(12.0))), 3.0, 4.0).fx("water"),
        RollOut => shape(format!("[Ram(speed: 30.0, secs: 0.9, damage: {}, knockback: {}, through: true, steer: 40.0, keep: 0.6)]", f(k.dmg(10.0)), f(k.kb(18.0))), 3.5, 4.0).fx("hardlight"),
        Bounce => shape("[Impulse(fwd: 8.0, up: 17.0)]", 2.5, 3.0).fx("rubber"),
        PhaseDash => shape("[Phase(secs: 1.2), Impulse(fwd: 12.0, up: 0.0)]", 5.0, 4.0).fx(if light { "spirit" } else { "" }),
        ElemStep => shape("[Blink(dist: 12.0, afterimage: true)]", 3.0, 3.0),
        Evasive(e) => {
            let (fwd, up, ev, blink) = match e {
                self::Evasive::Roll => (11.0, 3.0, 0.3, 0.0),
                self::Evasive::Slide => (15.0, 0.0, 0.2, 0.0),
                self::Evasive::Vault => (7.0, 11.0, 0.0, 0.0),
                self::Evasive::Springback => (-9.0, 10.0, 0.35, 0.0),
                self::Evasive::Sprint => (17.0, 1.0, 0.0, 0.0),
                self::Evasive::Sidestep => (0.0, 0.0, 0.25, 5.0),
                self::Evasive::Flee => (-12.0, 4.0, 0.3, 0.0),
                self::Evasive::Cartwheel => (10.0, 6.0, 0.25, 0.0),
                self::Evasive::Flop => (8.0, 2.0, 0.3, 0.0),
            };
            let mut ops = Vec::new();
            if blink > 0.0 {
                ops.push(format!("Blink(dist: {}, afterimage: true)", f(blink)));
            }
            if fwd != 0.0 || up != 0.0 {
                ops.push(format!("Impulse(fwd: {}, up: {})", f(fwd), f(up)));
            }
            if ev > 0.0 {
                ops.push(format!("Evade(secs: {})", f(ev)));
            }
            shape(format!("[{}]", ops.join(", ")), 2.5, 3.0)
        }
        // ------------------------------------------------------------------ defense
        Parry => shape(format!("[Counter(secs: 0.6, damage: {}, knockback: {}, up: 0.2{el})]", f(k.dmg(11.0)), f(k.kb(15.0))), 6.0, 4.0),
        Guard => shape("[Counter(secs: 1.0), Deflect(secs: 1.0, angle: 80.0)]", 6.0, 5.0).fx(if light { "hardlight" } else { "" }),
        Deflect => shape("[Deflect(secs: 1.1, angle: 90.0), Counter(secs: 0.3)]", 6.0, 4.0),
        Ward => {
            let zone = k.status().map(|st| format!(", Zone(radius: 4.0, secs: 2.5, status: Some({st}), follow: true)")).unwrap_or_default();
            shape(format!("[Field(strength: -45.0, radius: 4.5, secs: 1.0), Counter(secs: 0.4){zone}]"), 7.0, 5.0)
        }
        Brace => shape("[Empower(secs: 3.0, damage: 1.0, armor: 0.5), Counter(secs: 0.5)]", 8.0, 5.0),
        Evade => shape("[Evade(secs: 0.5, slow: 0.6, speed: 1.4, blink: 3.0)]", 6.0, 4.0),
        Smoke => shape("[Zone(radius: 5.0, secs: 3.0, status: Some(Shadow)), Evade(secs: 0.4)]", 8.0, 5.0).fx("smoke"),
        Heal => shape("[Mend(pct: 18.0, per_s: 3.0, secs: 3.0), Counter(secs: 0.3)]", 9.0, 5.0).fx(if light { "holy" } else { "" }),
        Shell => shape(format!("[Counter(secs: 1.2, damage: {}, knockback: {})]", f(k.dmg(6.0)), f(k.kb(10.0))), 6.0, 4.0),
        Intangible => shape("[Phase(secs: 1.4)]", 7.0, 4.0).fx(if light { "spirit" } else { "" }),
        // ------------------------------------------------------------------ combo
        Flurry => shape(
            format!(
                "[Barrage(hits: 6, interval: 0.08, range: {}, angle: 55.0, damage: {}, knockback: 2.0, finish: {}, advance: 3.0{el})]",
                f(k.rad(3.4)),
                f(k.dmg(4.0)),
                f(k.kb(16.0))
            ),
            6.0,
            8.0,
        )
        .juice("(punch: 2.5)".into()),
        FistBarrage => shape(
            format!(
                "[Barrage(hits: 8, interval: 0.07, range: {}, angle: 50.0, damage: {}, knockback: 2.0, finish: {}, advance: 2.0{el})]",
                f(k.rad(3.0)),
                f(k.dmg(3.0)),
                f(k.kb(18.0))
            ),
            6.0,
            8.0,
        )
        .juice("(punch: 2.5)".into()),
        Rush => shape(format!("[Rush(hits: 10, rate: 16.0, damage: {}, knockback: {}, range: 2.8)]", f(k.dmg(2.5)), f(k.kb(14.0))), 6.0, 8.0).juice("(punch: 2.5)".into()),
        Blitz => shape(
            format!("[Barrage(hits: 5, interval: 0.12, range: 3.2, angle: 60.0, damage: {}, knockback: 3.0, finish: {}, blink: true{el})]", f(k.dmg(5.0)), f(k.kb(18.0))),
            7.0,
            8.0,
        )
        .fx(match k.d.el {
            Element::Light | Element::Electric if k.d.tm != Temperament::Sneaky => "speedforce",
            Element::Light => "smoke",
            _ => "",
        })
        .juice("(punch: 3.0)".into()),
        Launcher => shape(
            format!(
                "[Shove(range: 3.4, angle: 60.0, damage: {}, knockback: 4.0, up: 1.4{el}), Impulse(fwd: 2.0, up: 13.0), SlamOnLand(radius: {}, damage: {}, knockback: {}, carve: 0.8)]",
                f(k.dmg(8.0)),
                f(k.rad(3.6)),
                f(k.dmg(11.0)),
                f(k.kb(16.0))
            ),
            7.0,
            8.0,
        )
        .juice("(shake: 0.2)".into()),
        Grab(g) => {
            let ops = match g {
                GrabKind::Constrict => format!("[Grab(range: 3.4, hold: 0.45, damage: {}, knockback: {}, up: 0.2)]", f(k.dmg(12.0)), f(k.kb(16.0))),
                GrabKind::Toss => format!("[Grab(range: 3.4, hold: 0.3, damage: {}, knockback: {}, up: 0.4)]", f(k.dmg(10.0)), f(k.kb(24.0))),
                GrabKind::Suplex => format!("[Grab(range: 3.2, hold: 0.3, damage: {}, knockback: {}, up: 0.35, back: true)]", f(k.dmg(10.0)), f(k.kb(22.0))),
            };
            shape(ops, 6.0, 8.0).juice("(punch: 3.0, shake: 0.15)".into())
        }
        Volley => shape(
            format!("[Salvo(count: 6, delay: 0.25, interval: 0.08, radius: 2.2, damage: {}, knockback: {}{el}, at: Aim, spread: 3.5)]", f(k.dmg(6.0)), f(k.kb(8.0))),
            7.0,
            8.0,
        )
        .juice("(punch: 2.0)".into()),
        Sweep => {
            let burst = if light { String::new() } else { format!(", Explode(radius: 3.0, damage: {}, knockback: {}{el}, at: Aim)", f(k.dmg(6.0)), f(k.kb(10.0))) };
            shape(format!("[Carve(length: 24.0, radius: 1.0, damage: {}){burst}]", f(k.dmg(12.0))), 7.0, 8.0).juice("(punch: 2.0)".into())
        }
        Maul(m) => {
            let ops = match m {
                MaulKind::Frenzy => format!(
                    "[Barrage(hits: 7, interval: 0.07, range: 3.0, angle: 80.0, damage: {}, knockback: 2.0, finish: {}, advance: 3.0{el})]",
                    f(k.dmg(3.0)),
                    f(k.kb(16.0))
                ),
                MaulKind::Rend => format!(
                    "[Barrage(hits: 3, interval: 0.16, range: 3.4, angle: 70.0, damage: {}, knockback: 4.0, finish: {}, advance: 5.0{el})]",
                    f(k.dmg(7.0)),
                    f(k.kb(18.0))
                ),
                MaulKind::PackHunt => format!(
                    "[Barrage(hits: 4, interval: 0.14, range: 3.2, angle: 60.0, damage: {}, knockback: 3.0, finish: {}, blink: true{el})]",
                    f(k.dmg(5.0)),
                    f(k.kb(16.0))
                ),
                MaulKind::Maul => format!(
                    "[Pounce(range: 12.0, speed: 36.0, cone: 12.0, damage: {}, knockback: 2.0), Barrage(hits: 4, interval: 0.1, range: 3.0, angle: 70.0, damage: {}, knockback: 2.0, finish: {}{el})]",
                    f(k.dmg(5.0)),
                    f(k.dmg(4.0)),
                    f(k.kb(14.0))
                ),
            };
            shape(ops, 6.0, 8.0).juice("(punch: 2.5)".into())
        }
        TailLash => shape(
            format!("[Barrage(hits: 3, interval: 0.15, range: {}, angle: 179.0, damage: {}, knockback: 6.0, finish: {}{el})]", f(k.rad(4.5)), f(k.dmg(6.0)), f(k.kb(16.0))),
            6.0,
            8.0,
        )
        .juice("(punch: 2.0)".into()),
        Curse => shape(
            format!(
                "[Afflict(range: 30.0, cone: 10.0, status: Some(Float), secs: 1.0, up: 6.0), DelayedHit(range: 30.0, cone: 10.0, secs: 0.7, damage: {}, knockback: {}{el})]",
                f(k.dmg(13.0)),
                f(k.kb(16.0))
            ),
            8.0,
            8.0,
        )
        .juice("(at: Aim, ripple: 3.0)".into()),
        Crescent => shape(
            format!("[Wave(length: 22.0, speed: 32.0, width: 3.0, carve: 0.6, damage: {}, knockback: {}{el})]", f(k.dmg(12.0)), f(k.kb(12.0))),
            6.0,
            8.0,
        )
        .juice("(punch: 2.5)".into()),
        Drill => shape(format!("[Drill(speed: 30.0, secs: 0.5, grind: 0.6, damage: {}, knockback: {}{el})]", f(k.dmg(12.0)), f(k.kb(20.0))), 7.0, 8.0).juice("(punch: 3.0)".into()),
        FollowThrough => shape(
            format!("[Finisher(lunge: 18.0, range: 3.5, angle: 60.0, damage: {}, knockback: {}{el}, window: 1.2, mult: 2.0)]", f(k.dmg(12.0)), f(k.kb(18.0))),
            6.0,
            8.0,
        )
        .juice("(punch: 3.0)".into()),
        Dogpile => shape(
            format!("[Salvo(count: 8, delay: 0.3, interval: 0.07, radius: 1.8, damage: {}, knockback: {}{el}, spread: 5.0, seek: true)]", f(k.dmg(5.0)), f(k.kb(6.0))),
            7.0,
            8.0,
        )
        .juice("(punch: 2.0)".into()),
        // ------------------------------------------------------------------ ultimate
        Storm => {
            let e = k.d.el;
            let ops = match e {
                Element::Water => format!("[Surge(width: 12.0, dist: 40.0, speed: 15.0, push: 18.0, damage: {}, element: Water)]", f(k.dmg(10.0))),
                Element::Ice => format!(
                    "[Zone(radius: {}, secs: 4.0, status: Some(Chill), slow: 0.5, at: Aim), Salvo(count: 6, delay: 0.9, interval: 0.15, radius: 3.0, damage: {}, knockback: {}, element: Ice, carve: 0.8, at: Aim, spread: 7.0, seek: true)]",
                    f(k.rad(10.0)),
                    f(k.dmg(12.0)),
                    f(k.kb(14.0))
                ),
                Element::Wind => format!(
                    "[Field(strength: 40.0, radius: {}, secs: 2.5, at: Aim), Zone(radius: 6.0, secs: 2.5, slow: 0.3, at: Aim), Salvo(count: 4, delay: 1.2, interval: 0.3, radius: 3.5, damage: {}, knockback: {}, element: Wind, at: Aim, spread: 3.0, up: 1.0)]",
                    f(k.rad(9.0)),
                    f(k.dmg(10.0)),
                    f(k.kb(18.0))
                ),
                Element::Earth => format!(
                    "[Explode(radius: {}, damage: {}, knockback: {}, carve: 1.5, element: Earth), Erupt(length: 20.0, width: 3.0, height: 4.0, count: 7, element: Earth, damage: {}, launch: 14.0)]",
                    f(k.rad(7.0)),
                    f(k.dmg(10.0)),
                    f(k.kb(18.0)),
                    f(k.dmg(12.0))
                ),
                Element::Poison => format!(
                    "[Zone(radius: {}, secs: 5.0, status: Some(Poison), dps: 4.0, slow: 0.2, at: Aim), Contagion(range: 30.0, cone: 12.0, status: Poison, secs: 5.0, spread: 5.0)]",
                    f(k.rad(9.0))
                ),
                Element::Psychic => format!(
                    "[Singularity(dist: 14.0, rise: 3.0, secs: 2.2, radius: {}, strength: 36.0, capture: 2.0, chunks: 6, damage: {}, knockback: {})]",
                    f(k.rad(10.0)),
                    f(k.dmg(12.0)),
                    f(k.kb(20.0))
                ),
                Element::Shadow => format!(
                    "[Zone(radius: {}, secs: 4.0, status: Some(Shadow)), DelayedHit(radius: {}, secs: 1.2, damage: {}, knockback: {}, element: Shadow)]",
                    f(k.rad(10.0)),
                    f(k.rad(10.0)),
                    f(k.dmg(16.0)),
                    f(k.kb(16.0))
                ),
                Element::Atomic => format!(
                    "[Explode(radius: {}, damage: {}, knockback: {}, carve: 2.5, element: Atomic), Zone(radius: {}, secs: 3.0, status: Some(Burn), dps: 3.0), Strain(pct: 6.0)]",
                    f(k.rad(9.0)),
                    f(k.dmg(22.0)),
                    f(k.kb(30.0)),
                    f(k.rad(8.0))
                ),
                _ => format!(
                    "[Salvo(count: 8, delay: 0.8, interval: 0.12, radius: 3.2, damage: {}, knockback: {}{el}, carve: 1.2, at: Aim, spread: 8.0, seek: true)]",
                    f(k.dmg(13.0)),
                    f(k.kb(16.0))
                ),
            };
            shape(ops, 20.0, 40.0)
        }
        Awaken => shape(
            format!(
                "[SelfResize(scale: 1.5, secs: 8.0), Empower(secs: 8.0, damage: 1.25, armor: 0.6, contact: {}, contact_kb: {}, contact_cd: 0.5{el}), Explode(radius: {}, damage: {}, knockback: {}, carve: 1.0{el})]",
                f(k.dmg(6.0)),
                f(k.kb(14.0)),
                f(k.rad(5.0)),
                f(k.dmg(8.0)),
                f(k.kb(18.0))
            ),
            24.0,
            40.0,
        ),
        Meteor => shape(
            format!("[Counter(secs: 1.2), Impulse(fwd: 10.0, up: 24.0), SlamOnLand(radius: {}, damage: {}, knockback: {}, carve: 3.0)]", f(k.rad(7.5)), f(k.dmg(22.0)), f(k.kb(28.0))),
            20.0,
            40.0,
        ),
        Blades => shape(
            format!(
                "[Barrage(hits: 8, interval: 0.09, range: 3.6, angle: 70.0, damage: {}, knockback: 2.0, finish: {}, blink: true{el}), Wave(length: 30.0, speed: 36.0, width: 7.0, carve: 1.0, damage: {}, knockback: {}{el})]",
                f(k.dmg(5.0)),
                f(k.kb(26.0)),
                f(k.dmg(14.0)),
                f(k.kb(18.0))
            ),
            20.0,
            40.0,
        ),
        Cannon => shape(
            format!("[Carve(length: 44.0, radius: 1.8, damage: {}), Explode(radius: {}, damage: {}, knockback: {}, carve: 1.5{el}, at: Aim)]", f(k.dmg(20.0)), f(k.rad(5.0)), f(k.dmg(14.0)), f(k.kb(20.0))),
            20.0,
            40.0,
        ),
        DeadAim => shape(format!("[LockOn(secs: 1.6, range: 55.0, damage: {}, knockback: {}, angle: 30.0)]", f(k.dmg(30.0)), f(k.kb(12.0))), 20.0, 40.0),
        Bombard => shape(
            format!("[Salvo(count: 14, delay: 0.35, interval: 0.1, radius: 2.2, damage: {}, knockback: {}{el}, spread: 11.0, seek: true)]", f(k.dmg(6.0)), f(k.kb(6.0))),
            20.0,
            40.0,
        ),
        Summon(form, flies, mech) => {
            let minion = match form {
                Form::Insect => "swarm",
                Form::Arachnid => "brood",
                Form::Bird => "flock",
                Form::Fish => "school",
                Form::Serpent => "nest",
                Form::Quadruped => "pack",
                _ => "horde",
            };
            let mut mechs = vec!["Swarm".to_string()];
            if flies {
                mechs.push("Flies".into());
            }
            if let Some(m) = mech {
                mechs.push(format!("{m:?}"));
            }
            let elp = if light { String::new() } else { format!(", element: {:?}", k.d.el) };
            let name = format!("{} {minion}", kit_name.to_lowercase());
            shape(
                format!(
                    "[Cast(recipe: (name: {name:?}, form: {form:?}, mech: [{}]{elp}, scale: 0.7, mass: 30.0, count: 6, temperament: Aggressive, intensity: 1.0), signature: Some(Summon))]",
                    mechs.join(", ")
                ),
                24.0,
                40.0,
            )
        }
        Stampede => shape(
            format!(
                "[Empower(secs: 5.0, damage: 1.2, armor: 0.7, contact: {}, contact_kb: {}, contact_cd: 0.4{el}), Ram(speed: 28.0, secs: 1.4, damage: {}, knockback: {}, through: true, steer: 60.0, keep: 0.5{el})]",
                f(k.dmg(7.0)),
                f(k.kb(16.0)),
                f(k.dmg(12.0)),
                f(k.kb(20.0))
            ),
            20.0,
            40.0,
        ),
        Overdrive => shape(
            format!(
                "[TimeStop(secs: 1.2, speed: 1.6), Barrage(hits: 7, interval: 0.1, range: 3.4, angle: 70.0, damage: {}, knockback: 2.0, finish: {}, blink: true{el})]",
                f(k.dmg(5.0)),
                f(k.kb(24.0))
            ),
            20.0,
            40.0,
        )
        .fx(if matches!(k.d.el, Element::Light | Element::Electric) { "speedforce" } else { "" }),
        Gamble => {
            let e = if light { Element::Fire } else { k.d.el };
            shape(
                format!(
                    "[Roulette(picks: [[Explode(radius: 7.0, damage: {}, knockback: {}, carve: 2.0, element: {e:?}, at: Aim)], [Salvo(count: 10, delay: 0.5, interval: 0.1, radius: 2.4, damage: {}, knockback: {}, at: Aim, spread: 9.0, seek: true)], [Field(strength: 45.0, radius: 10.0, secs: 2.0, at: Aim), Zone(radius: 7.0, secs: 3.0, status: Some(Stun), at: Aim)], [Empower(secs: 6.0, damage: 1.4, armor: 0.5, contact: 6.0, contact_kb: 12.0, contact_cd: 0.4), Mend(pct: 25.0)]])]",
                    f(k.dmg(20.0)),
                    f(k.kb(26.0)),
                    f(k.dmg(8.0)),
                    f(k.kb(10.0))
                ),
                20.0,
                40.0,
            )
        }
        Guardian => shape(
            format!("[Empower(secs: 6.0, damage: 1.15, armor: 0.4), Mend(pct: 20.0), Explode(radius: {}, damage: {}, knockback: {}{el})]", f(k.rad(6.0)), f(k.dmg(8.0)), f(k.kb(22.0))),
            22.0,
            40.0,
        )
        .fx(if light { "holy" } else { "" }),
        Piledriver => shape(
            format!(
                "[Grab(range: 3.4, hold: 0.45, damage: {}, knockback: 8.0, up: 1.4), Impulse(fwd: 2.0, up: 14.0), SlamOnLand(radius: {}, damage: {}, knockback: {}, carve: 1.5)]",
                f(k.dmg(14.0)),
                f(k.rad(5.0)),
                f(k.dmg(18.0)),
                f(k.kb(22.0))
            ),
            20.0,
            40.0,
        ),
        Assassinate => {
            let mut s = shape(
                format!("[WarpTo(range: 40.0, cone: 15.0, behind: 2.0), Shove(range: 3.2, angle: 60.0, damage: {}, knockback: {}{el})]", f(k.dmg(18.0)), f(k.kb(18.0))),
                20.0,
                40.0,
            )
            .fx(if light { "smoke" } else { "" });
            s.crit = Some((2.0, CritFx::Spark));
            s
        }
        Strafe => shape(
            format!(
                "[Impulse(fwd: 18.0, up: 10.0), SelfStatus(status: Float, secs: 1.0), Salvo(count: 8, delay: 0.3, interval: 0.08, radius: 2.4, damage: {}, knockback: {}, at: Front, dist: 12.0, spread: 6.0, seek: true)]",
                f(k.dmg(8.0)),
                f(k.kb(10.0))
            ),
            20.0,
            40.0,
        ),
        Orbital => shape(format!("[Salvo(count: 1, delay: 1.2, radius: {}, damage: {}, knockback: {}{el}, carve: 2.5, at: Aim)]", f(k.rad(7.0)), f(k.dmg(26.0)), f(k.kb(28.0))), 20.0, 40.0)
            .fx(if light { "hardlight" } else { "" }),
        Courage => shape(
            format!(
                "[Empower(secs: 6.0, damage: 1.35, armor: 0.6), Mend(pct: 20.0), Barrage(hits: 6, interval: 0.09, range: 3.0, angle: 60.0, damage: {}, knockback: 2.0, finish: {}, advance: 3.0)]",
                f(k.dmg(4.0)),
                f(k.kb(20.0))
            ),
            22.0,
            40.0,
        ),
        Knockout => shape(format!("[Drill(speed: 34.0, secs: 0.5, grind: 0.0, damage: {}, knockback: {}, up: 0.3{el})]", f(k.dmg(24.0)), f(k.kb(34.0))), 18.0, 40.0),
        Beatdown => shape(
            format!("[Barrage(hits: 10, interval: 0.07, range: 3.2, angle: 60.0, damage: {}, knockback: 2.0, finish: {}, advance: 4.0{el})]", f(k.dmg(3.0)), f(k.kb(26.0))),
            18.0,
            40.0,
        ),
        TopRope => shape(format!("[Leap(range: 18.0, apex: 7.0), SlamOnLand(radius: {}, damage: {}, knockback: {}, carve: 1.2)]", f(k.rad(5.0)), f(k.dmg(18.0)), f(k.kb(24.0))), 18.0, 40.0),
        Rage => shape(
            format!(
                "[Empower(secs: 6.0, damage: 1.4, armor: 0.7, contact: {}, contact_kb: {}, contact_cd: 0.5{el}), Ram(speed: 26.0, secs: 0.8, damage: {}, knockback: {}, through: true, keep: 0.4{el})]",
                f(k.dmg(5.0)),
                f(k.kb(12.0)),
                f(k.dmg(12.0)),
                f(k.kb(22.0))
            ),
            20.0,
            40.0,
        ),
        Nap => shape("[Mend(pct: 30.0, per_s: 4.0, secs: 4.0), Zone(radius: 7.0, secs: 3.0, slow: 0.55), Counter(secs: 1.0)]", 24.0, 40.0),
        FreakOut => shape(
            format!(
                "[Evade(secs: 0.6, slow: 0.8, speed: 1.5), Empower(secs: 5.0, damage: 1.3, armor: 0.6), Barrage(hits: 9, interval: 0.07, range: 3.0, angle: 90.0, damage: {}, knockback: 3.0, finish: {}, advance: 2.0)]",
                f(k.dmg(3.0)),
                f(k.kb(22.0))
            ),
            20.0,
            40.0,
        ),
        Showstopper => shape(
            format!(
                "[Explode(radius: {}, damage: {}, knockback: {}{el}), Zone(radius: 6.0, secs: 1.5, status: Some(Float)), Salvo(count: 5, delay: 0.9, interval: 0.12, radius: 2.4, damage: {}, knockback: {}, spread: 6.0, seek: true)]",
                f(k.rad(6.0)),
                f(k.dmg(8.0)),
                f(k.kb(14.0)),
                f(k.dmg(9.0)),
                f(k.kb(12.0))
            ),
            20.0,
            40.0,
        ),
        Throttle => shape(
            format!(
                "[Empower(secs: 4.0, damage: 1.2, armor: 0.6, contact: {}, contact_kb: {}, contact_cd: 0.4), Ram(speed: 40.0, secs: 1.2, damage: {}, knockback: {}, up: 0.2, through: true, steer: 40.0, keep: 0.6)]",
                f(k.dmg(6.0)),
                f(k.kb(16.0)),
                f(k.dmg(16.0)),
                f(k.kb(26.0))
            ),
            20.0,
            40.0,
        )
        .fx(if light { "hardlight" } else { "" }),
        TimeStop => shape("[TimeStop(secs: 2.0, speed: 2.0)]", 24.0, 40.0).fx(if light { "void" } else { "" }),
        // ------------------------------------------------------------------ bonus
        Grow => shape("[SelfResize(scale: 1.4, secs: 6.0)]", 12.0, 6.0),
        Shrink => shape("[SelfResize(scale: 0.6, secs: 5.0), Impulse(fwd: 10.0, up: 4.0)]", 10.0, 4.0),
        Yank => shape("[Yank(range: 28.0, speed: 26.0, cone: 12.0)]", 6.0, 4.0),
        WebSnare => shape("[Zone(radius: 4.0, secs: 3.0, status: Some(Stun), slow: 0.6, at: Aim)]", 8.0, 5.0),
        Seek => {
            let form = if k.d.wp == Some(Wp::Bow) { "Spear" } else { "Ball" };
            let elp = if light { String::new() } else { format!(", element: {:?}", k.d.el) };
            shape(
                format!(
                    "[Seeker(recipe: (name: \"homing shot\", form: {form}, mech: [Fast, Homing]{elp}, scale: 0.5, mass: 4.0), range: 45.0, cone: 20.0, speed: 34.0, damage: {}, knockback: {}{el}, hits: 2, chain: 10.0)]",
                    f(k.dmg(10.0)),
                    f(k.kb(12.0))
                ),
                6.0,
                5.0,
            )
        }
        Bomb => {
            let e = if light { Element::Fire } else { k.d.el };
            shape(format!("[Cast(recipe: (name: \"bomb\", form: Bomb, mech: [Explodes], element: {e:?}, scale: 0.6, mass: 6.0))]"), 4.0, 3.0)
        }
        Gravity => shape("[Field(strength: 40.0, radius: 8.0, secs: 1.5, at: Aim)]", 8.0, 5.0),
        Repulse => shape(format!("[Explode(radius: {}, damage: {}, knockback: {}{el})]", f(k.rad(5.0)), f(k.dmg(6.0)), f(k.kb(24.0))), 6.0, 4.0),
        Trap => shape("[Zone(radius: 3.5, secs: 6.0, status: Some(Stun), slow: 0.5, at: Aim)]", 8.0, 4.0),
        Rewind => shape("[Rewind(secs: 3.0)]", 12.0, 5.0).fx("void"),
        Portal => shape("[PlacePortal(range: 60.0, secs: 15.0)]", 3.0, 3.0),
        Drop => shape(format!("[Salvo(count: 1, delay: 0.8, radius: 4.0, damage: {}, knockback: {}, carve: 1.5, at: Aim)]", f(k.dmg(16.0)), f(k.kb(20.0))), 8.0, 5.0),
        Roar => shape("[GazeStun(range: 14.0, cone: 70.0, secs: 0.8, status: Stun, aimed: false)]", 10.0, 5.0),
    };
    Some(s)
}

/// Every ultimate lands with its title letters (the move's name), a boom, the character's colour and the
/// pack's impact frame.
fn ult_juice(k: &K, t: Tpl, s: Shape, title: &str) -> Shape {
    use Tpl::*;
    let (extra, punch) = match t {
        Storm | Orbital | Cannon => (", at: Aim, ripple: 8.0", 5.0),
        Awaken => (", ripple: 6.0, debris: 14.0", 5.0),
        Stampede | Throttle | Rage => (", debris: 12.0", 5.0),
        Guardian | Showstopper => (", ripple: 6.0", 5.0),
        TimeStop => (", ripple: 12.0", 5.0),
        DeadAim => ("", 3.0),
        Knockout => ("", 6.0),
        _ => ("", 5.0),
    };
    let shake = if t == DeadAim { 0.15 } else { 0.3 };
    Shape { juice: k.ult(title, extra, punch, shake), ..s }
}
