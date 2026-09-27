//! Wire messages (inside bd_net's session envelope). Bump `bd_net::PROTOCOL_VERSION` if these
//! change incompatibly.
//!
//! Lanes: everything is reliable + ordered except [`S2C::Snapshot`] (latest-only lane).
//! Order on the reliable lane is load-bearing: `WorldInit → TerrainLog* → WorldReady → Events /
//! TerrainCheck…` and every tick's `Events` precede the snapshot of that tick.

use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::mechanics::Recipe;
use bd_core::shape::ShapeSpec;
use bd_core::{ConstructId, PlayerId, Tick};
use bd_net::InputPacket;
use bd_sim::settings::GameSettings;
use bd_sim::{Aim, GameEvent, SimConfig, SnapshotDelta};
use serde::{Deserialize, Serialize};

use crate::sim_bridge::Cmd;

/// Longest phrase / costume / loadout string accepted from a peer.
pub const MAX_TEXT: usize = 80;
/// Largest client-supplied construct shape (prims) the host accepts; bigger ones are dropped
/// (the host builds the shape from the recipe instead).
pub const MAX_SHAPE_PRIMS: usize = 512;

/// Costume (cosmetic; sent in the lobby and at join).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetCostume {
    pub phrase: String,
    pub seed: u32,
    pub accent: Option<[f32; 3]>,
    pub head_scale: f32,
    /// `bd_shapes::costume::acc` bits to hide.
    pub hide: u32,
}

impl Default for NetCostume {
    fn default() -> Self {
        Self { phrase: "knight".into(), seed: 0, accent: None, head_scale: 1.0, hide: 0 }
    }
}

/// What a player shows the lobby (their own choice; the host sanitises it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetProfile {
    pub costume: NetCostume,
    pub loadout: [String; 4],
}

impl Default for NetProfile {
    fn default() -> Self {
        Self { costume: NetCostume::default(), loadout: ["hammer".into(), "shield".into(), "wings".into(), "rocket launcher".into()] }
    }
}

fn clean_text(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(MAX_TEXT).collect()
}

/// Oversized client-supplied shapes are dropped (the host builds the shape from the recipe).
fn small(s: Option<ShapeSpec>) -> Option<ShapeSpec> {
    s.filter(|s| s.prims.len() <= MAX_SHAPE_PRIMS && s.parts.len() <= MAX_SHAPE_PRIMS)
}

fn fin(v: f32, d: f32) -> f32 {
    if v.is_finite() { v } else { d }
}

impl NetProfile {
    /// Untrusted → safe.
    pub fn sanitized(mut self) -> Self {
        self.costume.phrase = clean_text(&self.costume.phrase);
        self.costume.head_scale = fin(self.costume.head_scale, 1.0).clamp(0.7, 1.35);
        self.costume.accent = self.costume.accent.map(|a| a.map(|c| fin(c, 1.0).clamp(0.0, 1.0)));
        for l in self.loadout.iter_mut() {
            *l = clean_text(l);
        }
        self
    }
}

/// Which map a match is played on (generation is deterministic from kind + seed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetMapKind {
    #[default]
    Archipelago,
    Miami,
    Orbit,
    Backrooms,
    /// Protocol v10.
    Sakura,
    /// Protocol v11.
    Ember,
    /// Mirrorfold (protocol bump with the map).
    Mirrorfold,
    /// High Roller (protocol v14).
    Casino,
    /// Valley of the End (protocol v15).
    Valley,
    /// Cybertron (protocol v16).
    Cybertron,
    /// Turtle Lair (protocol v17).
    Lair,
    /// Giant's Room (protocol v18).
    GiantsRoom,
    /// Hyperline (protocol v22; the train's slipstream is map-generated, same on host and clients).
    Hyperline,
}

impl NetMapKind {
    /// Every map, in menu order (the end-of-match vote shows them all). A new variant must be added
    /// here too: [`NetMapKind::index`] is exhaustive and the `map_list_is_complete` test checks it.
    pub const ALL: [NetMapKind; 13] = [
        NetMapKind::Archipelago,
        NetMapKind::Miami,
        NetMapKind::Orbit,
        NetMapKind::Backrooms,
        NetMapKind::Sakura,
        NetMapKind::Ember,
        NetMapKind::Mirrorfold,
        NetMapKind::Casino,
        NetMapKind::Valley,
        NetMapKind::Cybertron,
        NetMapKind::Lair,
        NetMapKind::GiantsRoom,
        NetMapKind::Hyperline,
    ];

