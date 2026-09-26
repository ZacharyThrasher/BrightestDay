//! `--scenario showcase`: a scripted ~90 s highlight reel on the current map (Archipelago by default,
//! `--map miami` works too) with 8 bots. Casts are **curated, not random**, but every phrase goes
//! through the real ring path — `World::parse_power_phrase` → `console::resolve_phrase` (the loaded
//! `RingLexicon`) → `World::cast_shaped` / `cast_incantation_shaped` / `power_move` / `stand_rush` —
//! exactly what a player's console does. Actors are bots whose input is scripted for the beat
//! (placed, aiming, pressing buttons); everyone else fights normally (casts off so the reel stays
//! curated). The director camera ([`crate::present::director`]) is told what to frame per beat.
//!
//! Stages are found from the terrain at start (flat open ground, a tall thin wall to punch through,
//! a ledge over the void), so the script works on any seed / map.
//!
//! Beats: Godzilla · Mjolnir thrown + returning · Optimus / Ironhide / Bumblebee → "roll out" →
//! "combine" (Devastator) · steam train through a tower · giant eeyore shootin laser beams · very
//! angry bees · black hole · portal · flying cage over the void · gomu gomu no anvil · stand rush
//! (ORA) · TMNT squad · heat vision · side launch + tether recovery · hulk smash star KO.

use crate::console::{resolve_phrase, RingLexicon, Resolved};
use crate::practice::SimArgs;
use crate::present::camera3p::ViewMode;
use crate::present::director::{Director, Focus, ScriptShot, Subject};
use crate::sim_bridge::{ActiveWorld, SimRole, SimTiming, ViewKeys};
use crate::state::{AfterLoading, AppState};
use crate::tick::{SimSet, SimTick};
use crate::util::v3;
use bd_core::input::{buttons, PlayerInput};
use bd_core::{ConstructId, IVec3, PlayerId, Vec3 as SV, VOXEL_SIZE};
use bd_lexicon::Lexicon;
use bd_sim::bots::BotBrain;
use bd_sim::{Aim, GameEvent, TargetHint, World};
use bevy::picking::Pickable;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use std::sync::Arc;

pub fn showcase_requested() -> bool {
    let a: Vec<String> = std::env::args().collect();
    a.windows(2).any(|w| w[0] == "--scenario" && w[1] == "showcase")
}

pub struct ShowcasePlugin;

impl Plugin for ShowcasePlugin {
    fn build(&self, app: &mut App) {
        if !showcase_requested() {
            return;
        }
        app.insert_resource(AfterLoading(AppState::InGame))
            .add_systems(OnEnter(AppState::InGame), start_showcase.after(crate::player::SpawnPlayerSet))
            .add_systems(SimTick, showcase_tick.in_set(SimSet::Simulate).run_if(resource_exists::<Showcase>))
            .add_systems(Update, draw_caption.run_if(resource_exists::<Showcase>));
    }
}

// ------------------------------------------------------------------------------------ state

/// Scripted input for a bot this beat.
#[derive(Clone, Copy, Debug)]
struct Actor {
    look: Look,
    held: u16,
    /// Buttons pressed for `pulse_ticks` more ticks.
    pulse: u16,
    pulse_ticks: u8,
    move_z: i8,
    move_x: i8,
    seq: u32,
}

#[derive(Clone, Copy, Debug)]
enum Look {
    Player(PlayerId),
    Point(SV),
}

impl Actor {
    fn at(look: Look) -> Self {
        Self { look, held: 0, pulse: 0, pulse_ticks: 0, move_z: 0, move_x: 0, seq: 0 }
    }
}

#[derive(Clone, Copy, Debug)]
struct Stage {
    pos: SV,
    /// Horizontal unit direction toward the open side.
    fwd: SV,
}

impl Stage {
    fn side(&self) -> SV {
        SV::new(-self.fwd.z, 0.0, self.fwd.x)
    }
}

#[derive(Clone, Debug, Default)]
struct Stages {
    flat: Vec<Stage>,
    /// Caster spot, direction to the wall, distance to the wall.
    tower: Option<(Stage, f32)>,
    /// A spot `edge` m from a ledge over the void along `fwd`.
    edge: Option<(Stage, f32)>,
}

#[derive(Default)]
struct BeatState {
    fired: u64,
    roles: Vec<PlayerId>,
    ids: [Option<ConstructId>; 6],
    flag: bool,
    n: u32,
}

#[derive(Resource)]
pub struct Showcase {
    bots: Vec<BotBrain>,
    actors: HashMap<PlayerId, Actor>,
    lex: Option<Arc<Lexicon>>,
    stages: Stages,
    ticks: u64,
    /// Seconds since the script started (sim time).
    t: f32,
    beat: usize,
    beat_t: f32,
    bs: BeatState,
    rotate: usize,
    events: Vec<GameEvent>,
    caption: Option<(String, Color, f32)>,
    done: bool,
}

#[derive(Component)]
struct Caption;

// ------------------------------------------------------------------------------------ setup

