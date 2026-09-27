//! VERB GRAMMAR — "describe a power" (docs/toolbox/COMPOSED_MOVE.md).
//!
//! Named moves (assets/powers/*.ron) are the famous shortcuts; this is the open end of the
//! toolbox: an unscripted phrase whose HEAD IS A SELF-ACTION VERB becomes a real move of the
//! player's own — `verb × style/element × modifier × place × target × count × duration`:
//! "shadow teleport behind ethan", "lightning dash through him", "gravity punch", "rewind 3
//! seconds", "pull everyone to me", "leap onto the tower", "swap with the dragon".
//!
//! * [`grammar`] reads a phrase into a [`ComposedMove`] (pure; vocabulary = `assets/verbs.ron`,
//!   compiled in: [`VerbTable::get`]). A phrase is only read when EVERY word is understood and it
//!   has exactly one verb — a construct ("a dragon", "giant sword") never is.
//! * [`ComposedMove`] is the closed-set, quantised SPEC that travels host-authoritatively like a
//!   borrowed move (`chain::ToolStep::Composed`): ids of table entries + enums + small ints. The
//!   host re-validates it ([`ComposedMove::sanitized`]) and expands it ([`build::build`]) into an
//!   ordinary [`MoveDef`] of existing ops — so pricing (presence tier → sparks + staleness),
//!   cooldown, chains, active phases and presentation are the SAME rules as every other move.
//! * [`exec`] fires it (`World::fire_composed`): target resolution ("him", names, "it", "the
//!   dragon", "everyone"), aim override, price, per-spec cooldown, events.
//!
//! Modifiers ("giant", "quick"…) and fusions ride [`ComposedMove::mods`]; the MODIFIERS & FUSION
//! worker owns that table (a starter list lives in verbs.ron) and applies the same bytes to named
//! moves too.

pub mod build;
mod exec;
pub mod grammar;

use std::collections::HashMap;
use std::sync::OnceLock;

use bd_core::PlayerId;
use bd_core::mechanics::Element;
use serde::{Deserialize, Serialize};

use super::{MoveDef, PowerDef};

pub use exec::{ComposeMemo, Forced};
pub use grammar::{Names, Reading, read, read_with};

/// Most modifier bytes a spec carries.
pub const MAX_MODS: usize = 4;
/// Largest count ("five times").
pub const MAX_COUNT: u8 = 5;
/// Longest duration a spec can say, in tenths of a second (each verb clamps further).
pub const MAX_TENTHS: u8 = 120;
/// Flag bit of a composed move's cooldown key (`chain::Toolbox::cds`); bits 8..24 are all ones so
/// `MoveRef::unpack(key - 1)` names power 0xFFFF, which never resolves (MAX_POWERS = 4000).
pub const KEY_FLAG: u32 = 0x8000_0000;

/// Where the move goes relative to its target (spatial words).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Place {
    #[default]
    None,
    Behind,
    Front,
    Above,
    Below,
    Beside,
    Around,
    Through,
    Toward,
    Away,
    Onto,
    Into,
    Up,
    Down,
    Forward,
}

/// Who / what the move is aimed at. The host resolves it when the move fires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    /// Nothing said: the crosshair.
    #[default]
    Aim,
    /// "me", "myself".
    Me,
    /// "him", "her", "them", "that guy": the last target you struck with a composed move (≤ a few
    /// seconds ago), else the enemy nearest the crosshair.
    Pronoun,
    /// "it", "that": your newest live construct, else the construct nearest the crosshair.
    It,
    /// A chain link after a targeted link ("teleport behind ethan, fire uppercut"): the same target.
    Same,
    /// "the nearest", "the closest": the living enemy nearest you.
    Nearest,
    /// "everyone", "all of them": the verb's area form.
    Everyone,
    /// A player by name ("ethan"). The host checks they exist and are alive.
    Player(PlayerId),
    /// "the dragon", "that wall of ice": the live construct whose name has this word (16-bit id
    /// hash of the word), else the construct nearest the crosshair.
    Thing(u16),
    /// Terrain ("the wall", "the tower", "the ground"): the crosshair point.
    Ground,
}

impl Target {
    /// Names someone / something (the host overrides the aim toward it).
    pub fn is_explicit(self) -> bool {
        matches!(self, Target::Pronoun | Target::It | Target::Same | Target::Nearest | Target::Player(_) | Target::Thing(_))
    }
}