    /// Position in [`NetMapKind::ALL`].
    pub fn index(self) -> usize {
        match self {
            NetMapKind::Archipelago => 0,
            NetMapKind::Miami => 1,
            NetMapKind::Orbit => 2,
            NetMapKind::Backrooms => 3,
            NetMapKind::Sakura => 4,
            NetMapKind::Ember => 5,
            NetMapKind::Mirrorfold => 6,
            NetMapKind::Casino => 7,
            NetMapKind::Valley => 8,
            NetMapKind::Cybertron => 9,
            NetMapKind::Lair => 10,
            NetMapKind::GiantsRoom => 11,
            NetMapKind::Hyperline => 12,
        }
    }

    /// Display name (the lobby's map knob uses the same words).
    pub fn name(self) -> &'static str {
        match self {
            NetMapKind::Archipelago => "ARCHIPELAGO",
            NetMapKind::Miami => "MIAMI",
            NetMapKind::Orbit => "ORBIT",
            NetMapKind::Backrooms => "BACKROOMS",
            NetMapKind::Sakura => "SAKURA KEEP",
            NetMapKind::Ember => "EMBER",
            NetMapKind::Mirrorfold => "MIRRORFOLD",
            NetMapKind::Casino => "HIGH ROLLER",
            NetMapKind::Valley => "VALLEY OF THE END",
            NetMapKind::Cybertron => "CYBERTRON",
            NetMapKind::Lair => "TURTLE LAIR",
            NetMapKind::GiantsRoom => "GIANT'S ROOM",
            NetMapKind::Hyperline => "HYPERLINE",
        }
    }

    /// The render / menu map kind.
    pub fn map_kind(self) -> crate::arena::MapKind {
        use crate::arena::MapKind;
        match self {
            NetMapKind::Archipelago => MapKind::Arena,
            NetMapKind::Miami => MapKind::Miami,
            NetMapKind::Orbit => MapKind::Orbit,
            NetMapKind::Backrooms => MapKind::Backrooms,
            NetMapKind::Sakura => MapKind::Sakura,
            NetMapKind::Ember => MapKind::Ember,
            NetMapKind::Mirrorfold => MapKind::Mirrorfold,
            NetMapKind::Casino => MapKind::Casino,
            NetMapKind::Valley => MapKind::Valley,
            NetMapKind::Cybertron => MapKind::Cybertron,
            NetMapKind::Lair => MapKind::Lair,
            NetMapKind::GiantsRoom => MapKind::GiantsRoom,
            NetMapKind::Hyperline => MapKind::Hyperline,
        }
    }

    /// From a render / menu map kind (the tutorial is not a match map: Archipelago).
    pub fn from_map_kind(m: crate::arena::MapKind) -> Self {
        Self::ALL.into_iter().find(|k| k.map_kind() == m).unwrap_or_default()
    }
}

/// The vote for the next match while a match is over (protocol v23).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NextVote {
    /// One vote per player (the latest click).
    pub votes: Vec<(PlayerId, NetMapKind)>,
    /// Seconds until the next match starts (at send time).
    pub secs_left: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetMap {
    pub kind: NetMapKind,
    pub seed: u64,
    /// `content_hash` of the freshly generated terrain (0 = unknown yet). A client whose build
    /// generates different voxels (another game version) refuses to play instead of desyncing.
    pub pristine_hash: u64,
}

