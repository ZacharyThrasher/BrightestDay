//! Dev (never set in play): `BD_WEAPON_TEST=<phrase>` swings one weapon on the practice map so the
//! class comparison sheet can be captured (docs/weapons/light_vs_heavy.png). The weapon is cast
//! through the real console caster + lexicon at t = 1 s, the local player's spot at t = 2 s is the
//! anchor (heading −Z), bots are idled and one is pinned in reach in front until the swing, and the
//! local input is driven with `InputOverride` / `LookAngles` from t = 3 s: a tap (then a second tap
//! for the string) for melee, a 1.2 s hold for guns.
//!
//! * `BD_WEAPON_SHOTS=<dir>`: screenshots at the first swing's wind-up / strike / follow-through
//!   (timed from the class curve the sim sweeps) or the gun's first shots →
//!   `<dir>/<slug>_<NN>_<tag>.png` (stitched by `tools/weapon_sheet.py`).
//! * `BD_WEAPON_CAM=0`: leave the camera alone (first person); otherwise a fixed side view of the
//!   swing (third person: pair with `--third-person`, `BD_HIDDEN_WINDOW=1`, `--quit-after`).
//! * `BD_WEAPON_TAPS=<n>`: a flurry — `n` melee taps every `BD_WEAPON_TAP_GAP` s (default 0.3)
//!   instead of the two-tap string (text-clutter sheets).
//! * `BD_WEAPON_AT=0.2,0.6,1.2`: shots at these offsets (s) after the first swing / shot instead of
//!   the class key poses.

use crate::input::{InputOverride, LookAngles};
use crate::sim_bridge::{ActiveWorld, GameEvents};
use crate::state::AppState;
use bd_core::PlayerId;
use bd_core::input::buttons;
use bd_sim::GameEvent;
use bd_sim::events::HeldFx;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

fn phrase() -> Option<&'static str> {
    static V: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    V.get_or_init(|| std::env::var("BD_WEAPON_TEST").ok().filter(|v| !v.is_empty())).as_deref()
}

fn slug() -> String {
    phrase().unwrap_or("weapon").replace(' ', "_")
}

fn shots_dir() -> Option<&'static std::path::Path> {
    static V: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        let d = std::env::var("BD_WEAPON_SHOTS").ok().filter(|v| !v.is_empty())?;
        let _ = std::fs::create_dir_all(&d);
        Some(std::path::PathBuf::from(d))
    })
    .as_deref()
}

fn env_f32s(k: &str) -> Vec<f32> {
    std::env::var(k).ok().map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default()
}

/// `BD_WEAPON_TAPS` / `BD_WEAPON_TAP_GAP`: (taps, gap s); `None` = the default two-tap string.
fn flurry() -> Option<(u32, f32)> {
    let n: u32 = std::env::var("BD_WEAPON_TAPS").ok()?.parse().ok()?;
    let gap = env_f32s("BD_WEAPON_TAP_GAP").first().copied().unwrap_or(0.3).max(0.05);
    Some((n.max(1), gap))
}

fn pin_camera() -> bool {
    std::env::var("BD_WEAPON_CAM").map_or(true, |v| v != "0")
}

#[derive(Resource, Default)]
struct WeaponDev {
    t: f32,
    cast: bool,
    anchor: Option<Vec3>,
    idle: Vec<PlayerId>,
    first_swing: bool,
    first_shot: bool,
    shots: Vec<(f32, String)>,
    taken: u32,
}

pub fn build(app: &mut App) {
    if phrase().is_none() {
        return;
    }
    app.init_resource::<WeaponDev>()
        .add_systems(Update, (dev_cast, dev_world).chain().before(crate::tick::FrameSet::Input))
        .add_systems(Update, dev_shots.after(super::PresentSet::Events))
        .add_systems(
            PostUpdate,
            dev_camera
                .after(crate::camera::CameraUpdateSet)
                .after(crate::present::camera3p::place_camera)
                .before(bevy::transform::TransformSystems::Propagate),
        );
}

fn dev_cast(time: Res<Time<Real>>, state: Res<State<AppState>>, mut dev: ResMut<WeaponDev>, mut caster: crate::console::Caster) {
    let Some(p) = phrase() else { return };
    if *state.get() != AppState::InGame {
        return;
    }
    dev.t += time.delta_secs();
    if !dev.cast && dev.t >= 1.0 {
        dev.cast = true;
        info!("weapon test: casting {p:?}");
        caster.cast(p, false, 0);
    }
}

fn is_gun(active: &ActiveWorld, me: PlayerId) -> bool {
    active.world.constructs.held_by(me).is_some_and(bd_sim::constructs::gun::is_gun)
}

