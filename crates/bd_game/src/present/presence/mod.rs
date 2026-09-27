//! PRESENCE GRAMMAR — the Hollow Purple treatment, generalised: every construct passes through the
//! same six beats, derived automatically from data it already has (recipe scale / cost / element /
//! mechanics, the legendary roll, spectacle + signature status). No per-construct authoring.
//!
//! | beat | what (scaled by [`tier::Tier`], styled by element) |
//! |---|---|
//! | GATHER | element motes drawn into the hands (held / worn) or the spawn point, ramping k² over the forming windup (it never adds latency: it overlaps the assembly); Big+ a gathering swell sound |
//! | MANIFEST | an element-coloured emissive surge on the construct, a shockwave ring, an element burst, a flash light; caster: FOV punch + a little shake; bystanders feel Big+ within `feel_radius`; the sound goes "shing" → hard-light hit → sub drop by tier |
//! | MATERIAL | `bd_fx::HardLightLook`: an element surface over the hard light (fire heat / ice frost rim / crawling arcs / caustics / swirl bands / molten cracks / streaming lines / bloom / void rim), faded by distance |
//! | PRESENCE | idle element motes, a pooled element light (Big+), a pooled element hum (pitch by size), motion signatures by mechanic (Heavy landings crack the ground, Fast streaks, Flies downdraft, Spin distortion, Explodes pulses unstably), a travel rumble for Titans |
//! | IMPACT | [`PresenceIndex::impact_mult`] for the hit juice |
//! | EXIT | the construct dissolves into its element's motes (on top of bd_fx's dissolve), the hum fades |
//!
//! CONTRAST is the point: a gun is a flick of motes; a sword gets a crisp gather + "shing"; a kaiju
//! gets near-Hollow-Purple. Hollow Purple itself (erasing orbs, `present::signature_fx`) keeps its
//! bespoke look and is skipped here. Budget: capped motes / frame, ≤ `max_lights` lights,
//! ≤ `max_hums` hum voices (tier × distance priority), material LOD by distance. Tunables:
//! `assets/tuning/presence.ron` (hot-reloaded). Dev: `BD_PRESENCE_OFF=1` disables everything.

pub mod dev;
pub mod gather;
pub mod tier;
pub mod tuning;

use super::constructs::{ConstructEntities, hand_transform};
use super::signature::element_rgb;
use super::sound::{Sfx3d, SfxLoopEmitter};
use super::{PresentSet, bv, corps_rgb};
use crate::camera::PlayerCamera;
use crate::juice::{CameraShakeEvent, FovPunch};
use crate::sim_bridge::{ActiveWorld, GameEvents, SimRole, WorldViews};
use crate::state::AppState;
use bd_core::ConstructId;
use bd_core::PlayerId;
use bd_core::mechanics::{Element, FormCategory, Mechanic};
use bd_fx::{BurstFx, BurstKind, ConstructVisual, Emit, FxParticles, HardLightLook, PFlags, PKind};
use bd_sim::GameEvent;
use bd_sim::replicate::cflags;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use tier::{Tier, TierInput};
use tuning::{PresenceTuning, PresenceTuningRes, TierFeel};

// ------------------------------------------------------------------------------------ API

/// Presence tier (+ hit-juice multiplier) of every live construct, for other presenters (impact).
#[derive(Resource, Default)]
pub struct PresenceIndex(pub HashMap<ConstructId, (Tier, f32)>);

impl PresenceIndex {
    #[allow(dead_code)] // API for other presenters
    pub fn tier(&self, id: ConstructId) -> Option<Tier> {
        self.0.get(&id).map(|t| t.0)
    }
    /// Multiplier on hit juice from this construct (1 = unknown / Normal).
    pub fn impact_mult(&self, id: Option<ConstructId>) -> f32 {
        id.and_then(|id| self.0.get(&id)).map_or(1.0, |t| t.1)
    }
}

// ------------------------------------------------------------------------------------ state

struct Tracked {
    id: ConstructId,
    owner: PlayerId,
    tier: Tier,
    element: Element,
    color: [f32; 3],
    category: FormCategory,
    /// Recipe scale (m).
    size: f32,
    heavy: bool,
    fast: bool,
    flies: bool,
    spin: bool,
    explodes: bool,
    creature: bool,
    /// Forming windup (s) and age.
    forming: f32,
    t: f32,
    manifested: bool,
    gather_sfx_done: bool,
    /// Emissive surge 0..1 (decays) — the manifest spike.
    surge: f32,
    gather_credit: f32,
    idle_credit: f32,
    motion_credit: f32,
    last_pos: Option<Vec3>,
    vel: Vec3,
    /// Ground height below (Flies downdraft), refreshed every ~0.25 s.
    ground: Option<f32>,
    ground_t: f32,
    /// Ended (gone / shattered): seconds since.
    ended: Option<f32>,
    /// Hum voice child entity + its current gain.
    hum: Option<(Entity, f32)>,
    /// Sampled surroundings for a wide gather.
    surround: gather::Surround,
    /// Checked the legendary flag in the views once.
    legend_checked: bool,
    /// Current centre + extent (m) this frame.
    centre: Vec3,
    extent: f32,
    lod: f32,
    dist: f32,
    live: bool,
}

#[derive(Resource, Default)]
struct PresenceState {
    live: Vec<Tracked>,
    /// (seq, provisional id) of client-predicted casts waiting for the host.
    predicted: Vec<(u32, ConstructId)>,
    /// Hum cue names (`SfxLoopEmitter` wants `&'static str`): interned once per distinct name.
    names: HashMap<String, &'static str>,
    /// Light pool assignment: construct per standing light slot.
    lights: Vec<Option<ConstructId>>,
    /// Flash lights: (age, life, peak) per slot.
    flashes: Vec<(f32, f32, f32)>,
    /// Flash placements from this frame's manifests: (slot, pos, colour, range).
    flash_at: Vec<(usize, Vec3, [f32; 3], f32)>,
    /// Continuous gathers ([`gather::GatherFx`]).
    streams: Vec<Stream>,
}

/// One continuous gather (a hold-to-grow energy object being charged).
struct Stream {
    owner: Option<PlayerId>,
    key: u32,
    req: gather::GatherFx,
    /// Seconds since the last request (the stream stops when its writer does).
    idle: f32,
    credit: f32,
    surround: gather::Surround,
}

