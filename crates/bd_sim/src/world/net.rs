//! What the network layer and the client need from the world: per-recipient snapshots, the client
//! mirror (`apply_snapshot` / `apply_event`), and prediction of the local player with exactly the
//! host's movement pipeline (modifiers, size, portals, fields).

use std::sync::Arc;

use bd_core::input::PlayerInput;
use bd_core::mechanics::Recipe;
use bd_core::{ConstructId, DT, PlayerId, Tick, Vec3};

use super::{Player, Seat, World};
use crate::constructs::{AnimState, SpawnParams, forming_secs, lifetime_secs};
use crate::events::GameEvent;
use crate::extract::apply_extract;
use crate::movement::{CollisionWorld, HookPhase, MoveEvents, MoveModifiers, PlayerState, step_player};
use crate::projectile::{Projectile, ProjectileKind, ProjectileSpawn};
use crate::replicate::*;
use crate::space::{ForceField, Link, LinkEnd, PortalEnd, PortalPair};
use crate::willpower::OathState;

/// Everything the local player's prediction needs besides the world (kept by the client from the
/// last snapshot).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictCtx {
    pub size: f32,
    pub stunned: bool,
    pub suit: bool,
}

fn player_snap(p: &Player, trail_kb: f32) -> PlayerSnap {
    let trail_kb = trail_kb.max(1.0);
    use pflags::*;
    let mut f = 0u16;
    let set = |f: &mut u16, b: u16, on: bool| {
        if on {
            *f |= b;
        }
    };
    set(&mut f, ALIVE, p.alive);
    set(&mut f, GROUNDED, p.mv.grounded);
    set(&mut f, CROUCHED, p.mv.crouched);
    set(&mut f, SLIDING, p.mv.sliding);
    set(&mut f, BOOSTING, p.mv.boosting);
    set(&mut f, TUMBLING, p.mv.tumble > 0.0);
    set(&mut f, SHIELD, p.shielded());
    set(&mut f, RECITING, p.oath.reciting());
    set(&mut f, TYPING, p.input.held(bd_core::input::buttons::TYPING));
    set(&mut f, SEATED, p.seat.is_some());
    set(&mut f, SUIT, p.seat.is_some_and(|s| s.suit));
    set(&mut f, HOOK_L_ATTACHED, p.mv.hooks[0].attached());
    set(&mut f, HOOK_R_ATTACHED, p.mv.hooks[1].attached());
    set(&mut f, LAUNCHED, p.launch.is_some());
    set(&mut f, ELIMINATED, p.eliminated);
    let hook = |i: usize| (p.mv.hooks[i].phase != HookPhase::Idle).then_some(QPos::new(p.mv.hooks[i].pos));
    PlayerSnap {
        id: p.id.0,
        flags: f.to_le_bytes(),
        pos: QPos::new(p.mv.pos),
        vel: QVel::new(p.mv.vel),
        look: QLook::new(p.input.yaw, p.input.pitch),
        pct: (p.damage_pct * 10.0).round().clamp(0.0, 65535.0) as u16,
        launch: p.launch.map_or(0, |l| (l.speed / trail_kb * 255.0).round().clamp(1.0, 255.0) as u8),
        stocks: p.stocks.unwrap_or(255),
        willpower: p.willpower.clamp(0.0, 255.0).round() as u8,
        status: p.status.bits(),
        size: (p.size.scale * 100.0).round().clamp(1.0, 255.0) as u8,
        hooks: [hook(0), hook(1)],
        seat: p.seat.map(|s| (s.construct.0, s.part)),
        power: p.power.snap(), // POWERS HOOK
    }
}

impl World {
    /// Full snapshot for one recipient (`None` = spectator). Feed it to a per-recipient
    /// [`DeltaEncoder`] before sending.
    pub fn snapshot_for(&self, recipient: Option<PlayerId>) -> WorldSnapshot {
        let mut s = WorldSnapshot::default();
        self.write_snapshot(recipient, &mut s);
        s
    }