#[allow(clippy::too_many_arguments)]
fn start_showcase(
    mut commands: Commands,
    args: Res<SimArgs>,
    mut copy: ResMut<crate::arena::SimArenaCopy>,
    mut director: ResMut<Director>,
    mut mode: ResMut<ViewMode>,
    ono: Option<ResMut<bd_fx::OnomatopoeiaSettings>>,
    existing: Option<Res<ActiveWorld>>,
) {
    if existing.is_some() {
        return;
    }
    let mut cfg = crate::practice::practice_config(args.seed, crate::arena::ARENA_SEED);
    cfg.rules.willpower = bd_sim::rules::WillpowerMode::Infinite;
    cfg.rules.caps = bd_sim::rules::CapMode::Chaos;
    cfg.rules.respawn_secs = 2.0;
    let mut world = crate::practice::build_world(&mut copy, cfg);
    let mut bots = bd_sim::bots::add_bots(&mut world, 0, 8, 0.85, args.seed);
    for b in &mut bots {
        b.casts_enabled = false;
    }
    world.start_live();
    let stages = find_stages(&world);
    info!("showcase: {} flat stages, tower {:?}, edge {:?}", stages.flat.len(), stages.tower.map(|t| t.1), stages.edge.map(|e| e.1));
    let mut active = ActiveWorld::new(world, None, SimRole::Offline);
    // The bridge must not tick it: the showcase does (scripted inputs).
    active.paused = true;
    commands.insert_resource(active);
    commands.insert_resource(Showcase {
        bots,
        actors: HashMap::default(),
        lex: None,
        stages,
        ticks: 0,
        t: 0.0,
        beat: 0,
        beat_t: 0.0,
        bs: BeatState::default(),
        rotate: 0,
        events: Vec::with_capacity(256),
        caption: None,
        done: false,
    });
    director.enabled = true;
    mode.director = true;
    if let Some(mut o) = ono {
        o.mode = bd_fx::OnoMode::FullComic;
    }
    commands.spawn((
        Caption,
        Text::new(""),
        TextFont::from_font_size(30.0),
        TextColor(Color::WHITE),
        TextShadow { offset: Vec2::new(2.0, 2.0), color: Color::srgba(0.0, 0.0, 0.0, 0.85) },
        Node { position_type: PositionType::Absolute, left: px(48), bottom: px(40), ..default() },
        Pickable::IGNORE,
        GlobalZIndex(60),
        Visibility::Hidden,
    ));
}

// ------------------------------------------------------------------------------------ terrain probes

fn solid(w: &World, p: SV) -> bool {
    w.voxels.get_at(p) != 0
}

/// Feet position on the ground surface nearest to `p.y` (standing room above), within -12..+8 m.
fn ground(w: &World, p: SV) -> Option<SV> {
    let vs = VOXEL_SIZE;
    let x = (p.x / vs).floor() as i32;
    let z = (p.z / vs).floor() as i32;
    let y0 = (p.y / vs).floor() as i32;
    let mut best: Option<(i32, f32)> = None;
    for y in (y0 - 24)..=(y0 + 16) {
        let s = |dy: i32| w.voxels.get(IVec3::new(x, y + dy, z)) != 0;
        if s(0) && !s(1) && !s(2) && !s(3) && !s(4) {
            let top = (y + 1) as f32 * vs;
            let d = (top - p.y).abs();
            if best.is_none_or(|b| d < b.1) {
                best = Some((y, d));
            }
        }
    }
    best.map(|(y, _)| SV::new(p.x, (y + 1) as f32 * vs, p.z))
}

fn flat_score(w: &World, p: SV) -> (u32, SV) {
    let mut ok = 0;
    let mut best_dir = SV::X;
    let mut best_run = 0.0f32;
    for k in 0..16 {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        let d = SV::new(a.cos(), 0.0, a.sin());
        let mut run = 0.0;
        for step in 1..=10 {
            let q = p + d * (step as f32 * 2.0);
            match ground(w, q) {
                // Walkable, level and open to the sky (no canopy / arch overhead: camera room).
                Some(g) if (g.y - p.y).abs() < 1.6 && (1..=14).all(|j| !solid(w, g + SV::Y * j as f32)) => {
                    run = step as f32 * 2.0;
                    if step <= 7 {
                        ok += 1;
                    }
                }
                _ => break,
            }
        }
        if run > best_run {
            best_run = run;
            best_dir = d;
        }
    }
    (ok, best_dir)
}

