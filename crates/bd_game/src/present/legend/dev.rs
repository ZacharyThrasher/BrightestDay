//! Dev (never set in play): `BD_LEGEND_TEST=<scene>` plays one legendary move on the practice map
//! so frame strips can be captured (docs/legendary). The weapon is cast through the real console
//! caster + lexicon at t = 1 s, the local player's spot at t = 2 s is the scene's anchor (heading
//! −Z), bots are idled (their brains drained) and pinned around it, the local input is driven with
//! `InputOverride` / `LookAngles`, and the move plays from t = 3 s.
//!
//! | scene | phrase | move |
//! |---|---|---|
//! | `mjolnir_recall` | mjolnir | RMB throw → lodges in the ground → tap RMB recall through a bot → the snap catch |
//! | `mjolnir_storm` | mjolnir | hold LMB (raise) → release: telegraph → lightning + arcs to a second bot |
//! | `shield_chain` | captain america's shield | charged throw bouncing between 3 bots, back to the hand |
//! | `buster_leap` | buster sword | hold LMB 0.7 s → leap and cleave (crater + shockwave + ground wave) |
//! | `chaos_sweep` | blades of chaos | LMB combo → the 360° finisher; then RMB chain pull to a far bot |
//! | `saber_deflect` | lightsaber | a bot fires a ring blast; the swing bats it toward another bot |
//! | `gravgun_punt` | gravity gun | ADS catches a boulder, LMB punts it at a bot |
//! | `golden_gun` / `needler` / `bfg` | golden gun / needler / bfg | one huge gold round / needles stacking to a supercombine / the green orb and its tendrils |
//!
//! * `BD_LEGEND_SHOTS=<dir>`: screenshots at the scene's key moments (fixed times + offsets after the
//!   move's `HeldFx` events) → `<dir>/<scene>_<NN>_<tag>.png` (stitched by `tools/legend_strip.py`).
//! * `BD_LEGEND_CAM=0`: leave the camera alone (e.g. a first-person run); otherwise it is pinned
//!   per scene, relative to the anchor. Pair with `--third-person`, `BD_HIDDEN_WINDOW=1`,
//!   `BD_UI_PREFS=<scratch>` and `--quit-after`.

use crate::input::{InputOverride, LookAngles};
use crate::sim_bridge::{ActiveWorld, GameEvents};
use crate::state::AppState;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form};
use bd_core::{ConstructId, PlayerId};
use bd_sim::GameEvent;
use bd_sim::events::HeldFx;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

fn scene() -> Option<&'static str> {
    static V: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    V.get_or_init(|| std::env::var("BD_LEGEND_TEST").ok().filter(|v| !v.is_empty())).as_deref()
}

fn shots_dir() -> Option<&'static std::path::Path> {
    static V: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        let d = std::env::var("BD_LEGEND_SHOTS").ok().filter(|v| !v.is_empty())?;
        let _ = std::fs::create_dir_all(&d);
        Some(std::path::PathBuf::from(d))
    })
    .as_deref()
}

fn pin_camera() -> bool {
    std::env::var("BD_LEGEND_CAM").map_or(true, |v| v != "0")
}

struct Spec {
    phrase: &'static str,
    /// Bots: (x, z) metres from the anchor (−z = ahead); pinned until `pin_until` s.
    bots: &'static [(f32, f32)],
    pin_until: f32,
    /// Camera: offset from the anchor and the point it looks at (anchor-relative, world axes).
    cam: ([f32; 3], [f32; 3]),
    /// Fixed-time shots (s).
    at: &'static [f32],
    /// Shots after events: (fx, offsets s, max times this fx triggers).
    after: &'static [(HeldFx, &'static [f32], u8)],
}

