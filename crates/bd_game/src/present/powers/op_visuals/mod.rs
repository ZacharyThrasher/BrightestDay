//! OP VISUALS — every generic-op event (`PowerEvent::Op(OpEvent::…)`, bd_sim `powers/ops/`) gets a
//! readable, dramatic beat in the move's own colour / style (`assets/tuning/move_fx.ron`, resolved
//! exactly like the move grammar: move `fx` → power `fx` → element → …), following the grammar
//! gather → release → world reaction → clean exit.
//!
//! The move grammar (`movefx`) presents each op through its classic beat (a pounce dashes, a zone
//! is a field…); this module draws what the op's OWN events say, from a handful of shared
//! primitives ([`prims`]):
//!
//! | primitive | used by |
//! |---|---|
//! | target MARKER (shape by kind, pulse by time left, on the target for everyone + a small glyph low in your own view when it's on YOU) | Tag, DelayedHit, LockOn, Lockout, GazeStun / Afflict daze, Contagion brand, MarkRecall seal, Capture crown, Grab clamp, Pounce / Seeker "hunted" |
//! | TELEGRAPH (ground ring that fills / cut lines that grow / sky column / sinking halo / pucker) | Salvo (`Telegraph`), Leap landing, Surge path |
//! | TETHER (travels out, latches, reels, snaps back; chain + kunai, energy lead, bungee, spirit) | Hook, Haul, TagStrike pull, Capture possession |
//! | STRIKE by style (clean slash that opens space · bolt from the sky · pillar of light · void pop · body bloom) | Salvo (`Strike`), DelayedHit burst, TagStrike, Seeker / Pounce / Ram hits |
//! | STATE AURA | Empower, Evade window, Counter / Guard / Deflect stances, Phase, Mend over time, LockOn glare, Rush |
//! | RESOURCE pulse | Mend |
//! | op RAY (thick beam in the move's colour on every leg + a flare at each bounce) | Zap (BounceBeam legs, Afflict, Disarm), LockedOut |
//! | extras | Wave crescent, Drill orb + grind, Leap arc, Roulette wheel, lone portal end, Rewind ghosts |
//!
//! Words (onomatopoeia) follow the style — a cut says SHING (or nothing), never KA-BOOM: see
//! [`strike_word`] and [`RecentMoves::delayed_word`] (the hook `impact.rs` uses for a delayed
//! blast's explosion word). Word placement / budget is the TEXT CLUTTER worker's.
//!
//! Cost: one pre-allocated pool of [`MAX_FX`] entries + [`MAX_LOCKS`] lock channels, immediate
//! segments / GPU particles only (no entities, meshes or materials), nothing allocated per frame
//! (the event buffer keeps its capacity). Drawn for every player; nothing is drawn in your lens and
//! nothing sits over the crosshair (your own markers go low in the view).

mod dev;
pub mod prims;

use self::prims::{AuraKind, MarkKind, StrikeKind, TetherBody, View};
use super::movefx::draw::{Look, Pen, zig};
use super::movefx::table::{Body, MoveFx};
use super::movefx::{look_of, resolve_look, style_word};
use super::super::sound::Sfx3d;
use super::super::{PresentSet, bv, corps_rgb, gv};
use crate::camera::{CameraUpdateSet, PlayerCamera};
use crate::juice::{CameraShakeEvent, HitStop};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, PlayerView, WorldViews};
use crate::state::AppState;
use crate::viewmodel::{HandsPublishSet, ViewmodelHands};
use bd_core::mechanics::Element;
use bd_core::{ConstructId, PlayerId};
use bd_fx::{FxParticles, SpawnOnomatopoeia};
use bd_sim::GameEvent;
use bd_sim::powers::ops::live::ToolboxFx;
use bd_sim::powers::ops::{GrabPhase, HaulPhase, HookPhase, LockPhase, OpEvent, PouncePhase, RamPhase, SeekPhase, StanceKind, SurgePhase};
use bd_sim::powers::{MoveDef, MoveOp, PowerEvent, PowerTable};
use bd_sim::status::StatusKind;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use std::f32::consts::PI;

/// Pool size (live op visuals at once; the oldest is recycled past it).
pub const MAX_FX: usize = 128;
/// Concurrent lock-on channels.
pub const MAX_LOCKS: usize = 8;
/// This module's share of the shared immediate region per frame (bd_fx `IMM_CAP` = 8192): a burst
/// of markers / tethers can never starve the other modules' lines (arcs, lightning, guns, movefx).
pub const IMM_BUDGET: usize = 2600;
/// Line pixel floor (thickness ≥ this × distance): ~2 px at 1080p — a 3 cm line at 40 m is sub-pixel.
pub const LINE_FLOOR: f32 = 0.0022;
const LOOKS: usize = 16;

/// What an op event is drawn with (the mapping table; [`visual_of`] is exhaustive, so a new
/// `OpEvent` variant fails to compile until it is mapped).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpVisual {
    Tether,
    Marker,
    Telegraph,
    Strike,
    Aura,
    Pulse,
    Ray,
    Crescent,
    Orb,
    Arc,
    Wheel,
    Portal,
    Ghosts,
    /// A one-shot burst / flash / ring moment.
    Burst,
    /// Presented elsewhere (the move title / fusion windup of a twisted move).
    Elsewhere,
}

/// The primary visual of an op event.
pub fn visual_of(e: &OpEvent) -> OpVisual {
    match e {
        OpEvent::Rewound { .. } => OpVisual::Ghosts,
        OpEvent::Ram { .. } => OpVisual::Strike,
        OpEvent::Hook { .. } => OpVisual::Tether,
        OpEvent::Grab { .. } => OpVisual::Marker,
        OpEvent::Stance { .. } => OpVisual::Aura,
        OpEvent::Parried { .. } => OpVisual::Burst,
        OpEvent::LockOn { .. } => OpVisual::Marker,
        OpEvent::LockedOut { .. } => OpVisual::Marker,
        OpEvent::Zone { .. } => OpVisual::Burst,
        OpEvent::Seek { .. } => OpVisual::Marker,
        OpEvent::Haul { .. } => OpVisual::Tether,
        OpEvent::Leap(_) => OpVisual::Arc,
        OpEvent::Surge { .. } => OpVisual::Telegraph,
        OpEvent::Roulette { .. } => OpVisual::Wheel,
        OpEvent::Pounce { .. } => OpVisual::Marker,
        OpEvent::Telegraph { .. } => OpVisual::Telegraph,
        OpEvent::Strike { .. } => OpVisual::Strike,
        OpEvent::Mended { .. } => OpVisual::Pulse,
        OpEvent::Evade { .. } => OpVisual::Aura,
        OpEvent::PortalShot { .. } => OpVisual::Portal,
        OpEvent::Empowered { .. } => OpVisual::Aura,
        OpEvent::Captured { .. } => OpVisual::Marker,
        OpEvent::Zap { .. } => OpVisual::Ray,
        OpEvent::Marked { .. } => OpVisual::Marker,
        OpEvent::Tagged { .. } => OpVisual::Marker,
        OpEvent::Toolbox { fx, .. } => match fx {
            ToolboxFx::DrillStart | ToolboxFx::DrillLatch | ToolboxFx::DrillRelease | ToolboxFx::DrillEnd => OpVisual::Orb,
            ToolboxFx::Mark => OpVisual::Marker,
            ToolboxFx::Recall | ToolboxFx::Warp => OpVisual::Ray,
            ToolboxFx::Copy | ToolboxFx::Flash | ToolboxFx::Banish => OpVisual::Burst,
            ToolboxFx::Gaze | ToolboxFx::Brand | ToolboxFx::BrandEnd => OpVisual::Marker,
            ToolboxFx::PhaseOn | ToolboxFx::PhaseOff | ToolboxFx::Rush => OpVisual::Aura,
            ToolboxFx::Wave => OpVisual::Crescent,
        },
        OpEvent::Twisted { .. } => OpVisual::Elsewhere,
    }
}

