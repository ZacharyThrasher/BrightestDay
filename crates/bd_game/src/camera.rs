//! First-person camera.
//!
//! Final camera transform (PostUpdate, before transform propagation):
//! `translation = ViewAnchor.eye + shake offset`, `rotation = LookAngles (this frame's mouse) × shake`.
//! Whoever owns the local view (placeholder noclip player now, bd_sim's predicted player later)
//! writes the *interpolated* eye position and velocity to [`ViewAnchor`] every frame.
//!
//! FOV: [`crate::settings::Settings::fov`] is the horizontal FOV at the window's aspect ratio;
//! [`DynamicFov`] adds speed widening and spring punches (`juice::FovPunch`).

use crate::input::LookAngles;
use crate::juice::CameraShake;
use crate::settings::{ComfortSettings, Settings};
use crate::state::AppState;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::PrimaryWindow;

/// Marker for the one gameplay camera.
#[derive(Component)]
pub struct PlayerCamera;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraUpdateSet;

/// Which camera rig drives the gameplay camera. A third-person over-the-shoulder rig slots in as
/// another variant: it reads the same `ViewAnchor` + `LookAngles` and places the camera behind the
/// eye (the crosshair ray then starts at the camera; see docs/DESIGN.md "Motion comfort").
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub enum CameraRig {
    #[default]
    FirstPerson,
}

/// Where the local eye is this frame (already interpolated) and how fast it moves.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct ViewAnchor {
    pub eye: Vec3,
    pub velocity: Vec3,
}

/// Speed-based widening + punch spring, in horizontal degrees added to the base FOV.
#[derive(Component, Debug, Clone, Default)]
pub struct DynamicFov {
    /// Current smoothed speed widening.
    pub widen: f32,
    /// Punch spring displacement and velocity.
    pub punch: f32,
    pub punch_vel: f32,
    /// Total extra degrees applied last frame (read-only diagnostics).
    pub applied: f32,
}

impl DynamicFov {
    const STIFFNESS: f32 = 260.0;
    const DAMPING_RATIO: f32 = 0.55;

    /// Kick the FOV by roughly `degrees` at the peak (negative narrows).
    pub fn kick(&mut self, degrees: f32) {
        let omega = Self::STIFFNESS.sqrt();
        self.punch_vel += degrees * omega * 1.6;
    }

    fn step(&mut self, dt: f32, speed: f32, speed_fov: bool, enabled: bool) {
        // Widen above running speed; ramps like the old prototype but capped lower (aim stability).
        let target = if speed_fov { ((speed - 12.0) * 0.35).clamp(0.0, 14.0) } else { 0.0 };
        self.widen += (target - self.widen) * (1.0 - (-5.0 * dt).exp());
        let k = Self::STIFFNESS;
        let c = 2.0 * k.sqrt() * Self::DAMPING_RATIO;
        // Semi-implicit Euler with small substeps: stable at any frame rate.
        let steps = ((dt / 0.004).ceil() as u32).clamp(1, 16);
        let h = dt / steps as f32;
        for _ in 0..steps {
            self.punch_vel += (-k * self.punch - c * self.punch_vel) * h;
            self.punch += self.punch_vel * h;
        }
        if !enabled {
            self.punch = 0.0;
            self.punch_vel = 0.0;
        }
        self.applied = self.widen + self.punch;
    }
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewAnchor>()
            .init_resource::<CameraRig>()
            .add_systems(Startup, spawn_camera)
            .configure_sets(PostUpdate, CameraUpdateSet.before(TransformSystems::Propagate))
            .add_systems(
                PostUpdate,
                (
                    menu_orbit.run_if(in_state(AppState::Menu)),
                    update_player_camera.run_if(in_state(AppState::InGame)),
                )
                    .in_set(CameraUpdateSet),
            );
    }
}

