//! Air jump + dodge presentation (sim: `bd_sim::movement` air_jump.rs / dodge.rs).
//!
//! * **Air jump** — a hard-light *step* under the feet: a corps-coloured tile of light chips, a ring of light
//!   chips racing outward on the step's plane, sparks spilling down (bd_fx particles). A small
//!   push-off camera kick for the local player (an FOV punch; the knee dip lives in `player.rs`),
//!   both comfort-gated. SFX: the `jump` cue pitched up (audio worker: a dedicated `air_jump` cue).
//! * **Dodge** — a hard-light streak along the path (a thinning beam) plus three fading afterimage
//!   silhouettes, corps-coloured; a small FOV pulse (comfort-gated). SFX: `boost_burst` pitched up
//!   (audio worker: a dedicated `dodge` whoosh).
//! * **Wall refresh** — a small corps-coloured scuff (a few chips + shards kicked off the surface)
//!   and a quiet high tick (`land_light` pitched up; audio worker: `wall_refresh`).
//! * Remote players: from the replicated `GameEvent::Move` flags (`AIR_JUMP` / `DODGE`) at the
//!   interpolated view position; the local player: from this frame's (predicted) `MoveEvents`.
//! * `--scenario airjump` (+ `--record`): two players hop / air-jump / dodge in front of the
//!   camera for clips (see [`scenario`]).

use super::sound::Sfx3d;
use super::{PresentSet, bv, corps_rgb};
use crate::juice::FovPunch;
use crate::player::{FrameMoveEvents, LocalPlayer, MoveTuning, PlayerSim};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use bd_fx::{BurstFx, BurstKind, Emit, FxParticles, PFlags, PKind};
use bd_sim::GameEvent;
use bd_sim::events::move_flags;
use bevy::prelude::*;

pub struct AirMovesPlugin;

impl Plugin for AirMovesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, air_move_fx.in_set(PresentSet::Events));
        scenario::build(app);
    }
}

/// Tiny deterministic jitter (no shared RNG state touched).
struct Jit(u32);

