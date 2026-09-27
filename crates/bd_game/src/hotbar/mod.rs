//! POWER HOTBAR — the 1–4 quick-cast loadout made dynamic (owner: "when a user says something that's
//! a power, it gets added as a key bind … Reinhardt ult becomes an icon on your HUD you can press 4 to
//! activate whenever … drag and drop different powers to different slots on the fly while holding
//! tab"). ONE system: the old phrase loadout (`UiPrefs::loadout`, keys 1–4) IS the hotbar.
//!
//! * **Slots** hold the phrase AND its canonical reading ([`SlotBind`]: a move ref, a described
//!   move's spec, a twist, or "a phrase" for constructs / signatures). A key press sends exactly the
//!   `Cmd::Toolbox` saying the phrase sends ([`fire::step_for`]): same host path, same cooldowns, same
//!   spark price, same staleness — a key is never cheaper than your voice, and repeating one slot
//!   wears it exactly like repeating the shout (cycle!). Phrase slots cast through the ring as said.
//! * **AUTO-BIND** (`UiPrefs::auto_bind`, on by default): a move you SAID (borrowed shout, kit move,
//!   held move by name, described / composed move, twisted / fused move, a signature) that the HOST
//!   CONFIRMED fired ([`fire::intake`]) goes onto the bar: already there → refreshed; else the first
//!   empty slot; else the least-recently-used UNPINNED slot (ties: the rightmost, so "reinhardt
//!   ultimate" on a fresh loadout lands on 4). Plain constructs and whole chains never auto-bind.
//! * **Tab** (the scoreboard key): the cursor frees (keyboard play continues), names appear over
//!   the slots, a tray of recent moves + your held powers' moves shows beside the bar; drag to swap,
//!   drag off to clear, drag a tray chip onto a slot to bind it, right-click to pin (auto-bind never
//!   replaces a pinned slot). Keyboard: Tab + a slot key selects, a second slot key swaps (the same
//!   key again pins), Delete clears. Release Tab: back to play at once. ([`hud`])
//! * **State hook** ([`SlotState`]): FIRST CALL / BURN ON DEATH set `Slot::state` (fresh / burned /
//!   claimed-by-rival); the HUD renders it and a non-fresh slot fires through the full phrase
//!   reading so their rival / echo reading applies (docs/toolbox/HOTBAR.md).
//!
//! Persistence: the phrases stay in `UiPrefs::loadout` (+ `loadout_pins`), readings are re-derived
//! from the phrase when the lexicon is ready (a `MoveRef` is a table index: never persisted). Old
//! prefs (plain construct phrases, no pins) load unchanged. `NetProfile::loadout` carries the phrases.

pub mod dismiss;
pub mod fire;
pub mod hud;
pub mod scenario;

use bd_core::ConstructId;
use bd_sim::powers::compose::ComposedMove;
use bd_sim::powers::toolbox::MoveRef;
use bd_sim::powers::twist::{TwistBase, TwistStep};
use bd_sim::powers::{MoveDef, PowerDef};
use bevy::prelude::*;

/// Slots on the bar (keys 1–4).
pub const SLOTS: usize = 4;
/// Recent moves kept for the Tab tray.
pub const RECENT: usize = 6;
/// Longest stored phrase (the prefs / net profile limit).
pub const MAX_PHRASE: usize = 80;

/// What a slot fires.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SlotBind {
    #[default]
    Empty,
    /// A phrase cast through the ring as said (constructs, grants, signatures when `signature`).
    Phrase { signature: bool },
    /// A named move (borrowed / kit; a held power's move fires as the held move while you hold it).
    Move { mv: MoveRef, kit: bool },
    /// A described move (VERB GRAMMAR).
    Composed(ComposedMove),
    /// A twisted / fused move (MODIFIERS & FUSION).
    Twist(Box<TwistStep>),
}

impl SlotBind {
    pub fn is_empty(&self) -> bool {
        matches!(self, SlotBind::Empty)
    }

