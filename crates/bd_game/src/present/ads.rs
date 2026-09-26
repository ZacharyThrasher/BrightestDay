//! Aim down sights, first and third person (CAMERA worker).
//!
//! * **Input**: the AIM binding (default RMB) *with a gun in hand* (`weapons::archetype_for`: gun,
//!   cannon, bow, staff, disc) holds — or toggles, `CameraPrefs::ads_toggle` — ADS and sends
//!   `buttons::ADS`; RMB's usual `SECONDARY` is dropped for that gun, and its alt-fire moves to the
//!   ALT FIRE binding (default Left Alt). Without a gun RMB is untouched (shove, block, throw…).
//! * **Gameplay** (bd_sim `gun.rs`): accuracy only — pellet cones tighten to `ADS_SPREAD`; the
//!   minigun pre-spins. Movement never reads the bit, so prediction and the host agree trivially.
//! * **Presentation**: [`AdsState`] `{ active, blend }` (eased, read by the hands worker's gun
//!   raise and by `camera3p`, which tightens over the shoulder); FOV zoom per gun ([`gun_zoom`]:
//!   pistol mild … railgun strong — the railgun's old "scope"); look sensitivity × `ads_sens` / zoom
//!   (1.0 = zoom-matched). Zoom speed pref; comfort preset = instant.

use super::camera3p::{ViewMode, comfort_cut};
use crate::input::{CursorCapture, InputOverride, LocalInputState, LookScale, UiFocus};
use crate::lobby::prefs::{Action, Bind, KeyBinds, UiPrefs};
use crate::settings::ComfortSettings;
use crate::sim_bridge::ActiveWorld;
use crate::state::{AppState, InGameMenu};
use crate::tick::FrameSet;
use bd_core::input::buttons;
use bd_core::mechanics::FormCategory;
use bd_sim::weapons::{GunArchetype, archetype_for};
use bevy::prelude::*;

/// ADS readability: a world word (shot / hit onomatopoeia) that would land on the sight line
/// (within ~5° of the crosshair, plus its own half size) slides off it and shrinks, so aiming stays
/// clear. `cam` = camera position + forward; `blend` = [`AdsState::blend`]. Returns (pos, size).
pub fn off_sight_line(at: Vec3, size: f32, cam: Vec3, fwd: Vec3, blend: f32) -> (Vec3, f32) {
    if blend <= 0.0 || !at.is_finite() {
        return (at, size);
    }
    let to = at - cam;
    let d = to.dot(fwd);
    if d <= 0.3 {
        return (at, size);
    }
    let perp = to - fwd * d;
    let r = perp.length();
    let lim = d * 0.09 + size * 0.6;
    if r >= lim {
        return (at, size);
    }
    // Away from the line (straight on it: to the lower right, where the own-shot words go).
    let right = fwd.cross(Vec3::Y).normalize_or(Vec3::X);
    let dir = if r > 1e-3 { perp / r } else { (right - Vec3::Y * 0.3).normalize() };
    (at + dir * (lim - r) * blend, size * (1.0 - 0.3 * blend))
}

/// Default time to full zoom (s), divided by `CameraPrefs::ads_speed`.
pub const ADS_SECS: f32 = 0.13;
/// Over the shoulder, ADS zooms this much less (fraction of the extra zoom).
pub const THIRD_ZOOM_CUT: f32 = 0.4;

/// Aim-down-sights state for this frame.
#[derive(Resource, Debug, Clone, Copy)]
pub struct AdsState {
    /// The aim button is held (or toggled) with a gun in hand.
    pub active: bool,
    /// Eased 0 (hip) … 1 (fully aimed).
    pub blend: f32,
    /// Full-ADS zoom factor of the current (or last) gun (≥ 1; tan-FOV divisor).
    pub zoom: f32,
    /// The gun in hand, if any.
    pub gun: Option<GunArchetype>,
    /// Zoom applied to the view this frame (eased; milder over the shoulder: the camera also
    /// moves in, and a narrow FOV behind a body reads claustrophobic).
    pub view_zoom: f32,
}

impl Default for AdsState {
    fn default() -> Self {
        Self { active: false, blend: 0.0, zoom: 1.0, gun: None, view_zoom: 1.0 }
    }
}

/// Scripted ADS for automation / screenshots (`Some(on)` replaces the devices).
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct AdsScript(pub Option<bool>);