    /// Like [`Self::snapshot_for`] but reusing `out`'s buffers.
    pub fn write_snapshot(&self, recipient: Option<PlayerId>, out: &mut WorldSnapshot) {
        out.tick = self.tick;
        let me = recipient.and_then(|r| self.player(r));
        out.ack = me.map_or(0, |p| p.last_input_seq);
        out.own = me.filter(|p| p.alive).map(|p| OwnState::from_state(&p.mv, p.size.scale, p.portal_cd));
        out.match_info.phase = self.match_state.phase as u8;
        out.match_info.round = self.match_state.round;
        out.match_info.time_left = self.time_left().ceil().clamp(0.0, u16::MAX as f32) as u16;
        out.match_info.winner = self.match_state.winner.map(|w| w.0);
        out.match_info.scores.clear();
        out.match_info.scores.extend(self.players.iter().map(|p| (p.id.0, p.kos, p.deaths)));
        // PARTY HOOK: party phase / standings / owned effects / the recipient's own offer.
        match &self.party {
            Some(party) => party.write_snap(recipient, self.party_time_left(), out.party.get_or_insert_with(Default::default)),
            None => out.party = None,
        }
        out.players.clear();
        let trail = self.cfg.tuning.smash.launch_trail_kb;
        out.players.extend(self.players.iter().map(|p| player_snap(p, trail)));
        out.constructs.clear();
        for c in self.constructs.iter() {
            if c.gone.is_some() {
                continue;
            }
            let attached = c.attached();
            let mut flags = 0u8;
            if c.forming_t > 0.0 {
                flags |= cflags::FORMING;
            }
            if attached {
                flags |= cflags::ATTACHED;
            }
            if c.pilot.is_some() {
                flags |= cflags::PILOTED;
            }
            if c.colossal {
                flags |= cflags::COLOSSAL;
            }
            if c.suit {
                flags |= cflags::SUIT;
            }
            out.constructs.push(ConstructSnap {
                id: c.id.0,
                owner: c.owner.0,
                pos: if attached { QPos::default() } else { QPos::new(c.pos) },
                rot: if attached { QRot::default() } else { QRot::new(c.rot) },
                anim: c.anim_state as u8,
                anim_since: c.anim_since as u16,
                integrity: ((c.integrity / c.max_integrity.max(1e-3)).clamp(0.0, 1.0) * 255.0).round() as u8,
                flags,
                size: (c.size_scale * 50.0).round().clamp(1.0, 255.0) as u8,
                weapon: c.weapon.map_or(0, |w| w.wire(&crate::weapons::spec_for(&c.recipe, w.arch))),
                xform: xform_wire(c),
            });
        }
        out.projectiles.clear();
        out.projectiles.extend(self.projectiles.iter().filter(|p| !p.dead).map(|p| ProjectileSnap {
            id: p.id,
            owner: p.spec.owner.0,
            kind: p.spec.kind as u8,
            origin: QPos::new(p.origin),
            vel: QVel::new(p.vel0),
            since: p.segment_tick as u16,
        }));
        out.projectiles.sort_by_key(|p| p.id);
        out.portals.clear();
        out.portals.extend(self.portals.iter().map(|p| PortalSnap {
            id: p.id,
            owner: p.owner.0,
            ends: p.ends.map(|e| PortalEndSnap {
                pos: QPos::new(e.pos),
                normal: QDir::new(e.normal),
                up: QDir::new(e.up),
                radius: (e.radius * 20.0).round().clamp(1.0, 255.0) as u8,
                hp: e.hp.clamp(0.0, 255.0) as u8,
            }),
        }));
        out.fields.clear();
        out.fields.extend(self.fields.iter().map(|f| FieldSnap {
            id: f.id,
            owner: f.owner.0,
            pos: QPos::new(f.pos),
            radius: (f.radius * 4.0).round().clamp(1.0, 255.0) as u8,
            strength: f.strength.round().clamp(-127.0, 127.0) as i8,
        }));
        out.links.clear();
        let end = |e: &LinkEnd| match *e {
            LinkEnd::Player(p) => LinkEndSnap::Player(p.0),
            LinkEnd::Construct(c) => LinkEndSnap::Construct(c.0),
            LinkEnd::Point(p) => LinkEndSnap::Point(QPos::new(p)),
        };
        out.links.extend(self.links.iter().map(|l| LinkSnap { id: l.id, a: end(&l.a), b: end(&l.b), max_len: (l.max_len * 4.0).round().clamp(1.0, 255.0) as u8 }));
    }

    // ------------------------------------------------------------------------------ client