#[derive(Component)]
struct PresenceLight(usize);

#[derive(Component)]
struct PresenceFlash(usize);

#[derive(Component)]
struct PresenceHum;

fn disabled() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var_os("BD_PRESENCE_OFF").is_some()) || super::perf_off("presence")
}

pub struct PresencePlugin;

impl Plugin for PresencePlugin {
    fn build(&self, app: &mut App) {
        // Messages used here that other plugins own (idempotent).
        app.add_message::<GameEvents>()
            .add_message::<Sfx3d>()
            .add_message::<BurstFx>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .add_message::<crate::net::PredictedCast>()
            .add_message::<gather::GatherFx>();
        app.insert_resource(PresenceTuningRes(tuning::load_tuning()))
            .init_resource::<PresenceState>()
            .init_resource::<PresenceIndex>()
            .add_systems(Startup, spawn_light_pool)
            .add_systems(Update, tuning::hot_reload)
            .add_systems(
                Update,
                (
                    (dev::legendary, presence_events).chain().in_set(PresentSet::Events),
                    dev::strips,
                    // After the construct sync (entities exist, transforms placed this frame).
                    (dev::hold, presence_beats, hold_streams, presence_pools).chain().after(PresentSet::Sync),
                ),
            )
            .add_systems(OnExit(AppState::InGame), clear);
    }
}

fn clear(mut commands: Commands, mut st: ResMut<PresenceState>, mut idx: ResMut<PresenceIndex>, hums: Query<Entity, With<PresenceHum>>) {
    st.live.clear();
    st.predicted.clear();
    st.streams.clear();
    for l in st.lights.iter_mut() {
        *l = None;
    }
    for f in st.flashes.iter_mut() {
        f.0 = f.1;
    }
    idx.0.clear();
    for e in &hums {
        commands.entity(e).try_despawn();
    }
}

/// The pooled, shadowless presence + flash lights (spawned once; intensity 0 when unused).
fn spawn_light_pool(mut commands: Commands, tuning: Res<PresenceTuningRes>, mut st: ResMut<PresenceState>) {
    let b = &tuning.0.budget;
    // Allocate the pool at its max (hot-reload can lower the cap, not raise it past this).
    let (nl, nf) = (b.max_lights.clamp(0, 8), b.max_flash_lights.clamp(0, 4));
    let light = || PointLight { intensity: 0.0, range: 10.0, radius: 0.3, shadow_maps_enabled: false, ..default() };
    for i in 0..nl {
        commands.spawn((Name::new("PresenceLight"), PresenceLight(i), light(), Transform::default(), Visibility::Hidden));
    }
    for i in 0..nf {
        commands.spawn((Name::new("PresenceFlash"), PresenceFlash(i), light(), Transform::default(), Visibility::Hidden));
    }
    st.lights = vec![None; nl];
    st.flashes = vec![(1.0, 1.0, 0.0); nf];
}

/// Element mote colour for a construct (Light → the caster's corps; legendary → gold).
fn mote_color(t: &PresenceTuning, e: Element, corps: [f32; 3], legendary: bool) -> [f32; 3] {
    if legendary && e == Element::Light {
        return crate::lobby::feed::GOLD_LINEAR;
    }
    if let Some(c) = t.element(e).color {
        return c;
    }
    match e {
        // Shadow's grade colour is near black: motes need a readable violet.
        Element::Shadow => [0.35, 0.1, 0.7],
        Element::Earth => [0.9, 0.5, 0.2],
        _ => element_rgb(e, corps),
    }
}

// ------------------------------------------------------------------------------------ events

