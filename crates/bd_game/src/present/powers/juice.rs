//! Power-move juice (owner: "things like the rinnegan should feel just sick to use, let players FEEL
//! powerful"). Generic and data-driven: every move's `juice` block in powers.ron (`MoveJuice`)
//! drives the same pieces, so any power gets them for free:
//!
//! * **Scale & weight**: a spherical distortion RIPPLE (a translucent hard-light shell racing out to
//!   `ripple` m, + a ground shockwave ring), loose debris flung out / sucked in (`debris`).
//! * **Layered audio**: a sub thump (`boom`) under the move + a world duck (`SignatureCue`).
//! * **Camera**: a FOV punch for the caster (`punch`), shake for players nearby (`shake`) — both
//!   comfort-gated in `juice`. Never takes control.
//! * **Title letters** (`title`): big hard-light words over the caster.
//! * Hit-stop on connects is the ordinary hit path's (every power hit is a real hit).
//!
//! Plus the moments of the generic effects: **Black Flash** crits (black-red lightning around the
//! impact, hit-stop, a suck-in sting), **Ultra Instinct** auto-dodges (a silver afterimage),
//! **Universal Pull** (a gravity line), **gravity cores** (debris spirals into the core, a swirl
//! field, a boom at the collapse), the **Sharingan read** (incoming shots inside the radius get a
//! telegraph ring on the holder's screen only) and **Infinity** (rounds hanging in the field ripple).

use super::super::sound::Sfx3d;
use super::super::{PresentSet, bv, corps_rgb};
use super::ascend::lin;
use crate::debris::{DebrisImpulse, SpawnDebris};
use crate::juice::{CameraShakeEvent, FovPunch, HitStop};
use crate::present::players::PlayerBody;
use crate::present::signature::{Cue, SignatureCue};
use crate::camera::PlayerCamera;
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use bd_core::mechanics::{Element, FormCategory};
use bd_core::{ConstructId, PlayerId};
use bd_fx::{BurstFx, BurstKind, ConstructEnd, ConstructVisual, ForceFieldFx, FxParticles, PFlags, ZapFx};
use bd_sim::GameEvent;
use bd_sim::powers::{At, CritFx, MoveJuice, PowerEvent, PowerTable, effects};
use bevy::prelude::*;

#[derive(Component)]
struct Ripple {
    t: f32,
    dur: f32,
    radius: f32,
    color: [f32; 3],
}

#[derive(Resource)]
struct JuiceAssets {
    sphere: Handle<Mesh>,
}

/// Live gravity cores (client side: the swirl + debris spiral).
#[derive(Resource, Default)]
struct Cores(Vec<(ConstructId, Entity, f32, f32)>);

pub struct PowerJuicePlugin;

impl Plugin for PowerJuicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Cores>()
            .add_message::<SignatureCue>()
            .add_systems(Startup, setup)
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, (juice_events, ripples, cores, read_and_infinity).chain().in_set(PresentSet::Sync));
    }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let sphere = meshes.add(Sphere::new(1.0).mesh().ico(4).unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(32, 16)));
    commands.insert_resource(JuiceAssets { sphere });
}

fn clear(mut commands: Commands, mut cores: ResMut<Cores>, ripples: Query<Entity, With<Ripple>>) {
    for (_, e, _, _) in cores.0.drain(..) {
        commands.entity(e).try_despawn();
    }
    for e in &ripples {
        commands.entity(e).try_despawn();
    }
}

/// The juice of a move that just fired: the def at the player's current tier for that power.
fn move_juice(views: &WorldViews, player: PlayerId, power: u16, slot: u8) -> Option<(&'static MoveJuice, &'static str)> {
    let tier = views.player(player).and_then(|v| v.power.held().find(|h| h.0 == power).map(|h| h.1)).unwrap_or(0);
    let d = PowerTable::get().def(power)?;
    let m = d.moves_at(tier).get(slot as usize)?;
    m.juice.as_ref().map(|j| (j, m.name.as_str()))
}

