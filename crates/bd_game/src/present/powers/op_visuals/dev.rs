//! OP VISUALS dev hooks for the movefx `phrase` scene (never set in play):
//!
//! * `BD_OPVIS_PRE="<move phrase>"` — before the actor fires `BD_MOVEFX_PHRASE`, the LAST target bot
//!   fires this one (at t = 1.9 s, aimed along its facing): something for the actor's move to act
//!   on (a summoned creature for "cappy throw" to capture, a shield for a hook to bounce off…).
//! * `BD_OPVIS_THEN="<move phrase>"` + `BD_OPVIS_THEN_AT=<s>` (default 1.5) — the local player fires a
//!   second phrase that long after the scene's move (a tag, then — seconds later — its strike).
//! * `BD_OPVIS_OFF=1` — no op visuals at all (A/B frames).
//! * `BD_OPVIS_LOG=1` — log every op event as it arrives; `BD_OPVIS_STATS=1` — log `OpVisStats`
//!   (live entries, peak, step µs) once a second.

use super::OpVisStats;
use crate::sim_bridge::{ActiveWorld, Cmd, SimCommand};
use crate::state::AppState;
use bevy::prelude::*;

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

/// `BD_OPVIS_OFF=1`: no op visuals at all (A/B frames).
pub fn off() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| env("BD_OPVIS_OFF").is_some())
}

/// `BD_OPVIS_LOG` (dev scenes only).
pub fn log_events() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| env("BD_MOVEFX_TEST").is_some() && env("BD_OPVIS_LOG").is_some())
}

#[derive(Resource, Default)]
struct Dev {
    t: f32,
    sent: bool,
    then_sent: bool,
    log_t: f32,
}

pub fn build(app: &mut App) {
    if env("BD_MOVEFX_TEST").is_none() || (env("BD_OPVIS_PRE").is_none() && env("BD_OPVIS_STATS").is_none() && env("BD_OPVIS_THEN").is_none()) {
        return;
    }
    app.init_resource::<Dev>().add_systems(Update, dev.run_if(in_state(AppState::InGame)));
}

fn steps_of(text: &str) -> Vec<bd_sim::powers::chain::ToolStep> {
    use bd_sim::powers::toolbox::{MoveIndex, split_chain};
    let ix = MoveIndex::shipped();
    let t = bd_sim::powers::PowerTable::get();
    let is_move = |s: &str| ix.read(t, None, s).is_move();
    split_chain(text, &is_move)
        .into_iter()
        .filter_map(|s| {
            let r = ix.read(t, None, &s);
            let kit = matches!(r, bd_sim::powers::toolbox::Reading::Kit(_));
            r.move_ref().map(|mv| bd_sim::powers::chain::ToolStep::Move { mv, kit })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn dev(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    mut st: ResMut<Dev>,
    stats: Res<OpVisStats>,
    ult: Res<super::super::ultimate::UltimateStats>,
    mv: Res<super::super::movefx::MoveFxStats>,
    mut cmds: MessageWriter<SimCommand>,
) {
    let Some(active) = active else { return };
    st.t += time.delta_secs();
    if env("BD_OPVIS_STATS").is_some() && st.t - st.log_t >= 1.0 {
        st.log_t = st.t;
        info!("op_visuals stats t={:.1}: live {} peak {} started {} step {:.0} us imm_dropped {}", st.t, stats.live, stats.peak, stats.started, stats.step_us, stats.imm_dropped);
        info!("presentation stats t={:.1}: movefx live {} step {:.0} us | ultimate live {} started {} step {:.0} us", st.t, mv.live, mv.step_us, ult.live, ult.started, ult.step_us);
    }
    // The scene's own move goes at t ≈ 2.6 s (movefx dev).
    let then_at = 2.62 + env("BD_OPVIS_THEN_AT").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.5);
    if !st.then_sent && st.t >= then_at && let Some(text) = env("BD_OPVIS_THEN") {
        st.then_sent = true;
        let steps = steps_of(&text);
        info!("op_visuals dev: local fires {text:?} ({} step(s))", steps.len());
        cmds.write(SimCommand::local(Cmd::Toolbox { steps, aim: None }));
    }
    if st.sent || st.t < 1.9 {
        return;
    }
    let Some(text) = env("BD_OPVIS_PRE") else { return };
    st.sent = true;
    let Some(p) = active.world.players.iter().filter(|p| Some(p.id) != active.local && p.alive).last() else { return };
    let steps = steps_of(&text);
    let eye = p.eye(active.world.params());
    let aim = bd_sim::Aim::new(eye, bd_core::input::look_dir(p.input.yaw, -0.05));
    info!("op_visuals dev: {:?} fires {text:?} ({} step(s))", p.id, steps.len());
    cmds.write(SimCommand::remote(p.id, Cmd::Toolbox { steps, aim: Some(aim) }));
}
