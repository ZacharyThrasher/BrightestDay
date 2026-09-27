//! CLICK TO CONFIRM — the ring's ghost in the world ([`super`] has the rules).
//!
//! * **A construct** (a cast, a signature, a chain's first cast): its OWN model — the same
//!   `ModelCache` entry the real construct will use (built once on a worker; the confirmed cast
//!   forms without a second build) — as one translucent hard-light `ConstructVisual` in the corps
//!   colour, streaming out of the ring, at its real size and the exact spot the cast would stand
//!   (`World::preview_spawn`: the sim's own placement, read-only; re-placed at ≤ 20 Hz as the
//!   crosshair moves, eased so it glides). Held / worn things float before the ring. Aimed at an
//!   own construct (it would graft) or at terrain with a modifier: no body, only the ring's arc.
//! * **A move / a chain** (named, borrowed, kit, held, described, twisted): each step's aim ghost
//!   from the move grammar's primitives in the move's own style colour — a blink's silhouette where
//!   you'd land, a dash / leap arc, a shove cone, a field / slam ring, a blast's telegraph, a reach
//!   line to the crosshair — chained from where the previous step ends (`hud` shows the pips).
//! * Immediate segments only for moves (no entities); one entity for a body. Fades in, breathes,
//!   thins as it times out; on confirm it dissolves as the real one forms from the ring.

use super::{ConfirmGate, Ended};
use crate::camera::{CameraUpdateSet, PlayerCamera, ViewAnchor};
use crate::input::LookAngles;
use crate::present::constructs::{hand_transform, ModelCache};
use crate::present::powers::movefx::draw::{circle, Look, Pen};
use crate::present::powers::movefx::table::MoveFx;
use crate::present::powers::movefx::{alias_op, resolve_look};
use crate::present::powers::op_visuals::prims::{self, StrikeKind, View};
use crate::present::{bq, bv, corps_rgb, gv};
use crate::sim_bridge::{ActiveWorld, Cmd, WorldViews};
use crate::viewmodel::{HandsPublishSet, ViewmodelHands};
use bd_core::mechanics::{FormCategory, Recipe};
use bd_core::shape::ShapeSpec;
use bd_fx::{AutoForm, ConstructEnd, ConstructVisual, FxParticles};
use bd_sim::powers::chain::ToolStep;
use bd_sim::powers::{At, MoveDef, MoveOp, PowerDef, PowerTable, compose, move_sel, twist};
use bd_sim::signature::SignatureCast;
use bd_sim::{Aim, TargetHint};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use std::f32::consts::TAU;

/// Ghost body opacity (translucent hard light) and its dim in the opaque style.
const GHOST_OPACITY: f32 = 0.2;
const OPAQUE_DIM: f32 = 0.5;
/// Re-place the body at most this often (s) while the crosshair moves.
const REPLACE_SECS: f32 = 0.05;

pub(super) fn build(app: &mut App) {
    app.init_resource::<GhostState>().add_systems(
        PostUpdate,
        (ghost_body, ghost_moves).chain().after(CameraUpdateSet).after(HandsPublishSet).before(TransformSystems::Propagate),
    );
}

/// The ghost body entity.
#[derive(Component)]
pub struct RingGhost;

/// What a body ghost is built from.
struct Body {
    recipe: Recipe,
    shape: Option<ShapeSpec>,
    alt: Option<ShapeSpec>,
    sig: Option<SignatureCast>,
    hint: Option<TargetHint>,
}

#[derive(Resource, Default)]
struct GhostState {
    serial: u32,
    body: Option<Body>,
    /// The validated recipe / normalised shape the model is built from (first placement).
    model: Option<(Recipe, Option<ShapeSpec>)>,
    ent: Option<Entity>,
    target: Option<Transform>,
    since: f32,
    ray: (Vec3, Vec3),
    t: f32,
}

