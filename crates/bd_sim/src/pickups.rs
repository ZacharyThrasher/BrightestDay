//! **POWER-UPS** (`MatchRules::powerups`, owner: "a mode that has certain mechanics behind powerups on
//! the map — a kaiju summon power up, an awakening, etc."). Off by default: FFA is bit-identical.
//!
//! With the mode on, a short list of *big* things ([`GateKind`], `tuning.pickups.gates`) needs a
//! pickup from the map:
//! * [`GateKind::Giant`] — kaiju / colossal / giant-height creatures (the giants tier);
//! * [`GateKind::Awakening`] — a power's transformation (Gear 5) and its top tiers
//!   (`awakening_min_tier`+);
//! * [`GateKind::Ultimate`] — signature ultimates (`ultimates` kinds: domains, beams), orbital strikes
//!   and big sky drops (meteors: `drop_scale`+).
//!
//! **Never say no**: without the pickup the phrase still makes something, through the downgrade paths
//! that already exist — a giant comes out person-sized (`giant_weak_scale`; the kaiju→creature steer of
//! the construct caps), a meteor fist-sized (the form minimum), a signature is cast as its plain phrase
//! (the signature's own can't-afford path), an awakening is the plain "power up" flare (no form).
//! With the pickup the same phrase is FULL size and the pickup is spent.
//!
//! **Pickups** ([`Pickup`]) are deterministic sim entities, host-authoritative, replicated as a whole
//! list in the snapshot when it changes (`WorldSnapshot::pickups`), moments as
//! `GameEvent::Pickup`. A shared timer (`every` s, first after `first_after` s) telegraphs a spawn
//! (`Telegraph`: a beacon pillar for `telegraph` s), then the pickup lies there (`Ground`,
//! `ground_life` s). Touch it to carry it (`Held`, one at a time, `held_life` s): the carrier glows in
//! its colour for everyone. KO'd → it drops where they last stood. Spent by the next gated cast.
//! Spots are derived on every map from its spawn points and centre ([`derive_spots`]: open ground
//! between spawns, away from them); `spots` in the tuning overrides them per map id.
//!
//! No text anywhere: each kind reads by look and sound (the client's `present/pickups.rs`).

use bd_core::{PlayerId, Tick, Vec3};
use serde::{Deserialize, Serialize};

use crate::World;
use crate::events::GameEvent;
use crate::signature::SignatureKind;

/// What a pickup unlocks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum GateKind {
    /// A thumping heart / egg: kaiju and other giants.
    #[default]
    Giant = 0,
    /// A swirling ascend orb: transformations and top tiers.
    Awakening = 1,
    /// A crackling star: signature ultimates, orbital strikes, meteors.
    Ultimate = 2,
}

impl GateKind {
    pub const ALL: [GateKind; 3] = [GateKind::Giant, GateKind::Awakening, GateKind::Ultimate];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PickupPhase {
    /// A beacon where it will appear (`until` = when it does).
    #[default]
    Telegraph = 0,
    /// Lying at `pos` (`until` = when it fades).
    Ground = 1,
    /// Carried by `holder` (`until` = when it fades).
    Held = 2,
}

/// One pickup. Doubles as its snapshot row (static between phase changes: the carrier's position
/// comes from the carrier).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pickup {
    pub id: u16,
    pub kind: GateKind,
    pub phase: PickupPhase,
    pub pos: Vec3,
    pub holder: Option<PlayerId>,
    pub until: Tick,
}

/// A pickup moment (`GameEvent::Pickup`) — for sound and bursts; state lives in the snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PickupMoment {
    Telegraph = 0,
    Appear = 1,
    Take = 2,
    Drop = 3,
    /// Spent on a gated cast (the FULL-size payoff).
    Spend = 4,
    Fade = 5,
}