#[allow(clippy::too_many_arguments)]
fn presence_events(
    mut events: MessageReader<GameEvents>,
    mut predicted: MessageReader<crate::net::PredictedCast>,
    active: Option<Res<ActiveWorld>>,
    tuning: Res<PresenceTuningRes>,
    mut st: ResMut<PresenceState>,
    mut idx: ResMut<PresenceIndex>,
    mut fx: ResMut<FxParticles>,
) {
    let Some(active) = active else {
        events.clear();
        predicted.clear();
        return;
    };
    let t = &tuning.0;
    if disabled() || !t.enabled {
        events.clear();
        predicted.clear();
        return;
    }
    let sig = &active.world.tuning().signature;
    let st = &mut *st;
    // Client prediction: the caster's gather starts on the key press, not a round trip later.
    for m in predicted.read() {
        use crate::net::PredictedCast as P;
        match m {
            P::Started { seq, recipe, .. } => {
                let Some(me) = active.local else { continue };
                let pid = ConstructId(u32::MAX - seq);
                let cost = bd_sim::willpower::base_cost(recipe);
                if st.live.len() < 256 && !bd_sim::signature::orb::is_erasing_orb(recipe) {
                    let corps = corps_rgb(active.corps_of(me));
                    st.live.push(track(t, pid, me, recipe, cost, bd_sim::constructs::forming_secs(recipe.form), false, false, corps));
                    st.predicted.push((*seq, pid));
                }
            }
            P::Confirmed { seq, id, .. } => {
                if let Some(i) = st.predicted.iter().position(|p| p.0 == *seq) {
                    let (_, pid) = st.predicted.swap_remove(i);
                    let dup = st.live.iter().any(|l| l.id == *id);
                    if let Some(j) = st.live.iter().position(|l| l.id == pid) {
                        if dup {
                            st.live.swap_remove(j);
                        } else {
                            st.live[j].id = *id;
                            let tr = st.live[j].tier;
                            idx.0.insert(*id, (tr, t.feel(tr).impact_mult));
                        }
                    }
                }
            }
            P::Rejected { seq, .. } => {
                if let Some(i) = st.predicted.iter().position(|p| p.0 == *seq) {
                    let (_, pid) = st.predicted.swap_remove(i);
                    st.live.retain(|l| l.id != pid);
                }
            }
        }
    }
    for ev in events.read() {
        match &ev.event {
            GameEvent::ConstructSpawned { id, owner, recipe, forming, cost, .. } => {
                if st.live.iter().any(|l| l.id == *id) || st.live.len() >= 256 {
                    continue;
                }
                // Hollow Purple keeps its bespoke look (present::signature_fx).
                if bd_sim::signature::orb::is_erasing_orb(recipe) {
                    continue;
                }
                // The client caster already tracks it from the prediction (adopted on Confirmed).
                if active.role == SimRole::Client && active.local == Some(*owner) && !st.predicted.is_empty() {
                    continue;
                }
                let c = active.world.constructs.get(*id);
                let legendary = c.is_some_and(|c| c.flair & cflags::LEGENDARY != 0);
                let spec = bd_sim::signature::spectacle(recipe, *cost, *forming, sig);
                let signature = spec.is_some_and(|s| {
                    s.kind == bd_sim::signature::SignatureKind::Domain || bd_sim::signature::is_orbital(recipe) || *cost >= t.tier.signature_cost
                });
                let corps = corps_rgb(active.corps_of(*owner));
                let mut tr = track(t, *id, *owner, recipe, *cost, *forming, legendary, spec.is_some() && !signature, corps);
                if signature {
                    tr.tier = Tier::Signature;
                }
                idx.0.insert(*id, (tr.tier, t.feel(tr.tier).impact_mult));
                st.live.push(tr);
            }
            GameEvent::ConstructGone { id, .. } | GameEvent::ConstructShattered { id, .. } => {
                idx.0.remove(id);
                let Some(l) = st.live.iter_mut().find(|l| l.id == *id && l.ended.is_none()) else { continue };
                l.ended = Some(0.0);
                // EXIT: it dissolves into its element (on top of bd_fx's voxel dissolve / shatter).
                let n = (t.feel(l.tier).exit_motes as f32 * t.element(l.element).motes) as u32;
                // (Not in the caster's face: first-person held items dissolve in the viewmodel.)
                if l.last_pos.is_some() && l.dist > 1.5 {
                    exit_motes(&mut fx, l, n.min(160));
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn track(
    t: &PresenceTuning,
    id: ConstructId,
    owner: PlayerId,
    recipe: &bd_core::mechanics::Recipe,
    cost: f32,
    forming: f32,
    legendary: bool,
    spectacle: bool,
    corps: [f32; 3],
) -> Tracked {
    let tier = tier::tier(&TierInput::of(recipe, cost, legendary, spectacle, false), &t.tier);
    let cat = recipe.form.category();
    Tracked {
        id,
        owner,
        tier,
        element: recipe.element,
        color: mote_color(t, recipe.element, corps, legendary),
        category: cat,
        size: if recipe.scale.is_finite() { recipe.scale.clamp(0.2, 60.0) } else { 1.0 },
        heavy: recipe.has(Mechanic::Heavy),
        fast: recipe.has(Mechanic::Fast),
        flies: recipe.has(Mechanic::Flies),
        spin: recipe.has(Mechanic::Spin),
        explodes: recipe.has(Mechanic::Explodes),
        creature: cat == FormCategory::Creature,
        forming: if forming.is_finite() { forming.clamp(0.0, 8.0) } else { 0.15 },
        t: 0.0,
        manifested: false,
        gather_sfx_done: false,
        surge: 0.0,
        gather_credit: 0.0,
        idle_credit: 0.0,
        motion_credit: 0.0,
        last_pos: None,
        vel: Vec3::ZERO,
        ground: None,
        ground_t: 0.0,
        ended: None,
        hum: None,
        surround: gather::Surround::default(),
        legend_checked: legendary,
        centre: Vec3::ZERO,
        extent: 0.5,
        lod: 1.0,
        dist: 0.0,
        live: false,
    }
}

// ------------------------------------------------------------------------------------ motes

/// One element mote flying from `pos` with `vel` (gather streaks, exits, idle).
fn mote(fx: &mut FxParticles, e: Element, pos: Vec3, vel: Vec3, color: [f32; 3], size: f32, life: f32) -> bool {
    let (flags, emissive, size, color) = match e {
        Element::Fire => {
            let hot = fx.rng.f() < 0.3;
            (PFlags::STRETCH, 4.5, size, if hot { [1.0, 0.75, 0.3] } else { color })
        }
        Element::Ice => (PFlags::PLAIN, 3.0, size * 0.8, [0.6 + color[0] * 0.4, 0.8 + color[1] * 0.2, 1.0]),
        Element::Electric => (PFlags::STRETCH | PFlags::WHITE_BIRTH, 6.0, size * 0.7, color),
        Element::Water => (PFlags::PLAIN, 2.6, size * 1.2, color),
        Element::Wind => (PFlags::STRETCH, 2.4, size * 0.7, color),
        Element::Earth => (PFlags::PLAIN, 1.8, size * 1.1, color),
        Element::Shadow => (PFlags::PLAIN, 0.9, size * 1.2, color),
        Element::Poison => (PFlags::PLAIN | PFlags::GROW, 3.0, size, color),
        _ => (PFlags::PLAIN, 3.5, size, color),
    };
    fx.emit(Emit { kind: PKind::Ballistic, flags, pos, vel, size, color, emissive, life, gravity: 0.0, spin: 3.0, ..default() })
}

/// An idle mote around a live construct, in its element's style.
fn idle_mote(fx: &mut FxParticles, e: Element, centre: Vec3, extent: f32, color: [f32; 3], size: f32) -> bool {
    let off = fx.rng.vec() * extent;
    let p = centre + off;
    let seed = fx.rng.next_u32();
    match e {
        Element::Fire | Element::Light | Element::Atomic | Element::Poison | Element::Shadow => {
            let (rise, wob, flags, emissive) = match e {
                Element::Fire => (1.8, 0.25, PFlags::PLAIN, 4.5),
                Element::Poison => (0.6, 0.15, PFlags::PLAIN | PFlags::POP, 3.0),
                Element::Shadow => (0.5, 0.35, PFlags::PLAIN, 0.8),
                Element::Atomic => (1.0, 0.2, PFlags::PLAIN | PFlags::WHITE_BIRTH, 4.0),
                _ => (0.7, 0.2, PFlags::PLAIN, 3.0),
            };
            fx.emit(Emit { kind: PKind::Rise, flags, pos: p, vel: Vec3::ZERO, size, color, emissive, life: fx_life(e), gravity: rise, spin: wob, seed, ..default() })
        }
        Element::Psychic | Element::Wind => fx.emit(Emit {
            kind: PKind::Orbit,
            flags: PFlags::STRETCH,
            pos: centre,
            vel: Vec3::new(extent * 1.1, if e == Element::Wind { 9.0 } else { 3.0 }, 0.0),
            size: size * 0.8,
            color,
            emissive: 3.0,
            life: fx_life(e),
            gravity: 0.0,
            seed,
            ..default()
        }),
        Element::Electric => {
            let d = fx.rng.dir();
            fx.emit(Emit {
                kind: PKind::Ballistic,
                flags: PFlags::STRETCH | PFlags::WHITE_BIRTH,
                pos: p,
                vel: d * 7.0,
                size: size * 0.6,
                color,
                emissive: 7.0,
                life: 0.12,
                gravity: 0.0,
                drag: 6.0,
                seed,
                ..default()
            })
        }
        // Ice frost, water drips, earth grit: they fall away from it.
        _ => {
            let (g, drag, emissive) = match e {
                Element::Ice => (0.25, 2.0, 3.0),
                Element::Water => (1.0, 0.3, 2.4),
                _ => (1.0, 0.5, 1.6),
            };
            fx.emit(Emit {
                kind: PKind::Ballistic,
                flags: PFlags::PLAIN | PFlags::FLOOR,
                pos: p,
                vel: Vec3::new(0.0, -0.3, 0.0),
                size,
                color,
                emissive,
                life: fx_life(e),
                gravity: g,
                drag,
                seed,
                ..default()
            })
        }
    }
}

fn fx_life(e: Element) -> f32 {
    match e {
        Element::Fire => 0.9,
        Element::Ice => 1.4,
        Element::Wind | Element::Psychic => 0.7,
        Element::Shadow => 1.3,
        _ => 1.0,
    }
}

/// EXIT: a construct breaks down into its element — motes outward from its body, drifting away.
fn exit_motes(fx: &mut FxParticles, l: &Tracked, n: u32) {
    let size = (0.06 + 0.02 * l.size.sqrt()).min(0.25);
    for _ in 0..n {
        let d = fx.rng.dir();
        let p = l.centre + d * l.extent * fx.rng.range(0.2, 1.0);
        let v = d * fx.rng.range(1.0, 3.0) * l.size.sqrt().min(3.0) + Vec3::Y * 0.6;
        let life = fx.rng.range(0.5, 1.1);
        mote(fx, l.element, p, v, l.color, size, life);
    }
}

/// A radial shockwave ring in the plane across `axis`.
fn shock_ring(fx: &mut FxParticles, c: Vec3, axis: Vec3, speed: f32, n: u32, color: [f32; 3], size: f32) {
    let (s1, s2) = axis.normalize_or(Vec3::Y).any_orthonormal_pair();
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let out = s1 * a.cos() + s2 * a.sin();
        fx.emit(Emit {
            kind: PKind::Ballistic,
            flags: PFlags::STRETCH | PFlags::PLAIN,
            pos: c + out * 0.3,
            vel: out * speed,
            size,
            color,
            emissive: 5.0,
            life: 0.35,
            gravity: 0.0,
            drag: 3.5,
            spin: 2.5,
            ..default()
        });
    }
}

/// Ground crack: a dust ring + grit (Heavy landings, downdraft).
fn ground_ring(fx: &mut FxParticles, c: Vec3, radius: f32, n: u32, color: [f32; 3], speed: f32) {
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU + fx.rng.f() * 0.3;
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        let seed = fx.rng.next_u32();
        let (sz, up) = (0.35 + 0.2 * fx.rng.f(), fx.rng.range(2.0, 4.0));
        fx.emit(Emit {
            kind: PKind::Ballistic,
            flags: PFlags::SMOKE,
            pos: c + out * radius * 0.3 + Vec3::Y * 0.1,
            vel: out * speed + Vec3::Y * 0.6,
            size: sz,
            color: [0.55, 0.5, 0.45],
            emissive: 0.0,
            life: 0.9,
            gravity: 0.05,
            drag: 2.5,
            seed,
            ..default()
        });
        if i % 3 == 0 {
            fx.emit(Emit {
                kind: PKind::Ballistic,
                flags: PFlags::PLAIN | PFlags::FLOOR,
                pos: c + out * radius * 0.2 + Vec3::Y * 0.1,
                vel: out * speed * 0.6 + Vec3::Y * up,
                size: 0.08,
                color,
                emissive: 2.0,
                life: 0.8,
                gravity: 1.0,
                seed,
                ..default()
            });
        }
    }
}

// ------------------------------------------------------------------------------------ beats

#[allow(clippy::too_many_arguments)]
fn presence_beats(
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    tuning: Res<PresenceTuningRes>,
    comfort: Res<crate::settings::ComfortSettings>,
    map: Res<ConstructEntities>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    tfs: Query<&Transform, With<ConstructVisual>>,
    mut visuals: Query<&mut ConstructVisual>,
    mut st: ResMut<PresenceState>,
    mut fx: ResMut<FxParticles>,
    mut sfx: MessageWriter<Sfx3d>,
    mut burst: MessageWriter<BurstFx>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut fov: MessageWriter<FovPunch>,
) {
    let Some(active) = active else { return };
    let st = &mut *st;
    if st.live.is_empty() {
        return;
    }
    let t = &tuning.0;
    let off = disabled() || !t.enabled;
    let dt = time.delta_secs().clamp(1e-4, 0.1);
    let eye = cam.single().map_or(Vec3::ZERO, |c| c.translation());
    let (cam_fwd, cam_right) = cam.single().map_or((Vec3::NEG_Z, Vec3::X), |c| (c.forward().as_vec3(), c.right().as_vec3()));
    let flashes_ok = comfort.flashes > 0.0;
    let b = &t.budget;
    let mut motes_left = if off { 0 } else { b.max_motes_per_frame };
    let mut rays = b.max_rays_per_frame;
    let vox_guard = arena.as_ref().map(|a| a.read());
    let vox = vox_guard.as_deref();
    // Highest tier first: when the mote budget runs out, the small stuff goes quiet.
    st.live.sort_by(|a, b| b.tier.cmp(&a.tier));
    let mut new_flashes: Vec<(Vec3, [f32; 3], f32, f32)> = Vec::new();
    for l in st.live.iter_mut() {
        l.t += dt;
        if let Some(e) = l.ended.as_mut() {
            *e += dt;
            continue;
        }
        let view = views.construct(l.id);
        // Legendary is sometimes only visible in the replicated flags: lift once.
        if !l.legend_checked
            && let Some(v) = view
        {
            l.legend_checked = true;
            if v.flags & cflags::LEGENDARY != 0 && l.tier < Tier::Titan {
                l.tier = Tier::Titan;
                if l.element == Element::Light {
                    l.color = crate::lobby::feed::GOLD_LINEAR;
                }
            }
        }
        let feel: &TierFeel = t.feel(l.tier);
        let el = t.element(l.element);
        let mine = active.local == Some(l.owner);
        // Where it is: the construct entity (held items ride the hand), else the view.
        let entity = map.0.get(&l.id).copied();
        let tf = entity.and_then(|e| tfs.get(e).ok());
        let wc = active.world.constructs.get(l.id);
        let (centre, extent) = match (wc, tf) {
            (Some(c), Some(tf)) if !(view.is_some_and(|v| v.attached())) => {
                let half = bv(c.half);
                (tf.translation + tf.rotation * bv(c.local_center), half.length().max(0.3))
            }
            (_, Some(tf)) => (tf.translation, (l.size * 0.5).max(0.3)),
            (Some(c), None) => (bv(c.center()), bv(c.half).length().max(0.3)),
            _ => match view {
                Some(v) if !v.attached() => (v.pos, (l.size * 0.5).max(0.3)),
                // Gone without an event (a resync): end quietly. A client prediction has no
                // entity / view yet: it rides the caster's hand.
                None if l.id.0 < u32::MAX - 1_000_000 => {
                    l.ended = Some(0.0);
                    continue;
                }
                _ => match views.player(l.owner) {
                    Some(p) => (hand_transform(p).translation, 0.4),
                    None => {
                        l.live = false;
                        continue;
                    }
                },
            },
        };
        l.live = true;
        if let Some(p) = l.last_pos {
            let v = (centre - p) / dt;
            if v.is_finite() && v.length() < 300.0 {
                l.vel = l.vel.lerp(v, 0.35);
            }
        }
        let prev_vy = l.vel.y;
        l.last_pos = Some(centre);
        l.centre = centre;
        l.extent = extent;
        l.dist = eye.distance(centre);
        let k = l.size.max(1.0).sqrt();
        l.lod = 1.0 - smoothstep(b.lod_full * k, b.lod_zero * k, l.dist);
        let held = matches!(l.category, FormCategory::Held | FormCategory::Worn);
        // The gather point: the hands for what you hold, the body for what stands on its own.
        let mut gather_at = if held { views.player(l.owner).map_or(centre, |p| hand_transform(p).translation) } else { centre };
        let mut msize = (0.05 + 0.025 * l.size.sqrt()).min(0.22);
        // First person: the hand sits on the camera — gather in front of the view instead, with
        // small motes (nothing may cover the screen).
        if held && eye.distance(gather_at) < 1.5 {
            gather_at = eye + cam_fwd * 1.4 + cam_right * 0.35 - Vec3::Y * 0.3;
            msize *= 0.45;
        }
        l.surge = (l.surge - dt / feel.surge_secs.max(0.02)).max(0.0);

        // ---- GATHER (through the forming windup; never delays the cast) ----
        if !l.manifested && !off {
            let window = feel.gather_secs.min(l.forming).max(0.05);
            let start = (l.forming - window).max(0.0);
            if l.t >= start {
                let kk = ((l.t - start) / window).clamp(0.0, 1.0);
                l.gather_credit += feel.gather_rate * el.motes * (0.25 + 0.75 * kk * kk) * dt;
                let n = (l.gather_credit as u32).min(motes_left).min(64);
                l.gather_credit -= l.gather_credit.floor();
                let near = held && eye.distance(gather_at) < 2.0;
                // First person: a small gather in front of the view (never through the camera).
                let radius = if near { 1.2 } else { feel.gather_radius * l.size.max(0.5).sqrt().min(4.0) };
                let life = if near { (window * 0.6).clamp(0.08, 0.4) } else { feel.gather_life.min(window.max(0.1)).max(0.08) };
                if radius > 4.0 && !near {
                    gather::refresh(&mut l.surround, &mut fx, gather_at, radius, &mut rays, vox, b.surround_samples);
                }
                let face = near.then_some(cam_fwd);
                gather::emit(&mut fx, Some(&l.surround), gather_at, radius, n, l.element, l.color, msize, life, face, eye);
                motes_left -= n;
            }
            if !l.gather_sfx_done && !feel.gather_sfx.is_empty() {
                // Its swell must END on the manifest: start it `len` before, or skip it if the
                // windup is too short to carry it (the manifest hit covers).
                let len = if l.tier >= Tier::Titan { 1.54 } else { 0.87 };
                if l.forming >= len * 0.6 && l.t >= (l.forming - len).max(0.0) {
                    l.gather_sfx_done = true;
                    let pitch = (len / l.forming.max(0.01)).clamp(1.0, 1.26);
                    sfx.write(Sfx3d::at(feel.gather_sfx.clone(), gather_at).gain(feel.gather_gain).pitch(pitch));
                } else if l.forming < len * 0.6 {
                    l.gather_sfx_done = true;
                }
            }
        }

        // ---- MANIFEST ----
        let formed = view.map_or(l.t >= l.forming, |v| v.flags & cflags::FORMING == 0 && l.t >= l.forming.min(0.05));
        if !l.manifested && (formed || l.t >= l.forming + 0.3) {
            l.manifested = true;
            if !off {
                l.surge = feel.surge * if flashes_ok { 1.0 } else { 0.35 };
                let ring_at = if held { gather_at } else { centre };
                if feel.ring > 0 && motes_left > 0 {
                    let axis = if held || l.flies { views.player(l.owner).map_or(Vec3::Y, |p| Quat::from_rotation_y(p.yaw) * Vec3::NEG_Z) } else { Vec3::Y };
                    let n = feel.ring.min(motes_left);
                    let near = eye.distance(ring_at) < 2.0;
                    let speed = feel.ring_speed * l.size.max(0.5).sqrt() * if near { 0.35 } else { 1.0 };
                    shock_ring(&mut fx, ring_at, axis, speed, n, l.color, msize * 1.4);
                    motes_left -= n;
                }
                if feel.burst > 0.0 && eye.distance(ring_at) > 3.0 {
                    burst.write(BurstFx { kind: BurstKind::Element(l.element), pos: ring_at, dir: Vec3::Y, scale: feel.burst * k, color: None });
                }
                if !feel.manifest_sfx.is_empty() {
                    // The local caster of a small cast already hears the ring's own form sound.
                    sfx.write(Sfx3d::at(feel.manifest_sfx.clone(), ring_at).gain(feel.manifest_gain));
                }
                if feel.flash_light > 0.0 && flashes_ok && l.lod > 0.0 {
                    new_flashes.push((ring_at + Vec3::Y * 0.5, l.color, feel.flash_light * el.light, feel.flash_range * k.min(3.0)));
                }
                if mine {
                    if feel.fov_punch > 0.0 {
                        fov.write(FovPunch { degrees: feel.fov_punch });
                    }
                    if feel.caster_shake > 0.0 {
                        shake.write(CameraShakeEvent { trauma: feel.caster_shake, dir: Vec3::ZERO });
                    }
                } else if feel.feel_radius > 0.0 && l.dist < feel.feel_radius {
                    // Bystanders FEEL a big one arrive (juice applies the comfort shake setting).
                    let f = 1.0 - l.dist / feel.feel_radius;
                    shake.write(CameraShakeEvent { trauma: feel.feel_trauma * f, dir: (centre - eye).normalize_or_zero() });
                }
            }
        }

        // ---- MATERIAL ----
        if let Some(e) = entity
            && let Ok(mut vis) = visuals.get_mut(e)
        {
            let form_k = if l.manifested { 1.0 } else { (l.t / l.forming.max(0.05)).clamp(0.0, 1.0) };
            let mut surge = l.surge;
            if l.explodes && l.manifested && !off {
                // Unstable: a pulse that quickens toward the end of its life.
                let life = wc.map_or(3.0, |c| if c.life.is_finite() && c.life > 0.1 { c.life } else { 3.0 });
                let age = wc.map_or(l.t, |c| c.age);
                let u = (age / life).clamp(0.0, 1.0);
                let hz = t.motion.unstable_hz[0] + (t.motion.unstable_hz[1] - t.motion.unstable_hz[0]) * u * u;
                let p = (0.5 + 0.5 * (l.t * hz * std::f32::consts::TAU).sin()).powi(4);
                surge = surge.max(p * t.motion.unstable_surge * (0.4 + 0.6 * u));
            }
            let want = if off {
                HardLightLook::default()
            } else {
                HardLightLook {
                    element: HardLightLook::code(l.element),
                    amount: feel.material * el.material * l.lod * form_k,
                    surge: surge * l.lod.max(0.3),
                }
            };
            if vis.look != want {
                vis.look = want;
            }
        }
        if off || !l.manifested {
            continue;
        }

        // ---- PRESENCE: idle motes ----
        if feel.idle_rate > 0.0 && l.lod > 0.0 && motes_left > 0 {
            l.idle_credit += feel.idle_rate * el.motes * l.lod * k.min(3.0) * dt;
            let n = (l.idle_credit as u32).min(motes_left).min(16);
            l.idle_credit -= l.idle_credit.floor();
            for _ in 0..n {
                idle_mote(&mut fx, l.element, centre, extent * 0.8, l.color, msize);
            }
            motes_left -= n;
        }

        // ---- PRESENCE: motion signatures by mechanic ----
        let speed = l.vel.length();
        let tierf = feel.priority.max(1.0).sqrt();
        if l.heavy && !l.creature && !held && prev_vy < -t.motion.heavy_land_speed && l.vel.y > -1.0 {
            // A heavy landing cracks the ground (creatures already get footfalls in construct_anim).
            let foot = centre - Vec3::Y * extent * 0.6;
            let n = ((10.0 * tierf) as u32).min(motes_left);
            ground_ring(&mut fx, foot, extent, n, l.color, 4.0 + 2.0 * tierf);
            motes_left -= n;
            sfx.write(Sfx3d::at(if l.tier >= Tier::Titan { "kaiju_step" } else { "land_heavy" }, foot).gain(0.5 + 0.2 * tierf));
            let r = feel.feel_radius.max(12.0);
            if l.dist < r {
                shake.write(CameraShakeEvent { trauma: (0.08 + feel.feel_trauma) * (1.0 - l.dist / r), dir: Vec3::NEG_Y });
            }
        }
        if motes_left > 0 && l.lod > 0.0 {
            let mut rate = 0.0;
            if l.fast && speed > t.motion.fast_speed {
                rate += t.motion.fast_rate;
            }
            if l.spin {
                rate += t.motion.spin_rate;
            }
            let near_ground = l.flies && {
                l.ground_t -= dt;
                if l.ground_t <= 0.0 {
                    l.ground_t = 0.25;
                    l.ground = vox.and_then(|v| bd_core::voxel::raycast(v, super::gv(centre), bd_core::Vec3::NEG_Y, t.motion.downdraft_height * k + extent).map(|h| h.point.y));
                }
                l.ground.is_some()
            };
            if near_ground {
                rate += t.motion.downdraft_rate;
            }
            l.motion_credit += rate * l.lod * dt;
            let n = (l.motion_credit as u32).min(motes_left).min(12);
            l.motion_credit -= l.motion_credit.floor();
            for i in 0..n {
                let pick = i as f32 / n.max(1) as f32;
                if l.fast && speed > t.motion.fast_speed && pick < 0.5 {
                    // Speed streaks peeling off behind it.
                    let p = centre + fx.rng.vec() * extent * 0.7;
                    fx.emit(Emit {
                        kind: PKind::Ballistic,
                        flags: PFlags::STRETCH | PFlags::PLAIN,
                        pos: p,
                        vel: -l.vel * 0.15,
                        size: msize,
                        color: l.color,
                        emissive: 3.0,
                        life: 0.22,
                        gravity: 0.0,
                        ..default()
                    });
                } else if near_ground && let Some(gy) = l.ground {
                    // Downdraft: dust blown out along the ground under it.
                    let a = fx.rng.f() * std::f32::consts::TAU;
                    let out = Vec3::new(a.cos(), 0.0, a.sin());
                    let seed = fx.rng.next_u32();
                    fx.emit(Emit {
                        kind: PKind::Ballistic,
                        flags: PFlags::SMOKE,
                        pos: Vec3::new(centre.x, gy + 0.15, centre.z) + out * extent * 0.4,
                        vel: out * (4.0 + 2.0 * k),
                        size: 0.3,
                        color: [0.6, 0.58, 0.55],
                        emissive: 0.0,
                        life: 0.7,
                        gravity: 0.0,
                        drag: 2.0,
                        seed,
                        ..default()
                    });
                } else {
                    // Spin: the air around it bent into orbiting streaks.
                    let seed = fx.rng.next_u32();
                    fx.emit(Emit {
                        kind: PKind::Orbit,
                        flags: PFlags::STRETCH | PFlags::PLAIN,
                        pos: centre,
                        vel: Vec3::new(extent * 1.2, 16.0, 0.0),
                        size: msize * 0.8,
                        color: [0.8 + l.color[0] * 0.2, 0.8 + l.color[1] * 0.2, 0.8 + l.color[2] * 0.2],
                        emissive: 1.6,
                        life: 0.3,
                        gravity: 0.0,
                        seed,
                        ..default()
                    });
                }
            }
            motes_left -= n;
        }
        // ---- PRESENCE: others feel a titan move ----
        if feel.travel_rumble > 0.0 && !mine && !l.creature && speed > 2.0 && l.dist < feel.feel_radius {
            let f = 1.0 - l.dist / feel.feel_radius;
            shake.write(CameraShakeEvent { trauma: feel.travel_rumble * f * dt * (speed / 10.0).min(1.5), dir: Vec3::ZERO });
        }
    }
    // Flash lights: grab the oldest slot.
    for (p, c, peak, range) in new_flashes {
        if let Some((i, _)) = st.flashes.iter().enumerate().max_by(|a, b| (a.1.0 / a.1.1.max(1e-3)).total_cmp(&(b.1.0 / b.1.1.max(1e-3)))) {
            st.flashes[i] = (0.0, 0.3, peak);
            st.flash_at.push((i, p, c, range));
        }
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let k = ((x - a) / (b - a).max(1e-3)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

// ------------------------------------------------------------------------------------ hold streams

/// Continuous gathers ([`gather::GatherFx`]): the stream swells as the held object grows.
#[allow(clippy::too_many_arguments)]
fn hold_streams(
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    tuning: Res<PresenceTuningRes>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut reqs: MessageReader<gather::GatherFx>,
    mut st: ResMut<PresenceState>,
    mut fx: ResMut<FxParticles>,
) {
    let t = &tuning.0;
    if disabled() || !t.enabled {
        reqs.clear();
        st.streams.clear();
        return;
    }
    let dt = time.delta_secs().clamp(1e-4, 0.1);
    let h = &t.hold;
    let st = &mut *st;
    for r in reqs.read() {
        if !(r.at.is_finite() && r.size.is_finite() && r.charge.is_finite()) {
            continue;
        }
        match st.streams.iter_mut().find(|s| s.owner == r.owner && s.key == r.key) {
            Some(s) => {
                s.req = *r;
                s.idle = 0.0;
            }
            None => {
                if st.streams.len() >= h.max_streams.max(1)
                    && let Some(i) = st.streams.iter().enumerate().max_by(|a, b| a.1.idle.total_cmp(&b.1.idle)).map(|(i, _)| i)
                {
                    // Recycle the stalest.
                    st.streams.swap_remove(i);
                }
                st.streams.push(Stream { owner: r.owner, key: r.key, req: *r, idle: 0.0, credit: 0.0, surround: gather::Surround::default() });
            }
        }
    }
    st.streams.retain(|s| s.idle < 0.25);
    if st.streams.is_empty() {
        return;
    }
    let Some(active) = active else { return };
    let (eye, fwd) = cam.single().map_or((Vec3::ZERO, Vec3::NEG_Z), |c| (c.translation(), c.forward().as_vec3()));
    let vox_guard = arena.as_ref().map(|a| a.read());
    let vox = vox_guard.as_deref();
    let mut rays = t.budget.max_rays_per_frame;
    let mut left = t.budget.max_motes_per_frame / 2;
    for s in st.streams.iter_mut() {
        s.idle += dt;
        let r = s.req;
        let charge = r.charge.clamp(0.0, 1.0);
        let size = r.size.clamp(0.05, 80.0);
        let radius = (h.radius_per_m * size).clamp(h.radius_min, h.radius_max) * (0.5 + 0.5 * charge);
        let rate = (h.rate_base + h.rate_per_m * size).min(h.rate_max) * (0.3 + 0.7 * charge);
        s.credit += rate * dt;
        let n = (s.credit as u32).min(left).min(48);
        s.credit -= s.credit.floor();
        left -= n;
        let near = eye.distance(r.at) < 2.0;
        let (radius, face) = if near { (radius.min(1.2), Some(fwd)) } else { (radius, None) };
        if radius > 4.0 {
            gather::refresh(&mut s.surround, &mut fx, r.at, radius, &mut rays, vox, t.budget.surround_samples);
        }
        let corps = r.owner.map_or([1.0; 3], |o| corps_rgb(active.corps_of(o)));
        let color = r.color.unwrap_or_else(|| mote_color(t, r.element, corps, false));
        let msize = (0.05 + 0.03 * size.sqrt()).min(0.3) * if near { 0.45 } else { 1.0 };
        // Far motes take longer to arrive (a readable stream, not a flash).
        let life = (0.25 + radius / 45.0).min(1.6);
        gather::emit(&mut fx, Some(&s.surround), r.at, radius, n, r.element, color, msize, life, face, eye);
    }
}

// ------------------------------------------------------------------------------------ pools

type LightQ<'w, 's> = Query<'w, 's, (&'static PresenceLight, &'static mut PointLight, &'static mut Transform, &'static mut Visibility), Without<PresenceFlash>>;
type FlashQ<'w, 's> = Query<'w, 's, (&'static PresenceFlash, &'static mut PointLight, &'static mut Transform, &'static mut Visibility), Without<PresenceLight>>;

/// Standing lights + flash lights + hum voices, capped and assigned by tier × distance; drops
/// finished trackers.
#[allow(clippy::too_many_arguments)]
fn presence_pools(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<PresenceTuningRes>,
    map: Res<ConstructEntities>,
    mut st: ResMut<PresenceState>,
    mut lights: LightQ,
    mut flashes: FlashQ,
    mut hums: Query<&mut SfxLoopEmitter, With<PresenceHum>>,
) {
    let t = &tuning.0;
    let dt = time.delta_secs().clamp(1e-4, 0.1);
    let st = &mut *st;
    let off = disabled() || !t.enabled;
    // ---- flash lights ----
    for (i, p, c, range) in std::mem::take(&mut st.flash_at) {
        if let Some((_, mut pl, mut tf, _)) = flashes.iter_mut().find(|(f, ..)| f.0 == i) {
            tf.translation = p;
            pl.color = Color::linear_rgb(0.5 + c[0] * 0.5, 0.5 + c[1] * 0.5, 0.5 + c[2] * 0.5);
            pl.range = range;
        }
    }
    for (f, mut pl, _, mut vis) in &mut flashes {
        let Some(s) = st.flashes.get_mut(f.0) else { continue };
        s.0 += dt;
        let k = (1.0 - s.0 / s.1.max(1e-3)).max(0.0);
        let want = if k > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let i = s.2 * k * k;
        if pl.intensity != i {
            pl.intensity = i;
        }
    }
    // ---- standing lights: the top `max_lights` by tier priority / distance ----
    let max_l = t.budget.max_lights.min(st.lights.len());
    let mut cands: Vec<(f32, usize)> = st
        .live
        .iter()
        .enumerate()
        .filter(|(_, l)| !off && l.live && l.manifested && l.ended.is_none() && t.feel(l.tier).light > 0.0 && l.lod > 0.0)
        .map(|(i, l)| (t.feel(l.tier).priority / (1.0 + l.dist / 25.0), i))
        .collect();
    cands.sort_by(|a, b| b.0.total_cmp(&a.0));
    cands.truncate(max_l);
    let chosen: Vec<ConstructId> = cands.iter().map(|&(_, i)| st.live[i].id).collect();
    // Keep existing assignments (no flicker), fill free slots.
    for slot in st.lights.iter_mut() {
        if slot.is_some_and(|id| !chosen.contains(&id)) {
            *slot = None;
        }
    }
    for id in &chosen {
        if !st.lights.contains(&Some(*id))
            && let Some(free) = st.lights.iter_mut().take(max_l).find(|s| s.is_none())
        {
            *free = Some(*id);
        }
    }
    for (pl_slot, mut pl, mut tf, mut vis) in &mut lights {
        let l = st.lights.get(pl_slot.0).copied().flatten().and_then(|id| st.live.iter().find(|l| l.id == id));
        match l {
            Some(l) => {
                let feel = t.feel(l.tier);
                let k = l.size.max(1.0).sqrt().min(3.0);
                let breathe = 1.0 + 0.08 * (l.t * 2.3).sin() + 1.5 * l.surge;
                tf.translation = l.centre;
                pl.color = Color::linear_rgb(l.color[0], l.color[1], l.color[2]);
                pl.intensity = feel.light * t.element(l.element).light * l.lod * breathe * k;
                pl.range = feel.light_range * k;
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
            None => {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                    pl.intensity = 0.0;
                }
            }
        }
    }
    // ---- hums: the top `max_hums` by tier priority / distance ----
    let b = &t.budget;
    let mut hc: Vec<(f32, usize)> = st
        .live
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            let f = t.feel(l.tier);
            !off && l.live && l.manifested && l.ended.is_none() && f.hum_gain > 0.0 && l.dist < b.hum_range * l.size.max(1.0).sqrt()
        })
        .map(|(i, l)| (t.feel(l.tier).priority / (1.0 + l.dist / 10.0), i))
        .collect();
    hc.sort_by(|a, b| b.0.total_cmp(&a.0));
    hc.truncate(b.max_hums);
    let names = &mut st.names;
    for (i, l) in st.live.iter_mut().enumerate() {
        let want = hc.iter().any(|&(_, j)| j == i);
        let feel = t.feel(l.tier);
        let target = if want { feel.hum_gain } else { 0.0 };
        match (l.hum, want) {
            (None, true) => {
                let Some(&parent) = map.0.get(&l.id) else { continue };
                let hum = t.element(l.element).hum;
                let name: &'static str = *names.entry(hum.clone()).or_insert_with(|| Box::leak(hum.into_boxed_str()));
                // Bigger = lower and deeper.
                let pitch = (1.5 / l.size.max(0.3)).powf(0.3).clamp(0.5, 1.5);
                let child = commands
                    .spawn((Name::new("PresenceHum"), PresenceHum, SfxLoopEmitter { name, gain: 0.0, pitch }, Transform::default()))
                    .id();
                commands.entity(parent).add_child(child);
                l.hum = Some((child, 0.0));
            }
            (Some((e, g)), _) => {
                // Ease in over ~0.6 s, out over ~0.3 s; despawn once silent.
                let rate = if target > g { dt / 0.6 } else { dt / 0.3 };
                let ng = if target > g { (g + rate).min(target) } else { (g - rate).max(target) };
                if let Ok(mut em) = hums.get_mut(e) {
                    em.gain = ng;
                }
                if ng <= 0.0 && !want {
                    commands.entity(e).try_despawn();
                    l.hum = None;
                } else {
                    l.hum = Some((e, ng));
                }
            }
            _ => {}
        }
    }
    // Drop finished trackers (after the exit + the hum fade), and ones that never appeared.
    st.live.retain(|l| {
        let done = l.ended.is_some_and(|e| e > 0.6) || (!l.live && l.t > 3.0 && l.last_pos.is_none());
        if done && let Some((e, _)) = l.hum {
            commands.entity(e).try_despawn();
        }
        !done
    });
}