/// The word a strike says, by how it lands: a cut is SHING (never KA-BOOM), a bolt crackles, a void
/// VWUMPs; a blast says its style's word through the explosion (see [`RecentMoves`]).
pub fn strike_word(kind: StrikeKind, style_word: Option<&'static str>) -> Option<&'static str> {
    match kind {
        StrikeKind::Slash | StrikeKind::Holy => Some("shing"),
        StrikeKind::Bolt => Some("electric"),
        StrikeKind::Void => Some("vwump"),
        StrikeKind::Blast => style_word,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Marker(MarkKind),
    Telegraph(StrikeKind),
    /// A bolt re-struck for a beat (`a` sky → `b` ground).
    Bolt,
    Tether(TetherBody),
    Aura(AuraKind),
    /// A flying crescent `a` → `b` over `dur`, `r` = width, `flag` = a flat disc.
    Crescent,
    /// A hand-held orb (`flag` = a crackling hand), `phase` 1 = grinding at `target`.
    Orb,
    /// A leap: `a` → `b`, apex `r`.
    Arc,
    /// A surge front from `a` along `dir`, `r` = width, `acc` = speed.
    Surge,
    /// A roulette: `n` of, `phase` pick.
    Wheel,
    /// A lone portal end at `a` (normal `dir`), `flag` = orange.
    Portal,
    /// A zone's inside (`a`, `r`), `n` = status byte (255 none).
    Zone,
    /// A dashed sightline owner eye → target.
    Sight,
    /// A homing thrown thing's trail (follows `construct`; binds to the owner's newest one near `a`).
    Trail,
    /// A gaze: twin beams out of the owner's eyes along their live look (`r` = reach, `acc` = cone
    /// half-angle, radians: fanned edge beams for a cone move).
    Gaze,
}

#[derive(Clone, Copy, Debug)]
struct Fx {
    live: bool,
    kind: Kind,
    look: Look,
    owner: PlayerId,
    target: Option<PlayerId>,
    construct: Option<ConstructId>,
    t: f32,
    dur: f32,
    a: Vec3,
    b: Vec3,
    dir: Vec3,
    r: f32,
    acc: f32,
    seed: u32,
    phase: u8,
    n: u8,
    flag: bool,
}

impl Default for Fx {
    fn default() -> Self {
        Self {
            live: false,
            kind: Kind::Sight,
            look: Look::default(),
            owner: PlayerId(0),
            target: None,
            construct: None,
            t: 0.0,
            dur: 0.0,
            a: Vec3::ZERO,
            b: Vec3::ZERO,
            dir: Vec3::NEG_Z,
            r: 0.0,
            acc: 0.0,
            seed: 0,
            phase: 0,
            n: 0,
            flag: false,
        }
    }
}

#[derive(Resource)]
struct Pool {
    fx: Vec<Fx>,
    seed: u32,
    peak: usize,
    started: u64,
}

impl Default for Pool {
    fn default() -> Self {
        Self { fx: Vec::with_capacity(MAX_FX), seed: 0x0B5E_55ED, peak: 0, started: 0 }
    }
}

impl Pool {
    fn add(&mut self, mut f: Fx) -> usize {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        if f.seed == 0 {
            f.seed = self.seed;
        }
        f.live = true;
        f.t = 0.0;
        self.started += 1;
        let k = if let Some(k) = self.fx.iter().position(|x| !x.live) {
            self.fx[k] = f;
            k
        } else if self.fx.len() < MAX_FX {
            self.fx.push(f);
            self.fx.len() - 1
        } else {
            // Recycle the entry nearest its end.
            let k = (0..self.fx.len()).max_by(|a, b| self.fx[*a].t_frac().total_cmp(&self.fx[*b].t_frac())).unwrap_or(0);
            self.fx[k] = f;
            k
        };
        self.peak = self.peak.max(self.fx.iter().filter(|x| x.live).count());
        k
    }

    fn kill(&mut self, pred: impl Fn(&Fx) -> bool) {
        for f in self.fx.iter_mut().filter(|f| f.live) {
            if pred(f) {
                f.live = false;
            }
        }
    }

    fn find(&mut self, pred: impl Fn(&Fx) -> bool) -> Option<&mut Fx> {
        self.fx.iter_mut().find(|f| f.live && pred(f))
    }
}

impl Fx {
    fn t_frac(&self) -> f32 {
        if self.dur > 0.0 { self.t / self.dur } else { 1.0 }
    }
}

/// A lock-on channel (client-side estimate of the host's growing marks, synced to its `Mark` beats).
#[derive(Clone, Copy, Debug)]
struct Chan {
    live: bool,
    owner: PlayerId,
    t: f32,
    secs: f32,
    range: f32,
    cos: f32,
    ids: [Option<PlayerId>; 8],
    fr: [f32; 8],
    look: Look,
}

impl Default for Chan {
    fn default() -> Self {
        Self { live: false, owner: PlayerId(0), t: 0.0, secs: 1.0, range: 40.0, cos: 0.8, ids: [None; 8], fr: [0.0; 8], look: Look::default() }
    }
}

impl Chan {
    fn slot(&mut self, id: PlayerId) -> Option<usize> {
        if let Some(k) = self.ids.iter().position(|x| *x == Some(id)) {
            return Some(k);
        }
        let k = self.ids.iter().position(|x| x.is_none())?;
        self.ids[k] = Some(id);
        self.fr[k] = 0.0;
        Some(k)
    }
}

#[derive(Resource, Default)]
struct Locks([Chan; MAX_LOCKS]);

/// The last move each player fired (its look + def), for the op events that follow it.
#[derive(Resource)]
struct Looks {
    e: [(PlayerId, Look, f32, Option<&'static MoveDef>); LOOKS],
    n: usize,
}

impl Default for Looks {
    fn default() -> Self {
        Self { e: [(PlayerId(u8::MAX), Look::default(), -100.0, None); LOOKS], n: 0 }
    }
}

impl Looks {
    fn set(&mut self, p: PlayerId, look: Look, now: f32, mv: Option<&'static MoveDef>) {
        if let Some(e) = self.e.iter_mut().find(|e| e.0 == p) {
            *e = (p, look, now, mv);
            return;
        }
        let k = self.n % LOOKS;
        self.e[k] = (p, look, now, mv);
        self.n += 1;
    }

    fn get(&self, p: PlayerId) -> Option<(Look, Option<&'static MoveDef>)> {
        self.e.iter().find(|e| e.0 == p).map(|e| (e.1, e.3))
    }
}

/// The last move each player fired and the word its blasts say (the explosion-word hook for
/// DELAYED blasts: a Salvo / DelayedHit / Seeker / TagStrike explosion lands seconds after its
/// `Fired`, so `impact.rs` can't see the move in the same batch — it asks here).
#[derive(Resource, Default)]
pub struct RecentMoves {
    e: [(Option<PlayerId>, f64, Option<&'static str>, bool); LOOKS],
    n: usize,
}

impl RecentMoves {
    pub fn note(&mut self, p: PlayerId, now: f64, word: Option<&'static str>, delayed: bool) {
        let k = self.e.iter().position(|e| e.0 == Some(p)).unwrap_or_else(|| {
            let k = self.n % LOOKS;
            self.n += 1;
            k
        });
        self.e[k] = (Some(p), now, word, delayed);
    }

    /// The word of `p`'s last move if it lands late (a delayed-strike op) and fired within 12 s:
    /// `Some(None)` = that move says nothing; `None` = no such move (use the default).
    pub fn delayed_word(&self, p: PlayerId, now: f64) -> Option<Option<&'static str>> {
        self.e.iter().find(|e| e.0 == Some(p) && e.3 && now - e.1 < 12.0).map(|e| e.2)
    }
}

/// A move with an op whose blast lands after the move fired.
pub fn has_delayed_blast(mv: &MoveDef) -> bool {
    mv.ops.iter().any(|o| matches!(o, MoveOp::Salvo { .. } | MoveOp::DelayedHit { .. } | MoveOp::TagStrike { .. } | MoveOp::Seeker { .. } | MoveOp::Surge { .. } | MoveOp::Erupt { .. }))
}

/// Perf readout.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct OpVisStats {
    pub live: usize,
    pub peak: usize,
    pub started: u64,
    pub step_us: f32,
    /// Immediate cubes dropped over [`IMM_BUDGET`] last frame.
    pub imm_dropped: usize,
}

/// Per player, the last time a strike word was said (a salvo says its word once, not per bolt).
#[derive(Resource, Default)]
struct WordGate([(Option<(PlayerId, u8)>, f32); 24]);

impl WordGate {
    /// `chan` separates a player's kinds of words / sounds (0 strike word, 1 telegraph tell, …).
    fn allow(&mut self, p: PlayerId, chan: u8, now: f32, gap: f32) -> bool {
        let p = (p, chan);
        if let Some(e) = self.0.iter_mut().find(|e| e.0 == Some(p)) {
            if now - e.1 < gap {
                return false;
            }
            e.1 = now;
            return true;
        }
        let k = self.0.iter().enumerate().min_by(|a, b| a.1.1.total_cmp(&b.1.1)).map_or(0, |x| x.0);
        self.0[k] = (Some(p), now);
        true
    }
}

pub struct OpVisualsPlugin;

impl Plugin for OpVisualsPlugin {
    fn build(&self, app: &mut App) {
        dev::build(app);
        app.init_resource::<Pool>()
            .init_resource::<Locks>()
            .init_resource::<Looks>()
            .init_resource::<RecentMoves>()
            .init_resource::<OpVisStats>()
            .init_resource::<WordGate>()
            .add_message::<Sfx3d>()
            .add_message::<CameraShakeEvent>()
            .add_message::<HitStop>()
            .add_message::<SpawnOnomatopoeia>()
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, intake.in_set(PresentSet::Sync))
            .add_systems(PostUpdate, step.after(CameraUpdateSet).after(HandsPublishSet).before(TransformSystems::Propagate));
    }
}

fn clear(mut pool: ResMut<Pool>, mut locks: ResMut<Locks>) {
    for f in pool.fx.iter_mut() {
        f.live = false;
    }
    for c in locks.0.iter_mut() {
        c.live = false;
    }
}

fn chest(v: &PlayerView) -> Vec3 {
    v.pos + Vec3::Y * 1.05 * v.size
}

fn eye_look(v: &PlayerView) -> (Vec3, Vec3) {
    let (eye, look, _) = super::super::constructs::body_frames(v);
    (eye, look * Vec3::NEG_Z)
}

fn hand_of(v: &PlayerView, hands: &ViewmodelHands, local: Option<PlayerId>) -> Vec3 {
    if Some(v.id) == local && hands.active {
        hands.world[1]
    } else {
        super::super::constructs::hand_transform(v).translation
    }
}

fn ground_below(active: &ActiveWorld, p: Vec3, max: f32) -> Option<Vec3> {
    bd_core::voxel::raycast(&active.world.voxels, gv(p + Vec3::Y * 0.3), bd_core::Vec3::NEG_Y, max).map(|h| bv(h.point))
}

fn nearest_player(views: &WorldViews, p: Vec3, max: f32) -> Option<PlayerId> {
    views.players.iter().filter(|v| v.alive()).map(|v| (v.id, chest(v).distance(p).min(v.pos.distance(p)))).filter(|x| x.1 <= max).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0)
}

fn view_of(cams: &Query<&GlobalTransform, With<PlayerCamera>>) -> View {
    cams.iter().next().map_or(View::default(), |g| View { cam: g.translation(), right: g.right().as_vec3(), up: g.up().as_vec3(), fwd: g.forward().as_vec3() })
}

/// The first op of a move matching `f` (for the numbers an event doesn't carry).
fn op_of<T>(mv: Option<&'static MoveDef>, f: impl Fn(&MoveOp) -> Option<T>) -> Option<T> {
    mv.and_then(|m| m.ops.iter().find_map(f))
}

type Writers<'a, 'w> = (&'a mut MessageWriter<'w, Sfx3d>, &'a mut MessageWriter<'w, CameraShakeEvent>, &'a mut MessageWriter<'w, HitStop>, &'a mut MessageWriter<'w, SpawnOnomatopoeia>);

/// Read the sim's events: remember each mover's look, then start every op beat.
#[allow(clippy::too_many_arguments)]
fn intake(
    time: Res<Time<Real>>,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    table: Res<MoveFx>,
    comfort: Res<ComfortSettings>,
    mut pool: ResMut<Pool>,
    mut locks: ResMut<Locks>,
    (mut looks, mut recent, mut gate): (ResMut<Looks>, ResMut<RecentMoves>, ResMut<WordGate>),
    mut fx: ResMut<FxParticles>,
    settings: Option<Res<bd_fx::FxSettings>>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    hands: Res<ViewmodelHands>,
    (mut buf, mut opened): (Local<Vec<OpEvent>>, Local<Vec<PlayerId>>),
    out: (MessageWriter<Sfx3d>, MessageWriter<CameraShakeEvent>, MessageWriter<HitStop>, MessageWriter<SpawnOnomatopoeia>),
) {
    let (mut sfx, mut shake, mut hitstop, mut ono) = out;
    let Some(active) = active.filter(|_| !dev::off()) else {
        events.clear();
        return;
    };
    let now = time.elapsed_secs();
    let now64 = time.elapsed_secs_f64();
    let table_ref = &*table;
    opened.clear();
    for ev in events.read() {
        match &ev.event {
            GameEvent::Power(PowerEvent::Fired { player, mv, .. }) => {
                if let Some((def, m)) = mv.resolve(PowerTable::get()) {
                    let corps = views.player(*player).map_or([1.0; 3], |v| corps_rgb(v.corps));
                    let look = resolve_look(table_ref, m, Some(def), corps);
                    looks.set(*player, look, now, Some(m));
                    recent.note(*player, now64, style_word(table_ref, m, Some(def)), has_delayed_blast(m));
                    // A move that comes out of the eyes (style `gaze`): twin beams from the face.
                    if table_ref.keys.get(look.style as usize).is_some_and(|k| k == "gaze") {
                        let (reach, half) = gaze_reach(m);
                        let pl = *player;
                        pool.kill(|f| f.kind == Kind::Gaze && f.owner == pl);
                        pool.add(Fx { kind: Kind::Gaze, look, owner: *player, r: reach, acc: half, dur: 0.55, ..default() });
                        if let Some(pv) = views.player(*player) {
                            sfx.write(Sfx3d::at("reveal_pulse", chest(pv)).pitch(0.6).gain(0.9));
                            sfx.write(Sfx3d::at("bfg_zap", chest(pv)).pitch(0.7).gain(0.5));
                        }
                    }
                }
            }
            GameEvent::Power(PowerEvent::Composed { player, spec, .. }) => {
                if let Some(m) = bd_sim::powers::compose::interned(spec) {
                    let corps = views.player(*player).map_or([1.0; 3], |v| corps_rgb(v.corps));
                    looks.set(*player, resolve_look(table_ref, m, None, corps), now, Some(m));
                    recent.note(*player, now64, style_word(table_ref, m, None), has_delayed_blast(m));
                }
            }
            GameEvent::PortalOpened { owner, .. } => {
                // The pair is open: the real portals take over from the lone ends.
                let o = *owner;
                pool.kill(|f| f.kind == Kind::Portal && f.owner == o);
                opened.push(o);
            }
            GameEvent::Power(PowerEvent::Op(o)) if !matches!(o, OpEvent::Twisted { .. }) => {
                if dev::log_events() {
                    info!("op_visuals event: {}", format!("{o:?}").chars().take(160).collect::<String>());
                }
                buf.push(o.clone());
            }
            _ => {}
        }
    }
    if buf.is_empty() {
        return;
    }
    let budget = settings.as_ref().map_or(1.0, |s| s.particle_budget).clamp(0.15, 1.0);
    let view = view_of(&cams);
    let mut pen = Pen { fx: &mut fx, motes: table.t.beats.motes * budget, scale: 1.0, cam: view.cam, avoid: None };
    pen.fx.scope(view.cam, LINE_FLOOR, IMM_BUDGET / 4);
    let mut c = Ctx {
        views: &views,
        active: &active,
        table: table_ref,
        looks: &looks,
        opened: &opened,
        pool: &mut pool,
        locks: &mut locks,
        gate: &mut gate,
        view,
        local: active.local,
        hands: &hands,
        calm: comfort.flashes <= 0.01,
        now,
    };
    let mut w: Writers = (&mut sfx, &mut shake, &mut hitstop, &mut ono);
    for o in buf.drain(..) {
        c.start(&o, &mut pen, &mut w);
    }
    pen.fx.end_scope();
}

struct Ctx<'a> {
    views: &'a WorldViews,
    active: &'a ActiveWorld,
    table: &'a MoveFx,
    looks: &'a Looks,
    /// Owners whose portal pair (re)opened this frame (their shot is no lone end).
    opened: &'a [PlayerId],
    pool: &'a mut Pool,
    locks: &'a mut Locks,
    gate: &'a mut WordGate,
    view: View,
    local: Option<PlayerId>,
    hands: &'a ViewmodelHands,
    calm: bool,
    now: f32,
}

fn snd(w: &mut Writers, name: &'static str, p: Vec3, pitch: f32, gain: f32) {
    w.0.write(Sfx3d::at(name, p).pitch(pitch).gain(gain));
}

fn word(w: &mut Writers, key: &'static str, p: Vec3, size: f32, color: [f32; 3], id: u64, local: bool) {
    let mut o = SpawnOnomatopoeia::key(key, p, size, color);
    o.event_id = id;
    o.local_player = local;
    w.3.write(o);
}

impl Ctx<'_> {
    /// The look an event of `p` draws in: the move they just fired, else the element's style.
    fn look(&self, p: PlayerId, el: Option<Element>) -> Look {
        if let Some((l, _)) = self.looks.get(p) {
            return l;
        }
        self.fallback(p, el)
    }

    fn fallback(&self, p: PlayerId, el: Option<Element>) -> Look {
        let corps = self.views.player(p).map_or([1.0; 3], |v| corps_rgb(v.corps));
        let el = el.filter(|e| *e != Element::Light);
        let style = el.and_then(|e| self.table.for_element(e)).unwrap_or(self.table.energy);
        look_of(self.table, style, None, el, None, corps)
    }

    fn mv(&self, p: PlayerId) -> Option<&'static MoveDef> {
        self.looks.get(p).and_then(|x| x.1)
    }

    fn involved(&self, a: PlayerId, b: Option<PlayerId>) -> bool {
        self.local.is_some_and(|l| l == a || Some(l) == b)
    }

    fn near(&self, p: Vec3, r: f32) -> bool {
        p.distance(self.view.cam) < r
    }

    fn shake(&self, w: &mut Writers, trauma: f32, stop: f32) {
        if self.calm {
            return;
        }
        w.1.write(CameraShakeEvent { trauma, dir: Vec3::ZERO });
        if stop > 0.0 {
            w.2.write(HitStop { secs: stop });
        }
    }

    fn marker(&mut self, owner: PlayerId, target: PlayerId, kind: MarkKind, look: Look, secs: f32) {
        // One marker of a kind per target: a re-mark refreshes it.
        let refresh = self.pool.find(|f| f.kind == Kind::Marker(kind) && f.target == Some(target));
        if let Some(f) = refresh {
            f.t = 0.0;
            f.dur = secs;
            f.look = look;
            f.owner = owner;
            return;
        }
        self.pool.add(Fx { kind: Kind::Marker(kind), look, owner, target: Some(target), dur: secs, ..default() });
    }

    #[allow(clippy::too_many_lines)]
    fn start(&mut self, o: &OpEvent, pen: &mut Pen, w: &mut Writers) {
        let v = self.view;
        match o {
            // ───────────── ARENA GAMES
            OpEvent::Rewound { player, from, to, path } => {
                // Ghosts of you along the route you took, newest brightest, a clock ring on arrival.
                let look = Look { main: [0.3, 0.75, 1.0], core: [0.85, 0.95, 1.0], ..self.look(*player, None) };
                let yaw = self.views.player(*player).map_or(0.0, |p| p.yaw);
                for (k, p) in path.iter().enumerate() {
                    let q = bv(*p);
                    if q.distance_squared(bv(*to)) < 0.04 {
                        continue;
                    }
                    pen.ghost(q, yaw, 1.0, look.main, look.glow * (0.25 + 0.08 * k as f32), 0.35 + 0.05 * k as f32);
                }
                for win in path.windows(2) {
                    pen.beam(bv(win[0]) + Vec3::Y, bv(win[1]) + Vec3::Y, 0.08, look.main, look.glow, 0.45);
                }
                pen.beam(bv(path[7]) + Vec3::Y, bv(*from) + Vec3::Y, 0.08, look.main, look.glow, 0.45);
                let at = bv(*to) + Vec3::Y;
                pen.ring(&look, at, Vec3::Y, 0.4, 6.0, 1.2, 0.45);
                pen.ring(&look, at, (at - v.cam).normalize_or(Vec3::Z), 0.3, 5.0, 1.0, 0.4);
                prims::flash(pen, bv(*from) + Vec3::Y, 1.0, look.main, look.glow, 0.12);
                snd(w, "recall_whoosh", at, 1.0, 0.9);
                snd(w, "mv_time_tick", at, 0.8, 0.8);
            }
            OpEvent::Ram { player, phase, target, pos, dir } => {
                let look = self.look(*player, None);
                let (p, d) = (bv(*pos), bv(*dir));
                match phase {
                    RamPhase::Start | RamPhase::End => {}
                    RamPhase::Pinned => {
                        prims::impact_star(pen, &look, p, d, 1.0);
                        if let Some(t) = target {
                            self.marker(*player, *t, MarkKind::Held, look, 1.2);
                        }
                        snd(w, "hit_heavy", p, 0.8, 0.9);
                    }
                    RamPhase::Hit => {
                        prims::impact_star(pen, &look, p, d, 1.1);
                        snd(w, "hit_heavy", p, 0.9, 0.8);
                    }
                    RamPhase::Slam => {
                        // The wall takes it: a splat ring in the wall's plane, dust, shards.
                        if let Some(t) = target {
                            let t = *t;
                            self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Held) && f.target == Some(t));
                        }
                        prims::impact_star(pen, &look, p, -d, 2.0);
                        pen.ring(&look, p, d, 0.3, 9.0, 1.6, 0.4);
                        let grit = Look { body: Body::Grain, ..look };
                        pen.burst(&grit, p, 1.6, 7.0, Some(-d), 0.9, 0.7);
                        pen.dust_wall(&look, p - d * 0.3, 0.8, 1.2, 0.8);
                        snd(w, "explosion_big", p, 0.8, 0.9);
                        snd(w, "wall_splat", p, 0.9, 1.0);
                        if self.involved(*player, *target) || self.near(p, 18.0) {
                            self.shake(w, 0.3, 0.09);
                        }
                    }
                    RamPhase::Bonk => {
                        let grit = Look { body: Body::Grain, ..look };
                        pen.burst(&grit, p, 0.8, 4.0, Some(-d), 0.8, 0.5);
                        pen.ring(&look, p, d, 0.2, 5.0, 0.8, 0.3);
                        snd(w, "collapse_rumble", p, 1.0, 0.5);
                    }
                    RamPhase::Escaped => {
                        if let Some(t) = target {
                            let t = *t;
                            self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Held) && f.target == Some(t));
                        }
                        let smoke = Look { body: Body::Smoke, main: [0.5, 0.5, 0.52], ..look };
                        pen.burst(&smoke, p, 0.6, 2.0, None, 0.0, 0.5);
                        snd(w, "dodge", p, 1.2, 0.7);
                    }
                }
            }
            OpEvent::Hook { player, phase, target, pos, dir, speed, range } => {
                let look = self.look(*player, None);
                let p = bv(*pos);
                match phase {
                    HookPhase::Fired => {
                        // A hook is a chain + kunai unless its body is a band / a spirit / a bolt.
                        let body = match look.body {
                            Body::Rubber | Body::Smoke | Body::Void | Body::Bolt | Body::Streak => prims::tether_body(&look),
                            _ => TetherBody::Chain,
                        };
                        let pl = *player;
                        self.pool.kill(|f| matches!(f.kind, Kind::Tether(_)) && f.owner == pl && f.phase < 3);
                        self.pool.add(Fx { kind: Kind::Tether(body), look, owner: *player, a: p, b: p, dir: bv(*dir).normalize_or(Vec3::NEG_Z), acc: speed.max(1.0), r: range.max(1.0), dur: 4.0, ..default() });
                        snd(w, "hook_fire", p, 1.0, 1.0);
                        snd(w, "chain_sweep", p, 1.1, 0.6);
                    }
                    HookPhase::Caught | HookPhase::Anchored => {
                        let pl = *player;
                        if let Some(f) = self.pool.find(|f| matches!(f.kind, Kind::Tether(_)) && f.owner == pl && f.phase == 0) {
                            f.b = p;
                            f.phase = if *phase == HookPhase::Caught { 1 } else { 2 };
                            f.target = *target;
                            f.t = 0.0;
                            f.dur = 2.5;
                        }
                        prims::impact_star(pen, &look, p, bv(*dir), 0.8);
                        snd(w, "hook_attach", p, if *phase == HookPhase::Caught { 1.0 } else { 0.8 }, 1.0);
                        if *phase == HookPhase::Caught {
                            snd(w, "chain_pull", p, 1.0, 0.9);
                            if let Some(t) = target {
                                self.marker(*player, *t, MarkKind::Held, look, 1.0);
                            }
                            if self.involved(*player, *target) {
                                self.shake(w, 0.15, 0.06);
                            }
                        }
                    }
                    HookPhase::End => {
                        let pl = *player;
                        let mut whiff = false;
                        if let Some(f) = self.pool.find(|f| matches!(f.kind, Kind::Tether(_)) && f.owner == pl && f.phase < 3) {
                            whiff = f.phase == 0;
                            if f.phase == 0 {
                                f.b = p;
                            }
                            f.phase = 3;
                            f.t = 0.0;
                            f.dur = 0.22;
                        }
                        if let Some(t) = target {
                            let t = *t;
                            self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Held) && f.target == Some(t));
                        }
                        snd(w, if whiff { "hook_break" } else { "tether_release" }, p, 1.0, 0.7);
                    }
                }
            }
            OpEvent::Grab { player, phase, target, pos, dir } => {
                let look = self.look(*player, None);
                let p = bv(*pos);
                match phase {
                    GrabPhase::Seize => {
                        claw(pen, &look, p, 0.9);
                        if let Some(t) = target {
                            self.marker(*player, *t, MarkKind::Held, look, 1.5);
                        }
                        snd(w, "grav_grab", p, 0.9, 1.0);
                        snd(w, "hit_crit", p, 0.55, 0.6);
                    }
                    GrabPhase::Throw => {
                        if let Some(t) = target {
                            let t = *t;
                            self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Held) && f.target == Some(t));
                        }
                        let d = bv(*dir).normalize_or(Vec3::Y);
                        prims::impact_star(pen, &look, p, d, 1.6);
                        pen.ring(&look, p, d, 0.3, 7.0, 1.2, 0.35);
                        let grit = Look { body: Body::Grain, ..look };
                        pen.burst(&grit, p, 1.2, 6.0, Some(d), 0.6, 0.6);
                        snd(w, "grav_punt", p, 1.0, 1.0);
                        if self.involved(*player, *target) {
                            self.shake(w, 0.22, 0.0);
                        }
                    }
                    GrabPhase::Whiff => {
                        claw(pen, &look, p, 0.5);
                        let smoke = Look { body: Body::Smoke, main: [0.5, 0.5, 0.52], ..look };
                        pen.burst(&smoke, p, 0.4, 1.5, None, 0.0, 0.4);
                        snd(w, "swing_whoosh_medium", p, 0.8, 0.7);
                    }
                }
            }
            OpEvent::Stance { player, kind, secs, angle } => {
                let el = match kind {
                    StanceKind::Deflect => Element::Wind,
                    _ => Element::Light,
                };
                let look = self.look(*player, Some(el));
                let aura = match kind {
                    StanceKind::Deflect => AuraKind::Deflect,
                    StanceKind::Counter => AuraKind::Counter,
                    StanceKind::Guard => AuraKind::Guard,
                };
                let pl = *player;
                self.pool.kill(|f| f.kind == Kind::Aura(aura) && f.owner == pl);
                self.pool.add(Fx { kind: Kind::Aura(aura), look, owner: *player, dur: secs.clamp(0.1, 10.0), r: angle.to_radians(), ..default() });
                if let Some(pv) = self.views.player(*player) {
                    let c = chest(pv);
                    prims::flash(pen, c, 0.9, look.core, look.glow, 0.1);
                    snd(w, match kind {
                        StanceKind::Deflect => "saber_swoosh",
                        StanceKind::Counter => "presence_shing",
                        StanceKind::Guard => "shield_block",
                    }, c, 1.3, 0.6);
                }
            }
            OpEvent::Parried { player, kind, attacker, pos, dir } => {
                let look = self.look(*player, None);
                let p = bv(*pos);
                let d = bv(*dir);
                match kind {
                    StanceKind::Deflect => {
                        // Sent back: sparks sprayed along the new heading, a ring where it turned.
                        prims::flash(pen, p, 0.9, look.core, look.glow * 1.4, 0.08);
                        let sparks = Look { body: Body::Bolt, ..look };
                        pen.burst(&sparks, p, 0.8, 9.0, Some(d.normalize_or(Vec3::Y)), 0.4, 0.25);
                        pen.ring(&look, p, d.normalize_or(Vec3::Y), 0.15, 5.0, 0.7, 0.2);
                        snd(w, "saber_deflect", p, 1.0, 0.9);
                    }
                    StanceKind::Counter => {
                        // The answer: a gleam star, a white ring, PARRY!.
                        let pl = *player;
                        self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::Counter) && f.owner == pl);
                        prims::impact_star(pen, &look, p, d, 1.5);
                        pen.ring(&look, p, (p - v.cam).normalize_or(Vec3::Z), 0.2, 8.0, 1.4, 0.3);
                        snd(w, "clash", p, 1.2, 1.0);
                        if self.gate.allow(*player, 3, self.now, 0.4) {
                            word(w, "parry", p + Vec3::Y * 0.6, 1.1, look.core, (p.x * 131.0) as i64 as u64 ^ pl.0 as u64, self.involved(*player, *attacker));
                        }
                    }
                    StanceKind::Guard => {
                        prims::flash(pen, p, 0.8, look.core, look.glow, 0.08);
                        pen.ring(&look, p, d.normalize_or(Vec3::Y), 0.2, 4.0, 0.8, 0.25);
                        snd(w, "shield_block", p, 1.0, 0.9);
                    }
                }
            }
            OpEvent::LockOn { player, phase, target, frac, secs, .. } => {
                let look = self.look(*player, Some(Element::Fire));
                let chest_p = self.views.player(*player).map(chest);
                match phase {
                    LockPhase::Start => {
                        let (range, angle) = op_of(self.mv(*player), |o| if let MoveOp::LockOn { range, angle, .. } = o { Some((*range, *angle)) } else { None }).unwrap_or((40.0, 35.0));
                        let angle = if angle > 0.0 { angle } else { 35.0 };
                        let pl = *player;
                        if let Some(ch) = self.locks.0.iter_mut().find(|c| !c.live || c.owner == pl) {
                            *ch = Chan { live: true, owner: *player, secs: secs.clamp(0.3, 10.0), range: range.clamp(2.0, 120.0), cos: angle.clamp(3.0, 89.0).to_radians().cos(), look, ..default() };
                        }
                        self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::Glare) && f.owner == pl);
                        self.pool.add(Fx { kind: Kind::Aura(AuraKind::Glare), look, owner: *player, dur: secs.clamp(0.3, 10.0) + 0.3, ..default() });
                        if let Some(c) = chest_p {
                            prims::flash(pen, c + Vec3::Y, 0.7, look.core, look.glow, 0.12);
                            snd(w, "ult_charge", c, 0.5, 1.0);
                            snd(w, "heartbeat", c, 0.8, 0.7);
                        }
                    }
                    LockPhase::Mark => {
                        let pl = *player;
                        if let (Some(t), Some(ch)) = (target, self.locks.0.iter_mut().find(|c| c.live && c.owner == pl))
                            && let Some(k) = ch.slot(*t)
                        {
                            ch.fr[k] = ch.fr[k].max(*frac);
                        }
                        if let Some(tv) = target.and_then(|t| self.views.player(t)) {
                            let head = tv.pos + Vec3::Y * 2.1 * tv.size;
                            if *frac >= 1.0 {
                                prims::flash(pen, chest(tv), 0.8, look.core, look.glow * 1.5, 0.1);
                                snd(w, "hit_crit", head, 2.0, 0.45);
                            } else {
                                snd(w, "needle_tick", head, 1.6, 0.4);
                            }
                        }
                    }
                    LockPhase::Fire => {
                        // Every marked target at once: a tracer from the gun to each, a crit flash on them.
                        let pl = *player;
                        let hand = self.views.player(*player).map(|pv| hand_of(pv, self.hands, self.local));
                        if let Some(ch) = self.locks.0.iter_mut().find(|c| c.live && c.owner == pl) {
                            for k in 0..8 {
                                let (Some(id), f) = (ch.ids[k], ch.fr[k]) else { continue };
                                if f < 0.05 {
                                    continue;
                                }
                                let Some(tv) = self.views.player(id).filter(|x| x.alive()) else { continue };
                                let tc = chest(tv);
                                if let Some(h) = hand {
                                    pen.beam(h, tc, 0.06 + 0.06 * f, look.core, look.glow * 2.5, 0.2);
                                    pen.beam(h, tc, 0.2 * f, look.main, look.glow, 0.12);
                                }
                                prims::impact_star(pen, &look, tc, tc - hand.unwrap_or(tc), 0.8 + 0.6 * f);
                            }
                            ch.live = false;
                        }
                        self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::Glare) && f.owner == pl);
                        if let Some(c) = chest_p {
                            prims::flash(pen, c + Vec3::Y * 0.3, 0.8, look.core, look.glow * 1.3, 0.1);
                            snd(w, "golden_gun", c, 0.9, 1.0);
                            snd(w, "gun_handcannon", c, 0.8, 0.8);
                            if self.involved(*player, None) {
                                self.shake(w, 0.2, 0.0);
                            }
                        }
                    }
                    LockPhase::Broken => {
                        let pl = *player;
                        if let Some(ch) = self.locks.0.iter_mut().find(|c| c.live && c.owner == pl) {
                            // The brackets fall apart.
                            for k in 0..8 {
                                if let Some(tv) = ch.ids[k].and_then(|id| self.views.player(id)) {
                                    let shards = Look { body: Body::Shard, ..look };
                                    pen.burst(&shards, chest(tv), 0.5, 3.0, None, 0.0, 0.4);
                                }
                            }
                            ch.live = false;
                        }
                        self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::Glare) && f.owner == pl);
                        if let Some(c) = chest_p {
                            let smoke = Look { body: Body::Smoke, main: [0.5, 0.5, 0.52], ..look };
                            pen.burst(&smoke, c, 0.6, 1.5, None, 0.0, 0.5);
                            snd(w, "charge_break", c, 1.0, 0.8);
                        }
                    }
                }
            }
            OpEvent::LockedOut { player, target, secs } => {
                let look = Look { main: [0.7, 0.3, 1.0], core: [0.95, 0.85, 1.0], body: Body::Glitch, ..self.look(*player, None) };
                if let (Some(a), Some(b)) = (self.views.player(*player), self.views.player(*target)) {
                    let (from, to) = (hand_of(a, self.hands, self.local), chest(b));
                    pen.beam(from, to, 0.12, look.main, look.glow * 1.2, 0.3);
                    let rs = pen.fx.rng.next_u32();
                    zig(pen.fx, from, to, 10, 0.3, rs, 0.03, look.core, look.glow * 2.0);
                    pen.burst(&look, to, 0.8, 3.0, None, 0.0, 0.4);
                    snd(w, "mv_glitch", to, 0.8, 0.8);
                }
                self.marker(*player, *target, MarkKind::Lockout, look, secs.clamp(0.2, 5.0));
            }
            OpEvent::Zone { player, pos, radius, secs, status, lockout, .. } => {
                let el = status.map(status_element);
                let look = self.look(*player, el);
                let n = if *lockout { 254 } else { status.map_or(255, |s| s as u8) };
                self.pool.add(Fx { kind: Kind::Zone, look, owner: *player, a: bv(*pos), r: radius.clamp(0.5, 30.0), dur: secs.clamp(0.2, 20.0), n, ..default() });
            }
            // ───────────── MYTHOLOGY & FOLKLORE
            OpEvent::Seek { player, phase, target, pos, dir } => {
                let look = self.look(*player, None);
                let p = bv(*pos);
                match phase {
                    SeekPhase::Launch => {
                        if let Some(t) = target {
                            self.marker(*player, *t, MarkKind::Hunted, look, 1.4);
                        }
                        let pl = *player;
                        self.pool.kill(|f| f.kind == Kind::Trail && f.owner == pl);
                        self.pool.add(Fx { kind: Kind::Trail, look, owner: *player, a: p, b: p, dur: 8.0, ..default() });
                        snd(w, "leap_whoosh", p, 1.3, 0.8);
                    }
                    SeekPhase::Strike => {
                        if let Some(t) = target {
                            let t = *t;
                            self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Hunted) && f.target == Some(t));
                        }
                        prims::impact_star(pen, &look, p, bv(*dir), 1.2);
                        snd(w, "hit_heavy", p, 1.0, 0.8);
                    }
                    SeekPhase::Dodged => {
                        pen.beam(p - bv(*dir) * 1.5, p + bv(*dir) * 1.5, 0.05, look.main, look.glow, 0.15);
                        snd(w, "swing_whoosh_light", p, 1.2, 0.8);
                    }
                    SeekPhase::Chain => {
                        if let Some(tv) = target.and_then(|t| self.views.player(t)) {
                            let rs = pen.fx.rng.next_u32();
                            zig(pen.fx, p, chest(tv), 8, 0.4, rs, 0.04, look.core, look.glow * 2.0);
                            pen.beam(p, chest(tv), 0.08, look.main, look.glow, 0.25);
                            self.marker(*player, tv.id, MarkKind::Hunted, look, 1.0);
                        }
                        snd(w, "ricochet_ding", p, 1.0, 0.8);
                    }
                    SeekPhase::Blocked => {
                        let sparks = Look { body: Body::Bolt, ..look };
                        pen.burst(&sparks, p, 0.8, 7.0, Some(-bv(*dir)), 0.8, 0.3);
                        snd(w, "clash", p, 1.0, 0.8);
                    }
                    SeekPhase::Return => {
                        // It turns for home: a flare at the turn and a rising whistle.
                        pen.ring(&look, p, bv(*dir).normalize_or(Vec3::Y), 0.2, 4.0, 0.7, 0.25);
                        snd(w, "recall_whoosh", p, 1.4, 0.6);
                    }
                    SeekPhase::Catch => {
                        let pl = *player;
                        self.pool.kill(|f| f.kind == Kind::Trail && f.owner == pl);
                        prims::flash(pen, p, 0.6, look.core, look.glow, 0.08);
                        snd(w, "recall_catch", p, 1.0, 0.8);
                    }
                }
            }
            OpEvent::Haul { player, phase, target, from, to, secs } => {
                let look = self.look(*player, Some(Element::Earth));
                let (a, b) = (bv(*from), bv(*to));
                match phase {
                    HaulPhase::Player => {
                        self.pool.add(Fx { kind: Kind::Tether(TetherBody::Chain), look, owner: *player, target: *target, b: a, phase: 1, dur: 1.2, ..default() });
                        snd(w, "chain_pull", a, 0.9, 1.0);
                    }
                    HaulPhase::Tear => {
                        let grit = Look { body: Body::Grain, ..look };
                        pen.burst(&grit, a, 1.6, 6.0, Some(Vec3::Y), 0.8, 0.8);
                        pen.ring(&look, a, Vec3::Y, 0.4, 6.0, 1.2, 0.35);
                        let rs = pen.fx.rng.next_u32();
                        pen.crack(&look, a, (b - a).normalize_or(Vec3::X), 2.5, rs);
                        self.pool.add(Fx { kind: Kind::Tether(TetherBody::Chain), look, owner: *player, a, b, phase: 5, acc: secs.max(0.1), dur: secs.max(0.1) + 0.1, ..default() });
                        snd(w, "break_stone", a, 0.8, 1.0);
                        snd(w, "collapse_rumble", a, 1.1, 0.8);
                    }
                    HaulPhase::Strike => {
                        prims::impact_star(pen, &look, a, b - a, 1.3);
                        snd(w, "heavy_crunch", a, 1.0, 0.9);
                    }
                    HaulPhase::Land => {
                        let pl = *player;
                        self.pool.kill(|f| matches!(f.kind, Kind::Tether(_)) && f.owner == pl && f.phase == 5);
                        pen.dust_wall(&look, a, 1.2, 1.4, 0.8);
                        snd(w, "land_heavy", a, 0.8, 1.0);
                    }
                }
            }
            OpEvent::Leap(arc) => {
                let look = self.look(arc.player, None);
                let (a, b) = (bv(arc.from), bv(arc.to));
                let pl = arc.player;
                self.pool.kill(|f| f.kind == Kind::Arc && f.owner == pl);
                let g = ground_below(self.active, b + Vec3::Y * 0.5, 4.0).unwrap_or(b);
                self.pool.add(Fx { kind: Kind::Arc, look, owner: arc.player, a, b: g, r: arc.apex, dur: arc.secs.clamp(0.1, 6.0) + 0.1, ..default() });
                snd(w, "leap_whoosh", a, 0.9, 1.0);
            }
            OpEvent::Surge { player, phase, target, pos, dir, width, speed, dist } => {
                // The wall is its ELEMENT (a tidal wave is water, whatever the move's colour key /
                // the caster's corps colour says); no element = the move's look.
                let el = op_of(self.mv(*player), |o| if let MoveOp::Surge { element, .. } = o { Some(*element) } else { None }).filter(|e| *e != Element::Light);
                let look = match el {
                    Some(e) => self.fallback(*player, Some(e)),
                    None => self.look(*player, None),
                };
                let p = bv(*pos);
                let d = bv(*dir);
                match phase {
                    SurgePhase::Start => {
                        let secs = (dist / speed.max(1.0)).clamp(0.1, 8.0);
                        self.pool.add(Fx { kind: Kind::Surge, look, owner: *player, a: p, dir: Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::NEG_Z), r: width.clamp(1.0, 30.0), acc: *speed, dur: secs, ..default() });
                        snd(w, "storm_geyser", p, 0.8, 1.0);
                    }
                    SurgePhase::Catch => {
                        if let Some(tv) = target.and_then(|t| self.views.player(t)) {
                            pen.burst(&look, chest(tv), 1.2, 5.0, Some(d), 0.8, 0.5);
                            snd(w, "hit_medium", chest(tv), 0.8, 0.8);
                        }
                    }
                    SurgePhase::Break => {
                        let pl = *player;
                        self.pool.kill(|f| f.kind == Kind::Surge && f.owner == pl);
                        let side = Vec3::Y.cross(d).normalize_or(Vec3::X);
                        for k in 0..5 {
                            let q = p + side * (k as f32 - 2.0) / 2.0 * width * 0.5 + Vec3::Y * 0.8;
                            pen.burst(&look, q, 0.8, 7.0, Some((d + Vec3::Y).normalize()), 0.8, 0.7);
                        }
                        snd(w, "explosion_small", p, 0.7, 0.9);
                    }
                }
            }
            OpEvent::Roulette { player, pick, of } => {
                let look = self.look(*player, None);
                let pl = *player;
                self.pool.kill(|f| f.kind == Kind::Wheel && f.owner == pl);
                self.pool.add(Fx { kind: Kind::Wheel, look, owner: *player, n: *of, phase: *pick, dur: 1.1, ..default() });
                if let Some(pv) = self.views.player(*player) {
                    snd(w, "summon_ping", chest(pv), 1.3, 0.7);
                }
            }
            // ───────────── ADVENTURE GAMES
            OpEvent::Pounce { player, phase, target, pos, dir } => {
                let look = self.look(*player, None);
                let p = bv(*pos);
                match phase {
                    PouncePhase::Start => {
                        if let Some(t) = target {
                            self.marker(*player, *t, MarkKind::Hunted, look, 0.6);
                            self.pool.add(Fx { kind: Kind::Sight, look, owner: *player, target: Some(*t), dur: 0.4, ..default() });
                        }
                        snd(w, "lunge", p, 1.2, 0.9);
                    }
                    PouncePhase::Hit => {
                        if let Some(t) = target {
                            let t = *t;
                            self.pool.kill(|f| (f.kind == Kind::Marker(MarkKind::Hunted) || f.kind == Kind::Sight) && f.target == Some(t));
                        }
                        prims::impact_star(pen, &look, p, bv(*dir), 1.4);
                        pen.ring(&look, p, Vec3::Y, 0.3, 6.0, 1.0, 0.3);
                        snd(w, "hit_heavy", p, 1.1, 0.9);
                        snd(w, "jump", p, 1.3, 0.6);
                    }
                    PouncePhase::Whiff => {
                        let pl = *player;
                        self.pool.kill(|f| f.kind == Kind::Sight && f.owner == pl);
                        pen.burst(&look, p, 0.4, 2.0, None, 0.0, 0.3);
                    }
                }
            }
            OpEvent::Telegraph { player, pos, radius, secs, element } => {
                let look = self.look(*player, Some(*element));
                let cut = op_of(self.mv(*player), |o| if let MoveOp::Salvo { cut, .. } = o { Some(*cut) } else { None }).unwrap_or(false);
                let kind = prims::strike_kind(&look, cut, *element);
                let raw = bv(*pos);
                let g = ground_below(self.active, raw + Vec3::Y * 0.2, radius.max(1.0) + 6.0);
                // A cut on a floor spot hangs in the space at body height (the lines read in the air).
                let p = if kind == StrikeKind::Slash && g.is_some_and(|g| raw.y - g.y < 0.6) { raw + Vec3::Y * 1.1 } else { raw };
                // `dir` keeps the host's spot (the Strike finds its telegraph by it).
                self.pool.add(Fx { kind: Kind::Telegraph(kind), look, owner: *player, a: p, b: g.unwrap_or(raw), dir: raw, flag: g.is_some(), r: *radius, dur: secs.max(0.05) + 0.25, acc: secs.max(0.05), ..default() });
                // A soft tell everyone hears (quiet; many strikes stack).
                if self.gate.allow(*player, 1, self.now, 0.12) {
                    snd(w, match kind {
                        StrikeKind::Bolt => "storm_mark",
                        StrikeKind::Slash => "presence_shing",
                        _ => "strike_telegraph",
                    }, p, 1.0 + 0.2 * (1.0 - secs.min(1.0)), 0.55);
                }
            }
            OpEvent::Strike { player, pos, radius, element, cut } => {
                let p = bv(*pos);
                // The telegraph that marked it hands over its look / cut seed / ground.
                let pl = *player;
                let tel = self.pool.find(|f| matches!(f.kind, Kind::Telegraph(_)) && f.owner == pl && f.dir.distance_squared(p) < 0.04).map(|f| {
                    f.live = false;
                    (f.look, f.seed, f.flag.then_some(f.b), f.kind, f.a)
                });
                let p = tel.map_or(p, |x| x.4);
                let look = tel.map(|x| x.0).unwrap_or_else(|| self.look(*player, Some(*element)));
                let kind = match tel.map(|x| x.3) {
                    Some(Kind::Telegraph(k)) => k,
                    _ => prims::strike_kind(&look, *cut, *element),
                };
                let seed = tel.map_or(pen.fx.rng.next_u32(), |x| x.1);
                let ground = tel.and_then(|x| x.2).or_else(|| ground_below(self.active, p + Vec3::Y * 0.2, radius.max(1.0) + 6.0));
                prims::strike(pen, &look, kind, p, ground, *radius, seed);
                if kind == StrikeKind::Bolt {
                    let g = ground.unwrap_or(p);
                    let top = g + Vec3::new((seed % 7) as f32 - 3.0, 28.0, (seed % 5) as f32 - 2.0);
                    self.pool.add(Fx { kind: Kind::Bolt, look, owner: *player, a: top, b: g, dur: 0.22, seed, ..default() });
                }
                let (name, pitch) = match kind {
                    StrikeKind::Slash => ("saber_swoosh", 1.4),
                    StrikeKind::Bolt => ("storm_bolt", 1.0),
                    StrikeKind::Holy => ("mv_holy", 1.0),
                    StrikeKind::Void => ("mv_void_thump", 1.0),
                    StrikeKind::Blast => ("", 1.0),
                };
                if !name.is_empty() {
                    snd(w, name, p, pitch, 0.9);
                }
                if kind == StrikeKind::Slash {
                    snd(w, "hit_crit", p, 1.9, 0.35);
                }
                if kind == StrikeKind::Bolt {
                    snd(w, "mv_bolt_crack", p, 1.0, 0.8);
                }
                // One word per salvo (not per strike), and only for a cut / bolt / holy / void —
                // a blast's word comes with its explosion (`RecentMoves::delayed_word`).
                if kind != StrikeKind::Blast
                    && self.gate.allow(*player, 0, self.now, 0.6)
                    && let Some(k) = strike_word(kind, None)
                {
                    word(w, k, p + Vec3::Y * 0.8, (radius / 2.5).clamp(0.7, 1.4), look.core, seed as u64 ^ ((pl.0 as u64) << 32), self.involved(*player, None));
                }
                if kind == StrikeKind::Bolt && self.near(p, 14.0) {
                    self.shake(w, 0.12, 0.0);
                }
            }
            OpEvent::Mended { player, pct, secs } => {
                let look = self.look(*player, Some(Element::Water));
                if let Some(pv) = self.views.player(*player) {
                    let own = Some(*player) == self.local && self.hands.active;
                    if !own {
                        prims::heal_pulse(pen, &look, pv.pos, pv.size, *pct);
                    } else {
                        pen.ring(&look, pv.pos + Vec3::Y * 0.1, Vec3::Y, 0.4, 3.0, 1.0, 0.45);
                    }
                    snd(w, "reveal_pulse", chest(pv), 1.3, 0.8);
                    snd(w, "oath_complete", chest(pv), 1.5, 0.35);
                }
                if *secs > 0.0 {
                    let pl = *player;
                    self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::Mend) && f.owner == pl);
                    self.pool.add(Fx { kind: Kind::Aura(AuraKind::Mend), look, owner: *player, dur: secs.clamp(0.1, 8.0), ..default() });
                }
            }
            OpEvent::Evade { player, dodged, attacker, pos, secs } => {
                let look = self.look(*player, None);
                let silver = Look { main: [0.75, 0.8, 1.0], core: [1.0, 1.0, 1.0], ..look };
                let pl = *player;
                if !*dodged {
                    self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::EvadeWindow) && f.owner == pl);
                    self.pool.add(Fx { kind: Kind::Aura(AuraKind::EvadeWindow), look: silver, owner: *player, dur: secs.clamp(0.05, 1.0), ..default() });
                    return;
                }
                self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::EvadeWindow) && f.owner == pl);
                let p = bv(*pos);
                let yaw = self.views.player(*player).map_or(0.0, |x| x.yaw);
                let feet = p - Vec3::Y * 1.0;
                pen.ghost(feet, yaw, 1.0, silver.main, silver.glow * 0.7, 0.6);
                prims::flash(pen, p, 0.8, silver.core, silver.glow * 1.4, 0.1);
                // Time bends: a slow wide ring off the dodge, a second one in the move's colour.
                pen.ring(&silver, p, Vec3::Y, 0.5, 5.0, 1.6, 0.9);
                pen.ring(&look, p, (p - v.cam).normalize_or(Vec3::Z), 0.4, 3.5, 1.2, 0.8);
                snd(w, "dodge", p, 0.6, 1.0);
                snd(w, "mv_time_tick", p, 0.7, 1.0);
                if *secs > 0.05 && self.gate.allow(pl, 2, self.now, 1.0) {
                    word(w, "slow", p + Vec3::Y * 0.9, 1.0, silver.main, (p.z * 97.0) as i64 as u64 ^ pl.0 as u64, self.involved(*player, *attacker));
                }
            }
            OpEvent::PortalShot { player, pos, normal, orange } => {
                let p = bv(*pos);
                let n = bv(*normal);
                let color = if *orange { [1.0, 0.45, 0.05] } else { [0.1, 0.45, 1.0] };
                let look = Look { main: color, core: [1.0, 0.95, 0.85], ..self.look(*player, None) };
                let (pl, or) = (*player, *orange);
                self.pool.kill(|f| f.kind == Kind::Portal && f.owner == pl && f.flag == or);
                if !self.opened.contains(&pl) {
                    self.pool.add(Fx { kind: Kind::Portal, look, owner: *player, a: p, dir: n, flag: *orange, dur: 20.0, ..default() });
                }
                if let Some(pv) = self.views.player(*player) {
                    // The shot: a fat bolt of portal colour with a white core, hand → wall.
                    let h = hand_of(pv, self.hands, self.local);
                    pen.beam(h, p, 0.14, color, 3.5, 0.16);
                    pen.beam(h, p, 0.05, [1.0, 0.97, 0.9], 5.0, 0.1);
                }
                prims::flash(pen, p + n * 0.15, 1.2, color, 4.0, 0.12);
                pen.ring(&look, p + n * 0.05, n, 0.2, 6.0, 1.2, 0.3);
                pen.burst(&look, p + n * 0.1, 0.8, 5.0, Some(n), 0.8, 0.35);
                snd(w, "mv_hardlight", p, if *orange { 0.85 } else { 1.15 }, 0.8);
            }
            OpEvent::Empowered { player, secs, on } => {
                let look = self.look(*player, None);
                let pl = *player;
                self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::Empower) && f.owner == pl);
                let Some(pv) = self.views.player(*player) else { return };
                if *on {
                    let rainbow = look.body == Body::Halo;
                    self.pool.add(Fx { kind: Kind::Aura(AuraKind::Empower), look, owner: *player, dur: secs.clamp(0.2, 10.0), flag: rainbow, ..default() });
                    pen.ring(&look, pv.pos + Vec3::Y * 0.1, Vec3::Y, 0.4, 9.0, 1.6, 0.4);
                    pen.burst(&look, chest(pv), 1.4, 6.0, Some(Vec3::Y), 1.0, 0.6);
                    prims::flash(pen, chest(pv), 0.9, look.core, look.glow, 0.12);
                    snd(w, "presence_manifest_big", chest(pv), 1.0, 0.9);
                    snd(w, "power_ready", chest(pv), 0.9, 0.4);
                } else {
                    let smoke = Look { body: Body::Smoke, ..look };
                    pen.burst(&smoke, chest(pv), 0.6, 2.0, None, 0.0, 0.5);
                    snd(w, "construct_dismiss", chest(pv), 0.8, 0.6);
                }
            }
            OpEvent::Captured { player, construct, pos, secs, on } => {
                let look = self.look(*player, None);
                let p = bv(*pos);
                let cid = *construct;
                if *on {
                    self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Captured) && f.construct == Some(cid));
                    self.pool.add(Fx { kind: Kind::Marker(MarkKind::Captured), look, owner: *player, construct: Some(cid), a: p, dur: secs.clamp(0.2, 10.0), ..default() });
                    self.pool.add(Fx { kind: Kind::Tether(TetherBody::Spirit), look, owner: *player, construct: Some(cid), b: p, phase: 1, dur: 0.6, ..default() });
                    pen.implode(&look, p, 2.0, 0.25, 1.0);
                    prims::flash(pen, p, 0.8, look.core, look.glow, 0.1);
                    snd(w, "summon_ping", p, 0.8, 0.9);
                    snd(w, "transform_combine", p, 1.3, 0.5);
                } else {
                    self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Captured) && f.construct == Some(cid));
                    pen.burst(&look, p, 1.2, 5.0, Some(Vec3::Y), 0.9, 0.5);
                    pen.ring(&look, p, Vec3::Y, 0.3, 5.0, 1.0, 0.3);
                    snd(w, "construct_dismiss", p, 1.1, 0.8);
                }
            }
            // ───────────── MOVIES & COMICS
            OpEvent::Zap { player, from, to, element, target } => {
                // A thick beam in the MOVE's colour on every leg, a hot core, a flare at each end.
                let look = self.look(*player, Some(*element));
                let (a, b) = (bv(*from), bv(*to));
                let d = (b - a).normalize_or(Vec3::Z);
                pen.beam(a, b, 0.24 * look.size, look.main, look.glow * 1.4, 0.45);
                pen.beam(a, b, 0.08 * look.size, look.core, look.glow * 2.4, 0.3);
                if look.body == Body::Bolt {
                    let rs = pen.fx.rng.next_u32();
                    zig(pen.fx, a, b, 10, 0.3, rs, 0.04, look.core, look.glow * 2.0);
                }
                prims::flash(pen, b, 1.0, look.core, look.glow * 1.5, 0.12);
                pen.ring(&look, b, d, 0.15, 4.0, 0.6, 0.25);
                pen.burst(&look, b, 0.6, 5.0, Some(-d), 0.9, 0.3);
                snd(w, "bfg_zap", b, 1.5, 0.35);
                // Afflict: the status sticks on the victim for its time (a daze over the eyes).
                if let Some(t) = target
                    && let Some((status, secs)) = op_of(self.mv(*player), |o| if let MoveOp::Afflict { status: Some(s), secs, .. } = o { Some((*s, *secs)) } else { None })
                    && matches!(status, StatusKind::Stun | StatusKind::Shadow | StatusKind::Float | StatusKind::Chill)
                {
                    self.marker(*player, *t, MarkKind::Dazed, look, secs.clamp(0.2, 2.5));
                }
            }
            OpEvent::Marked { player, target, secs, burst, point } => {
                let look = self.look(*player, Some(Element::Shadow));
                let t = *target;
                if !*burst {
                    self.marker(*player, *target, MarkKind::Delayed, look, secs.clamp(0.2, 12.0));
                    if let Some(tv) = self.views.player(*target) {
                        prims::flash(pen, chest(tv), 0.7, look.main, look.glow, 0.1);
                        snd(w, "storm_mark", chest(tv), 1.2, 0.6);
                    }
                    return;
                }
                self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Delayed) && f.target == Some(t));
                let p = bv(*point);
                let kind = match look.body {
                    Body::Void => StrikeKind::Void,
                    Body::Halo => StrikeKind::Holy,
                    _ => StrikeKind::Blast,
                };
                let rs = pen.fx.rng.next_u32();
                prims::strike(pen, &look, kind, p, None, 1.4, rs);
                // A dusting: the victim's shape crumbles upward in grit.
                let dust = Look { body: Body::Grain, main: look.main, ..look };
                pen.burst(&dust, p, 1.2, 3.0, Some(Vec3::Y), 1.2, 1.2);
                snd(w, "final_blow_suck", p, 0.9, 1.0);
                if self.involved(*player, Some(*target)) || self.near(p, 20.0) {
                    self.shake(w, 0.2, 0.1);
                }
            }
            // ───────────── MODERN ANIME
            OpEvent::Tagged { player, target, secs, strike } => {
                let look = self.look(*player, None);
                if !*strike {
                    self.marker(*player, *target, MarkKind::Tag, look, secs.clamp(0.3, 30.0));
                    if let Some(tv) = self.views.player(*target) {
                        prims::flash(pen, chest(tv), 0.6, look.core, look.glow, 0.08);
                        snd(w, "lodge_thunk", chest(tv), 1.1, 0.8);
                    }
                    return;
                }
                let t = *target;
                let tag_look = self.pool.find(|f| f.kind == Kind::Marker(MarkKind::Tag) && f.target == Some(t)).map(|f| {
                    f.live = false;
                    f.look
                });
                let look = if self.looks.get(*player).is_some() { look } else { tag_look.unwrap_or(look) };
                let Some(tv) = self.views.player(*target) else { return };
                let c = chest(tv);
                // RESONANCE: thorns burst out of the nail point, a dark-red ink bloom, a thump.
                for k in 0..7 {
                    let d = pen.fx.rng.dir();
                    let l = 0.9 + 0.5 * (k % 3) as f32;
                    pen.beam(c + d * 0.1, c + d * l, 0.06, if k % 2 == 0 { look.core } else { look.main }, look.glow * 1.8, 0.2);
                }
                prims::flash(pen, c, 0.8, look.core, look.glow * 1.3, 0.08);
                let ink = Look { body: Body::Liquid, main: look.main, ..look };
                pen.burst(&ink, c, 1.0, 5.0, None, 0.0, 0.5);
                pen.ring(&look, c, (c - v.cam).normalize_or(Vec3::Z), 0.2, 6.0, 1.0, 0.3);
                snd(w, "hook_slam", c, 0.8, 1.0);
                snd(w, "heartbeat", c, 1.4, 0.6);
                // A pull (Bungee Gum's snap): the band from you to them, reeling.
                if op_of(self.mv(*player), |o| if let MoveOp::TagStrike { pull, .. } = o { Some(*pull) } else { None }).is_some_and(|p| p > 0.0) {
                    let band = Look { body: Body::Rubber, ..look };
                    self.pool.add(Fx { kind: Kind::Tether(TetherBody::Bungee), look: band, owner: *player, target: Some(*target), b: c, phase: 1, dur: 0.8, ..default() });
                    snd(w, "mv_rubber_snap", c, 1.0, 0.9);
                }
                if self.involved(*player, Some(*target)) {
                    self.shake(w, 0.15, 0.05);
                }
            }
            // ───────────── SHONEN CLASSICS
            OpEvent::Toolbox { player, fx, from, to, secs, size } => self.toolbox(*player, *fx, bv(*from), bv(*to), *secs, *size, pen, w),
            OpEvent::Twisted { .. } => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn toolbox(&mut self, player: PlayerId, fx: ToolboxFx, from: Vec3, to: Vec3, secs: f32, size: f32, pen: &mut Pen, w: &mut Writers) {
        let el = op_of(self.mv(player), |o| match o {
            MoveOp::Drill { element, .. } | MoveOp::Wave { element, .. } => Some(*element),
            _ => None,
        });
        let look = self.look(player, el);
        let v = self.view;
        match fx {
            ToolboxFx::DrillStart => {
                // The orb held OUT in the palm for the whole lunge + grind.
                let bolt = look.body == Body::Bolt || el == Some(Element::Electric);
                self.pool.kill(|f| f.kind == Kind::Orb && f.owner == player);
                self.pool.add(Fx { kind: Kind::Orb, look, owner: player, dur: (secs + size).clamp(0.1, 4.0) + 0.1, flag: bolt, dir: (to - from).normalize_or(Vec3::NEG_Z), ..default() });
                pen.implode(&look, from, 1.2, 0.15, 0.8);
                snd(w, if bolt { "storm_arc" } else { "bfg_spinup" }, from, if bolt { 1.4 } else { 1.6 }, 0.9);
            }
            ToolboxFx::DrillLatch => {
                if let Some(f) = self.pool.find(|f| f.kind == Kind::Orb && f.owner == player) {
                    f.phase = 1;
                    f.b = to;
                    f.target = nearest_player(self.views, to, 2.5).filter(|t| *t != player);
                    f.t = 0.0;
                    f.dur = secs.clamp(0.05, 3.0) + 0.05;
                }
                prims::flash(pen, to, 0.8, look.core, look.glow * 1.3, 0.08);
                snd(w, "gun_minigun_spinup", to, 1.6, 0.7);
                snd(w, "hit_heavy", to, 0.9, 0.8);
                if self.involved(player, nearest_player(self.views, to, 2.5)) {
                    self.shake(w, 0.18, 0.0);
                }
            }
            ToolboxFx::DrillRelease => {
                self.pool.kill(|f| f.kind == Kind::Orb && f.owner == player);
                let d = (to - from).normalize_or(Vec3::NEG_Z);
                // The spiral shockwave out along the look + the blast.
                for k in 0..4 {
                    pen.ring(&look, to + d * k as f32 * 0.9, d, 0.3 + 0.2 * k as f32, 6.0, 1.0, 0.25 + 0.05 * k as f32);
                }
                prims::flash(pen, to, 1.1, look.core, look.glow * 1.5, 0.1);
                pen.burst(&look, to, 1.6, 10.0, Some(d), 0.6, 0.45);
                snd(w, "charge_release", to, 1.0, 1.0);
                snd(w, "explosion_small", to, 1.1, 0.8);
                if self.involved(player, nearest_player(self.views, to, 3.0)) || self.near(to, 10.0) {
                    self.shake(w, 0.25, 0.08);
                }
            }
            ToolboxFx::DrillEnd => {
                self.pool.kill(|f| f.kind == Kind::Orb && f.owner == player);
                let gust = Look { body: Body::Gust, ..look };
                pen.burst(&gust, to, 1.2, 6.0, None, 0.0, 0.4);
                pen.ring(&look, to, (to - from).normalize_or(Vec3::Y), 0.2, 5.0, 1.0, 0.3);
                snd(w, "charge_break", to, 1.2, 0.8);
            }
            ToolboxFx::Mark => {
                // A kunai streak, then the seal where it stuck (riding a player if it hit one).
                pen.beam(from, to, 0.05, look.core, look.glow * 2.0, 0.15);
                let yellow = Look { main: [1.0, 0.85, 0.2], core: [1.0, 1.0, 0.8], ..look };
                let rider = if size >= 1.0 { nearest_player(self.views, to, 2.0).filter(|t| *t != player) } else { None };
                self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Seal) && f.owner == player);
                let dur = op_of(self.mv(player), |o| if let MoveOp::MarkRecall { secs, .. } = o { Some(*secs) } else { None }).unwrap_or(secs).clamp(0.5, 30.0);
                self.pool.add(Fx { kind: Kind::Marker(MarkKind::Seal), look: yellow, owner: player, target: rider, a: to, dur, ..default() });
                snd(w, "lodge_thunk", to, 1.3, 0.8);
            }
            ToolboxFx::Recall => {
                // Flying Thunder God: vanish in a yellow streak, flash out at the seal. Not a bamf.
                let yellow = Look { main: [1.0, 0.85, 0.2], core: [1.0, 1.0, 0.8], body: Body::Streak, ..look };
                let up = Vec3::Y * 1.0;
                pen.beam(from + up, to + up, 0.12, yellow.main, yellow.glow * 2.0, 0.2);
                pen.beam(from + up, to + up, 0.04, yellow.core, yellow.glow * 3.0, 0.14);
                prims::flash(pen, from + up, 1.0, yellow.core, yellow.glow, 0.08);
                prims::flash(pen, to + up, 0.9, yellow.core, yellow.glow * 1.5, 0.1);
                pen.ring(&yellow, to + Vec3::Y * 0.1, Vec3::Y, 0.3, 6.0, 1.0, 0.3);
                self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Seal) && f.owner == player);
                snd(w, "recall_blink", to, 1.2, 1.0);
                snd(w, "mv_speed_zip", to, 1.3, 0.7);
            }
            ToolboxFx::Warp => {
                // Instant Transmission: the body squeezes to a flickering line and is gone; a soft
                // ring and a "shunk" where it reappears.
                let up = Vec3::Y * 1.0;
                pen.beam(from, from + up * 2.0, 0.05, look.core, look.glow * 2.0, 0.12);
                prims::flash(pen, from + up, 0.7, look.main, look.glow, 0.06);
                pen.ring(&look, to + up, (to + up - v.cam).normalize_or(Vec3::Z), 0.2, 3.0, 0.8, 0.25);
                pen.beam(to, to + up * 2.0, 0.08, look.main, look.glow * 1.6, 0.1);
                snd(w, "recall_blink", from, 0.8, 0.8);
                snd(w, "recall_catch", to, 0.7, 0.8);
            }
            ToolboxFx::Copy => {
                // The red eye flares: tomoe spin at the face.
                let red = Look { main: [1.0, 0.08, 0.08], core: [1.0, 0.7, 0.6], ..look };
                prims::flash(pen, from, 0.5, red.main, red.glow * 1.5, 0.2);
                pen.ring(&red, from, (from - v.cam).normalize_or(Vec3::Z), 0.05, 1.5, 0.6, 0.35);
                snd(w, "reveal_pulse", from, 0.8, 0.7);
            }
            ToolboxFx::Gaze => {
                // A thin red line eye to eye; the victim is dazed for the status time.
                let red = Look { main: [1.0, 0.1, 0.12], core: [1.0, 0.75, 0.7], ..look };
                pen.beam(from, to, 0.025, red.core, red.glow * 2.5, 0.35);
                prims::flash(pen, to, 0.4, red.main, red.glow, 0.15);
                if let Some(t) = nearest_player(self.views, to, 1.5).filter(|t| *t != player) {
                    self.marker(player, t, MarkKind::Dazed, red, secs.clamp(0.2, 3.0));
                }
                snd(w, "tinnitus", to, 1.3, 0.35);
            }
            ToolboxFx::Flash => {
                // A white nova off the face: only the people who looked get it (the sim decides).
                let white = Look { main: [1.0, 1.0, 0.95], core: [1.0, 1.0, 1.0], ..look };
                if !self.near(from, 2.0) {
                    prims::flash(pen, from, 1.6, white.core, white.glow * 2.0, 0.14);
                }
                pen.ring(&white, from, Vec3::Y, 0.4, size.clamp(2.0, 30.0) * 2.2, 1.6, 0.4);
                snd(w, "storm_nova", from, 1.3, 0.9);
            }
            ToolboxFx::PhaseOn => {
                self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::Phase) && f.owner == player);
                self.pool.add(Fx { kind: Kind::Aura(AuraKind::Phase), look, owner: player, dur: secs.clamp(0.1, 3.0), ..default() });
                pen.implode(&look, from, 1.4, 0.2, 1.0);
                snd(w, "mv_void_thump", from, 1.4, 0.7);
            }
            ToolboxFx::PhaseOff => {
                self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::Phase) && f.owner == player);
                pen.burst(&look, from, 0.6, 3.0, None, 0.0, 0.3);
                snd(w, "mv_void_thump", from, 1.0, 0.5);
            }
            ToolboxFx::Banish => {
                // A spiral sucks the thing in; it's spat out where it went.
                let swirl = Look { body: Body::Void, ..look };
                pen.implode(&swirl, from, 2.4, 0.3, 1.6);
                for k in 0..3 {
                    let n = Vec3::new((k as f32 * 1.3).cos(), 0.4, (k as f32 * 1.3).sin()).normalize();
                    pen.ring(&swirl, from, n, 1.8, -5.0, 0.8, 0.35);
                }
                if from.distance_squared(to) > 1.0 {
                    pen.burst(&swirl, to, 1.0, 4.0, None, 0.0, 0.4);
                }
                snd(w, "final_blow_suck", from, 1.1, 0.9);
                snd(w, "mv_void_thump", to, 1.0, 0.7);
            }
            ToolboxFx::Rush => {
                self.pool.kill(|f| f.kind == Kind::Aura(AuraKind::Rush) && f.owner == player);
                self.pool.add(Fx { kind: Kind::Aura(AuraKind::Rush), look, owner: player, dur: secs.clamp(0.1, 4.0), ..default() });
            }
            ToolboxFx::Brand => {
                let black = Look { body: Body::Flame, main: [0.6, 0.05, 0.3], core: [0.9, 0.2, 0.5], ..look };
                if let Some(t) = nearest_player(self.views, to, 2.5) {
                    self.marker(player, t, MarkKind::Brand, black, secs.clamp(0.2, 12.0));
                    if size >= 1.0 {
                        // It jumped: a lick of black fire across the gap.
                        pen.beam(from, to, 0.12, [0.02, 0.0, 0.02], 0.2, 0.3);
                    }
                }
                let smoke = Look { body: Body::Smoke, main: [0.03, 0.01, 0.03], dark: [0.02, 0.0, 0.02], ..look };
                pen.burst(&smoke, to, 1.0, 2.0, Some(Vec3::Y), 0.6, 0.6);
                snd(w, "mv_fire_whoomp", to, 0.6, 0.9);
            }
            ToolboxFx::BrandEnd => {
                if let Some(t) = nearest_player(self.views, to, 3.0) {
                    self.pool.kill(|f| f.kind == Kind::Marker(MarkKind::Brand) && f.target == Some(t));
                }
                let smoke = Look { body: Body::Smoke, main: [0.08, 0.06, 0.08], ..look };
                pen.burst(&smoke, to, 0.6, 1.5, Some(Vec3::Y), 0.5, 0.8);
            }
            ToolboxFx::Wave => {
                let flat = op_of(self.mv(player), |o| if let MoveOp::Wave { width, .. } = o { Some(*width) } else { None }).is_some_and(|wd| wd <= 2.0) && matches!(look.body, Body::Halo | Body::Energy);
                self.pool.add(Fx { kind: Kind::Crescent, look, owner: player, a: from + Vec3::Y * 1.0, b: to + Vec3::Y * 1.0, r: size.clamp(0.5, 12.0), dur: secs.clamp(0.05, 5.0), flag: flat, ..default() });
                prims::flash(pen, from + Vec3::Y, 0.7, look.core, look.glow * 1.2, 0.08);
                snd(w, "blade_wave", from, 1.0, 1.0);
            }
        }
    }
}