fn spec(s: &str) -> Spec {
    match s {
        "mjolnir_recall" => Spec {
            phrase: "mjolnir",
            bots: &[(3.5, -4.0)],
            pin_until: 4.45,
            cam: ([10.0, 4.0, -1.0], [0.0, 1.0, -7.0]),
            at: &[3.12],
            after: &[(HeldFx::Lodge, &[0.25], 1), (HeldFx::RecallStart, &[0.03, 0.09], 1), (HeldFx::Catch, &[0.0, 0.1], 1)],
        },
        "mjolnir_storm" => Spec {
            phrase: "mjolnir",
            bots: &[(0.0, -12.0), (4.5, -13.0)],
            pin_until: 4.75,
            cam: ([4.5, 2.8, 2.5], [0.0, 2.2, -8.0]),
            at: &[3.6],
            after: &[(HeldFx::StormRaise, &[0.15], 1), (HeldFx::StormMark, &[0.05, 0.35], 1), (HeldFx::StormStrike, &[0.02, 0.1], 1)],
        },
        "shield_chain" => Spec {
            phrase: "captain america's shield",
            bots: &[(0.0, -8.0), (4.5, -10.0), (-3.5, -12.0)],
            pin_until: 7.0,
            cam: ([7.0, 5.0, 1.0], [0.0, 1.0, -8.0]),
            at: &[3.5],
            after: &[(HeldFx::Throw, &[0.08], 1), (HeldFx::Ricochet, &[0.02], 2), (HeldFx::Catch, &[0.02], 1)],
        },
        "buster_leap" => Spec {
            phrase: "buster sword",
            bots: &[(1.5, -7.0)],
            pin_until: 20.0,
            cam: ([10.0, 3.0, -4.0], [0.0, 2.5, -6.0]),
            at: &[3.55],
            after: &[(HeldFx::LeapStart, &[0.08, 0.3], 1), (HeldFx::LeapSlam, &[0.0, 0.08, 0.3], 1)],
        },
        "chaos_sweep" => Spec {
            phrase: "blades of chaos",
            bots: &[(0.0, 5.0), (6.0, -0.5), (0.0, -12.0)],
            pin_until: 20.0,
            cam: ([10.0, 7.0, 3.0], [0.0, 0.5, -2.0]),
            at: &[],
            after: &[(HeldFx::ChainSweep, &[0.05, 0.14, 0.24], 1), (HeldFx::ChainPull, &[0.03, 0.14], 1)],
        },
        "saber_deflect" => Spec {
            phrase: "lightsaber",
            bots: &[(0.0, -14.0), (-6.0, -10.0)],
            pin_until: 20.0,
            cam: ([8.0, 3.0, -2.0], [-2.0, 1.5, -7.0]),
            at: &[],
            after: &[(HeldFx::SwingStart, &[0.0], 1), (HeldFx::Parry, &[0.0, 0.06, 0.14, 0.26], 1)],
        },
        "gravgun_punt" => Spec {
            phrase: "gravity gun",
            bots: &[(3.0, -16.0)],
            pin_until: 20.0,
            cam: ([8.0, 3.0, -3.0], [1.0, 1.5, -8.0]),
            at: &[3.3, 4.2],
            after: &[(HeldFx::GravGrab, &[0.06], 1), (HeldFx::GravPunt, &[0.03, 0.12, 0.3], 1)],
        },
        "golden_gun" => Spec {
            phrase: "golden gun",
            bots: &[(0.0, -15.0)],
            pin_until: 20.0,
            cam: ([6.0, 2.5, -1.0], [0.0, 1.5, -8.0]),
            at: &[2.9],
            after: &[(HeldFx::OneShot, &[0.0, 0.05, 0.15, 0.4], 1)],
        },
        "needler" => Spec {
            phrase: "needler",
            bots: &[(0.0, -12.0)],
            pin_until: 20.0,
            cam: ([5.0, 2.5, -6.0], [0.0, 1.2, -11.0]),
            at: &[],
            after: &[(HeldFx::NeedleStick, &[0.0], 4), (HeldFx::Supercombine, &[0.0, 0.1], 1)],
        },
        "bfg" => Spec {
            phrase: "bfg",
            bots: &[(5.0, -12.0), (0.0, -24.0)],
            pin_until: 20.0,
            cam: ([9.0, 4.0, 4.0], [0.0, 1.5, -10.0]),
            at: &[3.2, 3.5],
            after: &[(HeldFx::BfgZap, &[0.0], 2)],
        },
        _ => Spec { phrase: "mjolnir", bots: &[], pin_until: 0.0, cam: ([6.0, 3.0, 4.0], [0.0, 1.5, -6.0]), at: &[3.5, 4.5], after: &[] },
    }
}

