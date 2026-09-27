//! MOVE PRESENTATION GRAMMAR (TOOLBOX PRESENTATION; docs/toolbox/PRESENTATION.md).
//!
//! Owner: "unique flavor/control/playstyle + dramatic/cool presentation = fun" — and "for things like
//! stretchy punch it would be cool if it looked/felt different than stretchy pole". Constructs have
//! the Hollow-Purple grammar (presence tiers); this gives it to MOVES:
//!
//! * **Per op kind, a full beat** — gather → release → world reaction → clean exit. The gather
//!   plays concurrently with the release (never delays the sim): a blink implodes where you were as
//!   you burst out where you land, with a trace between; a dash tears the air, leaves streak ribbons
//!   and afterimages and skids when it lands; a shove throws a cone shockwave, cracks the ground and
//!   flings debris; a slam paints a dread ring under you that tightens as you fall, then craters with
//!   a dust wall; a reel shoots a line / limb to the anchor and snaps back; fields shimmer at the
//!   boundary and flare where anyone crosses; a swap flashes both ends at once; explosions bloom in
//!   the body's shape; time stops ripple from the caster.
//! * **Per style (the BODY the move is made of)** — `fx: "rubber"` on a move / power picks a style
//!   from `assets/tuning/move_fx.ron` (hot-reloaded): a [`table::Body`] (closed set: the shapes) +
//!   colours + numbers + sounds. Rubber limbs stretch from the shoulder, thin, wobble and band-snap
//!   back with a boing; rigid ones telescope in banded segments with a metallic ring; smoke BAMFs;
//!   bolts re-strike; speedforce streaks…
//! * **Poses**: [`MovePoses`] — the local viewmodel (punch / palms / grab / lunge / slam) and any
//!   player's third-person body (`present::players`).
//! * **Readable for everyone**: every beat is drawn for every player (it is how you read what an
//!   enemy just did); the local player's own motes are scaled down near the camera.
//! * **Cost**: GPU particles + immediate cubes only (bd_fx `FxParticles`); beats live in a
//!   pre-allocated pool ([`table::Beats::max_beats`], the oldest recycled); nothing allocates per
//!   frame. Mote counts follow the graphics particle budget.
//!
//! Inputs: `PowerEvent::Move` (a held power's move) → [`MoveFxFired`]; anything else that fires a
//! move (borrowed moves, chains, dev harnesses) writes [`MoveFxFired`] directly.

pub mod draw;
pub mod table;
mod dev;

use self::draw::{Look, Pen, basis, circle, light_limb, rigid_pole, rubber_limb, zig};
use self::table::{Body, Cue, MoveFx};
use super::super::sound::Sfx3d;
use super::super::{PresentSet, bv, corps_rgb, gv};
use super::ascend::lin;
use crate::camera::{CameraUpdateSet, PlayerCamera};
use crate::debris::DebrisImpulse;
use crate::juice::{CameraShakeEvent, FovPunch};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use crate::viewmodel::{HandsPublishSet, ViewmodelHands};
use bd_core::mechanics::Element;
use bd_core::{ConstructId, PlayerId};
use bd_fx::FxParticles;
use bd_sim::GameEvent;
use bd_sim::powers::chain::FireHow;
use bd_sim::powers::toolbox::MoveRef;
use bd_sim::powers::{At, MoveDef, MoveOp, PowerDef, PowerEvent, PowerTable};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// A move fired, as the presentation reads it. Written for every held power's move
/// (`PowerEvent::Move`); borrowed moves / chain links / dev harnesses write it directly.
#[derive(Message, Clone, Copy, Debug)]
pub struct MoveFxFired {
    pub player: PlayerId,
    pub mv: &'static MoveDef,
    /// The power the move belongs to (style / colour fallbacks), if any.
    pub power: Option<&'static PowerDef>,
    /// Fired as a borrowed / kit one-shot (not a held power's move).
    pub borrowed: bool,
    /// The global move id (TOOLBOX `MoveRef`; `None` for synthetic / dev fires).
    pub key: Option<MoveRef>,
    /// The mover's feet before / after the ops ran and the aim it fired along (from the sim's
    /// `PowerEvent::Fired`; `None` = read them from the player view).
    pub from: Option<Vec3>,
    pub to: Option<Vec3>,
    pub dir: Option<Vec3>,
    /// Steps of the current chain fired before this one.
    pub step: u8,
}

impl MoveFxFired {
    /// A synthetic fire (dev harnesses, tests): everything read from the player view.
    pub fn synthetic(player: PlayerId, mv: &'static MoveDef, power: Option<&'static PowerDef>, borrowed: bool) -> Self {
        Self { player, mv, power, borrowed, key: None, from: None, to: None, dir: None, step: 0 }
    }
}

/// A chain link fired: `index` (0-based) of `len` links — the HUD pops a pip. Written by the chain
/// runner (TOOLBOX CORE) / dev harnesses; see docs/toolbox/PRESENTATION_API.md.
#[derive(Message, Clone, Copy, Debug)]
pub struct MoveChainFx {
    pub player: PlayerId,
    pub index: u8,
    pub len: u8,
}

/// Which op a beat presents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    Cast,
    Impulse,
    Blink,
    Reel,
    Shove,
    Field,
    Room,
    Swap,
    Explode,
    Carve,
    TimeStop,
    Slam,
    Reach,
    Pulse,
    /// An incanted delivery's arm (gomu gomu no <anything>): a limb to the delivered construct.
    Arm,
}

impl OpKind {
    pub fn of(op: &MoveOp) -> Self {
        match op {
            MoveOp::Cast { .. } => Self::Cast,
            MoveOp::Impulse { .. } => Self::Impulse,
            MoveOp::Blink { .. } => Self::Blink,
            MoveOp::Reel { .. } => Self::Reel,
            MoveOp::Shove { .. } => Self::Shove,
            MoveOp::Field { .. } => Self::Field,
            MoveOp::Room { .. } => Self::Room,
            MoveOp::Swap { .. } => Self::Swap,
            MoveOp::Explode { .. } => Self::Explode,
            MoveOp::Carve { .. } => Self::Carve,
            MoveOp::TimeStop { .. } => Self::TimeStop,
            MoveOp::SlamOnLand { .. } => Self::Slam,
            MoveOp::Link { .. } | MoveOp::Weigh { .. } | MoveOp::Yank { .. } => Self::Reach,
            // Portal / Restore / Transform / Tier / SelfStatus / Singularity / every new op until
            // it is mapped: a body pulse (their own presentation does the rest).
            #[allow(unreachable_patterns)]
            _ => Self::Pulse,
        }
    }
}

/// A TOOLBOX content op (`powers/ops/`) → the classic op whose beat presents it: lunges dash, hooks
/// reel, grabs / locks / seekers reach, surges shove, zones are fields, carried blinks blink. Every
/// other new op gets the body pulse (its own `PowerEvent::Op` beats add the rest).
pub fn alias_op(op: &MoveOp) -> Option<MoveOp> {
    Some(match *op {
        MoveOp::Ram { speed, .. } => MoveOp::Impulse { fwd: speed.clamp(4.0, 60.0), up: 0.5 },
        MoveOp::Pounce { speed, .. } => MoveOp::Impulse { fwd: speed.clamp(4.0, 60.0), up: 1.0 },
        MoveOp::Leap { range, apex } => MoveOp::Impulse { fwd: (range * 0.6).clamp(4.0, 40.0), up: (apex * 2.0).clamp(6.0, 30.0) },
        MoveOp::BlinkCarry { dist, .. } => MoveOp::Blink { dist, afterimage: true },
        MoveOp::Hook { range, .. } | MoveOp::Haul { range, .. } => MoveOp::Reel { range, speed: 30.0 },
        MoveOp::Grab { range, .. } | MoveOp::LockOn { range, .. } | MoveOp::Seeker { range, .. } | MoveOp::Afflict { range, .. } | MoveOp::Lockout { range, .. } => MoveOp::Link { range },
        MoveOp::Surge { dist, width, element, .. } => MoveOp::Shove { range: dist.clamp(2.0, 20.0), angle: (width * 8.0).clamp(30.0, 120.0), damage: 0.0, knockback: 0.0, element, up: 0.0 },
        MoveOp::Zone { radius, secs, at, dist, .. } => MoveOp::Field { strength: 0.0, radius, secs, at, dist },
        // SHONEN CLASSICS (ops/live.rs; their own beats: `OpEvent::Toolbox`): the palm lunge dashes, the
        // travelling crescent reads as a slash line, the stand rush as a front flurry, a thrown mark /
        // a gaze / a brand / a swallow reach their target. (WarpTo / a recall send `Blinked` already.)
        MoveOp::Drill { speed, .. } => MoveOp::Impulse { fwd: speed.clamp(4.0, 60.0), up: 0.3 },
        MoveOp::Wave { length, width, .. } => MoveOp::Carve { length: length.clamp(1.0, 40.0), radius: (width * 0.25).clamp(0.3, 4.0), damage: 0.0, pull: false },
        MoveOp::Rush { range, .. } => MoveOp::Shove { range: range.max(2.8), angle: 55.0, damage: 0.0, knockback: 0.0, element: bd_core::mechanics::Element::Light, up: 0.0 },
        MoveOp::MarkRecall { range, .. } | MoveOp::GazeStun { range, .. } | MoveOp::Contagion { range, .. } | MoveOp::Banish { range, .. } => MoveOp::Link { range },
        _ => return None,
    })
}

