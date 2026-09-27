//! TWISTED MOVES (docs/toolbox/COMPOSED_MOVE.md §Modifiers & fusion) — the endless toolbox: every
//! famous move can be twisted. "giant rasengan", "triple kamehameha", "homing chidori", "ice hollow
//! purple", "black flash but fire", "reverse earthshatter", "rapid fire hadouken", "delayed pulse
//! bomb", "chain lightning rocket punch" — and FUSED: "rasengan + amaterasu", "fuse red and blue",
//! "bamf with earthshatter", "gravity field + rasengan".
//!
//! * **Modifiers** are DATA (`assets/move_modifiers.ron`, compiled in; [`ModTable::shipped`]): each is
//!   a handful of generic multipliers (size, reach, damage, knockback, speed, secs, cone, arc,
//!   cooldown), copies (count / repeat), behaviour mechanics (Homing, Pierce, Ricochet, Returning,
//!   Sticky, Explodes, Spin, Heavy), an element / style swap, and flags (reverse, delay, linger,
//!   chain, silent, invisible). Nothing is per move: [`ops::twist`] rewrites ANY move's ops by FIELD
//!   ROLES (every op's `radius` is a Size, every `damage` a Damage…, data-driven, so ops other
//!   workers add tomorrow are twisted too) plus a few typed conversions keyed by op kind (a homing
//!   lunge becomes a `Pounce`, a delayed blast a `Salvo`, a reversed blast an implosion…).
//! * **Fusion** ([`fuse`]): two moves → one, by what they ARE (op classes in the data): two
//!   projectiles merge into one orb with both behaviours and a Merge windup (the Hollow Purple
//!   grammar); a projectile + a strike = an imbued strike; a movement + an attack = the attack
//!   delivered at the end of the movement; a field + anything = the thing leaves that field.
//! * **Reading** ([`read`]): modifiers around a move core (prefix, suffix, "but X", "with X",
//!   adverbs, counts, "!!!"), fusion joiners ("+", "with", "fused with", "fuse A and B", "A and B
//!   combined", "A x B" only when both are moves). Pure; the core is whatever the TOOLBOX reads as a
//!   move (held / kit / borrow) or a famous cast the lexicon knows (a caller predicate).
//! * **Wire** ([`TwistStep`] inside `ToolStep::Twist`): base move refs + modifier ids; the host
//!   rebuilds the twisted move deterministically (nothing numeric from the client).
//! * **Price** ([`price`]): the twisted move's own presence tier (bigger ops read bigger) + each
//!   modifier's tier bump (giant +1) + the crowd rule for copies (triple = +1), capped below the
//!   Signature tier and at the bar; cooldown × the modifiers' factor on the BASE move's slot, and
//!   staleness counts the base move (cycling moves is what's cheap, not cycling adjectives).
//! * **Caps** ([`Caps`]): sizes ≤ 2.5×, damage ≤ 2×, ≤ 3 simultaneous copies, ≤ 8 shots, crowd
//!   control never lasts longer (status / stun / lock / time-stop seconds only shrink).

pub mod exec;
pub mod fuse;
pub mod ops;
pub mod read;

use std::sync::OnceLock;

use bd_core::mechanics::{Element, Mechanic, Recipe};
use bd_core::shape::ShapeSpec;
use serde::{Deserialize, Serialize};

use super::toolbox::MoveRef;
use crate::signature::SignatureCast;
use crate::status::StatusKind;

/// Most modifiers one side (or the whole) of a twisted move carries.
pub const MAX_MODS: usize = 8;

/// A field's role: which multiplier scales it ([`ops::twist`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Radii, widths, heights, recipe scale, carve: how BIG it is.
    Size,
    /// Ranges, lengths, blink distances: how FAR it reaches.
    Reach,
    Damage,
    /// Knockback, push, launch, field strength (sign kept).
    Knock,
    /// Travel / dash / projectile speeds.
    Speed,
    /// Durations of areas and buffs.
    Secs,
    /// Crowd-control seconds (statuses, stuns, locks, time stops, stances): only ever SHRINK.
    CcSecs,
    /// Aim-assist cones.
    Cone,
    /// Swing arcs (a cone hit's angle).
    Arc,
    /// Intrinsic counts (a salvo's strikes, a barrage's hits): × the copies instead of duplicating.
    Count,
    /// Never touched.
    Fixed,
}

/// What kind of word a modifier is (reading + presentation order of the title).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModKind {
    Size,
    Count,
    #[default]
    Behaviour,
    Element,
    Speed,
    Adverb,
    Style,
}

