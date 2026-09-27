//! ULTIMATE FRAME (PRESENTATION POLISH). Owner: "make sure as many of them feel special as possible…
//! ultimate power fantasy"; the bar is Hollow Purple. Every ultimate / Titan-tier move (a move tagged
//! `Ultimate`, or one whose ops put it in the Titan+ presence tier: time stops, singularities, huge
//! explosions) gets a three-act frame ON TOP of its op beats (movefx / op_visuals):
//!
//! 1. **Gather** (0 … 0.14 s, inside the move's active phase): the world pours into the caster —
//!    motes rushing in, a converging ring, a charge whine.
//! 2. **Release** (0.14 s): a bloom at the caster, a camera kick for everyone near, and the ELEMENT'S
//!    SILHOUETTE — each family reads differently from across the map:
//!    * energy (default, hard light…): three great circles bursting out into a sphere;
//!    * lightning: bolts hammering down out of the sky round the caster, a jagged ring;
//!    * holy: a pillar of light from the sky, halos climbing it;
//!    * void: an implosion, a black core, a violet rim ring;
//!    * fire: a column of flame, a ring of fire racing out;
//!    * ice / crystal: a crown of shards jutting up round the caster, then shattering;
//!    * everything else (smoke, water, earth, wind…): a column of its body + a shock ring.
//! 3. **Aftermath** (to 1.8 s): a scorch ring on the ground cooling, the body's motes drifting,
//!    residual crackle / embers / falling light.
//!
//! Plus a SKY BEACON (a thin column up from the caster for 0.6 s) so the moment reads at 40 m.
//! Nothing covers the crosshair: your own gather / bloom is dropped at the lens (the pen's guard),
//! the beacon and rings are away from the aim. Cost: a fixed pool of [`MAX`] frames, GPU particles
//! + immediate segments in a budgeted scope; nothing allocated per frame.

use super::super::sound::Sfx3d;
use super::super::{PresentSet, corps_rgb};
use super::movefx::draw::{Look, Pen, basis, circle, mix3, zig};
use super::movefx::table::{Body, MoveFx};
use super::movefx::{MoveFxFired, resolve_look};
use crate::camera::{CameraUpdateSet, PlayerCamera};
use crate::juice::{CameraShakeEvent, FovPunch};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, WorldViews};
use crate::state::AppState;
use crate::viewmodel::{HandsPublishSet, ViewmodelHands};
use bd_core::PlayerId;
use bd_fx::{FxParticles, PFlags};
use bd_sim::powers::{MoveDef, MoveOp, MoveTag};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use std::f32::consts::TAU;

const MAX: usize = 16;
const DUR: f32 = 1.8;
const GATHER: f32 = 0.14;
const IMM_BUDGET: usize = 900;

/// The silhouette family of a style's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Energy,
    Lightning,
    Holy,
    Void,
    Fire,
    Ice,
    Body,
}

pub fn family(look: &Look) -> Family {
    match look.body {
        Body::Energy | Body::Rubber | Body::Rigid | Body::Glitch | Body::Paper => Family::Energy,
        Body::Bolt | Body::Streak => Family::Lightning,
        Body::Halo => Family::Holy,
        Body::Void => Family::Void,
        Body::Flame => Family::Fire,
        Body::Shard => Family::Ice,
        Body::Smoke | Body::Liquid | Body::Grain | Body::Gust => Family::Body,
    }
}

/// Does a move get the ultimate frame? Tagged `Ultimate`, or Titan+ presence by its ops. A move that
/// is ONLY casts is presented by the construct / signature pipeline (Hollow Purple already).
pub fn is_ultimate(mv: &MoveDef) -> bool {
    let only_casts = !mv.ops.is_empty() && mv.ops.iter().all(|o| matches!(o, MoveOp::Cast { .. }));
    if only_casts {
        return false;
    }
    mv.tags.contains(&MoveTag::Ultimate)
        || matches!(bd_sim::powers::chain::move_tier(mv), Some(t) if t >= bd_sim::presence_tier::Tier::Titan)
}

#[derive(Clone, Copy)]
struct Frame {
    player: PlayerId,
    look: Look,
    fam: Family,
    t: f32,
    at: Vec3,
    ground: Vec3,
    size: f32,
    seed: u32,
    released: bool,
    mine_fp: bool,
}

#[derive(Resource, Default)]
struct Frames {
    f: [Option<Frame>; MAX],
    seed: u32,
}