fn dev_world(active: Option<ResMut<ActiveWorld>>, mut dev: ResMut<WeaponDev>, mut look: ResMut<LookAngles>, mut ov: ResMut<InputOverride>) {
    let Some(mut active) = active else { return };
    if dev.t <= 0.0 {
        return;
    }
    let t = dev.t;
    let idle: Vec<PlayerId> = active.bots.drain(..).map(|b| b.id).collect();
    if !idle.is_empty() {
        dev.idle = idle;
    }
    let Some(me) = active.local else { return };
    for p in active.world.players.iter_mut() {
        p.spawn_shield = 0.0;
        if p.id == me {
            p.willpower = p.willpower.max(100.0);
        }
    }
    if dev.anchor.is_none() && t >= 2.0 {
        dev.anchor = active.world.player(me).map(|p| super::bv(p.mv.pos));
    }
    ov.active = true;
    ov.move_x = 0;
    ov.move_z = 0;
    ov.held = 0;
    look.yaw = 0.0;
    look.pitch = -0.08;
    let Some(anchor) = dev.anchor else { return };
    let gun = is_gun(&active, me);
    // The target: in reach for melee (the reach class a little further), downrange for guns.
    let reach = active.world.constructs.held_by(me).map_or(1.2, |c| c.half.z * 2.0);
    let dist = if gun { 9.0 } else { (reach * 0.8 + 0.9).clamp(1.6, 3.4) };
    let t0 = 3.0;
    let pin = if gun { t0 + 0.05 } else { t0 + 0.02 };
    let idle = dev.idle.clone();
    for (i, id) in idle.iter().enumerate() {
        if t >= pin && i == 0 {
            continue;
        }
        let at = if i == 0 { anchor + Vec3::new(0.0, 0.0, -dist) } else { anchor + Vec3::new(40.0 + 3.0 * i as f32, 0.0, 40.0) };
        if let Some(p) = active.world.player_mut(*id) {
            p.mv.pos = super::super::gv(at + Vec3::Y * 0.05);
            p.mv.vel = bd_core::Vec3::ZERO;
        }
    }
    let win = |a: f32, b: f32| (t0 + a..t0 + b).contains(&t);
    if gun {
        if win(0.0, 1.2) {
            ov.held = buttons::PRIMARY;
        }
    } else if let Some((n, gap)) = flurry() {
        if (0..n).any(|k| win(k as f32 * gap, k as f32 * gap + 0.02)) {
            ov.held = buttons::PRIMARY;
        }
    } else if win(0.0, 0.02) || win(0.55, 0.57) {
        ov.held = buttons::PRIMARY;
    }
}

/// Shots at the first swing's key poses (from the class curve the sim sweeps) / the first shots.
fn dev_shots(mut events: MessageReader<GameEvents>, mut dev: ResMut<WeaponDev>, active: Option<Res<ActiveWorld>>, mut commands: Commands) {
    if phrase().is_none() {
        events.clear();
        return;
    }
    let t = dev.t;
    let local = active.as_ref().and_then(|a| a.local);
    for ev in events.read() {
        let Some(a) = active.as_deref() else { continue };
        match &ev.event {
            GameEvent::HeldFx { fx: HeldFx::SwingStart, owner, .. } | GameEvent::ShotFired { owner, .. }
                if Some(*owner) == local && !dev.first_swing && !dev.first_shot && !env_f32s("BD_WEAPON_AT").is_empty() =>
            {
                dev.first_swing = true;
                dev.first_shot = true;
                for (k, at) in env_f32s("BD_WEAPON_AT").into_iter().enumerate() {
                    dev.shots.push((t + at, format!("{}t{}ms", k + 1, (at * 1000.0).round() as i32)));
                }
            }
            GameEvent::HeldFx { id, fx: HeldFx::SwingStart, owner, strength, .. } if Some(*owner) == local && !dev.first_swing => {
                dev.first_swing = true;
                let Some(c) = a.world.constructs.get(*id) else { continue };
                let tu = &a.world.cfg.tuning;
                let Some((cv, _, _)) = bd_sim::constructs::held::event_curve(c, HeldFx::SwingStart, *strength, tu) else { continue };
                let rate = bd_sim::constructs::held::swing_rate(c, tu).max(0.1);
                let (s, e) = cv.active_window();
                info!("weapon test: {} anticipation {:.3} active {:.3} recovery {:.3} rate {rate:.2}", cv.name, cv.anticipation, cv.active, cv.recovery);
                dev.shots.push((t + s * 0.92 / rate, "1windup".into()));
                dev.shots.push((t + (s + (e - s) * 0.6) / rate, "2strike".into()));
                dev.shots.push((t + (e + cv.recovery * 0.25) / rate, "3follow".into()));
            }
            GameEvent::ShotFired { owner, .. } if Some(*owner) == local && !dev.first_shot => {
                dev.first_shot = true;
                dev.shots.push((t + 0.004, "1fire".into()));
                dev.shots.push((t + 0.05, "2kick".into()));
                dev.shots.push((t + 0.9, "3sustain".into()));
            }
            _ => {}
        }
    }
    let Some(dir) = shots_dir() else {
        dev.shots.retain(|x| x.0 > t);
        return;
    };
    dev.shots.sort_by(|a, b| a.0.total_cmp(&b.0));
    if dev.shots.first().is_some_and(|x| x.0 <= t) {
        let (_, tag) = dev.shots.remove(0);
        dev.taken += 1;
        let path = dir.join(format!("{}_{:02}_{tag}.png", slug(), dev.taken));
        info!("weapon test: shot {}", path.display());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}

/// A fixed side-on view of the swing (the wielder, the weapon's whole arc and the target).
fn dev_camera(dev: Res<WeaponDev>, mut cams: Query<&mut Transform, With<crate::camera::PlayerCamera>>) {
    if !pin_camera() {
        return;
    }
    let Some(anchor) = dev.anchor else { return };
    let eye = anchor + Vec3::new(5.2, 2.1, -1.4);
    let target = anchor + Vec3::new(0.0, 1.25, -1.5);
    for mut tf in &mut cams {
        *tf = Transform::from_translation(eye).looking_at(target, Vec3::Y);
    }
}