/// The body pose a move asks for (first person: viewmodel; third person: the player body).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovePose {
    /// Right fist thrust (shove, fist casts, stretch punches).
    Punch,
    /// Both palms out (fields, blasts, time, portals).
    Palms,
    /// Reach and grab (reel, yank, link).
    Grab,
    /// A lurch forward (blink, dash).
    Lunge,
    /// Arms up for the drop (slam).
    Slam,
    /// Fists clench (forms, statuses).
    Clench,
}

impl MovePose {
    pub fn of(op: OpKind) -> Self {
        match op {
            OpKind::Shove | OpKind::Cast | OpKind::Arm | OpKind::Carve => Self::Punch,
            OpKind::Reel | OpKind::Reach => Self::Grab,
            OpKind::Blink | OpKind::Impulse => Self::Lunge,
            OpKind::Slam => Self::Slam,
            OpKind::Field | OpKind::Room | OpKind::Swap | OpKind::Explode | OpKind::TimeStop => Self::Palms,
            OpKind::Pulse => Self::Clench,
        }
    }
}

/// Pose requests: `local` is taken by the viewmodel (`viewmodel_anim`), `by_player` is read by the
/// third-person body (`present::players`) — (pose, seconds since it started).
#[derive(Resource, Default)]
pub struct MovePoses {
    pub local: Option<(MovePose, f32)>,
    pub by_player: HashMap<PlayerId, (MovePose, f32)>,
}

impl MovePoses {
    /// The third-person pose of `p` while it plays (≤ 0.45 s).
    pub fn body(&self, p: PlayerId) -> Option<(MovePose, f32)> {
        self.by_player.get(&p).copied().filter(|(_, t)| *t < 0.45)
    }
}

/// One beat in the pool.
#[derive(Clone, Copy, Debug)]
struct Beat {
    live: bool,
    op: OpKind,
    look: Look,
    player: PlayerId,
    local: bool,
    t: f32,
    dur: f32,
    a: Vec3,
    b: Vec3,
    dir: Vec3,
    r: f32,
    seed: u32,
    construct: Option<ConstructId>,
    phase: u8,
    acc: f32,
    last: Vec3,
    mask: u64,
    flag: bool,
}

impl Default for Beat {
    fn default() -> Self {
        Self {
            live: false,
            op: OpKind::Pulse,
            look: Look::default(),
            player: PlayerId(0),
            local: false,
            t: 0.0,
            dur: 0.0,
            a: Vec3::ZERO,
            b: Vec3::ZERO,
            dir: Vec3::NEG_Z,
            r: 0.0,
            seed: 0,
            construct: None,
            phase: 0,
            acc: 0.0,
            last: Vec3::ZERO,
            mask: 0,
            flag: false,
        }
    }
}

#[derive(Resource, Default)]
struct Pool {
    beats: Vec<Beat>,
    seed: u32,
    /// Stats for the perf readout: beats started this session / peak live.
    started: u64,
    peak: usize,
}

impl Pool {
    fn add(&mut self, mut b: Beat, cap: usize) {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        b.seed = self.seed;
        b.live = true;
        self.started += 1;
        if let Some(slot) = self.beats.iter_mut().find(|x| !x.live) {
            *slot = b;
        } else if self.beats.len() < cap.max(8) {
            self.beats.push(b);
        } else if let Some(old) = self.beats.iter_mut().max_by(|x, y| x.t.total_cmp(&y.t)) {
            *old = b;
        }
        let live = self.beats.iter().filter(|x| x.live).count();
        self.peak = self.peak.max(live);
    }
}

/// This frame's side info from the sim events (fixed arrays: no allocation).
#[derive(Resource)]
struct Side {
    blinks: [(PlayerId, Vec3, Vec3, bool); 8],
    n_blinks: usize,
    swaps: [(PlayerId, Vec3, Vec3); 4],
    n_swaps: usize,
    spawned: [(PlayerId, ConstructId); 16],
    n_spawned: usize,
    arms: [(PlayerId, ConstructId); 4],
    n_arms: usize,
    booms: [(PlayerId, Vec3, f32); 8],
    n_booms: usize,
}

impl Default for Side {
    fn default() -> Self {
        let (p, c) = (PlayerId(0), ConstructId(0));
        Self {
            blinks: [(p, Vec3::ZERO, Vec3::ZERO, false); 8],
            n_blinks: 0,
            swaps: [(p, Vec3::ZERO, Vec3::ZERO); 4],
            n_swaps: 0,
            spawned: [(p, c); 16],
            n_spawned: 0,
            arms: [(p, c); 4],
            n_arms: 0,
            booms: [(p, Vec3::ZERO, 0.0); 8],
            n_booms: 0,
        }
    }
}

impl Side {
    fn clear(&mut self) {
        self.n_blinks = 0;
        self.n_swaps = 0;
        self.n_spawned = 0;
        self.n_arms = 0;
        self.n_booms = 0;
    }
}

/// Perf / debug readout (`present::powers::movefx::MoveFxStats`).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct MoveFxStats {
    pub live: usize,
    pub peak: usize,
    pub started: u64,
    /// Microseconds spent in the per-frame step (last frame).
    pub step_us: f32,
}

pub struct MoveFxPlugin;

impl Plugin for MoveFxPlugin {
    fn build(&self, app: &mut App) {
        dev::build(app);
        let fx = table::load();
        let cap = fx.t.beats.max_beats;
        app.insert_resource(fx)
            .insert_resource(Pool { beats: Vec::with_capacity(cap.max(8)), ..default() })
            .init_resource::<Side>()
            .init_resource::<MovePoses>()
            .init_resource::<MoveFxStats>()
            .add_message::<MoveFxFired>()
            .add_message::<MoveChainFx>()
            .add_message::<Sfx3d>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .add_message::<DebrisImpulse>()
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, (table::hot_reload, intake, start).chain().in_set(PresentSet::Sync))
            .add_systems(PostUpdate, step.after(CameraUpdateSet).after(HandsPublishSet).before(TransformSystems::Propagate));
    }
}

fn clear(mut pool: ResMut<Pool>, mut poses: ResMut<MovePoses>) {
    for b in pool.beats.iter_mut() {
        b.live = false;
    }
    poses.local = None;
    poses.by_player.clear();
}

/// The element a move is made of (first op naming a non-Light one).
pub fn move_element(mv: &MoveDef) -> Option<Element> {
    mv.ops.iter().find_map(|op| {
        let e = match op {
            MoveOp::Shove { element, .. }
            | MoveOp::Explode { element, .. }
            | MoveOp::Surge { element, .. }
            | MoveOp::Pounce { element, .. }
            | MoveOp::Counter { element, .. }
            | MoveOp::Salvo { element, .. }
            | MoveOp::Empower { element, .. } => *element,
            MoveOp::Cast { recipe, .. } => recipe.element,
            _ => Element::Light,
        };
        (e != Element::Light).then_some(e)
    })
}

fn power_element(def: &PowerDef) -> Option<Element> {
    def.graft.as_ref().and_then(|g| g.element).filter(|e| *e != Element::Light).or_else(|| def.pickup.as_ref().map(|p| p.element).filter(|e| *e != Element::Light))
}

/// Resolve the style of a move: move `fx` → power `fx` → the move's element → the power's element
/// → "energy". Colour: `juice.color` → the style's colour → the element's → the power's identity →
/// the corps colour.
pub fn resolve_look(fx: &MoveFx, mv: &MoveDef, power: Option<&PowerDef>, corps: [f32; 3]) -> Look {
    let el = move_element(mv).or_else(|| power.and_then(power_element));
    let style = dev::forced_style()
        .and_then(|k| fx.find(k))
        .or_else(|| fx.find(&mv.fx))
        .or_else(|| power.and_then(|p| fx.find(&p.fx)))
        // A power that delivers on a stretching arm (gomu gomu): its body is rubber.
        .or_else(|| power.filter(|p| p.delivery.arm).and_then(|_| fx.find("rubber")))
        .or_else(|| el.and_then(|e| fx.for_element(e)))
        .unwrap_or(fx.energy);
    look_of(fx, style, mv.juice.as_ref().and_then(|j| j.color).map(lin), el, power, corps)
}

