//! Match swapping: once a match is over everyone stays connected, votes on the next map (end
//! screen cards, `NetRequest::VoteMap`) and the host starts the next match on the top pick —
//! after [`host::VOTE_SECS`](super::host::VOTE_SECS) or when the host clicks — with the lobby's
//! current rules (`NetRequest::SetSettings`, the Match Rules page).
//!
//! Which maps are on the cards, RANDOM, hidden / locked picks: `super::map_vote` (host rules).
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
    dev_host_pick(h);
    if !h.vote_due() || net.build.is_some() || net.pending_world.is_some() {
        return;
    }
    let seed = (now_secs().to_bits() ^ 0x9E37_79B9_7F4A_7C15).max(1);
    // Decided once; a RANDOM win first plays its roulette (the countdown moves on).
    let Some(kind) = h.settle_vote(seed) else { return };
    let nm = NetMap { kind, seed: crate::arena::ARENA_SEED, pristine_hash: 0 };
    let settings = h.settings.clone();
    info!("net: next match on {kind:?}");
    net.build = std::thread::Builder::new().name("bd-net-world".into()).spawn(move || HostCore::build_world(&settings, nm, seed)).ok();
}

/// Dev / screenshot runs only: `BD_DEV_VOTE_AT=<secs>` makes the host pick the first map card
/// (`BD_DEV_VOTE_PICK=random`: RANDOM) that long after the vote opened.
fn dev_host_pick(h: &mut HostCore) {
    static CFG: std::sync::OnceLock<Option<(f64, bool)>> = std::sync::OnceLock::new();
    let Some((at, random)) = *CFG.get_or_init(|| {
        let at: f64 = std::env::var("BD_DEV_VOTE_AT").ok()?.parse().ok()?;
        Some((at, std::env::var("BD_DEV_VOTE_PICK").is_ok_and(|p| p.eq_ignore_ascii_case("random"))))
    }) else {
        return;
    };
    let me = h.me;
    let Some(v) = h.vote.as_ref() else { return };
    if v.votes.iter().any(|(p, _)| *p == me) || now_secs() < v.deadline - f64::from(super::host::VOTE_SECS) + at {
        return;
    }
    let pick = match (random && v.random, v.offers.first()) {
        (false, Some(k)) => super::proto::VotePick::Map(*k),
        _ => super::proto::VotePick::Random,
    };
    h.cast_pick(me, pick);
}

/// Dev / screenshot runs only (no effect unless set): `BD_DEV_SWAP_EVERY=<secs>` makes the host end
/// each match after that long and vote the next map of `BD_DEV_SWAP_MAPS=casino,lair,…`
/// (default: every map in menu order) — map swaps with real windowed clients, hands-free.
pub(super) fn dev_auto_swap(mut net: ResMut<NetState>, active: Option<ResMut<ActiveWorld>>, mut st: Local<(Option<super::proto::NetMapKind>, f64, usize, bool)>) {
    use super::proto::NetMapKind;
    static CFG: std::sync::OnceLock<Option<(f64, Vec<NetMapKind>)>> = std::sync::OnceLock::new();
    let Some((every, maps)) = CFG.get_or_init(|| {
        let every: f64 = std::env::var("BD_DEV_SWAP_EVERY").ok()?.parse().ok().filter(|s: &f64| *s > 1.0)?;
        let maps: Vec<NetMapKind> = std::env::var("BD_DEV_SWAP_MAPS")
            .map(|s| s.split(',').filter_map(|m| crate::arena::MapKind::from_cli(m.trim()).map(NetMapKind::from_map_kind)).collect())
            .unwrap_or_else(|_| NetMapKind::ALL.to_vec());
        (!maps.is_empty()).then_some((every, maps))
    }) else {
        return;
    };
    let (Some(h), Some(mut a)) = (net.host.as_mut(), active) else { return };
    if a.role != SimRole::Host || !h.in_match {
        return;
    }
    let now = now_secs();
    let (last, since, idx, voted) = &mut *st;
    if *last != Some(h.map.kind) {
        *last = Some(h.map.kind);
        *since = now;
        *voted = false;
    }
    if super::host::match_over(&a.world) {
        if h.vote.is_some() && !*voted {
            let k = maps[*idx % maps.len()];
            *idx += 1;
            *voted = true;
            h.force_next(k);
            info!("dev swap: next map {k:?}");
        }
    } else if now - *since >= *every {
        a.world.match_state.phase = bd_sim::MatchPhase::Ended;
    }
}