#[allow(clippy::too_many_arguments)]
fn juice_events(
    mut commands: Commands,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    assets: Res<JuiceAssets>,
    comfort: Res<ComfortSettings>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    bodies: Query<(&PlayerBody, &ConstructVisual, &GlobalTransform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cores: ResMut<Cores>,
    out: (
        MessageWriter<BurstFx>,
        MessageWriter<ZapFx>,
        MessageWriter<Sfx3d>,
        MessageWriter<SignatureCue>,
        MessageWriter<FovPunch>,
        MessageWriter<CameraShakeEvent>,
        MessageWriter<HitStop>,
        MessageWriter<DebrisImpulse>,
    ),
) {
    let (mut burst, mut zap, mut sfx, mut sig, mut fov, mut shake, mut hitstop, mut debris) = out;
    let Some(active) = active else {
        events.clear();
        return;
    };
    let cam = cams.iter().next().map(|g| g.translation()).unwrap_or(Vec3::ZERO);
    let calm = comfort.flashes <= 0.01;
    let local = active.local;
    // Casters whose TWISTED title already showed this batch (their base move's title is skipped).
    let mut twisted: [Option<PlayerId>; 8] = [None; 8];
    for ev in events.read() {
        let GameEvent::Power(pe) = &ev.event else { continue };
        match pe {
            // MODIFIERS & FUSION: a twisted / fused move names itself ("GIANT HOMING RASENGAN"). A
            // twisted famous CAST ("giant spirit bomb") sends no `Fired` at all, so its title only
            // exists here; a twisted move's own title replaces its base move's.
            PowerEvent::Op(bd_sim::powers::ops::OpEvent::Twisted { player, title, element, size, .. }) if !title.trim().is_empty() => {
                let Some(v) = views.player(*player) else { continue };
                if let Some(slot) = twisted.iter_mut().find(|s| s.is_none() || **s == Some(*player)) {
                    *slot = Some(*player);
                }
                let color = if *element == Element::Light { corps_rgb(v.corps) } else { bd_fx::palette::element_color(*element) };
                let mine = Some(*player) == local;
                let big = *size >= 1.5;
                let hand = v.pos + Vec3::Y * 1.3 * v.size;
                let mut sh = bd_fx::Shout::new(bd_fx::sanitize_shout(title, bd_fx::SHOUT_MAX_CHARS), color, if big { 1.9 } else { 1.5 }, mine, hand);
                sh.head = v.pos + Vec3::Y * 1.95 * v.size;
                sh.toward = hand + (Quat::from_euler(EulerRot::YXZ, v.yaw, v.pitch, 0.0) * Vec3::NEG_Z) * 6.0;
                sh.boost = if big { 2.0 } else { 1.5 };
                sh.seed = ev.tick ^ (player.0 as u64) << 24;
                sh.compact = mine;
                sh.title = true;
                sh.caster = player.0 as u64 + 1;
                commands.spawn((Name::new("MoveTitle"), sh));
            }
            PowerEvent::Move { .. } | PowerEvent::Fired { .. } => {
                // TOOLBOX: a borrowed / kit move sends only `Fired` (a held move sends both: its juice
                // plays once, from `Move`).
                let (player, seed_power, juice) = match pe {
                    PowerEvent::Move { player, power, slot } => (player, *power as u64, move_juice(&views, *player, *power, *slot)),
                    PowerEvent::Fired { player, mv, how, .. } if *how != bd_sim::powers::chain::FireHow::Held => {
                        (player, mv.power as u64, mv.resolve(PowerTable::get()).and_then(|(_, m)| m.juice.as_ref().map(|j| (j, m.name.as_str()))))
                    }
                    _ => continue,
                };
                let Some((j, name)) = juice else { continue };
                let Some(v) = views.player(*player) else { continue };
                let color = j.color.map(lin).unwrap_or(corps_rgb(v.corps));
                let eye = v.pos + Vec3::Y * 1.6 * v.size;
                let look = Quat::from_euler(EulerRot::YXZ, v.yaw, v.pitch, 0.0) * Vec3::NEG_Z;
                let at = match j.at {
                    At::Me => v.pos + Vec3::Y * 0.9 * v.size,
                    At::Front => eye + look * j.dist.clamp(0.0, 30.0),
                    At::Aim => {
                        let hit = bd_core::voxel::raycast(&active.world.voxels, super::super::gv(eye), super::super::gv(look), 60.0);
                        hit.map_or(eye + look * 20.0, |h| bv(h.point))
                    }
                };
                let mine = Some(*player) == local;
                if j.ripple > 0.0 {
                    let mat = materials.add(StandardMaterial {
                        base_color: Color::linear_rgba(color[0], color[1], color[2], 0.22),
                        emissive: LinearRgba::rgb(color[0] * 1.6, color[1] * 1.6, color[2] * 1.6),
                        alpha_mode: AlphaMode::Blend,
                        cull_mode: None,
                        unlit: true,
                        double_sided: true,
                        ..default()
                    });
                    commands.spawn((
                        Ripple { t: 0.0, dur: (0.22 + j.ripple * 0.022).min(0.6), radius: j.ripple, color },
                        Mesh3d(assets.sphere.clone()),
                        MeshMaterial3d(mat),
                        Transform::from_translation(at).with_scale(Vec3::splat(0.3)),
                        bevy::light::NotShadowCaster,
                    ));
                    if j.ripple >= 5.0 {
                        // Shards from your own body stay small for you (never a wall in your face).
                        let own = if at.distance(cam) < 3.0 { 0.3 } else { 1.0 };
                        burst.write(BurstFx { kind: BurstKind::Shards, pos: at, dir: Vec3::ZERO, scale: (j.ripple / 5.0).min(3.0) * own, color: Some(color) });
                    }
                }
                if j.debris != 0.0 {
                    debris.write(DebrisImpulse { center: at, radius: j.ripple.max(6.0) * 1.3, strength: j.debris });
                }
                if j.boom {
                    sfx.write(Sfx3d::at("explosion_big", at).pitch(0.55).gain(0.9));
                    sfx.write(Sfx3d::at("domain_open", at).pitch(0.8).gain(0.6));
                    sig.write(SignatureCue { cue: Cue::WorldDuck, pos: at, severity: 0.7, mine, element: Element::Light, construct: ConstructId(u32::MAX) });
                }
                if mine && j.punch > 0.0 {
                    fov.write(FovPunch { degrees: j.punch });
                }
                if j.shake > 0.0 && (mine || at.distance(cam) < j.ripple.max(8.0) * 2.0) {
                    shake.write(CameraShakeEvent { trauma: j.shake.min(0.4), dir: Vec3::ZERO });
                }
                if let Some(title) = j.title.as_deref().filter(|t| !t.is_empty()).filter(|_| !twisted.contains(&Some(*player))) {
                    let hand = v.pos + Vec3::Y * 1.3 * v.size;
                    let mut sh = bd_fx::Shout::new(bd_fx::sanitize_shout(title, bd_fx::SHOUT_MAX_CHARS), color, if j.boom { 1.9 } else { 1.3 }, mine, hand);
                    sh.head = v.pos + Vec3::Y * 1.95 * v.size;
                    sh.toward = at;
                    sh.boost = if j.boom { 2.0 } else { 1.3 };
                    sh.seed = ev.tick ^ seed_power << 20;
                    // The caster reads it small, low-right and brief (never over the aim).
                    sh.compact = mine;
                    // A move title (TEXT CLUTTER): hidden when comic text is Off; a chain's titles
                    // stack per caster instead of piling up (bd_fx text_budget `titles`).
                    sh.title = true;
                    sh.caster = player.0 as u64 + 1;
                    commands.spawn((Name::new("MoveTitle"), sh));
                }
                let _ = name;
            }
            PowerEvent::Crit { attacker, target, point, fx, mult } => {
                let p = bv(*point);
                let near = local.is_some_and(|l| l == *attacker || l == *target) || p.distance(cam) < 20.0;
                match fx {
                    CritFx::BlackFlash => {
                        // Black-red lightning bent around the impact.
                        for k in 0..7 {
                            let a = k as f32 * 0.9 + ev.tick as f32 * 0.37;
                            let dir = Vec3::new(a.cos(), ((k * 37 % 11) as f32 / 11.0 - 0.3) * 1.2, a.sin()).normalize_or(Vec3::Y);
                            let col = if k % 2 == 0 { [0.9, 0.04, 0.06] } else { [0.03, 0.01, 0.02] };
                            zap.write(ZapFx { from: p, to: p + dir * (1.2 + 0.25 * k as f32), color: col, duration: 0.28 });
                        }
                        burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::ZERO, scale: 1.4 + 0.2 * mult, color: Some([0.05, 0.0, 0.01]) });
                        burst.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: Vec3::ZERO, scale: 1.2, color: Some([1.0, 0.05, 0.08]) });
                        sfx.write(Sfx3d::at("final_blow_suck", p).pitch(0.8));
                    }
                    CritFx::Spark => {
                        burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::ZERO, scale: 1.2, color: Some([1.0, 0.95, 0.8]) });
                    }
                }
                sfx.write(Sfx3d::at("hit_crit", p).pitch(0.75));
                if near {
                    hitstop.write(HitStop { secs: 0.12 });
                    shake.write(CameraShakeEvent { trauma: 0.22, dir: Vec3::ZERO });
                }
            }
            PowerEvent::AutoDodged { player, from, .. } => {
                let silver = [0.8, 0.85, 1.0];
                let f = bv(*from);
                if let Some((_, vis, gt)) = bodies.iter().find(|(b, _, _)| b.id == *player) {
                    let mut ghost = ConstructVisual::new(vis.model.clone(), silver);
                    ghost.opacity = 0.45;
                    ghost.layers = vis.layers;
                    let mut t = gt.compute_transform();
                    t.translation = f;
                    commands.spawn((ghost, t, ConstructEnd::Dismiss { duration: 0.5 }));
                }
                if let Some(v) = views.player(*player) {
                    zap.write(ZapFx { from: f + Vec3::Y, to: v.pos + Vec3::Y, color: silver, duration: 0.12 });
                }
                sfx.write(Sfx3d::at("dodge", f).pitch(1.35).gain(0.8));
            }
            PowerEvent::Yanked { player, target, construct, from } => {
                let col = views.player(*player).map_or([0.7, 0.62, 0.95], |v| corps_rgb(v.corps));
                let to = views.player(*player).map_or(bv(*from), |v| v.pos + Vec3::Y);
                zap.write(ZapFx { from: bv(*from), to, color: [0.7, 0.62, 0.95], duration: 0.3 });
                burst.write(BurstFx { kind: BurstKind::Flash, pos: bv(*from), dir: Vec3::ZERO, scale: 0.8, color: Some(col) });
                sfx.write(Sfx3d::at("hook_fire", bv(*from)).pitch(0.6));
                let _ = (target, construct);
            }
            PowerEvent::Singularity { construct, pos, radius, secs, end, .. } => {
                let p = bv(*pos);
                if !*end {
                    let e = commands.spawn((ForceFieldFx { radius: *radius, attract: true, color: [0.45, 0.35, 0.65], intensity: 1.6 }, Transform::from_translation(p))).id();
                    cores.0.push((*construct, e, *radius, *secs));
                    sfx.write(Sfx3d::at("ult_charge", p).pitch(0.6));
                    sfx.write(Sfx3d::at("collapse_rumble", p).gain(0.9));
                } else {
                    if let Some(k) = cores.0.iter().position(|c| c.0 == *construct) {
                        let (_, e, _, _) = cores.0.swap_remove(k);
                        commands.entity(e).try_despawn();
                    }
                    burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::ZERO, scale: 3.0, color: Some([0.3, 0.2, 0.45]) });
                    burst.write(BurstFx { kind: BurstKind::Dust, pos: p, dir: Vec3::ZERO, scale: 3.0, color: None });
                    debris.write(DebrisImpulse { center: p, radius: radius * 1.2, strength: 18.0 });
                    sfx.write(Sfx3d::at("explosion_big", p).pitch(0.5));
                    sig.write(SignatureCue { cue: Cue::WorldDuck, pos: p, severity: 0.8, mine: false, element: Element::Shadow, construct: *construct });
                    if p.distance(cam) < radius * 2.0 && !calm {
                        shake.write(CameraShakeEvent { trauma: 0.3, dir: Vec3::ZERO });
                    }
                }
            }
            // Generic-op events (op rays, DelayedHit marks, the arena ops' beats…): `op_visuals`.
            _ => {}
        }
    }
}