impl Jit {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[allow(clippy::too_many_arguments)]
fn air_move_fx(
    mut events: MessageReader<GameEvents>,
    frame: Res<FrameMoveEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    local_q: Query<&PlayerSim, With<LocalPlayer>>,
    comfort: Res<ComfortSettings>,
    tuning: Res<MoveTuning>,
    mut fx: Option<ResMut<FxParticles>>,
    mut burst: MessageWriter<BurstFx>,
    mut sfx: MessageWriter<Sfx3d>,
    mut fov: MessageWriter<FovPunch>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let flashes = comfort.flashes.clamp(0.0, 1.0);
    let (dist, secs) = (tuning.0.dodge_distance, tuning.0.dodge_time.max(1e-3));
    // Remote players (the local one is played from its own predicted events below).
    for ev in events.read() {
        let GameEvent::Move { player, flags, .. } = &ev.event else { continue };
        if active.local == Some(*player) || flags & (move_flags::AIR_JUMP | move_flags::DODGE | move_flags::WALL_REFRESH) == 0 {
            continue;
        }
        let Some(v) = views.player(*player) else { continue };
        let color = corps_rgb(v.corps);
        let seed = ev.tick as u32 ^ (player.0 as u32).wrapping_mul(0x9E37_79B9);
        if flags & move_flags::AIR_JUMP != 0 {
            step_fx(fx.as_deref_mut(), &mut burst, v.pos, color, v.size, flashes, seed);
            sfx.write(Sfx3d::at("air_jump", v.pos).gain(0.8));
        }
        if flags & move_flags::WALL_REFRESH != 0 {
            // No normal on the wire: a scuff at the body.
            scuff_fx(fx.as_deref_mut(), &mut burst, v.pos + Vec3::Y * 0.9 * v.size, Vec3::ZERO, color, seed);
            sfx.write(Sfx3d::at("land_light", v.pos + Vec3::Y).gain(0.22).pitch(1.6));
        }
        if flags & move_flags::DODGE != 0 {
            let h = Vec3::new(v.vel.x, 0.0, v.vel.z);
            let path = h * secs;
            streak_fx(fx.as_deref_mut(), v.pos, path, color, v.size, seed);
            sfx.write(Sfx3d::at("dodge", v.pos + Vec3::Y).gain(0.8));
        }
    }
    // The local player.
    let Ok(sim) = local_q.single() else { return };
    let Some(v) = active.local.and_then(|id| views.player(id)) else { return };
    let color = corps_rgb(v.corps);
    for (k, ev) in frame.0.iter().enumerate() {
        let seed = views.tick as u32 ^ (k as u32).wrapping_mul(0x85EB_CA6B) ^ 0x5bd1;
        if ev.air_jumped {
            let pos = bv(sim.state.pos);
            step_fx(fx.as_deref_mut(), &mut burst, pos, color, v.size, flashes, seed);
            sfx.write(Sfx3d::at("air_jump", pos));
            fov.write(FovPunch { degrees: 0.9 });
        }
        if let Some(n) = ev.wall_refresh {
            let n = bv(n);
            let at = bv(sim.state.pos) + Vec3::Y * 0.9 * v.size - n * 0.42 * v.size;
            scuff_fx(fx.as_deref_mut(), &mut burst, at, n, color, seed);
            sfx.write(Sfx3d::at("land_light", at).gain(0.25).pitch(1.6));
        }
        if ev.dodged {
            let x = sim.state.extra;
            let path = bv(x.dodge_base) * secs + bv(x.dodge_dir) * dist;
            // Start from where the burst began (this frame's state is already one tick in).
            let pos = bv(sim.state.pos) - bv(x.dodge_base + x.dodge_dir * (dist / secs)) * bd_core::DT;
            streak_fx(fx.as_deref_mut(), pos, path, color, v.size, seed);
            sfx.write(Sfx3d::at("dodge", pos + Vec3::Y));
            fov.write(FovPunch { degrees: 1.6 });
        }
    }
}

/// The hard-light step: a glowing tile of chips + outward ring on the step plane + sparks spilling down.
fn step_fx(
    fx: Option<&mut FxParticles>,
    burst: &mut MessageWriter<BurstFx>,
    feet: Vec3,
    color: [f32; 3],
    size: f32,
    flashes: f32,
    seed: u32,
) {
    let s = size.clamp(0.3, 3.0);
    let at = feet + Vec3::Y * 0.03;
    burst.write(BurstFx { kind: BurstKind::Shards, pos: at, dir: Vec3::NEG_Y, scale: 0.55 * s, color: Some(color) });
    let Some(fx) = fx else { return };
    let mut j = Jit(seed | 1);
    // Ring: chips on the step's plane racing out and settling (drag) — a hex-ish platform edge.
    let n = 20;
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU + j.f() * 0.12;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        fx.emit(Emit {
            pos: at + d * 0.22 * s,
            vel: d * (6.5 + j.f()) * s,
            size: 0.085 * s,
            color,
            emissive: 4.0,
            life: 0.26 + 0.06 * j.f(),
            gravity: 0.0,
            drag: 9.0,
            spin: 0.0,
            seed: j.0,
            flags: PFlags::PLAIN | PFlags::WHITE_BIRTH,
            ..default()
        });
    }
    // Inner plate: a few chips that stay put and fade (the "platform" you stepped on).
    for i in 0..7 {
        let a = i as f32 / 7.0 * std::f32::consts::TAU;
        let r = if i == 0 { 0.0 } else { 0.28 * s };
        fx.emit(Emit {
            pos: at + Vec3::new(a.cos() * r, 0.0, a.sin() * r),
            size: 0.2 * s,
            color,
            // Brighter with flashes on (comfort: a calm glow).
            emissive: 1.5 + 2.5 * flashes,
            life: 0.18,
            gravity: 0.0,
            seed: j.0 ^ i,
            flags: PFlags::PLAIN,
            ..default()
        });
    }
}