/// How far a gaze move reaches and its cone half-angle (radians; 0 = a line).
fn gaze_reach(mv: &MoveDef) -> (f32, f32) {
    let mut reach: f32 = 0.0;
    let mut half: f32 = 0.0;
    for op in &mv.ops {
        match op {
            MoveOp::Carve { length, .. } => reach = reach.max(*length),
            MoveOp::Shove { range, angle, .. } => {
                reach = reach.max(*range);
                half = half.max((angle * 0.5).to_radians());
            }
            MoveOp::Afflict { range, .. } | MoveOp::GazeStun { range, .. } => reach = reach.max(*range),
            _ => {}
        }
    }
    (if reach > 0.5 { reach.min(80.0) } else { 25.0 }, half.clamp(0.0, 1.2))
}

/// A grab's claw: four talons closing on the point.
fn claw(pen: &mut Pen, look: &Look, p: Vec3, size: f32) {
    for k in 0..4 {
        let a = k as f32 * PI * 0.5 + 0.4;
        let d = Vec3::new(a.cos(), 0.6 * if k % 2 == 0 { 1.0 } else { -1.0 }, a.sin()).normalize();
        pen.beam(p + d * 0.9 * size, p + d * 0.2 * size, 0.06 * size, look.main, look.glow * 1.5, 0.18);
    }
    prims::flash(pen, p, 0.9 * size, look.core, look.glow, 0.1);
}