/// One modifier (`assets/move_modifiers.ron`). Multipliers default to 1, everything else to off.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModDef {
    /// The byte that travels on the wire (twists AND VERB GRAMMAR's composed specs): 1..=255,
    /// stable, never reused (1 = giant / "big", 2 = quick, 3 = long are the composed starter ids).
    pub n: u8,
    pub id: String,
    pub kind: ModKind,
    /// Whole-word phrases (lower case; multi-word allowed; mishearings included).
    pub words: Vec<String>,
    /// Title word ("GIANT"); empty = none.
    pub title: String,
    pub size: f32,
    pub reach: f32,
    pub damage: f32,
    pub knockback: f32,
    pub speed: f32,
    pub secs: f32,
    pub cone: f32,
    pub arc: f32,
    pub cooldown: f32,
    /// Simultaneous copies (a fan) — or sequential shots when the move can't be doubled in one tick.
    pub count: u8,
    /// Sequential shots (a stream along your look).
    pub repeat: u8,
    /// Seconds between shots (0 = the table default).
    pub interval: f32,
    /// Fan degrees between copies (0 = the table default).
    pub spread: f32,
    /// Behaviours (Homing, Pierce, Ricochet, Returning, Sticky, Explodes, Spin, Heavy, Fast).
    pub mech: Vec<Mechanic>,
    /// Element / style swap (the move re-skins AND re-flavours).
    pub element: Option<Element>,
    /// Presentation style key (`assets/tuning/move_fx.ron`); "" = the element's.
    pub fx: String,
    pub color: Option<[f32; 3]>,
    /// Pushes become pulls, blasts implode, dashes go backwards.
    pub reverse: bool,
    /// Seconds before it lands (blasts are planted, strikes marked, projectiles made sticky charges).
    pub delay: f32,
    /// Seconds a zone of its element lingers where it lands.
    pub linger: f32,
    /// Hits arc to this many more enemies (projectiles ricochet, strikes jump).
    pub chain: u8,
    /// No title / boom / camera punch.
    pub silent: bool,
    /// Presentation: no title, no colour, sneaky casts.
    pub invisible: bool,
    /// Presence-tier bump (giant +1, colossal +2, mini −1).
    pub tier: i8,
    /// Extra sparks on top of the tier.
    pub sparks: i8,
    /// Juice added (camera punch, shake, sub-bass boom).
    pub punch: f32,
    pub shake: f32,
    pub boom: bool,
}

impl Default for ModDef {
    fn default() -> Self {
        Self {
            n: 0,
            id: String::new(),
            kind: ModKind::Behaviour,
            words: vec![],
            title: String::new(),
            size: 1.0,
            reach: 1.0,
            damage: 1.0,
            knockback: 1.0,
            speed: 1.0,
            secs: 1.0,
            cone: 1.0,
            arc: 1.0,
            cooldown: 1.0,
            count: 1,
            repeat: 1,
            interval: 0.0,
            spread: 0.0,
            mech: vec![],
            element: None,
            fx: String::new(),
            color: None,
            reverse: false,
            delay: 0.0,
            linger: 0.0,
            chain: 0,
            silent: false,
            invisible: false,
            tier: 0,
            sparks: 0,
            punch: 0.0,
            shake: 0.0,
            boom: false,
        }
    }
}

/// Hard caps (fair and readable: no instant wins, crowd control stays short).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Caps {
    pub size_max: f32,
    pub size_min: f32,
    pub reach_max: f32,
    pub damage_max: f32,
    pub damage_min: f32,
    pub knock_max: f32,
    pub speed_max: f32,
    pub speed_min: f32,
    pub secs_max: f32,
    pub cone_max: f32,
    pub arc_max: f32,
    pub cooldown_max: f32,
    pub cooldown_min: f32,
    /// Simultaneous copies.
    pub count_max: u8,
    /// Sequential shots.
    pub repeat_max: u8,
    /// Per-copy damage when copied (× this once), per-shot damage ÷ shots^`repeat_falloff`.
    pub copy_damage: f32,
    pub repeat_falloff: f32,
    /// Tier bumps summed are clamped to ±this.
    pub tier_bump_max: i8,
    pub delay_max: f32,
    pub linger_max: f32,
    pub chain_max: u8,
    /// Seconds between shots / fan degrees when a modifier doesn't say.
    pub interval: f32,
    pub spread: f32,
    /// "!" in a row that make it a yell ([`ModTable::bang`] modifier).
    pub bang: u8,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            size_max: 2.5,
            size_min: 0.35,
            reach_max: 2.0,
            damage_max: 2.0,
            damage_min: 0.3,
            knock_max: 2.2,
            speed_max: 2.0,
            speed_min: 0.4,
            secs_max: 1.5,
            cone_max: 3.0,
            arc_max: 3.0,
            cooldown_max: 3.0,
            cooldown_min: 0.5,
            count_max: 3,
            repeat_max: 8,
            copy_damage: 0.75,
            repeat_falloff: 0.5,
            tier_bump_max: 2,
            delay_max: 3.0,
            linger_max: 6.0,
            chain_max: 4,
            interval: 0.16,
            spread: 12.0,
            bang: 3,
        }
    }
}

