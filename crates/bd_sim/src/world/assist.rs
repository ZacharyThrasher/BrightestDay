//! ASSIST HOOK — world plumbing for the melee lunge ([`crate::melee_assist`]) and hook-and-strike
//! ([`crate::hook`]). The rules live in those modules; this file only wires them into the tick
//! (one call each from `world/mod.rs` / `world/net.rs`):
//!
//! * [`World::assist_push_player_colliders`] / [`rewind_player_colliders`] / [`World::assist_post_move`]
//!   — players are hook targets during movement only (as each shooter saw them), then the rope
//!   rules (break / arrive → slam) run on the moved state.
//! * [`World::assist_after_constructs`] — swing starts (dash, aim nudge, slam), and this tick's
//!   melee hits of lunge / slam swings (connect juice, slam knockback).
//! * [`World::assist_bias_views`] — the hitbox aim nudge (constructs see a nudged look).

use bd_core::voxel::raycast;
use bd_core::{ConstructId, DT, PlayerId, Vec3};

use super::World;
use crate::constructs::AnimState;
use crate::constructs::held::{Mode, Swing};
use crate::events::GameEvent;
use crate::history::HullHistory;
use crate::hook::{AssistEvent, HookBreak, hookable, hooked_player, player_collider};
use crate::melee_assist::{AssistState, LungeDash, LungeVerdict, bias_look, ease, plan_dash, reach_of, swing_target_diag};
use crate::movement::{Collider, ColliderId, CollisionWorld, Hand, Hook, hand_pos, raycast_world};

/// Re-place the player hook boxes (appended last by [`World::assist_push_player_colliders`]) where
/// a shooter with `view_tick` saw them (`None` = the latest recorded hulls). Players without
/// history (just spawned) keep their current box.
pub(super) fn rewind_player_colliders(cols: &mut [Collider], history: &HullHistory, view_tick: Option<f64>, radius: f32) {
    let Some(now) = history.newest() else { return };
    let vt = view_tick.unwrap_or(now as f64);
    for c in cols.iter_mut().rev() {
        let ColliderId::Player(id) = c.id else { break };
        if let Some(h) = history.hull_at(id, vt) {
            c.center = h.center();
            c.half = h.half() + Vec3::splat(radius.max(0.0));
        }
    }
}

/// A swing that started this tick (see [`World::assist_after_constructs`]).
struct SwingStart {
    construct: ConstructId,
    target: Option<PlayerId>,
    dash: Option<LungeDash>,
    close: f32,
    heavy: bool,
    pos: Vec3,
    /// Diagnostics (`AssistState::verdict`).
    verdict: LungeVerdict,
}

impl World {
    /// Players become hook targets (non-solid, anchorable boxes appended after the construct
    /// colliders) when the match rule allows it. Returns the construct-collider count.
    pub(super) fn assist_push_player_colliders(&mut self) -> usize {
        let n = self.colliders.len();
        if !self.cfg.rules.tether_hooks_players {
            return n;
        }
        let r = self.cfg.tuning.assist.hook.radius;
        let base = &self.cfg.move_params;
        for p in &self.players {
            if hookable(p) {
                self.colliders.push(player_collider(p.id, p.hull(base), p.mv.vel, r));
            }
        }
        n
    }