pub fn look_of(fx: &MoveFx, style: u16, juice_color: Option<[f32; 3]>, el: Option<Element>, power: Option<&PowerDef>, corps: [f32; 3]) -> Look {
    let d = fx.def(style);
    let main = juice_color
        .or_else(|| d.color.map(lin))
        .or_else(|| el.map(bd_fx::palette::element_color))
        .or_else(|| power.map(crate::power_hud::derive::identity_color))
        .unwrap_or(corps);
    Look {
        style,
        body: d.body,
        main,
        core: lin(d.core),
        dark: lin(d.dark),
        glow: d.glow,
        density: d.density,
        rise: d.rise,
        jitter: d.jitter,
        wobble: d.wobble,
        speed: d.speed.clamp(0.25, 4.0),
        linger: d.linger.clamp(0.2, 5.0),
        size: d.size.clamp(0.2, 4.0),
        afterimages: d.afterimages,
    }
}

/// A cue → a positioned sound (names interned: no per-play allocation after the first).
fn play(sfx: &mut MessageWriter<Sfx3d>, c: &Cue, pos: Vec3, pitch: f32, gain: f32) {
    if c.is_some() {
        sfx.write(Sfx3d::at(intern(&c.0), pos).pitch(c.1 * pitch).gain(c.2 * gain));
    }
}

fn intern(s: &str) -> &'static str {
    static SET: std::sync::OnceLock<std::sync::Mutex<Vec<&'static str>>> = std::sync::OnceLock::new();
    let mut v = SET.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(x) = v.iter().find(|x| **x == s) {
        return x;
    }
    let x: &'static str = Box::leak(s.to_string().into_boxed_str());
    v.push(x);
    x
}

fn chest(v: &crate::sim_bridge::PlayerView) -> Vec3 {
    v.pos + Vec3::Y * 1.0 * v.size
}

fn eye_look(v: &crate::sim_bridge::PlayerView) -> (Vec3, Vec3) {
    let (eye, look, _) = super::super::constructs::body_frames(v);
    (eye, look * Vec3::NEG_Z)
}

/// The shoulder a limb grows from (third person / other players).
fn shoulder(v: &crate::sim_bridge::PlayerView) -> Vec3 {
    let (eye, _, body) = super::super::constructs::body_frames(v);
    eye + body * (Vec3::new(0.24, -0.3, -0.05) * v.size)
}

fn ground_below(active: &ActiveWorld, p: Vec3, max: f32) -> Option<Vec3> {
    bd_core::voxel::raycast(&active.world.voxels, gv(p + Vec3::Y * 0.3), bd_core::Vec3::NEG_Y, max).map(|h| bv(h.point))
}

fn aim_point(active: &ActiveWorld, eye: Vec3, look: Vec3, range: f32) -> (Vec3, bool) {
    match bd_core::voxel::raycast(&active.world.voxels, gv(eye), gv(look), range) {
        Some(h) => (bv(h.point), true),
        None => (eye + look * range, false),
    }
}

/// Read the sim's events: side info for this frame's beats, and every held power's move.
fn intake(mut events: MessageReader<GameEvents>, views: Res<WorldViews>, mut side: ResMut<Side>, mut fired: MessageWriter<MoveFxFired>) {
    for ev in events.read() {
        match &ev.event {
            GameEvent::Power(pe) => match pe {
                // ANY move (held, borrowed, kit): the TOOLBOX event carries the move id, where the
                // mover was / is and the aim. (Held moves also send the old `Move`: not read here.)
                PowerEvent::Fired { player, mv, how, from, to, dir, step, .. } => {
                    let table = PowerTable::get();
                    if let Some((def, m)) = mv.resolve(table) {
                        fired.write(MoveFxFired {
                            player: *player,
                            mv: m,
                            power: Some(def),
                            borrowed: *how != FireHow::Held,
                            key: Some(*mv),
                            from: Some(bv(*from)),
                            to: Some(bv(*to)),
                            dir: Some(bv(*dir)),
                            step: *step,
                        });
                    }
                }
                // VERB GRAMMAR: a described move ("shadow teleport behind ethan") presents like any
                // other: its spec expands to an ordinary move (ops + style `fx`), interned for the
                // `&'static` beats (docs/toolbox/COMPOSED_MOVE.md). Its `Fired` never resolves.
                PowerEvent::Composed { player, spec, from, to, dir, step, .. } => {
                    if let Some(m) = bd_sim::powers::compose::interned(spec) {
                        fired.write(MoveFxFired {
                            player: *player,
                            mv: m,
                            power: None,
                            borrowed: true,
                            key: None,
                            from: Some(bv(*from)),
                            to: Some(bv(*to)),
                            dir: Some(bv(*dir)),
                            step: *step,
                        });
                    }
                }
                PowerEvent::Blinked { player, from, to, afterimage } if side.n_blinks < side.blinks.len() => {
                    let n = side.n_blinks;
                    side.blinks[n] = (*player, bv(*from), bv(*to), *afterimage);
                    side.n_blinks += 1;
                }
                PowerEvent::Swapped { a, a_pos, b_pos, .. } if side.n_swaps < side.swaps.len() => {
                    let n = side.n_swaps;
                    side.swaps[n] = (*a, bv(*a_pos), bv(*b_pos));
                    side.n_swaps += 1;
                }
                PowerEvent::Delivered { player, construct, arm: true, .. } if side.n_arms < side.arms.len() => {
                    let n = side.n_arms;
                    side.arms[n] = (*player, *construct);
                    side.n_arms += 1;
                }
                _ => {}
            },
            GameEvent::ConstructSpawned { id, owner, .. } if side.n_spawned < side.spawned.len() => {
                let n = side.n_spawned;
                side.spawned[n] = (*owner, *id);
                side.n_spawned += 1;
            }
            GameEvent::Explosion { pos, radius, source: Some(src), .. } if side.n_booms < side.booms.len() => {
                let n = side.n_booms;
                side.booms[n] = (*src, bv(*pos), *radius);
                side.n_booms += 1;
            }
            _ => {}
        }
    }
}