    /// A MOVE (auto-bind material), not a plain phrase.
    pub fn is_move(&self) -> bool {
        matches!(self, SlotBind::Move { .. } | SlotBind::Composed(_) | SlotBind::Twist(_))
    }

    /// The host's cooldown / staleness key of this slot (`ToolboxSnap::cds`), if it has one.
    pub fn cd_key(&self) -> Option<u32> {
        match self {
            SlotBind::Move { mv, .. } => Some(mv.pack() + 1),
            SlotBind::Composed(c) => Some(c.cd_key()),
            SlotBind::Twist(t) => match &t.a.base {
                TwistBase::Move { mv, .. } => Some(mv.pack() + 1),
                TwistBase::Composed(s) => Some(bd_sim::powers::twist::split_composed(*s).0.cd_key()),
                _ => None,
            },
            _ => None,
        }
    }

    /// The named move under it (a twist's first base), if any.
    pub fn move_ref(&self) -> Option<MoveRef> {
        match self {
            SlotBind::Move { mv, .. } => Some(*mv),
            SlotBind::Twist(t) => match &t.a.base {
                TwistBase::Move { mv, .. } => Some(*mv),
                _ => None,
            },
            _ => None,
        }
    }
}

/// FIRST CALL / BURN ON DEATH hook: how a slot's thing stands for you right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SlotState {
    #[default]
    Fresh,
    /// Used in a life that ended (BURN ON DEATH): fires the ashen echo; drawn ash-grey.
    Burned,
    /// Someone else called it first (FIRST CALL): fires your rival version; drawn with a split rim.
    Claimed,
}

/// One slot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Slot {
    pub phrase: String,
    pub bind: SlotBind,
    /// The reading was derived (a phrase loaded from prefs waits for the lexicon).
    pub read: bool,
    /// Auto-bind never replaces a pinned slot.
    pub pinned: bool,
    /// LRU stamp (0 = never used this session).
    pub used: u64,
    pub state: SlotState,
    /// Presentation: the move behind it (look, cost, name) and its power.
    pub def: Option<&'static MoveDef>,
    pub power: Option<&'static PowerDef>,
    /// DISMISS BY SLOT: the constructs this slot called that may still be live.
    pub live: Vec<ConstructId>,
    /// DISMISS BY SLOT: a power this slot granted (held while it is).
    pub granted: Option<u16>,
}

impl Slot {
    pub fn is_empty(&self) -> bool {
        self.bind.is_empty() && self.phrase.trim().is_empty()
    }

    fn set(&mut self, e: &Entry) {
        self.phrase = e.phrase.clone();
        self.bind = e.bind.clone();
        self.read = true;
        self.def = e.def;
        self.power = e.power;
        self.state = SlotState::Fresh;
        self.live.clear();
        self.granted = None;
    }

    fn entry(&self) -> Entry {
        Entry { phrase: self.phrase.clone(), bind: self.bind.clone(), def: self.def, power: self.power }
    }
}

/// A bindable thing (a recent move, a tray chip, a slot's contents).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub phrase: String,
    pub bind: SlotBind,
    pub def: Option<&'static MoveDef>,
    pub power: Option<&'static PowerDef>,
}

impl Entry {
    pub fn new(phrase: &str, bind: SlotBind) -> Self {
        let phrase: String = phrase.trim().chars().filter(|c| !c.is_control()).take(MAX_PHRASE).collect();
        Self { phrase, bind, def: None, power: None }
    }
}

/// Identity of a slot's thing: the same move said two ways ("reinhardt ult" / "earthshatter") is one.
pub fn identity(phrase: &str, bind: &SlotBind) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut eat = |b: u8| {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    };
    match bind {
        SlotBind::Empty => return 0,
        SlotBind::Move { mv, .. } => {
            eat(1);
            mv.pack().to_le_bytes().into_iter().for_each(&mut eat);
        }
        SlotBind::Composed(c) => {
            eat(2);
            format!("{c:?}").bytes().for_each(&mut eat);
        }
        SlotBind::Twist(t) => {
            eat(3);
            format!("{t:?}").bytes().for_each(&mut eat);
        }
        SlotBind::Phrase { .. } => {
            eat(4);
            norm(phrase).bytes().for_each(&mut eat);
        }
    }
    h
}