fn status_element(s: StatusKind) -> Element {
    match s {
        StatusKind::Burn => Element::Fire,
        StatusKind::Poison => Element::Poison,
        StatusKind::Chill => Element::Ice,
        StatusKind::Stun => Element::Electric,
        StatusKind::Float => Element::Psychic,
        StatusKind::Shadow => Element::Shadow,
        StatusKind::Wet => Element::Water,
    }
}

/// Per frame: every live op visual draws / follows / ends; lock channels grow their marks.
#[allow(clippy::too_many_arguments)]
fn step(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    table: Res<MoveFx>,
    mut pool: ResMut<Pool>,
    mut locks: ResMut<Locks>,
    mut fx: ResMut<FxParticles>,
    hands: Res<ViewmodelHands>,
    settings: Option<Res<bd_fx::FxSettings>>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    mut stats: ResMut<OpVisStats>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else { return };
    let any_lock = locks.0.iter().any(|c| c.live);
    if !any_lock && !pool.fx.iter().any(|f| f.live) {
        stats.live = 0;
        return;
    }
    let t0 = std::time::Instant::now();
    let dt = time.delta_secs().min(0.1);
    let v = view_of(&cams);
    let budget = settings.as_ref().map_or(1.0, |s| s.particle_budget).clamp(0.15, 1.0);
    let mut pen = Pen { fx: &mut fx, motes: table.t.beats.motes * budget, scale: 1.0, cam: v.cam, avoid: None };
    pen.fx.scope(v.cam, LINE_FLOOR, IMM_BUDGET);
    let local = active.local;
    let fp = hands.active;
    let pool = &mut *pool;
    for f in pool.fx.iter_mut().filter(|f| f.live) {
        f.t += dt;
        if f.t >= f.dur {
            f.live = false;
            end_fx(f, &mut pen, &mut sfx, &views, &active);
            continue;
        }
        let left = f.dur - f.t;
        let u = (f.t / f.dur.max(1e-3)).clamp(0.0, 1.0);
        let look = f.look;
        let owner = views.player(f.owner).filter(|p| p.alive());
        let fade = (left / 0.25).clamp(0.0, 1.0) * (f.t / 0.08).min(1.0);
        match f.kind {
            Kind::Marker(kind) => {
                let (feet, size) = if let Some(t) = f.target {
                    let Some(tv) = views.player(t).filter(|p| p.alive()) else {
                        f.live = false;
                        continue;
                    };
                    (tv.pos, tv.size)
                } else if let Some(cid) = f.construct {
                    let Some(cv) = views.construct(cid) else {
                        f.live = false;
                        continue;
                    };
                    let s = cv.size.clamp(0.6, 4.0);
                    (cv.pos - Vec3::Y * 0.8 * s, s)
                } else {
                    (f.a, 0.3)
                };
                if f.target.is_some() && f.target == local && fp {
                    // On YOU in first person: a small glyph low in your view (never over the aim).
                    prims::self_marker(&mut pen, &v, &look, kind, f.t, left, f.dur, 1.0);
                } else {
                    prims::marker(&mut pen, &v, &look, kind, feet, size, f.t, left, f.dur, 1.0, f.seed);
                }
                if kind == MarkKind::Delayed
                    && feet.distance(v.cam) < 30.0
                    && (f.t * if left < 1.0 { 4.0 } else { 2.0 }) as u32 != ((f.t - dt) * if left < 1.0 { 4.0 } else { 2.0 }) as u32 {
                    sfx.write(Sfx3d::at("needle_tick", feet + Vec3::Y).pitch(if left < 1.0 { 1.3 } else { 1.0 }).gain(0.5));
                }
            }
            Kind::Telegraph(kind) => {
                let wait = f.acc.max(0.05);
                let uu = (f.t / wait).clamp(0.0, 1.0);
                let g = f.flag.then_some(f.b);
                prims::telegraph(&mut pen, &v, &look, kind, f.a, g, f.r, uu, (wait - f.t).max(0.0), f.t, f.seed);
            }
            Kind::Bolt => {
                prims::bolt_now(&mut pen, &look, f.a, f.b, u, f.seed.wrapping_add((f.t * 45.0) as u32));
            }
            Kind::Tether(body) => {
                let Some(ov) = owner else {
                    f.live = false;
                    continue;
                };
                let from = if matches!(body, TetherBody::Spirit) { chest(ov) } else { hand_of(ov, &hands, local) };
                let tip = match f.phase {
                    0 => {
                        // Flying out exactly as the host steps it (pos + dir × speed × t, ≤ range).
                        let flown = (f.acc * f.t).min(f.r);
                        if flown >= f.r {
                            f.phase = 3;
                            f.b = f.a + f.dir * f.r;
                            f.t = 0.0;
                            f.dur = 0.25;
                        }
                        f.a + f.dir * flown
                    }
                    1 => {
                        if let Some(tp) = f.target.and_then(|t| views.player(t)).filter(|p| p.alive()) {
                            f.b = chest(tp);
                        } else if let Some(cv) = f.construct.and_then(|c| views.construct(c)) {
                            f.b = cv.pos;
                        }
                        f.b
                    }
                    2 => f.b,
                    5 => {
                        // A torn-out chunk on the line: follow the real chunk once it's found
                        // (the owner's newest loose construct near where it should be), shedding grit.
                        let guess = f.a.lerp(f.b, (f.t / f.acc.max(0.05)).clamp(0.0, 1.0));
                        if f.construct.is_none() && f.t < 0.4 {
                            f.construct = views.constructs.iter().filter(|c| c.owner == f.owner && !c.attached() && c.pos.distance(guess) < 4.0).max_by_key(|c| c.id.0).map(|c| c.id);
                        }
                        let tip = f.construct.and_then(|c| views.construct(c)).map_or(guess, |c| c.pos);
                        if pen.fx.rng.f() < 0.5 {
                            let grit = Look { body: Body::Grain, ..look };
                            let rd = pen.fx.rng.dir();
                            pen.mote(&grit, tip + rd * 0.6, Vec3::new(rd.x, -1.0, rd.z), 0.6, 0.0);
                        }
                        tip
                    }
                    _ => {
                        // Retracting: the tip snaps home.
                        let k = (f.t / f.dur.max(0.05)).clamp(0.0, 1.0);
                        f.b.lerp(from, k * k)
                    }
                };
                let slack = match f.phase {
                    0 => 0.15,
                    3 => 0.8,
                    _ => 0.0,
                };
                prims::tether(&mut pen, &v, &look, body, from, tip, f.t, slack, f.phase != 5 || body == TetherBody::Chain, f.seed);
            }
            Kind::Aura(kind) => {
                let Some(ov) = owner else {
                    f.live = false;
                    continue;
                };
                let own_fp = Some(ov.id) == local && fp;
                let (_, front) = eye_look(ov);
                if own_fp && matches!(kind, AuraKind::Empower | AuraKind::Phase | AuraKind::Mend) {
                    // Your own state: a thin band low in your view (never over the aim).
                    prims::self_marker(&mut pen, &v, &look, MarkKind::Seal, f.t, left, f.dur, 1.0);
                }
                prims::aura(&mut pen, &v, &look, kind, ov.pos, ov.size, front, f.t, left, fade.max(0.3), f.r, f.flag, own_fp);
                if kind == AuraKind::Phase && !own_fp && (f.t * 8.0) as u32 != ((f.t - dt) * 8.0) as u32 {
                    pen.ghost(ov.pos, ov.yaw, ov.size, look.main, look.glow * 0.4, 0.18);
                }
            }
            Kind::Crescent => {
                let pos = f.a.lerp(f.b, u);
                prims::crescent(&mut pen, &v, &look, pos, f.b - f.a, f.r, f.flag, f.t, look.glow * fade.max(0.4));
            }
            Kind::Orb => {
                let Some(ov) = owner else {
                    f.live = false;
                    continue;
                };
                let r = 0.4 * ov.size;
                let (hand, fwd) = if Some(ov.id) == local && fp {
                    (hands.world[1] + v.fwd * 0.25, v.fwd)
                } else {
                    let (_, d) = eye_look(ov);
                    (hand_of(ov, &hands, local) + d * 0.3, d)
                };
                if f.phase == 1 {
                    // Grinding INTO them: at the contact, shreds spiralling off, both shake.
                    let c = f.target.and_then(|t| views.player(t)).map_or(f.b, |tp| chest(tp).lerp(hand, 0.35));
                    let jig = pen.fx.rng.dir() * 0.04;
                    prims::orb(&mut pen, &v, &look, c + jig, r * 1.4, f.t + 3.0, f.flag, look.glow * 1.3);
                    prims::grind_shreds(&mut pen, &look, c, fwd, f.t);
                    if (f.t * 14.0) as u32 != ((f.t - dt) * 14.0) as u32 {
                        sfx.write(Sfx3d::at("hit_light", c).pitch(1.4 + u).gain(0.4));
                    }
                } else {
                    prims::orb(&mut pen, &v, &look, hand, r, f.t, f.flag, look.glow);
                    if pen.fx.rng.f() < 0.5 {
                        let rd = pen.fx.rng.dir();
                        pen.mote(&look, hand, -fwd * 2.0 + rd, 0.25, 0.0);
                    }
                }
            }
            Kind::Arc => {
                let own = Some(f.owner) == local && fp;
                prims::arc_preview(&mut pen, &v, &look, f.a, f.b, if own { -1.0 } else { f.r }, Some(f.b), u, f.t);
            }
            Kind::Surge => {
                // The front's foot: a foam line racing along the run, spray off its top.
                let p = f.a + f.dir * f.acc * f.t;
                let side = Vec3::Y.cross(f.dir).normalize_or(Vec3::X);
                let half = f.r * 0.5;
                let g = p + Vec3::Y * 0.1;
                prims::seg(&mut pen, &v, g - side * half, g + side * half, 0.12, look.main, look.glow * fade);
                prims::seg(&mut pen, &v, g - side * half + f.dir * 0.3, g + side * half + f.dir * 0.3, 0.05, look.core, look.glow * 1.4 * fade);
                // The wall's face: body motes climbing it (water foam, sand, lava), a crest line on top.
                let crest = (1.6 + f.r * 0.12).min(4.0);
                for _ in 0..3 {
                    let q = p + side * (pen.fx.rng.f() * 2.0 - 1.0) * half + Vec3::Y * (0.3 + pen.fx.rng.f() * crest);
                    pen.mote(&look, q, f.dir * f.acc * 0.7 + Vec3::Y * 1.5, 0.45, 0.0);
                }
                let top = p + Vec3::Y * crest - f.dir * 0.4;
                prims::seg(&mut pen, &v, top - side * half * 0.9, top + side * half * 0.9, 0.08, look.core, look.glow * 0.8 * fade);
            }
            Kind::Wheel => {
                let Some(ov) = owner else {
                    f.live = false;
                    continue;
                };
                if Some(ov.id) == local && fp {
                    // Your own wheel: top of the view, small.
                    let c = v.cam + v.fwd * 2.5 + v.up * 0.85;
                    prims::wheel(&mut pen, &v, &look, c, f.n, f.phase, (f.t / 0.8).min(1.0), look.glow * fade.max(0.3));
                } else {
                    let c = ov.pos + Vec3::Y * 2.7 * ov.size;
                    prims::wheel(&mut pen, &v, &look, c, f.n, f.phase, (f.t / 0.8).min(1.0), look.glow * fade.max(0.3));
                }
                if f.t >= 0.8 && f.t - dt < 0.8 {
                    // The reveal: the chosen pip bursts.
                    let c = if Some(ov.id) == local && fp { v.cam + v.fwd * 2.5 + v.up * 0.85 } else { ov.pos + Vec3::Y * 2.7 * ov.size };
                    let top = c + v.up * 0.45;
                    prims::flash(&mut pen, top, 0.6, look.core, look.glow * 1.6, 0.15);
                    pen.burst(&look, top, 0.8, 3.0, None, 0.0, 0.4);
                    sfx.write(Sfx3d::at("summon_ping", c).pitch(1.6).gain(0.8));
                }
                // A tick per pip passing the pointer (slowing down with the wheel).
                let pips = |t: f32| ((1.0 - (1.0 - (t / 0.8).clamp(0.0, 1.0)).powi(3)) * 3.0 * f.n.max(2) as f32) as u32;
                if pips(f.t) != pips(f.t - dt) && f.t < 0.8 {
                    sfx.write(Sfx3d::at("type_key", ov.pos + Vec3::Y * 2.0).pitch(1.4).gain(0.35));
                }
            }
            Kind::Portal => {
                prims::portal_oval(&mut pen, &v, f.a, f.dir, look.main, f.t, look.glow * fade.max(0.5));
            }
            Kind::Zone => {
                // The inside of a lingering zone: status motes drifting in the disc.
                let body = match f.n {
                    0 => Body::Flame,
                    1 => Body::Smoke,
                    2 => Body::Shard,
                    3 => Body::Bolt,
                    4 => Body::Halo,
                    5 => Body::Smoke,
                    6 => Body::Liquid,
                    254 => Body::Glitch,
                    _ => look.body,
                };
                let zl = Look { body, ..look };
                let n = if f.r > 6.0 { 3 } else { 2 };
                for _ in 0..n {
                    let a = pen.fx.rng.f() * std::f32::consts::TAU;
                    let rr = f.r * pen.fx.rng.f().sqrt();
                    let q = f.a + Vec3::new(a.cos() * rr, pen.fx.rng.f() * 1.5 - 0.6, a.sin() * rr);
                    let vel = if body == Body::Shard { Vec3::new(0.3, -1.2, 0.2) } else { Vec3::Y * 0.8 };
                    pen.mote(&zl, q, vel, 0.6, 0.0);
                }
            }
            Kind::Trail => {
                // Bind to the thrown construct (it may appear a frame after the launch).
                if f.construct.is_none() {
                    let near = views.constructs.iter().filter(|c| c.owner == f.owner && !c.attached() && c.pos.distance(f.a) < 6.0).max_by_key(|c| c.id.0).map(|c| c.id);
                    f.construct = near;
                    if near.is_none() {
                        if f.t > 0.6 {
                            f.live = false;
                        }
                        continue;
                    }
                }
                let Some(cv) = f.construct.and_then(|c| views.construct(c)) else {
                    f.live = false;
                    continue;
                };
                let p = cv.pos;
                if f.flag {
                    // A fading ribbon laid behind it: every bend of the homing path stays readable.
                    let sv = p - f.b;
                    if sv.length_squared() > 0.01 {
                        pen.beam(f.b, p, 0.16 * look.size, look.main, look.glow * 1.1, 0.5);
                        pen.beam(f.b, p, 0.05 * look.size, look.core, look.glow * 2.0, 0.3);
                        let rd = pen.fx.rng.dir();
                        pen.mote(&look, p, rd * 0.6 - sv.normalize_or(Vec3::Z) * 2.0, 0.35, 0.0);
                        // Sharp turns flare (the homing reads as a decision, not a drift).
                        let turn = f.dir.dot(sv.normalize_or(f.dir));
                        if turn < 0.9 && f.n > 2 {
                            prims::flash(&mut pen, p, 0.5, look.core, look.glow, 0.08);
                        }
                        f.dir = sv.normalize_or(f.dir);
                        f.n = f.n.saturating_add(1);
                    }
                }
                f.flag = true;
                f.b = p;
                // The whistle: a rising sweep every third of a second while it flies.
                if (f.t * 3.0) as u32 != ((f.t - dt) * 3.0) as u32 {
                    sfx.write(Sfx3d::at("leap_whoosh", p).pitch(1.6 + 0.1 * ((f.t * 3.0) as u32 % 3) as f32).gain(0.45));
                }
            }
            Kind::Gaze => {
                let Some(ov) = owner else {
                    f.live = false;
                    continue;
                };
                // Out of the eyes along the LIVE look (it sweeps with the head), to the first wall.
                let (eye, look_q, _) = super::super::constructs::body_frames(ov);
                let fwd = look_q * Vec3::NEG_Z;
                let right = look_q * Vec3::X;
                let reach = bd_core::voxel::raycast(&active.world.voxels, gv(eye), gv(fwd), f.r).map_or(f.r, |h| h.t.max(0.5));
                let end = eye + fwd * reach;
                let k = fade * (1.0 - 0.4 * u);
                let own = Some(ov.id) == local && fp;
                for sgn in [-1.0f32, 1.0] {
                    let e = eye + right * sgn * 0.065 * ov.size + fwd * 0.12;
                    prims::seg(&mut pen, &v, e, end, 0.09 * look.size * (1.0 - 0.5 * u), look.main, look.glow * 1.3 * k);
                    prims::seg(&mut pen, &v, e, end, 0.03 * look.size, look.core, look.glow * 2.4 * k);
                    if !own {
                        prims::seg(&mut pen, &v, e - right * sgn * 0.05, e + right * sgn * 0.05, 0.06, look.core, look.glow * 3.0 * k);
                    }
                }
                if f.acc > 0.05 {
                    // A cone move: the edges of the gaze fan out.
                    let upv = look_q * Vec3::Y;
                    for (a, b) in [(right, 1.0f32), (right, -1.0), (upv, 0.6), (upv, -0.6)] {
                        let d = (fwd + a * b * f.acc.tan()).normalize_or(fwd);
                        prims::seg(&mut pen, &v, eye + fwd * 0.5, eye + d * reach * 0.9, 0.025, look.main, look.glow * 0.8 * k);
                    }
                }
                if pen.fx.rng.f() < 0.7 {
                    let rd = pen.fx.rng.dir();
                    pen.mote(&look, end, rd * 2.0 - fwd * 2.0, 0.3, 0.0);
                }
            }
            Kind::Sight => {
                let (Some(ov), Some(tv)) = (owner, f.target.and_then(|t| views.player(t))) else {
                    f.live = false;
                    continue;
                };
                let (a, b) = (chest(ov), chest(tv));
                dashed(&mut pen, &v, a, b, 0.03, look.main, look.glow * fade, f.t);
            }
        }
    }
    // Lock-on channels: grow the marks the way the host does (in the caster's cone + line of sight),
    // draw brackets on every target and a sightline back to the gunslinger.
    for ch in locks.0.iter_mut().filter(|c| c.live) {
        ch.t += dt;
        let Some(ov) = views.player(ch.owner).filter(|p| p.alive()).filter(|_| ch.t < ch.secs + 0.6) else {
            ch.live = false;
            continue;
        };
        let (eye, look_dir) = eye_look(ov);
        let full = (ch.secs * 0.8).max(0.1);
        let owner = ch.owner;
        for tv in views.players.iter().filter(|p| p.alive() && p.id != owner) {
            let c = chest(tv);
            let d = c - eye;
            let len = d.length();
            let seen = len <= ch.range && len > 0.5 && d.dot(look_dir) / len >= ch.cos && bd_core::voxel::raycast(&active.world.voxels, gv(eye), gv(d / len), len - 0.4).is_none();
            if seen && let Some(k) = ch.slot(tv.id) {
                ch.fr[k] = (ch.fr[k] + dt / full).min(1.0);
            }
        }
        let look = ch.look;
        for k in 0..8 {
            let (Some(id), fr) = (ch.ids[k], ch.fr[k]) else { continue };
            if fr < 0.02 {
                continue;
            }
            let Some(tv) = views.player(id).filter(|p| p.alive()) else { continue };
            if Some(id) == local && fp {
                prims::self_marker(&mut pen, &v, &look, MarkKind::Lock, ch.t, ch.secs - ch.t, ch.secs, fr);
            } else {
                prims::marker(&mut pen, &v, &look, MarkKind::Lock, tv.pos, tv.size, ch.t, ch.secs - ch.t, ch.secs, fr, k as u32);
            }
            // The sightline: the marked player can see where it comes from (break line of sight!).
            if fr > 0.15 {
                dashed(&mut pen, &v, eye, chest(tv), 0.012 + 0.02 * fr, look.main, look.glow * (0.3 + 0.9 * fr), ch.t);
            }
        }
    }
    let imm_dropped = pen.fx.end_scope();
    let live = pool.fx.iter().filter(|f| f.live).count();
    *stats = OpVisStats { live, peak: pool.peak, started: pool.started, step_us: t0.elapsed().as_secs_f32() * 1e6, imm_dropped };
}