fn find_stages(w: &World) -> Stages {
    let spawns: Vec<SV> = w.arena.spawns.clone();
    // Clutter: solid voxels in the air around a spot (trees, arches, ruins block the camera).
    // (Per mille of a 30 × 18 × 30 m box above the spot; 1 m columns catch tree trunks.)
    let clutter = |p: SV| -> u32 {
        let mut n = 0;
        let mut total = 0;
        for dx in -15..=15 {
            for dz in -15..=15 {
                for dy in 1..=12 {
                    total += 1;
                    if solid(w, p + SV::new(dx as f32, dy as f32 * 1.5, dz as f32)) {
                        n += 1;
                    }
                }
            }
        }
        n * 1000 / total.max(1)
    };
    // Scan the whole map for open, level, uncluttered ground; keep the best, spread apart.
    let b = w.arena.bounds;
    let mut cands: Vec<(f32, Stage)> = Vec::new();
    let mut x = b.min.x + 6.0;
    while x < b.max.x - 6.0 {
        let mut z = b.min.z + 6.0;
        while z < b.max.z - 6.0 {
            let (vx, vz) = ((x / VOXEL_SIZE).floor() as i32, (z / VOXEL_SIZE).floor() as i32);
            if let Some(ty) = w.voxels.top_solid_y(vx, vz) {
                let g = SV::new(x, (ty + 1) as f32 * VOXEL_SIZE, z);
                if g.y > w.arena.void_y + 6.0 {
                    let (ok, fwd) = flat_score(w, g);
                    if ok >= 70 {
                        let score = ok as f32 * 0.5 - clutter(g) as f32;
                        cands.push((score, Stage { pos: g, fwd }));
                    }
                }
            }
            z += 5.0;
        }
        x += 5.0;
    }
    cands.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut all: Vec<Stage> = Vec::new();
    for (_, st) in &cands {
        if all.len() < 14 && all.iter().all(|o| o.pos.distance(st.pos) > 20.0) {
            all.push(*st);
        }
    }
    for (i, st) in all.iter().enumerate() {
        let sc = cands.iter().find(|c| c.1.pos == st.pos).map_or(0.0, |c| c.0);
        info!("showcase: stage {i} at {:.1} fwd {:.2} score {sc:.1} clutter {}", st.pos, st.fwd, clutter(st.pos));
    }
    if all.is_empty() {
        all.push(Stage { pos: ground(w, w.arena.center + SV::Y * 4.0).unwrap_or(w.arena.center), fwd: SV::X });
    }
    // Tower: a tall, thin wall 8–16 m away across walkable ground.
    let mut tower: Option<(Stage, f32, f32)> = None;
    let mut edge: Option<(Stage, f32, f32)> = None;
    let mut probes: Vec<SV> = spawns.iter().filter_map(|s| ground(w, *s)).collect();
    for (_, st) in cands.iter().take(200) {
        probes.push(st.pos);
    }
    for p in &probes {
        let cl = clutter(*p) as f32;
        for k in 0..32 {
            let a = k as f32 / 32.0 * std::f32::consts::TAU;
            let d = SV::new(a.cos(), 0.0, a.sin());
            // --- tower
            if let Some(h) = bd_core::voxel::raycast(&w.voxels, *p + SV::Y * 1.2, d, 18.0)
                && (8.0..17.0).contains(&h.t)
            {
                let wall = h.point + d * 0.3;
                let tall = (1..=16).filter(|i| solid(w, wall + SV::Y * (*i as f32 * 0.5 + 0.5))).count();
                let thick = (1..=12).take_while(|i| solid(w, wall + d * (*i as f32 * 0.5))).count() as f32 * 0.5;
                let behind = ground(w, wall + d * (thick + 2.5));
                let walk = (1..(h.t as i32 - 1)).all(|i| ground(w, *p + d * i as f32).is_some_and(|g| (g.y - p.y).abs() < 1.2));
                if tall >= 15 && (1.0..4.5).contains(&thick) && walk {
                    let score = h.t + if behind.is_none() { 2.0 } else { 0.0 } - thick - cl * 0.3;
                    if tower.is_none_or(|t| score > t.2) {
                        tower = Some((Stage { pos: *p, fwd: d }, h.t, score));
                    }
                }
            }
            // --- edge over the void
            let mut gap = None;
            for i in 1..=18 {
                let q = *p + d * i as f32;
                match ground(w, q) {
                    Some(g) if (g.y - p.y).abs() < 1.5 => {}
                    _ => {
                        gap = Some(i as f32);
                        break;
                    }
                }
            }
            if let Some(g) = gap
                && (8.0..=16.0).contains(&g)
            {
                let void = (0..6).all(|i| {
                    let q = *p + d * (g + 2.0 + i as f32 * 3.0);
                    (0..40).all(|j| !solid(w, q + SV::Y * (6.0 - j as f32 * 1.0)))
                });
                if void {
                    let score = g - cl * 0.5;
                    if edge.is_none_or(|e| score > e.2) {
                        edge = Some((Stage { pos: *p, fwd: d }, g, score));
                    }
                }
            }
        }
    }
    Stages { flat: all, tower: tower.map(|t| (t.0, t.1)), edge: edge.map(|e| (e.0, e.1)) }
}

// ------------------------------------------------------------------------------------ the tick

fn showcase_tick(
    mut sc: ResMut<Showcase>,
    active: Option<ResMut<ActiveWorld>>,
    lex: Option<Res<RingLexicon>>,
    mut keys: ResMut<ViewKeys>,
    mut timing: ResMut<SimTiming>,
    mut director: ResMut<Director>,
) {
    let Some(mut active) = active else { return };
    let t0 = std::time::Instant::now();
    let sc = &mut *sc;
    if sc.lex.is_none() {
        sc.lex = lex.map(|l| l.0.clone());
    }
    sc.ticks += 1;
    // Script (commands land between ticks, like the console's).
    if let Some(lex) = sc.lex.clone()
        && !sc.done
    {
        run_script(sc, &mut active.world, &mut director, &lex);
    }
    // Inputs: scripted actors, thinking bots.
    let mut inputs: Vec<(PlayerId, PlayerInput)> = Vec::with_capacity(8);
    let w = &mut active.world;
    for bot in sc.bots.iter_mut() {
        if let Some(a) = sc.actors.get_mut(&bot.id) {
            inputs.push((bot.id, actor_input(w, bot.id, a)));
            continue;
        }
        let act = bot.think(w);
        if let Some(section) = act.begin_oath {
            w.begin_oath(bot.id, section);
        }
        if let Some((acc, secs)) = act.finish_oath {
            let _ = w.oath_result(bot.id, acc, secs);
        }
        inputs.push((bot.id, act.input));
    }
    let evs = w.tick(&inputs);
    sc.events.clear();
    sc.events.extend_from_slice(evs);
    for e in &sc.events {
        active.push_event(e.clone());
    }
    let keys = &mut *keys;
    std::mem::swap(&mut keys.prev, &mut keys.curr);
    active.world.write_snapshot(None, &mut keys.curr);
    if !keys.valid {
        active.world.write_snapshot(None, &mut keys.prev);
        keys.valid = true;
    }
    timing.record(t0.elapsed().as_secs_f32() * 1000.0);
}

