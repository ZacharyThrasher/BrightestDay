//! First-person viewmodel: the costume's gloves + the lantern ring, and the diegetic HUD.
//!
//! * **Hands** come from `bd_shapes::costume_first_person_hands(costume)` (your costume's gloves),
//!   meshed once per costume/corps change and parented to the gameplay camera. The ring on the right
//!   hand is a small emissive torus + a point light that lights the glove.
//! * **Willpower = ring glow**: brightness follows willpower; below ~20 % it dims and flickers; a cast
//!   you can't afford still happens, scaled down (never refused) — the ring sputters with it (the
//!   diegetic "that was all you had"). Casting flares it on the key press
//!   ([`RingCue::Forming`]), the oath brightens it letter by letter, voice makes it breathe.
//! * **Damage % (Smash model, COMBAT.md §1a) = ring colour + screen-edge light fractures**: the ring
//!   shifts white/corps → gold → orange → red as % rises, and thin cracks of light creep in from the
//!   screen edges (subtle; scaled by the comfort "screen flashes" setting). An optional small numeric
//!   % (setting, off by default) sits at the bottom.
//! * **Held constructs**: `HeldFx` events for the local player drive `bd_shapes::swing_curve(form,
//!   kind)` in the hand frame; the resulting world-space grip is published as [`FirstPersonHeld`]
//!   (SIM places the construct's visual there in first person).
//! * **Fire kick**: `ShotFired` by the local player kicks the viewmodel (pitch + push-back, critically
//!   damped spring, ~150 ms). The camera / aim never moves.
//! * **Keys**: power (G tap = move 0, hold = move 1), transform (Mouse3), dismiss (X), eject (E
//!   while seated in a cockpit).

use crate::camera::PlayerCamera;
use crate::console::{corps_color, RingCue, RingLexicon};
use crate::input::{LookAngles, UiFocus};
use crate::lobby::prefs::{Action, KeyBinds, UiPrefs};
use crate::lobby::UiSet;
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, Cmd, GameEvents, SimCommand};
use crate::state::{AppState, InGameMenu};
use bd_core::mechanics::{Form, FormCategory};
use bd_core::input::buttons;
use bd_core::ConstructId;
use bd_shapes::{PartMesh, SwingCurve, SwingKind};
use bd_sim::events::HeldFx;
use bd_sim::GameEvent;
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::picking::Pickable;
use bevy::prelude::*;

/// Procedural hand animation (sway, bob, tethers, ring, recoil, …): see the module docs.
#[path = "viewmodel_anim.rs"]
mod anim;
pub use anim::{HandsPublishSet, ViewmodelHands};

/// Where the local held construct is drawn in first person (SIM reads this).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct FirstPersonHeld {
    pub construct: Option<ConstructId>,
    /// World-space grip transform (grip at origin, item points −Z).
    pub grip: Transform,
    /// The same grip in camera space (`viewmodel_anim` re-places the item from this frame's camera).
    pub grip_local: Transform,
    /// First person + a held construct: SIM should place the visual at `grip`.
    pub active: bool,
}

/// Damped spring on a scalar.
#[derive(Clone, Copy, Debug, Default)]
struct Spring {
    x: f32,
    v: f32,
}

impl Spring {
    fn step(&mut self, dt: f32, omega: f32, zeta: f32) {
        let steps = ((dt / 0.004).ceil() as u32).clamp(1, 12);
        let h = dt / steps as f32;
        for _ in 0..steps {
            self.v += (-omega * omega * self.x - 2.0 * zeta * omega * self.v) * h;
            self.x += self.v * h;
        }
    }
}

struct ActiveSwing {
    curve: SwingCurve,
    t: f32,
    /// Holding a charge / guard: freeze at `hold_at`.
    holding: bool,
    /// The input bit that holds it (PRIMARY charge, SECONDARY guard): released locally → play on.
    button: u16,
}

#[derive(Resource, Default)]
struct VmState {
    /// Ring flare 0..1 (casting), decays.
    flare: f32,
    /// Sputter timer (couldn't afford).
    sputter: f32,
    /// Rendered glow (smoothed).
    glow: f32,
    kick_pitch: Spring,
    kick_back: Spring,
    punch: Spring,
    swing: Option<ActiveSwing>,
    held: Option<(ConstructId, Form)>,
    power_down: Option<f32>,
    power_long_sent: bool,
    /// Seconds since the last G tap (a second tap inside the window cycles the active power).
    power_since_tap: f32,
    /// Costume/corps the hands were built for.
    built_for: Option<(String, u32, u32, u32, u8)>,
    time: f32,
    ring_key: [f32; 3],
    /// Fingertip offsets from the elbow (unscaled arm space), from the glove mesh.
    tip: [Vec3; 2],
    /// Glove (hand block) bounds in unscaled arm space, min/max (right, left).
    glove: [(Vec3, Vec3); 2],
    /// `bd_shapes::weight_of` of the held construct (1 = nominal).
    held_w: f32,
    /// `time` when a charge was released locally (its heavy already plays; the host's heavy
    /// `SwingStart`, ~RTT later on a client, must not restart it).
    released_at: Option<f32>,
}

#[derive(Component)]
struct VmRoot;
#[derive(Component)]
struct VmArm {
    right: bool,
}
#[derive(Component)]
struct VmArmMesh;
/// Curled fingers / thumb blocks around a held grip (shown only while holding / supporting).
#[derive(Component)]
struct VmFinger {
    right: bool,
    thumb: bool,
}
#[derive(Component)]
struct VmRing;
#[derive(Component)]
struct VmRingLight;
#[derive(Component)]
struct Fracture {
    threshold: f32,
}
#[derive(Component)]
struct FractureRoot;
#[derive(Component)]
struct PercentText;