/// Start the beats of every move fired this frame: the release happens NOW (particles + sounds +
/// camera), the rest plays out in [`step`].
#[allow(clippy::too_many_arguments)]
fn start(
    time: Res<Time<Real>>,
    mut fired: MessageReader<MoveFxFired>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    table: Res<MoveFx>,
    mut side: ResMut<Side>,
    mut pool: ResMut<Pool>,
    mut poses: ResMut<MovePoses>,
    mut fx: ResMut<FxParticles>,
    settings: Option<Res<bd_fx::FxSettings>>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    hands: Res<ViewmodelHands>,
    out: (MessageWriter<Sfx3d>, MessageWriter<CameraShakeEvent>, MessageWriter<DebrisImpulse>, MessageWriter<FovPunch>),
) {
    let (mut sfx, mut shake, mut debris, mut fov) = out;
    let dt = time.delta_secs().min(0.1);
    for (_, (_, t)) in poses.by_player.iter_mut() {
        *t += dt;
    }
    let Some(active) = active.filter(|_| !dev::off()) else {
        fired.clear();
        side.clear();
        return;
    };
    let tb = &table.t;
    let cap = tb.beats.max_beats;
    let (cam, cam_fwd) = cams.iter().next().map_or((Vec3::ZERO, Vec3::NEG_Z), |g| (g.translation(), g.forward().as_vec3()));
    let budget = settings.as_ref().map_or(1.0, |s| s.particle_budget).clamp(0.15, 1.0);
    let local = active.local;
    for f in fired.read() {
        let Some(v) = views.player(f.player) else { continue };
        let corps = corps_rgb(v.corps);
        let look = resolve_look(&table, f.mv, f.power, corps);
        let (eye, look_dir) = eye_look(v);
        let dir = f.dir.filter(|d| d.length_squared() > 0.25).map_or(look_dir, |d| d.normalize());
        let hand = super::super::constructs::hand_transform(v).translation;
        let mine = Some(f.player) == local;
        let scale = if mine { tb.beats.own_scale } else { 1.0 };
        let own_fp = mine && hands.active;
        let up = Vec3::Y * 1.0 * v.size;
        // Where "me" is for this move's ops: the sim's before-position until a blink moves it.
        let mut me = f.from.map_or(chest(v), |p| p + up);
        let avoid = own_fp.then(|| f.to.map_or(chest(v), |p| p + up));
        let mut pen = Pen { fx: &mut fx, motes: tb.beats.motes * budget, scale, cam, avoid };
        // A borrowed move of a power with a companion (a Stand): its ghost steps out behind you
        // and throws the move, then fades (presentation only: nothing spawns in the sim).
        if f.borrowed && f.power.is_some_and(|p| p.companion.is_some()) {
            let (_, _, body) = super::super::constructs::body_frames(v);
            let at = v.pos + body * (Vec3::new(0.45, 0.25, 0.35) * v.size);
            pen.ghost(at, v.yaw, v.size * 1.15, look.main, look.glow * 0.7, 0.6 * look.linger);
            pen.burst(&look, at + Vec3::Y * v.size, 0.5, 2.0, None, 0.0, 0.4);
        }
        let mut first_pose = None;
        for op in &f.mv.ops {
            // TOOLBOX content ops present through the classic beat they read as (no allocation:
            // plain value variants).
            let alias = alias_op(op);
            let op = alias.as_ref().unwrap_or(op);
            let kind = OpKind::of(op);
            first_pose.get_or_insert(MovePose::of(kind));
            let mut b = Beat { op: kind, look, player: f.player, local: mine, a: me, b: me, dir, last: me, ..default() };
            let s = look.speed;
            let ops = &tb.sounds;
            let st = &table.def(look.style).sound;
            match op {
                MoveOp::Blink { afterimage, .. } => {
                    let found = side.blinks[..side.n_blinks].iter().find(|x| x.0 == f.player).copied();
                    let (from, to) = found.map(|x| (x.1, x.2)).or(f.from.zip(f.to)).unwrap_or((v.pos, v.pos + dir * 6.0));
                    let up = Vec3::Y * v.size;
                    b.a = from + up;
                    b.b = to + up;
                    b.dur = tb.beats.trace_secs * look.linger;
                    // Gather (implosion where you were) + release (burst where you are) at once.
                    pen.implode(&look, b.a, 1.4 * v.size, tb.beats.gather_secs * s, 1.0);
                    pen.flash(b.a, 1.3 * v.size, look.main, look.glow, 0.14 * s);
                    pen.trace(&look, b.a, b.b, tb.beats.trace_secs * look.linger);
                    pen.flash(b.b, 1.7 * v.size, look.core, look.glow * 1.2, 0.18 * s);
                    pen.burst(&look, b.b, 1.0, 5.5, None, 0.0, 0.45);
                    pen.ring(&look, to + Vec3::Y * 0.1, Vec3::Y, 0.3, 7.0, 0.9, 0.35);
                    if look.body == Body::Smoke {
                        // BAMF: a sulphur cloud where you were AND where you are, a few embers.
                        pen.burst(&look, b.a, 1.3, 2.4, None, 0.0, 0.7);
                        pen.burst(&look, b.b, 1.0, 2.8, None, 0.0, 0.6);
                        let embers = Look { body: Body::Flame, ..look };
                        pen.burst(&embers, b.b, 0.4, 3.0, None, 0.0, 0.4);
                    }
                    if *afterimage || look.afterimages > 0 {
                        pen.ghost(from, v.yaw, v.size, look.main, look.glow * 0.6, tb.beats.ghost_life * 1.8 * look.linger);
                    }
                    me = b.b;
                    if mine && hands.active {
                        pen.rush(&look, cam, cam_fwd, 1.0, 18.0);
                        fov.write(FovPunch { degrees: 4.0 });
                    }
                    play(&mut sfx, &ops.blink_depart, b.a, 1.0, 1.0);
                    play(&mut sfx, &st.release, b.a, 1.0, 1.0);
                    play(&mut sfx, &ops.blink_arrive, b.b, 1.0, 1.0);
                    play(&mut sfx, &st.impact, b.b, 1.0, 0.9);
                }
                MoveOp::Impulse { fwd, up } => {
                    let flat = Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::NEG_Z);
                    let d = (flat * *fwd + Vec3::Y * *up).normalize_or(Vec3::Y);
                    b.dir = d;
                    b.dur = tb.beats.dash_secs * s;
                    b.r = fwd.abs().max(up.abs());
                    // The air tears open behind you.
                    pen.ring(&look, b.a, d, 0.35 * v.size, 6.5, 0.8, 0.3);
                    pen.flash(b.a - d * 0.3, 1.0 * v.size, look.main, look.glow, 0.12);
                    pen.burst(&look, b.a - d * 0.4, 0.6, 5.0, Some(-d), 0.5, 0.35);
                    if *up > fwd.abs() && v.has(bd_sim::replicate::pflags::GROUNDED)
                        && let Some(g) = ground_below(&active, v.pos, 3.0)
                    {
                        // A launch: the ground rings out under you.
                        pen.ring(&look, g + Vec3::Y * 0.1, Vec3::Y, 0.4, 8.0, 1.0, 0.35);
                    }
                    if mine && hands.active {
                        pen.rush(&look, cam, cam_fwd, (b.r / 25.0).clamp(0.4, 1.2), 18.0);
                        fov.write(FovPunch { degrees: (b.r * 0.15).clamp(1.5, 5.0) });
                    }
                    play(&mut sfx, &ops.dash, b.a, 1.0, 1.0);
                    play(&mut sfx, &st.release, b.a, 1.0, 0.9);
                }
                MoveOp::Reel { range, .. } => {
                    let (anchor, _) = aim_point(&active, eye, dir, range.clamp(1.0, 80.0));
                    b.b = anchor;
                    b.a = hand;
                    b.dur = tb.beats.reel_secs;
                    play(&mut sfx, &ops.reel, hand, 1.0, 1.0);
                    play(&mut sfx, &st.stretch, hand, 1.0, 1.0);
                }
                MoveOp::Shove { range, angle, .. } => {
                    let r = range.clamp(0.5, 20.0);
                    let half = (angle.clamp(5.0, 170.0) * 0.5).to_radians();
                    b.a = hand;
                    b.b = hand + dir * r.min(9.0);
                    b.r = r;
                    b.dur = tb.beats.limb_out + tb.beats.limb_hold + tb.beats.limb_back;
                    b.dur *= s;
                    shove_release(&mut pen, &look, &active, &mut debris, v.pos, hand, dir, r, half, tb.beats.shove_secs * s, b.seed ^ f.player.0 as u32, &mut sfx, ops, st);
                    b.flag = matches!(look.body, Body::Rubber | Body::Rigid) || r >= 4.0;
                    b.acc = half;
                    b.dur = b.dur.max(bt_shove(tb) * look.speed * 1.4);
                }
                MoveOp::SlamOnLand { radius, .. } => {
                    b.r = radius.clamp(1.0, 20.0);
                    b.dur = tb.beats.slam_wait;
                    b.phase = 0;
                    b.flag = !v.has(bd_sim::replicate::pflags::GROUNDED);
                    play(&mut sfx, &ops.slam_dread, chest(v), 1.0, 1.0);
                }
                MoveOp::Field { strength, radius, secs, at, dist } => {
                    let p = if *at == At::Me { me } else { at_point(&active, v, *at, *dist) };
                    b.a = p;
                    b.r = radius.clamp(0.5, 40.0);
                    b.dur = secs.clamp(0.3, 12.0);
                    b.flag = *at == At::Me;
                    b.acc = *strength;
                    field_release(&mut pen, &look, p, b.r, *strength);
                    play(&mut sfx, &ops.field, p, if *strength < 0.0 { 1.15 } else { 0.9 }, 1.0);
                    play(&mut sfx, &st.release, p, 1.0, 0.8);
                }
                MoveOp::Room { radius, secs } => {
                    b.a = v.pos;
                    b.r = radius.clamp(0.5, 40.0);
                    b.dur = secs.clamp(0.3, 20.0);
                    b.flag = false;
                    field_release(&mut pen, &look, v.pos + Vec3::Y, b.r, 0.0);
                    play(&mut sfx, &ops.field, v.pos, 0.75, 1.0);
                }
                MoveOp::Swap { .. } => {
                    let found = side.swaps[..side.n_swaps].iter().find(|x| x.0 == f.player).copied();
                    let Some((_, pa, pb)) = found else { continue };
                    let up = Vec3::Y;
                    b.a = pa + up;
                    b.b = pb + up;
                    b.dur = tb.beats.trace_secs * 1.2;
                    for p in [b.a, b.b] {
                        pen.flash(p, 1.8, look.core, look.glow * 1.3, 0.16);
                        pen.burst(&look, p, 0.8, 5.0, None, 0.0, 0.4);
                        pen.ring(&look, p - up * 0.9, Vec3::Y, 0.3, 6.0, 0.7, 0.3);
                        play(&mut sfx, &ops.swap, p, 1.0, 1.0);
                    }
                    pen.trace(&look, b.a, b.b, b.dur);
                    play(&mut sfx, &st.release, b.a, 1.0, 0.8);
                }
                MoveOp::Explode { radius, at, dist, .. } => {
                    let p = if *at == At::Me { me } else { at_point(&active, v, *at, *dist) };
                    b.a = p;
                    b.r = radius.clamp(0.5, 30.0);
                    b.dur = 0.3 * look.linger;
                    explode_release(&mut pen, &look, &active, p, b.r);
                    play(&mut sfx, &ops.explode, p, (1.4 - b.r * 0.04).clamp(0.6, 1.3), 1.0);
                    play(&mut sfx, &st.impact, p, 1.0, 1.0);
                    if p.distance(cam) < b.r * 2.5 {
                        shake.write(CameraShakeEvent { trauma: (0.08 + b.r * 0.02).min(0.3), dir: Vec3::ZERO });
                    }
                }
                MoveOp::Carve { length, .. } => {
                    let (end, _) = aim_point(&active, eye, dir, length.clamp(1.0, 60.0));
                    b.a = hand;
                    b.b = end;
                    b.dur = tb.beats.trace_secs;
                    pen.trace(&look, hand, end, tb.beats.trace_secs * 1.5);
                    pen.burst(&look, end, 0.8, 4.0, Some(-dir), 0.8, 0.5);
                    if let Some(g) = ground_below(&active, v.pos, 2.5) {
                        pen.crack(&look, g, dir, length.min(20.0), b.seed);
                    }
                    play(&mut sfx, &ops.carve, hand, 1.0, 1.0);
                    play(&mut sfx, &st.release, hand, 1.0, 0.8);
                }
                MoveOp::TimeStop { secs, .. } => {
                    b.a = me;
                    b.dur = secs.clamp(0.3, 10.0);
                    pen.implode(&look, b.a, 3.0, 0.2, 1.4);
                    pen.ring(&look, b.a, Vec3::Y, 0.5, 18.0, 2.0, 0.6);
                    pen.ring(&look, b.a, dir, 0.5, 14.0, 1.2, 0.5);
                    pen.flash(b.a, 3.0, look.core, look.glow, 0.25);
                    play(&mut sfx, &ops.time, b.a, 1.0, 1.0);
                    play(&mut sfx, &st.release, b.a, 0.8, 0.8);
                }
                MoveOp::Cast { .. } => {
                    b.a = hand;
                    pen.implode(&look, hand + dir * 0.3, 0.8, tb.beats.gather_secs * s, 0.6);
                    pen.flash(hand + dir * 0.4, 0.7, look.core, look.glow, 0.12);
                    play(&mut sfx, &ops.cast, hand, 1.0, 0.8);
                    play(&mut sfx, &st.release, hand, 1.0, 1.0);
                    // A limb body carries the cast: the arm stretches to the new construct.
                    let spawned = side.spawned[..side.n_spawned].iter().find(|x| x.0 == f.player).map(|x| x.1);
                    let limb_power = f.power.is_some_and(|p| p.delivery.arm);
                    if (matches!(look.body, Body::Rubber | Body::Rigid) || limb_power) && spawned.is_some() {
                        b.op = OpKind::Arm;
                        b.construct = spawned;
                        b.dur = 1.3;
                        play(&mut sfx, &st.stretch, hand, 1.0, 1.0);
                    } else {
                        b.dur = 0.0;
                    }
                }
                MoveOp::Link { range } | MoveOp::Weigh { range, .. } | MoveOp::Yank { range, .. } => {
                    let (end, _) = aim_point(&active, eye, dir, range.clamp(1.0, 60.0));
                    b.a = hand;
                    b.b = end;
                    b.dur = tb.beats.trace_secs;
                    pen.trace(&look, hand, end, tb.beats.trace_secs);
                    pen.burst(&look, end, 0.6, 3.0, None, 0.0, 0.4);
                    play(&mut sfx, &ops.beam, hand, 1.0, 1.0);
                    play(&mut sfx, &st.impact, end, 1.0, 0.7);
                }
                _ => {
                    // A body pulse: a ring rising from the feet, a burst at the chest.
                    b.a = v.pos;
                    b.dur = 0.35 * look.linger;
                    pen.ring(&look, v.pos + Vec3::Y * 0.1, Vec3::Y, 0.4, 5.0, 0.8, 0.35);
                    pen.burst(&look, me, 0.6, 3.0, None, 0.0, 0.45);
                    play(&mut sfx, &ops.pulse, me, 1.0, 0.8);
                    play(&mut sfx, &st.release, me, 1.0, 0.7);
                }
            }
            if b.dur > 0.0 {
                pool.add(b, cap);
            }
        }
        if let Some(p) = first_pose {
            if mine {
                poses.local = Some((p, 1.0));
            }
            poses.by_player.insert(f.player, (p, 0.0));
        }
    }
    // Incanted deliveries with an arm (gomu gomu no <anything>): the rubber arm to the construct.
    for k in 0..side.n_arms {
        let (player, cid) = side.arms[k];
        let Some(v) = views.player(player) else { continue };
        let def = views.player(player).and_then(|v| v.power.power()).and_then(|p| PowerTable::get().def(p));
        let style = def.and_then(|d| table.find(&d.fx)).or_else(|| table.find("rubber")).unwrap_or(table.energy);
        let look = look_of(&table, style, None, None, None, corps_rgb(v.corps));
        let hand = super::super::constructs::hand_transform(v).translation;
        play(&mut sfx, &table.def(style).sound.stretch, hand, 1.0, 1.0);
        pool.add(Beat { op: OpKind::Arm, look, player, local: Some(player) == local, construct: Some(cid), a: hand, b: hand, dur: 1.3, ..default() }, cap);
    }
    // Slams land where their explosion went off.
    for k in 0..side.n_booms {
        let (src, p, r) = side.booms[k];
        if let Some(b) = pool.beats.iter_mut().find(|b| b.live && b.op == OpKind::Slam && b.player == src && b.phase == 0) {
            b.phase = 1;
            b.b = p;
            b.r = b.r.max(r);
        }
    }
    side.clear();
}