fn actor_input(w: &World, id: PlayerId, a: &mut Actor) -> PlayerInput {
    a.seq = a.seq.wrapping_add(1);
    let mut i = PlayerInput { seq: a.seq, ..Default::default() };
    let Some(me) = w.player(id) else { return i };
    let eye = me.eye(w.params());
    let target = match a.look {
        Look::Player(p) => w.player(p).filter(|p| p.alive).map(|p| p.mv.pos + SV::Y * 1.0),
        Look::Point(p) => Some(p),
    };
    let d = target.map(|t| (t - eye).normalize_or_zero()).filter(|d| *d != SV::ZERO).unwrap_or(me.look());
    i.yaw = (-d.x).atan2(-d.z);
    i.pitch = d.y.clamp(-0.99, 0.99).asin();
    i.move_x = a.move_x;
    i.move_z = a.move_z;
    i.buttons = a.held;
    if a.pulse_ticks > 0 {
        i.buttons |= a.pulse;
        a.pulse_ticks -= 1;
    }
    i
}

// ------------------------------------------------------------------------------------ script API

struct Ctx<'a> {
    w: &'a mut World,
    sc: &'a mut Showcase,
    dir: &'a mut Director,
    lex: &'a Lexicon,
    t: f32,
}

impl Ctx<'_> {
    /// True exactly once, the first tick `t >= at`.
    fn at(&mut self, k: u32, at: f32) -> bool {
        let bit = 1u64 << k;
        if self.t >= at && self.sc.bs.fired & bit == 0 {
            self.sc.bs.fired |= bit;
            return true;
        }
        false
    }
    fn role(&self, i: usize) -> PlayerId {
        self.sc.bs.roles.get(i).copied().unwrap_or(PlayerId(i as u8))
    }
    fn alive(&self, id: PlayerId) -> bool {
        self.w.player(id).is_some_and(|p| p.alive)
    }
    fn pos(&self, id: PlayerId) -> SV {
        self.w.player(id).map_or(SV::ZERO, |p| p.mv.pos)
    }
    fn stage(&self, k: usize) -> Stage {
        let f = &self.sc.stages.flat;
        f[k % f.len()]
    }

    /// Put a player at `p` (snapped to ground), facing `face`, still, unshielded.
    fn place(&mut self, id: PlayerId, p: SV, face: SV, pct: f32) {
        let g = ground(self.w, p + SV::Y * 1.0).unwrap_or(p);
        let Some(pl) = self.w.player_mut(id) else { return };
        if !pl.alive {
            return;
        }
        pl.mv.pos = g;
        pl.mv.vel = SV::ZERO;
        pl.last_safe_pos = g;
        pl.spawn_shield = 0.0;
        pl.launch = None;
        pl.damage_pct = pct;
        pl.seat = None;
        let d = (face - g).normalize_or_zero();
        pl.input.yaw = (-d.x).atan2(-d.z);
        self.sc.actors.insert(id, Actor::at(Look::Point(face + SV::Y * 1.2)));
    }
    fn look(&mut self, id: PlayerId, l: Look) {
        self.sc.actors.entry(id).or_insert(Actor::at(l)).look = l;
    }
    fn press(&mut self, id: PlayerId, b: u16, ticks: u8) {
        if let Some(a) = self.sc.actors.get_mut(&id) {
            a.pulse = b;
            a.pulse_ticks = ticks;
        }
    }
    fn hold(&mut self, id: PlayerId, b: u16) {
        if let Some(a) = self.sc.actors.get_mut(&id) {
            a.held = b;
        }
    }
    fn walk(&mut self, id: PlayerId, z: i8) {
        if let Some(a) = self.sc.actors.get_mut(&id) {
            a.move_z = z;
        }
    }
    fn free(&mut self, id: PlayerId) {
        self.sc.actors.remove(&id);
    }

    fn caption(&mut self, id: PlayerId, text: &str) {
        let c = self.w.player(id).map_or([1.0; 3], |p| p.corps.color());
        let col = Color::linear_rgb(c[0], c[1], c[2]);
        self.sc.caption = Some((text.to_string(), col, 0.0));
    }

    /// Say `text` like the ring console would, aiming at `at` (a point), hinting `target`.
    /// Returns the construct it made (if any).
    fn say(&mut self, id: PlayerId, text: &str, at: SV, target: Option<TargetHint>) -> Option<ConstructId> {
        if !self.alive(id) {
            return None;
        }
        self.caption(id, text);
        let eye = self.w.player(id).map(|p| p.eye(self.w.params()))?;
        let dir = (at - eye).normalize_or(SV::NEG_Z);
        // Keep aiming where we cast (thrown objects leave along the look).
        if let Some(a) = self.sc.actors.get_mut(&id) {
            a.look = Look::Point(at);
        }
        if let Some(pl) = self.w.player_mut(id) {
            pl.input.yaw = (-dir.x).atan2(-dir.z);
            pl.input.pitch = dir.y.clamp(-0.99, 0.99).asin();
        }
        let power = self.w.parse_power_phrase(id, text);
        match resolve_phrase(self.lex, power, text, false) {
            Resolved::PowerMove(slot) => {
                let r = self.w.power_move(id, slot);
                info!("showcase: {text:?} -> power move {slot}: {r:?}");
                None
            }
            Resolved::Rush(n) => {
                self.w.stand_rush(id, n);
                None
            }
            Resolved::Cast { res, incantation } => {
                if incantation {
                    info!("showcase: {text:?} is an incantation");
                }
                let modifier = res.kind == bd_lexicon::ResolutionKind::Graft;
                let aim = Aim { origin: eye, dir, target_hint: target, modifier };
                let res = *res;
                let r = if incantation {
                    self.w.cast_incantation_shaped(id, res.recipe, res.shape, res.alt_shape, aim)
                } else {
                    self.w.cast_shaped(id, res.recipe, res.shape, res.alt_shape, aim)
                };
                match r {
                    Ok(c) => {
                        if let Some(k) = self.w.constructs.get(c) {
                            info!("showcase: {text:?} -> #{} {:?} {:?} half {:?} at {:?}", c.0, k.recipe.name, k.recipe.form, k.half, k.pos);
                        }
                        Some(c)
                    }
                    Err(e) => {
                        warn!("showcase: {text:?} failed: {e:?}");
                        None
                    }
                }
            }
        }
    }
    fn cast_at(&mut self, id: PlayerId, text: &str, victim: PlayerId) -> Option<ConstructId> {
        let at = self.pos(victim) + SV::Y * 1.0;
        self.say(id, text, at, Some(TargetHint::Player(victim)))
    }

    fn shot(&mut self, s: ScriptShot) {
        self.dir.direct(s);
    }
    fn on_player(&self, id: PlayerId) -> ScriptShot {
        ScriptShot::on(Subject::Player(id))
    }
    fn on_construct(&self, c: Option<ConstructId>, fallback: PlayerId) -> ScriptShot {
        match c {
            Some(c) => ScriptShot::on(Subject::Construct(c)),
            None => ScriptShot::on(Subject::Player(fallback)),
        }
    }
    fn launched(&self, id: PlayerId) -> bool {
        self.sc.events.iter().any(|e| matches!(e, GameEvent::Launched { player, .. } if *player == id))
    }
    fn largest_of(&self, owner: PlayerId) -> Option<ConstructId> {
        self.w.constructs.owned_by(owner).filter(|c| c.gone.is_none()).max_by(|a, b| a.half.max_element().total_cmp(&b.half.max_element())).map(|c| c.id)
    }
    fn eat_or_grant(&mut self, id: PlayerId, key: &str) {
        let before = self.w.player(id).and_then(|p| p.power.id);
        let want = bd_sim::powers::PowerTable::get().index_of(key);
        if before.is_none() || before != want {
            let ok = self.w.grant_power(id, key);
            info!("showcase: p{} power {key}: had {before:?}, granted {ok}", id.0);
        }
    }
}