#[derive(Resource)]
struct VmAssets {
    arm_mat: Handle<StandardMaterial>,
    ring_mat: Handle<StandardMaterial>,
    finger_mat: Handle<StandardMaterial>,
}

/// Elbow positions in camera space (right, up, back).
const ELBOW_R: Vec3 = Vec3::new(0.24, -0.27, -0.06);
/// Where the fists sit in camera space at rest (the arm is placed so its fingertip lands here).
const FIST_R: Vec3 = Vec3::new(0.16, -0.135, -0.34);
const FIST_L: Vec3 = Vec3::new(-0.2, -0.17, -0.33);
/// First-person held items are drawn smaller (a viewmodel convention; the world model is unchanged).
/// Huge items are clamped further by screen coverage where the visual is placed
/// (`present::constructs::clamp_first_person`), so a giant hammer never covers the view.
const HELD_VIEW_SCALE: f32 = 0.24;
/// Gloves are drawn well under the body's arm size so they read as hands and frame the view (the
/// cross-section shrinks more than the length: slimmer fists, same reach).
const ARM_SCALE: Vec3 = Vec3::new(0.31, 0.31, 0.37);
const ELBOW_L: Vec3 = Vec3::new(-0.27, -0.31, -0.03);

pub struct ViewmodelPlugin;

impl Plugin for ViewmodelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FirstPersonHeld>()
            .init_resource::<VmState>()
            .add_systems(Startup, (setup_assets, spawn_fractures))
            .add_systems(Update, vm_keys.in_set(UiSet::Input))
            .add_plugins(anim::HandAnimPlugin)
            // Before `sync_constructs` reads `FirstPersonHeld` (same-frame grip).
            .add_systems(
                Update,
                (attach_viewmodel, rebuild_hands, vm_events, anim::drive_hand_anim, animate_viewmodel, draw_fractures, draw_percent)
                    .chain()
                    .in_set(UiSet::Draw)
                    .before(crate::present::PresentSet::Sync),
            );
    }
}

fn setup_assets(mut commands: Commands, mut mats: ResMut<Assets<StandardMaterial>>) {
    let arm_mat = mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.75, reflectance: 0.2, ..default() });
    let ring_mat = mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(2.0, 8.0, 3.0), unlit: true, ..default() });
    let finger_mat = mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.75, reflectance: 0.2, ..default() });
    commands.insert_resource(VmAssets { arm_mat, ring_mat, finger_mat });
}

fn local_corps(active: Option<&ActiveWorld>, prefs: &UiPrefs) -> bd_core::corps::Corps {
    active.and_then(|a| a.local_player().map(|p| p.corps)).unwrap_or(prefs.corps)
}

/// Spawn the viewmodel root under the gameplay camera (once the camera exists).
fn attach_viewmodel(mut commands: Commands, cams: Query<Entity, With<PlayerCamera>>, roots: Query<(), With<VmRoot>>, assets: Res<VmAssets>, mut meshes: ResMut<Assets<Mesh>>) {
    if !roots.is_empty() {
        return;
    }
    let Ok(cam) = cams.single() else { return };
    let ring_mesh = meshes.add(Torus::new(0.011, 0.02));
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    commands.entity(cam).with_children(|c| {
        c.spawn((Name::new("Viewmodel"), VmRoot, Transform::default(), Visibility::Hidden)).with_children(|root| {
            for right in [true, false] {
                root.spawn((VmArm { right }, Transform::from_translation(if right { ELBOW_R } else { ELBOW_L }).with_scale(ARM_SCALE), Visibility::Inherited)).with_children(|arm| {
                    arm.spawn((VmArmMesh, Transform::default(), Visibility::Inherited, NotShadowCaster));
                    for thumb in [false, true] {
                        arm.spawn((VmFinger { right, thumb }, Mesh3d(cube.clone()), MeshMaterial3d(assets.finger_mat.clone()), Transform::default(), Visibility::Hidden, NotShadowCaster));
                    }
                    if right {
                        arm.spawn((
                            VmRing,
                            Mesh3d(ring_mesh.clone()),
                            MeshMaterial3d(assets.ring_mat.clone()),
                            Transform::from_xyz(0.0, 0.0, -0.3).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                            NotShadowCaster,
                        ));
                        arm.spawn((VmRingLight, PointLight { intensity: 4000.0, range: 2.5, color: Color::srgb(0.3, 1.0, 0.45), shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, 0.05, -0.28)));
                    }
                });
            }
        });
    });
}

fn part_mesh_to_bevy(pm: &PartMesh, palette: &[[f32; 3]; 4], offset: Vec3) -> Mesh {
    let positions: Vec<[f32; 3]> = pm.positions.iter().map(|p| [p[0] - offset.x, p[1] - offset.y, p[2] - offset.z]).collect();
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, pm.normals.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, pm.uvs.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, pm.colors(palette));
    m.insert_indices(Indices::U32(pm.indices.clone()));
    m
}