/// Generate a map's terrain: `(voxels, arena info, seed)`. ~150 ms+: call on a worker.
pub fn build_terrain(kind: NetMapKind, seed: u64) -> (bd_voxel::VoxelWorld, bd_voxel::ArenaInfo, u64) {
    match kind {
        NetMapKind::Archipelago => {
            let (w, i) = bd_voxel::generate_arena(seed);
            (w, i, seed)
        }
        NetMapKind::Miami => {
            let (w, m) = bd_voxel::generate_miami(seed);
            (w, m.arena, seed)
        }
        NetMapKind::Orbit => {
            let (w, m) = bd_voxel::generate_orbit(seed);
            (w, m.arena, seed)
        }
        NetMapKind::Backrooms => {
            let (w, m) = bd_voxel::generate_backrooms(seed);
            (w, m.arena, seed)
        }
        NetMapKind::Sakura => {
            let (w, m) = bd_voxel::generate_sakura(seed);
            (w, m.arena, seed)
        }
        NetMapKind::Ember => {
            let (w, m) = bd_voxel::maps::generate_ember(seed);
            (w, m.arena, seed)
        }
        NetMapKind::Mirrorfold => {
            let (w, m) = bd_voxel::generate_mirror(seed);
            (w, m.arena, seed)
        }
        NetMapKind::Casino => {
            let (w, m) = bd_voxel::generate_casino(seed);
            (w, m.arena, seed)
        }
        NetMapKind::Valley => {
            let (w, m) = bd_voxel::generate_valley(seed);
            (w, m.arena, seed)
        }
        NetMapKind::Cybertron => {
            let (w, m) = bd_voxel::maps::generate_cybertron(seed);
            (w, m.arena, seed)
        }
        NetMapKind::Lair => {
            let (w, m) = bd_voxel::generate_lair(seed);
            (w, m.arena, seed)
        }
        NetMapKind::GiantsRoom => {
            let (w, m) = bd_voxel::generate_giant(seed);
            (w, m.arena, seed)
        }
        NetMapKind::Hyperline => {
            let (w, m) = bd_voxel::generate_hyperline(seed);
            (w, m.arena, seed)
        }
    }
}

/// One lobby row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LobbyPlayer {
    pub id: PlayerId,
    pub name: String,
    pub corps: Corps,
    pub ready: bool,
    pub is_host: bool,
    pub profile: NetProfile,
    /// Round trip to the host (ms; 0 for the host itself).
    pub rtt_ms: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LobbyInfo {
    pub players: Vec<LobbyPlayer>,
    pub settings: GameSettings,
    pub map: NetMap,
    /// A match is running (joiners go straight in).
    pub in_match: bool,
    /// Protocol v23: the match is over and the lobby votes on the next map.
    pub vote: Option<NextVote>,
    /// Protocol v23: competitive stats of the current / finished match (host-authoritative).
    pub stats: Vec<bd_sim::match_stats::StatLine>,
}

/// Commands a client may ask the host to run for its player. Mirrors `sim_bridge::Cmd` minus
/// host-only tools.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetCmd {
    /// `seq` = client cast sequence (echoed in [`S2C::CastResult`] for predicted spawns).
    Cast { seq: u32, recipe: Box<Recipe>, shape: Option<ShapeSpec>, alt_shape: Option<ShapeSpec>, aim: Aim, incantation: bool },
    /// Protocol v9: carries the crosshair aim (third-person convergence; host-validated).
    PowerMove { slot: u8, aim: Option<Aim> },
    StandRush(u16),
    /// Oath section index (the host looks the length up in its own table; never sent by the client).
    OathBegin { section: u8 },
    OathProgress { correct_chars: u16, accuracy: f32 },
    OathResult { accuracy: f32, seconds: f32, voice: bool },
    OathCancel,
    Dismiss,
    DismissOne(ConstructId),
    ToggleTransform,
    Eject,
    PartyPick(u8),
    /// A signature cast (`Cmd::Signature`); `seq` like `Cast`.
    Signature { seq: u32, recipe: Box<Recipe>, shape: Option<ShapeSpec>, aim: Aim, sig: bd_sim::signature::SignatureCast },
    /// FREEDOM HOOK: how the next cast was said (`Cmd::CastMeta`).
    CastMeta(bd_sim::freedom::CastMeta),
}

impl NetCmd {
    /// The client cast sequence of a cast-like command (echoed in [`S2C::CastResult`]).
    pub fn cast_seq(&self) -> Option<u32> {
        match self {
            NetCmd::Cast { seq, .. } | NetCmd::Signature { seq, .. } => Some(*seq),
            _ => None,
        }
    }