fn bt_shove(t: &table::MoveFxTable) -> f32 {
    t.beats.shove_secs
}

fn at_point(active: &ActiveWorld, v: &crate::sim_bridge::PlayerView, at: At, dist: f32) -> Vec3 {
    let (eye, look) = eye_look(v);
    match at {
        At::Me => chest(v),
        At::Front => eye + look * dist.clamp(0.0, 30.0),
        At::Aim => aim_point(active, eye, look, 60.0).0,
    }
}

#[allow(clippy::too_many_arguments)]
fn shove_release(
    pen: &mut Pen,
    look: &Look,
    active: &ActiveWorld,
    debris: &mut MessageWriter<DebrisImpulse>,
    feet: Vec3,
    hand: Vec3,
    dir: Vec3,
    r: f32,
    half: f32,
    secs: f32,
    seed: u32,
    sfx: &mut MessageWriter<Sfx3d>,
    ops: &table::OpSounds,
    st: &table::StyleSound,
) {
    // The fist flash, then a cone shockwave: three rings sweeping out along the cone.
    pen.flash(hand + dir * 0.3, 0.9, look.core, look.glow * 1.3, 0.1);
    pen.burst(look, hand + dir * 0.4, 1.2, r / secs.max(0.05) * 0.5, Some(dir), half.min(1.2), secs * 2.5);
    for k in 1..=3 {
        let f = k as f32 / 3.0;
        let c = hand + dir * r * f;
        let ring_r = (r * f * half.tan()).clamp(0.3, r);
        let n = (6.0 + ring_r * 5.0).min(40.0);
        let (u, v) = basis(dir);
        for i in 0..n as usize {
            let a = i as f32 / n * std::f32::consts::TAU;
            let d = u * a.cos() + v * a.sin();
            let seed2 = pen.fx.rng.next_u32();
            pen.put(bd_fx::Emit {
                flags: bd_fx::PFlags::STRETCH | bd_fx::PFlags::PLAIN,
                pos: c + d * ring_r,
                vel: dir * 6.0 + d * 3.0,
                size: 0.05 * look.size * pen.scale,
                color: draw::mix3(look.core, look.main, 0.3 + 0.2 * f),
                emissive: look.glow,
                life: 0.18 + 0.08 * f,
                gravity: 0.0,
                drag: 3.0,
                spin: 3.0,
                delay: secs * f,
                seed: seed2,
                ..default()
            });
        }
    }
    play(sfx, &ops.shove, hand, (1.3 - r * 0.03).clamp(0.7, 1.3), 1.0);
    play(sfx, &st.release, hand, 1.0, 1.0);
    // The ground under the cone cracks and the loose stuff in it is flung.
    if let Some(g) = ground_below(active, feet, 2.5) {
        let reach = (r * 0.9).min(14.0);
        pen.crack(look, g, dir, reach, seed);
        if r >= 5.0 {
            pen.crack(look, g, (dir + basis(Vec3::Y).0 * 0.35 * dir.x.signum()).normalize_or(dir), reach * 0.6, seed ^ 0x9E37);
        }
        play(sfx, &ops.crack, g + dir * reach * 0.4, 1.0, (r / 8.0).clamp(0.3, 1.0));
    }
    debris.write(DebrisImpulse { center: hand + dir * r * 0.5, radius: r * 0.8, strength: (r * 1.6).min(18.0) });
}

fn field_release(pen: &mut Pen, look: &Look, p: Vec3, r: f32, strength: f32) {
    pen.flash(p, 1.4, look.core, look.glow, 0.16);
    pen.ring(look, p - Vec3::Y * 0.8, Vec3::Y, 0.4, r / 0.28, 1.4, 0.3);
    if strength > 0.0 {
        pen.implode(look, p, r, 0.3, 1.4);
    } else {
        pen.burst(look, p, 1.2, r * 3.0, None, 0.0, 0.35);
    }
}