/// Every number and the gated list (`assets/tuning/combat.ron` → `pickups`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PickupTuning {
    /// Seconds between spawns (the shared timer) and before the first one.
    pub every: f32,
    pub first_after: f32,
    /// Beacon seconds before a pickup appears.
    pub telegraph: f32,
    /// Seconds it lies on the ground / stays with its carrier before fading.
    pub ground_life: f32,
    pub held_life: f32,
    /// Most pickups on the map at once (telegraphed + lying + carried).
    pub max_live: u8,
    /// Touch distance (m) from the body centre.
    pub touch: f32,
    /// Spawn weights by kind (a kind at 0 never spawns).
    pub weights: Vec<(GateKind, u8)>,
    /// What the mode gates (the rest stays free).
    pub gates: Vec<GateKind>,
    /// Power tiers at or above this need an Awakening (a power's transform always does).
    pub awakening_min_tier: u8,
    /// Signature kinds that need an Ultimate.
    pub ultimates: Vec<SignatureKind>,
    /// Sky drops (DropFromSky objects) at or above this scale (m) need an Ultimate ("a meteor").
    pub drop_scale: f32,
    /// A creature at or above this height is a giant (the giants tier; kaiju / colossal always).
    pub giant_height: f32,
    /// A gated giant without the pickup comes out this tall (m): person-sized.
    pub giant_weak_scale: f32,
    /// Derived spots keep this far (m) from every player spawn, and this far from each other.
    pub spawn_clear: f32,
    pub spot_spacing: f32,
    /// Per-map spot overrides: (map id, points). The game names the map (`World::set_pickup_map`).
    pub spots: Vec<(String, Vec<[f32; 3]>)>,
}

impl Default for PickupTuning {
    fn default() -> Self {
        Self {
            every: 50.0,
            first_after: 20.0,
            telegraph: 5.0,
            ground_life: 40.0,
            held_life: 45.0,
            max_live: 2,
            touch: 1.6,
            weights: vec![(GateKind::Giant, 1), (GateKind::Awakening, 1), (GateKind::Ultimate, 1)],
            gates: vec![GateKind::Giant, GateKind::Awakening, GateKind::Ultimate],
            awakening_min_tier: 2,
            ultimates: vec![SignatureKind::Domain, SignatureKind::Beam],
            drop_scale: 4.0,
            giant_height: 8.0,
            giant_weak_scale: 1.8,
            spawn_clear: 10.0,
            spot_spacing: 6.0,
            spots: Vec::new(),
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl PickupTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.every = fin(self.every, d.every, 5.0, 600.0);
        self.first_after = fin(self.first_after, d.first_after, 0.0, 600.0);
        self.telegraph = fin(self.telegraph, d.telegraph, 0.0, 30.0);
        self.ground_life = fin(self.ground_life, d.ground_life, 1.0, 600.0);
        self.held_life = fin(self.held_life, d.held_life, 1.0, 600.0);
        self.max_live = self.max_live.clamp(1, MAX_PICKUPS as u8);
        self.touch = fin(self.touch, d.touch, 0.3, 6.0);
        self.weights.truncate(8);
        self.gates.truncate(8);
        self.ultimates.truncate(8);
        self.awakening_min_tier = self.awakening_min_tier.clamp(1, 15);
        self.drop_scale = fin(self.drop_scale, d.drop_scale, 0.5, 100.0);
        self.giant_height = fin(self.giant_height, d.giant_height, 2.0, 100.0);
        self.giant_weak_scale = fin(self.giant_weak_scale, d.giant_weak_scale, 0.5, 8.0);
        self.spawn_clear = fin(self.spawn_clear, d.spawn_clear, 0.0, 60.0);
        self.spot_spacing = fin(self.spot_spacing, d.spot_spacing, 1.0, 40.0);
        self.spots.truncate(64);
        for (_, pts) in &mut self.spots {
            pts.retain(|p| p.iter().all(|v| v.is_finite()));
            pts.truncate(32);
        }
        self
    }

    pub fn gates(&self, k: GateKind) -> bool {
        self.gates.contains(&k)
    }
}

/// Hard cap on live pickups.
pub const MAX_PICKUPS: usize = 8;
/// Most derived spots.
pub const MAX_SPOTS: usize = 16;

/// Per-world pickup state (`World::pickups`).
#[derive(Clone, Debug, Default)]
pub struct PickupWorld {
    /// Live pickups, sorted by id (the replicated list).
    pub items: Vec<Pickup>,
    /// Where the next pickups may appear (derived once per arena, or the map's override).
    pub spots: Vec<Vec3>,
    spots_ready: bool,
    /// The map id the game named (per-map spot overrides).
    map: Option<String>,
    next_at: Option<Tick>,
    serial: u16,
    /// Carrier's last solid footing, per carried pickup id (host only; not replicated).
    anchors: Vec<(u16, Vec3)>,
}

impl PickupWorld {
    /// New round / new arena: everything goes, spots are derived again.
    pub fn reset(&mut self) {
        self.items.clear();
        self.anchors.clear();
        self.spots.clear();
        self.spots_ready = false;
        self.next_at = None;
    }

