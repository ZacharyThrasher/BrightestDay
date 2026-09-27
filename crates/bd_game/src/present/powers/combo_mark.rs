//! COMBO TARGET presentation (`bd_sim::powers::combo_target`): the "hunted" mark on a toolbox
//! command's soft target, the attacker's subtle outline, and the hold-to-focus key.
//!
//! * The host replicates each player's target (`PlayerSnap::hunt` → `PlayerView::hunt`).
//! * The TARGET sees the quarry mark on themselves (first person: a small glyph low in the view;
//!   third person: on the body) + a soft tick when it lands — the counterplay cue ("they're
//!   stringing me: dodge / break sight / turn their look"). Everyone else sees the quarry mark on
//!   the target's body (FFA awareness). The ATTACKER sees only a subtle outline on their target.
//! * Prediction: a local toolbox command (typed, spoken, hotbar key) picks its target on the client
//!   with the SAME pure rule as the host (`combo_target::pick`, the command's crosshair + the
//!   interpolated bodies + client voxels; a named player wins), so the outline shows on the command
//!   frame; the replicated truth replaces it as soon as it arrives.
//! * The focus key (`Action::Focus`, unbound by default) → `buttons::FOCUS` while held.

use super::super::sound::Sfx3d;
use super::super::{PresentSet, gv};
use super::movefx::draw::{Look, Pen};
use super::op_visuals::prims::{self, MarkKind, View};
use super::op_visuals::{IMM_BUDGET, LINE_FLOOR};
use crate::camera::{CameraUpdateSet, PlayerCamera};
use crate::input::{CursorCapture, LocalInputState, UiFocus};
use crate::lobby::prefs::{Action, KeyBinds};
use crate::sim_bridge::{ActiveWorld, Cmd, CmdSource, SimCommand, WorldViews};
use crate::state::{AppState, InGameMenu};
use crate::tick::FrameSet;
use crate::viewmodel::{HandsPublishSet, ViewmodelHands};
use bd_core::PlayerId;
use bd_core::input::buttons;
use bd_fx::FxParticles;
use bd_sim::powers::combo_target;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// How long a client-predicted pick shows before the host's truth must confirm it (s).
const PREDICT_SECS: f32 = 0.4;

/// Client-side state.
#[derive(Resource, Debug, Default)]
pub struct ComboMarks {
    /// The local player's predicted target and seconds it still shows unconfirmed.
    pub predicted: Option<(PlayerId, f32)>,
    /// Marked players and how long each has shown (fade-in / spin; small, no allocation after warmup).
    pub shown: Vec<(PlayerId, f32)>,
    /// The local player was hunted last frame (the landing tick).
    was_hunted: bool,
}

pub struct ComboMarkPlugin;

impl Plugin for ComboMarkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ComboMarks>()
            .add_message::<SimCommand>()
            .add_systems(Update, focus_key.in_set(FrameSet::Input))
            .add_systems(Update, predict.in_set(PresentSet::Sync))
            .add_systems(PostUpdate, draw.after(CameraUpdateSet).after(HandsPublishSet).before(TransformSystems::Propagate));
    }
}

/// The focus key → `buttons::FOCUS` while held. While typing in the ring console the bit keeps
/// what it was (a letter bound to it types; a held mouse button keeps focusing).
#[allow(clippy::too_many_arguments)]
fn focus_key(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Option<Res<KeyBinds>>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    focus: Res<UiFocus>,
    capture: Res<CursorCapture>,
    mut input: ResMut<LocalInputState>,
) {
    let in_play = *state.get() == AppState::InGame && !menu.0 && capture.grabbed;
    if in_play && focus.typing {
        return;
    }
    let held = in_play && binds.as_ref().is_some_and(|b| b.pressed(Action::Focus, &keys, &mouse));
    let bits = if held { buttons::FOCUS } else { 0 };
    if input.focus_held != bits {
        input.focus_held = bits;
    }
}

