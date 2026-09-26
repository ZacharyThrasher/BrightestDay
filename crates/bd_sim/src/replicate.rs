//! Replication types: compact, quantised snapshots and per-recipient deltas.
//!
//! * [`WorldSnapshot`] — the full continuous state one recipient needs: every player's compact
//!   render state ([`PlayerSnap`]), the recipient's own exact movement state ([`OwnState`], for
//!   prediction/reconciliation), constructs ([`ConstructSnap`]: transform + anim state; held/worn
//!   constructs carry no transform — clients attach them to their owner), projectile *segments*
//!   (clients simulate the ballistic arc), portals / fields / links and the match clock.
//! * [`SnapshotDelta`] — a snapshot encoded against a baseline the recipient acknowledged: only
//!   entities whose quantised bytes changed, plus removal lists. Static structures, idle players,
//!   projectile segments and match info cost nothing after the first send.
//! * [`DeltaEncoder`] (host, one per recipient) / [`DeltaDecoder`] (client).
//!
//! Events (hits, casts, carves…) go separately on the reliable lane (`crate::events::GameEvent`).
//!
//! Quantisation: positions 5 mm in i16 around [`Q_ORIGIN`] (±163 m), velocities 1 cm/s (±327 m/s),
//! rotations 4 × i8, look 12+12 bits, health / willpower / integrity u8. Decoding never panics:
//! every value decodes to something finite and clamped.

use std::collections::VecDeque;
use std::f32::consts::FRAC_PI_2;

use bd_core::{Quat, Tick, Vec3};
use serde::{Deserialize, Serialize};

use crate::movement::{Hook, MoveModifiers, PlayerState};

/// Positions are quantised relative to this point.
pub const Q_ORIGIN: Vec3 = Vec3::new(0.0, 64.0, 0.0);
const POS_SCALE: f32 = 200.0;
const VEL_SCALE: f32 = 100.0;

#[inline]
fn q16(v: f32, scale: f32) -> [u8; 2] {
    let x = if v.is_finite() { (v * scale).round().clamp(-32767.0, 32767.0) as i16 } else { 0 };
    x.to_le_bytes()
}

#[inline]
fn dq16(b: [u8; 2], scale: f32) -> f32 {
    i16::from_le_bytes(b) as f32 / scale
}

/// Position, 5 mm resolution, ±163 m around [`Q_ORIGIN`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QPos(pub [u8; 6]);

impl QPos {
    pub fn new(p: Vec3) -> Self {
        let r = p - Q_ORIGIN;
        let (a, b, c) = (q16(r.x, POS_SCALE), q16(r.y, POS_SCALE), q16(r.z, POS_SCALE));
        Self([a[0], a[1], b[0], b[1], c[0], c[1]])
    }
    pub fn get(&self) -> Vec3 {
        let b = self.0;
        Q_ORIGIN + Vec3::new(dq16([b[0], b[1]], POS_SCALE), dq16([b[2], b[3]], POS_SCALE), dq16([b[4], b[5]], POS_SCALE))
    }
}

/// Velocity, 1 cm/s resolution, ±327 m/s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QVel(pub [u8; 6]);

impl QVel {
    pub fn new(v: Vec3) -> Self {
        let (a, b, c) = (q16(v.x, VEL_SCALE), q16(v.y, VEL_SCALE), q16(v.z, VEL_SCALE));
        Self([a[0], a[1], b[0], b[1], c[0], c[1]])
    }
    pub fn get(&self) -> Vec3 {
        let b = self.0;
        Vec3::new(dq16([b[0], b[1]], VEL_SCALE), dq16([b[2], b[3]], VEL_SCALE), dq16([b[4], b[5]], VEL_SCALE))
    }
}

/// Rotation as 4 × i8 (≈ 1° error; rendering only).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QRot(pub [u8; 4]);

impl QRot {
    pub fn new(q: Quat) -> Self {
        let q = if q.is_finite() && q.length_squared() > 1e-6 { q.normalize() } else { Quat::IDENTITY };
        let q = if q.w < 0.0 { -q } else { q };
        let e = |v: f32| ((v * 127.0).round().clamp(-127.0, 127.0) as i8) as u8;
        Self([e(q.x), e(q.y), e(q.z), e(q.w)])
    }
    pub fn get(&self) -> Quat {
        let d = |b: u8| (b as i8) as f32 / 127.0;
        let q = Quat::from_xyzw(d(self.0[0]), d(self.0[1]), d(self.0[2]), d(self.0[3]));
        if q.length_squared() > 1e-6 { q.normalize() } else { Quat::IDENTITY }
    }
}

