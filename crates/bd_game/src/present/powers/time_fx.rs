//! TIME MANIPULATION moments (PRESENTATION POLISH). The owner loves ZA WARUDO: a time stop must be
//! an EVENT, not a grey filter. Presentation only — the sim slows the others (never freezes input).
//!
//! * **The bubble** (a `TimeStop` op: The World, Star Platinum, Chaos Control…): a translucent shell
//!   in the COMPLEMENT of the move's colour bursts out of the caster to ~40 m, washing every screen it
//!   passes (the hue swings round as it crosses your camera: the "inverted" frame), then sucks back
//!   in. Rings race along the ground, the drained world follows.
//! * **The stopped world**: desaturated, dimmed, tinted toward the move's colour; dust hangs frozen
//!   in the air round your camera; a clock tick every half second, slowing. The caster STAYS IN
//!   COLOUR: a lit outline round their body + a clock dial turning on the ground under them (so
//!   everyone reads who owns the moment, from across the map).
//! * **Time resumes**: a ring collapses back onto the caster, colour snaps back with a saturation
//!   kick, a whoosh + a glassy crack.
//! * **Witch Time** (a perfect `Evade`): no bubble — the world sinks into moonlit violet, a crescent
//!   ring round the dodger, a slow heartbeat.
//!
//! Replaces the old flat time-stop grey (`present::powers::time_stop_grey`); keeps `TimeStopGrey`
//! current. Comfort: "flashes" 0 drops the hue swing and the wash; the grade is gentler.
//! Cost: ≤ 2 dome entities per time stop (a mesh shared, one material each, spawned on the event),
//! a fixed slot array, immediate segments for the dial / outline; nothing allocated per frame.

use super::super::sound::Sfx3d;
use super::super::{PresentSet, corps_rgb};
use super::movefx::draw::{Look, Pen, circle, mix3};
use super::movefx::table::MoveFx;
use super::movefx::{MoveFxFired, resolve_look};
use super::TimeStopGrey;
use crate::camera::{CameraUpdateSet, PlayerCamera};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, WorldViews};
use crate::state::AppState;
use crate::viewmodel::{HandsPublishSet, ViewmodelHands};
use bd_core::PlayerId;
use bd_fx::{Emit, FxParticles, PFlags, PKind};
use bd_sim::powers::{MoveOp, power_flags};
use bevy::prelude::*;
use bevy::render::view::ColorGrading;
use std::f32::consts::{PI, TAU};

const SLOTS: usize = 16;
/// How far the bubble reaches (m).
const DOME_R: f32 = 42.0;
const DOME_OUT: f32 = 0.55;
const DOME_HOLD: f32 = 0.15;
const DOME_IN: f32 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    /// A TimeStop op: the full bubble.
    Stop,
    /// A perfect dodge's slow world (Witch Time).
    Witch,
}

#[derive(Clone, Copy)]
struct Stopper {
    id: PlayerId,
    on: bool,
    t: f32,
    look: Look,
    style: Style,
    ticks: u32,
}

/// What the last time move of each player looked like (set from `MoveFxFired`).
#[derive(Clone, Copy)]
struct Noted {
    id: PlayerId,
    look: Look,
    style: Style,
    at: f32,
    /// The bubble's reach (m): twice the move's `juice.ripple` (18 → 36 m), 18 … 42.
    reach: f32,
}

#[derive(Resource)]
struct TimeFx {
    s: [Option<Stopper>; SLOTS],
    noted: [Option<Noted>; SLOTS],
    /// Eased grade 0..1 and the look that tints it.
    grey: f32,
    tint: [f32; 3],
    witch: f32,
    /// Seconds since the last bubble started (hue swing) / the last resume (saturation kick).
    since_stop: f32,
    since_resume: f32,
    active: bool,
}

impl Default for TimeFx {
    fn default() -> Self {
        Self { s: [None; SLOTS], noted: [None; SLOTS], grey: 0.0, tint: [1.0; 3], witch: 0.0, since_stop: 99.0, since_resume: 99.0, active: false }
    }
}

#[derive(Resource)]
struct DomeMesh(Handle<Mesh>);