/// The COMPOSED MOVE spec: compact, closed-set, quantised. ~16 bytes on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComposedMove {
    /// Verb entry id ([`id16`] of `verbs.ron` `id`).
    pub verb: u16,
    /// Style entry id (0 = none).
    pub style: u16,
    pub place: Place,
    pub target: Target,
    /// 1..=[`MAX_COUNT`] ("twice", "3 times").
    pub count: u8,
    /// Duration in tenths of a second (0 = the verb's default).
    pub secs: u8,
    /// Modifier ids (MODIFIERS & FUSION's table; 0 = none), sorted, unique.
    pub mods: [u8; MAX_MODS],
}

/// The closed set of things a verb can be (what each does: [`build`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerbKind {
    // movement
    Teleport,
    Dash,
    Charge,
    Leap,
    Fly,
    Dodge,
    Phase,
    Swap,
    Rewind,
    Vanish,
    Grow,
    Shrink,
    // strikes
    Punch,
    Kick,
    Uppercut,
    Slam,
    Stomp,
    Smash,
    Slash,
    Flurry,
    Spin,
    Grab,
    // force
    Pull,
    Push,
    Lift,
    Crush,
    Explode,
    // afflict (the verb carries the element)
    Freeze,
    Burn,
    Shock,
    Poison,
    Blind,
    Stun,
    // defense / self
    Guard,
    Counter,
    Reflect,
    Absorb,
    Copy,
    Clone,
    Heal,
    Harden,
    TimeStop,
}