/// Phrase comparison key: lower case, letters / digits, single spaces.
pub fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for w in s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(w.chars().flat_map(|c| c.to_lowercase()));
    }
    out
}

/// What auto-bind did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    /// Already on the bar (refreshed).
    Already(usize),
    /// Placed in this slot (it was empty).
    Filled(usize),
    /// Replaced this slot's older thing (moved to the tray).
    Replaced(usize),
    /// Every slot is pinned: only the tray got it.
    Full,
}

impl Bound {
    pub fn slot(self) -> Option<usize> {
        match self {
            Bound::Already(i) | Bound::Filled(i) | Bound::Replaced(i) => Some(i),
            Bound::Full => None,
        }
    }
}

/// The hotbar (the runtime owner of the 1–4 loadout; `UiPrefs` mirrors it).
#[derive(Resource, Debug, Default)]
pub struct Hotbar {
    pub slots: [Slot; SLOTS],
    /// Recent moves (newest first), for the Tab tray.
    pub recent: Vec<Entry>,
    clock: u64,
    /// The bar changed: write it back to `UiPrefs` (phrases + pins).
    pub prefs_dirty: bool,
    /// Presentation pulses per slot (1 → 0): fired / bound / ready.
    pub flash: [f32; SLOTS],
    /// Presentation: a slot just took a new thing (bind flare, 1 → 0).
    pub bound: [f32; SLOTS],
    /// TOTAL LIMIT: a call was just refused at the cap (1 → 0): live slots + the limit pips pulse.
    pub refused: f32,
}

impl Hotbar {
    /// The bar as loaded from prefs (phrases unread until the lexicon is ready).
    pub fn from_prefs(loadout: &[String; SLOTS], pins: &[bool; SLOTS]) -> Self {
        let mut h = Hotbar::default();
        h.load(loadout, pins);
        h
    }

    /// (Re)load every slot whose phrase differs from the prefs (the Customize page edited it).
    pub fn load(&mut self, loadout: &[String; SLOTS], pins: &[bool; SLOTS]) {
        for (k, s) in self.slots.iter_mut().enumerate() {
            let p: String = loadout[k].trim().chars().filter(|c| !c.is_control()).take(MAX_PHRASE).collect();
            if s.phrase != p || !s.read && s.bind.is_empty() {
                *s = Slot { pinned: pins[k] && !p.is_empty(), bind: if p.is_empty() { SlotBind::Empty } else { SlotBind::Phrase { signature: false } }, read: p.is_empty(), phrase: p, ..Default::default() };
            } else {
                s.pinned = pins[k] && !s.is_empty();
            }
        }
    }

    /// The prefs mirror: (phrases, pins).
    pub fn to_prefs(&self) -> ([String; SLOTS], [bool; SLOTS]) {
        (std::array::from_fn(|k| self.slots[k].phrase.clone()), std::array::from_fn(|k| self.slots[k].pinned))
    }

