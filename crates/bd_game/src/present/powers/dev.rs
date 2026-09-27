//! Dev harness for power screenshots / frame strips:
//! `BD_POWERS_TEST="<secs>|<who>|<cmd>;..."` (practice, `--bots N`).
//!
//! * `who`: `me` (optionally `me@<yaw_deg>,<pitch_deg>`: hold the look, relative to the spawn
//!   facing, for 4 s), or `bot<N>@<metres>,<lateral>` — pin bot N that far in front of the camera
//!   (lateral m to the side), facing the camera, until the plan ends (`bot<N>` alone: already pinned).
//! * `cmd`: `!grant <key>` (e.g. `super_saiyan@4`), `!move <slot>` (a power move / selector byte;
//!   bots aim at the local player), `!tier <n>`, `!swap` (G double-tap), `!costume <phrase>` (a bot
//!   wears that costume, e.g. `goku`: costume-tint shots), `!say <phrase>` (a bot says a phrase: the
//!   ring's resolver — power phrase, then the lexicon — and its cast), `!line <text>` (a bot types a
//!   whole toolbox line: borrowed moves, chains, twisted casts "giant spirit bomb"), or — for `me` — any phrase
//!   through the real ring path (lexicon, power shouts, tiers).
//!
//! Bots stand down (AI off) once the plan starts. Everyone's willpower is topped up per command.

use crate::sim_bridge::{ActiveWorld, Cmd, SimCommand};
use crate::state::AppState;
use bd_core::PlayerId;
use bd_sim::powers::move_sel;
use bevy::prelude::*;

#[derive(Resource, Default)]
struct PowersDev {
    t: f32,
    next: usize,
    started: bool,
    /// (bot, metres ahead, lateral, placed-until).
    pins: Vec<(PlayerId, f32, f32, f32)>,
    look: Option<(f32, f32, f32)>,
    base: Option<(f32, bd_core::Vec3)>,
    refill: bool,
    /// Commands from the plan, sent by `dev_world` (the Caster owns the SimCommand writer).
    queue: Vec<SimCommand>,
}

pub fn build(app: &mut App) {
    if std::env::var_os("BD_POWERS_TEST").is_none() {
        return;
    }
    app.init_resource::<PowersDev>().add_systems(Update, (dev_world, dev_plan).chain().run_if(in_state(AppState::InGame)));
}

fn plan() -> &'static [(f32, String, String)] {
    static PLAN: std::sync::OnceLock<Vec<(f32, String, String)>> = std::sync::OnceLock::new();
    PLAN.get_or_init(|| {
        std::env::var("BD_POWERS_TEST")
            .unwrap_or_default()
            .split(';')
            .filter_map(|e| {
                let mut it = e.splitn(3, '|');
                let at: f32 = it.next()?.trim().parse().ok()?;
                let who = it.next()?.trim().to_string();
                let cmd = it.next()?.trim().to_string();
                (!cmd.is_empty()).then_some((at, who, cmd))
            })
            .collect()
    })
}

/// World side: start (bots stand down), the held look, pinned bots, willpower top-ups.
fn dev_world(
    time: Res<Time<Real>>,
    active: Option<ResMut<ActiveWorld>>,
    mut look: ResMut<crate::input::LookAngles>,
    mut dev: ResMut<PowersDev>,
    mut cmds: MessageWriter<SimCommand>,
) {
    for c in dev.queue.drain(..) {
        cmds.write(c);
    }
    let Some(mut active) = active else { return };
    dev.t += time.delta_secs();
    let Some(me) = active.local_player().map(|p| (p.id, p.mv.pos, p.alive)) else { return };
    if !me.2 {
        return;
    }
    if !dev.started {
        if dev.t < 0.6 {
            return;
        }
        dev.started = true;
        active.bots.clear();
        dev.base = Some((look.yaw, me.1));
    }
    let (base_yaw, origin) = dev.base.unwrap_or((look.yaw, me.1));
    if let Some((yaw, pitch, until)) = dev.look {
        if dev.t < until {
            look.yaw = base_yaw + yaw.to_radians();
            look.pitch = pitch.to_radians();
        } else {
            dev.look = None;
        }
    }
    // Pinned bots face the camera from in front of the spawn.
    let fwd = bd_core::input::look_dir(base_yaw, 0.0);
    let fwd = bd_core::Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(bd_core::Vec3::NEG_Z);
    let side = bd_core::Vec3::new(-fwd.z, 0.0, fwd.x);
    let pins = dev.pins.clone();
    let now = dev.t;
    for (bot, d, lat, until) in pins {
        let want = origin + fwd * d + side * lat;
        let ground = bd_core::voxel::raycast(&active.world.voxels, want + bd_core::Vec3::Y * 12.0, bd_core::Vec3::NEG_Y, 40.0).map(|h| h.point.y);
        let to_me = me.1 - want;
        let yaw = (-to_me.x).atan2(-to_me.z);
        if let Some(p) = active.world.player_mut(bot).filter(|p| p.alive) {
            // Placed for a moment, then free (a push / pull / core moves them for real).
            if now < until {
                p.mv.pos = bd_core::Vec3::new(want.x, ground.map_or(p.mv.pos.y, |g| g + 0.05), want.z);
                p.mv.vel = bd_core::Vec3::ZERO;
            }
            p.input.yaw = yaw;
            p.input.pitch = 0.0;
            p.spawn_shield = 0.0;
        }
    }
    if dev.refill {
        dev.refill = false;
        for p in active.world.players.iter_mut() {
            p.willpower = p.willpower.max(100.0);
        }
    }
}