/// Perf readout.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct UltimateStats {
    pub live: usize,
    pub started: u64,
    pub step_us: f32,
}

pub struct UltimatePlugin;

impl Plugin for UltimatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Frames>()
            .init_resource::<UltimateStats>()
            .add_message::<MoveFxFired>()
            .add_message::<Sfx3d>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .add_systems(OnExit(AppState::InGame), |mut f: ResMut<Frames>| f.f = [None; MAX])
            .add_systems(Update, start.in_set(PresentSet::Sync))
            .add_systems(PostUpdate, step.after(CameraUpdateSet).after(HandsPublishSet).before(TransformSystems::Propagate));
    }
}

fn ground_below(active: &ActiveWorld, p: Vec3) -> Vec3 {
    bd_core::voxel::raycast(&active.world.voxels, super::super::gv(p + Vec3::Y * 0.3), bd_core::Vec3::NEG_Y, 6.0).map_or(p - Vec3::Y, |h| super::super::bv(h.point))
}

#[allow(clippy::too_many_arguments)]
fn start(
    mut fired: MessageReader<MoveFxFired>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    table: Res<MoveFx>,
    hands: Res<ViewmodelHands>,
    mut frames: ResMut<Frames>,
    mut stats: ResMut<UltimateStats>,
    mut fx: ResMut<FxParticles>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else {
        fired.clear();
        return;
    };
    let cam = cams.iter().next().map_or(Vec3::ZERO, |g| g.translation());
    for f in fired.read() {
        if !is_ultimate(f.mv) {
            continue;
        }
        let Some(v) = views.player(f.player) else { continue };
        let look = resolve_look(&table, f.mv, f.power, corps_rgb(v.corps));
        let mine_fp = Some(f.player) == active.local && hands.active;
        let at = f.to.unwrap_or(v.pos) + Vec3::Y * 1.05 * v.size;
        frames.seed = frames.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let fr = Frame { player: f.player, look, fam: family(&look), t: 0.0, at, ground: ground_below(&active, at), size: v.size, seed: frames.seed, released: false, mine_fp };
        let k = frames.f.iter().position(|x| x.is_none()).unwrap_or_else(|| {
            (0..MAX).max_by(|a, b| frames.f[*a].map_or(0.0, |x| x.t).total_cmp(&frames.f[*b].map_or(0.0, |x| x.t))).unwrap_or(0)
        });
        frames.f[k] = Some(fr);
        stats.started += 1;
        // The gather: the world pours in.
        let mut pen = Pen { fx: &mut fx, motes: table.t.beats.motes, scale: if mine_fp { 0.6 } else { 1.0 }, cam, avoid: mine_fp.then_some(at) };
        pen.implode(&look, at, 4.5 * v.size, GATHER, 2.6);
        if !mine_fp {
            pen.ring(&look, fr.ground + Vec3::Y * 0.1, Vec3::Y, 7.0, -40.0, 1.6, GATHER);
        }
        sfx.write(Sfx3d::at("mv_ult_gather", at).gain(0.9));
    }
}