    /// The pickup `player` carries.
    pub fn held_by(&self, player: PlayerId) -> Option<&Pickup> {
        self.items.iter().find(|p| p.phase == PickupPhase::Held && p.holder == Some(player))
    }
}

/// How a gate answers ([`gate`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// The mode is off / this thing isn't gated: as always.
    Free,
    /// Gated and the caster carries the pickup: FULL size; spend it on success ([`spend`]).
    Loaded,
    /// Gated, no pickup: the weaker version.
    Locked,
}

/// THE gate query: may `player` do a `kind` thing at full size right now?
pub fn gate(w: &World, player: PlayerId, kind: GateKind) -> Gate {
    if !w.cfg.rules.powerups || !w.cfg.tuning.pickups.gates(kind) {
        return Gate::Free;
    }
    match w.pickups.held_by(player) {
        Some(p) if p.kind == kind => Gate::Loaded,
        _ => Gate::Locked,
    }
}

/// Shorthand: `gate(..) != Locked`.
pub fn gate_allows(w: &World, player: PlayerId, kind: GateKind) -> bool {
    gate(w, player, kind) != Gate::Locked
}

/// Spend `player`'s `kind` pickup (after a gated cast succeeded).
pub fn spend(w: &mut World, player: PlayerId, kind: GateKind) {
    let Some(k) = w.pickups.items.iter().position(|p| p.phase == PickupPhase::Held && p.holder == Some(player) && p.kind == kind) else { return };
    let p = w.pickups.items.remove(k);
    w.pickups.anchors.retain(|(id, _)| *id != p.id);
    let pos = w.player(player).map_or(p.pos, |q| q.mv.pos);
    w.events.push(GameEvent::Pickup { id: p.id, kind, moment: PickupMoment::Spend, pos, player: Some(player) });
}

// ------------------------------------------------------------------------------ cast gates

/// Is this (validated) recipe giant-tier?
pub fn is_giant_recipe(r: &bd_core::mechanics::Recipe, t: &PickupTuning) -> bool {
    use bd_core::mechanics::{Form, FormCategory};
    r.form == Form::Kaiju || crate::willpower::is_colossal(r) || (r.form.category() == FormCategory::Creature && r.scale >= t.giant_height)
}

/// Is this recipe an ultimate by itself (outside a signature cast): an orbital strike or a meteor?
pub fn is_ultimate_recipe(r: &bd_core::mechanics::Recipe, t: &PickupTuning) -> bool {
    use bd_core::mechanics::{FormCategory, Mechanic};
    crate::signature::is_orbital(r) || (r.has(Mechanic::DropFromSky) && r.form.category() == FormCategory::Object && r.scale >= t.drop_scale)
}

/// POWER-UPS HOOK (`spawn_cast`, before paying): gate a fresh construct. A Locked giant comes out
/// person-sized (a kaiju becomes a creature), a Locked meteor / orbital fist-sized. Returns the
/// pickup to spend if the construct gets made.
pub(crate) fn gate_cast(w: &World, player: PlayerId, r: &mut bd_core::mechanics::Recipe) -> Option<GateKind> {
    use bd_core::mechanics::Form;
    if !w.cfg.rules.powerups {
        return None;
    }
    let t = &w.cfg.tuning.pickups;
    let mut spend_kind = None;
    if is_giant_recipe(r, t) {
        match gate(w, player, GateKind::Giant) {
            Gate::Free => {}
            Gate::Loaded if !w.free_cast => spend_kind = Some(GateKind::Giant),
            _ => {
                if r.form == Form::Kaiju {
                    r.form = Form::Biped;
                }
                let spec = crate::constructs::form_spec(r.form);
                let weak = if r.form.category() == bd_core::mechanics::FormCategory::Creature { t.giant_weak_scale } else { spec.default_scale };
                r.scale = r.scale.min(weak.max(spec.min_scale));
            }
        }
    }
    // A signature cast already passed its own gate (`cast_signature`).
    if w.signature.cast.is_none() && is_ultimate_recipe(r, t) {
        match gate(w, player, GateKind::Ultimate) {
            Gate::Free => {}
            Gate::Loaded if !w.free_cast => spend_kind = spend_kind.or(Some(GateKind::Ultimate)),
            _ => {
                let spec = crate::constructs::form_spec(r.form);
                r.scale = r.scale.min(spec.min_scale.max(0.3));
            }
        }
    }
    spend_kind
}