/// Wall refresh: a small hard-light scuff where the body touched the wall (subtle: a few chips
/// kicked off the surface and a faint spark).
fn scuff_fx(fx: Option<&mut FxParticles>, burst: &mut MessageWriter<BurstFx>, at: Vec3, n: Vec3, color: [f32; 3], seed: u32) {
    burst.write(BurstFx { kind: BurstKind::Shards, pos: at, dir: n, scale: 0.25, color: Some(color) });
    let Some(fx) = fx else { return };
    let mut j = Jit(seed | 5);
    for i in 0..5u32 {
        let side = Vec3::new(j.f() - 0.5, j.f() - 0.2, j.f() - 0.5);
        fx.emit(Emit {
            pos: at,
            vel: n * 2.5 + side * 2.0,
            size: 0.06,
            color,
            emissive: 3.0,
            life: 0.2,
            gravity: 0.6,
            drag: 4.0,
            seed: j.0 ^ i,
            flags: PFlags::PLAIN | PFlags::WHITE_BIRTH,
            ..default()
        });
    }
}

/// The dodge streak: a thinning beam along the path + three fading afterimage silhouettes.
fn streak_fx(fx: Option<&mut FxParticles>, start: Vec3, path: Vec3, color: [f32; 3], size: f32, seed: u32) {
    let Some(fx) = fx else { return };
    let l2 = path.length_squared();
    if l2.is_nan() || l2 <= 1e-4 {
        return;
    }
    let s = size.clamp(0.3, 3.0);
    let mut j = Jit(seed | 3);
    let chest = Vec3::Y * 1.0 * s;
    fx.emit(Emit {
        kind: PKind::Beam,
        flags: PFlags::PLAIN,
        pos: start + chest + path * 0.5,
        vel: path,
        size: 0.16 * s,
        color,
        emissive: 3.5,
        life: 0.22,
        ..default()
    });
    // Afterimages: a column of cubes (legs, torso, head) at three points along the path, older =
    // dimmer and shorter-lived.
    for k in 0..3 {
        let f = k as f32 / 3.0;
        let base = start + path * f;
        let life = 0.12 + 0.08 * f;
        for (h, sz) in [(0.45f32, 0.24f32), (1.05, 0.3), (1.55, 0.2)] {
            fx.emit(Emit {
                pos: base + Vec3::Y * h * s,
                vel: path.normalize_or_zero() * 0.6,
                size: sz * s,
                color,
                emissive: 1.2 + 1.3 * f,
                life,
                gravity: 0.0,
                drag: 2.0,
                seed: j.0 ^ k,
                flags: PFlags::PLAIN,
                delay: 0.02 * k as f32,
                ..default()
            });
            j.f();
        }
    }
}

/// `--scenario airjump`: two players in front of the camera — one hops and air-jumps (steering)
/// every couple of seconds, the other dodges left / right — for `--record` clips / screenshots.
pub mod scenario {
    use super::*;
    use crate::input::LookAngles;
    use crate::tick::{SimSet, SimTick};
    use bd_core::PlayerId;
    use bd_core::input::{PlayerInput, buttons};