#[allow(clippy::too_many_arguments)]
fn step(
    time: Res<Time<Real>>,
    views: Res<WorldViews>,
    comfort: Res<ComfortSettings>,
    table: Res<MoveFx>,
    mut frames: ResMut<Frames>,
    mut stats: ResMut<UltimateStats>,
    mut fx: ResMut<FxParticles>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    out: (MessageWriter<Sfx3d>, MessageWriter<CameraShakeEvent>, MessageWriter<FovPunch>),
) {
    let (mut sfx, mut shake, mut fov) = out;
    if frames.f.iter().all(|f| f.is_none()) {
        stats.live = 0;
        return;
    }
    let t0 = std::time::Instant::now();
    let dt = time.delta_secs().min(0.1);
    let cam = cams.iter().next().map_or(Vec3::ZERO, |g| g.translation());
    let calm = comfort.flashes <= 0.01;
    let mut left = IMM_BUDGET;
    let mut live = 0;
    for slot in frames.f.iter_mut() {
        let Some(mut f) = *slot else { continue };
        // Your own ultimate in first person: no lines round your own body (they would cross the
        // lens / the aim) — the bloom, motes, sound and camera kick carry it.
        if !f.mine_fp {
            left = fx.imm_left().min(left);
        }
        fx.scope(cam, 0.0024, if f.mine_fp { 0 } else { left });
        f.t += dt;
        if f.t >= DUR {
            *slot = None;
            continue;
        }
        live += 1;
        // Ride the caster during the gather (a dash ultimate gathers on the move).
        if !f.released
            && let Some(v) = views.player(f.player)
        {
            f.at = v.pos + Vec3::Y * 1.05 * v.size;
        }
        let look = f.look;
        let s = f.size;
        let far = (f.at.distance(cam) / 15.0).clamp(1.0, 2.4);
        let mut pen = Pen { fx: &mut fx, motes: table.t.beats.motes, scale: if f.mine_fp { 0.6 } else { 1.0 }, cam, avoid: f.mine_fp.then_some(f.at) };
        if !f.released && f.t >= GATHER {
            f.released = true;
            release(&mut pen, &f, far, calm);
            let d = f.at.distance(cam);
            if f.mine_fp {
                fov.write(FovPunch { degrees: 5.0 });
            }
            if d < 28.0 {
                shake.write(CameraShakeEvent { trauma: (0.32 - d * 0.008).clamp(0.08, 0.32), dir: Vec3::ZERO });
            }
            let (a, b) = match f.fam {
                Family::Lightning => ("storm_bolt", "mv_bolt_crack"),
                Family::Holy => ("mv_holy", "presence_shing"),
                Family::Void => ("mv_void_thump", "ko_accent_void"),
                Family::Fire => ("mv_fire_whoomp", "chain_fire_roar"),
                Family::Ice => ("mv_ice", "storm_icicle"),
                Family::Energy => ("ult_fire", "mv_hardlight"),
                Family::Body => ("mv_shove_wave", "ult_fire"),
            };
            sfx.write(Sfx3d::at(a, f.at).pitch(0.8).gain(1.0));
            sfx.write(Sfx3d::at(b, f.at).pitch(0.7).gain(0.8));
            sfx.write(Sfx3d::at("explosion_big", f.at).pitch(0.45).gain(0.55));
        }
        if !f.released {
            // Gather: a converging ring closing on the caster (not in your own lens).
            let u = (f.t / GATHER).min(1.0);
            if !f.mine_fp {
                circle(pen.fx, f.at, (f.at - cam).normalize_or(Vec3::Z), (3.2 * (1.0 - u) + 0.3) * s, 24, 0.05 * far, look.core, look.glow * (1.0 + 2.0 * u), f.t * 9.0);
            }
            *slot = Some(f);
            continue;
        }
        let tr = f.t - GATHER;
        let u = tr / (DUR - GATHER);
        // The sky beacon: reads at 40 m.
        if tr < 0.6 && !f.mine_fp {
            let k = 1.0 - tr / 0.6;
            pen.fx.immediate_segment(f.ground, f.ground + Vec3::Y * 28.0, (0.08 + 0.2 * k) * far, mix3(look.core, look.main, 0.4), look.glow * 2.2 * k, PFlags::PLAIN);
        }
        // Aftermath: the scorch ring cooling on the ground.
        let cool = (1.0 - u).powi(2);
        if cool > 0.05 {
            circle(pen.fx, f.ground + Vec3::Y * 0.05, Vec3::Y, 3.0 * s, 32, 0.12 * far * cool.sqrt(), mix3(look.main, look.core, 0.3 * cool), look.glow * (0.5 + 0.8 * cool), 0.0);
        }
        match f.fam {
            Family::Energy => {
                // Three great circles bursting out into a sphere, fading.
                if tr < 0.55 {
                    let k = tr / 0.55;
                    let r = (0.8 + 7.5 * (1.0 - (1.0 - k).powi(3))) * s;
                    let g = look.glow * 2.0 * (1.0 - k);
                    for (i, n) in [Vec3::Y, Vec3::new(1.0, 0.3, 0.0), Vec3::new(-0.3, 0.4, 1.0)].into_iter().enumerate() {
                        circle(pen.fx, f.at, n, r, 40, (0.1 - 0.07 * k) * far, if i == 0 { look.core } else { look.main }, g, tr * (2.0 + i as f32));
                    }
                }
                if pen.fx.rng.f() < 0.6 * (1.0 - u) {
                    let d = pen.fx.rng.dir();
                    pen.mote(&look, f.at + d * 2.0 * s, d * 1.5 + Vec3::Y, 0.6, 0.0);
                }
            }
            Family::Lightning => {
                // Bolts hammer the ground round the caster for 0.5 s, re-struck, then residual arcs.
                if tr < 0.5 {
                    let n = 4;
                    for k in 0..n as u32 {
                        let born = k as f32 * 0.08;
                        if tr < born || tr > born + 0.22 {
                            continue;
                        }
                        let a = (f.seed.wrapping_add(k * 977) % 628) as f32 * 0.01;
                        let r = (2.0 + (k % 2) as f32 * 2.5) * s;
                        let g = f.ground + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                        let fade = 1.0 - (tr - born) / 0.22;
                        let seed = f.seed ^ k.wrapping_mul(131) ^ ((tr * 40.0) as u32);
                        zig(pen.fx, g + Vec3::Y * 24.0, g, 12, 1.2, seed, (0.14 * fade + 0.04) * far, look.core, look.glow * 3.0 * fade);
                        zig(pen.fx, g + Vec3::Y * 24.0, g, 9, 2.0, seed ^ 0x5555, (0.06 * fade + 0.02) * far, look.main, look.glow * 2.0 * fade);
                        if tr - dt < born {
                            pen.flash(g + Vec3::Y * 0.5, 1.6, look.core, look.glow * 2.0, 0.14);
                            let sparks = Look { body: Body::Bolt, ..look };
                            pen.burst(&sparks, g + Vec3::Y * 0.2, 1.0, 9.0, Some(Vec3::Y), 1.0, 0.35);
                            sfx.write(Sfx3d::at("mv_bolt_crack", g).pitch(0.9 + 0.1 * k as f32).gain(0.7));
                        }
                    }
                    // A jagged ring racing out along the ground.
                    let rr = (1.0 + 16.0 * (tr / 0.5)) * s;
                    let segs = 28;
                    let mut prev = None;
                    for i in 0..=segs {
                        let a = i as f32 / segs as f32 * TAU;
                        let j = 1.0 + 0.08 * (((f.seed as f32 * 0.001 + i as f32 * 1.7 + tr * 30.0).sin()) * 2.0);
                        let q = f.ground + Vec3::new(a.cos(), 0.02, a.sin()) * rr * j + Vec3::Y * 0.1;
                        if let Some(p) = prev {
                            pen.fx.immediate_segment(p, q, 0.06 * far, look.core, look.glow * 2.0 * (1.0 - tr / 0.5), PFlags::PLAIN);
                        }
                        prev = Some(q);
                    }
                } else if pen.fx.rng.f() < 0.3 * (1.0 - u) {
                    let d = pen.fx.rng.dir();
                    let a = f.ground + Vec3::new(d.x, 0.1, d.z) * 2.5 * s;
                    let rs = pen.fx.rng.next_u32();
                    zig(pen.fx, a, a + Vec3::new(d.z, 0.4, -d.x) * 1.2, 4, 0.25, rs, 0.03 * far, look.core, look.glow * 2.0);
                }
            }
            Family::Holy => {
                // The pillar: from the sky onto the caster, thinning; halos climbing it.
                if tr < 0.9 {
                    let k = tr / 0.9;
                    let w = (1.0 - k).powf(0.7);
                    // A cage of light: rays round the caster (they stay visible inside it) and a
                    // core shaft from above their head into the sky.
                    let head = f.ground + Vec3::Y * 2.3 * s;
                    pen.fx.immediate_segment(head, f.ground + Vec3::Y * 30.0, (0.3 * s * w + 0.03) * far.min(1.6), look.core, look.glow * 2.6, PFlags::PLAIN);
                    let rr = (1.1 + 0.6 * k) * s;
                    for i in 0..6 {
                        let a = i as f32 / 6.0 * TAU + tr * 0.8;
                        let b = f.ground + Vec3::new(a.cos() * rr, 0.0, a.sin() * rr);
                        pen.fx.immediate_segment(b, b + Vec3::Y * 18.0, (0.09 * w + 0.02) * far.min(1.6), look.main, look.glow * 1.6, PFlags::PLAIN);
                    }
                    for i in 0..3 {
                        let h = ((k * 1.4 + i as f32 / 3.0).fract()) * 12.0;
                        circle(pen.fx, f.ground + Vec3::Y * h, Vec3::Y, (1.8 - 0.08 * h) * s, 28, 0.07 * far, look.core, look.glow * 2.0 * w, tr);
                    }
                }
                // Light falling like feathers.
                if pen.fx.rng.f() < 0.8 * (1.0 - u) {
                    let d = pen.fx.rng.dir();
                    let q = f.ground + Vec3::new(d.x * 3.0, 5.0 + d.y.abs() * 5.0, d.z * 3.0) * s;
                    pen.mote(&look, q, Vec3::new(d.z, -1.4, -d.x) * 0.8, 1.0, 0.0);
                }
            }
            Family::Void => {
                // A black core that hangs, a violet rim ring, space swirling in.
                if tr < 0.7 {
                    let k = tr / 0.7;
                    let r = (1.4 * (1.0 - k) + 0.2) * s;
                    let n = (f.at - cam).normalize_or(Vec3::Z);
                    for i in 0..4 {
                        circle(pen.fx, f.at, n, r * (0.4 + 0.2 * i as f32), 20, (0.25 - 0.04 * i as f32) * s, look.dark, 0.2, 0.0);
                    }
                    circle(pen.fx, f.at, n, r * 1.25, 28, 0.08 * far, look.main, look.glow * 2.4 * (1.0 - k), tr * 5.0);
                    let rr = (0.5 + 12.0 * k) * s;
                    circle(pen.fx, f.ground + Vec3::Y * 0.1, Vec3::Y, rr, 36, 0.09 * far, look.main, look.glow * 1.6 * (1.0 - k), -tr * 3.0);
                }
                if pen.fx.rng.f() < 0.7 * (1.0 - u) {
                    pen.implode(&look, f.at, 3.0 * s, 0.35, 0.08);
                }
            }
            Family::Fire => {
                // A column of flame, a ring of fire racing out, embers.
                if tr < 0.8 {
                    let k = tr / 0.8;
                    for _ in 0..3 {
                        let d = pen.fx.rng.dir();
                        let q = f.ground + Vec3::new(d.x, 0.2, d.z) * 0.8 * s;
                        let big = Look { size: look.size * 2.2, ..look };
                        pen.mote(&big, q, Vec3::Y * (10.0 * (1.0 - k) + 3.0) + Vec3::new(d.x, 0.0, d.z), 0.7, 0.0);
                    }
                    let rr = (0.5 + 10.0 * k) * s;
                    let (u1, w1) = basis(Vec3::Y);
                    for i in 0..10 {
                        let a = i as f32 / 10.0 * TAU + tr;
                        let q = f.ground + (u1 * a.cos() + w1 * a.sin()) * rr;
                        if pen.fx.rng.f() < 0.5 {
                            pen.mote(&look, q, Vec3::Y * 2.5, 0.4, 0.0);
                        }
                    }
                    circle(pen.fx, f.ground + Vec3::Y * 0.08, Vec3::Y, rr, 36, 0.1 * far, look.core, look.glow * 2.0 * (1.0 - k), 0.0);
                }
            }
            Family::Ice => {
                // A crown of shards juts up round the caster, holds, then shatters.
                let n = 10;
                let grow = (tr / 0.12).min(1.0);
                if tr < 1.0 {
                    for i in 0..n {
                        let a = i as f32 / n as f32 * TAU + (f.seed % 100) as f32 * 0.06;
                        let r = (2.2 + 0.5 * ((i * 7) % 3) as f32) * s;
                        let base = f.ground + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                        let h = (1.4 + 0.8 * ((i * 5) % 3) as f32) * s * grow;
                        let tip = base + Vec3::new(a.cos() * 0.5, 1.0, a.sin() * 0.5).normalize() * h;
                        let rot = Quat::from_rotation_arc(Vec3::Y, (tip - base).normalize_or(Vec3::Y));
                        pen.fx.immediate_box((base + tip) * 0.5, rot, Vec3::new(0.35 * s, h, 0.35 * s), mix3(look.core, look.main, 0.5), look.glow * 1.1, PFlags::PLAIN);
                    }
                } else if tr - dt < 1.0 {
                    let shard = Look { body: Body::Shard, ..look };
                    for i in 0..n {
                        let a = i as f32 / n as f32 * TAU;
                        let q = f.ground + Vec3::new(a.cos() * 2.4 * s, 1.0 * s, a.sin() * 2.4 * s);
                        pen.burst(&shard, q, 0.5, 5.0, None, 0.0, 0.5);
                    }
                    sfx.write(Sfx3d::at("mv_crystal", f.ground).pitch(1.1).gain(0.7));
                }
            }
            Family::Body => {
                if tr < 0.6 && pen.fx.rng.f() < 0.9 {
                    let d = pen.fx.rng.dir();
                    let q = f.ground + Vec3::new(d.x, 0.3, d.z) * 1.0 * s;
                    pen.mote(&look, q, Vec3::Y * 7.0 + Vec3::new(d.x, 0.0, d.z) * 2.0, 0.7, 0.0);
                }
            }
        }
        *slot = Some(f);
    }
    fx.end_scope();
    stats.live = live;
    stats.step_us = t0.elapsed().as_secs_f32() * 1e6;
}

