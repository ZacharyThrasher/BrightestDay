//! **FIRST CALL** and **BURN ON DEATH** — two optional host match rules (both off by default; FFA is
//! bit-identical while they are off). Owner: "only the first person to say something can use it — if I
//! say rasengan, nobody else can; if I say Optimus Prime, he is mine for the match. Make sure it doesn't
//! feel bad for the person who is denied." and "a mode that doesn't let people re-use things after they
//! die".
//!
//! * **Keys** ([`keys`]): every use has an IDENTITY — one stable 64-bit [`CallKey`]. Named things only
//!   (famous moves, characters, powers / techniques / fruits, named icons and signatures): a
//!   character's costume summon, its kit and every one of its moves share ONE key ("optimus prime",
//!   "optimus prime combo" and "ion blaster" are all Optimus); a power and all its moves share its key.
//!   Generic things ("a dragon", "fire uppercut") have no named key. [`BurnScope::Everything`] adds a
//!   lenient key for them too: a construct's form + model seed ("a dragon" == "fire dragon"), a
//!   described move's verb + style.
//! * **FIRST CALL**: the first player to USE a named key owns it for the match (KOs never release it;
//!   leaving does). Same-tick ties go to the lower player id (the host decides; the loser's version
//!   turns into the rival version after the fact, [`CallEvent::Rival`]). Anyone else who calls it is
//!   NEVER refused and never pays more: they get the **RIVAL** version — the same move twisted with the
//!   wordless `rival` modifier and the modifier of its opposite element (data: `assets/first_call.ron`),
//!   a construct in the opposite element, a character summon in its evil-twin costume, and every
//!   construct / move the rival makes carries a mark the presentation darkens under the caller's own
//!   corps colour. A rival holding a claimed power fires its moves as rival moves.
//! * **BURN ON DEATH**: everything a player used in one life ([`FirstCallWorld::life`]) is burned for
//!   them when they are KO'd (never before). Calling a burned thing is never refused: it comes out as
//!   its **ECHO** — weaker, ashen, silent, the same cooldown and a spark cheaper (the `echo` modifier:
//!   ×0.6 damage, one tier down; constructs ×0.75 scale, −1 spark). Both at once = an ashen rival,
//!   priced as the echo.
//! * Deterministic, host-authoritative. The whole state replicates in [`CallSnap`] (only while a rule
//!   is on) so clients read owners / burns / marks for presentation and optimistic reading.
//! * Wordless: no text anywhere beyond the two lobby labels (`bd_game::present::first_call`).

pub mod gate;
pub mod keys;

use std::sync::OnceLock;

use bd_core::mechanics::Element;
use bd_core::{ConstructId, PlayerId, Tick, Vec3};
use serde::{Deserialize, Serialize};

pub use keys::{CallKey, CallKind, Ident};

/// BURN ON DEATH scope (the host's knob).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum BurnScope {
    /// Off (the default).
    #[default]
    Off = 0,
    /// Named things only (the FIRST CALL keys).
    Named = 1,
    /// Everything: named things plus constructs by their form / model and described moves by verb +
    /// style.
    Everything = 2,
}

impl BurnScope {
    pub fn on(self) -> bool {
        self != BurnScope::Off
    }
}

// ------------------------------------------------------------------------------------ data

/// `assets/first_call.ron` → rival rules.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RivalDef {
    /// Modifier ids (assets/move_modifiers.ron) on every rival move.
    pub mods: Vec<String>,
    /// Element → the modifier id that swaps a rival move to its opposite element.
    pub opposite: Vec<(Element, String)>,
    /// Costume grammar words dressing a rival character summon ("black").
    pub costume_words: String,
}

/// `assets/first_call.ron` → echo rules.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EchoDef {
    pub mods: Vec<String>,
    pub scale: f32,
    pub mass: f32,
    /// Sparks off an echo construct (never below 1).
    pub discount: u8,
}

impl Default for EchoDef {
    fn default() -> Self {
        Self { mods: vec![], scale: 0.75, mass: 0.6, discount: 1 }
    }
}

/// The whole data file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CallData {
    pub rival: RivalDef,
    pub echo: EchoDef,
    /// Normalised names that are never an identity.
    pub never: Vec<String>,
    pub max_claims: u16,
    pub max_burns: u16,
    pub max_life: u16,
}

impl Default for CallData {
    fn default() -> Self {
        Self { rival: RivalDef::default(), echo: EchoDef::default(), never: vec![], max_claims: 512, max_burns: 1024, max_life: 64 }
    }
}

