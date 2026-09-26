//! Charging presentation (owner: "think about in battle anime when they power up — super visible,
//! call attention; darken the world ever so slightly; strong glow/aura that makes them pop without
//! destroying visual clarity"). The sim side is `bd_sim::charge`; this reads only the replicated
//! state (`pflags::CHARGING` + `PlayerSnap::charge`) and `GameEvent::Charge`, so it looks the same
//! in practice, host and client.
//!
//! * **Aura** — a bd_fx [`ChargeAuraFx`] per charger (flames, sparks, lifting pebbles in the
//!   terrain's colour, pressure ring + cracks, shockwave pulse every ~0.6 s → 0.45 s as it builds,
//!   a light pillar at range) + a corps-coloured point light washing the ground.
//! * **Pop** — the charger's hard-light body gets an emissive boost (brighter palette → bloom) and a
//!   faint white sheen.
//! * **World dim** — while any charger is in view, the world's shadows + midtones lose up to ~8 %
//!   (eased over ~0.3 s; multiple chargers don't stack). Done in the camera's `ColorGrading`
//!   section gains (free: the uniform is applied anyway): highlights — the chargers' boosted
//!   bodies, auras, projectiles, constructs' bright edges — are barely touched, and UI isn't graded.
//! * **Burst** — reaching a full pool (or releasing a long charge) fires a radial burst + ground
//!   shockwave + a flash near the charger only; a hit collapses the aura into falling shards.
//! * **Comfort** (`ComfortSettings::flashes` = 0): no flashes and no dim pulsing; the aura stays.
//! * **Audio hooks** — [`ChargeCue`] messages (start / pulse / full / broken / release) and the
//!   [`ChargeLoops`] resource (every active charger's position + level, for a loop whose pitch rises
//!   with the level). Minimal one-shots through the existing `Sfx3d` path meanwhile.
//! * `--scenario charge [--charge-layout near|far|three|four|none]` — screenshot / perf scene
//!   (see [`scenario`]).

use super::PresentSet;
use super::impact::FlashLight;
use super::players::PlayerBody;
use super::sound::Sfx3d;
use crate::camera::PlayerCamera;
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, ViewKeys, WorldViews};
use crate::state::AppState;
use crate::tick::FrameSet;
use bd_core::PlayerId;
use bd_fx::{AuraBurstFx, ChargeAuraFx, ConstructVisual};
use bd_sim::GameEvent;
use bd_sim::charge::{ChargeEvent, WIRE_STEPS_PER_SEC};
use bd_sim::replicate::pflags;
use bevy::light::NotShadowCaster;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;

/// Peak world dim (fraction of shadow / midtone luminance removed).
pub const DIM_MAX: f32 = 0.08;
/// Seconds of hold time for the aura to reach full size.
const LEVEL_SECS: f32 = 3.5;

/// What happened to a charge, for the audio mixer (the audio worker wires these to cues:
/// `charge_start`, `charge_pulse`, `charge_full`, `charge_break`, `charge_release`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChargeCueKind {
    Start,
    /// One shockwave pulse (~every 0.6 s → 0.45 s).
    Pulse,
    Full,
    Broken,
    /// Released after `held` seconds.
    Release,
}

/// Audio hook (read by the bd_audio mixer once it wires charge cues; nothing reads it yet).
#[allow(dead_code)]
#[derive(Message, Clone, Copy, Debug)]
pub struct ChargeCue {
    pub player: PlayerId,
    pub pos: Vec3,
    pub kind: ChargeCueKind,
    /// 0..1 power level at the moment.
    pub level: f32,
    pub held: f32,
    pub local: bool,
}

/// One active charger for a looping sound (`charge_loop`, pitch rising with `level`).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct ChargeLoop {
    pub player: PlayerId,
    pub pos: Vec3,
    pub level: f32,
    /// Fade 0..1 (follow it for the loop's gain).
    pub intensity: f32,
    pub local: bool,
}

/// Every audible charger this frame (rebuilt each frame; no allocation after warm-up).
#[derive(Resource, Default, Debug)]
pub struct ChargeLoops(pub Vec<ChargeLoop>);