/// (Re)build the glove meshes when the costume or corps changes.
#[allow(clippy::too_many_arguments)]
fn rebuild_hands(
    mut commands: Commands,
    prefs: Res<UiPrefs>,
    lex: Option<Res<RingLexicon>>,
    active: Option<Res<ActiveWorld>>,
    assets: Res<VmAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut st: ResMut<VmState>,
    arms: Query<(&VmArm, &Children)>,
    arm_meshes: Query<Entity, With<VmArmMesh>>,
    mut rings: Query<&mut Transform, (With<VmRing>, Without<VmFinger>)>,
    mut lights: Query<&mut Transform, (With<VmRingLight>, Without<VmRing>)>,
    (mut fingers, mut mats): (FingerQuery, ResMut<Assets<StandardMaterial>>),
) {
    let Some(lex) = lex else { return };
    if arms.is_empty() {
        return;
    }
    let corps = local_corps(active.as_deref(), &prefs);
    let c = &prefs.costume;
    let key = (c.phrase.clone(), c.seed, c.hide, (c.head_scale * 100.0) as u32, corps.index());
    if st.built_for.as_ref() == Some(&key) {
        return;
    }
    st.built_for = Some(key);
    // Hands only: arm-mounted gear (shields, saws) would fill the view.
    let mut c1 = c.clone();
    c1.hide |= bd_shapes::acc::LEFT_ARM;
    let spec = crate::lobby::costume::costume_spec(&lex.0, &c1);
    let hands = bd_shapes::costume_first_person_hands(&spec);
    let model = bd_shapes::costume_model(&hands);
    let parts = bd_shapes::mesh_model(&model);
    let cc = corps.color();
    let accent = crate::lobby::costume::costume_accent(&lex.0, c).unwrap_or([cc[0] * 0.6 + 0.4, cc[1] * 0.6 + 0.4, cc[2] * 0.6 + 0.4]);
    let palette = [cc.map(|x| x * 0.8 + 0.05), accent, cc.map(|x| x * 0.25 + 0.03), [1.0, 1.0, 0.95]];
    for (arm, children) in &arms {
        let role = if arm.right { bd_core::shape::PartRole::ArmRight } else { bd_core::shape::PartRole::ArmLeft };
        let found = model.parts.iter().enumerate().find(|(_, p)| p.role == Some(role));
        for child in children.iter() {
            if !arm_meshes.contains(child) {
                continue;
            }
            match found {
                Some((i, p)) => {
                    let pivot = Vec3::from_array(p.pivot.to_array());
                    let pm = &parts[i];
                    if pm.is_empty() {
                        continue;
                    }
                    // Fingertip = farthest point along −Z from the elbow.
                    let tip = pm.positions.iter().map(|q| Vec3::from_array(*q) - pivot).fold(Vec3::ZERO, |a, b| if b.z < a.z { b } else { a });
                    // Use the glove's centre line at the tip (not a corner).
                    let tip = Vec3::new(0.0, tip.y * 0.5, tip.z);
                    let side = if arm.right { 0 } else { 1 };
                    st.tip[side] = tip;
                    // The hand block: vertices in the far half of the arm (glove + cuff).
                    let (mut gmin, mut gmax, mut far) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), (f32::MAX, 0usize));
                    for (k, q) in pm.positions.iter().enumerate() {
                        let v = Vec3::from_array(*q) - pivot;
                        if v.z < tip.z * 0.52 {
                            gmin = gmin.min(v);
                            gmax = gmax.max(v);
                        }
                        if v.z < far.0 {
                            far = (v.z, k);
                        }
                    }
                    if gmin.cmple(gmax).all() {
                        st.glove[side] = (gmin, gmax);
                        place_fingers(&mut fingers, arm.right, gmin, gmax);
                    }
                    // Fingers take the glove's colour at the fingertip (a touch darker: reads as a fold).
                    if arm.right
                        && let Some(c) = pm.colors(&palette).get(far.1)
                        && let Some(mut m) = mats.get_mut(&assets.finger_mat)
                    {
                        m.base_color = Color::linear_rgb(c[0] * 0.8, c[1] * 0.8, c[2] * 0.8);
                    }
                    let mesh = meshes.add(part_mesh_to_bevy(pm, &palette, pivot));
                    commands.entity(child).insert((Mesh3d(mesh), MeshMaterial3d(assets.arm_mat.clone())));
                    if arm.right {
                        let ring_at = Vec3::new(tip.x * 0.5, tip.y * 0.5 + 0.03, tip.z + 0.07);
                        for mut t in &mut rings {
                            t.translation = ring_at;
                        }
                        for mut t in &mut lights {
                            t.translation = ring_at + Vec3::new(0.0, 0.06, 0.02);
                        }
                    }
                }
                None => {
                    // Costume without arms (shouldn't happen): a plain glove block.
                    let mesh = meshes.add(Cuboid::new(0.09, 0.09, 0.34));
                    commands.entity(child).insert((Mesh3d(mesh), MeshMaterial3d(assets.arm_mat.clone()), Transform::from_xyz(0.0, 0.0, -0.17)));
                }
            }
        }
    }
}

type FingerQuery<'w, 's> = Query<'w, 's, (&'static VmFinger, &'static mut Transform), (Without<VmRing>, Without<VmRingLight>)>;

/// Curled fingers wrap the front-bottom of the fist (where a grip passes through the palm) and a
/// thumb lies over the inner top edge: shown while the hand holds something, it reads as gripping.
fn place_fingers(fingers: &mut FingerQuery, right: bool, gmin: Vec3, gmax: Vec3) {
    let (s, c) = (gmax - gmin, (gmin + gmax) * 0.5);
    for (f, mut tf) in fingers.iter_mut().filter(|(f, _)| f.right == right) {
        *tf = if f.thumb {
            let inner = if right { gmin.x - s.x * 0.04 } else { gmax.x + s.x * 0.04 };
            Transform::from_xyz(inner, c.y + s.y * 0.18, c.z - s.z * 0.12).with_scale(Vec3::new(s.x * 0.24, s.y * 0.3, s.z * 0.55))
        } else {
            Transform::from_xyz(c.x, c.y - s.y * 0.22, gmin.z - s.z * 0.04).with_scale(Vec3::new(s.x * 0.92, s.y * 0.5, s.z * 0.26))
        };
    }
}

/// The local player's damage % ("strain"). Smash model: uncapped, 0 at respawn.
pub fn strain_pct(p: &bd_sim::Player) -> f32 {
    if p.damage_pct.is_finite() { p.damage_pct.max(0.0) } else { 0.0 }
}