/// The shipped data (compiled in: every peer substitutes identically).
pub const DATA_RON: &str = include_str!("../../../../assets/first_call.ron");
/// Named curated icons (generated: tools/first_call/first_call_icons.py).
pub const ICONS_RON: &str = include_str!("../../../../assets/first_call_icons.ron");

impl CallData {
    pub fn parse(src: &str) -> Result<CallData, String> {
        let mut d: CallData = ron::from_str(src).map_err(|e| e.to_string())?;
        d.never = d.never.iter().map(|w| crate::powers::norm_words(w)).filter(|w| !w.is_empty()).collect();
        d.echo.scale = if d.echo.scale.is_finite() { d.echo.scale.clamp(0.3, 1.0) } else { 0.75 };
        d.echo.mass = if d.echo.mass.is_finite() { d.echo.mass.clamp(0.2, 1.0) } else { 0.6 };
        d.max_claims = d.max_claims.clamp(16, 4096);
        d.max_burns = d.max_burns.clamp(16, 8192);
        d.max_life = d.max_life.clamp(8, 512);
        Ok(d)
    }

    /// The shipped data (a parse error → the defaults with a stderr note; `cargo test -p bd_sim --test
    /// first_call` goes red).
    pub fn shipped() -> &'static CallData {
        static D: OnceLock<CallData> = OnceLock::new();
        D.get_or_init(|| match CallData::parse(DATA_RON) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("first_call.ron: {e} (default first-call data this run)");
                CallData::default()
            }
        })
    }

    /// Wire bytes of the rival modifiers (+ the opposite-element one for `element`).
    pub fn rival_mods(&self, element: Element) -> Vec<u8> {
        let t = crate::powers::twist::ModTable::shipped();
        let mut v: Vec<u8> = self.rival.mods.iter().filter_map(|m| t.id_of(m)).collect();
        if let Some((_, m)) = self.rival.opposite.iter().find(|(e, _)| *e == element)
            && let Some(n) = t.id_of(m)
        {
            v.push(n);
        }
        v
    }

    /// The element a rival construct / move of `element` becomes.
    pub fn rival_element(&self, element: Element) -> Element {
        let t = crate::powers::twist::ModTable::shipped();
        self.rival
            .opposite
            .iter()
            .find(|(e, _)| *e == element)
            .and_then(|(_, m)| t.id_of(m))
            .and_then(|n| t.get(n))
            .and_then(|m| m.element)
            .unwrap_or(element)
    }

    /// Wire bytes of the echo modifiers.
    pub fn echo_mods(&self) -> Vec<u8> {
        let t = crate::powers::twist::ModTable::shipped();
        self.echo.mods.iter().filter_map(|m| t.id_of(m)).collect()
    }
}

// ------------------------------------------------------------------------------------ state

/// One owned key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClaimRow {
    pub key: CallKey,
    pub owner: PlayerId,
    pub kind: CallKind,
    /// Tick it was claimed (same-tick ties go to the lower player id).
    pub tick: Tick,
}

/// One burned (player, key).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BurnRow {
    pub key: CallKey,
    pub player: PlayerId,
    pub kind: CallKind,
}

/// Construct marks ([`MarkRow::mark`] bits).
pub mod mark {
    /// A rival version (the caller's corps colour over a darkened / negative palette).
    pub const RIVAL: u8 = 1 << 0;
    /// An echo (ashen, flickering).
    pub const ECHO: u8 = 1 << 1;
}

/// A construct made as a rival / echo version (presentation).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarkRow {
    pub construct: u32,
    pub mark: u8,
    pub key: CallKey,
}

/// The replicated state (whole, only while a rule is on; clients keep it in their replica).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallSnap {
    /// Sorted by key.
    pub claims: Vec<ClaimRow>,
    /// Sorted by (player, key).
    pub burns: Vec<BurnRow>,
    /// Sorted by construct id.
    pub marks: Vec<MarkRow>,
}

impl CallSnap {
    /// Who owns `key` (FIRST CALL).
    pub fn owner_of(&self, key: CallKey) -> Option<PlayerId> {
        self.claims.binary_search_by_key(&key, |c| c.key).ok().map(|i| self.claims[i].owner)
    }

    /// Is `key` burned for `player` (BURN ON DEATH)?
    pub fn burned(&self, player: PlayerId, key: CallKey) -> bool {
        self.burns.binary_search_by_key(&(player, key), |b| (b.player, b.key)).is_ok()
    }