// ------------------------------------------------------------------------------ spots

/// Pickup spots for this arena: the map's override, else derived — the centre, the midpoints
/// between pairs of spawns and the halfway points from each spawn to the centre, dropped onto solid
/// ground with head room, at least `spawn_clear` from every spawn and `spot_spacing` apart.
/// Deterministic (depends on the terrain only).
pub fn derive_spots(w: &World) -> Vec<Vec3> {
    let t = &w.cfg.tuning.pickups;
    if let Some(map) = &w.pickups.map
        && let Some((_, pts)) = t.spots.iter().find(|(m, _)| m == map)
        && !pts.is_empty()
    {
        return pts.iter().map(|p| Vec3::from_array(*p)).collect();
    }
    let a = &w.arena;
    let mut cands: Vec<Vec3> = Vec::with_capacity(64);
    cands.push(a.center);
    for (i, s) in a.spawns.iter().enumerate() {
        cands.push(s.lerp(a.center, 0.5));
        for o in a.spawns.iter().skip(i + 1) {
            cands.push(s.lerp(*o, 0.5));
        }
    }
    let mut out: Vec<Vec3> = Vec::with_capacity(MAX_SPOTS);
    for c in cands {
        if out.len() >= MAX_SPOTS {
            break;
        }
        let Some(g) = ground_at(w, c) else { continue };
        if a.spawns.iter().any(|s| s.distance(g) < t.spawn_clear) || out.iter().any(|o| o.distance(g) < t.spot_spacing) {
            continue;
        }
        out.push(g);
    }
    if out.is_empty() {
        // Nothing qualified (a tiny map): the centre's ground, else the centre.
        out.push(ground_at(w, a.center).unwrap_or(a.center));
    }
    out
}

/// Solid ground under (or a little above) `p` with head room, inside the playable volume.
fn ground_at(w: &World, p: Vec3) -> Option<Vec3> {
    if !p.is_finite() {
        return None;
    }
    let top = p + Vec3::Y * 8.0;
    let hit = bd_core::voxel::raycast(&w.voxels, top, Vec3::NEG_Y, 40.0)?;
    let g = hit.point;
    let b = &w.arena.bounds;
    let inside = g.cmpge(b.min).all() && (g + Vec3::Y).cmple(b.max).all();
    if g.y < w.arena.void_y + 2.0 || !inside || hit.normal.y <= 0 {
        return None;
    }
    // Head room: nothing solid in the 2.5 m above.
    if bd_core::voxel::raycast(&w.voxels, g + Vec3::Y * 0.3, Vec3::Y, 2.5).is_some() {
        return None;
    }
    Some(g)
}

// ------------------------------------------------------------------------------ tick

fn secs(s: f32) -> Tick {
    (s.max(0.0) / bd_core::DT).round() as Tick
}

/// Deterministic pick in `0..n` for spawn `serial` (no draw from the world rng).
fn roll(seed: u64, serial: u16, salt: u64, n: u32) -> u32 {
    let mut x = seed ^ (serial as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt;
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x % n.max(1) as u64) as u32
}

impl World {
    /// POWER-UPS: name the map for per-map spot overrides (`tuning.pickups.spots`). Optional.
    pub fn set_pickup_map(&mut self, map_id: &str) {
        self.pickups.map = Some(map_id.to_string());
        self.pickups.spots_ready = false;
    }

