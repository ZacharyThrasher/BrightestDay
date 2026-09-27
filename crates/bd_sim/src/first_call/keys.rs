//! FIRST CALL keys: WHAT a use is, as one stable 64-bit identity ([`CallKey`]).
//!
//! One index ([`Identities::shipped`], built once from compiled-in data, identical on every peer)
//! joins every name the data gives a thing:
//! 1. **powers** (every kind, kits included): the display name + every alias → the entry's identity;
//!    two entries with the same display name ("Juggernaut" twice) are one identity;
//! 2. **costume characters** (`assets/costumes/characters.ron`): joined to a power / kit whose display
//!    name (then alias) is one of the character's names — so the bare-name costume summon of Optimus
//!    Prime and the `kit_optimus_prime` kit are ONE key;
//! 3. **named icons** (`assets/first_call_icons.ron`, generated from the icon files minus generic
//!    vocabulary): joined by canonical name / display name, found again from a recipe by its icon id
//!    or its authored procedural seed;
//! 4. **shouts** (every move's shout → its power) and **signature names** (`assets/signatures.ron`),
//!    for named casts ("kamehameha" is Goku's if a Goku kit shouts it).
//!
//! Keys: `fnv("id:" + canonical name)` — stable across builds as long as the name stays. A generic
//! thing has no named key; [`thing_key`] / [`composed_key`] are the lenient BURN-everything keys.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use bd_core::mechanics::{ModelSource, Recipe};
use serde::{Deserialize, Serialize};

use crate::powers::toolbox::{MoveIndex, MoveRef};
use crate::powers::{PowerKind, PowerTable, norm_words, split_tier};

/// One identity.
pub type CallKey = u64;

/// What an identity is (the glyph shape in the Tab collection; never shown as text).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum CallKind {
    /// A generic construct (BURN everything).
    #[default]
    Thing = 0,
    /// A character: costume summon, kit, their moves.
    Character = 1,
    /// A power / technique / fruit / stand and its moves.
    Power = 2,
    /// A described move (BURN everything).
    Move = 3,
    /// A named icon (a famous weapon, vehicle, creature).
    Icon = 4,
    /// A named signature ultimate.
    Signature = 5,
}

/// An identity and its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ident {
    pub key: CallKey,
    pub kind: CallKind,
}

/// FNV-1a 64 (stable across platforms / runs).
pub fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The key of a canonical name.
pub fn name_key(canon: &str) -> CallKey {
    let mut v = Vec::with_capacity(canon.len() + 3);
    v.extend_from_slice(b"id:");
    v.extend_from_slice(canon.as_bytes());
    fnv(&v).max(1)
}

/// BURN everything: a construct's lenient identity — its form + model ("a dragon" and "fire dragon"
/// share the head noun's seed; an icon its id).
pub fn thing_key(r: &Recipe) -> CallKey {
    let mut v: Vec<u8> = b"thing:".to_vec();
    v.push(r.form as u8);
    match &r.model {
        ModelSource::Procedural { seed } => v.extend_from_slice(&seed.to_le_bytes()),
        ModelSource::Icon(id) => v.extend_from_slice(id.as_bytes()),
    }
    fnv(&v).max(1)
}

/// BURN everything: a described move's lenient identity — its verb + style ("homing fire punch" and
/// "fire punch" are one punch).
pub fn composed_key(spec: &crate::powers::compose::ComposedMove) -> CallKey {
    let mut v: Vec<u8> = b"move:".to_vec();
    v.extend_from_slice(&spec.verb.to_le_bytes());
    v.extend_from_slice(&spec.style.to_le_bytes());
    fnv(&v).max(1)
}

#[derive(Deserialize)]
struct SigFile {
    #[serde(default)]
    names: Vec<SigName>,
}

#[derive(Deserialize)]
struct SigName {
    name: String,
    #[serde(default)]
    title: Option<String>,
}