fn explode_release(pen: &mut Pen, look: &Look, active: &ActiveWorld, p: Vec3, r: f32) {
    let k = (r / 4.0).clamp(0.5, 3.0);
    match look.body {
        Body::Void => {
            // Collapse first, then the bloom.
            pen.implode(look, p, r * 1.2, 0.15, 1.6 * k);
            pen.flash(p, r * 0.8, look.main, look.glow, 0.3);
        }
        _ => pen.flash(p, r * 0.7, look.core, look.glow * 1.2, 0.2),
    }
    pen.burst(look, p, 1.8 * k, r * 3.0, None, 0.0, 0.5);
    if matches!(look.body, Body::Flame | Body::Smoke) {
        let puff = Look { body: Body::Smoke, main: look.dark, ..*look };
        pen.burst(&puff, p, 0.9 * k, r * 1.2, None, 0.0, 0.8);
    }
    if let Some(g) = ground_below(active, p, r + 1.5) {
        pen.ring(look, g + Vec3::Y * 0.1, Vec3::Y, 0.5, r * 3.2, 1.5 * k, 0.35);
    }
}

/// Per frame: every live beat draws / follows / ends. PostUpdate, after the viewmodel hands are
/// published (the local limb grows from your real first-person fist).
#[allow(clippy::too_many_arguments)]
fn step(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    table: Res<MoveFx>,
    mut pool: ResMut<Pool>,
    mut fx: ResMut<FxParticles>,
    hands: Res<ViewmodelHands>,
    settings: Option<Res<bd_fx::FxSettings>>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    mut stats: ResMut<MoveFxStats>,
    out: (MessageWriter<Sfx3d>, MessageWriter<CameraShakeEvent>, MessageWriter<DebrisImpulse>),
) {
    let (mut sfx, mut shake, mut debris) = out;
    let Some(active) = active else { return };
    if !pool.beats.iter().any(|b| b.live) {
        stats.live = 0;
        return;
    }
    let t0 = std::time::Instant::now();
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs();
    let (cam, cam_fwd) = cams.iter().next().map_or((Vec3::ZERO, Vec3::NEG_Z), |g| (g.translation(), g.forward().as_vec3()));
    let budget = settings.as_ref().map_or(1.0, |s| s.particle_budget).clamp(0.15, 1.0);
    let tb = &table.t;
    let bt = &tb.beats;
    let pool = &mut *pool;
    for b in pool.beats.iter_mut().filter(|b| b.live) {
        b.t += dt;
        let look = b.look;
        let v = views.player(b.player).filter(|v| v.alive());
        let st = &table.def(look.style).sound;
        let scale = if b.local { bt.own_scale } else { 1.0 };
        let avoid = (b.local && hands.active).then(|| v.map_or(cam, chest));
        let mut pen = Pen { fx: &mut fx, motes: bt.motes * budget, scale, cam, avoid };
        let u = if b.dur > 0.0 { (b.t / b.dur).clamp(0.0, 1.0) } else { 1.0 };
        // Limbs read from across the map for everyone else (and in third person); your own
        // first-person arm stays slim.
        let lsize = |v: &crate::sim_bridge::PlayerView| v.size * if b.local && hands.active { 1.0 } else { 1.6 };
        // Root of a limb: your first-person fist, else the body's shoulder.
        let root = |v: &crate::sim_bridge::PlayerView| if b.local && hands.active { hands.world[1] } else { shoulder(v) };
        match b.op {
            OpKind::Blink | OpKind::Swap => {
                pen.trace_now(&look, b.a, b.b, u, b.seed.wrapping_add((b.t * 30.0) as u32));
            }
            OpKind::Impulse => {
                let Some(v) = v else {
                    b.live = false;
                    continue;
                };
                let c = chest(v);
                let grounded = v.has(bd_sim::replicate::pflags::GROUNDED);
                if b.local && hands.active && b.t < b.dur {
                    pen.rush(&look, cam, cam_fwd, 1.0 - u, 3.0);
                }
                if b.t < b.dur {
                    // Streak ribbons at the shoulders and feet + a trail of body motes.
                    let seg = c - b.last;
                    if seg.length_squared() > 0.0025 {
                        let (side, _) = basis(Vec3::new(seg.x, 0.0, seg.z).normalize_or(b.dir));
                        let fade = 1.0 - u;
                        for o in [side * 0.3, -side * 0.3, Vec3::Y * -0.75] {
                            let o = o * v.size;
                            let col = if look.body == Body::Streak { look.core } else { look.main };
                            let thick = if b.local { 0.07 } else { 0.12 };
                            pen.beam(b.last + o, c + o, thick * look.size * (0.4 + 0.6 * fade), col, look.glow * (0.6 + fade), 0.22 * look.linger);
                        }
                        if look.body == Body::Streak || look.body == Body::Bolt {
                            zig(pen.fx, b.last, c, 4, 0.25 + look.jitter, b.seed.wrapping_add((b.t * 60.0) as u32), 0.025, look.main, look.glow * 1.5 * fade);
                        }
                        pen.mote(&look, c - seg.normalize_or(b.dir) * 0.3, -seg / dt.max(1e-3) * 0.08, 0.35, u);
                    }
                    b.acc += dt;
                    if look.afterimages > 0 && b.acc >= bt.ghost_every && b.phase < look.afterimages {
                        b.acc = 0.0;
                        b.phase += 1;
                        let k = 1.0 - b.phase as f32 / (look.afterimages as f32 + 1.0);
                        pen.ghost(v.pos, v.yaw, v.size, draw::mix3(look.main, look.core, 0.2), look.glow * 0.5 * k, bt.ghost_life * look.linger);
                    }
                }
                if !grounded {
                    b.flag = true;
                }
                if b.flag && grounded {
                    // The landing skid: grit sprays along the slide, a short scar in the ground.
                    let vel = v.vel;
                    let flat = Vec3::new(vel.x, 0.0, vel.z);
                    let sp = flat.length();
                    if sp > 3.0 {
                        let d = flat / sp;
                        let grit = Look { body: Body::Grain, ..look };
                        pen.burst(&grit, v.pos + Vec3::Y * 0.15, 0.9, sp * 0.4, Some((d + Vec3::Y * 0.6).normalize()), 0.5, 0.5);
                        pen.crack(&look, v.pos, d, (sp * 0.12).min(4.0), b.seed);
                        play(&mut sfx, &tb.sounds.skid, v.pos, 1.0, (sp / 20.0).clamp(0.3, 1.0));
                    }
                    b.live = false;
                    continue;
                }
                b.last = c;
                if b.t > b.dur.max(1.4) {
                    b.live = false;
                    continue;
                }
                continue;
            }
            OpKind::Reel => {
                let Some(v) = v else {
                    b.live = false;
                    continue;
                };
                let from = root(v);
                let out_t = bt.limb_out * 2.0 * look.speed;
                let arrived = chest(v).distance(b.b) < bt.reel_arrive;
                if b.phase == 0 && b.t >= out_t {
                    b.phase = 1;
                    pen.burst(&look, b.b, 0.6, 3.0, None, 0.0, 0.35);
                    pen.flash(b.b, 0.8, look.core, look.glow, 0.1);
                    play(&mut sfx, &tb.sounds.reel_anchor, b.b, 1.0, 1.0);
                    play(&mut sfx, &st.impact, b.b, 1.0, 0.8);
                }
                if b.phase == 1 && (arrived || b.t > bt.reel_secs) {
                    b.phase = 2;
                    b.acc = b.t;
                    play(&mut sfx, &st.exit, from, 1.0, 1.0);
                }
                let tip = match b.phase {
                    0 => from.lerp(b.b, ease_out((b.t / out_t).clamp(0.0, 1.0))),
                    1 => b.b,
                    _ => {
                        let k = ((b.t - b.acc) / (bt.limb_back * look.speed)).clamp(0.0, 1.0);
                        if k >= 1.0 {
                            b.live = false;
                            continue;
                        }
                        b.b.lerp(from, ease_in_back(k))
                    }
                };
                limb(&mut pen, &look, from, tip, b.t, b.phase, lsize(v), b.seed);
                continue;
            }
            OpKind::Shove => {
                // The cone itself, for a beat: its edges and the wave front racing out.
                let cone_t = bt.shove_secs * look.speed * 1.4;
                if b.t < cone_t {
                    let k = ease_out(b.t / cone_t);
                    let fade = 1.0 - b.t / cone_t;
                    let reach = b.r.min(14.0) * k;
                    let front = b.a + b.dir * reach;
                    let rr = (reach * b.acc.tan()).clamp(0.1, 12.0);
                    let (u1, v1) = basis(b.dir);
                    circle(pen.fx, front, b.dir, rr, 20, 0.05, draw::mix3(look.core, look.main, 0.4), look.glow * 1.3 * fade, b.t * 3.0);
                    for e in 0..6 {
                        let a = e as f32 / 6.0 * std::f32::consts::TAU + 0.3;
                        let edge = front + (u1 * a.cos() + v1 * a.sin()) * rr;
                        pen.fx.immediate_segment(b.a.lerp(edge, 0.25), edge, 0.025, look.main, look.glow * fade, bd_fx::PFlags::PLAIN);
                    }
                }
                if !b.flag {
                    if b.t >= cone_t {
                        b.live = false;
                    }
                    continue;
                }
                let Some(v) = v else {
                    b.live = false;
                    continue;
                };
                // The limb / spectral fist flies to the end of the cone and snaps back.
                let from = root(v);
                let (_, look_dir) = eye_look(v);
                let far = from + look_dir * b.r.min(9.0);
                let (o, h) = (bt.limb_out * look.speed, bt.limb_hold * look.speed);
                let tip = if b.t < o {
                    from.lerp(far, ease_out(b.t / o))
                } else if b.t < o + h {
                    far
                } else {
                    let k = ((b.t - o - h) / (bt.limb_back * look.speed)).clamp(0.0, 1.0);
                    if b.phase == 0 {
                        b.phase = 1;
                        play(&mut sfx, &st.exit, from, 1.0, 0.9);
                    }
                    far.lerp(from, ease_in_back(k))
                };
                let wob_phase = if b.t < o + h { 0 } else { 2 };
                limb(&mut pen, &look, from, tip, b.t, wob_phase, lsize(v), b.seed);
            }
            OpKind::Arm => {
                let (Some(v), Some(c)) = (v, b.construct.and_then(|c| views.construct(c))) else {
                    // The construct is gone: snap back from where it was.
                    if b.phase < 2 {
                        b.phase = 2;
                        b.acc = b.t;
                        if let Some(v) = v {
                            play(&mut sfx, &st.exit, root(v), 1.0, 1.0);
                        }
                    }
                    if let Some(v) = v {
                        let k = ((b.t - b.acc) / (bt.limb_back * look.speed)).clamp(0.0, 1.0);
                        if k < 1.0 {
                            let from = root(v);
                            limb(&mut pen, &look, from, b.b.lerp(from, ease_in_back(k)), b.t, 2, lsize(v), b.seed);
                            continue;
                        }
                    }
                    b.live = false;
                    continue;
                };
                let from = root(v);
                let tip = c.pos;
                let dist = tip.distance(from);
                b.b = tip;
                if dist > 1.6 {
                    b.flag = true;
                }
                if b.phase < 2 && (b.t > b.dur || (b.flag && dist < 1.1)) {
                    b.phase = 2;
                    b.acc = b.t;
                    play(&mut sfx, &st.exit, from, 1.0, 1.0);
                }
                if b.phase == 2 {
                    let k = ((b.t - b.acc) / (bt.limb_back * look.speed)).clamp(0.0, 1.0);
                    if k >= 1.0 {
                        b.live = false;
                        continue;
                    }
                    limb(&mut pen, &look, from, tip.lerp(from, ease_in_back(k)), b.t, 2, lsize(v), b.seed);
                } else {
                    limb(&mut pen, &look, from, tip, b.t, 1, lsize(v), b.seed);
                }
                continue;
            }
            OpKind::Slam => {
                let Some(v) = v else {
                    b.live = false;
                    continue;
                };
                let grounded = v.has(bd_sim::replicate::pflags::GROUNDED);
                if !grounded {
                    b.flag = true;
                }
                if b.phase == 0 && b.flag && grounded && b.t > 0.08 {
                    b.phase = 1;
                    b.b = v.pos;
                }
                if b.phase == 0 {
                    // The dread: a dark ring under you that tightens as you come down.
                    if let Some(g) = ground_below(&active, v.pos, 40.0) {
                        let h = (v.pos.y - g.y).max(0.0);
                        if b.acc == 0.0 {
                            b.acc = h.max(1.0);
                        }
                        let close = 1.0 - (h / b.acc).clamp(0.0, 1.0);
                        let r = b.r * (1.15 - 0.5 * close);
                        let p = g + Vec3::Y * 0.06;
                        let spin = now * (1.5 + close * 4.0);
                        circle(pen.fx, p, Vec3::Y, r, 28, 0.07 + 0.05 * close, look.dark, 0.3, spin);
                        circle(pen.fx, p, Vec3::Y, r * 0.55, 20, 0.05, look.main, look.glow * (0.4 + close), -spin);
                        // A drop line from you to the mark (dashed).
                        let top = v.pos;
                        let n = 8;
                        for i in (0..n).step_by(2) {
                            let a = top.lerp(p, i as f32 / n as f32);
                            let c = top.lerp(p, (i + 1) as f32 / n as f32);
                            pen.fx.immediate_segment(a, c, 0.04, look.main, look.glow * 0.5 * close, bd_fx::PFlags::PLAIN);
                        }
                        if (b.t * 20.0) as u32 != ((b.t - dt) * 20.0) as u32 {
                            let a = pen.fx.rng.f() * std::f32::consts::TAU;
                            let q = p + Vec3::new(a.cos(), 0.0, a.sin()) * r;
                            let grit = Look { body: Body::Grain, ..look };
                            pen.mote(&grit, q, Vec3::Y * (1.5 + 2.0 * close), 0.4, 0.0);
                        }
                    }
                    if b.t > b.dur {
                        b.live = false;
                    }
                    continue;
                }
                // Landed: crater ring, dust wall, radial cracks, a flash, shake nearby.
                let p = b.b;
                let r = b.r;
                pen.flash(p + Vec3::Y * 0.4, r * 0.6, look.core, look.glow, 0.2);
                pen.ring(&look, p + Vec3::Y * 0.15, Vec3::Y, 0.5, r * 3.0, 2.0, 0.4);
                pen.dust_wall(&look, p, r * 0.75, 2.2 + r * 0.25, 1.6);
                pen.burst(&look, p + Vec3::Y * 0.3, 1.2, r * 2.0, Some(Vec3::Y), 0.9, 0.6);
                for k in 0..6 {
                    let a = k as f32 / 6.0 * std::f32::consts::TAU + (b.seed % 100) as f32 * 0.01;
                    pen.crack(&look, p, Vec3::new(a.cos(), 0.0, a.sin()), r * (0.7 + 0.3 * ((k * 7 % 5) as f32 / 5.0)), b.seed.wrapping_add(k));
                }
                debris.write(DebrisImpulse { center: p, radius: r * 1.4, strength: 10.0 + r });
                play(&mut sfx, &tb.sounds.slam_land, p, (1.2 - r * 0.03).clamp(0.6, 1.2), 1.0);
                play(&mut sfx, &st.impact, p, 1.0, 1.0);
                let dcam = p.distance(cam);
                if b.local || dcam < r * 3.0 {
                    shake.write(CameraShakeEvent { trauma: (0.12 + r * 0.015).min(0.32) * if b.local { 1.0 } else { (1.0 - dcam / (r * 3.0)).max(0.2) }, dir: Vec3::ZERO });
                }
                b.live = false;
                continue;
            }
            OpKind::Field | OpKind::Room => {
                let center = if b.flag { v.map_or(b.a, chest) } else { b.a };
                let fade_in = (b.t / 0.25).min(1.0);
                let fade_out = ((b.dur - b.t) / 0.4).clamp(0.0, 1.0);
                let k = fade_in * fade_out;
                // The boundary shimmers: a slow rotating ground rim + a faint band.
                let ground = ground_below(&active, center, b.r + 2.0).map_or(center.y - 1.0, |g| g.y) + 0.06;
                let rim = Vec3::new(center.x, ground, center.z);
                let pulse = 0.6 + 0.4 * (now * 4.0 + b.seed as f32).sin();
                circle(pen.fx, rim, Vec3::Y, b.r, 40, 0.05, look.main, look.glow * 0.5 * k * pulse, now * 0.4);
                if b.op == OpKind::Room {
                    circle(pen.fx, rim + Vec3::Y * 0.3, Vec3::Y, b.r, 40, 0.025, look.core, look.glow * 0.3 * k, -now * 0.3);
                }
                // Anyone crossing it: a flare where they cross.
                for p in &views.players {
                    if !p.alive() || p.id.0 >= 64 {
                        continue;
                    }
                    let bit = 1u64 << p.id.0;
                    let c = chest(p);
                    let inside = c.distance(center) < b.r;
                    let was = b.mask & bit != 0;
                    if b.t > 0.1 && inside != was && !(b.flag && p.id == b.player) {
                        let n = (c - center).normalize_or(Vec3::X);
                        let at = center + n * b.r;
                        pen.flash(at, 0.9, look.core, look.glow, 0.12);
                        pen.ring(&look, at, n, 0.2, 3.0, 0.5, 0.25);
                        play(&mut sfx, &tb.sounds.cross, at, if inside { 0.9 } else { 1.2 }, 0.7);
                    }
                    if inside {
                        b.mask |= bit;
                    } else {
                        b.mask &= !bit;
                    }
                }
            }
            OpKind::TimeStop => {
                // The tick: a slow ripple off the caster twice a second; two hands sweeping round.
                let Some(v) = v else {
                    b.live = false;
                    continue;
                };
                let c = chest(v);
                if (b.t * 2.0) as u32 != ((b.t - dt) * 2.0) as u32 {
                    pen.ring(&look, c, Vec3::Y, 0.6, 9.0, 0.8, 0.5);
                }
                let fade = ((b.dur - b.t) / 0.4).clamp(0.0, 1.0) * (b.t / 0.2).min(1.0);
                let (u1, v1) = basis(Vec3::Y);
                let a1 = b.t * 6.0;
                let a2 = b.t * 0.6;
                let h1 = c + (u1 * a1.cos() + v1 * a1.sin()) * 1.1 * v.size;
                let h2 = c + (u1 * a2.cos() + v1 * a2.sin()) * 0.7 * v.size;
                pen.fx.immediate_segment(c, h1, 0.03, look.core, look.glow * fade, bd_fx::PFlags::PLAIN);
                pen.fx.immediate_segment(c, h2, 0.05, look.main, look.glow * fade, bd_fx::PFlags::PLAIN);
                circle(pen.fx, c, Vec3::Y, 1.2 * v.size, 24, 0.02, look.main, look.glow * 0.4 * fade, 0.0);
            }
            OpKind::Explode => {
                if look.body == Body::Bolt && b.t < 0.2 {
                    for k in 0..5u32 {
                        let a = k as f32 * 1.26 + b.seed as f32 * 0.001;
                        let d = Vec3::new(a.cos(), 0.3 * ((k % 3) as f32 - 1.0), a.sin());
                        zig(pen.fx, b.a, b.a + d * b.r * 0.8, 6, 0.3 + look.jitter, b.seed.wrapping_add(k * 13 + (b.t * 40.0) as u32), 0.04, look.core, look.glow * 2.0);
                    }
                }
            }
            OpKind::Cast | OpKind::Carve | OpKind::Reach | OpKind::Pulse => {
                pen.trace_now(&look, b.a, b.b, u, b.seed.wrapping_add((b.t * 30.0) as u32));
            }
        }
        if b.t >= b.dur {
            b.live = false;
        }
    }
    let live = pool.beats.iter().filter(|b| b.live).count();
    *stats = MoveFxStats { live, peak: pool.peak, started: pool.started, step_us: t0.elapsed().as_secs_f32() * 1e6 };
}