    /// POWER-UPS HOOK (tick, after powers): the shared timer, telegraphs, touches, drops, fades.
    /// Nothing at all while the mode is off.
    pub(crate) fn step_pickups(&mut self) {
        if !self.cfg.rules.powerups {
            if !self.pickups.items.is_empty() {
                self.pickups.items.clear();
                self.pickups.anchors.clear();
            }
            return;
        }
        let t = self.cfg.tuning.pickups.clone();
        let now = self.tick;
        if !self.pickups.spots_ready {
            self.pickups.spots = derive_spots(self);
            self.pickups.spots_ready = true;
        }
        let next = *self.pickups.next_at.get_or_insert(now + secs(t.first_after));
        // The shared timer: telegraph the next one.
        if now >= next {
            self.pickups.next_at = Some(now + secs(t.every));
            let live = self.pickups.items.len();
            let total: u32 = t.weights.iter().map(|(_, w)| *w as u32).sum();
            if live < (t.max_live as usize).min(MAX_PICKUPS) && total > 0 && !self.pickups.spots.is_empty() {
                let serial = self.pickups.serial.wrapping_add(1).max(1);
                self.pickups.serial = serial;
                // Kind by weight; spot: a free one (not taken by another pickup).
                let mut r = roll(self.cfg.seed, serial, 0x6b69_6e64, total);
                let mut kind = t.weights[0].0;
                for (k, w) in &t.weights {
                    if r < *w as u32 {
                        kind = *k;
                        break;
                    }
                    r -= *w as u32;
                }
                let spots = &self.pickups.spots;
                let n = spots.len() as u32;
                let start = roll(self.cfg.seed, serial, 0x7370_6f74, n);
                let free = (0..n).map(|k| spots[((start + k) % n) as usize]).find(|s| !self.pickups.items.iter().any(|p| p.phase != PickupPhase::Held && p.pos.distance(*s) < 1.0));
                if let Some(pos) = free {
                    let id = serial;
                    self.pickups.items.push(Pickup { id, kind, phase: PickupPhase::Telegraph, pos, holder: None, until: now + secs(t.telegraph) });
                    self.pickups.items.sort_by_key(|p| p.id);
                    self.events.push(GameEvent::Pickup { id, kind, moment: PickupMoment::Telegraph, pos, player: None });
                }
            }
        }
        let base = self.cfg.move_params.clone();
        let mut k = 0;
        while k < self.pickups.items.len() {
            let p = self.pickups.items[k];
            match p.phase {
                PickupPhase::Telegraph => {
                    if now >= p.until {
                        let q = &mut self.pickups.items[k];
                        q.phase = PickupPhase::Ground;
                        q.until = now + secs(t.ground_life);
                        self.events.push(GameEvent::Pickup { id: p.id, kind: p.kind, moment: PickupMoment::Appear, pos: p.pos, player: None });
                    }
                }
                PickupPhase::Ground => {
                    if now >= p.until {
                        self.pickups.items.remove(k);
                        self.events.push(GameEvent::Pickup { id: p.id, kind: p.kind, moment: PickupMoment::Fade, pos: p.pos, player: None });
                        continue;
                    }
                    // First alive, empty-handed player (by id) touching it takes it.
                    let taker = self.players.iter().find(|q| {
                        q.alive && q.seat.is_none_or(|s| s.suit) && self.pickups.held_by(q.id).is_none() && q.hull(&base).center().distance(p.pos + Vec3::Y * 0.8) < t.touch + 0.6
                    });
                    if let Some(q) = taker {
                        let (who, at) = (q.id, q.mv.pos);
                        let it = &mut self.pickups.items[k];
                        it.phase = PickupPhase::Held;
                        it.holder = Some(who);
                        it.until = now + secs(t.held_life);
                        self.pickups.anchors.push((p.id, at));
                        self.events.push(GameEvent::Pickup { id: p.id, kind: p.kind, moment: PickupMoment::Take, pos: at, player: Some(who) });
                    }
                }
                PickupPhase::Held => {
                    let holder = p.holder.and_then(|h| self.pidx(h));
                    let anchor = self.pickups.anchors.iter().find(|(id, _)| *id == p.id).map(|a| a.1).unwrap_or(p.pos);
                    if now >= p.until {
                        self.pickups.items.remove(k);
                        self.pickups.anchors.retain(|(id, _)| *id != p.id);
                        let pos = holder.map_or(anchor, |h| self.players[h].mv.pos);
                        self.events.push(GameEvent::Pickup { id: p.id, kind: p.kind, moment: PickupMoment::Fade, pos, player: p.holder });
                        continue;
                    }
                    match holder {
                        Some(h) if self.players[h].alive => {
                            // Remember solid footing (where it drops on a KO).
                            if self.players[h].mv.grounded {
                                let at = self.players[h].mv.pos;
                                if let Some(a) = self.pickups.anchors.iter_mut().find(|(id, _)| *id == p.id) {
                                    a.1 = at;
                                }
                            }
                        }
                        _ => {
                            // KO'd (or gone): it drops where they last stood.
                            let it = &mut self.pickups.items[k];
                            it.phase = PickupPhase::Ground;
                            it.holder = None;
                            it.pos = anchor;
                            it.until = now + secs(t.ground_life);
                            self.pickups.anchors.retain(|(id, _)| *id != p.id);
                            self.events.push(GameEvent::Pickup { id: p.id, kind: p.kind, moment: PickupMoment::Drop, pos: anchor, player: p.holder });
                        }
                    }
                }
            }
            k += 1;
        }
    }
}