/// One expanding / collapsing shell.
#[derive(Component)]
struct Dome {
    t: f32,
    /// false = the resume ring: starts big, collapses fast.
    out: bool,
    color: [f32; 3],
    alpha: f32,
    mat: Handle<StandardMaterial>,
    owner: PlayerId,
    reach: f32,
}

pub struct TimeFxPlugin;

impl Plugin for TimeFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TimeFx>()
            .add_message::<MoveFxFired>()
            .add_message::<Sfx3d>()
            .add_systems(Startup, setup)
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, (note, edges, domes).chain().in_set(PresentSet::Sync))
            .add_systems(PostUpdate, (draw, grade).after(CameraUpdateSet).after(HandsPublishSet));
    }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let m = meshes.add(Sphere::new(1.0).mesh().ico(4).unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(32, 16)));
    commands.insert_resource(DomeMesh(m));
}

fn clear(mut commands: Commands, mut st: ResMut<TimeFx>, domes: Query<Entity, With<Dome>>, mut cams: Query<&mut ColorGrading, With<PlayerCamera>>) {
    let was = st.active || st.grey > 0.0;
    *st = TimeFx::default();
    for e in &domes {
        commands.entity(e).try_despawn();
    }
    if was {
        for mut cg in &mut cams {
            reset(&mut cg);
        }
    }
}

fn reset(cg: &mut ColorGrading) {
    cg.global.post_saturation = 1.04;
    cg.global.exposure = 0.0;
    cg.global.hue = 0.0;
    cg.global.temperature = 0.0;
    cg.global.tint = 0.0;
}

/// Remember the look of every time move as it fires (its bubble / tint colours).
fn note(time: Res<Time<Real>>, mut fired: MessageReader<MoveFxFired>, views: Res<WorldViews>, table: Res<MoveFx>, mut st: ResMut<TimeFx>) {
    let now = time.elapsed_secs();
    for f in fired.read() {
        let style = if f.mv.ops.iter().any(|o| matches!(o, MoveOp::TimeStop { .. })) {
            Style::Stop
        } else if f.mv.ops.iter().any(|o| matches!(o, MoveOp::Evade { .. })) {
            Style::Witch
        } else {
            continue;
        };
        let corps = views.player(f.player).map_or([1.0; 3], |v| corps_rgb(v.corps));
        let look = resolve_look(&table, f.mv, f.power, corps);
        let ripple = f.mv.juice.as_ref().map_or(0.0, |j| j.ripple);
        let reach = if ripple > 0.0 { (ripple * 2.0).clamp(18.0, DOME_R) } else { DOME_R };
        let n = Noted { id: f.player, look, style, at: now, reach };
        let k = st.noted.iter().position(|x| x.is_some_and(|x| x.id == f.player)).or_else(|| st.noted.iter().position(|x| x.is_none())).unwrap_or(0);
        st.noted[k] = Some(n);
    }
}

fn complement(c: [f32; 3]) -> [f32; 3] {
    let m = c[0].max(c[1]).max(c[2]).max(0.05);
    let n = [c[0] / m, c[1] / m, c[2] / m];
    mix3([1.0 - n[0], 1.0 - n[1], 1.0 - n[2]], [0.9, 0.9, 1.0], 0.2)
}