/// Draw a limb / line of the style's body from `from` to `tip`. `phase`: 0 extending, 1 holding,
/// 2 snapping back (rubber wobble swells on the snap).
#[allow(clippy::too_many_arguments)]
fn limb(pen: &mut Pen, look: &Look, from: Vec3, tip: Vec3, t: f32, phase: u8, size: f32, seed: u32) {
    let thick = 0.13 * size * look.size;
    match look.body {
        Body::Rubber => {
            // Elastic: wobble decays while held, rings hard on the snap-back.
            let wob = look.wobble.max(0.05) * match phase {
                0 => 0.5,
                1 => 0.6 * (-t * 3.0).exp() + 0.15,
                _ => 1.0,
            };
            rubber_limb(pen.fx, from, tip, thick * 1.2, wob, t * 3.5, look.main, look.core, look.glow);
        }
        Body::Rigid => {
            let ext = match phase {
                0 => (t * 9.0).min(1.0),
                _ => 1.0,
            };
            rigid_pole(pen.fx, from, tip, thick * 1.05, ext, look.dark, look.main, look.glow);
        }
        Body::Bolt | Body::Streak => {
            zig(pen.fx, from, tip, 10, 0.12 + look.jitter * 0.5, seed.wrapping_add((t * 40.0) as u32), 0.05, look.core, look.glow * 2.0);
            zig(pen.fx, from, tip, 8, 0.2 + look.jitter, seed.wrapping_add((t * 40.0) as u32 ^ 0x77), 0.025, look.main, look.glow * 1.3);
        }
        Body::Smoke | Body::Liquid | Body::Void => {
            // A tendril: dark body, bright seam, drips / puffs shed along it.
            rubber_limb(pen.fx, from, tip, thick, 0.08 + look.wobble, t * 2.0, look.dark, look.main, look.glow * 0.6);
            if pen.fx.rng.f() < 0.35 {
                let p = from.lerp(tip, pen.fx.rng.f());
                pen.mote(look, p, Vec3::ZERO, 0.4, 0.0);
            }
        }
        _ => light_limb(pen.fx, from, tip, thick, look.main, look.core, look.glow),
    }
}