/// What a move IS, for fusion (by its ops' variant names, `classes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpClass {
    /// Thrown / fired things (casts, seekers, beams, waves, hooks).
    Projectile,
    /// Strikes (cone hits, flurries, grabs, finishers).
    Melee,
    /// Getting somewhere (blinks, dashes, leaps, lunges).
    Movement,
    /// Blasts and placed strikes (explosions, salvos, slams, eruptions).
    Blast,
    /// Lingering areas (fields, zones, rooms, singularities, time stops).
    Field,
    /// On yourself (stances, buffs, heals, statuses).
    Buff,
    /// Aimed at a person (afflictions, yanks, links, locks).
    Control,
}

/// Fusion knobs + joiners.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FusionDef {
    /// Op variant name → class (unknown ops: Projectile if they carry a recipe, else Blast).
    pub classes: Vec<(String, OpClass)>,
    /// "A <infix> B" ("fused with", "combined with", "+", "plus", "with").
    pub infix: Vec<String>,
    /// "<prefix> A <joiner> B" ("fuse", "combine", "fusion of").
    pub prefix: Vec<String>,
    pub joiners: Vec<String>,
    /// "A and B <suffix>" ("combined", "fused", "together").
    pub suffix: Vec<String>,
    /// "A x B": only when BOTH sides are moves ("goku x luffy" stays a costume mix).
    pub cross: Vec<String>,
    /// Cooldown = the longer one × this; both moves' slots cool.
    pub cooldown: f32,
    pub tier: i8,
    pub sparks: i8,
    /// Two projectiles merged: size × this, damage × this.
    pub merge_size: f32,
    pub merge_damage: f32,
    /// A strike imbued with a projectile: damage × this.
    pub imbue_damage: f32,
    pub title_join: String,
}

impl Default for FusionDef {
    fn default() -> Self {
        Self {
            classes: vec![],
            infix: vec![],
            prefix: vec![],
            joiners: vec![],
            suffix: vec![],
            cross: vec![],
            cooldown: 1.25,
            tier: 1,
            sparks: 0,
            merge_size: 1.25,
            merge_damage: 0.85,
            imbue_damage: 1.3,
            title_join: " + ".into(),
        }
    }
}

/// Element → (style key, colour, the non-CC status it may leave).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ElementLook {
    pub element: Element,
    pub fx: String,
    pub color: [f32; 3],
    #[serde(default)]
    pub status: Option<StatusKind>,
}

/// The whole modifier file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModTable {
    pub caps: Caps,
    /// Field roles: "field" or "Op.field" (the op-specific one wins). Unlisted numbers are Fixed.
    pub roles: Vec<(String, Role)>,
    /// Words skipped inside "but …" / "with …" tails ("it", "its", "make", "more", "extra"…).
    pub fillers: Vec<String>,
    /// Leading words skipped before a move ("a", "the", "my"…).
    pub articles: Vec<String>,
    /// Number words ("three" → 3); digits, "x3" and "3x" are read too.
    pub numbers: Vec<(String, u32)>,
    /// A number → the modifier id standing for it (the largest key ≤ n).
    pub counts: Vec<(u32, String)>,
    /// Words that introduce a modifier tail ("but", "with", "except").
    pub tails: Vec<String>,
    /// Words dropped from the END of a line before reading ("please", "now", "bro").
    pub trailing: Vec<String>,
    pub elements: Vec<ElementLook>,
    pub modifiers: Vec<ModDef>,
    pub fusion: FusionDef,
    /// The modifier a "!!!" yell adds ("" = none).
    pub bang_mod: String,
    /// Op variants that may be copied several times in ONE tick (instant, no per-player state):
    /// a "triple" of these fans out at once; anything else fires as quick sequential shots.
    pub fan: Vec<String>,
    /// The word index ([`ModTable::words`], built on first use).
    #[serde(skip)]
    pub(crate) index: OnceLock<read::WordIndex>,
}