fn spawn_camera(mut commands: Commands) {
    let near = 0.05;
    commands.spawn((
        Name::new("PlayerCamera"),
        PlayerCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 70f32.to_radians(),
            near,
            far: 2000.0,
            near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -near),
            ..default()
        }),
        DynamicFov::default(),
        Transform::from_xyz(0.0, 20.0, 40.0).looking_at(Vec3::ZERO, Vec3::Y),
        // Render-look components (Hdr, Bloom, Tonemapping, fog, AA) are added by `render`.
    ));
}

/// Horizontal FOV (degrees) → vertical FOV (radians) for a given aspect ratio.
pub fn hfov_to_vfov(hfov_deg: f32, aspect: f32) -> f32 {
    let h = hfov_deg.clamp(1.0, 170.0).to_radians();
    2.0 * ((h * 0.5).tan() / aspect.max(0.1)).atan()
}

fn window_aspect(window: &Window) -> f32 {
    let (w, h) = (window.physical_width().max(1), window.physical_height().max(1));
    w as f32 / h as f32
}

#[allow(clippy::too_many_arguments)]
fn update_player_camera(
    time: Res<Time<Real>>,
    anchor: Res<ViewAnchor>,
    look: Res<LookAngles>,
    shake: Res<CameraShake>,
    settings: Res<Settings>,
    comfort: Res<ComfortSettings>,
    rig: Res<CameraRig>,
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Single<(&mut Transform, &mut Projection, &mut DynamicFov), With<PlayerCamera>>,
) {
    let (mut tf, mut proj, mut dfov) = cam.into_inner();
    let dt = time.delta_secs().min(0.1);
    dfov.step(dt, anchor.velocity.length(), comfort.speed_fov, comfort.fov_kick > 0.0);

    // Rotation never rolls: yaw/pitch from this frame's mouse, shake adds only pitch/yaw.
    let base_rot = look.rotation();
    match *rig {
        CameraRig::FirstPerson => {
            tf.translation = anchor.eye + base_rot * shake.offset;
            tf.rotation = base_rot * shake.rotation;
        }
    }

    if let Projection::Perspective(p) = proj.as_mut() {
        let hfov = (settings.fov + dfov.applied).min(150.0);
        let v = hfov_to_vfov(hfov, window_aspect(&window));
        if (p.fov - v).abs() > 1e-5 {
            p.fov = v;
        }
    }
}

/// Title-screen backdrop: slow orbit around the arena.
fn menu_orbit(
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Single<(&mut Transform, &mut Projection), With<PlayerCamera>>,
) {
    let (mut tf, mut proj) = cam.into_inner();
    let t = time.elapsed_secs() * 0.035;
    let r = 58.0;
    let c = arena.map(|a| a.info.center).unwrap_or(Vec3::new(0.0, 6.0, 0.0));
    let eye = c + Vec3::new(t.cos() * r, 16.0 + (t * 1.7).sin() * 3.0, t.sin() * r);
    *tf = Transform::from_translation(eye).looking_at(c, Vec3::Y);
    if let Projection::Perspective(p) = proj.as_mut() {
        p.fov = hfov_to_vfov(settings.fov.min(100.0), window_aspect(&window));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hfov_conversion() {
        // 90° horizontal at 1:1 is 90° vertical.
        assert!((hfov_to_vfov(90.0, 1.0).to_degrees() - 90.0).abs() < 1e-3);
        // 100° horizontal at 16:9 is ~67.7° vertical.
        assert!((hfov_to_vfov(100.0, 16.0 / 9.0).to_degrees() - 67.7).abs() < 0.2);
    }

    #[test]
    fn punch_returns_to_rest() {
        let mut f = DynamicFov::default();
        f.kick(4.0);
        let mut peak: f32 = 0.0;
        for _ in 0..240 {
            f.step(1.0 / 144.0, 0.0, true, true);
            peak = peak.max(f.punch);
        }
        assert!(peak > 2.0 && peak < 8.0, "peak {peak}");
        assert!(f.punch.abs() < 0.05);
    }
}