    /// Client mirror: adopt a host snapshot. Remote players take the snapshot's render state; the
    /// local player (`local`) takes the exact `own` movement state (the game's reconciler replays
    /// unacknowledged inputs on top). Constructs get transforms / anim state (models come from
    /// `ConstructSpawned` events via [`Self::apply_event`]); projectiles, portals, fields and links
    /// are replaced.
    pub fn apply_snapshot(&mut self, snap: &WorldSnapshot, local: Option<PlayerId>) {
        self.tick = snap.tick;
        self.match_state.phase = match snap.match_info.phase {
            1 => crate::rules::MatchPhase::Live,
            2 => crate::rules::MatchPhase::Ended,
            _ => crate::rules::MatchPhase::Warmup,
        };
        self.match_state.round = snap.match_info.round;
        self.match_state.winner = snap.match_info.winner.map(PlayerId);
        // PARTY HOOK: mirror the party (owned effects drive prediction, e.g. Slippery).
        if let (Some(party), Some(ps)) = (self.party.as_mut(), snap.party.as_ref()) {
            party.apply_snap(ps);
        }
        let base = self.cfg.move_params.clone();
        // Players.
        self.players.retain(|p| snap.players.iter().any(|s| s.id == p.id.0));
        for s in &snap.players {
            let id = PlayerId(s.id);
            let i = match self.pidx(id) {
                Some(i) => i,
                None => {
                    let corps = bd_core::corps::Corps::from_index(s.id);
                    let p = Player::new(id, format!("player {}", s.id), corps, s.pos.get(), &base);
                    let at = self.players.partition_point(|q| q.id < id);
                    self.players.insert(at, p);
                    at
                }
            };
            let p = &mut self.players[i];
            let fl = s.flags();
            p.alive = fl & pflags::ALIVE != 0;
            p.damage_pct = s.pct as f32 / 10.0;
            p.stocks = (s.stocks != 255).then_some(s.stocks);
            p.eliminated = fl & pflags::ELIMINATED != 0;
            p.willpower = s.willpower as f32;
            p.size.scale = s.size_scale();
            p.size.target = p.size.scale;
            p.spawn_shield = if fl & pflags::SHIELD != 0 { p.spawn_shield.max(DT) } else { 0.0 };
            p.seat = s.seat.map(|(c, part)| Seat { construct: ConstructId(c), part, suit: fl & pflags::SUIT != 0 });
            p.power.apply_snap(s.power); // POWERS HOOK (prediction reads the power's movement mods)
            if Some(id) == local {
                if let Some(own) = &snap.own {
                    p.mv = own.to_state();
                    p.size.scale = if own.size.is_finite() { own.size.clamp(0.25, 2.5) } else { 1.0 };
                    p.portal_cd = if own.portal_cd.is_finite() { own.portal_cd } else { 0.0 };
                }
                p.last_input_seq = snap.ack;
            } else {
                p.mv.pos = s.pos.get();
                p.mv.vel = s.vel.get();
                p.mv.grounded = fl & pflags::GROUNDED != 0;
                p.mv.crouched = fl & pflags::CROUCHED != 0;
                p.mv.sliding = fl & pflags::SLIDING != 0;
                p.mv.boosting = fl & pflags::BOOSTING != 0;
                p.mv.tumble = if fl & pflags::TUMBLING != 0 { p.mv.tumble.max(DT) } else { 0.0 };
                let (yaw, pitch) = s.look.get();
                p.input.yaw = yaw;
                p.input.pitch = pitch;
                for h in 0..2 {
                    let attached = fl & if h == 0 { pflags::HOOK_L_ATTACHED } else { pflags::HOOK_R_ATTACHED } != 0;
                    match s.hooks[h] {
                        Some(q) => {
                            p.mv.hooks[h].phase = if attached { HookPhase::Attached } else { HookPhase::Flying };
                            p.mv.hooks[h].pos = q.get();
                        }
                        None => p.mv.hooks[h] = Default::default(),
                    }
                }
            }
            // Status bits → presence only (timers are host-side).
            for k in crate::status::StatusKind::ALL {
                let on = s.status & k.bit() != 0;
                let t = &mut p.status.timers[k as usize];
                *t = if on { t.max(DT) } else { 0.0 };
            }
            if fl & pflags::RECITING == 0 {
                p.oath = OathState::Idle;
            }
        }
        // Constructs.
        let tick = self.tick;
        self.constructs.as_slice();
        let keep: Vec<u32> = snap.constructs.iter().map(|c| c.id).collect();
        let gone: Vec<ConstructId> = self.constructs.iter().filter(|c| keep.binary_search(&c.id.0).is_err()).map(|c| c.id).collect();
        for id in gone {
            self.constructs.remove(id);
        }
        for cs in &snap.constructs {
            let id = ConstructId(cs.id);
            if self.constructs.get(id).is_none() {
                // Model unknown yet (event on the reliable lane will fill it in): a stand-in.
                let recipe = Recipe::unknown("?");
                self.constructs.spawn_with_id(id, stand_in(PlayerId(cs.owner), recipe, cs.pos.get(), tick));
            }
            if let Some(c) = self.constructs.get_mut(id) {
                if cs.flags & cflags::ATTACHED == 0 {
                    c.prev_pos = c.pos;
                    c.pos = cs.pos.get();
                    c.rot = cs.rot.get();
                }
                c.anim_state = AnimState::from_u8(cs.anim);
                c.anim_since = (tick & !0xFFFF) | cs.anim_since as Tick;
                if c.anim_since > tick {
                    c.anim_since = c.anim_since.saturating_sub(0x1_0000);
                }
                c.integrity = c.max_integrity * cs.integrity as f32 / 255.0;
                c.forming_t = if cs.flags & cflags::FORMING != 0 { c.forming_t.max(DT) } else { 0.0 };
                c.suit = cs.flags & cflags::SUIT != 0;
                c.colossal = cs.flags & cflags::COLOSSAL != 0;
                c.size_scale = (cs.size as f32 / 50.0).clamp(0.1, 5.0);
                {
                    use crate::constructs::components::XMode;
                    let x = &mut c.comp.xform;
                    x.progress = (cs.xform & 31) as f32 / 31.0;
                    x.busy = cs.xform & 32 != 0;
                    x.combined = cs.xform & 64 != 0;
                    x.want = if cs.xform & 128 != 0 { XMode::Robot } else { XMode::Alt };
                    x.mode = if x.progress >= 0.5 { XMode::Robot } else { XMode::Alt };
                }
                if let Some(w) = c.weapon.as_mut() {
                    w.phase = crate::weapons::WeaponPhase::from_u8(cs.weapon & 7);
                    let level = (cs.weapon >> 3) as f32 / 31.0;
                    w.spin = level;
                    w.charge = level;
                }
            }
        }
        // Projectiles: rebuild from segments (positions advanced to now).
        self.projectiles.clear();
        for ps in &snap.projectiles {
            let kind = ProjectileKind::from_u8(ps.kind);
            let since = {
                let t = (tick & !0xFFFF) | ps.since as Tick;
                if t > tick { t.saturating_sub(0x1_0000) } else { t }
            };
            let dt = tick.saturating_sub(since) as f32 * DT;
            let origin = ps.origin.get();
            let v0 = ps.vel.get();
            let g = Vec3::new(0.0, -kind.gravity(), 0.0);
            let mut spec = ProjectileSpawn::ring_blast(PlayerId(ps.owner), origin, v0);
            spec.kind = kind;
            spec.vel = v0;
            self.projectiles.push(Projectile {
                id: ps.id,
                spec,
                pos: origin + v0 * dt + g * (0.5 * dt * dt),
                vel: v0 + g * dt,
                age: dt,
                origin,
                vel0: v0,
                segment_tick: since,
                lag: 0.0,
                travelled: 0.0,
                portal_cd: 0.0,
                dead: false,
            });
        }
        self.portals.clear();
        for p in &snap.portals {
            let end = |e: &PortalEndSnap| {
                let mut pe = PortalEnd::new(e.pos.get(), e.normal.get(), e.up.get(), e.radius as f32 / 20.0);
                pe.hp = e.hp as f32;
                pe
            };
            self.portals.push(PortalPair { id: p.id, owner: PlayerId(p.owner), ends: [end(&p.ends[0]), end(&p.ends[1])], expires: Tick::MAX, construct: None });
        }
        self.fields.clear();
        for f in &snap.fields {
            self.fields.push(ForceField { id: f.id, owner: PlayerId(f.owner), pos: f.pos.get(), radius: f.radius as f32 / 4.0, strength: f.strength as f32, expires: Tick::MAX, construct: None });
        }
        self.links.clear();
        let end = |e: &LinkEndSnap| match *e {
            LinkEndSnap::Player(p) => LinkEnd::Player(PlayerId(p)),
            LinkEndSnap::Construct(c) => LinkEnd::Construct(ConstructId(c)),
            LinkEndSnap::Point(q) => LinkEnd::Point(q.get()),
        };
        for l in &snap.links {
            self.links.push(Link { id: l.id, owner: None, a: end(&l.a), b: end(&l.b), max_len: l.max_len as f32 / 4.0, expires: Tick::MAX, hp: 1.0 });
        }
        // Colliders for prediction.
        self.colliders.clear();
        self.constructs.write_colliders(&mut self.colliders);
    }

