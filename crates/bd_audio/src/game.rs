//! Simulation → sound: the one mapping from `bd_sim` events to cues, shared by the client and the
//! offline renderer (so what the report measures is what the game plays).
//!
//! Also drives the mix state from gameplay (combat heat, heavy-hit ducks, KO negative space,
//! final-blow suck), scares the fauna on explosions, computes occlusion for world one-shots
//! (budgeted per frame), synthesises remote footsteps from player motion (the voice limiter keeps
//! only the nearest few), and runs per-player minigun loops.

use bd_core::PlayerId;
use bd_core::voxel::{Material, VoxelQuery, world_to_voxel};
use bd_sim::GameEvent;
use bd_sim::combat::HitKind;
use bd_sim::events::{GoneReason, move_flags};
use bd_sim::movement::MoveEvents;
use bd_sim::rules::MatchPhase;
use bd_sim::weapons::GunArchetype;
use bd_voxel::{DebrisClass, material_def};
use glam::Vec3;

use crate::acoustics::occlusion;
use crate::ambience::AmbienceDirector;
use crate::director::MixDirector;
use crate::engine::{AudioEngine, PlayOpts, VoiceHandle};

/// KO-streak skinline key of a corps (`ko_streak_<key>`).
pub fn corps_key(c: bd_core::corps::Corps) -> &'static str {
    use bd_core::corps::Corps as C;
    match c {
        C::Willpower => "willpower",
        C::Rage => "rage",
        C::Greed => "greed",
        C::Fear => "fear",
        C::Hope => "hope",
        C::Compassion => "compassion",
        C::Love => "love",
        C::Life => "life",
    }
}

/// Sound family of a voxel material.
pub fn material_family(m: Material) -> &'static str {
    let d = material_def(m);
    let n = d.name;
    if n.contains("glass") || n.contains("neon") || n.contains("solar") || n.contains("light") {
        return "glass";
    }
    if n.contains("hull") || n.contains("steel") || n.contains("mullion") {
        return "metal";
    }
    match d.debris {
        DebrisClass::Crystal => "crystal",
        DebrisClass::Leaves => "leaves",
        DebrisClass::Wood => "wood",
        DebrisClass::Dirt | DebrisClass::Grass | DebrisClass::Sand => "dirt",
        DebrisClass::Stone => "stone",
    }
}

/// Dominant material around a point (shell sample): what a crater was made of.
pub fn material_around<W: VoxelQuery + ?Sized>(w: &W, p: Vec3, r: f32) -> Option<Material> {
    let mut counts: [(Material, u8); 8] = [(0, 0); 8];
    let dirs = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Z,
        Vec3::NEG_Z,
        Vec3::NEG_Y,
        Vec3::Y,
        Vec3::new(0.7, -0.7, 0.0),
        Vec3::new(-0.7, -0.7, 0.0),
        Vec3::new(0.0, -0.7, 0.7),
        Vec3::new(0.0, -0.7, -0.7),
    ];
    for d in dirs {
        let m = w.material(world_to_voxel(p + d * (r + 0.4)));
        if m == 0 {
            continue;
        }
        if let Some(c) = counts.iter_mut().find(|c| c.0 == m || c.1 == 0) {
            c.0 = m;
            c.1 += 1;
        }
    }
    counts.iter().filter(|c| c.1 > 0).max_by_key(|c| c.1).map(|c| c.0)
}

/// Where the ears are + who is who.
pub struct SimCtx<'a, W: VoxelQuery + ?Sized> {
    pub voxels: &'a W,
    pub ear: Vec3,
    pub local: Option<PlayerId>,
    /// Player feet position lookup.
    pub pos_of: &'a dyn Fn(PlayerId) -> Option<Vec3>,
    /// Lower-case corps name of a player (KO streak skinline).
    pub corps_of: &'a dyn Fn(PlayerId) -> Option<&'static str>,
    /// Set by the client: its own movement is voiced from predicted `MoveEvents`, not `Move` events.
    pub local_move_predicted: bool,
    /// Match ends on this KO (last stock / score limit) — the full final-blow treatment.
    pub match_point: bool,
}

