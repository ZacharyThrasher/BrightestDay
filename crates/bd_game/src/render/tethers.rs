//! Tether ropes: thin emissive corps-coloured segments from the hand to the hook head, with a
//! slight sag when slack. Hook travel is visible (the head is interpolated between ticks).

use crate::camera::{CameraUpdateSet, ViewAnchor};
use crate::input::LookAngles;
use crate::player::{LocalPlayer, MoveTuning, PlayerSim};
use crate::tick::TickClock;
use crate::util::v3;
use bd_core::corps::Corps;
use bd_sim::movement::HookPhase;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

const SEGMENTS: usize = 16;
/// Rope thickness at the hand / at the hook (tapered so the near end doesn't cover the view).
const THICKNESS_HAND: f32 = 0.010;
const THICKNESS_HOOK: f32 = 0.040;
const HEAD_SIZE: f32 = 0.16;

#[derive(Component)]
struct RopeSegment {
    hand: usize,
    index: usize,
}

#[derive(Component)]
struct HookHead(usize);

pub struct TetherRenderPlugin;

impl Plugin for TetherRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ropes).add_systems(
            PostUpdate,
            update_ropes.after(CameraUpdateSet).after(crate::viewmodel::HandsPublishSet).before(TransformSystems::Propagate),
        );
    }
}

fn spawn_ropes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let c = Corps::Willpower.color();
    let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let rope = materials.add(StandardMaterial {
        base_color: Color::srgb(c[0], c[1], c[2]),
        emissive: LinearRgba::rgb(c[0] * 3.0, c[1] * 3.0, c[2] * 3.0),
        ..default()
    });
    let head = materials.add(StandardMaterial {
        base_color: Color::srgb(c[0], c[1], c[2]),
        emissive: LinearRgba::rgb(c[0] * 6.0 + 1.0, c[1] * 6.0 + 1.0, c[2] * 6.0 + 1.0),
        ..default()
    });
    for hand in 0..2 {
        for index in 0..SEGMENTS {
            commands.spawn((
                RopeSegment { hand, index },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(rope.clone()),
                Transform::default(),
                Visibility::Hidden,
                NotShadowCaster,
            ));
        }
        commands.spawn((HookHead(hand), Mesh3d(mesh.clone()), MeshMaterial3d(head.clone()), Transform::default(), Visibility::Hidden, NotShadowCaster));
    }
}

#[allow(clippy::type_complexity)]
fn update_ropes(
    clock: Res<TickClock>,
    anchor: Res<ViewAnchor>,
    look: Res<LookAngles>,
    tuning: Res<MoveTuning>,
    player: Query<&PlayerSim, With<LocalPlayer>>,
    hands: Res<crate::viewmodel::ViewmodelHands>,
    tp_hands: Res<crate::present::camera3p::ThirdPersonHands>,
    mut segs: Query<(&RopeSegment, &mut Transform, &mut Visibility), Without<HookHead>>,
    mut heads: Query<(&HookHead, &mut Transform, &mut Visibility), Without<RopeSegment>>,
) {
    let sim = player.single().ok();
    let p = &tuning.0;
    let rot = look.rotation();
    let right = rot * Vec3::X;
    let up = rot * Vec3::Y;
    let fwd = rot * Vec3::NEG_Z;
    let ho = v3(p.hand_offset);

    // Per hand: (start, end, sag) or None.
    let mut ropes: [Option<(Vec3, Vec3, f32)>; 2] = [None, None];
    if let Some(sim) = sim {
        for (i, rope) in ropes.iter_mut().enumerate() {
            let hook = sim.state.hooks[i];
            if hook.phase == HookPhase::Idle {
                continue;
            }
            let side = if i == 0 { -1.0 } else { 1.0 };
            // Start slightly below/forward of the eye so the rope enters the view from the hand.
            // First person: the rope leaves the animated glove (this frame's camera).
            // Third person (CAMERA): the rope leaves the body's hand.
            let start = if hands.active {
                hands.world[i]
            } else if tp_hands.active {
                tp_hands.world[i]
            } else { anchor.eye + right * (side * ho.x) + up * ho.y.min(-0.25) + fwd * ho.z.max(0.35) };
            let end = if hook.phase == HookPhase::Flying && sim.prev.hooks[i].phase == HookPhase::Flying {
                v3(sim.prev.hooks[i].pos).lerp(v3(hook.pos), clock.alpha)
            } else {
                v3(hook.pos)
            };
            let dist = start.distance(end);
            let sag = if hook.phase == HookPhase::Attached {
                let slack = (hook.rope_len * hook.rope_len - dist * dist).max(0.0).sqrt();
                (slack * 0.35).min(3.0)
            } else {
                dist * 0.015
            };
            *rope = Some((start, end, sag));
        }
    }

    for (seg, mut tf, mut vis) in &mut segs {
        let Some((start, end, sag)) = ropes[seg.hand] else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        let at = |t: f32| start.lerp(end, t) - Vec3::Y * (sag * 4.0 * t * (1.0 - t));
        let (t0, t1) = (seg.index as f32 / SEGMENTS as f32, (seg.index + 1) as f32 / SEGMENTS as f32);
        let (a, b) = (at(t0), at(t1));
        let d = b - a;
        let len = d.length();
        if len < 1e-4 {
            *vis = Visibility::Hidden;
            continue;
        }
        tf.translation = (a + b) * 0.5;
        tf.rotation = Quat::from_rotation_arc(Vec3::Z, d / len);
        let th = THICKNESS_HAND + (THICKNESS_HOOK - THICKNESS_HAND) * ((t0 + t1) * 0.5);
        tf.scale = Vec3::new(th, th, len + th);
        if *vis != Visibility::Visible {
            *vis = Visibility::Visible;
        }
    }
    for (head, mut tf, mut vis) in &mut heads {
        let Some((start, end, _)) = ropes[head.0] else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        tf.translation = end;
        tf.rotation = Quat::from_rotation_arc(Vec3::Z, (end - start).normalize_or(Vec3::Z));
        tf.scale = Vec3::splat(HEAD_SIZE);
        if *vis != Visibility::Visible {
            *vis = Visibility::Visible;
        }
    }
}