    /// The mark of construct `id` (0 = none).
    pub fn mark_of(&self, id: ConstructId) -> u8 {
        self.marks.binary_search_by_key(&id.0, |m| m.construct).ok().map_or(0, |i| self.marks[i].mark)
    }

    /// `player`'s claims (Tab's collection).
    pub fn claims_of(&self, player: PlayerId) -> impl Iterator<Item = &ClaimRow> + '_ {
        self.claims.iter().filter(move |c| c.owner == player)
    }

    /// `player`'s burned keys (Tab's collection, ashen).
    pub fn burns_of(&self, player: PlayerId) -> impl Iterator<Item = &BurnRow> + '_ {
        self.burns.iter().filter(move |b| b.player == player)
    }

    /// How `player` calling `key` comes out ([`Verdict`]), as far as this replica knows (clients read
    /// it to present optimistically; the host's word arrives with the event).
    pub fn verdict(&self, player: PlayerId, key: CallKey, first_call: bool, burn: bool) -> Verdict {
        let rival = if first_call { self.owner_of(key).filter(|o| *o != player) } else { None };
        let echo = burn && self.burned(player, key);
        Verdict { rival, echo }
    }
}

/// How one use comes out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Verdict {
    /// Owned by this other player: the rival version.
    pub rival: Option<PlayerId>,
    /// Burned for the caller: the echo.
    pub echo: bool,
}

impl Verdict {
    pub fn plain(&self) -> bool {
        self.rival.is_none() && !self.echo
    }
}

/// Host + replica state (`World::first_call`). Empty while both rules are off.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FirstCallWorld {
    pub snap: CallSnap,
    /// Keys each player used in their current life (burned on KO): (player, key, kind).
    pub life: Vec<(PlayerId, CallKey, CallKind)>,
    /// Burned keys waiting for their owner's respawn beat: (player, count).
    pub pending_crumble: Vec<(PlayerId, u16)>,
    /// > 0 while a gated command runs (nested casts / moves are part of it, never gated again).
    pub(crate) depth: u8,
    /// Echo pricing of the construct cast in progress (sparks off).
    pub(crate) echo_discount: u8,
}

impl FirstCallWorld {
    pub fn reset(&mut self) {
        let depth = self.depth;
        *self = Self::default();
        self.depth = depth;
    }

    pub fn is_empty(&self) -> bool {
        self.snap.claims.is_empty()
            && self.snap.burns.is_empty()
            && self.snap.marks.is_empty()
            && self.life.is_empty()
            && self.pending_crumble.is_empty()
    }

    /// Hash (the per-tick invariant hash; nothing while empty, so FFA hashes never move).
    pub fn hash_into(&self, h: &mut crate::rng::Fnv64) {
        for c in &self.snap.claims {
            h.write(&c.key.to_le_bytes());
            h.write(&[c.owner.0]);
        }
        for b in &self.snap.burns {
            h.write(&b.key.to_le_bytes());
            h.write(&[b.player.0]);
        }
        for m in &self.snap.marks {
            h.write(&m.construct.to_le_bytes());
            h.write(&[m.mark]);
        }
        for (p, k, _) in &self.life {
            h.write(&[p.0]);
            h.write(&k.to_le_bytes());
        }
    }
}

// ------------------------------------------------------------------------------------ events

/// A FIRST CALL / BURN moment (`GameEvent::FirstCall`) — presentation only (state is in the snapshot).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CallEvent {
    /// `player` called `key` first: it is theirs for the match (their sigil on it from now on).
    Claimed { player: PlayerId, key: CallKey, kind: CallKind },
    /// `player` called `owner`'s `key`: the RIVAL version came out (both corps colours clash at `pos`,
    /// a sting; the owner sees their rival appear). Also sent when a same-tick tie went to `owner`.
    Rival { player: PlayerId, owner: PlayerId, key: CallKey, kind: CallKind, pos: Vec3 },
    /// `player` called a thing burned for them: its ECHO came out (ashen, lesser).
    Echo { player: PlayerId, key: CallKey, kind: CallKind },
    /// `player` called something fresh this life (BURN ON DEATH on): the subtle spark at the ring.
    Fresh { player: PlayerId, key: CallKey, kind: CallKind },
    /// `player` was KO'd: `n` things they used this life are burned now.
    Burned { player: PlayerId, n: u16 },
    /// `player` respawned with `n` newly burned tools: the ring sheds its ash.
    Crumble { player: PlayerId, n: u16 },
}