/// The client's own pick for a local toolbox command (same rule as the host).
fn predict(time: Res<Time<Real>>, mut cmds: MessageReader<SimCommand>, active: Option<Res<ActiveWorld>>, views: Res<WorldViews>, input: Res<LocalInputState>, mut marks: ResMut<ComboMarks>) {
    if let Some((_, left)) = marks.predicted.as_mut() {
        *left -= time.delta_secs();
        if *left <= 0.0 {
            marks.predicted = None;
        }
    }
    let Some(active) = active else {
        cmds.clear();
        return;
    };
    let Some(me) = active.local else {
        cmds.clear();
        return;
    };
    for c in cmds.read() {
        let (CmdSource::Local, Cmd::Toolbox { steps, aim }) = (&c.source, &c.cmd) else { continue };
        let mine = views.player(me).and_then(|v| v.hunt);
        let pick = if let Some(t) = combo_target::named_in(steps, aim.as_ref()).filter(|&t| t != me && views.player(t).is_some_and(|v| v.alive())) {
            Some(t)
        } else if input.focus_held != 0 && mine.is_some() {
            mine
        } else if let Some(a) = aim {
            let base = &active.world.cfg.move_params;
            let cands = views.players.iter().filter(|v| v.id != me && v.alive()).map(|v| {
                let hw = base.hull_half_width * v.size;
                let h = base.stand_height * v.size;
                let min = gv(v.pos) - bd_core::Vec3::new(hw, 0.0, hw);
                let max = gv(v.pos) + bd_core::Vec3::new(hw, h, hw);
                (v.id, bd_core::voxel::Aabb { min, max })
            });
            combo_target::pick(a.origin, a.dir, cands, |x, y| combo_target::clear_line(&active.world.voxels, x, y))
        } else {
            None
        };
        marks.predicted = pick.map(|t| (t, PREDICT_SECS));
    }
}

fn quarry_look() -> Look {
    Look { main: [1.0, 0.32, 0.1], core: [1.0, 0.86, 0.62], dark: [0.25, 0.04, 0.02], glow: 2.2, ..Look::default() }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mut marks: ResMut<ComboMarks>,
    mut fx: ResMut<FxParticles>,
    hands: Res<ViewmodelHands>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else { return };
    let local = active.local;
    let dt = time.delta_secs().min(0.1);
    let marks = &mut *marks;
    // The local attacker's target: the host's truth, else the prediction.
    let own = local.and_then(|me| views.player(me)).and_then(|v| v.hunt).or(marks.predicted.map(|p| p.0));
    if views.player(local.unwrap_or(PlayerId(u8::MAX))).is_some_and(|v| v.hunt.is_some()) {
        marks.predicted = None;
    }
    let hunted = |id: PlayerId| views.players.iter().any(|h| h.alive() && h.hunt == Some(id) && Some(h.id) != local);
    let any = own.is_some() || views.players.iter().any(|v| v.hunt.is_some());
    if !any {
        marks.shown.clear();
        marks.was_hunted = false;
        return;
    }
    // age the shown list (targets that stopped being marked drop out)
    marks.shown.retain(|(id, _)| hunted(*id) || own == Some(*id));
    for (_, t) in marks.shown.iter_mut() {
        *t += dt;
    }
    let v = cams.iter().next().map_or(View::default(), |g| View { cam: g.translation(), right: g.right().as_vec3(), up: g.up().as_vec3(), fwd: g.forward().as_vec3() });
    let look = quarry_look();
    let mut pen = Pen { fx: &mut fx, motes: 1.0, scale: 1.0, cam: v.cam, avoid: None };
    pen.fx.scope(v.cam, LINE_FLOOR, IMM_BUDGET / 8);
    let me_hunted = local.is_some_and(hunted);
    for tv in views.players.iter().filter(|p| p.alive()) {
        let by_others = hunted(tv.id);
        let by_me = own == Some(tv.id);
        if !by_others && !by_me {
            continue;
        }
        let t = match marks.shown.iter().find(|s| s.0 == tv.id) {
            Some(s) => s.1,
            None => {
                marks.shown.push((tv.id, 0.0));
                0.0
            }
        };
        if Some(tv.id) == local {
            if hands.active {
                prims::self_marker(&mut pen, &v, &look, MarkKind::Quarry, t, 9.0, 9.0, 1.0);
            } else {
                prims::marker(&mut pen, &v, &look, MarkKind::Quarry, tv.pos, tv.size, t, 9.0, 9.0, 1.0, tv.id.0 as u32);
            }
        } else if by_others {
            prims::marker(&mut pen, &v, &look, MarkKind::Quarry, tv.pos, tv.size, t, 9.0, 9.0, 1.0, tv.id.0 as u32);
        } else {
            prims::quarry_outline(&mut pen, &v, &look, tv.pos, tv.size, t);
        }
    }
    pen.fx.end_scope();
    if me_hunted && !marks.was_hunted {
        // the landing tick: you are someone's quarry
        let at = local.and_then(|me| views.player(me)).map_or(v.cam, |p| p.pos + Vec3::Y);
        sfx.write(Sfx3d::at("needle_tick", at).pitch(0.7).gain(0.6));
    }
    marks.was_hunted = me_hunted;
}