/// White → gold → orange → red as % rises (readability colour, same ramp as body glow).
pub fn strain_color(pct: f32) -> Color {
    let stops = [(0.0, Color::srgb(1.0, 1.0, 1.0)), (50.0, Color::srgb(1.0, 0.85, 0.3)), (100.0, Color::srgb(1.0, 0.5, 0.1)), (150.0, Color::srgb(1.0, 0.12, 0.08))];
    for w in stops.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if pct <= b {
            return ca.mix(&cb, ((pct - a) / (b - a)).clamp(0.0, 1.0));
        }
    }
    stops[3].1
}

// ------------------------------------------------------------------------------------------ keys

#[allow(clippy::too_many_arguments)]
fn vm_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    focus: Res<UiFocus>,
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    aim: Option<Res<crate::present::camera3p::AimRay>>,
    mut st: ResMut<VmState>,
    mut cmds: MessageWriter<SimCommand>,
) {
    let Some(a) = active else { return };
    // CAMERA: power moves fly along the crosshair ray (third person: converged), like casts.
    let aim = aim.as_deref().filter(|r| r.dir.is_finite() && r.dir.length_squared() > 0.5).map(|r| bd_sim::world::Aim::new(crate::util::core_v3(r.origin), crate::util::core_v3(r.dir)));
    if *state.get() != AppState::InGame || menu.0 || focus.typing {
        st.power_down = None;
        return;
    }
    let Some(me) = a.local_player() else { return };
    // Power: tap = move 0 (on release), hold ≥ 0.28 s = move 1 (fires at the threshold).
    const HOLD: f32 = 0.28;
    let has_power = a.world.player_power(me.id).is_some();
    st.power_since_tap = (st.power_since_tap + time.delta_secs()).min(9.0);
    if binds.just_pressed(Action::Power, &keys, &mouse) {
        st.power_down = Some(0.0);
        st.power_long_sent = false;
    }
    if let Some(t) = st.power_down.as_mut() {
        *t += time.delta_secs();
        let t = *t;
        if t >= HOLD && !st.power_long_sent {
            st.power_long_sent = true;
            if has_power {
                cmds.write(SimCommand::local(Cmd::PowerMove(1, aim)));
            }
        }
        if binds.just_released(Action::Power, &keys, &mouse) {
            if t < HOLD && has_power {
                // G double-tap: the next held power becomes the active one (powers mix).
                let slot = if st.power_since_tap < 0.3 { bd_sim::powers::move_sel::CYCLE } else { 0 };
                cmds.write(SimCommand::local(Cmd::PowerMove(slot, aim)));
                st.power_since_tap = if slot == 0 { 0.0 } else { 9.0 };
            }
            st.power_down = None;
        }
    }
    if binds.just_pressed(Action::Transform, &keys, &mouse) {
        cmds.write(SimCommand::local(Cmd::ToggleTransform));
    }
    if binds.just_pressed(Action::Dismiss, &keys, &mouse) {
        cmds.write(SimCommand::local(Cmd::Dismiss));
    }
    if me.seat.is_some_and(|s| !s.suit) && binds.just_pressed(Action::TetherR, &keys, &mouse) {
        cmds.write(SimCommand::local(Cmd::Eject));
    }
}

// ------------------------------------------------------------------------------------------ events

fn swing_kind_for(fx: HeldFx, strength: f32) -> Option<(SwingKind, bool)> {
    Some(match fx {
        HeldFx::SwingStart if strength >= 1.9 => (SwingKind::Heavy, false),
        HeldFx::SwingStart if strength > 1.05 => (SwingKind::Combo(((strength - 1.0) * 10.0).round().clamp(1.0, 3.0) as u8), false),
        HeldFx::SwingStart => (SwingKind::Primary, false),
        HeldFx::ChargeStart => (SwingKind::Heavy, true),
        HeldFx::GuardUp | HeldFx::Parry => (SwingKind::Secondary, true),
        HeldFx::Uppercut | HeldFx::Lunge | HeldFx::GroundPound | HeldFx::DiveStart => (SwingKind::Secondary, false),
        HeldFx::Throw => (SwingKind::Throw, false),
        _ => return None,
    })
}

