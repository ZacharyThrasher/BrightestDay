//! Match swapping: once a match is over everyone stays connected, votes on the next map (end
//! screen cards, `NetRequest::VoteMap`) and the host starts the next match on the top pick —
//! after [`host::VOTE_SECS`](super::host::VOTE_SECS) or when the host clicks — with the lobby's
//! current rules (`NetRequest::SetSettings`, the Match Rules page).
//!
//! The next match reuses the join / resync path end to end: the host builds a fresh world on a
//! worker (`HostCore::build_world`), swaps it into `ActiveWorld` in place and re-sends every
//! client a `WorldInit` (map kind + seed + rules in `cfg`); clients regenerate that map off-thread
//! exactly as a late joiner does. The render mirror follows `CurrentMap` (see `present::terrain`).

use bd_net::now_secs;
use bd_sim::World;
use bd_sim::events::GoneReason;
use bevy::prelude::*;

use super::NetState;
use super::host::HostCore;
use super::proto::NetMap;
use crate::sim_bridge::{ActiveWorld, SimRole};
use crate::state::InGameMenu;

/// While the host has the menu open (changing the rules) the countdown keeps at least this much.
const HOLD_SECS: f64 = 3.0;

/// Replace the world under `a` (next match / resync). Constructs of the old world that the new
/// one doesn't have (same id, same owner and form) are ended for presentation first, so a reused
/// construct id never inherits an old model.
pub fn swap_world(a: &mut ActiveWorld, w: World) {
    let gone: Vec<bd_core::ConstructId> = a
        .world
        .constructs
        .iter()
        .filter(|c| w.constructs.get(c.id).is_none_or(|n| n.owner != c.owner || n.recipe.form != c.recipe.form))
        .map(|c| c.id)
        .collect();
    for id in gone {
        a.push_event(bd_sim::GameEvent::ConstructGone { id, reason: GoneReason::Reset, refund: 0.0 });
    }
    a.world = w;
}

/// Host: when the vote is due, build the next match's world on a worker (`match_flow` swaps it in).
pub(super) fn host_vote_flow(mut net: ResMut<NetState>, menu: Res<InGameMenu>, active: Option<Res<ActiveWorld>>) {
    let net = &mut *net;
    let Some(h) = net.host.as_mut() else { return };
    if h.vote.is_none() || !active.is_some_and(|a| a.role == SimRole::Host) {
        return;
    }
    if menu.0 {
        h.hold_vote(HOLD_SECS);
    }
    if !h.vote_due() || net.build.is_some() || net.pending_world.is_some() {
        return;
    }
    let seed = (now_secs().to_bits() ^ 0x9E37_79B9_7F4A_7C15).max(1);
    let kind = h.next_map(seed);
    let nm = NetMap { kind, seed: crate::arena::ARENA_SEED, pristine_hash: 0 };
    let settings = h.settings.clone();
    info!("net: next match on {kind:?}");
    net.build = std::thread::Builder::new().name("bd-net-world".into()).spawn(move || HostCore::build_world(&settings, nm, seed)).ok();
}