    /// Client mirror: apply a reliable-lane event (terrain edits, construct models, joins, arena
    /// resets). Purely cosmetic events are ignored here (the client handles juice itself).
    pub fn apply_event(&mut self, e: &GameEvent) {
        match e {
            GameEvent::Carve { op } => {
                let _ = super::combat::apply_carve_op(&mut self.voxels, op);
            }
            // POWERS HOOK: Crazy Diamond rebuilt terrain.
            GameEvent::Power(crate::powers::PowerEvent::Restored { voxels, .. }) => {
                crate::powers::apply_restore(&mut self.voxels, voxels);
            }
            GameEvent::Extracted { op, construct, .. } => {
                let out = apply_extract(&mut self.voxels, op);
                if let Some(c) = self.constructs.get_mut(*construct) {
                    let (half, lc) = out.model.body_box();
                    c.half = half;
                    c.local_center = lc;
                    c.model = Some(Arc::new(out.model));
                }
            }
            GameEvent::ConstructSpawned { id, owner, recipe, pos, rot, forming, cost } => {
                let r = (**recipe).clone();
                let model = self.constructs.get(*id).and_then(|c| c.model.clone());
                let mut sp = stand_in(*owner, r, *pos, self.tick);
                sp.rot = *rot;
                sp.forming = *forming;
                sp.cost = *cost;
                sp.model = model;
                self.constructs.spawn_with_id(*id, sp);
            }
            GameEvent::Grafted { id, recipe, .. } => {
                if let Some(c) = self.constructs.get_mut(*id) {
                    c.recipe = (**recipe).clone();
                }
            }
            GameEvent::PlayerJoined { player, name, corps } => {
                match self.pidx(*player) {
                    Some(i) => {
                        self.players[i].name = name.clone();
                        self.players[i].corps = *corps;
                    }
                    None => {
                        let _ = self.add_player(*player, name.clone(), *corps);
                    }
                }
            }
            GameEvent::PlayerLeft { player } => {
                if let Some(i) = self.pidx(*player) {
                    self.players.remove(i);
                }
            }
            GameEvent::ArenaReset { seed } => {
                let src = match self.cfg.arena {
                    super::ArenaSource::Archipelago { .. } => super::ArenaSource::Archipelago { seed: *seed },
                    other => other,
                };
                // Built in the background since the match ended (see `MatchPhase` below); only
                // falls back to building here if the host skipped the scoreboard.
                let (w, info, s) = match self.take_prefetched_arena_for(src) {
                    Some(b) => b,
                    None => super::build_arena(src),
                };
                self.voxels = w;
                self.arena = info;
                self.arena_seed = s;
            }
            GameEvent::MatchPhase { phase: crate::rules::MatchPhase::Ended, .. } => {
                if self.cfg.rules.regen_islands_between_rounds {
                    self.prefetch_arena();
                }
            }
            GameEvent::ConstructShapes { id, shape, alt_shape } => {
                if let Some(c) = self.constructs.get_mut(*id) {
                    if shape.is_some() {
                        c.shape = shape.clone();
                    }
                }
                let alt = super::actions::sanitize_shape((**alt_shape).clone());
                self.constructs.set_alt_shape(*id, alt);
            }
            GameEvent::ConstructSliced { id, cut, .. } => {
                if let Some(c) = self.constructs.get_mut(*id) {
                    c.slice = Some(*cut);
                }
            }
            _ => {}
        }
    }