fn pv(p: PlayerId) -> Focus {
    Focus::Subject(Subject::Player(p))
}

fn bvec(v: SV) -> Vec3 {
    v3(v)
}

// ------------------------------------------------------------------------------------ beats

type BeatFn = fn(&mut Ctx);

/// (name, seconds, roles, script)
const BEATS: &[(&str, f32, usize, BeatFn)] = &[
    ("godzilla", 8.5, 3, beat_godzilla),
    ("mjolnir", 6.0, 2, beat_mjolnir),
    ("transformers", 11.0, 2, beat_transformers),
    ("train", 7.0, 2, beat_train),
    ("eeyore", 5.5, 2, beat_eeyore),
    ("bees", 5.0, 2, beat_bees),
    ("black hole", 6.0, 3, beat_black_hole),
    ("portal", 5.0, 1, beat_portal),
    ("cage", 6.0, 2, beat_cage),
    ("gomu", 5.5, 2, beat_gomu),
    ("stand", 5.0, 2, beat_stand),
    ("tmnt", 6.0, 2, beat_tmnt),
    ("heat vision", 5.0, 2, beat_heat_vision),
    ("tether", 6.5, 2, beat_tether),
    ("hulk", 7.0, 2, beat_hulk),
];

fn run_script(sc: &mut Showcase, w: &mut World, dir: &mut Director, lex: &Lexicon) {
    let dt = bd_core::DT;
    sc.t += dt;
    if sc.beat >= BEATS.len() {
        if !sc.done {
            sc.done = true;
            sc.actors.clear();
            dir.release();
            info!("showcase: script finished at {:.1} s", sc.t);
        }
        return;
    }
    if sc.beat_t == 0.0
        && let Ok(only) = std::env::var("BD_SHOWCASE_ONLY")
        && !only.split(',').any(|k| k.trim().parse::<usize>().ok() == Some(sc.beat))
    {
        sc.beat += 1;
        return;
    }
    if sc.beat_t == 0.0 {
        // Assign roles: alive bots, rotating so everyone gets a turn.
        let alive: Vec<PlayerId> = sc.bots.iter().map(|b| b.id).filter(|id| w.player(*id).is_some_and(|p| p.alive)).collect();
        let n = BEATS[sc.beat].2;
        let mut roles = Vec::new();
        for k in 0..alive.len() {
            if roles.len() >= n {
                break;
            }
            roles.push(alive[(sc.rotate + k) % alive.len()]);
        }
        sc.rotate += n;
        sc.bs = BeatState { roles, ..Default::default() };
        info!("showcase: beat {} '{}' at {:.1} s", sc.beat, BEATS[sc.beat].0, sc.t);
    }
    if std::env::var_os("BD_SHOWCASE_DEBUG").is_some() && (sc.beat_t / 0.5).floor() != ((sc.beat_t - dt) / 0.5).floor() {
        if let Some(r0) = sc.bs.roles.first() {
            for k in w.constructs.owned_by(*r0) {
                info!(
                    "showcase dbg t={:.1} p{} #{} {:?} half {:?} pos {:?} gone {:?} xform mode {:?} busy {} combined {} hub {:?}",
                    sc.beat_t, r0.0, k.id.0, k.recipe.name, k.half, k.pos, k.gone, k.comp.xform.mode, k.comp.xform.busy, k.comp.xform.combined, k.comp.xform.hub
                );
            }
        }
    }
    let f = BEATS[sc.beat].3;
    let t = sc.beat_t;
    let mut ctx = Ctx { w, sc, dir, lex, t };
    f(&mut ctx);
    sc.beat_t += dt;
    if sc.beat_t >= BEATS[sc.beat].1 {
        // End of beat: actors go back to fighting; the beat's constructs are dismissed so the next
        // stage is clean (big ones keep going for a moment through their own shatter).
        let roles = std::mem::take(&mut sc.bs.roles);
        for r in &roles {
            sc.actors.remove(r);
            w.dismiss(*r);
        }
        sc.beat += 1;
        sc.beat_t = 0.0;
    }
}