#[allow(clippy::too_many_arguments)]
fn vm_events(
    mut events: MessageReader<GameEvents>,
    mut cues: MessageReader<RingCue>,
    active: Option<Res<ActiveWorld>>,
    mut st: ResMut<VmState>,
    mut sfx: MessageWriter<crate::audio::PlaySfx>,
    input: Res<crate::input::LocalInputState>,
) {
    for c in cues.read() {
        match c {
            RingCue::Forming { .. } => {
                st.flare = 1.0;
                st.punch.v -= 1.6;
                sfx.write(crate::audio::PlaySfx::new("construct_form").gain(0.45));
            }
            RingCue::Rush => {
                st.punch.v -= 2.2;
                st.flare = st.flare.max(0.6);
            }
            RingCue::Wake => st.flare = st.flare.max(0.35),
            RingCue::Sleep => {}
        }
    }
    let Some(a) = active else {
        events.clear();
        return;
    };
    let Some(local) = a.local else {
        events.clear();
        return;
    };
    // Which construct do we hold (and what form)?
    // (Palm energy, `Form::Hands`, is nothing in the hand: the hands stay open and pose themselves —
    // `viewmodel_anim`'s palm layer; present::palm_beam draws the orb and the beam.)
    let held_c = a.world.constructs.held_by(local).filter(|c| c.category == FormCategory::Held && c.recipe.form != Form::Hands);
    let held = held_c.map(|c| (c.id, c.recipe.form));
    let held_pole = held_c.is_some_and(|c| c.has(bd_core::mechanics::Mechanic::Extends) && !bd_sim::constructs::gun::is_gun(c)); // WUKONG
    // The sim's quantised weight: the first-person swing matches its hit windows.
    let swing_w = held_c.map_or(1.0, |c| bd_sim::constructs::held::swing_weight(&c.recipe));
    st.held_w = held_c.map_or(1.0, |c| bd_shapes::weight_of(&c.recipe));
    if held.map(|h| h.0) != st.held.map(|h| h.0) {
        st.swing = None;
    }
    st.held = held;
    for e in events.read() {
        match &e.event {
            GameEvent::HeldFx { owner, fx, strength, .. } if *owner == local => {
                let Some((_, form)) = st.held else { continue };
                match fx {
                    HeldFx::SwingActive | HeldFx::SwingImpact | HeldFx::Catch | HeldFx::Block => {
                        if *fx == HeldFx::SwingImpact {
                            st.kick_back.v += 1.2;
                        }
                        if *fx == HeldFx::Catch {
                            st.punch.v += 1.0;
                        }
                        continue;
                    }
                    _ => {}
                }
                if let Some((kind, hold)) = swing_kind_for(*fx, *strength) {
                    // The heavy's start (the charge's release, strength 2) continues the held
                    // curve instead of restarting — or is already playing (released locally).
                    let heavy_start = *fx == HeldFx::SwingStart && *strength >= 1.9;
                    if heavy_start {
                        let released = st.released_at.take().is_some_and(|t| st.time - t < 0.6);
                        if let Some(s) = st.swing.as_mut()
                            && s.holding
                        {
                            s.holding = false;
                            continue;
                        }
                        if released {
                            continue;
                        }
                    }
                    let button = if *fx == HeldFx::ChargeStart { buttons::PRIMARY } else { buttons::SECONDARY };
                    let curve = crate::present::extend::swing_curve_for(held_pole, form, kind, *fx, *strength, swing_w); // WUKONG: poles twirl
                    st.swing = Some(ActiveSwing { curve, t: 0.0, holding: hold, button });
                }
            }
            GameEvent::ShotFired { owner, recoil, construct, .. } if *owner == local => {
                if construct.is_none() {
                    // Ring blast: a fist of light — the arm punches out.
                    st.punch.v -= 3.5;
                    st.flare = st.flare.max(0.7);
                } else {
                    let k = (recoil / 6.0).clamp(0.25, 1.6);
                    st.kick_pitch.v += 9.0 * k;
                    st.kick_back.v += 2.4 * k;
                }
            }
            // FREEDOM: casts are never refused for willpower — an unaffordable one sputters
            // (`cflags::SPUTTER`, a scaled-down flickering construct) and the ring sputters with it.
            GameEvent::ConstructSpawned { id, owner, .. } if *owner == local && bd_sim::freedom::is_sputtering(&a.world, *id) => {
                st.sputter = 0.6;
                st.flare = 0.0;
                sfx.write(crate::audio::PlaySfx::new("willpower_low").gain(0.5));
            }
            GameEvent::OathFinished { player, .. } if *player == local => st.flare = 1.0,
            _ => {}
        }
    }
    // Let go of the button → the held curve plays on NOW (a client doesn't wait ~RTT for the
    // host's heavy `SwingStart`; a guard dropped / charge cut short with no event never sticks).
    let now = st.time;
    if let Some(s) = st.swing.as_mut()
        && s.holding
        && !input.current.held(s.button)
    {
        s.holding = false;
        if s.button == buttons::PRIMARY {
            st.released_at = Some(now);
        }
    }
}