/// What an entry does as it ends: a leap lands in a crater, a crescent dissipates.
fn end_fx(f: &Fx, pen: &mut Pen, sfx: &mut MessageWriter<Sfx3d>, views: &WorldViews, active: &ActiveWorld) {
    let look = f.look;
    match f.kind {
        Kind::Arc => {
            // Landed (if they did land here): a crater ring, a dust wall, radial cracks.
            let Some(ov) = views.player(f.owner).filter(|p| p.alive()) else { return };
            if ov.pos.distance(f.b) > 4.0 {
                return;
            }
            let g = ground_below(active, ov.pos + Vec3::Y * 0.5, 3.0).unwrap_or(f.b);
            pen.ring(&look, g + Vec3::Y * 0.12, Vec3::Y, 0.4, 9.0, 1.6, 0.35);
            pen.dust_wall(&look, g, 1.3, 2.2, 1.2);
            for k in 0..5u32 {
                let a = k as f32 / 5.0 * std::f32::consts::TAU + f.seed as f32 * 1e-3;
                pen.crack(&look, g, Vec3::new(a.cos(), 0.0, a.sin()), 2.2 + (k % 2) as f32, f.seed.wrapping_add(k));
            }
            sfx.write(Sfx3d::at("leap_slam", g).gain(0.9));
        }
        Kind::Crescent => {
            let d = (f.b - f.a).normalize_or(Vec3::NEG_Z);
            pen.burst(&look, f.b, 0.8, 4.0, Some(d), 0.9, 0.35);
        }
        _ => {}
    }
}