/// The joined name index.
#[derive(Debug, Default)]
pub struct Identities {
    /// Power id → identity (`None`: a name in the `never` list).
    powers: Vec<Option<Ident>>,
    /// Costume character index → identity.
    chars: Vec<Option<Ident>>,
    /// Named icon id → identity.
    icon_ids: HashMap<String, Ident>,
    /// Authored procedural seed of a named icon → identity (ambiguous seeds dropped).
    icon_seeds: HashMap<u32, Ident>,
    /// Signature names / titles and shouts → identity (named casts).
    named_casts: HashMap<String, Ident>,
    /// Every alias → identity (tests, tools).
    alias: HashMap<String, Ident>,
}

impl Identities {
    /// The index of the shipped data.
    pub fn shipped() -> &'static Identities {
        static IX: OnceLock<Identities> = OnceLock::new();
        IX.get_or_init(|| Identities::build(PowerTable::get(), MoveIndex::shipped(), super::CallData::shipped()))
    }

    pub fn build(t: &PowerTable, ix: &MoveIndex, data: &super::CallData) -> Identities {
        let never: HashSet<&str> = data.never.iter().map(|s| s.as_str()).collect();
        let ok = |n: &str| !n.is_empty() && !never.contains(n);
        let mut display: HashMap<String, Ident> = HashMap::new();
        let mut alias: HashMap<String, Ident> = HashMap::new();
        // 1. powers (kits are characters)
        let mut powers = Vec::with_capacity(t.powers.len());
        for p in &t.powers {
            let canon = norm_words(&p.name);
            if !ok(&canon) {
                powers.push(None);
                continue;
            }
            let kind = if p.kind == PowerKind::Kit { CallKind::Character } else { CallKind::Power };
            let id = display.get(&canon).or_else(|| alias.get(&canon)).copied().unwrap_or(Ident { key: name_key(&canon), kind });
            display.entry(canon).or_insert(id);
            for n in &p.names {
                let n = norm_words(n);
                if ok(&n) {
                    alias.entry(n).or_insert(id);
                }
            }
            powers.push(Some(id));
        }
        // 2. costume characters
        let table = bd_shapes::costume::look::table();
        let mut chars = Vec::with_capacity(table.chars.len());
        for c in &table.chars {
            let names: Vec<String> = c.names.iter().map(|n| norm_words(n)).filter(|n| ok(n)).collect();
            let Some(first) = names.first() else {
                chars.push(None);
                continue;
            };
            let id = names
                .iter()
                .find_map(|n| display.get(n))
                .or_else(|| names.iter().find_map(|n| alias.get(n)))
                .copied()
                .unwrap_or(Ident { key: name_key(first), kind: CallKind::Character });
            display.entry(first.clone()).or_insert(id);
            for n in &names {
                alias.entry(n.clone()).or_insert(id);
            }
            chars.push(Some(id));
        }
        // shouts → their power (a famous move's name is its character's / power's)
        let mut shouts: HashMap<String, Ident> = HashMap::new();
        for (shout, mr) in ix.shouts() {
            if let Some(Some(id)) = powers.get(mr.power as usize)
                && ok(shout)
            {
                shouts.entry(shout.to_string()).or_insert(*id);
            }
        }
        // 3. named icons
        let rows: Vec<(String, String, String, u32)> = match ron::from_str(super::ICONS_RON) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("first_call_icons.ron: {e} (no named icons this run)");
                vec![]
            }
        };
        let mut icon_ids = HashMap::new();
        let mut icon_seeds: HashMap<u32, Ident> = HashMap::new();
        let mut bad_seeds: HashSet<u32> = HashSet::new();
        let mut named_casts: HashMap<String, Ident> = HashMap::new();
        for (canon, disp, icon, seed) in &rows {
            let c = norm_words(canon);
            let d = norm_words(disp);
            if !ok(&c) {
                continue;
            }
            let id = display
                .get(&c)
                .or_else(|| alias.get(&c))
                .or_else(|| shouts.get(&c))
                .or_else(|| display.get(&d))
                .copied()
                .unwrap_or(Ident { key: name_key(&c), kind: CallKind::Icon });
            display.entry(c.clone()).or_insert(id);
            alias.entry(c.clone()).or_insert(id);
            if ok(&d) {
                named_casts.entry(d).or_insert(id);
            }
            named_casts.entry(c).or_insert(id);
            if !icon.is_empty() {
                icon_ids.entry(icon.clone()).or_insert(id);
            }
            if *seed != 0 {
                match icon_seeds.get(seed) {
                    Some(prev) if prev.key != id.key => {
                        bad_seeds.insert(*seed);
                    }
                    _ => {
                        icon_seeds.insert(*seed, id);
                    }
                }
            }
        }
        for s in bad_seeds {
            icon_seeds.remove(&s);
        }
        // 4. shouts, then signature names
        for (shout, id) in shouts {
            named_casts.entry(shout).or_insert(id);
        }
        if let Ok(f) = ron::from_str::<SigFile>(SIGNATURES_RON) {
            for s in f.names {
                let n = norm_words(&s.name);
                let canon = s.title.as_deref().map(norm_words).unwrap_or_else(|| n.clone());
                if !ok(&n) || !ok(&canon) {
                    continue;
                }
                let id = named_casts
                    .get(&canon)
                    .or_else(|| named_casts.get(&n))
                    .or_else(|| alias.get(&canon))
                    .copied()
                    .unwrap_or(Ident { key: name_key(&canon), kind: CallKind::Signature });
                named_casts.entry(canon).or_insert(id);
                named_casts.entry(n).or_insert(id);
            }
        }
        Identities { powers, chars, icon_ids, icon_seeds, named_casts, alias }
    }

    /// A power's identity (and every move of it).
    pub fn of_power(&self, power: u16) -> Option<Ident> {
        self.powers.get(power as usize).copied().flatten()
    }

    /// A move's identity: its power's (a character and all their moves share one key).
    pub fn of_move(&self, mv: MoveRef) -> Option<Ident> {
        self.of_power(mv.power)
    }

    /// Costume character index → identity.
    pub fn of_character(&self, ci: usize) -> Option<Ident> {
        self.chars.get(ci).copied().flatten()
    }

    /// A normalised name's identity (tests, tools).
    pub fn of_name(&self, n: &str) -> Option<Ident> {
        let n = norm_words(n);
        self.alias.get(&n).or_else(|| self.named_casts.get(&n)).copied()
    }

    /// A construct recipe's NAMED identity: a grant (fruit / technique), a costume summon's
    /// character, a named icon (by id or authored seed), or — for a signature cast — its name.
    /// `None` = a generic thing.
    pub fn of_recipe(&self, r: &Recipe, signature: bool) -> Option<Ident> {
        if let Some(g) = &r.grants
            && let Some(p) = PowerTable::get().index_of(split_tier(g).0)
        {
            return self.of_power(p);
        }
        let by_model = match &r.model {
            ModelSource::Icon(id) => match id.strip_prefix(bd_shapes::costume::summon::PREFIX) {
                Some(text) => bd_shapes::costume::grammar::parse(text).and_then(|p| p.base).and_then(|(ci, _)| self.of_character(ci)),
                None => self.icon_ids.get(id).copied(),
            },
            ModelSource::Procedural { seed } => self.icon_seeds.get(seed).copied(),
        };
        by_model.or_else(|| if signature { self.named_casts.get(&norm_words(&r.name)).copied() } else { None })
    }

    /// Counts (tests, tools): (powers with an identity, characters, icon ids, icon seeds, named casts).
    pub fn counts(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.powers.iter().flatten().count(),
            self.chars.iter().flatten().count(),
            self.icon_ids.len(),
            self.icon_seeds.len(),
            self.named_casts.len(),
        )
    }
}

/// The signature table (names only are read here; bd_lexicon owns the rest of it).
const SIGNATURES_RON: &str = include_str!("../../../../assets/signatures.ron");