/// View yaw (12 bits, 0.09°) + pitch (12 bits).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QLook(pub [u8; 3]);

impl QLook {
    pub fn new(yaw: f32, pitch: f32) -> Self {
        let tau = std::f32::consts::TAU;
        let y = if yaw.is_finite() { ((yaw.rem_euclid(tau) / tau) * 4096.0).round() as u32 % 4096 } else { 0 };
        let p = if pitch.is_finite() { (((pitch.clamp(-FRAC_PI_2, FRAC_PI_2) / FRAC_PI_2) * 0.5 + 0.5) * 4095.0).round() as u32 } else { 2048 };
        let v = y | (p.min(4095) << 12);
        Self([v as u8, (v >> 8) as u8, (v >> 16) as u8])
    }
    pub fn get(&self) -> (f32, f32) {
        let v = self.0[0] as u32 | (self.0[1] as u32) << 8 | (self.0[2] as u32) << 16;
        let yaw = (v & 4095) as f32 / 4096.0 * std::f32::consts::TAU;
        let pitch = (((v >> 12) & 4095) as f32 / 4095.0 - 0.5) * 2.0 * FRAC_PI_2;
        (yaw, pitch)
    }
}

/// Unit vector as 3 × i8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QDir(pub [u8; 3]);

impl QDir {
    pub fn new(d: Vec3) -> Self {
        let d = d.normalize_or_zero();
        let e = |v: f32| ((v * 127.0).round().clamp(-127.0, 127.0) as i8) as u8;
        Self([e(d.x), e(d.y), e(d.z)])
    }
    pub fn get(&self) -> Vec3 {
        let d = |b: u8| (b as i8) as f32 / 127.0;
        let v = Vec3::new(d(self.0[0]), d(self.0[1]), d(self.0[2])).normalize_or_zero();
        if v == Vec3::ZERO { Vec3::Y } else { v }
    }
}

/// Bits of [`PlayerSnap::flags`].
pub mod pflags {
    pub const ALIVE: u16 = 1 << 0;
    pub const GROUNDED: u16 = 1 << 1;
    pub const CROUCHED: u16 = 1 << 2;
    pub const SLIDING: u16 = 1 << 3;
    pub const BOOSTING: u16 = 1 << 4;
    pub const TUMBLING: u16 = 1 << 5;
    pub const SHIELD: u16 = 1 << 6;
    pub const RECITING: u16 = 1 << 7;
    pub const TYPING: u16 = 1 << 8;
    pub const SEATED: u16 = 1 << 9;
    pub const SUIT: u16 = 1 << 10;
    pub const HOOK_L_ATTACHED: u16 = 1 << 11;
    pub const HOOK_R_ATTACHED: u16 = 1 << 12;
}

/// Compact render state of one player (sent for everyone, including the recipient).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerSnap {
    pub id: u8,
    /// [`pflags`], little-endian.
    pub flags: [u8; 2],
    pub pos: QPos,
    pub vel: QVel,
    pub look: QLook,
    pub health: u8,
    pub willpower: u8,
    /// `StatusSet::bits()`.
    pub status: u8,
    /// Body scale × 100 (Shrink / Grow).
    pub size: u8,
    /// Hook heads / anchors while flying or attached.
    pub hooks: [Option<QPos>; 2],
    /// Seated in (construct id, part).
    pub seat: Option<(u32, u8)>,
    /// POWERS HOOK: held power + its state flags (COMBAT.md §5h).
    #[serde(default)]
    pub power: crate::powers::PowerSnap,
}

impl PlayerSnap {
    pub fn flags(&self) -> u16 {
        u16::from_le_bytes(self.flags)
    }
    pub fn has(&self, f: u16) -> bool {
        self.flags() & f != 0
    }
    pub fn size_scale(&self) -> f32 {
        (self.size as f32 / 100.0).clamp(0.25, 2.5)
    }
}

