
//! `--scenario piloting --pilot-shot truck|transform|robot|starscream|board` (practice): dev frames
//! of piloting (docs/piloting/). You cast the real lexicon's "optimus prime" (default-piloted: you
//! spawn seated in the truck) or "starscream" on open ground; bots stop thinking and the others are
//! benched; the script drives your input. `transform`: a kick off the ground, then the Transform key
//! mid-air (robot + landing slam); `robot`: transformed, walking; `board`: a follower Optimus you walk
//! up to (the seat glows), then board with the Transform key.

use crate::console::RingLexicon;
use crate::input::{InputOverride, LookAngles};
use crate::sim_bridge::ActiveWorld;
use bd_core::input::buttons;
use bd_core::{ConstructId, PlayerId, Vec3};
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::prelude::{App, IntoScheduleConfigs, Res, ResMut, Resource, Time, Update, info, warn};
use bevy::time::Real;

#[derive(Resource, Clone, Debug, Default)]
pub struct PilotScenario {
    shot: String,
    t: f32,
    placed: bool,
    id: Option<ConstructId>,
    dir: Vec3,
    step: u32,
    /// Combat shots: the enemy dummy standing in front.
    dummy: Option<PlayerId>,
    /// Voice shot: lines already said.
    said: u8,
}

/// How a combat shot plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Script {
    /// Robot mode: four LMB taps, then Transform (the archetype's transform attack + finisher).
    String,
    /// Beast mode (the alt mode walks): two bites, a held tail sweep, held breath / a roar, a bite,
    /// then Transform (the robot bursts out into its finisher).
    Beast,
    /// Voice: "<kit> combo", then "<kit's blaster>", then "transform" — all said mid-fight.
    Voice,
    /// V3 SAUCE: light string hits, a held heavy, a blaster shot, then the string to its ender and
    /// Transform (the vehicle attack + finisher) — every beat of the move grammar in one strip.
    Sauce,
}

/// V3 SAUCE script times: (the ender's tap, the Transform press 0.13 s after it).
fn sauce_times(string_hits: u8) -> (f32, f32) {
    let ender = 5.7 + (string_hits.max(1) - 1) as f32 * 0.33;
    (ender, ender + 0.13)
}

/// Combat frame-strip shots (TRANSFORMER COMBAT + TRANSFORMERS FEEL): the phrase each one casts,
/// whether it fights in robot mode, and its script. Every kind: truck, tank, car, jet, helicopter,
/// bike, boat, train, dino, beast, combiner, planet-sized, a small suit and voice orders.
fn combat_shot(shot: &str) -> Option<(&'static str, bool, Script)> {
    use Script::*;
    Some(match shot {
        "combo" => ("optimus prime", true, String),
        "megatron" => ("megatron", true, String),
        "bumblebee" => ("bumblebee transformer", true, String),
        "flyby" => ("starscream", true, String),
        "soundwave" => ("soundwave", true, String),
        "tankcombo" => ("a transforming tank", true, String),
        "helicombo" => ("a robot that turns into a helicopter", true, String),
        "bikecombo" => ("arcee", true, String),
        "motorcycle" => ("a robot that turns into a motorcycle", true, String),
        "boat" => ("a robot that turns into a boat", true, String),
        "train" => ("a robot that turns into a train", true, String),
        "grimlock" => ("grimlock", false, Beast),
        "grimrobot" => ("grimlock", true, String),
        "primal" => ("optimus primal", false, Beast),
        "primalrobot" => ("optimus primal", true, String),
        "devastator" => ("devastator", true, String),
        "unicron" => ("unicron", true, String),
        "smallcar" => ("a small robot that turns into a car", true, String),
        "voice" => ("optimus prime", true, Voice),
        "sauce_truck" => ("optimus prime", true, Sauce),
        "sauce_jet" => ("starscream", true, Sauce),
        "sauce_tank" => ("a transforming tank", true, Sauce),
        "sauce_dino" => ("grimlock", true, Sauce),
        _ => return None,
    })
}