    /// Movement modifiers the host would apply to `player` this tick (statuses, worn constructs,
    /// size, suit). Prediction writes these into the state before stepping.
    pub fn move_mods_for(&self, player: PlayerId, input: &PlayerInput, st: &PlayerState) -> MoveModifiers {
        let mut mods = MoveModifiers::default();
        if let Some(p) = self.player(player) {
            p.status.modify_movement(&mut mods);
            mods.speed_mult *= p.size.speed_mult();
            if p.seat.is_some_and(|s| s.suit) {
                mods.speed_mult *= 0.85;
            }
        }
        self.constructs.movement_mods(player, input, st, &mut mods);
        if let Some(p) = self.player(player) {
            p.power.modify_movement(st.grounded, &mut mods); // POWERS HOOK (same order as the host)
        }
        mods
    }

    /// Client prediction: step `state` with `input` exactly like the host does for `player`
    /// (collision against this world's terrain + construct colliders, modifiers, size, portals,
    /// fields). Use it inside `bd_net::Reconciler::predict/reconcile`.
    pub fn predict_player(&self, player: PlayerId, state: &mut PlayerState, input: &PlayerInput) -> MoveEvents {
        let Some(p) = self.player(player) else { return MoveEvents::default() };
        if p.seat.is_some_and(|s| !s.suit) {
            return MoveEvents::default();
        }
        state.mods = self.move_mods_for(player, input, state);
        let mut params = p.params(&self.cfg.move_params);
        crate::party::hooks::tweak_params(self.party.as_ref(), player, &mut params); // PARTY HOOK (Slippery)
        let pre_center = state.hull(&params).center();
        let pre_vel = state.vel;
        let cw = CollisionWorld::new(&self.voxels, &self.colliders).ignoring(crate::movement::ColliderId::Player(player));
        let input = if p.status.stunned() { super::stunned_input(input) } else { *input };
        let ev = step_player(state, &input, &cw, &params, DT);
        let mut inp = input;
        let mut cd = p.portal_cd;
        let _ = super::spatial::player_portals(state, &mut inp, &mut cd, &params, pre_center, pre_vel, &self.portals);
        super::spatial::player_fields(state, player, state.tethered(), p.size.scale, &self.fields, DT);
        ev
    }