    /// Client side: which bridge commands travel to the host (`GrantPower` is a local tool).
    pub fn from_cmd(c: &Cmd, cast_seq: u32) -> Option<Self> {
        Some(match c.clone() {
            Cmd::Cast { recipe, shape, alt_shape, aim, incantation } => {
                let shape = small(shape);
                NetCmd::Cast { seq: cast_seq, recipe, shape, alt_shape: small(alt_shape), aim, incantation }
            }
            Cmd::PowerMove(slot, aim) => NetCmd::PowerMove { slot, aim },
            Cmd::StandRush(n) => NetCmd::StandRush(n),
            Cmd::OathBegin { section } => NetCmd::OathBegin { section },
            Cmd::OathProgress { correct_chars, accuracy } => NetCmd::OathProgress { correct_chars, accuracy },
            Cmd::OathResult { accuracy, seconds, voice } => NetCmd::OathResult { accuracy, seconds, voice },
            Cmd::OathCancel => NetCmd::OathCancel,
            Cmd::Dismiss => NetCmd::Dismiss,
            Cmd::DismissOne(id) => NetCmd::DismissOne(id),
            Cmd::ToggleTransform => NetCmd::ToggleTransform,
            Cmd::Eject => NetCmd::Eject,
            Cmd::PartyPick(i) => NetCmd::PartyPick(i),
            Cmd::GrantPower(_) => return None,
            Cmd::Signature { recipe, shape, aim, sig } => NetCmd::Signature { seq: cast_seq, recipe, shape: small(shape), aim, sig },
            Cmd::CastMeta(m) => NetCmd::CastMeta(m),
        })
    }

    /// Host side: validate an untrusted command and turn it into a bridge command.
    pub fn into_cmd(self) -> Option<Cmd> {
        Some(match self {
            NetCmd::Cast { recipe, shape, alt_shape, aim, incantation, .. } => {
                let shape = small(shape);
                // The sim validates the recipe and re-checks the aim against the caster's eye.
                let aim = Aim { origin: aim.origin, dir: aim.dir, target_hint: aim.target_hint, modifier: aim.modifier };
                Cmd::Cast { recipe, shape, alt_shape: small(alt_shape), aim, incantation }
            }
            // The sim re-checks the aim against the caster's eye / look (`World::resolve_aim`).
            NetCmd::PowerMove { slot, aim } => Cmd::PowerMove(slot, aim.map(|a| Aim { target_hint: None, modifier: false, ..a })),
            NetCmd::StandRush(n) => Cmd::StandRush(n.min(64)),
            // Out of any oath's range → section 0 (the sim also falls back per corps).
            NetCmd::OathBegin { section } => Cmd::OathBegin { section: if (section as usize) < bd_sim::willpower::MAX_OATH_SECTIONS { section } else { 0 } },
            NetCmd::OathProgress { correct_chars, accuracy } => Cmd::OathProgress { correct_chars, accuracy: fin(accuracy, 0.0).clamp(0.0, 1.0) },
            NetCmd::OathResult { accuracy, seconds, voice } => {
                Cmd::OathResult { accuracy: fin(accuracy, 0.0).clamp(0.0, 1.0), seconds: fin(seconds, 60.0).clamp(0.5, 600.0), voice }
            }
            NetCmd::OathCancel => Cmd::OathCancel,
            NetCmd::Dismiss => Cmd::Dismiss,
            NetCmd::DismissOne(id) => Cmd::DismissOne(id),
            NetCmd::ToggleTransform => Cmd::ToggleTransform,
            NetCmd::Eject => Cmd::Eject,
            NetCmd::PartyPick(i) => Cmd::PartyPick(i.min(8)),
            NetCmd::Signature { recipe, shape, aim, sig, .. } => {
                // The sim validates the recipe, re-checks the aim and caps the amplified scale / cost.
                let aim = Aim { origin: aim.origin, dir: aim.dir, target_hint: aim.target_hint, modifier: false };
                Cmd::Signature { recipe, shape: small(shape), aim, sig: sig.sanitized() }
            }
            // Untrusted words: the shout sanitiser, then the sim's own clamp (energy is a u8 already).
            NetCmd::CastMeta(m) => {
                let phrase = bd_fx::sanitize_shout(&m.phrase, bd_fx::SHOUT_MAX_CHARS);
                Cmd::CastMeta(bd_sim::freedom::CastMeta { phrase, ..m }.sanitized())
            }
        })
    }
}