/// Current world dim (0..`DIM_MAX`), for overlays / tests.
#[derive(Resource, Default, Debug)]
pub struct ChargeDim {
    pub value: f32,
    /// Section gains currently written to the camera (1 = untouched).
    written: f32,
}

struct AuraSlot {
    entity: Entity,
    intensity: f32,
    held: f32,
    next_pulse: f32,
    was_on: bool,
}

#[derive(Resource, Default)]
struct Auras(HashMap<PlayerId, AuraSlot>);

#[derive(Component)]
struct ChargeAura;

pub struct ChargePlugin;

impl Plugin for ChargePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ChargeCue>()
            .init_resource::<ChargeLoops>()
            .init_resource::<ChargeDim>()
            .init_resource::<Auras>()
            .add_systems(OnExit(AppState::InGame), clear_auras)
            .add_systems(Update, charge_events.in_set(PresentSet::Events))
            .add_systems(Update, (sync_auras, body_glow, world_dim).chain().in_set(FrameSet::Present).after(PresentSet::Sync));
        scenario::build(app);
    }
}

fn clear_auras(mut commands: Commands, mut auras: ResMut<Auras>, mut loops: ResMut<ChargeLoops>) {
    for (_, s) in auras.0.drain() {
        commands.entity(s.entity).try_despawn();
    }
    loops.0.clear();
}

/// Aura level 0..1 from hold time (eased: most of the growth in the first seconds).
pub fn level_of(held: f32) -> f32 {
    let x = (held / LEVEL_SECS).clamp(0.0, 1.0);
    1.0 - (1.0 - x) * (1.0 - x)
}

/// Shockwave period for a level (quickens as it builds).
fn pulse_period(level: f32) -> f32 {
    0.6 - 0.15 * level
}

fn comfort_calm(c: &ComfortSettings) -> bool {
    c.flashes <= 0.01
}

/// Terrain colour under a point (linear RGB), for the lifting pebbles.
fn ground_color(arena: Option<&crate::arena::ArenaWorld>, feet: Vec3) -> [f32; 3] {
    let fallback = [0.36, 0.3, 0.24];
    let Some(a) = arena else { return fallback };
    let w = a.read();
    let p = bd_core::Vec3::new(feet.x, feet.y - 0.25, feet.z);
    let m = w.get_at(p);
    if m == bd_voxel::mat::AIR {
        return fallback;
    }
    let c = bd_voxel::material_def(m).color;
    let lin = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    [lin(c[0]), lin(c[1]), lin(c[2])]
}