fn combat_phrase(shot: &str) -> Option<&'static str> {
    combat_shot(shot).map(|s| s.0)
}

/// Say `text` like the console does: a move / chain goes to the toolbox, anything else is cast.
fn say(active: &mut ActiveWorld, lex: &bd_lexicon::Lexicon, me: PlayerId, text: &str) {
    let held = active.world.player(me).map(|p| p.power.clone());
    let powers_on = active.world.cfg.rules.powers_enabled;
    if let Some(steps) = crate::toolbox::command(lex, held.as_ref(), powers_on, text, true) {
        let r = active.world.toolbox(me, steps, None);
        info!("piloting scenario: said \"{text}\" (toolbox) -> {r:?}");
        return;
    }
    let it = lex.intent(text, &[], true);
    let r = it.resolution;
    let Some(eye) = active.world.player(me).map(|p| p.eye(active.world.params())) else { return };
    let look = active.world.player(me).map_or(Vec3::NEG_Z, |p| p.look());
    let res = active.world.cast_shaped(me, r.recipe, r.shape, r.alt_shape, bd_sim::Aim::new(eye, look));
    info!("piloting scenario: said \"{text}\" (cast) -> {res:?}");
}

pub fn build(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "piloting") {
        return;
    }
    let shot = args.windows(2).find(|w| w[0] == "--pilot-shot").map(|w| w[1].clone()).unwrap_or_else(|| "truck".into());
    info!("piloting scenario: {shot}");
    app.insert_resource(PilotScenario { shot, ..Default::default() }).add_systems(Update, script.run_if(resource_exists::<ActiveWorld>));
    // V3 SAUCE strips: `BD_SAUCE_STRIP=<dir>` — a fixed side camera + frames at the script's beats.
    if let Ok(dir) = std::env::var("BD_SAUCE_STRIP") {
        app.insert_resource(SauceStrip { dir: dir.into(), ..Default::default() })
            .add_systems(Update, sauce_strip.after(script).run_if(resource_exists::<ActiveWorld>))
            .add_systems(
                bevy::prelude::PostUpdate,
                sauce_camera
                    .after(crate::camera::CameraUpdateSet)
                    .after(crate::present::camera3p::place_camera)
                    .before(bevy::transform::TransformSystems::Propagate),
            );
    }
}

/// `BD_SAUCE_STRIP` state: the side camera's anchor and which beats were captured.
#[derive(bevy::prelude::Resource, Default)]
struct SauceStrip {
    dir: std::path::PathBuf,
    /// (look-at point, camera position), fixed once the robot stands in its lane.
    cam: Option<(bevy::prelude::Vec3, bevy::prelude::Vec3)>,
    taken: u32,
    burst_at: Option<f32>,
    finish_at: Option<f32>,
}