/// Client → host.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum C2S {
    /// First message after the handshake: a random per-client token. A client that drops and
    /// reconnects with the same token within [`crate::net::host::REJOIN_SECS`] gets its player
    /// (KOs, deaths, stocks) back.
    Token(u64),
    Profile(NetProfile),
    Ready(bool),
    /// Inputs (with redundancy) + the newest snapshot tick decoded (delta baseline ack).
    Input { packet: InputPacket<PlayerInput>, snap_ack: Tick },
    Cmd(NetCmd),
    /// The snapshot baseline was unknown: send a keyframe.
    KeyframeReq,
    /// Terrain / world desync detected: send the whole world again.
    Resync,
    /// Cosmetic: the phrase this player just cast, for everyone's shout callout (host re-sanitises,
    /// rate-limits and relays it as [`S2C::Shout`]).
    Shout(crate::present::shout::ShoutLine),
    /// The match is over: this player's pick for the next map (protocol v23; the latest one counts).
    MapVote(NetMapKind),
}

/// Everything a joining (or resyncing) client needs to rebuild the world.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldInit {
    pub cfg: SimConfig,
    pub map: NetMap,
    /// Seed of the terrain currently loaded (regenerate from it, then replay the log).
    pub arena_seed: u64,
    pub tick: Tick,
    /// `ConstructSpawned` for every live construct.
    pub manifest: Vec<GameEvent>,
    /// Number of terrain events that follow in `TerrainLog` chunks.
    pub terrain_events: u32,
}

/// What became of a client's cast.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CastOutcome {
    /// New construct (its `ConstructSpawned` event follows).
    Spawned(ConstructId),
    /// Grafted onto an existing construct.
    Grafted(ConstructId),
    Failed(bd_sim::events::CastError),
}

/// Host → client.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum S2C {
    Lobby(LobbyInfo),
    WorldInit(Box<WorldInit>),
    /// Terrain edits since the arena was generated, in order (chunked).
    TerrainLog(Vec<GameEvent>),
    /// End of the init stream.
    WorldReady,
    /// Events of one or more ticks, in sim order.
    Events { tick: Tick, events: Vec<GameEvent> },
    /// Terrain hash after every event sent so far (desync detection).
    TerrainCheck { tick: Tick, hash: u64 },
    /// Outcome of the client's cast `seq` (sent just before the events that contain the spawn).
    CastResult { seq: u32, outcome: CastOutcome },
    /// The match ended / host went back to the lobby.
    BackToLobby,
    /// Latest lane.
    Snapshot(SnapshotDelta),
    /// Cosmetic: `player` shouted `line` (receivers re-sanitise and slur-filter it).
    Shout { player: PlayerId, line: crate::present::shout::ShoutLine },
    /// The host's combat tuning changed (`assets/tuning/combat.ron` hot reload): clients apply it
    /// to their replica / prediction world (and to a world init still in flight). At join the
    /// tuning travels in `WorldInit.cfg.tuning`. Protocol v7.
    Tuning(Box<bd_sim::tuning::CombatTuning>),
}