fn beat_godzilla(c: &mut Ctx) {
    let (a, b, d) = (c.role(0), c.role(1), c.role(2));
    let st = c.stage(0);
    if c.at(0, 0.0) {
        let spot = st.pos + st.fwd * 15.0;
        c.place(a, st.pos, spot, 0.0);
        c.place(b, spot + st.side() * 3.0, st.pos, 40.0);
        c.place(d, spot - st.side() * 4.0 + st.fwd * 3.0, st.pos, 60.0);
        c.shot(c.on_player(a).with(Focus::Point(bvec(spot))).dist(9.0));
    }
    if c.at(1, 0.5) {
        let spot = st.pos + st.fwd * 14.0;
        let g = c.say(a, "godzilla", spot, Some(TargetHint::Player(b)));
        c.sc.bs.ids[0] = g;
        c.shot(c.on_construct(g, a).with(pv(b)));
    }
    if c.at(2, 1.8) {
        c.free(b);
        c.free(d);
    }
    if c.at(3, 3.5) {
        c.free(a);
    }
}

fn beat_mjolnir(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let st = c.stage(0);
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.place(b, st.pos + st.fwd * 8.5, st.pos, 60.0);
        c.look(a, Look::Player(b));
        c.shot(c.on_player(a).with(pv(b)).side(bvec(st.side())).dist(8.5));
    }
    if c.at(1, 0.3) {
        c.cast_at(a, "mjolnir", b);
    }
    if c.at(5, 1.0) {
        c.look(a, Look::Player(b));
    }
    if c.at(2, 1.3) {
        c.press(a, buttons::SECONDARY, 2);
    }
    if c.at(3, 1.6) {
        c.free(b);
    }
}

fn beat_transformers(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let st = c.stage(1);
    let side = st.side();
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.place(b, st.pos + st.fwd * 14.0, st.pos, 30.0);
        c.shot(c.on_player(a).with(Focus::Point(bvec(st.pos + st.fwd * 5.0))).dist(12.0));
    }
    // Three transformers, cast steeply at the ground close by (so no ray crosses the others).
    if c.at(1, 0.2) {
        let o = c.say(a, "optimus prime", st.pos + st.fwd * 5.0, Some(TargetHint::Player(b)));
        c.sc.bs.ids[0] = o;
        c.shot(c.on_construct(o, a).with(pv(a)).dist(12.0).elev(8.0));
    }
    if c.at(2, 0.55) {
        c.say(a, "ironhide", st.pos + st.fwd * 3.0 + side * 4.5, Some(TargetHint::Player(b)));
    }
    if c.at(3, 0.9) {
        c.say(a, "bumblebee transformer", st.pos + st.fwd * 3.0 - side * 4.5, Some(TargetHint::Player(b)));
    }
    if c.at(4, 1.5) {
        let o = c.sc.bs.ids[0].filter(|o| c.w.constructs.get(*o).is_some_and(|k| k.gone.is_none())).or_else(|| c.largest_of(a));
        let at = o.and_then(|o| c.w.constructs.get(o)).map_or(st.pos + st.fwd * 5.0, |k| k.center());
        c.say(a, "roll out", at, o.map(TargetHint::Construct));
    }
    if c.at(5, 3.4) {
        let o = c.sc.bs.ids[0].filter(|o| c.w.constructs.get(*o).is_some_and(|k| k.gone.is_none())).or_else(|| c.largest_of(a));
        let at = o.and_then(|o| c.w.constructs.get(o)).map_or(st.pos + st.fwd * 5.0, |k| k.center());
        c.say(a, "combine", at, o.map(TargetHint::Construct));
    }
    if c.t > 3.5 {
        // Follow the combined giant.
        let big = c.largest_of(a);
        if big.is_some() && big != c.sc.bs.ids[1] {
            c.sc.bs.ids[1] = big;
            c.shot(c.on_construct(big, a).dist(24.0).elev(6.0).smooth());
        }
    }
    if c.at(6, 7.0) {
        c.free(b);
    }
}

fn beat_train(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let Some((st, wall)) = c.sc.stages.tower.or_else(|| Some((c.stage(3), 16.0))) else { return };
    let side = st.side();
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.place(b, st.pos + st.fwd * (wall - 1.8), st.pos, 170.0);
        c.shot(c.on_player(b).with(pv(a)).side(bvec(side)).dist(16.0).elev(12.0));
    }
    if c.at(1, 0.4) {
        let tr = c.cast_at(a, "steam powered train", b);
        c.sc.bs.ids[0] = tr;
        c.shot(c.on_construct(tr, a).with(pv(b)).side(bvec(side)).elev(10.0).smooth());
    }
    if !c.sc.bs.flag && c.launched(b) {
        c.sc.bs.flag = true;
        c.free(b);
        c.shot(c.on_player(b).with(Focus::Point(bvec(st.pos + st.fwd * wall + SV::Y * 3.0))).side(bvec(side)).smooth());
    }
}