/// Frames at the script's beats: light hit, heavy coil, heavy release, blaster, ender, the vehicle
/// attack (×2), the finisher (×2) — the same beats with the sauce on or off (`BD_TF_SAUCE=0`).
fn sauce_strip(mut commands: bevy::prelude::Commands, sc: Res<PilotScenario>, active: Res<ActiveWorld>, mut st: ResMut<SauceStrip>) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    use bd_sim::constructs::brawl::XPhase;
    let Some(id) = sc.id else { return };
    let Some(c) = active.world.constructs.get(id) else { return };
    let t = sc.t;
    if st.cam.is_none() && t > 2.45 {
        let d = bevy::prelude::Vec3::new(sc.dir.x, 0.0, sc.dir.z).normalize_or(bevy::prelude::Vec3::NEG_Z);
        let side = bevy::prelude::Vec3::new(-d.z, 0.0, d.x);
        let ctr = c.center();
        let at = bevy::prelude::Vec3::new(ctr.x, ctr.y, ctr.z) + d * 4.0;
        let k = (c.half.y * 2.0 / 4.8).max(1.0); // bigger bodies (a dino) get a wider shot
        st.cam = Some((at, at + side * 14.0 * k + bevy::prelude::Vec3::Y * 3.0 * k));
    }
    match c.brawl.xatk {
        XPhase::Burst if st.burst_at.is_none() => st.burst_at = Some(t),
        XPhase::Finish if st.finish_at.is_none() => st.finish_at = Some(t),
        _ => {}
    }
    let row = active.world.cfg.tuning.transformer.robot_row(c);
    let (ender, _) = sauce_times(row.string_hits);
    // Heavy release / ender: just past the impact frame's hold (the hit-stop still holds the pose).
    let beats = [
        Some(2.64),
        Some(4.05 + row.heavy_hold * 0.75),
        Some(4.05 + row.heavy_hold + 0.2),
        Some(5.04),
        Some(ender + 0.2),
        st.burst_at.map(|b| b + 0.12),
        st.burst_at.map(|b| b + 0.32),
        st.finish_at.map(|f| f + 0.05),
        st.finish_at.map(|f| f + 0.3),
    ];
    let k = st.taken as usize;
    if let Some(Some(at)) = beats.get(k)
        && t >= *at
    {
        st.taken += 1;
        let path = st.dir.join(format!("{}_{k}.png", sc.shot));
        info!("sauce strip: beat {k} at t={t:.2} -> {}", path.display());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}

fn sauce_camera(st: Res<SauceStrip>, mut cams: bevy::prelude::Query<&mut bevy::prelude::Transform, bevy::prelude::With<crate::camera::PlayerCamera>>) {
    let Some((at, pos)) = st.cam else { return };
    for mut tf in &mut cams {
        *tf = bevy::prelude::Transform::from_translation(pos).looking_at(at, bevy::prelude::Vec3::Y);
    }
}

/// The most open, flat ground near `me`: a spot with a clear lane (samples every 4 m for 60 m,
/// 6 m wide, within 2 m of the start height, nothing overhead) — (stand-at, lane direction).
fn stage(w: &bd_sim::World, me: Vec3) -> Option<(Vec3, Vec3)> {
    let top = |x: f32, z: f32| bd_core::voxel::raycast(&w.voxels, Vec3::new(x, 150.0, z), Vec3::NEG_Y, 170.0).map(|h| h.point.y);
    let mut cands: Vec<Vec3> = vec![me];
    for r in [12.0f32, 24.0, 36.0] {
        for k in 0..16 {
            let a = k as f32 / 16.0 * std::f32::consts::TAU;
            cands.push(me + Vec3::new(a.cos() * r, 0.0, a.sin() * r));
        }
    }
    for gx in -8..=8 {
        for gz in -8..=8 {
            cands.push(Vec3::new(gx as f32 * 10.0, 0.0, gz as f32 * 10.0));
        }
    }
    let mut best: Option<(u32, Vec3, Vec3)> = None;
    for c in cands {
        let Some(y0) = top(c.x, c.z) else { continue };
        if bd_core::voxel::raycast(&w.voxels, Vec3::new(c.x, y0 + 1.0, c.z), Vec3::Y, 30.0).is_some() {
            continue;
        }
        for k in 0..8 {
            let a = k as f32 / 8.0 * std::f32::consts::TAU;
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            let mut score = 0;
            for s in 0..15 {
                let p = c + d * (s as f32 * 4.0);
                for o in [-3.0f32, 0.0, 3.0] {
                    let q = p + Vec3::new(-d.z, 0.0, d.x) * o;
                    if top(q.x, q.z).is_some_and(|y| (y - y0).abs() < 1.0) {
                        score += 1;
                    }
                }
            }
            if best.is_none_or(|b| score > b.0) {
                best = Some((score, Vec3::new(c.x, y0 + 0.05, c.z), d));
            }
        }
    }
    best.map(|b| (b.1, b.2))
}

fn face(look: &mut LookAngles, d: Vec3, pitch: f32) {
    look.yaw = (-d.x).atan2(-d.z);
    look.pitch = pitch;
}