    /// After movement: drop the player boxes again, then the hook-and-strike rules on every rope
    /// that holds a player (breaks, arrival → slam, `Hooked` announcements).
    pub(super) fn assist_post_move(&mut self, n_cols: usize, dt: f32) {
        self.colliders.truncate(n_cols);
        let t = self.cfg.tuning.assist.hook;
        for i in 0..self.players.len() {
            if !self.players[i].alive {
                self.players[i].assist.hook_los = [0.0; 2];
                continue;
            }
            // The rule prediction applies too (rope too long).
            let params = self.players[i].params(&self.cfg.move_params);
            let pre = self.players[i].mv.hooks;
            let broke = crate::hook::local_rules(&mut self.players[i].mv, &t, &params);
            for hand in 0..2 {
                let Some(victim) = hooked_player(&pre[hand]) else {
                    self.players[i].assist.hook_los[hand] = 0.0;
                    continue;
                };
                let id = self.players[i].id;
                if broke[hand] {
                    self.events.push(GameEvent::Assist {
                        player: id,
                        other: Some(victim),
                        kind: AssistEvent::HookBroken { hand: hand as u8, why: HookBreak::TooFar },
                        pos: pre[hand].pos,
                    });
                    continue;
                }
                if self.players[i].move_events.tether_attached[hand] {
                    self.events.push(GameEvent::Assist {
                        player: id,
                        other: Some(victim),
                        kind: AssistEvent::Hooked { hand: hand as u8 },
                        pos: pre[hand].pos,
                    });
                }
                match self.hook_rule(i, hand, victim, dt) {
                    Some(Ok(why)) => {
                        let at = self.players[i].mv.hooks[hand].pos;
                        self.players[i].mv.hooks[hand] = Hook::default();
                        self.players[i].assist.hook_los[hand] = 0.0;
                        self.events.push(GameEvent::Assist {
                            player: id,
                            other: Some(victim),
                            kind: AssistEvent::HookBroken { hand: hand as u8, why },
                            pos: at,
                        });
                    }
                    Some(Err(())) => {
                        // Arrived with a melee weapon: the rope lets go, the next swing slams.
                        let at = self.players[i].mv.hooks[hand].pos;
                        self.players[i].mv.hooks[hand] = Hook::default();
                        self.players[i].assist.hook_los[hand] = 0.0;
                        self.players[i].assist.slam_t = t.slam_window;
                        self.players[i].assist.slam_swing = None;
                        self.events.push(GameEvent::Assist { player: id, other: Some(victim), kind: AssistEvent::SlamReady, pos: at });
                    }
                    None => {}
                }
            }
        }
    }

    /// One rope from player `i`'s `hand` to `victim`: `Some(Ok(why))` = it breaks, `Some(Err(()))`
    /// = arrived (slam), `None` = holds. Updates the line-of-sight timer.
    fn hook_rule(&mut self, i: usize, hand: usize, victim: PlayerId, dt: f32) -> Option<Result<HookBreak, ()>> {
        let t = self.cfg.tuning.assist.hook;
        let base = &self.cfg.move_params;
        let hook = self.players[i].mv.hooks[hand];
        if !hook.attached() {
            return None;
        }
        let Some(vi) = self.pidx(victim) else { return Some(Ok(HookBreak::TooFar)) };
        let (p, v) = (&self.players[i], &self.players[vi]);
        // The victim escapes: a dodge, a boost burst, or a tether of their own.
        if crate::hook::hook_breaks_on_dodge(v) || (t.breaks_on_boost && v.move_events.boost_burst) {
            return Some(Ok(HookBreak::Dodged));
        }
        if t.breaks_on_tether && v.move_events.tether_fired.iter().any(|f| *f) {
            return Some(Ok(HookBreak::Countered));
        }
        let params = p.params(base);
        let chest = p.mv.chest(&params);
        let hand_at = hand_pos(&p.mv, &params, p.input.yaw, p.input.pitch, if hand == 0 { Hand::Left } else { Hand::Right });
        // The victim's swing cuts the rope (last tick's blade segment).
        if let Some(c) = self.constructs.held_by(victim).filter(|c| c.held.blade_live) {
            let s = &c.held;
            if crate::hook::segment_distance(s.blade0, s.blade1, hand_at, hook.pos) <= s.blade_r + t.cut_margin {
                return Some(Ok(HookBreak::Cut));
            }
        }
        // Line of sight (terrain) along the rope.
        let d = hook.pos - chest;
        let dist = d.length();
        let blocked = dist > 0.6 && raycast(&self.voxels, chest, d / dist, dist - 0.4).is_some();
        let los = &mut self.players[i].assist.hook_los[hand];
        *los = if blocked { *los + dt } else { 0.0 };
        if *los > t.los_grace {
            return Some(Ok(HookBreak::LostSight));
        }
        // Arrival with a melee weapon in hand.
        let (p, v) = (&self.players[i], &self.players[vi]);
        let melee = self.constructs.held_by(p.id).is_some_and(|c| c.active() && !crate::constructs::gun::is_gun(c));
        if melee {
            let reach = crate::geom::dist_sq_point_aabb(&v.hull(base), chest).sqrt();
            if reach <= t.arrive_dist {
                return Some(Err(()));
            }
        }
        None
    }