impl Default for ModTable {
    fn default() -> Self {
        Self {
            index: OnceLock::new(),
            fan: vec![],
            caps: Caps::default(),
            roles: vec![],
            fillers: vec![],
            articles: vec![],
            numbers: vec![],
            counts: vec![],
            tails: vec![],
            trailing: vec![],
            elements: vec![],
            modifiers: vec![],
            fusion: FusionDef::default(),
            bang_mod: String::new(),
        }
    }
}

/// The shipped data (compiled in: every peer twists identically).
pub const SHIPPED_RON: &str = include_str!("../../../../../assets/move_modifiers.ron");

impl ModTable {
    pub fn parse(src: &str) -> Result<ModTable, String> {
        let mut t: ModTable = ron::from_str(src).map_err(|e| e.to_string())?;
        t.sanitize();
        Ok(t)
    }

    /// The shipped table (a parse error → an empty table with a stderr note: nothing twists, nothing
    /// crashes; `cargo test -p bd_sim --test twist` goes red).
    pub fn shipped() -> &'static ModTable {
        static T: OnceLock<ModTable> = OnceLock::new();
        T.get_or_init(|| match ModTable::parse(SHIPPED_RON) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("move_modifiers.ron: {e} (no move modifiers this run)");
                ModTable::default()
            }
        })
    }

    fn sanitize(&mut self) {
        self.modifiers.truncate(250);
        // a wire byte of 0 or one already taken: that modifier is dropped (the tests name it)
        let mut seen = [false; 256];
        self.modifiers.retain(|m| {
            let ok = m.n != 0 && !seen[m.n as usize];
            seen[m.n as usize] = true;
            ok
        });
        let norm = |s: &mut String| *s = super::norm_words(s);
        for m in &mut self.modifiers {
            m.id = m.id.trim().to_ascii_lowercase();
            m.words.iter_mut().for_each(norm);
            m.words.retain(|w| !w.is_empty());
            // "rapid-fire" and "rapid fire" normalise alike: keep one
            let mut seen_w = std::collections::HashSet::new();
            m.words.retain(|w| seen_w.insert(w.clone()));
            let fin = |v: &mut f32, lo: f32, hi: f32| *v = if v.is_finite() { v.clamp(lo, hi) } else { 1.0 };
            for v in [&mut m.size, &mut m.reach, &mut m.damage, &mut m.knockback, &mut m.speed, &mut m.secs, &mut m.cone, &mut m.arc, &mut m.cooldown] {
                fin(v, 0.05, 10.0);
            }
            m.count = m.count.max(1);
            m.repeat = m.repeat.max(1);
            for v in [&mut m.interval, &mut m.spread, &mut m.delay, &mut m.linger, &mut m.punch, &mut m.shake] {
                *v = if v.is_finite() { v.clamp(0.0, 60.0) } else { 0.0 };
            }
            m.mech.truncate(4);
            m.title = m.title.trim().to_uppercase();
        }
        self.fillers.iter_mut().for_each(norm);
        self.articles.iter_mut().for_each(norm);
        self.tails.iter_mut().for_each(norm);
        self.trailing.iter_mut().for_each(norm);
        for (w, _) in &mut self.numbers {
            norm(w);
        }
        self.counts.sort_by_key(|(n, _)| *n);
        for s in [&mut self.fusion.infix, &mut self.fusion.prefix, &mut self.fusion.joiners, &mut self.fusion.suffix, &mut self.fusion.cross] {
            // "+" / "&" stay symbols; words are normalised
            for w in s.iter_mut() {
                if w.chars().any(|c| c.is_alphanumeric()) {
                    norm(w);
                }
            }
        }
    }

    /// The wire byte of modifier `id` ("giant" → 1).
    pub fn id_of(&self, id: &str) -> Option<u8> {
        self.modifiers.iter().find(|m| m.id == id).map(|m| m.n)
    }

    /// The modifier a wire byte names (0 / unknown = `None`).
    pub fn get(&self, n: u8) -> Option<&ModDef> {
        if n == 0 {
            return None;
        }
        self.modifiers.iter().find(|m| m.n == n)
    }

    /// The modifier standing for a count of `n` (largest `counts` key ≤ n).
    pub fn count_mod(&self, n: u32) -> Option<u8> {
        if n < 2 {
            return None;
        }
        self.counts.iter().rev().find(|(k, _)| *k <= n).and_then(|(_, id)| self.id_of(id))
    }

    /// The "!!!" modifier.
    pub fn bang(&self) -> Option<u8> {
        if self.bang_mod.is_empty() { None } else { self.id_of(&self.bang_mod) }
    }

    /// A field's role (`"Op.field"` first, then `"field"`; unlisted = Fixed).
    pub fn role(&self, op: &str, field: &str) -> Role {
        let mut plain = None;
        for (k, r) in &self.roles {
            if let Some((o, f)) = k.split_once('.') {
                if o == op && f == field {
                    return *r;
                }
            } else if k == field && plain.is_none() {
                plain = Some(*r);
            }
        }
        plain.unwrap_or(Role::Fixed)
    }

    pub fn class_of(&self, op: &str) -> Option<OpClass> {
        self.fusion.classes.iter().find(|(o, _)| o == op).map(|(_, c)| *c)
    }

    pub fn look(&self, e: Element) -> Option<&ElementLook> {
        self.elements.iter().find(|l| l.element == e)
    }
}