    /// Recipes of every live construct (send to a late joiner along with its first keyframe).
    pub fn construct_manifest(&self) -> Vec<GameEvent> {
        let mut v = Vec::new();
        for c in self.constructs.iter().filter(|c| c.gone.is_none()) {
            v.push(GameEvent::ConstructSpawned {
                id: c.id,
                owner: c.owner,
                recipe: Box::new(c.recipe.clone()),
                pos: c.pos,
                rot: c.rot,
                forming: c.forming_t,
                cost: c.cost,
            });
            if let Some(cut) = c.slice {
                v.push(GameEvent::ConstructSliced { id: c.id, parent: cut.parent, cut });
            }
            if let Some(a) = c.alt_shape.clone() {
                v.push(GameEvent::ConstructShapes { id: c.id, shape: c.shape.clone(), alt_shape: a });
            }
        }
        v
    }
}

fn stand_in(owner: PlayerId, recipe: Recipe, pos: Vec3, tick: Tick) -> SpawnParams {
    let forming = forming_secs(recipe.form);
    let life = lifetime_secs(recipe.form);
    SpawnParams {
        owner,
        recipe,
        shape: None,
        pos,
        rot: bd_core::Quat::IDENTITY,
        vel: Vec3::ZERO,
        target: None,
        cost: 0.0,
        integrity: 100.0,
        forming,
        life,
        tick,
        colossal: false,
        model: None,
    }
}

/// Transformer state as one replicated byte (see `ConstructSnap::xform`).
fn xform_wire(c: &crate::constructs::Construct) -> u8 {
    use crate::constructs::components::{XMode, eff_has};
    if !eff_has(c, bd_core::mechanics::Mechanic::Transforms) {
        return 0;
    }
    let x = &c.comp.xform;
    let p = (x.progress.clamp(0.0, 1.0) * 31.0).round() as u8;
    p | (x.busy as u8) << 5 | (x.combined as u8) << 6 | ((x.want == XMode::Robot) as u8) << 7
}

/// The bits of the wire byte that mark a *change* worth an event (target mode, combined).
pub(super) fn xform_key(c: &crate::constructs::Construct) -> u8 {
    let w = xform_wire(c);
    if w == 0 { 0 } else { 1 | (w & (64 | 128)) }
}