/// A dashed line a → b, dashes crawling toward b.
fn dashed(pen: &mut Pen, v: &View, a: Vec3, b: Vec3, thick: f32, color: [f32; 3], glow: f32, t: f32) {
    let len = a.distance(b);
    if len < 0.1 {
        return;
    }
    let n = ((len / 0.8) as usize).clamp(2, 60);
    let shift = (t * 3.0).fract();
    for i in 0..n {
        let f0 = ((i as f32 + shift) / n as f32).min(1.0);
        let f1 = ((i as f32 + shift + 0.5) / n as f32).min(1.0);
        prims::seg(pen, v, a.lerp(b, f0), a.lerp(b, f1), thick, color, glow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_sim::powers::ops::LeapArc;

    fn all_events() -> Vec<OpEvent> {
        let p = PlayerId(1);
        let z = bd_core::Vec3::ZERO;
        let mut v = vec![
            OpEvent::Rewound { player: p, from: z, to: z, path: [z; 8] },
            OpEvent::Ram { player: p, phase: RamPhase::Slam, target: None, pos: z, dir: z },
            OpEvent::Hook { player: p, phase: HookPhase::Fired, target: None, pos: z, dir: z, speed: 1.0, range: 1.0 },
            OpEvent::Grab { player: p, phase: GrabPhase::Seize, target: None, pos: z, dir: z },
            OpEvent::Stance { player: p, kind: StanceKind::Counter, secs: 1.0, angle: 0.0 },
            OpEvent::Parried { player: p, kind: StanceKind::Counter, attacker: None, pos: z, dir: z },
            OpEvent::LockOn { player: p, phase: LockPhase::Start, target: None, frac: 0.0, secs: 1.0, hits: 0 },
            OpEvent::LockedOut { player: p, target: p, secs: 1.0 },
            OpEvent::Zone { player: p, field: None, pos: z, radius: 1.0, secs: 1.0, status: None, slow: 0.0, lockout: false },
            OpEvent::Seek { player: p, phase: SeekPhase::Launch, target: None, pos: z, dir: z },
            OpEvent::Haul { player: p, phase: HaulPhase::Tear, target: None, from: z, to: z, secs: 1.0 },
            OpEvent::Leap(LeapArc { player: p, from: z, to: z, secs: 1.0, apex: 1.0 }),
            OpEvent::Surge { player: p, phase: SurgePhase::Start, target: None, pos: z, dir: z, width: 1.0, speed: 1.0, dist: 1.0 },
            OpEvent::Roulette { player: p, pick: 0, of: 3 },
            OpEvent::Pounce { player: p, phase: PouncePhase::Start, target: None, pos: z, dir: z },
            OpEvent::Telegraph { player: p, pos: z, radius: 1.0, secs: 1.0, element: Element::Light },
            OpEvent::Strike { player: p, pos: z, radius: 1.0, element: Element::Light, cut: true },
            OpEvent::Mended { player: p, pct: 10.0, secs: 1.0 },
            OpEvent::Evade { player: p, dodged: true, attacker: None, pos: z, secs: 1.0 },
            OpEvent::PortalShot { player: p, pos: z, normal: z, orange: false },
            OpEvent::Empowered { player: p, secs: 1.0, on: true },
            OpEvent::Captured { player: p, construct: ConstructId(1), pos: z, secs: 1.0, on: true },
            OpEvent::Zap { player: p, from: z, to: z, element: Element::Light, target: None },
            OpEvent::Marked { player: p, target: p, secs: 1.0, burst: false, point: z },
            OpEvent::Tagged { player: p, target: p, secs: 1.0, strike: false },
        ];
        for fx in [
            ToolboxFx::DrillStart,
            ToolboxFx::DrillLatch,
            ToolboxFx::DrillRelease,
            ToolboxFx::DrillEnd,
            ToolboxFx::Mark,
            ToolboxFx::Recall,
            ToolboxFx::Warp,
            ToolboxFx::Copy,
            ToolboxFx::Gaze,
            ToolboxFx::Flash,
            ToolboxFx::PhaseOn,
            ToolboxFx::PhaseOff,
            ToolboxFx::Banish,
            ToolboxFx::Rush,
            ToolboxFx::Brand,
            ToolboxFx::BrandEnd,
            ToolboxFx::Wave,
        ] {
            v.push(OpEvent::Toolbox { player: p, fx, from: z, to: z, secs: 1.0, size: 1.0 });
        }
        v
    }

    /// Every op event (and every shonen beat) has a visual of its own; only the twisted-move
    /// announcement is presented elsewhere (the title / fusion windup). `visual_of` has no wildcard
    /// arm, so a new `OpEvent` variant can't compile until it is mapped here.
    #[test]
    fn op_visuals_every_op_event_has_a_visual() {
        let evs = all_events();
        assert!(evs.len() >= 25 + 17);
        for e in &evs {
            assert_ne!(visual_of(e), OpVisual::Elsewhere, "{e:?} has no visual");
        }
    }

    #[test]
    fn op_visuals_cut_words_never_boom() {
        assert_eq!(strike_word(StrikeKind::Slash, Some("explosion")), Some("shing"));
        assert_ne!(strike_word(StrikeKind::Bolt, Some("explosion")), Some("explosion"));
        assert_eq!(strike_word(StrikeKind::Blast, Some("bamf")), Some("bamf"), "a blast says its style's word");
    }

    #[test]
    fn op_visuals_delayed_word_hook() {
        let mut r = RecentMoves::default();
        r.note(PlayerId(3), 10.0, Some("shing"), true);
        r.note(PlayerId(4), 10.0, Some("bamf"), false);
        assert_eq!(r.delayed_word(PlayerId(3), 12.0), Some(Some("shing")));
        assert_eq!(r.delayed_word(PlayerId(3), 30.0), None, "too old");
        assert_eq!(r.delayed_word(PlayerId(4), 11.0), None, "not a delayed-blast move");
        assert_eq!(r.delayed_word(PlayerId(9), 11.0), None);
    }

    #[test]
    fn op_visuals_pool_recycles_without_growing() {
        let mut p = Pool::default();
        for k in 0..(MAX_FX * 3) {
            p.add(Fx { dur: 1.0 + k as f32, ..default() });
        }
        assert_eq!(p.fx.len(), MAX_FX);
        assert_eq!(p.fx.capacity(), MAX_FX, "no reallocation past the cap");
    }

    #[test]
    fn op_visuals_lock_channel_slots() {
        let mut c = Chan::default();
        for k in 0..8 {
            assert_eq!(c.slot(PlayerId(k)), Some(k as usize));
        }
        assert_eq!(c.slot(PlayerId(3)), Some(3));
        assert_eq!(c.slot(PlayerId(99)), None, "full: no panic");
    }
}