/// Does this event change terrain (logged for late joiners)?
pub fn is_terrain_event(e: &GameEvent) -> bool {
    // MAP MECH: mover phases too, so late joiners rebuild the transforming panels' colliders.
    matches!(e, GameEvent::Carve { .. } | GameEvent::Extracted { .. } | GameEvent::Power(bd_sim::powers::PowerEvent::Restored { .. }) | GameEvent::ArenaReset { .. } | GameEvent::MapMover { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_profile_is_cleaned() {
        let p = NetProfile {
            costume: NetCostume { phrase: "\u{7}x".repeat(500), seed: 1, accent: Some([f32::NAN, 9.0, -1.0]), head_scale: f32::INFINITY, hide: 0 },
            loadout: ["a".repeat(1000), String::new(), "\n".into(), "ok".into()],
        }
        .sanitized();
        assert!(p.costume.phrase.chars().count() <= MAX_TEXT);
        assert_eq!(p.costume.accent, Some([1.0, 1.0, 0.0]));
        assert_eq!(p.costume.head_scale, 1.0);
        assert!(p.loadout.iter().all(|l| l.chars().count() <= MAX_TEXT && !l.contains('\n')));
    }

    #[test]
    fn signature_cmd_roundtrips_and_is_sanitised() {
        use bd_sim::signature::{SignatureCast, SignatureKind};
        let aim = Aim::new(bd_core::Vec3::ZERO, bd_core::Vec3::NEG_Z);
        let sig = SignatureCast { kind: SignatureKind::Domain, power: f32::NAN, merge: None };
        let cmd = Cmd::Signature { recipe: Box::new(Recipe::unknown("Domain Expansion")), shape: None, aim, sig };
        let nc = NetCmd::from_cmd(&cmd, 7).expect("travels");
        assert_eq!(nc.cast_seq(), Some(7));
        let back: C2S = bd_net::codec::decode(&bd_net::codec::encode(&C2S::Cmd(nc))).unwrap();
        let C2S::Cmd(nc) = back else { panic!("cmd") };
        match nc.into_cmd() {
            Some(Cmd::Signature { sig, .. }) => assert_eq!(sig.power, 0.0),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cast_meta_travels_and_is_sanitised() {
        let hostile = bd_sim::freedom::CastMeta { phrase: format!("KING KONG\u{7}<b>{}", "!".repeat(400)), energy: 255, resize: f32::INFINITY };
        let nc = NetCmd::from_cmd(&Cmd::CastMeta(hostile), 3).expect("travels");
        assert_eq!(nc.cast_seq(), None, "not a cast itself");
        let back: C2S = bd_net::codec::decode(&bd_net::codec::encode(&C2S::Cmd(nc))).unwrap();
        let C2S::Cmd(nc) = back else { panic!("cmd") };
        match nc.into_cmd() {
            Some(Cmd::CastMeta(m)) => {
                assert!(m.phrase.starts_with("KING KONG") && m.phrase.len() <= bd_sim::freedom::PHRASE_MAX && !m.phrase.contains('<'));
                assert_eq!((m.energy, m.resize), (255, 1.0));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn map_list_is_complete() {
        for (i, k) in NetMapKind::ALL.iter().enumerate() {
            assert_eq!(k.index(), i, "{k:?}");
            assert_eq!(NetMapKind::from_map_kind(k.map_kind()), *k);
            assert!(!k.name().is_empty());
        }
        let back: C2S = bd_net::codec::decode(&bd_net::codec::encode(&C2S::MapVote(NetMapKind::Lair))).unwrap();
        assert!(matches!(back, C2S::MapVote(NetMapKind::Lair)));
    }

    #[test]
    fn roundtrip_messages() {
        let m = C2S::Input { packet: InputPacket { view_tick: 12.5, inputs: vec![PlayerInput::default()] }, snap_ack: 9 };
        let b = bd_net::codec::encode(&m);
        let back: C2S = bd_net::codec::decode(&b).unwrap();
        assert!(matches!(back, C2S::Input { snap_ack: 9, .. }));
        let s = S2C::TerrainCheck { tick: 5, hash: 77 };
        let back: S2C = bd_net::codec::decode(&bd_net::codec::encode(&s)).unwrap();
        assert!(matches!(back, S2C::TerrainCheck { tick: 5, hash: 77 }));
    }

    #[test]
    fn oath_begin_carries_a_section_and_is_clamped() {
        for (sent, got) in [(0u8, 0u8), (1, 1), (3, 3), (4, 0), (255, 0)] {
            let n = NetCmd::from_cmd(&Cmd::OathBegin { section: sent }, 0).unwrap();
            let back: NetCmd = bd_net::codec::decode(&bd_net::codec::encode(&n)).unwrap();
            assert!(matches!(back, NetCmd::OathBegin { section } if section == sent));
            assert!(matches!(back.into_cmd(), Some(Cmd::OathBegin { section }) if section == got), "{sent}");
        }
        for voice in [false, true] {
            let n = NetCmd::from_cmd(&Cmd::OathResult { accuracy: f32::NAN, seconds: 5.0, voice }, 0).unwrap();
            let back: NetCmd = bd_net::codec::decode(&bd_net::codec::encode(&n)).unwrap();
            assert!(matches!(back.into_cmd(), Some(Cmd::OathResult { accuracy: 0.0, seconds: 5.0, voice: v }) if v == voice));
        }
    }
}