fn ease_out(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    1.0 - (1.0 - x) * (1.0 - x) * (1.0 - x)
}

/// Snap-back: a tiny wind-up then a hard pull (band snap).
fn ease_in_back(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let c = 1.9;
    (c + 1.0) * x * x * x - c * x * x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> table::MoveFxTable {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/tuning/move_fx.ron");
        let text = std::fs::read_to_string(&path).expect("assets/tuning/move_fx.ron");
        table::parse(&text).expect("move_fx.ron parses")
    }

    #[test]
    fn move_fx_shipped_table_has_the_core_styles() {
        let fx = MoveFx::new(shipped());
        for k in ["energy", "rubber", "rigid", "smoke", "lightning", "speedforce", "shadow", "ink", "holy", "void", "blood", "paper", "sand", "water", "ice", "fire", "wind", "steam", "crystal", "hardlight", "chi", "cursed", "glitch"] {
            assert!(fx.find(k).is_some(), "style {k} missing from move_fx.ron");
        }
        assert_eq!(fx.def(fx.find("rubber").unwrap()).body, Body::Rubber);
        assert_eq!(fx.def(fx.find("rigid").unwrap()).body, Body::Rigid);
        assert_eq!(fx.find("Speed Force"), fx.find("speedforce"), "keys ignore case / spaces");
        // Every alias lands on a real style.
        for (a, t) in &fx.t.aliases {
            assert!(fx.find(t).is_some(), "alias {a} → {t}: no such style");
        }
        for (e, k) in &fx.t.elements {
            assert!(fx.find(k).is_some(), "element {e:?} → {k}: no such style");
        }
    }

    /// Every `fx:` key written in the powers data names a style (typos fall back to energy
    /// silently in play — this is where they are caught).
    #[test]
    fn move_fx_every_fx_key_in_the_powers_data_exists() {
        let fx = MoveFx::new(shipped());
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut files = vec![root.join("powers.ron")];
        if let Ok(rd) = std::fs::read_dir(root.join("powers")) {
            files.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ron")));
        }
        let mut bad = Vec::new();
        for f in files {
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            for (i, line) in text.lines().enumerate() {
                let line = line.split("//").next().unwrap_or("");
                let mut rest = line;
                while let Some(k) = rest.find("fx: \"") {
                    let s = &rest[k + 5..];
                    let Some(end) = s.find('"') else { break };
                    let key = &s[..end];
                    // `fx:` but not `…_fx:` / `crit_fx:` etc.
                    let before = rest[..k].chars().last();
                    if !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && !key.is_empty() && fx.find(key).is_none() {
                        bad.push(format!("{}:{}: fx \"{key}\"", f.display(), i + 1));
                    }
                    rest = &s[end..];
                }
            }
        }
        assert!(bad.is_empty(), "unknown move fx keys (add them to assets/tuning/move_fx.ron styles / aliases):\n{}", bad.join("\n"));
    }

    #[test]
    fn move_fx_style_resolution_order() {
        let fx = MoveFx::new(shipped());
        let mut mv = MoveDef { ops: vec![MoveOp::Shove { range: 4.0, angle: 60.0, damage: 5.0, knockback: 10.0, element: Element::Fire, up: 0.0 }], ..default() };
        let fire = resolve_look(&fx, &mv, None, [1.0; 3]);
        assert_eq!(fire.style, fx.for_element(Element::Fire).unwrap(), "element fallback");
        mv.fx = "rubber".into();
        assert_eq!(resolve_look(&fx, &mv, None, [1.0; 3]).body, Body::Rubber, "the move's own key wins");
        mv.fx = "no such style".into();
        assert_eq!(resolve_look(&fx, &mv, None, [1.0; 3]).style, fire.style, "an unknown key falls through");
    }

    #[test]
    fn snap_back_eases_past_the_start_then_home() {
        assert!(ease_in_back(0.15) < 0.0, "a tiny wind-up (the band pulls taut)");
        assert!((ease_in_back(1.0) - 1.0).abs() < 1e-5);
        assert!(ease_out(0.5) > 0.5);
    }

    #[test]
    fn every_op_has_a_beat_and_a_pose() {
        let ops = [
            MoveOp::Impulse { fwd: 1.0, up: 0.0 },
            MoveOp::Blink { dist: 3.0, afterimage: false },
            MoveOp::Reel { range: 5.0, speed: 5.0 },
            MoveOp::TimeStop { secs: 1.0, speed: 2.0 },
            MoveOp::Transform,
            MoveOp::Tier(1),
        ];
        for op in &ops {
            let k = OpKind::of(op);
            let _ = MovePose::of(k);
        }
        assert_eq!(OpKind::of(&MoveOp::Transform), OpKind::Pulse);
    }
}