    /// After the constructs stepped (before their intents are applied): lunge / slam swings'
    /// hits, then this tick's swing starts (dash, aim nudge, slam), then the timers.
    pub(super) fn assist_after_constructs(&mut self) {
        let dt = DT;
        let lt_connect = (
            self.cfg.tuning.assist.lunge.connect_impact,
            self.cfg.tuning.assist.lunge.connect_heavy_impact,
            self.cfg.tuning.assist.lunge.connect_knockback,
            self.cfg.tuning.assist.lunge.keep_closing_speed,
        );
        let ht = self.cfg.tuning.assist.hook;
        // 1. Hits delivered this tick by a lunge swing / the slam swing.
        for k in 0..self.cout.player_hits.len() {
            let h = self.cout.player_hits[k];
            let (Some(src), Some(cid)) = (h.source, h.construct) else { continue };
            let Ok(si) = self.players.binary_search_by_key(&src, |p| p.id) else { continue };
            let a = self.players[si].assist;
            if a.lunge_swing == Some(cid) {
                if self.cfg.tuning.assist.lunge.hit_resets_cooldown {
                    // A lunge that connects can chain into the next one.
                    self.players[si].assist.lunge_cd = 0.0;
                }
                let hit = &mut self.cout.player_hits[k];
                hit.impact *= lt_connect.0 * if a.lunge_heavy { lt_connect.1 } else { 1.0 };
                hit.knockback *= lt_connect.2;
                if lt_connect.3 && hit.closing_speed.is_none() {
                    hit.closing_speed = Some(a.lunge_close);
                }
            }
            if a.slam_swing == Some(cid) {
                let hit = &mut self.cout.player_hits[k];
                hit.knockback *= ht.slam_knockback;
                hit.impulse_dir = hit.impulse_dir.normalize_or_zero() + Vec3::Y * ht.slam_lift;
                hit.impact *= ht.slam_impact;
                if !a.slam_hit {
                    self.players[si].assist.slam_hit = true;
                    self.events.push(GameEvent::Assist { player: src, other: Some(h.target), kind: AssistEvent::Slam, pos: h.point });
                }
            }
        }
        // 2. Timers; swings that ended let go of their target / lunge / slam.
        for i in 0..self.players.len() {
            if !self.players[i].alive {
                self.players[i].assist = AssistState::default();
                continue;
            }
            let id = self.players[i].id;
            let held = self.constructs.held_by(id).map(|c| (c.id, c.held.mode));
            let swinging = |cid: Option<ConstructId>| {
                cid.is_some() && held.is_some_and(|(h, m)| Some(h) == cid && matches!(m, Mode::Swing | Mode::Charge))
            };
            let a = &mut self.players[i].assist;
            a.lunge_cd = (a.lunge_cd - dt).max(0.0);
            a.slam_t = (a.slam_t - dt).max(0.0);
            a.aim_t += dt;
            if !swinging(a.swing) {
                a.swing = None;
                a.aim_target = None;
            }
            if !swinging(a.lunge_swing) {
                a.lunge_swing = None;
            }
            if !swinging(a.slam_swing) {
                a.slam_swing = None;
            }
        }
        // 3. Swings that started this tick.
        for i in 0..self.players.len() {
            let Some(s) = self.assist_swing_start(i) else { continue };
            let id = self.players[i].id;
            let a = &mut self.players[i].assist;
            if a.slam_t > 0.0 {
                a.slam_t = 0.0;
                a.slam_swing = Some(s.construct);
                a.slam_hit = false;
            }
            a.swing = Some(s.construct);
            a.aim_target = s.target;
            a.aim_t = 0.0;
            a.verdict = s.verdict;
            a.verdict_tick = self.tick;
            if let Some(dash) = s.dash {
                a.lunge_cd = self.cfg.tuning.assist.lunge.cooldown;
                a.lunge_swing = Some(s.construct);
                a.lunge_close = s.close;
                a.lunge_heavy = s.heavy;
                self.players[i].mv.lunge = dash;
                self.events.push(GameEvent::Assist { player: id, other: s.target, kind: AssistEvent::Lunge, pos: s.pos });
            }
        }
    }