/// Plan side: the commands (the local player's phrases go through the real ring path).
fn dev_plan(
    active: Option<Res<ActiveWorld>>,
    mut dev: ResMut<PowersDev>,
    mut caster: crate::console::Caster,
    mut costumes: ResMut<crate::present::players::PlayerCostumes>,
    lex: Option<Res<crate::console::RingLexicon>>,
) {
    let Some(active) = active else { return };
    let plan = plan();
    if !dev.started {
        return;
    }
    let Some(me) = active.local_player().map(|p| (p.id, p.mv.pos)) else { return };
    while let Some((at, who, cmd)) = plan.get(dev.next) {
        if dev.t < *at {
            break;
        }
        dev.next += 1;
        dev.refill = true;
        let (who, spec) = match who.split_once('@') {
            Some((w, s)) => (w.to_string(), Some(s.to_string())),
            None => (who.clone(), None),
        };
        let nums: Vec<f32> = spec.as_deref().unwrap_or("").split(',').filter_map(|x| x.trim().parse().ok()).collect();
        let (id, is_me) = if who == "me" {
            if let [yaw, pitch] = nums[..] {
                dev.look = Some((yaw, pitch, dev.t + 4.0));
            }
            (me.0, true)
        } else {
            let Some(n) = who.strip_prefix("bot").and_then(|n| n.parse::<u8>().ok()) else { continue };
            if let [d, lat] = nums[..] {
                dev.pins.retain(|p| p.0 != PlayerId(n));
                let until = dev.t + 0.4;
                dev.pins.push((PlayerId(n), d, lat, until));
            }
            (PlayerId(n), false)
        };
        let aim = if is_me {
            None
        } else {
            active.world.player(id).map(|p| {
                let eye = p.eye(active.world.params());
                bd_sim::Aim::new(eye, (me.1 + bd_core::Vec3::Y * 1.0 - eye).normalize_or(bd_core::Vec3::NEG_Z))
            })
        };
        let send = |c: Cmd| if is_me { SimCommand::local(c) } else { SimCommand::remote(id, c) };
        info!("powers test {:.2}: {who} {cmd}", dev.t);
        if let Some(key) = cmd.strip_prefix("!grant ") {
            dev.queue.push(send(Cmd::GrantPower(key.trim().to_string())));
        } else if let Some(s) = cmd.strip_prefix("!move ").and_then(|s| s.trim().parse::<u8>().ok()) {
            dev.queue.push(send(Cmd::PowerMove(s, aim)));
        } else if let Some(t) = cmd.strip_prefix("!tier ").and_then(|s| s.trim().parse::<u8>().ok()) {
            dev.queue.push(send(Cmd::PowerMove(move_sel::select(0, t), aim)));
        } else if let Some(phrase) = cmd.strip_prefix("!line ").filter(|_| !is_me) {
            // A bot types a whole TOOLBOX line (borrowed / kit moves, chains, twisted / fused moves:
            // "giant spirit bomb", "homing rasengan -> chidori").
            let Some(lex) = lex.as_ref() else { continue };
            if let Some(steps) = crate::toolbox::command(&lex.0, None, true, phrase.trim(), false) {
                dev.queue.push(send(Cmd::Toolbox { steps, aim }));
            }
        } else if let Some(phrase) = cmd.strip_prefix("!say ").filter(|_| !is_me) {
            let Some(lex) = lex.as_ref() else { continue };
            let power = active.world.parse_power_phrase(id, phrase.trim());
            match crate::console::resolve_phrase(&lex.0, power, phrase.trim(), false) {
                crate::console::Resolved::PowerMove(slot) => dev.queue.push(send(Cmd::PowerMove(slot, aim))),
                crate::console::Resolved::Cast { res, incantation } => {
                    if let Some(aim) = aim {
                        let res = *res;
                        dev.queue.push(send(Cmd::Cast { recipe: Box::new(res.recipe), shape: res.shape, alt_shape: res.alt_shape, aim, incantation }));
                    }
                }
                crate::console::Resolved::Rush(_) => {}
            }
        } else if let Some(phrase) = cmd.strip_prefix("!costume ").filter(|_| !is_me) {
            costumes.0.insert(id, crate::present::players::CostumeChoice::phrase(phrase.trim(), 0));
        } else if cmd == "!swap" {
            dev.queue.push(send(Cmd::PowerMove(move_sel::CYCLE, aim)));
        } else if is_me {
            caster.cast(cmd, false, 0);
        }
    }
}