struct GunLoop {
    owner: PlayerId,
    handle: VoiceHandle,
    last: f32,
}

/// Per-player footstep accumulator.
#[derive(Clone, Copy, Default)]
struct Stride {
    dist: f32,
    was_grounded: bool,
}

/// A player's charge aura loop.
struct ChargeLoop {
    player: PlayerId,
    handle: VoiceHandle,
    started: f32,
}

pub struct SimAudio {
    time: f32,
    charges: Vec<ChargeLoop>,
    occl_budget: u32,
    guns: Vec<GunLoop>,
    strides: [Stride; 256],
    /// Occlusion raycast pairs allowed per frame for one-shots.
    pub occlusion_per_frame: u32,
}

impl Default for SimAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl SimAudio {
    pub fn new() -> Self {
        Self { time: 0.0, charges: Vec::new(), occl_budget: 0, guns: Vec::new(), strides: [Stride::default(); 256], occlusion_per_frame: 24 }
    }

    /// Start of a frame.
    pub fn begin_frame(&mut self, dt: f32, engine: &mut AudioEngine) {
        self.time += dt.max(0.0);
        self.occl_budget = self.occlusion_per_frame;
        let now = self.time;
        self.guns.retain(|g| {
            if now - g.last > 0.12 {
                engine.stop(g.handle, 0.05);
                engine.play("gun_minigun_spindown", PlayOpts::ui().gain_db(-6.0));
                false
            } else {
                true
            }
        });
    }

    fn at<W: VoxelQuery + ?Sized>(&mut self, ctx: &SimCtx<W>, p: Vec3) -> PlayOpts {
        let mut o = PlayOpts::at(p);
        if self.occl_budget > 0 && p.distance(ctx.ear) < 120.0 {
            self.occl_budget -= 1;
            o.occlusion = occlusion(ctx.voxels, ctx.ear, p);
        }
        o
    }

    /// Own sound if `who` is the local player, else a world sound at `p`.
    fn by<W: VoxelQuery + ?Sized>(&mut self, ctx: &SimCtx<W>, who: Option<PlayerId>, p: Vec3) -> PlayOpts {
        if who.is_some() && who == ctx.local { PlayOpts::own() } else { self.at(ctx, p) }
    }

    fn play_mat(engine: &mut AudioEngine, prefix: &str, fam: &str, o: PlayOpts) {
        let mut buf = [0u8; 48];
        let key = join(&mut buf, prefix, fam);
        if engine.play(key, o).is_none() {
            let fallback = match fam {
                "metal" | "glass" | "crystal" => "crystal",
                "leaves" | "dirt" => "dirt",
                _ => "stone",
            };
            let key = join(&mut buf, prefix, fallback);
            if engine.play(key, o).is_none() {
                let key = join(&mut buf, prefix, "stone");
                engine.play(key, o);
            }
        }
    }