/// The recipient's own movement state, exact (f32) so reconciliation replays from the host's
/// truth. Round-trips `PlayerState` bit-exactly.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OwnState {
    pub pos: Vec3,
    pub vel: Vec3,
    /// grounded, crouched, sliding, coyote_ok, boosting.
    pub bits: u8,
    /// `None` = straight up.
    pub ground_normal: Option<Vec3>,
    /// `None` = zero.
    pub ground_vel: Option<Vec3>,
    /// slide_cooldown, jump_buffer, air_time, gas, gas_regen_delay, burst_cooldown, tumble.
    pub timers: [f32; 7],
    pub prev_buttons: u16,
    /// `None` = idle (default).
    pub hooks: [Option<Hook>; 2],
    /// `None` = default modifiers.
    pub mods: Option<MoveModifiers>,
    /// Exact body scale (prediction must use the same hull).
    pub size: f32,
    pub portal_cd: f32,
}

impl OwnState {
    pub fn from_state(s: &PlayerState, size: f32, portal_cd: f32) -> Self {
        let bits = s.grounded as u8 | (s.crouched as u8) << 1 | (s.sliding as u8) << 2 | (s.coyote_ok as u8) << 3 | (s.boosting as u8) << 4;
        let idle = Hook::default();
        Self {
            pos: s.pos,
            vel: s.vel,
            bits,
            ground_normal: (s.ground_normal != Vec3::Y).then_some(s.ground_normal),
            ground_vel: (s.ground_vel != Vec3::ZERO).then_some(s.ground_vel),
            timers: [s.slide_cooldown, s.jump_buffer, s.air_time, s.gas, s.gas_regen_delay, s.burst_cooldown, s.tumble],
            prev_buttons: s.prev_buttons,
            hooks: [(s.hooks[0] != idle).then_some(s.hooks[0]), (s.hooks[1] != idle).then_some(s.hooks[1])],
            mods: (s.mods != MoveModifiers::default()).then_some(s.mods),
            size,
            portal_cd,
        }
    }

    /// Rebuild the movement state (non-finite values are sanitised, never propagated).
    pub fn to_state(&self) -> PlayerState {
        let fin = |v: Vec3, d: Vec3| if v.is_finite() { v } else { d };
        let f = |v: f32| if v.is_finite() { v } else { 0.0 };
        PlayerState {
            pos: fin(self.pos, Vec3::ZERO),
            vel: fin(self.vel, Vec3::ZERO),
            grounded: self.bits & 1 != 0,
            ground_normal: fin(self.ground_normal.unwrap_or(Vec3::Y), Vec3::Y),
            ground_vel: fin(self.ground_vel.unwrap_or(Vec3::ZERO), Vec3::ZERO),
            crouched: self.bits & 2 != 0,
            sliding: self.bits & 4 != 0,
            slide_cooldown: f(self.timers[0]),
            jump_buffer: f(self.timers[1]),
            air_time: f(self.timers[2]),
            coyote_ok: self.bits & 8 != 0,
            gas: f(self.timers[3]),
            gas_regen_delay: f(self.timers[4]),
            boosting: self.bits & 16 != 0,
            burst_cooldown: f(self.timers[5]),
            tumble: f(self.timers[6]),
            prev_buttons: self.prev_buttons,
            hooks: [self.hooks[0].unwrap_or_default(), self.hooks[1].unwrap_or_default()],
            mods: self.mods.unwrap_or_default(),
        }
    }
}