#[derive(Resource, Default)]
struct LegendDev {
    t: f32,
    cast: bool,
    anchor: Option<Vec3>,
    idle: Vec<PlayerId>,
    boulder: Option<ConstructId>,
    /// One-shot input moments already played.
    swung: bool,
    fired: bool,
    swept: bool,
    swing_at: Option<f32>,
    seeded: bool,
    shots: Vec<(f32, String)>,
    taken: u32,
    seen: Vec<(HeldFx, u8)>,
}

pub fn build(app: &mut App) {
    if scene().is_none() {
        return;
    }
    app.init_resource::<LegendDev>()
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

fn dev_cast(time: Res<Time<Real>>, state: Res<State<AppState>>, mut dev: ResMut<LegendDev>, mut caster: crate::console::Caster) {
    let Some(s) = scene() else { return };
    if *state.get() != AppState::InGame {
        return;
    }
    dev.t += time.delta_secs();
    if !dev.cast && dev.t >= 1.0 {
        dev.cast = true;
        let phrase = spec(s).phrase;
        info!("legend test {s}: casting {phrase:?}");
        caster.cast(phrase, false, 0);
    }
}

fn ground(w: &bd_sim::World, p: Vec3) -> Vec3 {
    let from = super::super::gv(p + Vec3::Y * 6.0);
    bd_core::voxel::raycast(&w.voxels, from, bd_core::Vec3::NEG_Y, 24.0).map_or(p, |h| Vec3::new(p.x, h.point.y, p.z))
}

/// Yaw / pitch from `eye` toward `at`.
fn aim(eye: Vec3, at: Vec3) -> (f32, f32) {
    let d = (at - eye).normalize_or(Vec3::NEG_Z);
    ((-d.x).atan2(-d.z), d.y.clamp(-1.0, 1.0).asin())
}

#[allow(clippy::too_many_lines)]
fn dev_world(active: Option<ResMut<ActiveWorld>>, mut dev: ResMut<LegendDev>, mut look: ResMut<LookAngles>, mut ov: ResMut<InputOverride>) {
    let Some(s) = scene() else { return };
    let Some(mut active) = active else { return };
    if dev.t <= 0.0 {
        return;
    }
    let t = dev.t;
    let sp = spec(s);
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
        info!("legend test {s}: anchor {:?}", dev.anchor);
    }
    ov.active = true;
    ov.move_x = 0;
    ov.move_z = 0;
    ov.held = 0;
    look.yaw = 0.0;
    look.pitch = 0.0;
    let Some(anchor) = dev.anchor else { return };
    let spot = |x: f32, z: f32| anchor + Vec3::new(x, 0.0, z);
    let eye = active.world.player(me).map_or(anchor + Vec3::Y * 1.6, |p| super::bv(p.eye(active.world.params())));
    let bot_spots: Vec<Vec3> = sp.bots.iter().map(|&(x, z)| ground(&active.world, spot(x, z))).collect();
    let chest = |i: usize| bot_spots.get(i).copied().unwrap_or(anchor + Vec3::new(0.0, 0.0, -10.0)) + Vec3::Y * 1.0;
    // Bots stand where the scene wants them (until released into the move).
    if t < sp.pin_until {
        let idle = dev.idle.clone();
        for (i, id) in idle.iter().enumerate() {
            let at = bot_spots.get(i).copied().unwrap_or_else(|| anchor + Vec3::new(30.0 + 3.0 * i as f32, 0.0, 30.0));
            if let Some(p) = active.world.player_mut(*id) {
                p.mv.pos = super::super::gv(at + Vec3::Y * 0.05);
                p.mv.vel = bd_core::Vec3::ZERO;
            }
        }
    }
    if (t * 2.0).floor() != ((t - 0.02) * 2.0).floor() {
        let held = active.world.constructs.held_by(me).map(|c| (c.recipe.name.clone(), c.recipe.form, c.recipe.mechanics.clone(), c.recipe.element, c.anim_state));
        info!("legend test {s} t={t:.2}: held {held:?}");
    }
    let t0 = 3.0;
    let win = |a: f32, b: f32| (t0 + a..t0 + b).contains(&t);
    match s {
        "mjolnir_recall" => {
            look.pitch = -0.16;
            if win(0.0, 0.06) || win(1.4, 1.46) {
                ov.held = buttons::SECONDARY;
            }
            // Once it lodged: a bot steps onto the way back.
            if win(1.0, 1.45)
                && let Some(c) = active.world.constructs.held_by(me)
                && !c.attached()
            {
                let mid = (super::bv(c.pos) + anchor) * 0.5;
                let at = ground(&active.world, mid);
                if let Some(id) = dev.idle.first().copied()
                    && let Some(p) = active.world.player_mut(id)
                {
                    p.mv.pos = super::super::gv(at + Vec3::Y * 0.05);
                    p.mv.vel = bd_core::Vec3::ZERO;
                }
            }
        }
        "mjolnir_storm" => {
            (look.yaw, look.pitch) = aim(eye, chest(0));
            if win(0.0, 1.3) {
                ov.held = buttons::PRIMARY;
            }
        }
        "shield_chain" => {
            (look.yaw, look.pitch) = aim(eye, chest(0));
            if win(0.0, 0.85) {
                ov.held = buttons::PRIMARY;
            }
        }
        "buster_leap" => {
            look.pitch = -0.05;
            if win(0.0, 0.7) {
                ov.held = buttons::PRIMARY;
            }
        }
        "chaos_sweep" => {
            if t < t0 + 3.2 {
                look.pitch = -0.1;
                let k = ((t - t0) / 0.45).floor();
                if !dev.swept && t >= t0 && (t - t0) - k * 0.45 < 0.05 {
                    ov.held = buttons::PRIMARY;
                }
            } else {
                (look.yaw, look.pitch) = aim(eye, chest(2));
                if win(3.5, 3.55) {
                    ov.held = buttons::SECONDARY;
                }
            }
        }
        "saber_deflect" => {
            (look.yaw, look.pitch) = aim(eye, chest(1));
            // The shooter fires a ring blast at us at t0; we swing as it closes in.
            if let Some(shooter) = dev.idle.first().copied() {
                if !dev.fired && t >= t0 {
                    dev.fired = true;
                    let from = chest(0) + Vec3::Y * 0.6;
                    let (y, p) = aim(from, eye - Vec3::Y * 0.4);
                    let input = PlayerInput { yaw: y, pitch: p, buttons: buttons::PRIMARY, ..Default::default() };
                    active.remote_inputs.push((shooter, input));
                    info!("legend test {s}: shooter fires");
                }
                let close = active.world.projectiles.iter().any(|pr| !pr.dead && pr.spec.owner == shooter && super::bv(pr.pos).distance(eye) < 7.5);
                if close && !dev.swung {
                    dev.swung = true;
                    dev.swing_at = Some(t);
                    dev.shots.push((t, "incoming".into()));
                    info!("legend test {s}: swing");
                }
                if dev.swing_at.is_some_and(|a| t < a + 0.08) {
                    ov.held = buttons::PRIMARY;
                }
            }
        }
        "gravgun_punt" => {
            if dev.boulder.is_none()
                && t >= 2.2
                && let Some(bot) = dev.idle.first().copied()
            {
                let r = bd_sim::bots::recipe("boulder", Form::Boulder, &[], Element::Earth, bd_sim::constructs::form_spec(Form::Boulder).default_scale);
                let from = super::super::gv(chest(0));
                dev.boulder = active.world.cast(bot, r, None, bd_sim::world::Aim::new(from, bd_core::Vec3::NEG_Z)).ok();
                info!("legend test {s}: boulder {:?}", dev.boulder);
            }
            if let Some(b) = dev.boulder
                && t < t0
                && let g = ground(&active.world, spot(0.0, -5.0))
                && let Some(c) = active.world.constructs.get_mut(b)
            {
                c.pos = super::super::gv(g + Vec3::Y * 0.8);
                c.vel = bd_core::Vec3::ZERO;
            }
            let rock = dev.boulder.and_then(|b| active.world.constructs.get(b)).map(|c| super::bv(c.center()));
            if t < t0 + 1.4 {
                if let Some(r) = rock {
                    (look.yaw, look.pitch) = aim(eye, r);
                }
            } else {
                (look.yaw, look.pitch) = aim(eye, chest(0));
            }
            if win(0.5, 0.56) {
                ov.held = buttons::ADS;
            }
            if win(1.6, 1.66) {
                ov.held = buttons::PRIMARY;
            }
        }
        "golden_gun" => {
            (look.yaw, look.pitch) = aim(eye, chest(0));
            if win(0.0, 0.06) {
                ov.held = buttons::PRIMARY;
            }
        }
        "needler" => {
            (look.yaw, look.pitch) = aim(eye, chest(0));
            look.yaw += 0.1;
            if win(0.0, 1.8) {
                ov.held = buttons::PRIMARY;
            }
        }
        "bfg" => {
            (look.yaw, look.pitch) = aim(eye, chest(1));
            if win(0.0, 0.06) {
                ov.held = buttons::PRIMARY;
            }
        }
        _ => {}
    }
}