// ------------------------------------------------------------------------------------ the wire

/// What a twisted move is built on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TwistBase {
    /// Any move of any power / tier / kit (a borrowed shout or "<kit> <move>").
    Move { mv: MoveRef, kit: bool },
    /// A held power's move selector (`move_sel`: a plain slot or `MOVE_OF`).
    Held(u8),
    /// A famous cast the lexicon resolved ("spirit bomb", "hadouken"): the recipe as resolved
    /// (costume / signature applied), twisted on the host.
    Cast { recipe: Box<Recipe>, shape: Option<ShapeSpec>, signature: Option<SignatureCast> },
    /// A DESCRIBED move (VERB GRAMMAR, docs/toolbox/COMPOSED_MOVE.md): "homing fire punch",
    /// "rapid fire uppercut". Its `spec.mods` are always moved into the side's modifiers (host and
    /// client alike): a modifier is applied ONCE, by the twist.
    Composed(crate::powers::compose::ComposedMove),
}

/// A twisted move's base as the presentation event names it (see `OpEvent::Twisted`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BaseRef {
    /// A famous cast (the client has no table entry for it: the event's title / element say it).
    #[default]
    None,
    Move(MoveRef),
    /// A described move's spec (its `mods` empty).
    Composed(crate::powers::compose::ComposedMove),
}

/// A composed spec with its modifier bytes taken out (they belong to the twist side), and those bytes.
pub fn split_composed(spec: crate::powers::compose::ComposedMove) -> (crate::powers::compose::ComposedMove, Vec<u8>) {
    let mut s = spec;
    let mods: Vec<u8> = s.mods.iter().copied().filter(|m| *m != 0).collect();
    s.mods = [0; crate::powers::compose::MAX_MODS];
    (s, mods)
}

/// One side of a twisted move: a base and its own modifiers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TwistSide {
    pub base: TwistBase,
    /// Modifier wire bytes ([`ModDef::n`]), phrase order.
    pub mods: Vec<u8>,
}

/// A twisted / fused move (travels in `ToolStep::Twist`; the host rebuilds it — [`exec`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TwistStep {
    pub a: TwistSide,
    /// Fused with this.
    pub b: Option<TwistSide>,
    /// Modifiers on the whole (after fusing).
    pub mods: Vec<u8>,
    /// 0 = the command; k > 0 = the host's own follow-up shot k (clients' are reset to 0).
    pub shot: u8,
}

impl TwistStep {
    /// Untrusted → safe (modifier lists capped, unknown ids dropped, follow-ups reset).
    pub fn sanitized(mut self, t: &ModTable) -> Self {
        let fix = |m: &mut Vec<u8>| {
            m.retain(|id| t.get(*id).is_some());
            m.truncate(MAX_MODS);
        };
        for s in std::iter::once(&mut self.a).chain(self.b.iter_mut()) {
            // a composed base's own modifier bytes move to the side: applied once, by the twist
            if let TwistBase::Composed(spec) = &mut s.base {
                let (clean, mods) = split_composed(*spec);
                *spec = clean;
                let mut all = mods;
                all.extend(s.mods.iter().copied());
                s.mods = all;
            }
            fix(&mut s.mods);
        }
        fix(&mut self.mods);
        self.shot = 0;
        self
    }

    /// Every modifier id, all sides.
    pub fn all_mods(&self) -> impl Iterator<Item = u8> + '_ {
        self.a.mods.iter().chain(self.b.iter().flat_map(|b| b.mods.iter())).chain(self.mods.iter()).copied()
    }
}

/// Words of a twist for logs / previews ("giant + homing on rasengan").
pub fn describe(t: &ModTable, mods: &[u8]) -> String {
    mods.iter().filter_map(|m| t.get(*m)).map(|m| m.id.as_str()).collect::<Vec<_>>().join(" + ")
}