    /// The mix's reaction to an event, without voicing it: combat heat, heavy-hit / KO / final-blow
    /// ducks, the danger "rung bell", fauna scare. The client calls this for every event (its
    /// presenters voice the events); [`SimAudio::event`] calls it too.
    pub fn react<W: VoxelQuery + ?Sized>(
        &mut self,
        e: &GameEvent,
        ctx: &SimCtx<W>,
        engine: &mut AudioEngine,
        director: &mut MixDirector,
        ambience: Option<&mut AmbienceDirector>,
    ) {
        let local = ctx.local;
        let ear = ctx.ear;
        match e {
            GameEvent::Hit { target, source, point, lethal, impact, .. } => {
                let heavy = impact.strength > 1.0 || impact.heavy;
                let local_attacker = source.is_some() && *source == local;
                let local_victim = Some(*target) == local;
                if local_attacker || local_victim {
                    director.combat(1.0);
                    if heavy {
                        director.heavy_hit(engine);
                        // Rung bell: a heavy hit taken while already in danger.
                        if local_victim && director.weights().danger > 0.5 {
                            engine.play("tinnitus", PlayOpts::ui().gain_db(-4.0));
                        }
                    }
                    if *lethal {
                        if ctx.match_point {
                            director.final_blow(engine);
                        } else {
                            engine.play("final_blow_suck", PlayOpts::ui().gain_db(-6.0));
                        }
                    }
                } else {
                    director.combat_at(0.5, *point, ear);
                }
            }
            GameEvent::WallSplat { point, .. } => director.combat_at(0.6, *point, ear),
            GameEvent::Ko { killer: Some(k), .. } if Some(*k) == local => director.ko_confirm(engine),
            GameEvent::Explosion { pos, radius, .. } => {
                director.combat_at(0.8, *pos, ear);
                if let Some(a) = ambience {
                    a.scare(*pos, (radius / 5.0).min(1.5), ear);
                }
                if pos.distance(ear) < 25.0 && *radius >= 4.0 {
                    director.heavy_hit(engine);
                }
            }
            GameEvent::ShotFired { owner, muzzle, .. } => {
                if Some(*owner) == local {
                    director.combat(0.3);
                } else {
                    director.combat_at(0.25, *muzzle, ear);
                }
            }
            GameEvent::Clash { point, .. } => director.combat_at(0.5, *point, ear),
            _ => {}
        }
    }