    fn tick_clock(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// The slot already holding this thing.
    pub fn find(&self, phrase: &str, bind: &SlotBind) -> Option<usize> {
        let id = identity(phrase, bind);
        (id != 0).then(|| self.slots.iter().position(|s| identity(&s.phrase, &s.bind) == id)).flatten()
    }

    /// A slot was used (pressed, or its thing fired): most recently used now.
    pub fn touch(&mut self, i: usize) {
        if i < SLOTS {
            self.slots[i].used = self.tick_clock();
        }
    }

    /// Remember a fired move for the tray (newest first, deduplicated, capped).
    pub fn note_recent(&mut self, e: &Entry) {
        let id = identity(&e.phrase, &e.bind);
        if id == 0 {
            return;
        }
        self.recent.retain(|r| identity(&r.phrase, &r.bind) != id);
        self.recent.insert(0, e.clone());
        self.recent.truncate(RECENT);
    }

    /// The slot auto-bind would use for a NEW thing: the first empty one, else the least recently
    /// used unpinned one (ties → the rightmost), else none (all pinned).
    pub fn victim(&self) -> Option<usize> {
        if let Some(i) = self.slots.iter().position(|s| s.is_empty()) {
            return Some(i);
        }
        let mut best: Option<usize> = None;
        for (i, s) in self.slots.iter().enumerate() {
            if s.pinned {
                continue;
            }
            if best.is_none_or(|b| s.used <= self.slots[b].used) {
                best = Some(i);
            }
        }
        best
    }

    /// AUTO-BIND: a confirmed fired move goes onto the bar (see the module docs). Always noted for
    /// the tray.
    pub fn auto_bind(&mut self, e: &Entry) -> Bound {
        self.note_recent(e);
        if let Some(i) = self.find(&e.phrase, &e.bind) {
            self.touch(i);
            // a better presentation reading (the twisted def) refreshes the slot's look
            if e.def.is_some() {
                self.slots[i].def = e.def;
                self.slots[i].power = e.power;
            }
            return Bound::Already(i);
        }
        let Some(i) = self.victim() else { return Bound::Full };
        let was_empty = self.slots[i].is_empty();
        self.slots[i].set(e);
        self.slots[i].pinned = false;
        self.touch(i);
        self.prefs_dirty = true;
        self.bound[i] = 1.0;
        if was_empty { Bound::Filled(i) } else { Bound::Replaced(i) }
    }

    /// Drag slot `a` onto slot `b` (or keyboard select + select): the two swap (pins travel).
    pub fn swap(&mut self, a: usize, b: usize) {
        if a == b || a >= SLOTS || b >= SLOTS {
            return;
        }
        self.slots.swap(a, b);
        self.flash.swap(a, b);
        self.bound[b] = 1.0;
        self.prefs_dirty = true;
    }

    /// Drag a slot off the bar / Delete: empty it (its thing goes to the tray).
    pub fn clear(&mut self, i: usize) {
        if i >= SLOTS || self.slots[i].is_empty() {
            return;
        }
        let e = self.slots[i].entry();
        if e.bind.is_move() {
            self.note_recent(&e);
        }
        self.slots[i] = Slot { read: true, ..Default::default() };
        self.prefs_dirty = true;
    }

    /// Drop a tray chip on slot `i`. If that thing is already on the bar it MOVES (the two slots
    /// swap); otherwise it takes the slot (the old thing goes to the tray). A hand-placed thing is
    /// the most recently used.
    pub fn place(&mut self, i: usize, e: &Entry) {
        if i >= SLOTS {
            return;
        }
        if let Some(j) = self.find(&e.phrase, &e.bind) {
            self.swap(j, i);
            self.touch(i);
            return;
        }
        let old = self.slots[i].entry();
        if old.bind.is_move() {
            self.note_recent(&old);
        }
        let pinned = self.slots[i].pinned;
        self.slots[i].set(e);
        self.slots[i].pinned = pinned;
        self.touch(i);
        self.bound[i] = 1.0;
        self.prefs_dirty = true;
    }

    /// Right-click / the same key twice under Tab: pin or unpin (an empty slot can't be pinned).
    pub fn toggle_pin(&mut self, i: usize) {
        if i < SLOTS && !self.slots[i].is_empty() {
            self.slots[i].pinned = !self.slots[i].pinned;
            self.bound[i] = 0.6;
            self.prefs_dirty = true;
        }
    }

    /// Tray entries: recent moves not on the bar, then `extra` (held powers' moves), deduplicated,
    /// at most `max`.
    pub fn tray(&self, extra: &[Entry], max: usize) -> Vec<Entry> {
        let on_bar: Vec<u64> = self.slots.iter().map(|s| identity(&s.phrase, &s.bind)).collect();
        let mut out: Vec<Entry> = Vec::with_capacity(max);
        let mut seen: Vec<u64> = Vec::with_capacity(max);
        for e in self.recent.iter().chain(extra) {
            let id = identity(&e.phrase, &e.bind);
            if id == 0 || on_bar.contains(&id) || seen.contains(&id) {
                continue;
            }
            seen.push(id);
            out.push(e.clone());
            if out.len() >= max {
                break;
            }
        }
        out
    }

    pub fn tick(&mut self, dt: f32) {
        for f in self.flash.iter_mut().chain(self.bound.iter_mut()) {
            *f = (*f - dt * 2.5).max(0.0);
        }
        self.refused = (self.refused - dt * 0.8).max(0.0);
    }
}

/// Keep `UiPrefs` (loadout + pins) and the bar in step both ways: the Customize page edits the
/// phrases, the bar writes back what it bound / moved / pinned.
fn sync_prefs(mut bar: ResMut<Hotbar>, mut prefs: ResMut<crate::lobby::prefs::UiPrefs>) {
    if bar.prefs_dirty {
        bar.prefs_dirty = false;
        let (l, p) = bar.to_prefs();
        if prefs.loadout != l || prefs.loadout_pins != p {
            prefs.loadout = l;
            prefs.loadout_pins = p;
        }
        return;
    }
    if prefs.is_changed() {
        let (l, p) = bar.to_prefs();
        if prefs.loadout != l || prefs.loadout_pins != p {
            let (loadout, pins) = (prefs.loadout.clone(), prefs.loadout_pins);
            bar.load(&loadout, &pins);
        }
    }
}

pub struct HotbarPlugin;

impl Plugin for HotbarPlugin {
    fn build(&self, app: &mut App) {
        let bar = {
            let prefs = app.world().get_resource::<crate::lobby::prefs::UiPrefs>();
            prefs.map(|p| Hotbar::from_prefs(&p.loadout, &p.loadout_pins)).unwrap_or_default()
        };
        app.insert_resource(bar).add_systems(Update, sync_prefs.after(crate::lobby::UiSet::Input).before(crate::lobby::UiSet::Draw));
        fire::build(app);
        hud::build(app);
        dismiss::build(app);
        scenario::build(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(n: u8) -> Entry {
        Entry::new(&format!("move {n}"), SlotBind::Move { mv: MoveRef::new(10 + n as u16, 0, 0), kit: true })
    }

    fn default_bar() -> Hotbar {
        let l = ["hammer".to_string(), "shield".into(), "wings".into(), "rocket launcher".into()];
        Hotbar::from_prefs(&l, &[false; 4])
    }

    #[test]
    fn auto_bind_lands_on_4_then_lru_and_refreshes_repeats() {
        let mut h = default_bar();
        // a fresh loadout: every slot unused → the rightmost goes first ("reinhardt ult" → 4)
        assert_eq!(h.auto_bind(&mv(1)), Bound::Replaced(3));
        assert_eq!(h.slots[3].phrase, "move 1");
        assert_eq!(h.auto_bind(&mv(2)), Bound::Replaced(2));
        // saying it again (any wording of the same move) refreshes, never duplicates
        let again = Entry::new("other words", mv(1).bind);
        assert_eq!(h.auto_bind(&again), Bound::Already(3));
        assert_eq!(h.slots.iter().filter(|s| s.bind == mv(1).bind).count(), 1);
        // pressing slot 1 makes it recent: the next new move replaces slot 2 (the LRU)
        h.touch(0);
        assert_eq!(h.auto_bind(&mv(3)), Bound::Replaced(1));
        // now LRU = move 2 (slot 3, older than move 1's refresh)
        assert_eq!(h.auto_bind(&mv(4)), Bound::Replaced(2));
        assert!(h.prefs_dirty);
    }

    #[test]
    fn empty_slots_fill_first_and_pins_are_never_replaced() {
        let l = ["hammer".to_string(), String::new(), "wings".into(), String::new()];
        let mut h = Hotbar::from_prefs(&l, &[true, false, false, false]);
        assert_eq!(h.auto_bind(&mv(1)), Bound::Filled(1));
        assert_eq!(h.auto_bind(&mv(2)), Bound::Filled(3));
        h.toggle_pin(1);
        h.toggle_pin(2);
        h.toggle_pin(3);
        assert!(h.slots.iter().all(|s| s.pinned));
        assert_eq!(h.auto_bind(&mv(3)), Bound::Full, "all pinned: nothing replaced");
        assert_eq!(h.recent[0], mv(3), "…but the tray has it");
        h.toggle_pin(2);
        assert_eq!(h.auto_bind(&mv(4)), Bound::Replaced(2), "the one unpinned slot");
        assert!(h.slots[0].pinned && h.slots[0].phrase == "hammer");
        // an empty slot can't be pinned
        h.clear(1);
        h.toggle_pin(1);
        assert!(!h.slots[1].pinned);
    }

    #[test]
    fn drag_drop_swap_clear_place() {
        let mut h = default_bar();
        h.auto_bind(&mv(1)); // → 4
        h.toggle_pin(3);
        // drag 4 onto 1: swap, the pin travels with the move
        h.swap(3, 0);
        assert_eq!(h.slots[0].phrase, "move 1");
        assert!(h.slots[0].pinned && !h.slots[3].pinned);
        assert_eq!(h.slots[3].phrase, "hammer");
        // drag 1 off the bar: cleared, the move goes back to the tray
        h.clear(0);
        assert!(h.slots[0].is_empty());
        assert_eq!(h.tray(&[], 8)[0], mv(1));
        // drop the tray chip on 3: it takes the slot; "wings" isn't a move (not in the tray)
        h.place(2, &mv(1));
        assert_eq!(h.slots[2].phrase, "move 1");
        assert!(h.tray(&[], 8).is_empty(), "on the bar: no longer in the tray");
        // drop the same thing on slot 2: it MOVES there (swap), never duplicates
        h.place(1, &mv(1));
        assert_eq!(h.slots[1].phrase, "move 1");
        assert_eq!(h.slots[2].phrase, "shield", "the two slots swapped");
        // tray: recent first, then held extras, no duplicates, capped
        h.note_recent(&mv(5));
        let t = h.tray(&[mv(5), mv(6), mv(7)], 2);
        assert_eq!(t, vec![mv(5), mv(6)]);
    }

    #[test]
    fn identity_folds_wording_for_moves_but_not_phrases() {
        let a = SlotBind::Move { mv: MoveRef::new(3, 0, 1), kit: true };
        assert_eq!(identity("reinhardt ult", &a), identity("earthshatter", &a));
        let p = SlotBind::Phrase { signature: false };
        assert_eq!(identity("A  Big Hammer!", &p), identity("a big hammer", &p));
        assert_ne!(identity("hammer", &p), identity("shield", &p));
        assert_eq!(identity("", &SlotBind::Empty), 0);
    }

    #[test]
    fn prefs_round_trip_and_customize_edits_reload() {
        let mut h = default_bar();
        h.auto_bind(&mv(1));
        h.toggle_pin(3);
        let (l, p) = h.to_prefs();
        assert_eq!(l[3], "move 1");
        assert_eq!(p, [false, false, false, true]);
        // reloading the same prefs keeps the in-session reading
        h.load(&l, &p);
        assert!(h.slots[3].bind.is_move());
        // the Customize page typed a new phrase into slot 2: that slot reloads (unread phrase)
        let mut l2 = l.clone();
        l2[1] = "giant sword".into();
        h.load(&l2, &p);
        assert_eq!(h.slots[1].phrase, "giant sword");
        assert!(!h.slots[1].read && h.slots[1].bind == SlotBind::Phrase { signature: false });
        assert!(h.slots[3].bind.is_move(), "untouched slots keep their reading");
    }
}