/// The release beat (once): the bloom, the silhouette's opening burst, the shock ring.
fn release(pen: &mut Pen, f: &Frame, far: f32, calm: bool) {
    let look = f.look;
    let s = f.size;
    if !calm {
        pen.flash(f.at, (1.1 * s * far.min(1.8)).min(2.2), look.core, look.glow * 2.2, 0.12);
    }
    pen.ring(&look, f.ground + Vec3::Y * 0.12, Vec3::Y, 0.6, 22.0, 2.4, 0.55);
    pen.ring(&look, f.at, Vec3::Y, 0.4, 14.0, 1.4, 0.4);
    match f.fam {
        Family::Energy => pen.burst(&look, f.at, 2.2, 14.0, None, 0.0, 0.5),
        Family::Lightning => {
            let sparks = Look { body: Body::Bolt, ..look };
            pen.burst(&sparks, f.at, 2.4, 16.0, None, 0.0, 0.3);
        }
        Family::Holy => {
            let halo = Look { body: Body::Halo, ..look };
            pen.burst(&halo, f.at, 1.8, 6.0, Some(Vec3::Y), 0.7, 1.0);
        }
        Family::Void => {
            pen.implode(&look, f.at, 6.0 * s, 0.25, 2.4);
            pen.flash(f.at, 1.4 * s, look.dark, 0.3, 0.2);
        }
        Family::Fire => pen.burst(&look, f.at, 2.4, 9.0, Some(Vec3::Y), 1.2, 0.8),
        Family::Ice => {
            let shard = Look { body: Body::Shard, ..look };
            pen.burst(&shard, f.at, 2.0, 11.0, None, 0.0, 0.6);
        }
        Family::Body => {
            pen.burst(&look, f.at, 2.2, 10.0, None, 0.0, 0.7);
            pen.dust_wall(&look, f.ground, 2.0 * s, 3.0, 1.4);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> MoveFx {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/tuning/move_fx.ron");
        let text = std::fs::read_to_string(&path).expect("assets/tuning/move_fx.ron");
        MoveFx::new(super::super::movefx::table::parse(&text).expect("move_fx.ron parses"))
    }

    #[test]
    fn ultimate_frame_takes_ultimates_and_titans_but_not_pure_casts() {
        let t = bd_sim::powers::PowerTable::get();
        let (mut ults, mut casts) = (0, 0);
        for m in t.powers.iter().flat_map(|d| d.moves.iter()) {
            if m.ops.iter().any(|o| matches!(o, MoveOp::TimeStop { .. })) {
                assert!(is_ultimate(m), "a time stop ({}) is always an ultimate moment", m.name);
            }
            if m.tags.contains(&MoveTag::Ultimate) && m.ops.iter().all(|o| matches!(o, MoveOp::Cast { .. })) && !m.ops.is_empty() {
                assert!(!is_ultimate(m), "{}: a cast-only ultimate is the signature pipeline's", m.name);
                casts += 1;
            }
            ults += is_ultimate(m) as u32;
        }
        assert!(ults > 50, "only {ults} ultimate frames in the shipped data");
        let _ = casts;
    }

    #[test]
    fn element_silhouettes_differ() {
        let fx = shipped();
        let fam = |k: &str| {
            let d = fx.def(fx.find(k).expect(k));
            family(&Look { body: d.body, ..Look::default() })
        };
        assert_eq!(fam("lightning"), Family::Lightning);
        assert_eq!(fam("energy"), Family::Energy);
        assert_eq!(fam("holy"), Family::Holy);
        assert_eq!(fam("void"), Family::Void);
        assert_eq!(fam("fire"), Family::Fire);
        assert_eq!(fam("ice"), Family::Ice);
        assert_ne!(fam("lightning"), fam("energy"));
        assert_ne!(fam("holy"), fam("energy"));
    }
}