    /// Voice one simulation event (and react to it). The offline renderer's full mapping.
    pub fn event<W: VoxelQuery + ?Sized>(
        &mut self,
        e: &GameEvent,
        ctx: &SimCtx<W>,
        engine: &mut AudioEngine,
        director: &mut MixDirector,
        ambience: Option<&mut AmbienceDirector>,
    ) {
        self.react(e, ctx, engine, director, ambience);
        let local = ctx.local;
        let ear = ctx.ear;
        match e {
            GameEvent::Hit { target, source, kind, point, crit, lethal, impact, .. } => {
                let p = *point;
                let strength = impact.strength;
                let heavy = strength > 1.0 || impact.heavy;
                let local_attacker = source.is_some() && *source == local;
                let local_victim = Some(*target) == local;
                let name = if heavy {
                    "hit_heavy"
                } else if strength > 0.45 {
                    "hit_medium"
                } else {
                    "hit_light"
                };
                let mut o = self.at(ctx, p).gain_db(-4.0 + 4.0 * strength.min(1.0));
                if local_victim {
                    o = PlayOpts::own().gain_db(o.gain_db);
                }
                engine.play(name, o);
                if matches!(kind, HitKind::RingBlast) {
                    engine.play("ring_blast_hit", self.at(ctx, p).gain_db(-3.0));
                }
                if local_attacker {
                    engine.play(if strength > 0.8 { "hit_confirm_heavy" } else { "hit_confirm" }, PlayOpts::ui());
                }
                if *crit {
                    let o = if local_attacker || local_victim { PlayOpts::ui() } else { self.at(ctx, p) };
                    engine.play("hit_crit", o);
                }
                let _ = lethal;
            }
            GameEvent::Blocked { point, .. } => {
                engine.play("shield_block", self.at(ctx, *point));
            }
            GameEvent::WallSplat { player, point, speed, .. } => {
                engine.play("wall_splat", self.by(ctx, Some(*player), *point).gain_db(((speed / 25.0).clamp(0.6, 1.2)).log10() * 20.0));
            }
            GameEvent::Ko { victim, killer, streak, double, void, shutdown, pos, .. } => {
                let p = *pos;
                engine.play("ko_shatter", self.at(ctx, p));
                if let Some(k) = killer
                    && let Some(corps) = (ctx.corps_of)(*k)
                {
                    let mut buf = [0u8; 48];
                    let name = join(&mut buf, "ko_streak_", corps);
                    let step = (*streak).clamp(1, 5) as u8;
                    if Some(*k) == local {
                        engine.play(name, PlayOpts::ui().step(step));
                        if *void {
                            engine.play("ko_accent_void", PlayOpts::ui());
                        }
                        if *double {
                            engine.play("ko_accent_double", PlayOpts::ui());
                        }
                        if *shutdown {
                            engine.play("ko_streak_end", PlayOpts::ui());
                        }
                    } else {
                        // You can hear who is streaking — in the world, at the KO.
                        let o = self.at(ctx, p).step(step).gain_db(-6.0);
                        engine.play(name, o);
                    }
                }
                if Some(*victim) == local {
                    engine.play("ko", PlayOpts::ui());
                }
            }
            GameEvent::Explosion { pos, radius, .. } => {
                let p = *pos;
                let big = *radius >= 4.0;
                engine.play(if big { "explosion_big" } else { "explosion_small" }, self.at(ctx, p));
                engine.play("debris_patter", self.at(ctx, p).gain_db(-4.0));
            }
            GameEvent::Carve { op } => {
                if let bd_voxel::CarveShape::Sphere { center, radius } = op.shape
                    && let Some(m) = material_around(ctx.voxels, center, radius)
                {
                    let fam = material_family(m);
                    let o = self.at(ctx, center).gain_db((radius / 3.0).clamp(0.3, 1.5).log10() * 20.0);
                    Self::play_mat(engine, "break_", fam, o);
                }
            }
            GameEvent::PunchThrough { player, entry, material, .. } => {
                let fam = material_family(*material);
                let o = self.by(ctx, Some(*player), *entry);
                Self::play_mat(engine, "break_", fam, o);
                engine.play("hit_heavy", o);
            }
            GameEvent::ShotFired { owner, archetype, muzzle, dir, .. } => {
                let mut o = self.by(ctx, Some(*owner), *muzzle);
                // A gun pointed at you is a threat: it wins voice stealing.
                if Some(*owner) != local {
                    let to_me = (ear - *muzzle).normalize_or_zero();
                    if dir.normalize_or_zero().dot(to_me) > 0.97 {
                        o = o.priority(25.0);
                    }
                }
                let name = match archetype {
                    GunArchetype::Ring => "ring_blast",
                    GunArchetype::HandCannon => "gun_handcannon",
                    GunArchetype::SuperShotgun => "gun_shotgun",
                    GunArchetype::Rocket => "gun_rocket_fire",
                    GunArchetype::Railgun => "gun_railgun_fire",
                    GunArchetype::Bow => "tether_fire",
                    GunArchetype::Minigun => {
                        let now = self.time;
                        if let Some(g) = self.guns.iter_mut().find(|g| g.owner == *owner) {
                            g.last = now;
                            engine.update(g.handle, *muzzle, o.gain_db, 1.0, o.occlusion);
                        } else if let Some(h) = engine.play("gun_minigun_loop", o) {
                            self.guns.push(GunLoop { owner: *owner, handle: h, last: now });
                        }
                        ""
                    }
                    #[allow(unreachable_patterns)]
                    _ => "gun_handcannon",
                };
                if !name.is_empty() {
                    engine.play(name, o);
                }
            }
            GameEvent::ShotTrace { from, to, normal, hit_player, .. } => {
                if *normal != Vec3::ZERO && hit_player.is_none() && from.distance(*to) > 0.5 {
                    let m = ctx.voxels.material(world_to_voxel(*to - *normal * 0.25));
                    if m != 0 {
                        let o = self.at(ctx, *to);
                        Self::play_mat(engine, "bullet_impact_", material_family(m), o);
                    }
                }
            }
            GameEvent::ProjectileEnded { pos, hit_player, .. } => {
                if hit_player.is_none() {
                    engine.play("bullet_impact_stone", self.at(ctx, *pos).gain_db(-3.0));
                }
            }
            GameEvent::Clash { point, strength, .. } => {
                engine.play("clash", self.at(ctx, *point).gain_db(((0.6 + strength * 0.4).min(1.3)).log10() * 20.0));
            }
            GameEvent::ConstructSpawned { owner, pos, .. } => {
                let o = self.by(ctx, Some(*owner), *pos);
                engine.play("construct_form", o);
            }
            GameEvent::ConstructShattered { pos, .. } => {
                engine.play("construct_shatter", self.at(ctx, *pos));
                engine.play("break_hardlight", self.at(ctx, *pos).gain_db(-4.0));
            }
            GameEvent::ConstructGone { reason: GoneReason::Dismissed, .. } => {
                engine.play("construct_dismiss", PlayOpts::ui().gain_db(-6.0));
            }
            GameEvent::Respawn { player, pos, .. } => {
                let o = if Some(*player) == local { PlayOpts::ui() } else { self.at(ctx, *pos).gain_db(-4.0) };
                engine.play("respawn", o);
            }
            GameEvent::MatchPhase { phase, .. } => match phase {
                MatchPhase::Live => {
                    engine.play("match_start", PlayOpts::ui());
                }
                MatchPhase::Ended => {
                    engine.play("match_end", PlayOpts::ui());
                }
                _ => {}
            },
            GameEvent::HeldFx { owner, fx, pos, strength, .. } => {
                use bd_sim::events::HeldFx as H;
                let o = self.by(ctx, Some(*owner), *pos);
                match fx {
                    H::SwingImpact | H::DiveSlam | H::GroundPound => {
                        engine.play(if *strength > 1.5 { "hit_heavy" } else { "hit_medium" }, o);
                    }
                    H::Parry => {
                        engine.play("shield_block", o.pitch(1.3));
                    }
                    H::Block => {
                        engine.play("shield_block", o);
                    }
                    _ => {}
                }
            }
            GameEvent::Charge { player, kind, .. } => {
                if let Some(p) = (ctx.pos_of)(*player) {
                    self.charge(ctx, *player, *kind, p, engine);
                }
            }
            GameEvent::Tech { pos, .. } => {
                engine.play("shield_block", self.at(ctx, *pos).pitch(1.6).gain_db(-4.0));
            }
            GameEvent::CreatureAttack { impact, .. } if *impact >= 1.5 => {
                // Position comes from the construct store on the client; the offline run has none.
            }
            GameEvent::Move { player, flags, speed } => {
                if ctx.local_move_predicted && Some(*player) == local {
                    return;
                }
                let Some(p) = (ctx.pos_of)(*player) else { return };
                let f = *flags;
                let o = self.by(ctx, Some(*player), p);
                if f & move_flags::JUMP != 0 {
                    engine.play("jump", o.pitch(if f & move_flags::PERFECT_HOP != 0 { 1.03 } else { 1.0 }));
                }
                if f & move_flags::LAND_HARD != 0 {
                    engine.play("land_heavy", o);
                }
                if f & move_flags::TETHER_FIRE != 0 {
                    engine.play("tether_fire", o);
                }
                if f & move_flags::TETHER_ATTACH != 0 {
                    engine.play("tether_attach", o);
                }
                if f & move_flags::TETHER_RELEASE != 0 {
                    engine.play("tether_release", o);
                }
                if f & move_flags::BOOST != 0 {
                    engine.play("boost_burst", o);
                }
                if f & move_flags::WALL_HIT != 0 && *speed > 8.0 {
                    engine.play("land_heavy", o.pitch(1.06));
                }
            }
            _ => {}
        }
    }