/// Zoom per gun: pistols mild, precision guns strong.
pub fn gun_zoom(a: GunArchetype) -> f32 {
    use GunArchetype::*;
    match a {
        Ring => 1.0,
        HandCannon => 1.35,
        SuperShotgun => 1.15,
        Minigun => 1.2,
        Rocket => 1.3,
        Railgun => 2.6,
        Bow => 1.5,
        Cannon => 1.25,
        Staff => 1.25,
        Disc => 1.2,
        // Palm energy: no sights, just a slight lean in.
        HandBeam => 1.1,
    }
}

/// The local player's held gun (alive, in hand).
pub fn held_gun(active: &ActiveWorld) -> Option<GunArchetype> {
    let me = active.local?;
    if !active.world.player(me).is_some_and(|p| p.alive) {
        return None;
    }
    let c = active.world.constructs.held_by(me).filter(|c| c.category == FormCategory::Held)?;
    archetype_for(&c.recipe)
}

/// Look-sensitivity multiplier at a given ADS blend: `ads_sens` / zoom when fully aimed.
pub fn sens_scale(blend: f32, zoom: f32, ads_sens: f32) -> f32 {
    let b = blend.clamp(0.0, 1.0);
    let z = 1.0 + (zoom.max(1.0) - 1.0) * b;
    (1.0 + (ads_sens - 1.0) * b) / z
}

pub struct AdsPlugin;

impl Plugin for AdsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AdsState>()
            .init_resource::<AdsScript>()
            .add_systems(Update, (ads_input, ads_blend).chain().in_set(FrameSet::Input));
    }
}

#[allow(clippy::too_many_arguments)]
fn ads_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Option<Res<KeyBinds>>,
    prefs: Option<Res<UiPrefs>>,
    capture: Res<CursorCapture>,
    ov: Res<InputOverride>,
    script: Res<AdsScript>,
    focus: Res<UiFocus>,
    menu: Res<InGameMenu>,
    state: Res<State<AppState>>,
    mode: Res<ViewMode>,
    active: Option<Res<ActiveWorld>>,
    mut ads: ResMut<AdsState>,
    mut input: ResMut<LocalInputState>,
    mut toggled: Local<bool>,
) {
    let gun = if mode.piloting || mode.director { None } else { active.as_deref().and_then(held_gun) };
    ads.gun = gun;
    if let Some(g) = gun {
        ads.zoom = gun_zoom(g);
    }
    let devices = capture.grabbed && !ov.active && *state.get() == AppState::InGame && !menu.0;
    let aim_bind = binds.as_ref().map_or(Bind::Mouse(MouseButton::Right), |b| b.get(Action::Aim));
    let alt_bind = binds.as_ref().map_or(Bind::Key(KeyCode::KeyH), |b| b.get(Action::AltFire));
    // A letter bound to these must not fire while typing in the ring console.
    let live = |b: Bind| devices && !(focus.typing && matches!(b, Bind::Key(_)));
    let toggle = prefs.as_ref().is_some_and(|p| p.camera.ads_toggle);
    let want = match script.0 {
        Some(on) => on,
        None if !live(aim_bind) || gun.is_none() => {
            *toggled = false;
            false
        }
        None if toggle => {
            if aim_bind.just_pressed(&keys, &mouse) {
                *toggled = !*toggled;
            }
            *toggled
        }
        None => aim_bind.pressed(&keys, &mouse),
    };
    ads.active = want && gun.is_some();

    // Input bits (applied by `LocalInputState::next_tick`).
    let rmb_aims = gun.is_some() && aim_bind == Bind::Mouse(MouseButton::Right) && devices;
    let alt_held = live(alt_bind) && alt_bind.pressed(&keys, &mouse);
    let alt_just = live(alt_bind) && alt_bind.just_pressed(&keys, &mouse);
    let mask = if rmb_aims { buttons::SECONDARY } else { 0 };
    let held = if ads.active { buttons::ADS } else { 0 } | if alt_held { buttons::SECONDARY } else { 0 };
    if input.ext_mask != mask {
        input.ext_mask = mask;
    }
    if input.ext_held != held {
        input.ext_held = held;
    }
    if alt_just {
        input.ext_latched |= buttons::SECONDARY;
    }
}