fn cast(active: &mut ActiveWorld, lex: &bd_lexicon::Lexicon, me: PlayerId, phrase: &str, dir: Vec3) -> Option<ConstructId> {
    let it = lex.intent(phrase, &[], false);
    let mut r = it.resolution;
    // The console's costume hook, exactly as a summon runs it (transformers keep their own body).
    if bd_shapes::costume::summon::apply(phrase, &mut r.recipe, &mut r.shape) {
        r.alt_shape = None;
    }
    info!("piloting scenario: \"{phrase}\" -> {} pilot {:?}", r.signature(), it.pilot);
    let eye = active.world.player(me).map(|p| p.eye(active.world.params()))?;
    match active.world.cast_shaped(me, r.recipe, r.shape, r.alt_shape, bd_sim::Aim::new(eye, (dir + Vec3::new(0.0, -0.35, 0.0)).normalize_or_zero())) {
        Ok(id) => {
            if let Some(c) = active.world.constructs.get_mut(id) {
                c.life = 1e6;
            }
            Some(id)
        }
        Err(e) => {
            warn!("piloting scenario: cast failed {e:?}");
            None
        }
    }
}

fn script(
    time: Res<Time<Real>>,
    lex: Option<Res<RingLexicon>>,
    mut sc: ResMut<PilotScenario>,
    mut active: ResMut<ActiveWorld>,
    mut look: ResMut<LookAngles>,
    mut ov: ResMut<InputOverride>,
    cam: bevy::prelude::Query<&bevy::prelude::GlobalTransform, bevy::prelude::With<crate::camera::PlayerCamera>>,
) {
    let dt = time.delta_secs();
    sc.t += dt;
    if sc.t < 0.8 {
        return;
    }
    let Some((me, my_pos)) = active.local_player().filter(|p| p.alive).map(|p| (p.id, p.mv.pos)) else { return };
    if !sc.placed {
        let Some(lex) = lex.as_ref() else { return };
        let Some((from, dir)) = stage(&active.world, my_pos) else {
            if sc.t > 6.0 {
                warn!("piloting scenario: no stage found");
                sc.placed = true;
            }
            return;
        };
        active.bots.clear();
        active.world.cfg.rules.willpower = bd_sim::WillpowerMode::Infinite;
        let others: Vec<PlayerId> = active.world.players.iter().map(|p| p.id).filter(|id| *id != me).collect();
        let combat = combat_phrase(&sc.shot).is_some();
        for (k, id) in others.into_iter().enumerate() {
            if combat && k == 0 {
                sc.dummy = Some(id); // the combat shots keep one enemy standing still as the target
                continue;
            }
            if let Some(p) = active.world.player_mut(id) {
                p.alive = false;
                p.respawn_timer = 1e6;
            }
        }
        if let Some(p) = active.world.player_mut(me) {
            p.mv.pos = from;
            p.mv.vel = Vec3::ZERO;
            p.spawn_shield = 0.0;
        }
        face(&mut look, dir, -0.12);
        let phrase = match sc.shot.as_str() {
            s if combat_phrase(s).is_some() => combat_phrase(s).unwrap_or("optimus prime"),
            "starscream" => "starscream",
            "board" => "optimus prime that follows me",
            _ => "optimus prime",
        };
        sc.id = cast(&mut active, &lex.0, me, phrase, dir);
        sc.dir = dir;
        sc.placed = true;
        sc.t = 1.0;
        return;
    }
    let Some(id) = sc.id.filter(|id| active.world.constructs.get(*id).is_some()) else { return };
    let t = sc.t;
    if (t * 4.0).floor() != ((t - dt) * 4.0).floor() {
        if let Some(c) = active.world.constructs.get(id) {
            let seat = active.world.player(me).and_then(|p| p.seat).map(|s| s.construct);
            let cam_d = cam.single().map(|g| (crate::util::core_v3(g.translation()) - c.center()).length()).unwrap_or(-1.0);
            info!("piloting scenario t={t:.1}: pos {:?} vel {:?} vehicle {} xform {:?} seat {:?} grounded {} half {:?} cam {cam_d:.1} m xatk {:?} step {} hits {}", c.pos, c.vel, c.held.vehicle, c.comp.xform.mode, seat, c.grounded, c.half, c.brawl.xatk, c.brawl.step, c.brawl.last_xattack_hits);
        }
    }
    ov.active = true;
    ov.held = 0;
    ov.move_x = 0;
    ov.move_z = 0;
    let dir = sc.dir;
    match sc.shot.as_str() {
        // Cruise down the lane in the truck (third person).
        "truck" => {
            face(&mut look, dir, -0.5);
            ov.move_z = if t > 1.6 { 1 } else { 0 };
        }
        // Drive, get kicked into the air, press Transform mid-jump: the robot lands in a slam.
        "transform" => {
            face(&mut look, dir, -0.5);
            ov.move_z = 1;
            if t > 2.4 && sc.step == 0 {
                sc.step = 1;
                if let Some(c) = active.world.constructs.get_mut(id) {
                    c.vel.y = 11.0;
                    c.grounded = false;
                }
            }
            if t > 2.65 && sc.step == 1 {
                sc.step = 2;
                active.world.toggle_transform(me);
            }
        }
        // Transformed on the ground, then walking the robot.
        "robot" => {
            face(&mut look, dir, -0.45);
            if t > 1.4 && sc.step == 0 {
                sc.step = 1;
                active.world.toggle_transform(me);
            }
            ov.move_z = if t > 2.8 { 1 } else { 0 };
        }
        // The jet: throttle up and climb along the lane.
        "starscream" => {
            face(&mut look, dir, if t < 2.0 { 0.0 } else { 0.28 });
            ov.move_z = 1;
        }
        // Stand beside your follower Optimus looking at it (the seat glows), then board it with the
        // Transform key.
        "board" => {
            let Some(c) = active.world.constructs.get(id) else { return };
            let (center, half, cpos, grounded) = (c.center(), c.half, c.pos, c.grounded || c.body.supported);
            if let Some(c) = active.world.constructs.get_mut(id) {
                c.vel.x = 0.0;
                c.vel.z = 0.0;
            }
            if sc.step == 0 && t > 2.2 && grounded {
                sc.step = 1;
                let side = Vec3::new(dir.z, 0.0, -dir.x);
                let at = cpos + side * (half.x + 1.6);
                let gy = bd_core::voxel::raycast(&active.world.voxels, Vec3::new(at.x, cpos.y + 6.0, at.z), Vec3::NEG_Y, 20.0).map_or(cpos.y, |h| h.point.y);
                if let Some(p) = active.world.player_mut(me) {
                    p.mv.pos = Vec3::new(at.x, gy + 0.05, at.z);
                    p.mv.vel = Vec3::ZERO;
                }
            }
            let eye = active.world.player(me).map(|p| p.eye(active.world.params())).unwrap_or(my_pos);
            let to = center - eye;
            look.yaw = (-to.x).atan2(-to.z);
            look.pitch = (to.y / to.length().max(0.1)).asin();
            if t > 4.8 && sc.step == 1 {
                sc.step = 2;
                let ok = active.world.toggle_transform(me);
                info!("piloting scenario: board key -> {ok}");
            }
        }
        // TRANSFORMER COMBAT strips: a dummy in front; robot strings then Transform (the
        // archetype's transform attack, the finisher follows by itself); beast modes bite, sweep,
        // breathe / roar, then burst out as the robot; the voice shot says it all mid-fight.
        s if combat_shot(s).is_some() => {
            let Some((_, want_robot, how)) = combat_shot(s) else { return };
            face(&mut look, dir, -0.18);
            let Some(dummy) = sc.dummy else { return };
            let (center, half, robot, string_hits) = {
                let Some(c) = active.world.constructs.get(id) else { return };
                (c.center(), c.half, !c.held.vehicle, active.world.cfg.tuning.transformer.robot_row(c).string_hits)
            };
            if t > 1.2 && sc.step == 0 {
                sc.step = 1;
                if robot != want_robot {
                    active.world.toggle_transform(me);
                }
            }
            // The dummy stands in front until the finishing move starts.
            let busy = active.world.constructs.get(id).is_some_and(|c| c.brawl.xatk != bd_sim::constructs::brawl::XPhase::None || c.brawl.order != bd_sim::constructs::brawl::Order::None);
            let hold_until = match how {
                Script::String => 3.75,
                Script::Beast => 5.3,
                Script::Voice => 2.7,
                Script::Sauce => sauce_times(string_hits).1,
            };
            if t > 2.4 && !busy && t < hold_until {
                let at = center + dir * (half.x.max(half.z) + 1.6);
                let gy = bd_core::voxel::raycast(&active.world.voxels, Vec3::new(at.x, center.y + 6.0, at.z), Vec3::NEG_Y, 30.0).map_or(my_pos.y, |h| h.point.y);
                if let Some(p) = active.world.player_mut(dummy) {
                    p.alive = true;
                    p.mv.pos = Vec3::new(at.x, gy + 0.05, at.z);
                    p.mv.vel = Vec3::ZERO;
                    p.damage_pct = 0.0;
                    p.spawn_shield = 0.0;
                }
            }
            let tap = |ks: &[f32]| ks.iter().any(|&k| t > k && t < k + 0.05);
            match how {
                // Four taps of LMB in rhythm (0.33 s apart: a press during the recovery is dropped
                // until stage 3's buffer), then Transform 0.12 s after the ender — inside the V2 flash
                // window.
                Script::String => {
                    ov.held = if tap(&[2.6, 2.93, 3.26, 3.59]) { buttons::PRIMARY } else { 0 };
                    if t > 3.72 && sc.step == 1 {
                        sc.step = 3;
                        let ok = active.world.toggle_transform(me);
                        info!("piloting scenario: transform attack -> {ok}");
                    }
                }
                // Bite, bite, a held tail sweep, held breath (or a roar), a bite, Transform.
                Script::Beast => {
                    let mut held = 0;
                    if tap(&[2.6, 3.0, 5.2]) || (t > 3.3 && t < 3.85) {
                        held |= buttons::PRIMARY;
                    }
                    if t > 4.2 && t < 5.0 {
                        held |= buttons::SECONDARY;
                    }
                    ov.held = held;
                    if t > 5.3 && sc.step == 1 {
                        sc.step = 3;
                        let ok = active.world.toggle_transform(me);
                        info!("piloting scenario: burst out -> {ok}");
                    }
                }
                // Light taps, a held heavy, a blaster shot, the string to its ender, Transform.
                Script::Sauce => {
                    let (ender_at, transform_at) = sauce_times(string_hits);
                    let n = string_hits.max(1) as usize;
                    let light = (n - 1).min(3);
                    let mut held = 0;
                    let taps = (0..light).map(|i| 2.6 + i as f32 * 0.33).chain((0..n).map(|i| ender_at - (n - 1 - i) as f32 * 0.33));
                    if taps.into_iter().any(|k| t > k && t < k + 0.05) || (t > 4.05 && t < 4.75) {
                        held |= buttons::PRIMARY;
                    }
                    if t > 5.0 && t < 5.05 {
                        held |= buttons::SECONDARY;
                    }
                    ov.held = held;
                    if t > transform_at && sc.step == 1 {
                        sc.step = 3;
                        let ok = active.world.toggle_transform(me);
                        info!("piloting scenario: sauce transform attack -> {ok}");
                    }
                }
                // Said, not pressed.
                Script::Voice => {
                    let Some(lex) = lex.as_ref() else { return };
                    let lines: [(f32, &str); 3] = [(2.6, "optimus prime combo"), (6.3, "ion blaster"), (7.3, "transform")];
                    if let Some(&(_, text)) = lines.get(sc.said as usize).filter(|(at, _)| t > *at) {
                        sc.said += 1;
                        say(&mut active, &lex.0, me, text);
                    }
                }
            }
        }
        _ => {}
    }
}