fn body_of(cmds: &[Cmd]) -> Option<Body> {
    for c in cmds {
        match c {
            Cmd::Cast { recipe, shape, alt_shape, aim, .. } if !aim.modifier => {
                return Some(Body { recipe: (**recipe).clone(), shape: shape.clone(), alt: alt_shape.clone(), sig: None, hint: aim.target_hint });
            }
            Cmd::Signature { recipe, shape, aim, sig } => return Some(Body { recipe: (**recipe).clone(), shape: shape.clone(), alt: None, sig: Some(*sig), hint: aim.target_hint }),
            Cmd::Toolbox { steps, .. } => {
                for s in steps {
                    let cast = match s {
                        ToolStep::Cast { recipe, shape, alt_shape, signature, .. } => Some((recipe, shape, alt_shape, signature)),
                        _ => None,
                    };
                    if let Some((recipe, shape, alt, sig)) = cast {
                        return Some(Body { recipe: (**recipe).clone(), shape: shape.clone(), alt: alt.clone(), sig: *sig, hint: None });
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// The crosshair ray the Caster would aim along (`Caster::crosshair`).
fn crosshair(ray: Option<&crate::present::camera3p::AimRay>, anchor: &ViewAnchor, look: &LookAngles) -> (Vec3, Vec3) {
    match ray {
        Some(r) if r.dir.is_finite() && r.dir.length_squared() > 0.5 => (r.origin, r.dir),
        _ => (anchor.eye, look.forward()),
    }
}

#[allow(clippy::too_many_arguments)]
fn ghost_body(
    mut commands: Commands,
    time: Res<Time<Real>>,
    gate: Res<ConfirmGate>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    anchor: Res<ViewAnchor>,
    look: Res<LookAngles>,
    ray: Option<Res<crate::present::camera3p::AimRay>>,
    hands: Option<Res<ViewmodelHands>>,
    fxs: Option<Res<bd_fx::FxSettings>>,
    cache: Option<ResMut<ModelCache>>,
    mut st: ResMut<GhostState>,
    mut q: Query<(&mut ConstructVisual, &mut Transform, &mut Visibility), With<RingGhost>>,
) {
    let dt = time.delta_secs().min(0.1);
    let pending = gate.pending.as_ref();
    // The previous ghost ended (confirmed / dropped / replaced / timed out): its exit.
    if st.serial != 0 && pending.is_none_or(|p| p.serial != st.serial) {
        if let Some(e) = st.ent.take() {
            let secs = match gate.ended {
                Some((Ended::Confirmed, s)) if s == st.serial => 0.12, // the real one is forming here
                Some((Ended::Expired, s)) if s == st.serial => 0.6,
                Some((Ended::Cancelled, s)) if s == st.serial => 0.3,
                _ => 0.1,
            };
            commands.entity(e).remove::<RingGhost>().try_insert(ConstructEnd::Dismiss { duration: secs });
        }
        *st = GhostState::default();
    }
    let (Some(p), Some(active), Some(mut cache)) = (pending, active, cache) else { return };
    let Some(me) = active.local else { return };
    let st = &mut *st;
    let fp_ring = hands.as_ref().filter(|h| h.active).map(|h| h.world[1]);
    if st.serial != p.serial {
        st.serial = p.serial;
        st.body = body_of(&p.cmds);
        st.since = f32::MAX;
    }
    let Some(body) = st.body.as_ref() else { return };
    st.t += dt;
    st.since += dt;
    let (o, d) = crosshair(ray.as_deref(), &anchor, &look);
    let local = views.player(me);
    // Where the ring is (the ghost streams out of it; the energy arc leads to it).
    let ring = fp_ring.unwrap_or_else(|| local.map_or(o, |v| hand_transform(v).translation));
    let moved = st.ray.0.distance(o) > 0.05 || st.ray.1.angle_between(d) > 0.004;
    if st.since >= REPLACE_SECS && (moved || st.since > 0.25) {
        st.since = 0.0;
        st.ray = (o, d);
        let category = body.recipe.form.category();
        if matches!(category, FormCategory::Held | FormCategory::Worn) {
            // Held / worn: floats before the ring (the real one appears in the hand / on the back).
            let at = if fp_ring.is_some() { o + d * 1.5 - Vec3::Y * 0.2 } else { ring + d * 0.9 };
            st.target = Some(Transform::from_translation(at).looking_to(d, Vec3::Y));
            if st.model.is_none() {
                st.model = Some((bd_sim::willpower::validate_recipe(body.recipe.clone(), &active.world.cfg.rules), body.shape.clone()));
            }
        } else {
            let aim = Aim { origin: gv(o), dir: gv(d), target_hint: body.hint, modifier: false };
            let pv = match body.sig {
                Some(sig) => active.world.preview_signature_spawn(me, body.recipe.clone(), body.shape.clone(), aim, sig),
                None => active.world.preview_spawn(me, body.recipe.clone(), body.shape.clone(), aim),
            };
            st.target = pv.as_ref().map(|pv| Transform::from_translation(bv(pv.pos)).with_rotation(bq(pv.rot)));
            if st.model.is_none()
                && let Some(pv) = pv
            {
                st.model = Some((pv.recipe, pv.shape));
            }
        }
    }
    let life = p.life();
    let corps = corps_rgb(active.corps_of(me));
    let Some(target) = st.target else {
        // (would graft / animate terrain: no body)
        if let Some(e) = st.ent
            && let Ok((_, _, mut vis)) = q.get_mut(e)
        {
            *vis = Visibility::Hidden;
        }
        return;
    };
    match st.ent {
        None => {
            let Some((r, shape)) = st.model.as_ref() else { return };
            let Some(built) = cache.request(r, shape.as_ref(), body.alt.as_ref()) else { return };
            let mut vis = ConstructVisual::new(built.model.clone(), corps).forming_from(ring);
            if let Some(k) = built.key.clone() {
                vis = vis.with_key(k);
            }
            if let Some(c) = built.colors.clone() {
                vis = vis.with_colors(c);
            }
            if let Some(a) = r.accent {
                vis = vis.with_accent(Color::srgb(a[0], a[1], a[2]).to_linear().to_f32_array_no_alpha());
            }
            vis.opacity = GHOST_OPACITY;
            vis.seed = 0x6057_u32.wrapping_add(st.serial);
            let e = commands.spawn((Name::new("RingGhost"), RingGhost, AutoForm::new(0.22), vis, target)).id();
            st.ent = Some(e);
        }
        Some(e) => {
            let Ok((mut vis, mut tf, mut shown)) = q.get_mut(e) else { return };
            if *shown != Visibility::Inherited {
                *shown = Visibility::Inherited;
                *tf = target;
            }
            // Glide to the new spot (never lag the crosshair by much).
            let k = 1.0 - (-20.0 * dt).exp();
            tf.translation = tf.translation.lerp(target.translation, k);
            tf.rotation = tf.rotation.slerp(target.rotation, k);
            tf.scale = target.scale;
            let opaque = fxs.as_ref().is_some_and(|f| !f.translucent);
            let breathe = 0.5 + 0.5 * (st.t * 3.2).sin();
            vis.opacity = (GHOST_OPACITY * (0.35 + 0.65 * life)).max(0.05);
            vis.dim = if opaque { OPAQUE_DIM + 0.3 * (1.0 - life) } else { 0.15 * (1.0 - life) };
            vis.flash = (0.03 + 0.07 * breathe) * life;
            vis.ring = Some(ring);
            vis.arc = life > 0.3;
        }
    }
}

// ------------------------------------------------------------------------------------ move ghosts

/// Every move step of a capture, in order (a construct step is the body ghost's).
fn each_move(cmds: &[Cmd], w: &bd_sim::World, me: bd_core::PlayerId, mut f: impl FnMut(&'static MoveDef, Option<&'static PowerDef>)) {
    let t = PowerTable::get();
    let held = |sel: u8| -> Option<(&'static MoveDef, Option<&'static PowerDef>)> {
        if sel & move_sel::SELECT != 0 || sel == move_sel::CYCLE {
            return None;
        }
        let (h, s) = move_sel::split_move_of(sel);
        let p = w.player(me)?;
        let hv = p.power.held().find(|x| x.slot == h)?;
        let def: &'static PowerDef = hv.def;
        def.moves_at(hv.tier).get(s as usize).map(|m| (m, Some(def)))
    };
    let base = |b: &twist::TwistBase| -> Option<(&'static MoveDef, Option<&'static PowerDef>)> {
        match b {
            twist::TwistBase::Move { mv, .. } => mv.resolve(t).map(|(p, m)| (m, Some(p))),
            twist::TwistBase::Held(sel) => held(*sel),
            twist::TwistBase::Composed(c) => compose::interned(c).map(|m| (m, Some(compose::composed_power()))),
            _ => None,
        }
    };
    for c in cmds {
        match c {
            Cmd::PowerMove(sel, _) => {
                if let Some((m, p)) = held(*sel) {
                    f(m, p);
                }
            }
            Cmd::Toolbox { steps, .. } => {
                for s in steps {
                    let got = match s {
                        ToolStep::Held(sel) => held(*sel),
                        ToolStep::Move { mv, .. } => mv.resolve(t).map(|(p, m)| (m, Some(p))),
                        ToolStep::Composed(c) => compose::interned(c).map(|m| (m, Some(compose::composed_power()))),
                        ToolStep::Twist(tw) => base(&tw.a.base),
                        ToolStep::Cast { .. } => None,
                    };
                    if let Some((m, p)) = got {
                        f(m, p);
                    }
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ghost_moves(
    time: Res<Time<Real>>,
    gate: Res<ConfirmGate>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    anchor: Res<ViewAnchor>,
    look: Res<LookAngles>,
    ray: Option<Res<crate::present::camera3p::AimRay>>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    table: Option<Res<MoveFx>>,
    settings: Option<Res<bd_fx::FxSettings>>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    fx: Option<ResMut<FxParticles>>,
    mut t: Local<(u32, f32)>,
) {
    let (Some(p), Some(active), Some(table), Some(mut fx)) = (gate.pending.as_ref(), active.as_deref(), table, fx) else { return };
    let Some(me) = active.local else { return };
    let Some(v) = views.player(me) else { return };
    if t.0 != p.serial {
        *t = (p.serial, 0.0);
    }
    t.1 += time.delta_secs().min(0.1);
    let tt = t.1;
    let view = cams.iter().next().map_or(View::default(), |g| View { cam: g.translation(), right: g.right().as_vec3(), up: g.up().as_vec3(), fwd: g.forward().as_vec3() });
    let (o, d) = crosshair(ray.as_deref(), &anchor, &look);
    let aim_point = ray.as_deref().map_or(o + d * 30.0, |r| r.point);
    let flat = Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::NEG_Z);
    let corps = corps_rgb(active.corps_of(me));
    let a = p.life() * (tt / 0.15).min(1.0);
    let budget = settings.as_ref().map_or(1.0, |s| s.particle_budget).clamp(0.15, 1.0);
    let mut pen = Pen { fx: &mut fx, motes: table.t.beats.motes * budget, scale: 1.0, cam: view.cam, avoid: None };
    let ground = |q: Vec3| -> Option<Vec3> {
        let a = arena.as_deref()?;
        bd_core::voxel::raycast(&*a.read(), gv(q + Vec3::Y * 1.5), bd_core::Vec3::NEG_Y, 40.0).map(|h| bv(h.point))
    };
    let mut from = v.pos;
    let mut k = 0u32;
    each_move(&p.cmds, &active.world, me, |mv, power| {
        let mut look = resolve_look(&table, mv, power, corps);
        look.glow *= 0.75 * a;
        from = draw_move(&mut pen, &view, &look, mv, from, o, d, flat, aim_point, v.size, tt + k as f32 * 0.37, &ground);
        k += 1;
    });
}

/// A dashed line (every other of `n` pieces), drifting with `phase`.
#[allow(clippy::too_many_arguments)]
fn dash(pen: &mut Pen, v: &View, a: Vec3, b: Vec3, n: usize, thick: f32, color: [f32; 3], glow: f32, phase: f32) {
    let n = n.max(2);
    let off = phase.rem_euclid(1.0) / n as f32;
    for i in (0..n).step_by(2) {
        let f0 = (i as f32 / n as f32 + off).min(1.0);
        let f1 = ((i + 1) as f32 / n as f32 + off).min(1.0);
        prims::seg(pen, v, a.lerp(b, f0), a.lerp(b, f1), thick, color, glow);
    }
}

/// Where you'd stand: a hard-light outline of a body at `feet` (ground ring, spine, shoulders, head).
fn silhouette(pen: &mut Pen, v: &View, look: &Look, feet: Vec3, size: f32, t: f32) {
    let s = size.max(0.3);
    let g = look.glow;
    prims::dashed_ring(pen, v, feet + Vec3::Y * 0.06, Vec3::Y, 0.55 * s, 16, 0.04, look.main, g * 0.8, t * 0.8);
    dash(pen, v, feet + Vec3::Y * 0.1 * s, feet + Vec3::Y * 1.38 * s, 8, 0.035, look.main, g * 0.7, t * 0.6);
    let sh = feet + Vec3::Y * 1.36 * s;
    let side = v.right * 0.3 * s;
    prims::seg(pen, v, sh - side, sh + side, 0.035, look.main, g * 0.7);
    let head = feet + Vec3::Y * 1.64 * s;
    circle(pen.fx, head, (head - v.cam).normalize_or(v.fwd), 0.2 * s, 14, 0.035, look.core, g * 0.9, t);
}

/// One move's aim ghost from `from` (feet); returns where the mover would stand after it.
#[allow(clippy::too_many_arguments)]
fn draw_move(pen: &mut Pen, v: &View, look: &Look, mv: &MoveDef, from: Vec3, eye: Vec3, dir: Vec3, flat: Vec3, aim_point: Vec3, size: f32, t: f32, ground: &dyn Fn(Vec3) -> Option<Vec3>) -> Vec3 {
    let g = look.glow;
    let chest = from + Vec3::Y * 1.1 * size;
    let at = |a: At, dist: f32| match a {
        At::Me => from,
        At::Aim => aim_point,
        At::Front => from + flat * dist,
    };
    let reach = |range: f32| {
        let to = aim_point - eye;
        if to.length() <= range { aim_point } else { eye + dir * range }
    };
    let mut end = from;
    let mut drew = false;
    for op in &mv.ops {
        let op = alias_op(op).unwrap_or_else(|| op.clone());
        drew = true;
        match op {
            MoveOp::Blink { dist, .. } => {
                let to = from + flat * dist.clamp(0.5, 60.0);
                let to = ground(to).filter(|q| (q.y - to.y).abs() < 6.0).unwrap_or(to);
                dash(pen, v, chest, to + Vec3::Y * 1.1 * size, 12, 0.03, look.main, g * 0.5, t * 1.5);
                silhouette(pen, v, look, to, size, t);
                end = to;
            }
            MoveOp::Impulse { fwd, up } => {
                let to = from + flat * (fwd * 0.55).clamp(1.0, 40.0);
                let gd = ground(to);
                prims::arc_preview(pen, v, look, from + Vec3::Y * 0.3, gd.unwrap_or(to), (up * 0.3).clamp(0.5, 12.0), gd, 0.0, t);
                end = gd.unwrap_or(to);
            }
            MoveOp::Reel { range, .. } => {
                let tip = reach(range);
                dash(pen, v, chest, tip, 16, 0.035, look.main, g * 0.7, t * 2.0);
                prims::dashed_ring(pen, v, tip, (tip - v.cam).normalize_or(v.fwd), 0.35, 10, 0.04, look.core, g, t);
                end = ground(tip).unwrap_or(tip);
            }
            MoveOp::Link { range } | MoveOp::Swap { range } | MoveOp::Yank { range, .. } | MoveOp::Weigh { range, .. } => {
                let tip = reach(range);
                dash(pen, v, chest, tip, 16, 0.03, look.main, g * 0.6, -t * 2.0);
                prims::dashed_ring(pen, v, tip, (tip - v.cam).normalize_or(v.fwd), 0.5, 12, 0.04, look.core, g, t);
            }
            MoveOp::Shove { range, angle, .. } => {
                let half = (angle.clamp(10.0, 170.0) * 0.5).to_radians();
                let base = from + Vec3::Y * 0.08;
                let l = Quat::from_rotation_y(half) * flat;
                let r = Quat::from_rotation_y(-half) * flat;
                dash(pen, v, base, base + l * range, 8, 0.04, look.main, g * 0.7, t);
                dash(pen, v, base, base + r * range, 8, 0.04, look.main, g * 0.7, t);
                let a0 = (-r.z).atan2(r.x);
                prims::arc(pen, v, base, Vec3::NEG_Y, range, a0, half * 2.0 / TAU, 20, 0.05, look.core, g * 0.8);
            }
            MoveOp::Field { radius, at: a, dist, .. } => {
                let c = at(a, dist);
                let c = ground(c).unwrap_or(c);
                prims::dashed_ring(pen, v, c + Vec3::Y * 0.08, Vec3::Y, radius.clamp(0.5, 30.0), 32, 0.05, look.main, g * 0.8, t * 0.4);
            }
            MoveOp::Room { radius, .. } | MoveOp::Restore { radius } => {
                prims::dashed_ring(pen, v, from + Vec3::Y * 0.08, Vec3::Y, radius.clamp(0.5, 30.0), 32, 0.05, look.main, g * 0.8, t * 0.4);
            }
            MoveOp::Explode { radius, at: a, dist, .. } => {
                let c = at(a, dist);
                let gd = ground(c);
                let u = 0.5 + 0.5 * (t * 2.0).sin();
                prims::telegraph(pen, v, look, StrikeKind::Blast, c, gd, radius, u, 2.0, t, 7);
            }
            MoveOp::SlamOnLand { radius, .. } => {
                let gd = ground(from).unwrap_or(from);
                let u = 0.5 + 0.5 * (t * 2.0).sin();
                prims::telegraph(pen, v, look, StrikeKind::Blast, gd, Some(gd), radius, u, 2.0, t, 11);
            }
            MoveOp::Carve { length, radius, .. } => {
                let side = Vec3::new(-flat.z, 0.0, flat.x) * radius.clamp(0.2, 4.0);
                let a0 = from + Vec3::Y * 0.1;
                let len = length.clamp(1.0, 60.0);
                dash(pen, v, a0 + side, a0 + side + flat * len, 14, 0.04, look.main, g * 0.7, t);
                dash(pen, v, a0 - side, a0 - side + flat * len, 14, 0.04, look.main, g * 0.7, t);
            }
            MoveOp::Singularity { dist, radius, .. } => {
                let c = eye + dir * dist.clamp(1.0, 60.0);
                circle(pen.fx, c, (c - v.cam).normalize_or(v.fwd), 0.6, 16, 0.05, look.dark, 0.4, t * 3.0);
                prims::dashed_ring(pen, v, ground(c).unwrap_or(c) + Vec3::Y * 0.08, Vec3::Y, radius.clamp(1.0, 30.0), 32, 0.05, look.main, g * 0.7, -t * 0.5);
            }
            MoveOp::TimeStop { .. } => {
                prims::dashed_ring(pen, v, from + Vec3::Y * 0.08, Vec3::Y, 7.0, 40, 0.04, look.main, g * 0.5, t * 0.2);
            }
            MoveOp::Cast { .. } => {
                dash(pen, v, chest, aim_point, 16, 0.03, look.main, g * 0.5, t * 2.0);
                prims::dashed_ring(pen, v, aim_point, (aim_point - v.cam).normalize_or(v.fwd), 0.6, 12, 0.04, look.core, g, t);
            }
            _ => drew = false,
        }
    }
    if !drew {
        // A state on yourself (a form, a stance, a status): a ring around the body.
        prims::dashed_ring(pen, v, from + Vec3::Y * 0.9 * size, Vec3::Y, 0.8 * size, 18, 0.04, look.main, g * 0.8, t);
    }
    end
}