// ------------------------------------------------------------------------------------------ animation

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn animate_viewmodel(
    time: Res<Time>,
    prefs: Res<UiPrefs>,
    look: Res<LookAngles>,
    state: Res<State<AppState>>,
    active: Option<Res<ActiveWorld>>,
    oath: Res<crate::oath::OathState>,
    voice: Res<crate::voice_ui::VoiceUi>,
    assets: Res<VmAssets>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut st: ResMut<VmState>,
    mut held_out: ResMut<FirstPersonHeld>,
    (pose, anchor, mut hands_out, tuning, mut bite): (
        Res<anim::HandPose>,
        Res<crate::camera::ViewAnchor>,
        ResMut<ViewmodelHands>,
        Res<anim::VmTuningRes>,
        Option<ResMut<crate::present::impact::connect::ContactBite>>,
    ),
    mut held_q: (
        Option<Res<crate::present::constructs::ConstructEntities>>,
        Query<&bd_fx::ConstructVisual>,
        Option<Res<anim::AdsState>>,
        Res<crate::camera::CameraRig>,
        Local<Option<((ConstructId, usize), anim::HeldGeom)>>,
        Query<(&VmFinger, &mut Visibility), Without<VmRoot>>,
    ),
    mut root: Query<(&mut Transform, &mut Visibility), (With<VmRoot>, Without<VmArm>)>,
    mut arms: Query<(&VmArm, &mut Transform), Without<VmRoot>>,
    mut light: Query<&mut PointLight, With<VmRingLight>>,
) {
    let dt = time.delta_secs().min(0.05);
    let st = &mut *st;
    st.time += dt;
    // CONNECT: contact resistance (the swing drags through the body, then snaps through), the
    // hand kick on a connect and the lunge's forward thrust (`present::impact::connect`).
    let (swing_rate, kick, thrust) = bite.as_deref_mut().map_or((1.0, 0.0, 0.0), |b| (b.rate, std::mem::take(&mut b.kick), std::mem::take(&mut b.thrust)));
    st.kick_back.v += kick;
    st.punch.v -= thrust;
    let me = active.as_ref().and_then(|a| a.local_player());
    // (A KO keeps the hands up for a moment so they can drop.)
    let first_person = !prefs.third_person && *held_q.3 == crate::camera::CameraRig::FirstPerson;
    let show = *state.get() == AppState::InGame && first_person && me.is_some_and(|p| (p.alive || pose.ko_visible) && p.seat.is_none_or(|s| s.suit));
    let Ok((mut root_tf, mut vis)) = root.single_mut() else { return };
    let want = if show { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    hands_out.active = show;
    if !show {
        held_out.active = false;
        held_out.construct = None;
        return;
    }
    let Some(me) = me else { return };
    let corps = me.corps;

    // --- springs: kick ~150 ms (ω≈28, ζ≈0.75), sway softer.
    st.kick_pitch.step(dt, 28.0, 0.75);
    st.kick_back.step(dt, 28.0, 0.8);
    st.punch.step(dt, 22.0, 0.55);
    // Look sway, bob, tethers, ring, recoil…: `anim::HandPose` (frame-rate independent springs).
    root_tf.translation = pose.root_t + Vec3::new(0.0, 0.0, st.kick_back.x.clamp(-0.1, 0.25) * 0.06);
    root_tf.rotation = pose.root_r * Quat::from_rotation_x((st.kick_pitch.x * 0.012).clamp(-0.05, 0.35));

    // --- swing curve (right hand frame)
    // Rest: the item is held up at the side (tilted up and out), clear of the crosshair.
    // Guns / launchers / bows / shields point where you aim (DOOM-style), just right of centre.
    let aims = st.held.is_some_and(|(_, f)| matches!(f, Form::Gun | Form::Cannon | Form::Bow | Form::Staff | Form::Shield | Form::Gauntlet));
    let mut swing_tf: Option<Transform> = None;
    if let Some(s) = st.swing.as_mut() {
        if !s.holding || s.t < s.curve.hold_at {
            s.t += dt * swing_rate;
        }
        if s.holding {
            s.t = s.t.min(s.curve.hold_at);
        }
        let a3 = s.curve.sample(s.t);
        let (sc, r, tr3) = a3.to_scale_rotation_translation();
        swing_tf = Some(Transform { translation: Vec3::from_array(tr3.to_array()), rotation: Quat::from_array(r.to_array()), scale: Vec3::from_array(sc.to_array()) });
        if s.t >= s.curve.duration() && !s.holding {
            st.swing = None;
        }
    }

    // --- arms. Holding something: the item sits IN the right fist (its grip point on the palm) and
    // hand + item move as one rigid body (the arm turns with the item); two-handed guns bring the
    // left palm to the fore-grip. Empty hands: the rest pose. Both + `anim::HandPose` on top.
    let tu = &tuning.0;
    let tip_of = |i: usize| if st.tip[i] == Vec3::ZERO { Vec3::new(0.0, 0.0, -0.6) } else { st.tip[i] };
    // Palm = the middle of the hand block (0 right, 1 left); grip_palm along the arm if unknown.
    let palm_of = |i: usize| match st.glove[i] {
        (mn, mx) if mn.cmplt(mx).all() => Vec3::new(0.0, (mn.y + mx.y) * 0.5, (mn.z + mx.z) * 0.5),
        _ => Vec3::new(0.0, tip_of(i).y, tip_of(i).z * tu.grip_palm),
    };
    // Grip / support points from bd_shapes' grip API, computed once per held model.
    let geom = st.held.and_then(|(id, form)| {
        let e = *held_q.0.as_ref()?.0.get(&id)?;
        let vis = held_q.1.get(e).ok()?;
        let key = (id, std::sync::Arc::as_ptr(&vis.model) as usize);
        match *held_q.4 {
            Some((k, g)) if k == key => Some(g),
            _ => {
                let g = anim::held_geom_model(tu, &vis.model, form);
                *held_q.4 = Some((key, g));
                Some(g)
            }
        }
    });
    let ext = st.punch.x.clamp(-0.25, 0.1);
    let sights = st.held.is_some_and(|(_, f)| matches!(f, Form::Gun | Form::Cannon | Form::Bow | Form::Staff));
    let ads_blend = if sights { held_q.2.as_deref().map_or(0.0, |a| a.blend.clamp(0.0, 1.0)) } else { 0.0 };
    let (r_rot, r_elbow, item_root) = {
        let tip = tip_of(0);
        match (st.held, geom) {
            (Some(_), Some(geom)) if anim::legacy_grip() => {
                // Dev A/B only (`BD_VM_LEGACY_GRIP=1`): the pre-grip placement — item origin at the
                // fingertips, camera-aligned, arm following 35 % of the swing.
                let mut rot = pose.hand_r[anim::RIGHT] * Quat::from_euler(EulerRot::YXZ, 0.16, 0.30, 0.0);
                let rest = if aims {
                    Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.06, 0.03, 0.0)).with_translation(Vec3::new(-0.02, 0.02, 0.0))
                } else {
                    Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.7, 0.75, -0.5)).with_translation(Vec3::new(0.05, -0.03, 0.02))
                };
                let sw = swing_tf.unwrap_or(rest);
                if swing_tf.is_some() {
                    rot *= Quat::IDENTITY.slerp(sw.rotation, 0.35);
                }
                let elbow = FIST_R + pose.hand_t[anim::RIGHT] - rot * (tip * ARM_SCALE) + Vec3::new(0.0, 0.0, ext);
                let at = Transform::from_translation(elbow).with_rotation(rot).with_scale(ARM_SCALE).transform_point(tip);
                let item = Transform::from_translation(at).mul_transform(sw);
                let old = anim::HeldGeom { grip: Vec3::ZERO, ..geom };
                (rot, elbow, Some((anim::place_item(tu, &old, item.translation, item.rotation, HELD_VIEW_SCALE * item.scale.max_element()), geom)))
            }
            (Some(_), Some(geom)) => {
                let gf = anim::grip_frame(tu, aims, st.held_w, swing_tf.as_ref());
                let mut rot = pose.hand_r[anim::RIGHT] * gf.arm_rot;
                let fist = gf.fist + pose.hand_t[anim::RIGHT] + Vec3::new(0.0, 0.0, ext);
                let mut palm = fist - rot * (tip * ARM_SCALE) + rot * (palm_of(0) * ARM_SCALE);
                let mut item_rot = pose.hand_r[anim::RIGHT] * gf.item_rot;
                let mut scale = anim::hip_scale(tu, &geom, HELD_VIEW_SCALE * gf.item_scale);
                if ads_blend > 0.0 {
                    // Aim down sights: up to the eye along the camera axis, sight under the crosshair.
                    let (ads_palm, ads_scale) = anim::ads_palm(tu, &geom, scale);
                    scale += (ads_scale - scale) * ads_blend;
                    let arm_rest = Quat::from_euler(EulerRot::YXZ, tu.grip_gun_arm[0], tu.grip_gun_arm[1], 0.0);
                    rot = rot.slerp(anim::reach_rot(arm_rest, ELBOW_R, ads_palm), ads_blend);
                    palm = palm.lerp(ads_palm, ads_blend);
                    item_rot = item_rot.slerp(Quat::IDENTITY, ads_blend);
                }
                let elbow = palm - rot * (palm_of(0) * ARM_SCALE);
                (rot, elbow, Some((anim::place_item_scaled(&geom, palm, item_rot, scale), geom)))
            }
            _ => {
                let rot = pose.hand_r[anim::RIGHT] * Quat::from_euler(EulerRot::YXZ, 0.16, 0.30, 0.0);
                let fist = FIST_R + pose.hand_t[anim::RIGHT] + Vec3::new(0.0, 0.0, ext);
                (rot, fist - rot * (tip * ARM_SCALE), None)
            }
        }
    };
    let (l_rot, l_elbow, l_grip) = {
        let tip = tip_of(1);
        let rest = Quat::from_euler(EulerRot::YXZ, -0.20, 0.22, 0.0);
        let rot = pose.hand_r[anim::LEFT] * rest;
        let fist = FIST_L + pose.hand_t[anim::LEFT];
        let elbow = fist - rot * (tip * ARM_SCALE);
        let support = pose.support.max(ads_blend);
        match item_root.as_ref().and_then(|(item, geom)| anim::support_point(tu, geom, item)).filter(|_| support > 0.0) {
            Some(target) => {
                // Support hand: aim the forearm at the fore-grip and put the palm on it.
                let s_rot = anim::reach_rot(rest, ELBOW_L, target);
                let s_elbow = target - s_rot * (palm_of(1) * ARM_SCALE);
                let w = support;
                (rot.slerp(s_rot, w), elbow.lerp(s_elbow, w), w)
            }
            None => (rot, elbow, 0.0),
        }
    };
    for (arm, mut tf) in &mut arms {
        let (h, rot, elbow) = if arm.right { (anim::RIGHT, r_rot, r_elbow) } else { (anim::LEFT, l_rot, l_elbow) };
        tf.translation = elbow;
        tf.rotation = rot;
        // Where the rope leaves the hand (camera space).
        hands_out.local[h] = root_tf.transform_point(tf.transform_point(tip_of(if arm.right { 0 } else { 1 })));
    }
    // Curled fingers + thumb only while the hand closes on something.
    let gripping = [item_root.is_some(), l_grip > 0.5];
    for (f, mut v) in &mut held_q.5 {
        let want = if gripping[if f.right { 0 } else { 1 }] { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }

    // --- ring glow = willpower
    let wp_max = active.as_ref().map_or(100.0, |a| a.world.rules().willpower_max()).max(1.0);
    let wp = (me.willpower / wp_max).clamp(0.0, 1.0);
    st.flare = (st.flare - dt * 2.2).max(0.0);
    st.sputter = (st.sputter - dt).max(0.0);
    let mut target = 0.15 + 0.85 * wp;
    if wp < 0.2 {
        // Low: dim and flicker.
        let n = ((st.time * 23.0).sin() * (st.time * 7.3).cos()).abs();
        target *= 0.5 + 0.5 * n;
    }
    if st.sputter > 0.0 {
        target *= if (st.time * 40.0).sin() > 0.0 { 1.0 } else { 0.1 };
    }
    if oath.open {
        target = target.max(0.3 + 0.9 * oath.progress());
    }
    if voice.listening {
        target += voice.level * 1.2;
    }
    target += st.flare * 2.0 + pose.glow;
    st.glow += (target - st.glow) * (1.0 - (-18.0 * dt).exp());
    let pct = strain_pct(me);
    let ring_col = corps_color(corps).mix(&strain_color(pct), (pct / 60.0).clamp(0.0, 1.0));
    let c = ring_col.to_linear();
    let g = 1.0 + st.glow * 9.0;
    let key = [c.red * g, c.green * g, c.blue * g];
    // Only touch the material when the glow visibly changed (avoids a GPU re-upload every frame).
    if (key[0] - st.ring_key[0]).abs() + (key[1] - st.ring_key[1]).abs() + (key[2] - st.ring_key[2]).abs() > 0.02
        && let Some(mut m) = mats.get_mut(&assets.ring_mat)
    {
        st.ring_key = key;
        m.emissive = LinearRgba::rgb(c.red * g, c.green * g, c.blue * g);
        m.base_color = ring_col;
    }
    for mut l in &mut light {
        l.intensity = 600.0 + st.glow * 9000.0;
        l.color = ring_col;
    }

    // --- first-person held grip (world space)
    let held = st.held;
    held_out.construct = held.map(|h| h.0);
    held_out.active = held.is_some() && item_root.is_some();
    if let Some((item, _)) = item_root {
        // Camera space: the root (sway, bob, recoil) carries hand and item together.
        held_out.grip_local = root_tf.mul_transform(item);
        // This frame's view (the camera's GlobalTransform is still last frame's here);
        // `anim::publish_hands` re-places the item from the final camera transform in PostUpdate.
        let cam_tf = Transform::from_translation(anchor.eye).with_rotation(look.rotation());
        held_out.grip = cam_tf.mul_transform(held_out.grip_local);
    }
}