/// The distortion shells race out and fade (and fade out where the shell passes the camera: a
/// shell from your own body never smears across the near plane).
fn ripples(
    mut commands: Commands,
    time: Res<Time<Real>>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    mut q: Query<(Entity, &mut Ripple, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs().min(0.1);
    let cam = cams.iter().next().map(|g| g.translation());
    for (e, mut r, mut tf, mat) in &mut q {
        r.t += dt;
        let u = (r.t / r.dur).clamp(0.0, 1.0);
        if u >= 1.0 {
            commands.entity(e).try_despawn();
            continue;
        }
        let ease = 1.0 - (1.0 - u) * (1.0 - u) * (1.0 - u);
        let radius = (0.3 + (r.radius - 0.3) * ease).max(0.05);
        tf.scale = Vec3::splat(radius);
        if let Some(mut m) = materials.get_mut(&mat.0) {
            let near = cam.map_or(1.0, |c| ripple_near_fade((c.distance(tf.translation) - radius).abs()));
            // Inside the shell (a 22 m snap ripple around a bystander) it would tint the whole
            // screen: keep it a faint shimmer — a flash never whites out someone else's view.
            let near = if cam.is_some_and(|c| c.distance(tf.translation) < radius) { near * 0.12 } else { near };
            let a = 0.24 * (1.0 - u) * (1.0 - u) * near;
            m.base_color = Color::linear_rgba(r.color[0], r.color[1], r.color[2], a);
            let k = 1.8 * (1.0 - u) * near;
            m.emissive = LinearRgba::rgb(r.color[0] * k, r.color[1] * k, r.color[2] * k);
        }
    }
}

/// A ripple shell `gap` m from the camera: 0 (hidden) when the shell passes through the lens, 1 from
/// ~1.8 m away.
pub fn ripple_near_fade(gap: f32) -> f32 {
    let t = ((gap - 0.25) / 1.55).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Gravity cores: follow the core, spiral debris in, and drip terrain chunks up into it.
fn cores(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mut cores: ResMut<Cores>,
    mut commands: Commands,
    mut tfs: Query<&mut Transform, With<ForceFieldFx>>,
    mut debris: MessageWriter<DebrisImpulse>,
    mut spawn: MessageWriter<SpawnDebris>,
) {
    if cores.0.is_empty() {
        return;
    }
    let Some(active) = active else { return };
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();
    cores.0.retain(|(cid, e, radius, _)| {
        let Some(c) = views.construct(*cid) else {
            commands.entity(*e).try_despawn();
            return false;
        };
        if let Ok(mut tf) = tfs.get_mut(*e) {
            tf.translation = c.pos;
        }
        // Continuous inward pull on loose debris (the orbiting mass).
        debris.write(DebrisImpulse { center: c.pos, radius: *radius, strength: -30.0 * dt });
        // A few chunks rip up off the ground toward it every frame.
        for k in 0..2 {
            let a = t * 3.1 + k as f32 * 2.4 + cid.0 as f32;
            let r = radius * (0.35 + 0.5 * ((t * 1.7 + k as f32).sin() * 0.5 + 0.5));
            let top = Vec3::new(c.pos.x + a.cos() * r, c.pos.y, c.pos.z + a.sin() * r);
            let Some(ground) = bd_core::voxel::raycast(&active.world.voxels, super::super::gv(top), bd_core::Vec3::NEG_Y, 30.0) else { continue };
            let base = bv(ground.point) + Vec3::Y * 0.3;
            let to = (c.pos - base).normalize_or(Vec3::Y);
            spawn.write(SpawnDebris { pos: base, vel: to * 14.0 + Vec3::Y * 3.0, color: [0.3, 0.26, 0.22], size: 0.18 + 0.1 * (k as f32), emissive: 0.0, lifetime: 2.5 });
        }
        true
    });
}

/// Sharingan / Six Eyes (`ReadSlow`): incoming rounds and thrown things inside the radius get a
/// telegraph ring on the HOLDER's screen only. Infinity (`ProjectileSlowField`): rounds hanging in
/// anyone's field shimmer.
fn read_and_infinity(active: Option<Res<ActiveWorld>>, views: Res<WorldViews>, time: Res<Time<Real>>, cams: Query<&GlobalTransform, With<PlayerCamera>>, mut fx: ResMut<FxParticles>) {
    let Some(active) = active else { return };
    let w = &active.world;
    if w.projectiles.is_empty() && w.constructs.is_empty() {
        return;
    }
    let cam = cams.iter().next();
    let (right, up) = cam.map_or((Vec3::X, Vec3::Y), |g| (g.right().as_vec3(), g.up().as_vec3()));
    let t = time.elapsed_secs();
    let ring = |fx: &mut FxParticles, c: Vec3, r: f32, col: [f32; 3], e: f32| {
        let n = 14;
        let mut prev = c + right * r;
        for i in 1..=n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let p = c + (right * a.cos() + up * a.sin()) * r;
            fx.immediate_segment(prev, p, 0.035, col, e, PFlags::PLAIN);
            prev = p;
        }
    };
    // The local reader.
    if let Some(me) = active.local.and_then(|id| w.player(id)).filter(|p| p.alive)
        && let Some(r) = effects::read_radius(&me.power)
    {
        let my = bv(me.hull(w.params()).center());
        let col = me.power.held().find_map(|h| h.def.ascend_at(h.tier).and_then(|a| a.eye_glow)).map(lin).unwrap_or([1.0, 0.15, 0.1]);
        for p in &w.projectiles {
            if p.spec.owner == me.id {
                continue;
            }
            let pos = bv(p.pos);
            let d = my - pos;
            if d.length() > r || bv(p.vel).dot(d) <= 0.0 {
                continue;
            }
            let pulse = 0.35 + 0.1 * (t * 18.0).sin();
            ring(&mut fx, pos, pulse, col, 5.0);
        }
        for c in w.constructs.iter() {
            if c.owner == me.id || c.category != FormCategory::Object || !c.comp.launched || c.attached() {
                continue;
            }
            let pos = bv(c.center());
            let d = my - pos;
            if d.length() > r || bv(c.vel).dot(d) <= 0.0 || c.vel.length() < 3.0 {
                continue;
            }
            ring(&mut fx, pos, c.half.max_element() + 0.3, col, 4.0);
        }
    }
    // Infinity: rounds inside a field ripple (everyone sees it).
    for v in &views.players {
        if v.power.id == 0 || !v.alive() {
            continue;
        }
        let Some(p) = w.player(v.id) else { continue };
        let Some((radius, _)) = effects::slow_field(&p.power) else { continue };
        let center = bv(p.hull(w.params()).center());
        for pr in &w.projectiles {
            if pr.spec.owner == v.id {
                continue;
            }
            let pos = bv(pr.pos);
            if pos.distance(center) > radius {
                continue;
            }
            for k in 0..2 {
                let ph = ((t * 2.2 + k as f32 * 0.5) % 1.0) * 0.8 + 0.15;
                ring(&mut fx, pos, ph, [0.55, 0.8, 1.0], 3.0 * (1.0 - ph));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_ripple_shell_fades_where_it_crosses_the_camera() {
        assert_eq!(super::ripple_near_fade(0.0), 0.0, "through the lens: hidden");
        assert!(super::ripple_near_fade(0.9) > 0.2 && super::ripple_near_fade(0.9) < 0.8);
        assert_eq!(super::ripple_near_fade(3.0), 1.0, "far: untouched");
    }
}