/// Rising / falling edges of every player's TIME_STOP flag: the bubble, the resume.
#[allow(clippy::too_many_arguments)]
fn edges(
    mut commands: Commands,
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    table: Res<MoveFx>,
    mesh: Option<Res<DomeMesh>>,
    comfort: Res<ComfortSettings>,
    mut st: ResMut<TimeFx>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fx: ResMut<FxParticles>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else { return };
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs();
    st.since_stop += dt;
    st.since_resume += dt;
    let cam = cams.iter().next().map_or(Vec3::ZERO, |g| g.translation());
    let calm = comfort.flashes <= 0.01;
    let local = active.local;
    let mut pen = Pen { fx: &mut fx, motes: table.t.beats.motes, scale: 1.0, cam, avoid: None };
    // Falling edges (and gone players) first.
    for k in 0..SLOTS {
        let Some(mut s) = st.s[k] else { continue };
        let v = views.player(s.id);
        let on = v.is_some_and(|v| v.alive() && v.power.has(power_flags::TIME_STOP));
        if on {
            s.t += dt;
            st.s[k] = Some(s);
            continue;
        }
        st.s[k] = None;
        if !s.on {
            continue;
        }
        // TIME RESUMES.
        st.since_resume = 0.0;
        let c = v.map_or(cam, |v| v.pos + Vec3::Y * v.size);
        let look = s.look;
        pen.ring(&look, c - Vec3::Y * 0.9, Vec3::Y, 6.0, -9.0, 1.2, 0.5);
        pen.burst(&look, c, 1.0, 5.0, None, 0.0, 0.5);
        if s.style == Style::Stop {
            if let Some(m) = &mesh {
                spawn_dome(&mut commands, &mut materials, m, c, complement(look.main), false, s.id, calm, DOME_R * 0.6);
            }
            sfx.write(Sfx3d::at("mv_time_resume", c).gain(1.0).exempt());
        } else {
            sfx.write(Sfx3d::at("mix_whoosh_out", c).pitch(0.8).gain(0.7));
        }
    }
    // Rising edges.
    for v in views.players.iter().filter(|v| v.alive() && v.power.has(power_flags::TIME_STOP)) {
        if st.s.iter().any(|s| s.is_some_and(|s| s.id == v.id)) {
            continue;
        }
        let noted = st.noted.iter().flatten().find(|n| n.id == v.id && now - n.at < 1.5).copied();
        let look = noted.map_or_else(|| Look { main: corps_rgb(v.corps), core: [1.0; 3], ..Look::default() }, |n| n.look);
        let style = noted.map_or(Style::Stop, |n| n.style);
        let Some(k) = st.s.iter().position(|s| s.is_none()) else { break };
        st.s[k] = Some(Stopper { id: v.id, on: true, t: 0.0, look, style, ticks: 0 });
        // At eye height: from inside (the caster, anyone near) the bubble is an even wash.
        let c = v.pos + Vec3::Y * 1.6 * v.size;
        let mine = Some(v.id) == local;
        match style {
            Style::Stop => {
                st.since_stop = 0.0;
                if let Some(m) = &mesh {
                    let reach = noted.map_or(DOME_R, |n| n.reach);
                    spawn_dome(&mut commands, &mut materials, m, c, complement(look.main), true, v.id, calm, reach);
                }
                // Ground rings racing out + the air punched outward (not from under your own lens).
                if !mine {
                    for (i, sp) in [30.0f32, 20.0, 12.0].into_iter().enumerate() {
                        pen.ring(&look, v.pos + Vec3::Y * (0.1 + 0.05 * i as f32), Vec3::Y, 0.5, sp, 1.4, 0.7);
                    }
                }
                pen.implode(&look, c, 2.4, 0.25, 1.6);
                pen.flash(c, 1.0, look.core, look.glow * 1.6, 0.12);
                sfx.write(Sfx3d::at("mv_time_stop", c).gain(1.0).exempt());
                sfx.write(Sfx3d::at("domain_open", c).pitch(0.65).gain(0.5).exempt());
                if !mine {
                    sfx.write(Sfx3d::ui("heartbeat").pitch(0.8).gain(0.7));
                }
                // Dust hangs frozen round your camera for the whole stop.
                hang_dust(&mut pen, cam, look, 2.6);
            }
            Style::Witch => {
                let moon = Look { main: [0.55, 0.3, 1.0], core: [0.9, 0.8, 1.0], ..look };
                pen.ring(&moon, v.pos + Vec3::Y * 0.1, Vec3::Y, 0.4, 14.0, 1.2, 0.6);
                pen.flash(c, 1.4, moon.core, moon.glow * 1.3, 0.12);
                sfx.write(Sfx3d::at("mix_whoosh_in", c).pitch(0.6).gain(0.9).exempt());
                sfx.write(Sfx3d::at("mv_time_tick", c).pitch(1.2).gain(0.7));
                hang_dust(&mut pen, cam, moon, 1.6);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_dome(commands: &mut Commands, materials: &mut Assets<StandardMaterial>, mesh: &DomeMesh, at: Vec3, color: [f32; 3], out: bool, owner: PlayerId, calm: bool, reach: f32) {
    let a = if calm { 0.08 } else { 0.2 };
    let mat = materials.add(StandardMaterial {
        base_color: Color::linear_rgba(color[0], color[1], color[2], a),
        emissive: LinearRgba::rgb(color[0] * 0.8, color[1] * 0.8, color[2] * 0.8),
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        unlit: true,
        double_sided: true,
        ..default()
    });
    commands.spawn((
        Name::new("TimeDome"),
        Dome { t: 0.0, out, color, alpha: a, mat: mat.clone(), owner, reach },
        Mesh3d(mesh.0.clone()),
        MeshMaterial3d(mat),
        Transform::from_translation(at).with_scale(Vec3::splat(if out { 0.3 } else { reach })),
        bevy::light::NotShadowCaster,
    ));
}

/// Motes hanging dead still round the camera (time stopped): a few dozen, one-shot.
fn hang_dust(pen: &mut Pen, cam: Vec3, look: Look, secs: f32) {
    for _ in 0..48 {
        let d = pen.fx.rng.dir();
        let r = 2.5 + pen.fx.rng.f() * 9.0;
        let p = cam + Vec3::new(d.x, d.y * 0.5, d.z) * r;
        let col = mix3(look.core, [0.95, 0.95, 1.0], 0.5);
        let size = 0.035 + pen.fx.rng.f() * 0.03;
        pen.put(Emit { kind: PKind::Ballistic, flags: PFlags::PLAIN, pos: p, vel: Vec3::ZERO, size, color: col, emissive: 1.4, life: secs, gravity: 0.0, drag: 0.0, spin: 0.2, ..default() });
    }
}

/// The bubble expands (ease-out), holds a beat, sucks back in; the resume shell just collapses.
fn domes(mut commands: Commands, time: Res<Time<Real>>, views: Res<WorldViews>, mut q: Query<(Entity, &mut Dome, &mut Transform)>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let dt = time.delta_secs().min(0.1);
    for (e, mut d, mut tf) in &mut q {
        d.t += dt;
        let (scale, alpha, done) = if d.out {
            let t = d.t;
            let r = d.reach;
            if t < DOME_OUT {
                let u = t / DOME_OUT;
                (0.3 + (r - 0.3) * (1.0 - (1.0 - u).powi(3)), 1.0, false)
            } else if t < DOME_OUT + DOME_HOLD {
                (r, 1.0, false)
            } else {
                let u = ((t - DOME_OUT - DOME_HOLD) / DOME_IN).min(1.0);
                (r * (1.0 - u * u).max(0.01), 1.0 - 0.5 * u, u >= 1.0)
            }
        } else {
            let u = (d.t / 0.3).min(1.0);
            (d.reach * (1.0 - u).powi(2) + 0.2, 1.0 - u, u >= 1.0)
        };
        if done {
            commands.entity(e).try_despawn();
            continue;
        }
        // Follow the caster while it collapses back (it is THEIR moment).
        if let Some(v) = views.player(d.owner) {
            tf.translation = tf.translation.lerp(v.pos + Vec3::Y * v.size, 0.2);
        }
        tf.scale = Vec3::splat(scale);
        if let Some(mut m) = materials.get_mut(&d.mat) {
            let c = d.color;
            m.base_color = Color::linear_rgba(c[0], c[1], c[2], (d.alpha * alpha).clamp(0.0, 0.3));
        }
    }
}

/// Every frame of a stop: the caster's outline (they stay in colour), the clock dial under them, the
/// ticks.
fn draw(views: Res<WorldViews>, hands: Res<ViewmodelHands>, mut st: ResMut<TimeFx>, mut fx: ResMut<FxParticles>, cams: Query<&GlobalTransform, With<PlayerCamera>>, active: Option<Res<ActiveWorld>>, mut sfx: MessageWriter<Sfx3d>) {
    if st.s.iter().all(|s| s.is_none()) {
        return;
    }
    let Some(active) = active else { return };
    let cam = cams.iter().next().map_or(Vec3::ZERO, |g| g.translation());
    fx.scope(cam, 0.0022, 700);
    for k in 0..SLOTS {
        let Some(mut s) = st.s[k] else { continue };
        let Some(v) = views.player(s.id) else { continue };
        let look = s.look;
        let own_fp = Some(s.id) == active.local && hands.active;
        let fade = (s.t / 0.25).min(1.0);
        let feet = v.pos + Vec3::Y * 0.06;
        let chest = v.pos + Vec3::Y * 1.05 * v.size;
        let far = (chest.distance(cam) / 15.0).clamp(1.0, 2.4);
        match s.style {
            Style::Stop if own_fp => {
                // Your own stop in first person: the dial would lie across the bottom of your view.
                let n = (s.t * 2.0) as u32;
                if n > s.ticks {
                    s.ticks = n;
                    sfx.write(Sfx3d::ui("mv_time_tick").pitch(if n % 2 == 0 { 0.75 } else { 0.62 }).gain(0.6));
                }
            }
            Style::Stop => {
                // The clock dial: twelve ticks, two hands, turning (seen from anywhere).
                let r = 2.4 * v.size;
                circle(&mut fx, feet, Vec3::Y, r, 36, 0.05 * far, look.main, look.glow * 1.2 * fade, 0.0);
                circle(&mut fx, feet, Vec3::Y, r * 0.93, 36, 0.02 * far, look.core, look.glow * 0.8 * fade, 0.0);
                for i in 0..12 {
                    let a = i as f32 / 12.0 * TAU;
                    let d = Vec3::new(a.cos(), 0.0, a.sin());
                    let inner = if i % 3 == 0 { 0.72 } else { 0.82 };
                    fx.immediate_segment(feet + d * r * inner, feet + d * r * 0.93, if i % 3 == 0 { 0.09 } else { 0.05 } * far, look.core, look.glow * 1.4 * fade, PFlags::PLAIN);
                }
                // Hands: they tick (jump) once a second, the fast one every half second.
                let a1 = -((s.t * 2.0).floor() / 12.0) * TAU;
                let a2 = -(s.t.floor() / 60.0 + 0.25) * TAU;
                for (a, len, th) in [(a1, 0.85, 0.06), (a2, 0.55, 0.11)] {
                    let d = Vec3::new(a.cos(), 0.0, a.sin());
                    fx.immediate_segment(feet + Vec3::Y * 0.02, feet + Vec3::Y * 0.02 + d * r * len, th * far, look.core, look.glow * 1.6 * fade, PFlags::PLAIN);
                }
                if !own_fp {
                    outline(&mut fx, cam, chest, v.size, look, s.t, far, fade);
                }
                // Tick… tock, slowing (the world has stopped; only the caster's clock runs).
                let n = (s.t * 2.0) as u32;
                if n > s.ticks {
                    s.ticks = n;
                    let c = v.pos + Vec3::Y;
                    sfx.write(Sfx3d::at("mv_time_tick", c).pitch(if n % 2 == 0 { 0.75 } else { 0.62 }).gain(0.8));
                }
            }
            Style::Witch => {
                let moon = [0.55, 0.3, 1.0];
                // A crescent ring round the dodger, turning slowly.
                let r = 1.3 * v.size;
                let seg_n = 20;
                for i in 0..seg_n {
                    let f0 = i as f32 / seg_n as f32;
                    let f1 = (i + 1) as f32 / seg_n as f32;
                    let a0 = s.t * 0.8 + f0 * PI * 1.4;
                    let a1 = s.t * 0.8 + f1 * PI * 1.4;
                    let w = (f0 * PI).sin();
                    fx.immediate_segment(feet + Vec3::new(a0.cos(), 0.0, a0.sin()) * r, feet + Vec3::new(a1.cos(), 0.0, a1.sin()) * r, (0.02 + 0.08 * w) * far, moon, 3.0 * fade, PFlags::PLAIN);
                }
                if !own_fp {
                    outline(&mut fx, cam, chest, v.size, Look { main: moon, ..look }, s.t, far, fade);
                }
                let n = (s.t * 1.2) as u32;
                if n > s.ticks {
                    s.ticks = n;
                    sfx.write(Sfx3d::ui("heartbeat").pitch(0.7).gain(0.5));
                }
            }
        }
        st.s[k] = Some(s);
    }
    fx.end_scope();
}

/// A lit outline round a body, facing the camera: the one thing in colour in a stopped world.
#[allow(clippy::too_many_arguments)]
fn outline(fx: &mut FxParticles, cam: Vec3, chest: Vec3, size: f32, look: Look, t: f32, far: f32, fade: f32) {
    let n = (chest - cam).normalize_or(Vec3::NEG_Z);
    let side = Vec3::Y.cross(n).normalize_or(Vec3::X);
    let up = n.cross(side).normalize_or(Vec3::Y);
    let (rx, ry) = (0.6 * size, 1.15 * size);
    let br = 0.85 + 0.15 * (t * 7.0).sin();
    let mut prev = chest + up * ry;
    for i in 1..=24 {
        let a = i as f32 / 24.0 * TAU;
        let q = chest + side * a.sin() * rx + up * a.cos() * ry;
        fx.immediate_segment(prev, q, 0.045 * far, look.main, look.glow * 1.6 * br * fade, PFlags::PLAIN);
        prev = q;
    }
}

/// The grade: the stopped world drains toward the move's colour; the bubble swings the hue as it
/// passes; time resuming snaps colour back with a kick. Writes only while something is happening.
fn grade(
    time: Res<Time<Real>>,
    views: Res<WorldViews>,
    comfort: Res<ComfortSettings>,
    mut st: ResMut<TimeFx>,
    mut grey_res: ResMut<TimeStopGrey>,
    mut cams: Query<(&GlobalTransform, &mut ColorGrading), With<PlayerCamera>>,
) {
    let dt = time.delta_secs().min(0.1);
    let calm = comfort.flashes <= 0.01;
    let stop = st.s.iter().flatten().find(|s| s.style == Style::Stop).copied();
    let witch = st.s.iter().flatten().find(|s| s.style == Style::Witch).copied();
    // Players flagged without a slot yet (a frame early): still grey.
    let flagged = views.players.iter().any(|p| p.power.has(power_flags::TIME_STOP));
    let tg = if stop.is_some() || (flagged && witch.is_none()) { 1.0 } else { 0.0 };
    let tw = if witch.is_some() && stop.is_none() { 1.0 } else { 0.0 };
    let (ri, ro) = (7.0, 5.0);
    st.grey += (tg - st.grey) * (1.0 - (-(if tg > st.grey { ri } else { ro }) * dt).exp());
    st.witch += (tw - st.witch) * (1.0 - (-(if tw > st.witch { 8.0 } else { 4.0 }) * dt).exp());
    if let Some(s) = stop.or(witch) {
        st.tint = s.look.main;
    }
    grey_res.0 = st.grey.max(st.witch);
    let kick = if st.since_resume < 0.5 { (1.0 - st.since_resume / 0.5).powi(2) } else { 0.0 };
    let swing = if !calm && st.since_stop < DOME_OUT { (st.since_stop / DOME_OUT * PI).sin() } else { 0.0 };
    let busy = st.grey > 1e-3 || st.witch > 1e-3 || kick > 0.0 || swing > 0.0;
    if !busy {
        if st.active {
            st.active = false;
            for (_, mut cg) in &mut cams {
                reset(&mut cg);
            }
        }
        return;
    }
    st.active = true;
    let g = st.grey;
    let w = st.witch;
    let c = st.tint;
    // Warm / cool and magenta / green of the move's colour.
    let warm = (c[0] - c[2]).clamp(-1.0, 1.0);
    let mag = ((c[0] + c[2]) * 0.5 - c[1]).clamp(-1.0, 1.0);
    let depth = if calm { 0.6 } else { 0.9 };
    for (_, mut cg) in &mut cams {
        cg.global.post_saturation = 1.04 * (1.0 - depth * g) * (1.0 - 0.55 * w) + 0.45 * kick;
        cg.global.exposure = -0.22 * g - 0.3 * w + 0.12 * kick;
        cg.global.temperature = 0.25 * warm * g - 0.2 * w;
        cg.global.tint = 0.25 * mag * g + 0.45 * w;
        cg.global.hue = swing * PI * 0.9;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bubble_is_the_complement_of_the_move() {
        let gold = complement([1.0, 0.8, 0.2]);
        assert!(gold[2] > gold[0], "gold's bubble leans blue: {gold:?}");
        let violet = complement([0.6, 0.2, 1.0]);
        assert!(violet[1] > violet[2], "violet's bubble leans green: {violet:?}");
        for c in [gold, violet, complement([0.0; 3])] {
            assert!(c.iter().all(|x| x.is_finite() && *x >= 0.0 && *x <= 1.2));
        }
    }
}