    /// CHARGE HOOK (anime power-up): start / full / broken / released for `player` at `pos`.
    pub fn charge<W: VoxelQuery + ?Sized>(&mut self, ctx: &SimCtx<W>, player: PlayerId, kind: bd_sim::charge::ChargeEvent, pos: Vec3, engine: &mut AudioEngine) {
        use bd_sim::charge::ChargeEvent as C;
        let o = self.by(ctx, Some(player), pos);
        let stop = |s: &mut Self, engine: &mut AudioEngine, fade: f32| {
            s.charges.retain(|c| {
                if c.player == player {
                    engine.stop(c.handle, fade);
                    false
                } else {
                    true
                }
            });
        };
        match kind {
            C::Start => {
                stop(self, engine, 0.05);
                if let Some(h) = engine.play("charge_loop", o.gain_db(-8.0)) {
                    self.charges.push(ChargeLoop { player, handle: h, started: self.time });
                }
            }
            C::Full => {
                engine.play("charge_full", o);
            }
            C::Broken => {
                stop(self, engine, 0.03);
                engine.play("charge_break", o);
            }
            C::Released => {
                stop(self, engine, 0.25);
                engine.play("charge_release", o);
            }
        }
    }

    /// Per frame: charge auras follow their player and rise with how long they have charged
    /// (pitch 1 → 1.5, +8 dB over ~3 s). `pos_of` = current position (None = gone → stop).
    pub fn charge_frame(&mut self, local: Option<PlayerId>, pos_of: &dyn Fn(PlayerId) -> Option<Vec3>, engine: &mut AudioEngine) {
        let now = self.time;
        self.charges.retain(|c| match pos_of(c.player) {
            Some(p) => {
                let level = ((now - c.started) / 3.0).clamp(0.0, 1.0);
                let p = if Some(c.player) == local { engine.listener().pos } else { p };
                engine.update(c.handle, p, -8.0 + 8.0 * level, 1.0 + 0.5 * level * level, 0.0);
                true
            }
            None => {
                engine.stop(c.handle, 0.1);
                false
            }
        });
    }

