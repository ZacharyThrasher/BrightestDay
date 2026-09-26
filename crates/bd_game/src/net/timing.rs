//! Client-side event timing: when presentation should see each host event.
//!
//! Remote players and constructs are drawn at the interpolation render tick (~2 snapshots in the
//! past). Events about *them* (a remote swing, a hit between two other players, a remote
//! construct forming or shattering) are therefore held back until the render tick reaches the
//! tick they happened on, so a hit never shows before the swing that caused it. Events that
//! involve the **local** player (hits on you, your casts' confirmations, your knockback, your
//! kills) and match-level events are released immediately: you feel them at once.
//!
//! Terrain events (carves, extracts, restores) always go through the delayed queue *in stream
//! order* (they carry no cause, and the render mirror must apply them in exactly the host's
//! order). The replica `World` itself applies every event on arrival — only presentation waits.

use std::collections::VecDeque;

use bd_core::{ConstructId, PlayerId, Tick};
use bd_sim::events::Traveller;
use bd_sim::powers::PowerEvent;
use bd_sim::{GameEvent, World};

/// Longest an event is held (safety net if the interpolation clock stalls).
pub const MAX_HOLD_SECS: f64 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    /// Show now.
    Now,
    /// Show when the interpolated render tick reaches the event's tick.
    AtRenderTime,
}

fn owner_of(w: &World, id: ConstructId) -> Option<PlayerId> {
    w.constructs.get(id).map(|c| c.owner)
}

/// Classify one event (call after the replica applied it, so spawns are known).
pub fn timing(e: &GameEvent, me: Option<PlayerId>, w: &World) -> Timing {
    use GameEvent as E;
    let is_me = |p: PlayerId| Some(p) == me;
    let is_me_opt = |p: Option<PlayerId>| p.is_some() && p == me;
    let mine_c = |c: ConstructId| owner_of(w, c).is_some_and(is_me);
    let t = |local: bool| if local { Timing::Now } else { Timing::AtRenderTime };
    match e {
        // Match / lobby level: immediately.
        E::PlayerJoined { .. } | E::PlayerLeft { .. } | E::MatchPhase { .. } | E::ChaosCast { .. } => Timing::Now,
        E::PartyPhase { .. } | E::PartyRoundEnd { .. } | E::PartyOffer { .. } | E::PartyLocked { .. } | E::PartyBoomOrBust { .. } => Timing::Now,
        E::PartyReveal { .. } | E::PartyEnd { .. } => Timing::Now,
        // Terrain: one ordered stream, always at render time.
        E::Carve { .. } | E::Extracted { .. } | E::ArenaReset { .. } => Timing::AtRenderTime,
        E::Power(PowerEvent::Restored { .. }) => Timing::AtRenderTime,
        // Player-centred.
        E::Hit { target, source, .. } => t(is_me(*target) || is_me_opt(*source)),
        E::Blocked { target, source, .. } => t(is_me(*target) || is_me_opt(*source)),
        E::Ko { victim, killer, .. } => t(is_me(*victim) || is_me_opt(*killer)),
        E::WallSplat { player, .. }
        | E::Respawn { player, .. }
        | E::Shove { player, .. }
        | E::CastFailed { player, .. }
        | E::Grafted { player, .. }
        | E::Move { player, .. }
        | E::OathBegan { player }
        | E::OathFinished { player, .. }
        | E::OathInterrupted { player, .. }
        | E::OathCancelled { player }
        | E::OathRejected { player, .. }
        | E::Seated { player, .. }
        | E::Ejected { player, .. }
        | E::Resized { player, .. }
        | E::VoidRescue { player, .. }
        | E::PartyFx { player, .. } => t(is_me(*player)),
        E::StatusApplied { player, source, .. } => t(is_me(*player) || is_me_opt(*source)),
        E::ProjectileFired { owner, .. } | E::ShotFired { owner, .. } | E::Hitscan { owner, .. } => t(is_me(*owner)),
        E::ProjectileEnded { hit_player, .. } => t(is_me_opt(*hit_player)),
        E::HeldFx { owner, .. } => t(is_me(*owner)),
        E::ConstructSpawned { owner, .. } => t(is_me(*owner)),
        E::ConstructHit { id, source, .. } => t(mine_c(*id) || is_me_opt(*source)),
        E::ConstructShattered { id, .. } | E::ConstructGone { id, .. } | E::CreatureAttack { id, .. } | E::CreatureMode { id, .. } => {
            t(mine_c(*id))
        }
        E::Clash { a, b, .. } => t(mine_c(*a) || mine_c(*b)),
        E::Explosion { source, .. } => t(is_me_opt(*source)),
        E::Teleported { who, .. } => match who {
            Traveller::Player(p) => t(is_me(*p)),
            Traveller::Construct(c) => t(mine_c(*c)),
            Traveller::Projectile(_) => Timing::AtRenderTime,
        },
        E::PortalOpened { owner, .. } => t(is_me(*owner)),
        E::FieldCreated { field } => t(is_me(field.owner)),
        E::LinkCreated { link } => t(is_me_opt(link.owner)),
        E::PortalClosed { .. } | E::FieldEnded { .. } | E::LinkBroken { .. } => Timing::AtRenderTime,
        E::Power(p) => match p {
            PowerEvent::TimeStop { .. } => Timing::Now,
            PowerEvent::Swapped { a, b_player, .. } => t(is_me(*a) || is_me_opt(*b_player)),
            PowerEvent::FruitSpawned { .. } => Timing::AtRenderTime,
            PowerEvent::Gained { player, .. }
            | PowerEvent::Lost { player, .. }
            | PowerEvent::Overdose { player, .. }
            | PowerEvent::Drained { player, .. }
            | PowerEvent::Seastone { player, .. }
            | PowerEvent::Scattered { player, .. }
            | PowerEvent::Shrugged { player, .. }
            | PowerEvent::Move { player, .. }
            | PowerEvent::Delivered { player, .. }
            | PowerEvent::Blinked { player, .. }
            | PowerEvent::Transformed { player, .. }
            | PowerEvent::StandRush { player, .. } => t(is_me(*player)),
            #[allow(unreachable_patterns)]
            _ => Timing::AtRenderTime,
        },
        #[allow(unreachable_patterns)]
        _ => Timing::AtRenderTime,
    }
}