fn beat_eeyore(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let st = c.stage(4);
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.place(b, st.pos + st.fwd * 16.0, st.pos, 80.0);
        c.shot(c.on_player(a).with(pv(b)));
    }
    if c.at(1, 0.3) {
        let e = c.say(a, "giant eeyore shootin laser beams", st.pos + st.fwd * 6.0, Some(TargetHint::Player(b)));
        c.shot(c.on_construct(e, a).with(pv(b)).smooth());
    }
    if c.at(2, 1.5) {
        c.free(b);
    }
}

fn beat_bees(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let st = c.stage(4);
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.place(b, st.pos + st.fwd * 10.0, st.pos, 60.0);
        c.shot(c.on_player(b).with(pv(a)).dist(10.0));
    }
    if c.at(1, 0.3) {
        c.cast_at(a, "very angry bees", b);
    }
    if c.at(2, 3.0) {
        c.free(b);
    }
}

fn beat_black_hole(c: &mut Ctx) {
    let (a, b, d) = (c.role(0), c.role(1), c.role(2));
    let st = c.stage(5);
    let spot = st.pos + st.fwd * 4.0;
    if c.at(0, 0.0) {
        c.place(a, st.pos - st.fwd * 2.0, spot, 0.0);
        c.place(b, spot + st.side() * 4.0 + st.fwd * 2.0, spot, 70.0);
        c.place(d, spot - st.side() * 4.0 + st.fwd * 3.0, spot, 90.0);
        c.shot(c.on_player(a).with(Focus::Point(bvec(spot))).dist(12.0));
    }
    if c.at(1, 0.3) {
        let near = st.pos + st.fwd * 2.5;
        let at = ground(c.w, near + SV::Y).unwrap_or(near);
        let h = c.say(a, "black hole", at, None);
        c.sc.bs.ids[0] = h;
        c.shot(c.on_construct(h, a).with(pv(b)).dist(14.0).elev(24.0).smooth());
    }
    if c.at(2, 1.0) {
        c.free(b);
        c.free(d);
    }
}

fn beat_portal(c: &mut Ctx) {
    let a = c.role(0);
    let st = c.stage(0);
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.shot(c.on_player(a).dist(9.0).side(bvec(st.side())));
    }
    if c.at(1, 0.3) {
        let far = ground(c.w, st.pos + st.fwd * 18.0 + SV::Y * 2.0).unwrap_or(st.pos + st.fwd * 18.0);
        c.say(a, "portal", far, None);
    }
    if c.at(2, 1.2) {
        c.walk(a, 1);
        c.hold(a, buttons::JUMP);
    }
    if c.at(3, 1.6) {
        c.hold(a, 0);
    }
    if c.at(4, 3.2) {
        c.free(a);
    }
}

fn beat_cage(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let (st, gap) = c.sc.stages.edge.unwrap_or((c.stage(7), 12.0));
    if c.at(0, 0.0) {
        let edge = st.pos + st.fwd * (gap - 2.5);
        c.place(b, edge, st.pos, 50.0);
        c.place(a, edge - st.fwd * 9.0, edge, 0.0);
        c.shot(c.on_player(b).with(pv(a)).side(bvec(st.side())));
    }
    if c.at(1, 0.3) {
        let k = c.cast_at(a, "flying cage", b);
        c.sc.bs.ids[0] = k;
        c.shot(c.on_construct(k, a).with(pv(b)).side(bvec(st.side())).smooth());
    }
}

fn beat_gomu(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let st = c.stage(1);
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.place(b, st.pos + st.fwd * 12.0, st.pos, 110.0);
        c.shot(c.on_player(a).with(pv(b)).side(bvec(st.side())));
    }
    if c.at(1, 0.2) {
        c.say(a, "gum gum fruit", st.pos + st.fwd * 2.0, None);
    }
    if c.at(2, 0.5) {
        c.walk(a, 1);
    }
    if c.at(3, 1.1) {
        c.walk(a, 0);
        c.eat_or_grant(a, "gomu_gomu");
        c.look(a, Look::Player(b));
    }
    if c.at(4, 1.8) {
        c.cast_at(a, "gomu gomu no anvil", b);
    }
    if c.at(5, 3.4) {
        c.cast_at(a, "gomu gomu no bowling ball", b);
    }
}

fn beat_stand(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let st = c.stage(4);
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.place(b, st.pos + st.fwd * 4.0, st.pos, 100.0);
        c.shot(c.on_player(a).with(pv(b)).side(bvec(st.side())).dist(9.0));
    }
    if c.at(1, 0.2) {
        c.cast_at(a, "star platinum", b);
    }
    if c.t > 1.2 && c.t < 3.6 {
        c.sc.bs.n += 1;
        if c.sc.bs.n % 18 == 0 {
            let at = c.pos(b) + SV::Y;
            c.say(a, "ORA ORA ORA ORA", at, None);
            if c.sc.bs.n == 18 {
                c.caption(a, "ORA ORA ORA ORA ORA ORA ORA ORA");
            } else if let Some(cap) = c.sc.caption.as_mut() {
                cap.0 = "ORA ORA ORA ORA ORA ORA ORA ORA".into();
            }
        }
    }
    if c.at(2, 3.7) {
        c.free(b);
    }
}