    /// The local player's predicted movement events (client) — same sounds as `Move`, own-voiced.
    pub fn local_move(&mut self, ev: &MoveEvents, engine: &mut AudioEngine) {
        let o = PlayOpts::own();
        if ev.jumped {
            engine.play("jump", o.pitch(if ev.perfect_hop { 1.03 } else { 1.0 }));
        }
        if let Some(l) = ev.landed {
            if l.impact_speed > 14.0 {
                engine.play("land_heavy", o.gain_db(((l.impact_speed / 25.0).clamp(0.6, 1.2)).log10() * 20.0));
            } else if l.impact_speed > 3.0 {
                engine.play("land_light", o.gain_db(((l.impact_speed / 12.0).clamp(0.4, 1.0)).log10() * 20.0));
            }
        }
        for i in 0..2 {
            if ev.tether_fired[i] {
                engine.play("tether_fire", o);
            }
            if ev.tether_attached[i] {
                engine.play("tether_attach", o);
            }
            if ev.tether_released[i] || ev.tether_broken[i] {
                engine.play("tether_release", o);
            }
        }
        if ev.boost_burst {
            engine.play("boost_burst", o);
        }
        if let Some(w) = ev.wall_hit
            && w.speed > 8.0
        {
            engine.play("land_heavy", o.pitch(1.06));
        }
    }

    /// Footsteps for everyone from their motion this frame (`moved` = horizontal metres).
    /// The per-sound voice limit keeps only the nearest few audible.
    pub fn footsteps<W: VoxelQuery + ?Sized>(
        &mut self,
        ctx: &SimCtx<W>,
        who: PlayerId,
        feet: Vec3,
        moved: f32,
        grounded: bool,
        engine: &mut AudioEngine,
    ) {
        let s = &mut self.strides[who.0 as usize];
        if !grounded {
            s.dist = 1.8; // first step comes quickly after landing
            s.was_grounded = false;
            return;
        }
        s.was_grounded = true;
        s.dist += moved.max(0.0);
        if s.dist > 2.6 {
            s.dist = 0.0;
            let o = self.by(ctx, Some(who), feet);
            engine.play("footstep", o);
        }
    }
}

/// `prefix + name` into a stack buffer (no allocation per event).
fn join<'a>(buf: &'a mut [u8; 48], a: &str, b: &str) -> &'a str {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let n = (a.len() + b.len()).min(48);
    let na = a.len().min(n);
    buf[..na].copy_from_slice(&a[..na]);
    buf[na..n].copy_from_slice(&b[..n - na]);
    std::str::from_utf8(&buf[..n]).unwrap_or("")
}