fn ads_blend(
    time: Res<Time<Real>>,
    mode: Res<ViewMode>,
    comfort: Res<ComfortSettings>,
    prefs: Option<Res<UiPrefs>>,
    mut ads: ResMut<AdsState>,
    mut scale: ResMut<LookScale>,
    mut raw: Local<f32>,
) {
    let target = if ads.active { 1.0 } else { 0.0 };
    let cam = prefs.as_ref().map(|p| p.camera).unwrap_or_default();
    if comfort_cut(&comfort) {
        *raw = target;
    } else {
        let step = time.delta_secs().min(0.1) * cam.ads_speed() / ADS_SECS;
        *raw = if *raw < target { (*raw + step).min(target) } else { (*raw - step).max(target) };
    }
    let x = raw.clamp(0.0, 1.0);
    let blend = x * x * (3.0 - 2.0 * x);
    if ads.blend != blend {
        ads.blend = blend;
    }
    // Over the shoulder the camera also dollies in, so the FOV zooms less.
    let zoom = 1.0 + (ads.zoom - 1.0) * (1.0 - THIRD_ZOOM_CUT * mode.blend);
    let vz = 1.0 + (zoom - 1.0) * blend;
    if ads.view_zoom != vz {
        ads.view_zoom = vz;
    }
    let s = sens_scale(blend, zoom, cam.ads_sens());
    if (scale.0 - s).abs() > 1e-6 {
        scale.0 = s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Another player's shot / hit word right on the crosshair slides off the sight line while
    /// aiming (and not at hip fire); words already off to the side stay put.
    #[test]
    fn words_slide_off_the_sight_line_while_aiming() {
        let (cam, fwd) = (Vec3::new(0.0, 1.6, 0.0), Vec3::NEG_Z);
        let on = Vec3::new(0.05, 1.6, -10.0);
        let (p, s) = off_sight_line(on, 1.0, cam, fwd, 1.0);
        let r = (p - cam - fwd * (p - cam).dot(fwd)).length();
        assert!(r >= 10.0 * 0.09 + 0.6 - 1e-3, "pushed off the line: {r}");
        assert!(s < 1.0);
        assert_eq!(off_sight_line(on, 1.0, cam, fwd, 0.0), (on, 1.0), "hip fire: untouched");
        let side = Vec3::new(4.0, 1.6, -10.0);
        assert_eq!(off_sight_line(side, 1.0, cam, fwd, 1.0), (side, 1.0), "already clear");
        let behind = Vec3::new(0.0, 1.6, 5.0);
        assert_eq!(off_sight_line(behind, 1.0, cam, fwd, 1.0), (behind, 1.0));
        // Dead on the line: lower right.
        let (p, _) = off_sight_line(Vec3::new(0.0, 1.6, -10.0), 1.0, cam, fwd, 1.0);
        assert!(p.x > 0.3 && p.y < 1.6);
    }

    #[test]
    fn zoom_matched_sensitivity() {
        assert_eq!(sens_scale(0.0, 2.6, 1.0), 1.0);
        assert!((sens_scale(1.0, 2.0, 1.0) - 0.5).abs() < 1e-6);
        assert!((sens_scale(1.0, 2.0, 1.5) - 0.75).abs() < 1e-6);
        // Monotonic while zooming in.
        let mut last = 1.0;
        for k in 0..=10 {
            let s = sens_scale(k as f32 / 10.0, 1.35, 1.0);
            assert!(s <= last + 1e-6);
            last = s;
        }
    }

    #[test]
    fn railgun_zooms_hardest_pistol_mild() {
        assert!(gun_zoom(GunArchetype::Railgun) > 2.0);
        assert!(gun_zoom(GunArchetype::HandCannon) < 1.5);
        assert!(gun_zoom(GunArchetype::SuperShotgun) < gun_zoom(GunArchetype::HandCannon));
    }

    /// The ADS bit survives the client → host path: send buffer with redundancy, the wire codec,
    /// the host's jitter queue (and a starvation repeat keeps it held).
    #[test]
    fn ads_bit_survives_a_net_round_trip() {
        use bd_core::input::PlayerInput;
        use bd_net::{InputBuffer, InputQueue};
        let mut buf: InputBuffer<PlayerInput> = InputBuffer::new(3);
        for k in 0..6u32 {
            let b = if k >= 2 { buttons::ADS | buttons::PRIMARY } else { 0 };
            buf.push(PlayerInput { yaw: 0.1 * k as f32, buttons: b, ..Default::default() });
        }
        let pkt = buf.packet(12.5).expect("packet");
        let bytes = bd_net::codec::encode(&pkt);
        let back: bd_net::InputPacket<PlayerInput> = bd_net::codec::decode(&bytes).expect("decode");
        assert_eq!(back, pkt);
        let mut q: InputQueue<PlayerInput> = InputQueue::new(2);
        q.receive(back);
        let mut got = Vec::new();
        for _ in 0..8 {
            if let Some(i) = q.next() {
                got.push(i.input);
            }
        }
        assert!(got.iter().filter(|i| i.held(buttons::ADS)).count() >= 4, "{got:?}");
        assert!(got.last().is_some_and(|i| i.held(buttons::ADS)), "a repeated (starved) input keeps ADS held");
    }
}