fn beat_tmnt(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let st = c.stage(1);
    let side = st.side();
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.place(b, st.pos + st.fwd * 14.0, st.pos, 70.0);
        c.shot(c.on_player(a).with(Focus::Point(bvec(st.pos + st.fwd * 4.0))).dist(12.0).elev(14.0));
    }
    let names = ["leonardo", "raphael", "donatello", "michelangelo"];
    for (k, n) in names.iter().enumerate() {
        if c.at(1 + k as u32, 0.2 + k as f32 * 0.12) {
            // A line abreast in front of the caster, each aimed steeply at its own spot.
            let off = (k as f32 - 1.5) * 2.4;
            let at = ground(c.w, st.pos + st.fwd * 3.0 + side * off + SV::Y).unwrap_or(st.pos + st.fwd * 3.0);
            let id = c.say(a, n, at, Some(TargetHint::Player(b)));
            if k == 0 {
                c.sc.bs.ids[0] = id;
            }
        }
    }
    if c.at(5, 0.8) {
        c.caption(a, "leonardo  raphael  donatello  michelangelo");
    }
    if c.at(6, 2.2) {
        c.shot(c.on_player(b).with(pv(a)).dist(13.0).smooth());
        c.free(b);
    }
}

fn beat_heat_vision(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let st = c.stage(1);
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.place(b, st.pos + st.fwd * 8.0, st.pos, 90.0);
        c.shot(c.on_player(a).with(pv(b)).side(bvec(st.side())).dist(10.0).elev(12.0));
    }
    if c.at(1, 0.1) {
        c.eat_or_grant(a, "kryptonian");
        c.look(a, Look::Player(b));
    }
    if c.at(2, 0.6) {
        let at = c.pos(b) + SV::Y * 0.8;
        c.say(a, "heat vision", at, None);
    }
    if c.at(3, 1.2) {
        c.free(b);
    }
    if c.at(4, 2.8) {
        let at = c.pos(b) + SV::Y * 0.8;
        c.say(a, "heat vision", at, None);
    }
}

fn beat_tether(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let (st, gap) = c.sc.stages.edge.unwrap_or((c.stage(11), 12.0));
    let edge = st.pos + st.fwd * (gap - 2.5);
    if c.at(0, 0.0) {
        c.place(b, edge, st.pos, 85.0);
        c.place(a, edge - st.fwd * 3.0, edge + st.fwd * 5.0, 0.0);
        c.shot(c.on_player(b).with(pv(a)).side(bvec(st.side())).dist(13.0));
    }
    if c.at(1, 0.2) {
        c.say(a, "giant hammer", edge + SV::Y, Some(TargetHint::Player(b)));
    }
    if c.at(2, 0.9) {
        c.look(a, Look::Player(b));
        c.press(a, buttons::PRIMARY, 3);
    }
    if c.at(3, 1.9) {
        c.press(a, buttons::PRIMARY, 3);
    }
    // Recovery: once launched, the victim fires a tether back at the island and reels in.
    if !c.sc.bs.flag && c.launched(b) {
        c.sc.bs.flag = true;
        c.sc.bs.n = 0;
    }
    if c.sc.bs.flag {
        c.sc.bs.n += 1;
        let n = c.sc.bs.n;
        let anchor = st.pos + st.fwd * (gap - 4.0) + SV::Y * 0.2;
        if n == 30 {
            c.look(b, Look::Point(anchor));
        }
        if n == 40 {
            c.hold(b, buttons::TETHER_L | buttons::REEL | buttons::BOOST);
        }
        if n == 40 + 150 {
            c.hold(b, buttons::JUMP);
            c.walk(b, 1);
        }
        if n == 40 + 200 {
            c.free(b);
        }
    }
}

fn beat_hulk(c: &mut Ctx) {
    let (a, b) = (c.role(0), c.role(1));
    let st = c.stage(0);
    if c.at(0, 0.0) {
        c.place(a, st.pos, st.pos + st.fwd * 10.0, 0.0);
        c.place(b, st.pos + st.fwd * 7.5, st.pos, 260.0);
        c.shot(c.on_player(a).with(pv(b)).side(bvec(st.side())).dist(15.0).elev(20.0));
        c.eat_or_grant(a, "gamma");
    }
    if c.at(1, 0.4) {
        let at = c.pos(a) + st.fwd * 4.0 + SV::Y;
        c.say(a, "hulk out", at, None);
    }
    if c.at(2, 2.4) {
        c.look(a, Look::Player(b));
    }
    if c.at(3, 2.7) {
        let at = c.pos(b) + SV::Y;
        c.say(a, "hulk smash", at, None);
    }
    if !c.sc.bs.flag && c.launched(b) {
        c.sc.bs.flag = true;
        c.free(b);
        c.shot(c.on_player(b).with(pv(a)).smooth());
    }
}

// ------------------------------------------------------------------------------------ caption

fn draw_caption(time: Res<Time<Real>>, mut sc: ResMut<Showcase>, mut q: Query<(&mut Text, &mut TextColor, &mut Visibility), With<Caption>>) {
    let Ok((mut text, mut color, mut vis)) = q.single_mut() else { return };
    let dt = time.delta_secs();
    let Some((s, col, age)) = sc.caption.as_mut() else {
        *vis = Visibility::Hidden;
        return;
    };
    *age += dt;
    const HOLD: f32 = 2.2;
    let a = if *age < HOLD { 1.0 } else { (1.0 - (*age - HOLD) / 0.4).max(0.0) };
    if a <= 0.0 {
        sc.caption = None;
        *vis = Visibility::Hidden;
        return;
    }
    // Typed out quickly, like the ring console.
    let shown = ((*age / 0.02) as usize).min(s.len());
    let want = format!("> {}", &s[..shown]);
    if text.0 != want {
        text.0 = want;
    }
    let c = col.to_srgba();
    color.0 = Color::srgba(0.6 + c.red * 0.4, 0.6 + c.green * 0.4, 0.6 + c.blue * 0.4, a);
    *vis = Visibility::Visible;
}