/// Extra behaviour a style lends (beyond its element and look).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rider {
    #[default]
    None,
    /// Earth / metal: harder, slower hits; slams leave bigger craters.
    Heavy,
    /// Gravity: strikes pin the victim down; arrivals pull things in.
    Gravity,
    /// Smoke: arrivals leave a blinding puff.
    Blind,
    /// Speed: farther moves, faster cooldowns, strikes become a quick flurry.
    Speed,
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize)]
pub struct VerbEntry {
    pub id: String,
    pub kind: VerbKind,
    pub words: Vec<String>,
    /// Words of `words` that are also common THINGS: bare, they stay constructs.
    #[serde(default)]
    pub noun_words: Vec<String>,
    /// × damage and knockback.
    #[serde(default = "one")]
    pub power: f32,
    /// × range / distance.
    #[serde(default = "one")]
    pub reach: f32,
    /// Cooldown override (s); 0 = the kind's.
    #[serde(default)]
    pub cd: f32,
    /// Presentation style key override ("" = the style's, else the kind's).
    #[serde(default)]
    pub fx: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StyleEntry {
    pub id: String,
    pub element: Element,
    pub fx: String,
    pub words: Vec<String>,
    #[serde(default)]
    pub rider: Rider,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlaceEntry {
    pub place: Place,
    pub words: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModEntry {
    pub id: u8,
    pub name: String,
    pub words: Vec<String>,
}

/// `assets/verbs.ron`.
#[derive(Clone, Debug, Deserialize)]
pub struct VerbTable {
    pub verbs: Vec<VerbEntry>,
    pub styles: Vec<StyleEntry>,
    pub places: Vec<PlaceEntry>,
    pub me: Vec<String>,
    pub pronouns: Vec<String>,
    pub it_words: Vec<String>,
    pub everyone: Vec<String>,
    pub nearest: Vec<String>,
    pub terrain: Vec<String>,
    pub filler: Vec<String>,
    pub counts: Vec<(String, u8)>,
    pub numbers: Vec<(String, f32)>,
    pub seconds: Vec<String>,
    pub times: Vec<String>,
    pub duration_lead: Vec<String>,
    pub respell: Vec<(String, String)>,
    #[serde(default)]
    pub modifiers: Vec<ModEntry>,
    #[serde(skip)]
    verb_ix: HashMap<u16, usize>,
    #[serde(skip)]
    style_ix: HashMap<u16, usize>,
    /// Load problems (id collisions), for the validator / tests.
    #[serde(skip)]
    pub warnings: Vec<String>,
}

/// The shipped vocabulary source (compiled in: every peer reads the same table).
pub const VERBS_RON: &str = include_str!("../../../../../assets/verbs.ron");

/// 16-bit id of a table entry / word (FNV-1a folded; never 0).
pub fn id16(s: &str) -> u16 {
    let h = crate::rng::word_hash(s);
    let v = (h ^ (h >> 16) ^ (h >> 32) ^ (h >> 48)) as u16;
    if v == 0 { 1 } else { v }
}

impl VerbTable {
    /// Parse a table (tests / the validator can feed their own).
    pub fn parse(src: &str) -> Result<VerbTable, String> {
        let mut t: VerbTable = ron::from_str(src).map_err(|e| e.to_string())?;
        let norm = |v: &mut Vec<String>| {
            for w in v.iter_mut() {
                *w = super::norm_words(w);
            }
            v.retain(|w| !w.is_empty());
        };
        for v in &mut t.verbs {
            norm(&mut v.words);
            norm(&mut v.noun_words);
            v.power = if v.power.is_finite() { v.power.clamp(0.3, 2.0) } else { 1.0 };
            v.reach = if v.reach.is_finite() { v.reach.clamp(0.3, 2.0) } else { 1.0 };
            v.cd = if v.cd.is_finite() { v.cd.clamp(0.0, 30.0) } else { 0.0 };
        }
        for s in &mut t.styles {
            norm(&mut s.words);
        }
        for p in &mut t.places {
            norm(&mut p.words);
        }
        for m in &mut t.modifiers {
            norm(&mut m.words);
        }
        for l in [&mut t.me, &mut t.pronouns, &mut t.it_words, &mut t.everyone, &mut t.nearest, &mut t.terrain, &mut t.filler, &mut t.seconds, &mut t.times, &mut t.duration_lead] {
            norm(l);
        }
        for (w, _) in &mut t.counts {
            *w = super::norm_words(w);
        }
        for (w, _) in &mut t.numbers {
            *w = super::norm_words(w);
        }
        for (a, b) in &mut t.respell {
            *a = super::norm_words(a);
            *b = super::norm_words(b);
        }
        for (k, v) in t.verbs.iter().enumerate() {
            let id = id16(&v.id);
            if let Some(o) = t.verb_ix.insert(id, k) {
                t.warnings.push(format!("verb id {:?} collides with {:?} (rename one)", v.id, t.verbs[o].id));
            }
        }
        for (k, s) in t.styles.iter().enumerate() {
            let id = id16(&s.id);
            if let Some(o) = t.style_ix.insert(id, k) {
                t.warnings.push(format!("style id {:?} collides with {:?} (rename one)", s.id, t.styles[o].id));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for m in &t.modifiers {
            if m.id == 0 || !seen.insert(m.id) {
                t.warnings.push(format!("modifier id {} of {:?} is 0 or reused", m.id, m.name));
            }
        }
        Ok(t)
    }

    /// The shipped table (a parse failure = an empty table + a stderr note: composed moves are
    /// simply off, nothing crashes; `cargo test -p bd_sim --test compose` goes red).
    pub fn get() -> &'static VerbTable {
        static T: OnceLock<VerbTable> = OnceLock::new();
        T.get_or_init(|| match VerbTable::parse(VERBS_RON) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("verbs.ron: {e} (composed moves off)");
                VerbTable::empty()
            }
        })
    }

    fn empty() -> VerbTable {
        VerbTable {
            verbs: vec![],
            styles: vec![],
            places: vec![],
            me: vec![],
            pronouns: vec![],
            it_words: vec![],
            everyone: vec![],
            nearest: vec![],
            terrain: vec![],
            filler: vec![],
            counts: vec![],
            numbers: vec![],
            seconds: vec![],
            times: vec![],
            duration_lead: vec![],
            respell: vec![],
            modifiers: vec![],
            verb_ix: HashMap::new(),
            style_ix: HashMap::new(),
            warnings: vec![],
        }
    }

    pub fn verb(&self, id: u16) -> Option<&VerbEntry> {
        self.verb_ix.get(&id).and_then(|k| self.verbs.get(*k))
    }

    pub fn style(&self, id: u16) -> Option<&StyleEntry> {
        if id == 0 {
            return None;
        }
        self.style_ix.get(&id).and_then(|k| self.styles.get(*k))
    }

    pub fn modifier(&self, id: u8) -> Option<&ModEntry> {
        if id == 0 {
            return None;
        }
        self.modifiers.iter().find(|m| m.id == id)
    }
}

impl ComposedMove {
    /// A spec for verb `verb_id` (a verbs.ron `id`).
    pub fn of(verb_id: &str) -> ComposedMove {
        ComposedMove { verb: id16(verb_id), count: 1, ..Default::default() }
    }

    /// Host validation: unknown verb → `None`; unknown style / modifiers dropped; count and
    /// duration clamped; modifiers sorted + unique. Idempotent.
    pub fn sanitized(self) -> Option<ComposedMove> {
        self.sanitized_in(VerbTable::get())
    }

    pub fn sanitized_in(mut self, t: &VerbTable) -> Option<ComposedMove> {
        t.verb(self.verb)?;
        if t.style(self.style).is_none() {
            self.style = 0;
        }
        self.count = self.count.clamp(1, MAX_COUNT);
        self.secs = self.secs.min(MAX_TENTHS);
        let mut m: Vec<u8> = self.mods.iter().copied().filter(|id| t.modifier(*id).is_some()).collect();
        m.sort_unstable();
        m.dedup();
        self.mods = [0; MAX_MODS];
        for (k, id) in m.into_iter().take(MAX_MODS).enumerate() {
            self.mods[k] = id;
        }
        if let Target::Thing(0) = self.target {
            self.target = Target::It;
        }
        Some(self)
    }

    /// The verb's kind (unknown verb → `None`).
    pub fn kind(&self) -> Option<VerbKind> {
        VerbTable::get().verb(self.verb).map(|v| v.kind)
    }

    /// Readable canonical name ("shadow teleport behind"), the expanded move's `name`.
    pub fn name(&self) -> String {
        let t = VerbTable::get();
        let mut s = String::new();
        for m in self.mods.iter().filter_map(|id| t.modifier(*id)) {
            s.push_str(&m.name);
            s.push(' ');
        }
        if let Some(st) = t.style(self.style) {
            s.push_str(&st.id);
            s.push(' ');
        }
        s.push_str(t.verb(self.verb).map_or("move", |v| v.id.as_str()));
        if self.place != Place::None {
            s.push(' ');
            s.push_str(&format!("{:?}", self.place).to_lowercase());
        }
        s
    }

    /// The mechanical identity for cooldown + staleness: verb + place + area form. Style, target,
    /// count and wording don't change it, so re-wording a move never dodges its cooldown or its
    /// staleness price ("fire teleport behind him" and "shadow teleport behind ethan" share one).
    pub fn cd_hash(&self) -> u16 {
        let s = format!("{}:{:?}:{}", self.verb, self.place, self.target == Target::Everyone);
        id16(&s)
    }

    /// Cooldown-table key ([`KEY_FLAG`] layout; see the const docs).
    pub fn cd_key(&self) -> u32 {
        let h = self.cd_hash() as u32;
        KEY_FLAG | (0xFFFF << 8) | (h & 0xFF) | (((h >> 8) & 0x7F) << 24)
    }

    /// Is a packed cooldown / queue key a composed one?
    pub fn is_key(k: u32) -> bool {
        k & KEY_FLAG != 0 && (k >> 8) & 0xFFFF == 0xFFFF
    }

    /// The move this spec expands to ([`build::build`]); `None` = unknown verb.
    pub fn expand(&self) -> Option<MoveDef> {
        build::build(self)
    }
}

/// PRECEDENCE helper (the console + the validator): does the lexicon know this exact phrase as
/// something CURATED — a signature ("kamehameha"), a grant (a power / tier name, a stand rush), or
/// an icon whose name IS the whole phrase ("iron man")? Then it stays the lexicon's, even if it
/// also reads as a described move. (An icon matched on one word of it — "teleport behind the
/// dragon" finds the dragon icon — does not count.)
pub fn curated_by_lexicon(phrase: &str, signature: bool, grant: bool, model: &bd_core::mechanics::ModelSource) -> bool {
    if signature || grant {
        return true;
    }
    let bd_core::mechanics::ModelSource::Icon(id) = model else { return false };
    let strip = |s: &str| {
        let n = super::norm_words(&s.replace('_', " "));
        let w: Vec<&str> = n.split(' ').filter(|w| !matches!(*w, "a" | "an" | "the")).collect();
        w.join(" ")
    };
    strip(id) == strip(phrase)
}

/// The stand-in power every composed move runs under (no kind rules, no moves, nothing to hold).
pub fn composed_power() -> &'static PowerDef {
    static D: OnceLock<PowerDef> = OnceLock::new();
    D.get_or_init(|| PowerDef {
        id: "composed".into(),
        name: "composed".into(),
        kind: super::PowerKind::Kit,
        names: vec![],
        claim: vec![],
        body: Default::default(),
        graft: None,
        incantations: vec![],
        delivery: Default::default(),
        shouts: vec![],
        moves: vec![],
        transform: None,
        companion: None,
        pickup: None,
        effects: vec![],
        ascend: None,
        tiers: vec![],
        fx: String::new(),
    })
}

/// Presentation: the expanded move of `spec` as a `&'static` (interned; the client presenters take
/// `&'static MoveDef`). Bounded: past [`MAX_INTERNED`] distinct specs, `None` (present nothing new).
pub fn interned(spec: &ComposedMove) -> Option<&'static MoveDef> {
    use std::sync::Mutex;
    static CACHE: OnceLock<Mutex<HashMap<ComposedMove, &'static MoveDef>>> = OnceLock::new();
    let spec = spec.sanitized()?;
    let m = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut g = m.lock().ok()?;
    if let Some(d) = g.get(&spec) {
        return Some(d);
    }
    if g.len() >= MAX_INTERNED {
        return None;
    }
    let d: &'static MoveDef = Box::leak(Box::new(spec.expand()?));
    g.insert(spec, d);
    Some(d)
}

/// Distinct composed moves one process presents (a few hundred in a long session).
pub const MAX_INTERNED: usize = 4096;