/// Events waiting for the render clock.
#[derive(Default)]
pub struct EventDelay {
    queue: VecDeque<(Tick, f64, GameEvent)>,
    /// Constructs whose spawn is still queued: later events about them must queue too.
    held_spawns: Vec<ConstructId>,
    pub delayed_total: u64,
    /// Smoothed hold time (ms).
    pub avg_hold_ms: f32,
}

impl EventDelay {
    /// Route one event (already applied to the replica) to `now_out` or the queue.
    pub fn route(&mut self, e: GameEvent, tick: Tick, now: f64, me: Option<PlayerId>, w: &World, now_out: &mut Vec<GameEvent>) {
        let mut when = timing(&e, me, w);
        // Never show something about a construct before its (held) spawn.
        let about = match &e {
            GameEvent::ConstructHit { id, .. }
            | GameEvent::ConstructShattered { id, .. }
            | GameEvent::ConstructGone { id, .. }
            | GameEvent::CreatureAttack { id, .. }
            | GameEvent::CreatureMode { id, .. }
            | GameEvent::Grafted { id, .. } => Some(*id),
            _ => None,
        };
        if about.is_some_and(|id| self.held_spawns.contains(&id)) {
            when = Timing::AtRenderTime;
        }
        match when {
            Timing::Now => now_out.push(e),
            Timing::AtRenderTime => {
                if let GameEvent::ConstructSpawned { id, .. } = &e {
                    self.held_spawns.push(*id);
                }
                self.delayed_total += 1;
                self.queue.push_back((tick, now, e));
            }
        }
    }

    /// Release everything whose tick the render clock has reached (in arrival order).
    pub fn release(&mut self, render_tick: f64, now: f64, out: &mut Vec<GameEvent>) {
        while let Some((tick, at, _)) = self.queue.front() {
            if (*tick as f64) > render_tick && now - at < MAX_HOLD_SECS {
                break;
            }
            let Some((_, at, e)) = self.queue.pop_front() else { break };
            self.avg_hold_ms += (((now - at) * 1000.0) as f32 - self.avg_hold_ms) * 0.05;
            if let GameEvent::ConstructSpawned { id, .. } = &e {
                self.held_spawns.retain(|h| h != id);
            }
            out.push(e);
        }
    }

    /// Release everything now (world replaced / left).
    pub fn flush(&mut self, out: &mut Vec<GameEvent>) {
        out.extend(self.queue.drain(..).map(|(_, _, e)| e));
        self.held_spawns.clear();
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::corps::Corps;
    use bd_voxel::{CarveOp, CarveShape};

    fn carve(seed: u32) -> GameEvent {
        GameEvent::Carve { op: CarveOp { shape: CarveShape::Sphere { center: bd_core::Vec3::ZERO, radius: 1.0 }, power: 1.0, seed } }
    }

    #[test]
    fn local_now_remote_at_render_time_terrain_in_order() {
        let mut w = World::new(bd_sim::SimConfig { arena: bd_sim::ArenaSource::Flat { size_m: 32.0 }, ..Default::default() });
        for i in 0..3u8 {
            let _ = w.add_player(PlayerId(i), format!("p{i}"), Corps::from_index(i));
        }
        let me = Some(PlayerId(1));
        let (mut d, mut out) = (EventDelay::default(), Vec::new());
        let shove = |p: u8| GameEvent::Shove { player: PlayerId(p), origin: bd_core::Vec3::ZERO, dir: bd_core::Vec3::X };
        d.route(carve(1), 100, 0.0, me, &w, &mut out);
        d.route(shove(1), 100, 0.0, me, &w, &mut out); // mine: now
        d.route(shove(2), 100, 0.0, me, &w, &mut out); // remote: later
        d.route(carve(2), 101, 0.0, me, &w, &mut out);
        d.route(GameEvent::MatchPhase { phase: bd_sim::MatchPhase::Live, round: 1, winner: None }, 101, 0.0, me, &w, &mut out);
        assert_eq!(out.len(), 2, "own shove + match phase immediately");
        out.clear();
        d.release(99.5, 0.01, &mut out);
        assert!(out.is_empty(), "render clock not there yet");
        d.release(100.0, 0.02, &mut out);
        assert!(matches!(out[0], GameEvent::Carve { op } if op.seed == 1));
        assert!(matches!(out[1], GameEvent::Shove { player, .. } if player == PlayerId(2)));
        assert_eq!(out.len(), 2);
        d.release(100.5, 1.0, &mut out); // stale → released by the safety net, still in order
        assert!(matches!(out[2], GameEvent::Carve { op } if op.seed == 2));
        assert_eq!(d.len(), 0);
    }
}