    #[derive(Resource, Clone, Debug, Default)]
    pub struct AirJumpScenario {
        placed: bool,
        yaw: f32,
        t: f32,
        start_tick: u64,
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "airjump") {
            return;
        }
        info!("air jump scenario");
        app.insert_resource(AirJumpScenario::default())
            .add_systems(Update, place.run_if(resource_exists::<ActiveWorld>))
            .add_systems(SimTick, drive.in_set(SimSet::Input).run_if(resource_exists::<ActiveWorld>));
    }

    fn ground_at(w: &bd_sim::World, x: f32, z: f32, near_y: f32) -> Option<bd_core::Vec3> {
        let from = bd_core::Vec3::new(x, near_y + 20.0, z);
        let hit = bd_core::voxel::raycast(&w.voxels, from, bd_core::Vec3::NEG_Y, 60.0)?;
        ((hit.point.y - near_y).abs() < 6.0).then_some(bd_core::Vec3::new(x, hit.point.y + 0.05, z))
    }

    fn place(mut sc: ResMut<AirJumpScenario>, time: Res<Time<Real>>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>) {
        sc.t += time.delta_secs();
        if sc.placed || sc.t < 0.5 {
            return;
        }
        let Some(me) = active.local_player().filter(|p| p.alive).map(|p| p.mv.pos) else { return };
        let mut spots = Vec::new();
        let mut chosen = None;
        for step in 0..24 {
            let yaw = step as f32 * std::f32::consts::TAU / 24.0;
            let dir = bd_core::input::look_dir(yaw, 0.0);
            let side = bd_core::Vec3::new(-dir.z, 0.0, dir.x);
            spots.clear();
            // Flat ground for both actors and their whole moves.
            let ok = [(1u8, 5.5f32, -1.6f32), (2, 7.0, 1.8)].iter().all(|&(id, d, lat)| {
                let p = me + dir * d + side * lat;
                let probe =
                    [-4.5f32, 0.0, 4.5].iter().all(|o| ground_at(&active.world, p.x + side.x * o, p.z + side.z * o, me.y).is_some());
                match ground_at(&active.world, p.x, p.z, me.y).filter(|_| probe) {
                    Some(g) => {
                        spots.push((id, g));
                        true
                    }
                    None => false,
                }
            });
            if ok {
                chosen = Some(yaw);
                break;
            }
        }
        let Some(yaw) = chosen else {
            if sc.t > 6.0 {
                warn!("air jump scenario: no open ground found around the spawn");
                sc.placed = true;
            }
            return;
        };
        sc.yaw = yaw;
        for (id, g) in &spots {
            if let Some(p) = active.world.player_mut(PlayerId(*id)) {
                p.mv.pos = *g;
                p.mv.vel = bd_core::Vec3::ZERO;
                p.spawn_shield = 0.0;
            }
        }
        active.bots.clear();
        look.yaw = yaw;
        look.pitch = -0.12;
        sc.start_tick = active.world.tick;
        sc.placed = true;
        info!("air jump scenario: placed {} players, yaw {:.0}°", spots.len(), yaw.to_degrees());
    }

    fn drive(sc: Res<AirJumpScenario>, mut active: ResMut<ActiveWorld>) {
        if !sc.placed {
            return;
        }
        let tick = active.world.tick;
        let k = tick.saturating_sub(sc.start_tick) % 240;
        // Face across the view (profile to the camera).
        let yaw = sc.yaw + std::f32::consts::FRAC_PI_2;
        // 1: ground jump at 0, air jump at 0.4 s steering, lands, repeats every 2 s.
        if active.world.player(PlayerId(1)).is_some() {
            let b = if k == 10 || k == 58 || k == 130 || k == 180 { buttons::JUMP } else { 0 };
            let mx = if (58..100).contains(&k) {
                1
            } else if (180..220).contains(&k) {
                -1
            } else {
                0
            };
            active.remote_inputs.push((PlayerId(1), PlayerInput { seq: tick as u32, yaw, move_x: mx, buttons: b, ..Default::default() }));
        }
        // 2: dodge right, then left (cooldown 1.1 s), a hop + air dodge now and then.
        if active.world.player(PlayerId(2)).is_some() {
            let (b, mx) = match k {
                30 => (buttons::DODGE, 1),
                170 => (buttons::DODGE, -1),
                _ => (0, 0),
            };
            active
                .remote_inputs
                .push((PlayerId(2), PlayerInput { seq: tick as u32, yaw: sc.yaw, move_x: mx, buttons: b, ..Default::default() }));
        }
    }
}