    /// Did player `i`'s held melee weapon start a swing this tick? (Primary, combo steps, released
    /// heavy; any non-gun held item — the slam rides on it; only lunging forms get a target.)
    fn assist_swing_start(&self, i: usize) -> Option<SwingStart> {
        let p = &self.players[i];
        if !p.alive || p.busy() {
            return None;
        }
        let c = self.constructs.held_by(p.id)?;
        if c.gone.is_some() || c.held.mode != Mode::Swing || c.anim_state != AnimState::Windup || c.anim_since != self.tick {
            return None;
        }
        if !matches!(c.held.swing, Swing::Primary | Swing::Combo0 | Swing::Combo1 | Swing::Combo2 | Swing::Heavy)
            || crate::constructs::gun::is_gun(c)
        {
            return None;
        }
        let base = &self.cfg.move_params;
        let params = p.params(base);
        let eye = p.mv.eye(&params);
        let chest = p.mv.chest(&params);
        let mut s = SwingStart { construct: c.id, target: None, dash: None, close: 0.0, heavy: false, pos: eye, verdict: LungeVerdict::Form };
        let lt = &self.cfg.tuning.assist.lunge;
        let Some(reach) = reach_of(lt, c, p.size.scale) else { return Some(s) };
        s.heavy = reach.heavy;
        let vt = p.view_tick;
        let cw = CollisionWorld::new(&self.voxels, &self.colliders);
        let cands = self.players.iter().filter(|q| q.id != p.id && q.alive && !q.shielded()).map(|q| {
            let hull = vt.and_then(|v| self.history.hull_at(q.id, v)).unwrap_or_else(|| q.hull(base));
            (q.id, hull)
        });
        let clear = |a: Vec3, b: Vec3| {
            let d = b - a;
            let len = d.length();
            len < 1e-3 || raycast_world(&cw, a, d / len, len, false).is_none_or(|h| h.t >= len - 0.3)
        };
        let tg = match swing_target_diag(eye, p.look(), &reach, cands, clear) {
            Ok(tg) => tg,
            Err(miss) => {
                s.verdict = LungeVerdict::Miss(miss);
                return Some(s);
            }
        };
        s.target = Some(tg.id);
        s.verdict = if tg.dist <= reach.reach {
            LungeVerdict::InReach
        } else if !lt.enabled {
            LungeVerdict::Disabled
        } else if p.mv.lunge.active() {
            LungeVerdict::Dashing
        } else if p.assist.lunge_cd > 0.0 {
            LungeVerdict::Cooldown
        } else {
            LungeVerdict::Disabled
        };
        if lt.enabled && p.assist.lunge_cd <= 0.0 && tg.dist > reach.reach && !p.mv.lunge.active() {
            let tvel = self.player(tg.id).map_or(Vec3::ZERO, |q| q.mv.vel);
            if let Some(dash) = plan_dash(lt, &reach, chest, p.mv.vel, tg.point, tvel) {
                let dir = (dash.to - chest).normalize_or_zero();
                s.close = (p.mv.vel - tvel).dot(dir).max(0.0);
                s.dash = Some(dash);
                s.pos = chest;
                s.verdict = LungeVerdict::Lunged;
            }
        }
        Some(s)
    }

    /// The hitbox aim nudge: constructs see each swinging player's look turned toward their swing
    /// target (eased in over `aim_ramp_secs`, at most `aim_max_deg`). Never touches the input.
    pub(super) fn assist_bias_views(&mut self) {
        let lt = &self.cfg.tuning.assist.lunge;
        if lt.aim_max_deg <= 0.0 {
            return;
        }
        let base = &self.cfg.move_params;
        for v in self.views.iter_mut() {
            let Ok(i) = self.players.binary_search_by_key(&v.id, |p| p.id) else { continue };
            let p = &self.players[i];
            let a = &p.assist;
            let Some(tid) = a.aim_target else { continue };
            if a.aim_t > lt.aim_hold_secs {
                continue;
            }
            let Ok(ti) = self.players.binary_search_by_key(&tid, |q| q.id) else { continue };
            let q = &self.players[ti];
            if !q.alive {
                continue;
            }
            let hull = p.view_tick.and_then(|vt| self.history.hull_at(tid, vt)).unwrap_or_else(|| q.hull(base));
            let k = if lt.aim_ramp_secs > 0.0 { ease(a.aim_t / lt.aim_ramp_secs) } else { 1.0 };
            v.look = bias_look(v.look, v.eye, hull.center(), lt.aim_max_deg.to_radians() * k, lt.aim_release_deg.to_radians());
        }
    }
}
