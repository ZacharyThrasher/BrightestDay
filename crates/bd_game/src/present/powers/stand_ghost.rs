//! The STAND GHOST (PRESENTATION POLISH): a borrowed move of a power with a companion ("dio za
//! warudo", "star platinum ora") shows the Stand ITSELF stepping out behind you and throwing the
//! move — its real model (the same procedural build the held Stand uses: `bd_shapes::build_model`
//! of the companion recipe), translucent in the move's colour, sliding out past your shoulder, then
//! dissolving. Presentation only: nothing spawns in the sim.
//!
//! Models are built once per companion power on worker threads at startup (a few dozen stands at
//! most); until one is ready the move grammar's particle silhouette stands in.

use super::movefx::draw::Pen;
use crate::state::AppState;
use bd_fx::{ConstructEnd, ConstructVisual, FxParticles};
use bd_sim::powers::{PowerDef, PowerTable};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use std::sync::Arc;

/// A Stand steps out for a borrowed move (written by the move grammar).
#[derive(Message, Clone, Copy, Debug)]
pub struct StandGhostFx {
    pub power: &'static PowerDef,
    /// Where its feet go, which way it faces, where it lunges (world, metres).
    pub feet: Vec3,
    pub rot: Quat,
    pub fwd: Vec3,
    pub color: [f32; 3],
    pub glow: f32,
    /// The owner's body scale.
    pub size: f32,
}

impl StandGhostFx {
    /// Where a Stand steps out for a player: behind the shoulder for everyone else; for your own
    /// first-person view out to the RIGHT and ahead (you see it throw the move; never on the aim).
    pub fn spot(feet: Vec3, body: Quat, size: f32, own_fp: bool) -> Vec3 {
        let o = if own_fp { Vec3::new(3.4, 0.0, -2.2) } else { Vec3::new(0.7, 0.25, 0.6) };
        feet + body * (o * size)
    }
}

/// The Stand a power calls: its `companion`, else (a user's kit: Jotaro, Dio…) the stand its rush
/// summons (`Rush { stand }`).
pub fn stand_recipe(def: &PowerDef) -> Option<&bd_sim::powers::RecipeDef> {
    def.companion.as_ref().map(|c| &c.recipe).or_else(|| {
        def.moves.iter().flat_map(|m| m.ops.iter()).find_map(|o| match o {
            bd_sim::powers::MoveOp::Rush { stand: Some(r), .. } => Some(r),
            _ => None,
        })
    })
}

/// Does a borrowed move of `def` get the ghost? A Stand power, and the move doesn't already summon
/// the real Stand (a rush steps it out in the sim).
pub fn wants_ghost(def: &PowerDef, mv: &bd_sim::powers::MoveDef) -> bool {
    stand_recipe(def).is_some() && !mv.ops.iter().any(|o| matches!(o, bd_sim::powers::MoveOp::Rush { .. }))
}

#[derive(Resource, Default)]
struct StandModels {
    ready: HashMap<&'static str, Arc<bd_shapes::VoxelModel>>,
    pending: HashMap<&'static str, Task<bd_shapes::VoxelModel>>,
}

/// A ghost gliding out (seconds, velocity).
#[derive(Component)]
struct StepOut(f32, Vec3);

pub struct StandGhostPlugin;

impl Plugin for StandGhostPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StandModels>()
            .add_message::<StandGhostFx>()
            .add_systems(OnEnter(AppState::InGame), prewarm)
            .add_systems(Update, (finish, spawn, step_out).chain().run_if(in_state(AppState::InGame)));
    }
}

/// Every companion's model, built off the frame once.
fn prewarm(mut models: ResMut<StandModels>) {
    let table = PowerTable::get();
    let pool = AsyncComputeTaskPool::get();
    for def in table.powers.iter() {
        let Some(recipe) = stand_recipe(def) else { continue };
        let key: &'static str = def.id.as_str();
        if models.ready.contains_key(key) || models.pending.contains_key(key) {
            continue;
        }
        let mut r = recipe.to_recipe();
        r.temperament = bd_core::mechanics::Temperament::Protective;
        r.intensity = r.intensity.max(0.7);
        models.pending.insert(key, pool.spawn(async move { bd_shapes::build_model(&r).1 }));
    }
}

fn finish(mut models: ResMut<StandModels>) {
    if models.pending.is_empty() {
        return;
    }
    let mut done: [Option<(&'static str, bd_shapes::VoxelModel)>; 8] = Default::default();
    let mut n = 0;
    for (k, t) in models.pending.iter_mut() {
        if n < done.len()
            && let Some(m) = block_on(poll_once(t))
        {
            done[n] = Some((*k, m));
            n += 1;
        }
    }
    for (k, m) in done.into_iter().flatten() {
        models.pending.remove(k);
        models.ready.insert(k, Arc::new(m));
    }
}

fn spawn(mut commands: Commands, mut ev: MessageReader<StandGhostFx>, models: Res<StandModels>, mut fx: ResMut<FxParticles>) {
    for g in ev.read() {
        let Some(model) = models.ready.get(g.power.id.as_str()) else {
            // Not built yet: the particle silhouette.
            let mut pen = Pen { fx: &mut fx, motes: 1.0, scale: 1.0, cam: Vec3::splat(1e6), avoid: None };
            let yaw = g.rot.to_euler(EulerRot::YXZ).0;
            pen.ghost(g.feet, yaw, g.size * 1.15, g.color, g.glow * 0.7, 0.6);
            continue;
        };
        let mut vis = ConstructVisual::new(model.clone(), g.color);
        // Ghostly: translucent in the move's colour, a brief glint as it appears (not a white block).
        vis.opacity = 0.34;
        vis.flash = 0.25;
        let t = Transform::from_translation(g.feet).with_rotation(g.rot).with_scale(Vec3::splat(g.size.clamp(0.5, 2.0)));
        commands.spawn((Name::new("StandGhost"), vis, t, StepOut(0.0, g.fwd * 2.5), ConstructEnd::Dismiss { duration: 0.75 }));
    }
}

/// It steps out past your shoulder (fast, then settles) while it dissolves.
fn step_out(time: Res<Time<Real>>, mut q: Query<(&mut StepOut, &mut Transform, &mut ConstructVisual)>) {
    let dt = time.delta_secs().min(0.1);
    for (mut s, mut t, mut vis) in &mut q {
        s.0 += dt;
        let k = (-s.0 * 6.0).exp();
        t.translation += s.1 * k * dt;
        vis.flash = (vis.flash - dt * 2.0).max(0.0);
    }
}