#[allow(clippy::too_many_arguments)]
fn charge_events(
    mut commands: Commands,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    comfort: Res<ComfortSettings>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    mut burst: MessageWriter<AuraBurstFx>,
    mut sfx: MessageWriter<Sfx3d>,
    mut cues: MessageWriter<ChargeCue>,
) {
    if active.is_none() {
        events.clear();
        return;
    }
    let flashes = comfort.flashes.clamp(0.0, 1.0);
    for ev in events.read() {
        let GameEvent::Charge { player, kind, held } = &ev.event else { continue };
        let Some(v) = views.player(*player) else { continue };
        let pos = v.pos;
        let level = level_of(*held);
        let color = super::corps_rgb(v.corps);
        let height = 1.8 * v.size;
        let grounded = v.has(pflags::GROUNDED);
        let ground = grounded.then(|| (pos.y, ground_color(arena.as_deref(), pos)));
        let cue = |kind| ChargeCue { player: *player, pos, kind, level, held: *held, local: v.is_local };
        match kind {
            ChargeEvent::Start => {
                cues.write(cue(ChargeCueKind::Start));
                sfx.write(Sfx3d::at("gun_railgun_charge", pos + Vec3::Y).gain(0.55).pitch(0.5));
            }
            ChargeEvent::Full => {
                cues.write(cue(ChargeCueKind::Full));
                burst.write(AuraBurstFx { pos, color, height, scale: 0.9 + 0.6 * level, flash: flashes > 0.01, broken: false, ground });
                sfx.write(Sfx3d::at("oath_complete", pos + Vec3::Y).gain(0.9).pitch(1.15));
                if flashes > 0.01 {
                    commands.spawn((
                        FlashLight { t: 0.0, life: 0.3, peak: 1.4e6 * flashes },
                        PointLight {
                            color: super::rgb_color([0.5 + color[0] * 0.5, 0.5 + color[1] * 0.5, 0.5 + color[2] * 0.5]),
                            intensity: 0.0,
                            range: 14.0,
                            shadow_maps_enabled: false,
                            ..default()
                        },
                        Transform::from_translation(pos + Vec3::Y * height * 0.6),
                    ));
                }
            }
            ChargeEvent::Broken => {
                cues.write(cue(ChargeCueKind::Broken));
                if v.alive() {
                    burst.write(AuraBurstFx { pos, color, height, scale: 0.6 + 0.6 * level, flash: false, broken: true, ground: None });
                    sfx.write(Sfx3d::at("break_hardlight", pos + Vec3::Y).gain(0.7).pitch(0.8));
                }
            }
            ChargeEvent::Released => {
                cues.write(cue(ChargeCueKind::Release));
                // Letting go of a long charge: a smaller burst (no flash) — the "HAAA!" moment.
                if *held > 2.5 && v.alive() {
                    burst.write(AuraBurstFx { pos, color, height, scale: 0.5 + 0.4 * level, flash: false, broken: false, ground });
                    sfx.write(Sfx3d::at("mix_whoosh_out", pos + Vec3::Y).gain(0.6));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_auras(
    mut commands: Commands,
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    keys: Res<ViewKeys>,
    comfort: Res<ComfortSettings>,
    mode: Res<super::camera3p::ViewMode>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    mut auras: ResMut<Auras>,
    mut loops: ResMut<ChargeLoops>,
    mut q: Query<(&mut ChargeAuraFx, &mut Transform, &mut PointLight)>,
    mut cues: MessageWriter<ChargeCue>,
) {
    loops.0.clear();
    if active.is_none() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let cam = cams.iter().next().map(|g| g.translation()).unwrap_or(Vec3::ZERO);
    let calm = comfort_calm(&comfort);
    let third = mode.third_person();
    for v in &views.players {
        let on = v.alive() && v.has(pflags::CHARGING);
        let held_wire = keys.curr.players.binary_search_by_key(&v.id.0, |p| p.id).ok().map_or(0, |i| keys.curr.players[i].charge);
        let slot = match auras.0.get_mut(&v.id) {
            Some(s) => s,
            None => {
                if !on {
                    continue;
                }
                let color = super::corps_rgb(v.corps);
                let e = commands
                    .spawn((
                        Name::new(format!("ChargeAura{}", v.id.0)),
                        ChargeAura,
                        ChargeAuraFx::new(color, v.id.0 as u32 * 7919 + 17),
                        Transform::from_translation(v.pos),
                        PointLight {
                            color: super::rgb_color(color),
                            intensity: 0.0,
                            range: 8.0,
                            radius: 0.4,
                            shadow_maps_enabled: false,
                            ..default()
                        },
                        NotShadowCaster,
                    ))
                    .id();
                auras.0.entry(v.id).or_insert(AuraSlot { entity: e, intensity: 0.0, held: 0.0, next_pulse: 0.25, was_on: false })
            }
        };
        // Ease in fast (it should snap on), out a little slower.
        let target = if on { 1.0 } else { 0.0 };
        let rate = if on { 9.0 } else { 4.0 };
        slot.intensity += (target - slot.intensity) * (1.0 - (-rate * dt).exp());
        if on {
            // Follow the host's hold time, advancing smoothly between snapshots.
            let host = held_wire as f32 / WIRE_STEPS_PER_SEC;
            slot.held = if !slot.was_on || (host - slot.held).abs() > 0.4 { host } else { slot.held + dt };
        }
        let level = level_of(slot.held);
        let mut pulse = false;
        if on {
            slot.next_pulse -= dt;
            if slot.next_pulse <= 0.0 {
                slot.next_pulse += pulse_period(level);
                pulse = true;
                cues.write(ChargeCue { player: v.id, pos: v.pos, kind: ChargeCueKind::Pulse, level, held: slot.held, local: v.is_local });
            }
        } else {
            slot.next_pulse = 0.25;
        }
        slot.was_on = on;
        let Ok((mut aura, mut tf, mut light)) = q.get_mut(slot.entity) else { continue };
        tf.translation = v.pos;
        aura.intensity = slot.intensity;
        aura.level = level;
        aura.height = 1.8 * v.size;
        let grounded = v.has(pflags::GROUNDED);
        aura.ground = grounded.then_some(v.pos.y);
        if grounded && on {
            aura.ground_color = ground_color(arena.as_deref(), v.pos);
        }
        aura.view_distance = cam.distance(v.pos);
        aura.first_person = v.is_local && !third;
        aura.calm = calm;
        aura.pulse |= pulse;
        // The aura lights its surroundings (ground wash in corps colour).
        let flicker = if calm { 1.0 } else { 0.9 + 0.1 * (slot.held * 21.0).sin() };
        light.intensity = slot.intensity * (3.0e5 + 6.0e5 * level) * flicker;
        light.range = 6.0 + 4.0 * level;
        if slot.intensity > 0.02 {
            loops.0.push(ChargeLoop { player: v.id, pos: v.pos, level, intensity: slot.intensity, local: v.is_local });
        }
    }
    // Faded out / gone: despawn.
    let dead: Vec<PlayerId> = auras
        .0
        .iter()
        .filter(|(id, s)| match views.player(**id) {
            None => true,
            Some(v) => s.intensity < 0.01 && !v.has(pflags::CHARGING),
        })
        .map(|(id, _)| *id)
        .collect();
    for id in dead {
        if let Some(s) = auras.0.remove(&id) {
            commands.entity(s.entity).try_despawn();
        }
    }
}

/// Chargers pop: brighter palette (bloom) + a faint white sheen on their hard-light body.
fn body_glow(
    auras: Res<Auras>,
    comfort: Res<ComfortSettings>,
    time: Res<Time<Real>>,
    mut bodies: Query<(&PlayerBody, &mut ConstructVisual)>,
) {
    if auras.0.is_empty() {
        return;
    }
    let calm = comfort_calm(&comfort);
    let t = time.elapsed_secs();
    for (b, mut cv) in &mut bodies {
        let Some(s) = auras.0.get(&b.id) else { continue };
        let k = s.intensity.clamp(0.0, 1.0);
        if k < 0.01 {
            continue;
        }
        let level = level_of(s.held);
        let boost = 1.0 + k * (0.45 + 0.45 * level);
        let c = cv.corps_color;
        cv.corps_color = [c[0] * boost, c[1] * boost, c[2] * boost];
        if let Some(a) = cv.accent {
            cv.accent = Some([a[0] * boost, a[1] * boost, a[2] * boost]);
        }
        let shimmer = if calm { 0.0 } else { 0.03 * (t * 11.0 + b.id.0 as f32).sin() };
        cv.flash = cv.flash.max(k * (0.03 + 0.03 * level + shimmer));
        cv.opacity = cv.opacity.max(0.92 + 0.08 * k);
    }
}

/// Darken the world slightly while a charger is in view (never the UI; highlights barely).
fn world_dim(
    time: Res<Time<Real>>,
    auras: Res<Auras>,
    views: Res<WorldViews>,
    comfort: Res<ComfortSettings>,
    mut dim: ResMut<ChargeDim>,
    mut cams: Query<(&GlobalTransform, &mut ColorGrading), With<PlayerCamera>>,
) {
    let dt = time.delta_secs().min(0.1);
    let calm = comfort_calm(&comfort);
    let off = std::env::var_os("BD_CHARGE_NODIM").is_some();
    let Some((cam_tf, _)) = cams.iter().next() else { return };
    let cam = cam_tf.translation();
    let fwd = cam_tf.forward().as_vec3();
    // Strongest in-view charger (no stacking past the cap).
    let mut want = 0.0f32;
    for (id, s) in &auras.0 {
        let Some(v) = views.player(*id) else { continue };
        let to = v.pos + Vec3::Y * 0.9 * v.size - cam;
        let d = to.length();
        let in_view = d < 6.0 || to.dot(fwd) > d * 0.55;
        if !in_view || d > 140.0 {
            continue;
        }
        let near = (1.0 - (d - 40.0) / 100.0).clamp(0.35, 1.0);
        want = want.max(s.intensity * near * (0.75 + 0.25 * level_of(s.held)));
    }
    let mut target = DIM_MAX * want;
    if !calm && target > 0.0 {
        // A faint thrum with the shockwaves.
        target *= 0.9 + 0.1 * (time.elapsed_secs() * std::f32::consts::TAU / 0.55).sin().abs();
    }
    if off {
        target = 0.0;
    }
    // ~0.3 s ease in / out.
    dim.value += (target - dim.value) * (1.0 - (-10.0 * dt).exp());
    if dim.value < 1e-4 && target == 0.0 {
        dim.value = 0.0;
    }
    let g = 1.0 - dim.value;
    if (g - dim.written).abs() < 5e-4 && dim.written != 0.0 {
        return;
    }
    dim.written = g;
    for (_, mut cg) in &mut cams {
        cg.shadows.gain = g;
        cg.midtones.gain = g;
        cg.highlights.gain = 1.0 - dim.value * 0.25;
    }
}

// ------------------------------------------------------------------------------------ scenario

/// `--scenario charge [--charge-layout near|far|three|four|none]` (practice, `--bots 4`): the local
/// player stands still; bots 1–3 are placed in front of the camera and hold the charge button; bot
/// 4 stands beside them with a wall + ring blasts (readability check). Env: `BD_CHARGE_NODIM=1`
/// (dim off, for the side-by-side), `BD_CHARGE_COMFORT=1` (comfort preset for this run only).
pub mod scenario {
    use super::*;
    use crate::input::LookAngles;
    use crate::tick::{SimSet, SimTick};
    use bd_core::input::{PlayerInput, buttons};

    #[derive(Resource, Clone, Debug)]
    pub struct ChargeScenario {
        /// (player, distance along the view, lateral offset).
        pub chargers: Vec<(u8, f32, f32)>,
        placed: bool,
        yaw: f32,
        t: f32,
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "charge") {
            return;
        }
        let layout = args.windows(2).find(|w| w[0] == "--charge-layout").map(|w| w[1].clone()).unwrap_or_else(|| "three".into());
        let chargers = match layout.as_str() {
            "near" => vec![(1, 5.0, 0.0)],
            "far" => vec![(1, 40.0, 0.0)],
            "four" => vec![(1, 7.0, -2.5), (2, 10.0, 3.0), (3, 14.0, -1.0), (4, 5.0, 1.5)],
            "none" => vec![],
            _ => vec![(1, 6.0, -2.0), (2, 10.0, 2.5), (3, 15.0, -0.5)],
        };
        info!("charge scenario: layout {layout} ({} chargers)", chargers.len());
        app.insert_resource(ChargeScenario { chargers, placed: false, yaw: 0.0, t: 0.0 })
            .add_systems(Update, (place, comfort_override).run_if(resource_exists::<ActiveWorld>))
            .add_systems(SimTick, drive.in_set(SimSet::Input).run_if(resource_exists::<ActiveWorld>));
    }

    fn comfort_override(mut comfort: ResMut<ComfortSettings>) {
        if std::env::var_os("BD_CHARGE_COMFORT").is_some() && *comfort != ComfortSettings::COMFORT {
            *comfort = ComfortSettings::COMFORT;
        }
    }

    fn ground_at(w: &bd_sim::World, x: f32, z: f32, near_y: f32) -> Option<bd_core::Vec3> {
        let from = bd_core::Vec3::new(x, near_y + 20.0, z);
        let hit = bd_core::voxel::raycast(&w.voxels, from, bd_core::Vec3::NEG_Y, 60.0)?;
        let y = hit.point.y;
        ((y - near_y).abs() < 14.0).then_some(bd_core::Vec3::new(x, y + 0.05, z))
    }

    fn place(mut sc: ResMut<ChargeScenario>, time: Res<Time<Real>>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>) {
        sc.t += time.delta_secs();
        if sc.placed || sc.t < 0.5 {
            return;
        }
        let Some(me) = active.local_player().filter(|p| p.alive).map(|p| p.mv.pos) else { return };
        // Find a view direction with ground at every charger spot (and at the helper's spot).
        let mut spots = Vec::new();
        let mut chosen = None;
        for step in 0..24 {
            let yaw = step as f32 * std::f32::consts::TAU / 24.0;
            let dir = bd_core::input::look_dir(yaw, 0.0);
            let side = bd_core::Vec3::new(-dir.z, 0.0, dir.x);
            spots.clear();
            let mut ok = true;
            let mut all: Vec<(u8, f32, f32)> = sc.chargers.clone();
            all.push((5, 8.0, 5.0));
            for &(id, d, lat) in &all {
                let p = me + dir * d + side * lat;
                match ground_at(&active.world, p.x, p.z, me.y) {
                    Some(g) => spots.push((id, g)),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                chosen = Some(yaw);
                break;
            }
        }
        let Some(yaw) = chosen else {
            if sc.t > 6.0 {
                warn!("charge scenario: no open ground found around the spawn");
                sc.placed = true;
            }
            return;
        };
        sc.yaw = yaw;
        let world = &mut active.world;
        for (id, g) in &spots {
            if let Some(p) = world.player_mut(PlayerId(*id)) {
                p.mv.pos = *g;
                p.mv.vel = bd_core::Vec3::ZERO;
                p.spawn_shield = 0.0;
                // Staggered pools: the first charger fills up (burst) a couple of seconds in.
                p.willpower = if *id == 1 { 93.0 } else { 25.0 + 15.0 * *id as f32 };
            }
        }
        // The helper (id 5): a wall, so a construct is in the readability shots.
        if spots.iter().any(|(id, _)| *id == 5) {
            let pos = spots.iter().find(|(id, _)| *id == 5).map(|s| s.1).unwrap_or(me);
            let eye = pos + bd_core::Vec3::Y * 1.6;
            let back = bd_core::input::look_dir(yaw + std::f32::consts::PI * 0.5, 0.0);
            use bd_core::mechanics::{Element, Form};
            let wall = bd_sim::bots::recipe("wall", Form::Wall, &[], Element::Light, 3.0);
            let _ = world.cast(PlayerId(5), wall, None, bd_sim::Aim::new(eye, back));
        }
        // Bots stand down: the scenario drives them.
        active.bots.clear();
        look.yaw = yaw;
        look.pitch = -0.08;
        sc.placed = true;
        info!("charge scenario: placed {} players, yaw {:.0}°", spots.len(), yaw.to_degrees());
    }

    fn drive(sc: Res<ChargeScenario>, mut active: ResMut<ActiveWorld>) {
        if !sc.placed {
            return;
        }
        let tick = active.world.tick;
        for &(id, _, _) in &sc.chargers {
            if active.world.player(PlayerId(id)).is_none() {
                continue;
            }
            // Face the camera (back along the view).
            let yaw = sc.yaw + std::f32::consts::PI;
            active
                .remote_inputs
                .push((PlayerId(id), PlayerInput { seq: tick as u32, yaw, pitch: 0.0, buttons: buttons::CHARGE, ..Default::default() }));
        }
        // Helper: ring blasts up into the sky now and then (projectiles in the shots).
        if active.world.player(PlayerId(5)).is_some() {
            let fire = (tick / 30) % 3 == 0;
            let b = if fire { buttons::PRIMARY } else { 0 };
            active
                .remote_inputs
                .push((PlayerId(5), PlayerInput { seq: tick as u32, yaw: sc.yaw + 0.9, pitch: 0.35, buttons: b, ..Default::default() }));
        }
    }
}