/// Screenshots at the scene's fixed times and after its events.
fn dev_shots(mut events: MessageReader<GameEvents>, mut dev: ResMut<LegendDev>, active: Option<Res<ActiveWorld>>, mut commands: Commands) {
    let Some(s) = scene() else {
        events.clear();
        return;
    };
    let sp = spec(s);
    let t = dev.t;
    let local = active.as_ref().and_then(|a| a.local);
    for ev in events.read() {
        let GameEvent::HeldFx { fx, owner, .. } = &ev.event else { continue };
        // Our own moves (the saber scene's deflect is ours too).
        if Some(*owner) != local {
            continue;
        }
        info!("legend test {s} t={t:.2}: {fx:?}");
        if s == "chaos_sweep" && *fx == HeldFx::ChainSweep {
            dev.swept = true;
        }
        let Some(&(_, offs, max)) = sp.after.iter().find(|a| a.0 == *fx) else { continue };
        let n = match dev.seen.iter_mut().find(|x| x.0 == *fx) {
            Some(x) => {
                x.1 += 1;
                x.1
            }
            None => {
                dev.seen.push((*fx, 1));
                1
            }
        };
        if n > max {
            continue;
        }
        for (k, o) in offs.iter().enumerate() {
            let tag = format!("{fx:?}{}", if offs.len() > 1 { format!("+{k}") } else { String::new() }).to_lowercase();
            dev.shots.push((t + o, tag));
        }
    }
    if !dev.seeded {
        // Seed the fixed times once.
        dev.seeded = true;
        for a in sp.at {
            dev.shots.push((*a, format!("t{a:.2}")));
        }
    }
    let Some(dir) = shots_dir() else {
        dev.shots.retain(|x| x.0 > t);
        return;
    };
    // One capture per frame (two in the same frame lose one); the rest wait a frame.
    dev.shots.sort_by(|a, b| a.0.total_cmp(&b.0));
    let due: Vec<(f32, String)> = dev.shots.first().filter(|x| x.0 <= t).cloned().into_iter().collect();
    if !due.is_empty() {
        dev.shots.remove(0);
    }
    for (_, tag) in due {
        dev.taken += 1;
        let path = dir.join(format!("{s}_{:02}_{tag}.png", dev.taken));
        info!("legend test {s}: shot {}", path.display());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}

/// The scene's camera (relative to the anchor; `BD_LEGEND_CAM=0` leaves it alone).
fn dev_camera(dev: Res<LegendDev>, mut cams: Query<&mut Transform, With<crate::camera::PlayerCamera>>) {
    let Some(s) = scene() else { return };
    if !pin_camera() {
        return;
    }
    let Some(anchor) = dev.anchor else { return };
    let (off, at) = spec(s).cam;
    let eye = anchor + Vec3::new(off[0], off[1], off[2]);
    let target = anchor + Vec3::new(at[0], at[1], at[2]);
    for mut tf in &mut cams {
        *tf = Transform::from_translation(eye).looking_at(target, Vec3::Y);
    }
}