// ------------------------------------------------------------------------------------------ health fractures

fn draw_percent(
    prefs: Res<UiPrefs>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    active: Option<Res<ActiveWorld>>,
    mut last: Local<Option<i32>>,
    mut q: Query<(&mut Text, &mut TextColor, &mut Visibility), With<PercentText>>,
) {
    let Ok((mut t, mut col, mut vis)) = q.single_mut() else { return };
    let me = active.as_ref().and_then(|a| a.local_player()).filter(|p| p.alive);
    let show = prefs.show_percent && *state.get() == AppState::InGame && !menu.0 && me.is_some();
    let want = if show { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    let Some(me) = me.filter(|_| show) else { return };
    let pct = strain_pct(me).round().min(999.0) as i32;
    if *last != Some(pct) {
        *last = Some(pct);
        t.0.clear();
        use std::fmt::Write as _;
        let _ = write!(t.0, "{pct}%");
        col.0 = strain_color(pct as f32).with_alpha(0.85);
    }
}

fn spawn_fractures(mut commands: Commands) {
    commands.spawn((
        Name::new("DamagePercent"),
        PercentText,
        Text::new(""),
        TextFont::from_font_size(20.0),
        TextColor(Color::WHITE),
        TextShadow { offset: Vec2::new(0.0, 1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.6) },
        TextLayout::justify(Justify::Center),
        Node { position_type: PositionType::Absolute, left: percent(50), bottom: px(18), width: px(120), margin: UiRect::left(px(-60)), ..default() },
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(31),
    ));
    // Deterministic cracks: each is a short jagged polyline of thin rotated bars growing in from an
    // edge. Crack i appears once damage passes its threshold.
    let mut rng = crate::util::Rng::new(0xF4AC);
    commands
        .spawn((
            Name::new("HealthFractures"),
            FractureRoot,
            Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: px(0), bottom: px(0), ..default() },
            Visibility::Hidden,
            Pickable::IGNORE,
            GlobalZIndex(30),
        ))
        .with_children(|root| {
            for i in 0..14 {
                let threshold = 0.25 + 0.7 * (i as f32 / 14.0);
                // Start on an edge (percent coords).
                let edge = i % 4;
                let t = rng.range(0.08, 0.92);
                let (mut x, mut y, inward): (f32, f32, f32) = match edge {
                    0 => (t * 100.0, 0.0, 90.0),
                    1 => (100.0, t * 100.0, 180.0),
                    2 => (t * 100.0, 100.0, 270.0),
                    _ => (0.0, t * 100.0, 0.0),
                };
                let mut ang = inward + rng.range(-35.0, 35.0);
                let segs = 3 + (rng.f32() * 3.0) as usize;
                for s in 0..segs {
                    let len = rng.range(2.5, 6.0) * (1.0 - s as f32 * 0.12);
                    let (sx, sy) = (ang.to_radians().cos(), ang.to_radians().sin());
                    root.spawn((
                        Fracture { threshold: threshold + s as f32 * 0.04 },
                        Node {
                            position_type: PositionType::Absolute,
                            left: percent(x),
                            top: percent(y),
                            width: vw(len),
                            height: px(if s == 0 { 2.5 } else { 1.5 }),
                            ..default()
                        },
                        UiTransform { rotation: Rot2::degrees(ang), ..default() },
                        BackgroundColor(Color::NONE),
                        Pickable::IGNORE,
                    ));
                    // Next segment starts at this one's end (approx, vw ≈ % of width).
                    x += sx * len;
                    y += sy * len * 16.0 / 9.0;
                    ang += rng.range(-40.0, 40.0);
                    let _ = s;
                }
            }
        });
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_fractures(
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    active: Option<Res<ActiveWorld>>,
    comfort: Res<ComfortSettings>,
    time: Res<Time>,
    mut shown: Local<f32>,
    mut root: Query<&mut Visibility, With<FractureRoot>>,
    mut cracks: Query<(&Fracture, &mut BackgroundColor)>,
) {
    let me = active.as_ref().and_then(|a| a.local_player());
    // Cracks start ~40 %, full at ~160 %.
    let pct = me.filter(|p| p.alive).map_or(0.0, strain_pct);
    let dmg = ((pct - 20.0) / 140.0).clamp(0.0, 1.0);
    let dt = time.delta_secs();
    // Cracks close slowly, open fast.
    let k = if dmg > *shown { 1.0 - (-12.0 * dt).exp() } else { 1.0 - (-2.0 * dt).exp() };
    let prev = *shown;
    *shown += (dmg - *shown) * k;
    let on = *state.get() == AppState::InGame && !menu.0 && *shown > 0.2;
    let Ok(mut vis) = root.single_mut() else { return };
    let want = if on { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if !on || (prev - *shown).abs() < 1e-4 && !comfort.is_changed() {
        return;
    }
    let c = strain_color(pct).mix(&Color::WHITE, 0.35);
    let strength = 0.25 + 0.35 * comfort.flashes;
    for (f, mut bg) in &mut cracks {
        let a = ((*shown - f.threshold) / 0.12).clamp(0.0, 1.0) * strength;
        let want = if a <= 0.001 { Color::NONE } else { c.with_alpha(a) };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}