/// Bits of [`ConstructSnap::flags`].
pub mod cflags {
    pub const FORMING: u8 = 1 << 0;
    /// Held / worn / suit: no transform on the wire; attach to the owner.
    pub const ATTACHED: u8 = 1 << 1;
    pub const PILOTED: u8 = 1 << 2;
    pub const COLOSSAL: u8 = 1 << 3;
    pub const SUIT: u8 = 1 << 4;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstructSnap {
    pub id: u32,
    pub owner: u8,
    /// Zero when attached.
    pub pos: QPos,
    pub rot: QRot,
    /// `AnimState as u8`.
    pub anim: u8,
    /// Low 16 bits of the tick the anim state began (clients derive anim time).
    pub anim_since: u16,
    /// Integrity fraction × 255.
    pub integrity: u8,
    pub flags: u8,
    /// Shrink/Grow scale × 50.
    pub size: u8,
    /// Gun state: `WeaponPhase` (3 bits) | level (spin / charge / reload progress, 5 bits).
    pub weapon: u8,
}

/// One ballistic projectile segment (clients simulate `origin + v·t + ½·g·t²`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectileSnap {
    pub id: u16,
    pub owner: u8,
    pub kind: u8,
    pub origin: QPos,
    pub vel: QVel,
    /// Low 16 bits of the tick the segment started.
    pub since: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortalEndSnap {
    pub pos: QPos,
    pub normal: QDir,
    pub up: QDir,
    /// Radius × 20.
    pub radius: u8,
    pub hp: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortalSnap {
    pub id: u16,
    pub owner: u8,
    pub ends: [PortalEndSnap; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldSnap {
    pub id: u16,
    pub owner: u8,
    pub pos: QPos,
    /// Radius × 4.
    pub radius: u8,
    /// Strength (m/s², signed).
    pub strength: i8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinkEndSnap {
    Player(u8),
    Construct(u32),
    Point(QPos),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LinkSnap {
    pub id: u16,
    pub a: LinkEndSnap,
    pub b: LinkEndSnap,
    /// Length × 4.
    pub max_len: u8,
}

/// Match clock + scoreboard. Changes rarely (time left is whole seconds).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MatchSnap {
    /// `MatchPhase as u8`.
    pub phase: u8,
    pub round: u32,
    pub time_left: u16,
    pub winner: Option<u8>,
    /// (player, KOs, deaths).
    pub scores: Vec<(u8, u16, u16)>,
}

/// Full per-recipient snapshot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub tick: Tick,
    /// Last input sequence the host consumed for the recipient.
    pub ack: u32,
    pub match_info: MatchSnap,
    pub own: Option<OwnState>,
    /// Sorted by id.
    pub players: Vec<PlayerSnap>,
    /// Sorted by id.
    pub constructs: Vec<ConstructSnap>,
    /// Sorted by id.
    pub projectiles: Vec<ProjectileSnap>,
    pub portals: Vec<PortalSnap>,
    pub fields: Vec<FieldSnap>,
    pub links: Vec<LinkSnap>,
    /// Party mode state (`None` in FFA): phase, clock, standings, owned effects, own offer.
    #[serde(default)]
    pub party: Option<crate::party::PartySnap>,
}

/// A snapshot encoded against an acknowledged baseline.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SnapshotDelta {
    pub tick: Tick,
    /// Ticks between this snapshot and its baseline (0 = no baseline: a keyframe).
    pub baseline_ago: u16,
    pub ack: u32,
    pub match_info: Option<MatchSnap>,
    /// Always present when the recipient has a living player (it changes every tick).
    pub own: Option<OwnState>,
    pub players: Vec<PlayerSnap>,
    pub removed_players: Vec<u8>,
    pub constructs: Vec<ConstructSnap>,
    pub removed_constructs: Vec<u32>,
    pub projectiles: Vec<ProjectileSnap>,
    pub removed_projectiles: Vec<u16>,
    /// Whole lists, only when changed (they are tiny and rare).
    pub portals: Option<Vec<PortalSnap>>,
    pub fields: Option<Vec<FieldSnap>>,
    pub links: Option<Vec<LinkSnap>>,
    /// Party state when it changed (or on a keyframe). `Some(None)` = left party mode.
    #[serde(default)]
    pub party: Option<Option<crate::party::PartySnap>>,
}

/// Merge-walk two id-sorted lists: changed/new items of `cur`, and ids missing from `cur`.
fn diff_sorted<T: Copy + PartialEq, K: Ord + Copy>(base: &[T], cur: &[T], key: impl Fn(&T) -> K, changed: &mut Vec<T>, removed: &mut Vec<K>) {
    let (mut i, mut j) = (0, 0);
    while i < base.len() || j < cur.len() {
        match (base.get(i), cur.get(j)) {
            (Some(b), Some(c)) => {
                let (kb, kc) = (key(b), key(c));
                if kb == kc {
                    if b != c {
                        changed.push(*c);
                    }
                    i += 1;
                    j += 1;
                } else if kb < kc {
                    removed.push(kb);
                    i += 1;
                } else {
                    changed.push(*c);
                    j += 1;
                }
            }
            (Some(b), None) => {
                removed.push(key(b));
                i += 1;
            }
            (None, Some(c)) => {
                changed.push(*c);
                j += 1;
            }
            (None, None) => break,
        }
    }
}

/// Apply `changed` / `removed` onto an id-sorted `base`.
fn patch_sorted<T: Copy, K: Ord + Copy>(base: &[T], changed: &[T], removed: &[K], key: impl Fn(&T) -> K) -> Vec<T> {
    let mut out: Vec<T> = base.iter().copied().filter(|b| !removed.contains(&key(b))).collect();
    for c in changed {
        match out.binary_search_by_key(&key(c), &key) {
            Ok(i) => out[i] = *c,
            Err(i) => out.insert(i, *c),
        }
    }
    out
}

impl SnapshotDelta {
    /// Encode `cur` against `base` (None ⇒ keyframe containing everything).
    pub fn encode(base: Option<&WorldSnapshot>, cur: &WorldSnapshot) -> Self {
        let empty = WorldSnapshot::default();
        let (b, ago) = match base {
            Some(b) if b.tick < cur.tick && cur.tick - b.tick <= u16::MAX as Tick => (b, (cur.tick - b.tick) as u16),
            _ => (&empty, 0),
        };
        let mut d = SnapshotDelta { tick: cur.tick, baseline_ago: ago, ack: cur.ack, own: cur.own, ..Default::default() };
        if ago == 0 || b.match_info != cur.match_info {
            d.match_info = Some(cur.match_info.clone());
        }
        diff_sorted(&b.players, &cur.players, |p| p.id, &mut d.players, &mut d.removed_players);
        diff_sorted(&b.constructs, &cur.constructs, |c| c.id, &mut d.constructs, &mut d.removed_constructs);
        diff_sorted(&b.projectiles, &cur.projectiles, |p| p.id, &mut d.projectiles, &mut d.removed_projectiles);
        if ago == 0 || b.portals != cur.portals {
            d.portals = Some(cur.portals.clone());
        }
        if ago == 0 || b.fields != cur.fields {
            d.fields = Some(cur.fields.clone());
        }
        if ago == 0 || b.links != cur.links {
            d.links = Some(cur.links.clone());
        }
        if (ago == 0 && cur.party.is_some()) || (ago > 0 && b.party != cur.party) {
            d.party = Some(cur.party.clone());
        }
        d
    }

    /// Rebuild the full snapshot. Returns None if a baseline is required but `base` is missing or
    /// has the wrong tick.
    pub fn apply(&self, base: Option<&WorldSnapshot>) -> Option<WorldSnapshot> {
        let empty = WorldSnapshot::default();
        let b = if self.baseline_ago == 0 {
            &empty
        } else {
            let b = base?;
            if b.tick.checked_add(self.baseline_ago as Tick) != Some(self.tick) {
                return None;
            }
            b
        };
        Some(WorldSnapshot {
            tick: self.tick,
            ack: self.ack,
            match_info: self.match_info.clone().unwrap_or_else(|| b.match_info.clone()),
            own: self.own,
            players: patch_sorted(&b.players, &self.players, &self.removed_players, |p| p.id),
            constructs: patch_sorted(&b.constructs, &self.constructs, &self.removed_constructs, |c| c.id),
            projectiles: patch_sorted(&b.projectiles, &self.projectiles, &self.removed_projectiles, |p| p.id),
            portals: self.portals.clone().unwrap_or_else(|| b.portals.clone()),
            fields: self.fields.clone().unwrap_or_else(|| b.fields.clone()),
            links: self.links.clone().unwrap_or_else(|| b.links.clone()),
            party: self.party.clone().unwrap_or_else(|| b.party.clone()),
        })
    }
}

/// Host side, one per recipient: remembers what was sent, encodes against the newest snapshot the
/// client acknowledged.
#[derive(Default)]
pub struct DeltaEncoder {
    sent: VecDeque<WorldSnapshot>,
    acked: Option<Tick>,
    /// Snapshots kept for baselines (≈ 1 s at 60 Hz).
    pub capacity: usize,
}

impl DeltaEncoder {
    pub fn new() -> Self {
        Self { sent: VecDeque::new(), acked: None, capacity: 64 }
    }

    /// The client confirmed it decoded the snapshot for `tick`.
    pub fn ack(&mut self, tick: Tick) {
        if self.sent.iter().any(|s| s.tick == tick) && self.acked.is_none_or(|a| tick > a) {
            self.acked = Some(tick);
        }
    }

    pub fn encode(&mut self, cur: &WorldSnapshot) -> SnapshotDelta {
        let base = self.acked.and_then(|t| self.sent.iter().find(|s| s.tick == t));
        let d = SnapshotDelta::encode(base, cur);
        // Drop what can no longer be a baseline.
        if let Some(a) = self.acked {
            while self.sent.front().is_some_and(|s| s.tick < a) {
                self.sent.pop_front();
            }
        }
        self.sent.push_back(cur.clone());
        while self.sent.len() > self.capacity.max(2) {
            self.sent.pop_front();
        }
        d
    }

    /// Forget everything (client reconnected / requested a keyframe).
    pub fn reset(&mut self) {
        self.sent.clear();
        self.acked = None;
    }
}

/// Client side: keeps recent full snapshots to resolve baselines.
#[derive(Default)]
pub struct DeltaDecoder {
    received: VecDeque<WorldSnapshot>,
    pub capacity: usize,
}

impl DeltaDecoder {
    pub fn new() -> Self {
        Self { received: VecDeque::new(), capacity: 64 }
    }

    /// Decode; on success the caller should ack `result.tick` to the host. `None` ⇒ the baseline
    /// is unknown (ask for a keyframe).
    pub fn decode(&mut self, d: &SnapshotDelta) -> Option<WorldSnapshot> {
        let base = if d.baseline_ago == 0 {
            None
        } else {
            let t = d.tick.checked_sub(d.baseline_ago as Tick)?;
            Some(self.received.iter().find(|s| s.tick == t)?)
        };
        let full = d.apply(base)?;
        if self.received.back().is_none_or(|b| full.tick > b.tick) {
            self.received.push_back(full.clone());
            while self.received.len() > self.capacity.max(2) {
                self.received.pop_front();
            }
        }
        Some(full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantisers_round_trip() {
        let p = Vec3::new(-95.3, 12.25, 117.9);
        assert!((QPos::new(p).get() - p).length() < 0.01);
        let v = Vec3::new(55.5, -30.25, 0.01);
        assert!((QVel::new(v).get() - v).length() < 0.02);
        let q = Quat::from_euler(glam::EulerRot::YXZ, 1.2, -0.4, 0.1);
        assert!(QRot::new(q).get().angle_between(q) < 0.03);
        let (y, pi) = QLook::new(4.0, -0.7).get();
        assert!((y - 4.0).abs() < 0.01 && (pi + 0.7).abs() < 0.01);
        assert_eq!(QPos::new(Vec3::NAN).get(), Q_ORIGIN);
    }

    #[test]
    fn own_state_round_trips_exactly() {
        let mut s = PlayerState::new(Vec3::new(1.5, 2.25, -3.0), &crate::movement::MoveParams::default());
        s.vel = Vec3::new(10.1, -2.0, 0.3);
        s.tumble = 0.4;
        s.hooks[1].phase = crate::movement::HookPhase::Attached;
        s.hooks[1].pos = Vec3::new(4.0, 20.0, 1.0);
        s.mods.gravity_scale = 0.3;
        let o = OwnState::from_state(&s, 1.0, 0.0);
        assert_eq!(o.to_state(), s);
    }

    #[test]
    fn delta_round_trip() {
        let mut a = WorldSnapshot { tick: 10, ..Default::default() };
        for i in 0..5u8 {
            a.players.push(PlayerSnap { id: i, pos: QPos::new(Vec3::splat(i as f32)), ..Default::default() });
            a.constructs.push(ConstructSnap { id: i as u32 * 3, owner: i, ..Default::default() });
        }
        let mut b = a.clone();
        b.tick = 12;
        b.players[2].pos = QPos::new(Vec3::splat(9.0));
        b.constructs.remove(1);
        b.constructs.push(ConstructSnap { id: 100, owner: 1, ..Default::default() });
        let d = SnapshotDelta::encode(Some(&a), &b);
        assert_eq!(d.players.len(), 1);
        assert_eq!(d.constructs.len(), 1);
        assert_eq!(d.removed_constructs, vec![3]);
        assert_eq!(d.apply(Some(&a)).unwrap(), b);
        assert!(d.apply(None).is_none());
        let key = SnapshotDelta::encode(None, &b);
        assert_eq!(key.apply(None).unwrap(), b);
    }
}
