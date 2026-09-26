//! Party effect **hooks** (COMBAT.md §5g): what effects do beyond their standing graft.
//!
//! Entry points (each a one-line, commented hook in the world):
//! * [`modify_cast`] — inside `World::cast` after validation: Wild Magic substitution, every owned
//!   effect's standing graft, Kaiju Heart. Party grafts are free: the cast is priced as typed.
//! * [`after_cast`] — after a successful fresh cast: Echo (a free copy 1 s later), Mini-Me.
//! * [`pre_reap`] — before shattered constructs are removed: Main Character split, Party Popper,
//!   Portal Pals, Black Hole Sun.
//! * [`end_of_tick`] — just before the reap each tick: reacts to the events so far (hits → Piñata / Copycat,
//!   KOs → Kaiju Heart / Last Words, projectile impacts → Popcorn), runs scheduled actions, and
//!   the continuous effects (Butterfingers, Hiccups, Sidekick, Loud, Growth Spurt, Magnetic
//!   Personality, Reverse Card, Mini-Me following).
//! * [`tweak_params`] — per-player movement params (Slippery); host and client prediction.
//!
//! Deterministic (party RNG, slice order) and allocation-free per tick in steady state: scratch
//! vectors keep their capacity; only casts / spawns (rare events) allocate.

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, ModelSource, Recipe, Temperament};
use bd_core::{ConstructId, PlayerId, Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::effects::{Hook, hook_bits as hb};
use super::graft::apply_graft;
use super::{MAX_OWNED, PartyState, Scheduled, ticks};
use crate::combat::Explosion;
use crate::constructs::{SpawnParams, forming_secs, lifetime_secs};
use crate::events::{GameEvent, GoneReason};
use crate::movement::MoveParams;
use crate::projectile::ProjectileSpawn;
use crate::rng::SimRng;
use crate::space::PortalEnd;
use crate::willpower::{base_cost, integrity_for_cost, validate_recipe};
use crate::world::{Aim, World};

/// What a [`GameEvent::PartyFx`] shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PartyFx {
    Confetti = 0,
    PortalPals = 1,
    BlackHole = 2,
    /// Main Character: a construct split instead of shattering.
    Split = 3,
    Echo = 4,
    MiniMe = 5,
    /// Growth Spurt step.
    Grow = 6,
    /// Kaiju Heart: creatures grew after a KO.
    KaijuGrow = 7,
    /// Wild Magic replaced a cast (the new recipe arrives with `ConstructSpawned`).
    WildMagic = 8,
    /// Butterfingers: the held item slipped.
    Slip = 9,
    Pinata = 10,
    Honk = 11,
    Copycat = 12,
    Sidekick = 13,
    /// Last Words: a bomb is ticking where the player fell (goes off 1 s later).
    LastWords = 14,
    Popcorn = 15,
    Hiccup = 16,
}

// Tunables of the hooks (behaviour thresholds, not balance numbers).
const SLIPPERY_FRICTION: f32 = 0.1;
const MINI_SCALE: f32 = 0.4;
const KAIJU_STEP: f32 = 1.35;
const LAST_WORDS_FUSE: f32 = 1.0;
const COPYCAT_COOLDOWN: f32 = 6.0;
const PINATA_COOLDOWN: f32 = 0.75;
const POPCORN_COOLDOWN: f32 = 0.12;
const MAGNET_RADIUS: f32 = 12.0;
const MAGNET_ACCEL: f32 = 14.0;
const REVERSE_TURN: f32 = 5.0;
const HONK_EVERY: f32 = 2.0;
const GROWTH_STEPS: u32 = 4;

/// Slippery: ice under the player's feet. Host movement and client prediction both call this.
pub fn tweak_params(party: Option<&PartyState>, id: PlayerId, params: &mut MoveParams) {
    let Some(p) = party else { return };
    if p.any_hooks & hb::SLIPPERY != 0 && p.hooks_of(id) & hb::SLIPPERY != 0 {
        params.friction *= SLIPPERY_FRICTION;
        params.stop_speed *= 0.3;
        params.ground_accel *= 0.5;
    }
}

/// The candy a Piñata spills (small, harmless-looking, owned by whoever hit the piñata).
pub(crate) fn pinata_pool() -> Vec<Recipe> {
    use Mechanic::*;
    let mk = |name: &str, form: Form, m: &[Mechanic], e: Element, scale: f32, count: u8, t: Temperament| {
        let mut r = crate::bots::recipe(name, form, m, e, scale);
        r.count = count;
        r.temperament = t;
        r.intensity = if t == Temperament::Calm { 0.0 } else { 0.8 };
        r
    };
    vec![
        mk("candy", Form::Ball, &[Bounce], Element::Light, 0.4, 1, Temperament::Calm),
        mk("gumdrop bomb", Form::Bomb, &[Explodes], Element::Fire, 0.4, 1, Temperament::Calm),
        mk("sugar bees", Form::Insect, &[Swarm, Flies], Element::Light, 0.3, 3, Temperament::Aggressive),
        mk("jelly wolf", Form::Quadruped, &[Walks], Element::Light, 0.6, 1, Temperament::Playful),
        mk("toffee boulder", Form::Boulder, &[Heavy], Element::Earth, 0.7, 1, Temperament::Calm),
        mk("sherbet sprite", Form::Blob, &[Flies], Element::Ice, 0.5, 1, Temperament::Playful),
    ]
}

fn fx(w: &mut World, player: PlayerId, fx: PartyFx, pos: Vec3, construct: Option<ConstructId>) {
    w.events.push(GameEvent::PartyFx { player, fx, pos, construct });
}

fn hook_param<T>(party: &PartyState, id: PlayerId, f: impl Fn(&Hook) -> Option<T>) -> Option<T> {
    party.effects_of(id).find_map(|s| s.hook(&f))
}

// ================================================================================ casting

/// Party cast modifier (called from `World::cast` after validation; no-op in FFA / without
/// effects). Re-validates `r` and prices the cast as typed (party grafts never cost more).
pub fn modify_cast(w: &mut World, i: usize, r: &mut Recipe) {
    let Some(party) = w.party.as_ref() else { return };
    let Some(id) = w.players.get(i).map(|p| p.id) else { return };
    let Some(pp) = party.player(id) else { return };
    if pp.n_effects == 0 {
        return;
    }
    let (owned, n, hooks) = (pp.effects, pp.n_effects as usize, pp.hooks);
    let table = party.table.clone();
    let typed = base_cost(r);
    // Wild Magic: the ring misfires into a random phrase.
    if hooks & hb::WILD_MAGIC != 0 {
        let chance = hook_param(party, id, |h| if let Hook::WildMagic { chance } = h { Some(*chance) } else { None }).unwrap_or(0.25);
        if let Some(party) = w.party.as_mut()
            && party.rng.chance(chance)
        {
            let seed = party.rng.next_u64();
            let wild = party.phrase_source.as_ref().and_then(|s| s.random_recipe(seed)).unwrap_or_else(|| crate::fuzz::random_sane_recipe(&mut SimRng::new(seed)));
            *r = validate_recipe(wild, &w.cfg.rules);
            let pos = w.players[i].mv.pos;
            fx(w, id, PartyFx::WildMagic, pos, None);
        }
    }
    for e in &owned[..n.min(MAX_OWNED)] {
        if let Some(s) = table.get(*e) {
            apply_graft(r, &s.graft);
        }
    }
    // Kaiju Heart: a size class per KO this round; three KOs make a kaiju.
    if hooks & hb::KAIJU_HEART != 0 && r.form.category() == FormCategory::Creature {
        let k = w.players[i].kos.min(4) as i32;
        if k > 0 {
            r.scale *= KAIJU_STEP.powi(k);
            if k >= 3 && r.form != Form::Kaiju {
                r.form = Form::Kaiju;
                r.scale = r.scale.max(10.0);
                if !r.mechanics.contains(&Mechanic::Stomp) {
                    r.mechanics.push(Mechanic::Stomp);
                }
            }
        }
    }
    let cur = std::mem::replace(r, Recipe::unknown(""));
    *r = validate_recipe(cur, &w.cfg.rules);
    let after = base_cost(r);
    if after > 0.0 && typed.is_finite() && (after - typed).abs() > 1e-6 {
        w.party_cost_ratio = Some((typed / after).clamp(0.05, 20.0));
    }
}

/// After a successful cast (`ev0` = event index before it): Echo and Mini-Me for fresh casts.
pub fn after_cast(w: &mut World, player: PlayerId, id: ConstructId, aim: Aim, ev0: usize) {
    let Some(party) = w.party.as_ref() else { return };
    let hooks = party.hooks_of(player);
    if hooks & (hb::ECHO | hb::MINI_ME) == 0 {
        return;
    }
    let fresh = w.events.get(ev0..).is_some_and(|evs| evs.iter().any(|e| matches!(e, GameEvent::ConstructSpawned { id: x, .. } if *x == id)));
    let Some(c) = w.constructs.get(id).filter(|c| fresh && !c.attached() && c.pilot.is_none()) else { return };
    let recipe = c.recipe.clone();
    let category = c.category;
    let pos = c.center();
    // Copies keep an icon's authored shapes (vehicle + robot).
    let shapes: super::Shapes = (c.shape.as_deref().cloned(), c.alt_shape.as_deref().cloned());
    if hooks & hb::ECHO != 0 {
        let delay = hook_param(party, player, |h| if let Hook::Echo { delay } = h { Some(*delay) } else { None }).unwrap_or(1.0);
        let due = w.tick + ticks(delay).max(1);
        if let Some(party) = w.party.as_mut() {
            party.scheduled.push(Scheduled::Cast { due, player, recipe: Box::new(recipe.clone()), aim, parent: Some(id), shapes: Box::new(shapes.clone()) });
        }
    }
    if hooks & hb::MINI_ME != 0 {
        let mut mini = recipe;
        mini.name = format!("mini {}", mini.name);
        mini.scale *= MINI_SCALE;
        if category != FormCategory::Creature && !mini.mechanics.contains(&Mechanic::Flies) {
            mini.mechanics.push(Mechanic::Flies);
        }
        if let Ok(m) = w.spawn_free(player, mini, aim, Some(id), shapes.0, shapes.1) {
            if let Some(party) = w.party.as_mut() {
                party.companions.push((m, id));
            }
            fx(w, player, PartyFx::MiniMe, pos, Some(m));
        }
    }
}

/// Spawn a construct directly at `pos` for `owner` (free, a child of `parent`). Held / worn forms
/// are refused (they would jump into the owner's hand).
pub(crate) fn spawn_direct(w: &mut World, owner: PlayerId, recipe: Recipe, pos: Vec3, vel: Vec3, parent: Option<ConstructId>, life: Option<f32>) -> Option<ConstructId> {
    if w.constructs.len() + 8 >= crate::world::MAX_CONSTRUCTS || !pos.is_finite() || !vel.is_finite() {
        return None;
    }
    let target = w.player(owner)?.last_target;
    let r = validate_recipe(recipe, &w.cfg.rules);
    if matches!(r.form.category(), FormCategory::Held | FormCategory::Worn) {
        return None;
    }
    let cost = base_cost(&r);
    let forming = forming_secs(r.form).min(0.3);
    let life = life.filter(|l| l.is_finite() && *l > 0.0).unwrap_or_else(|| lifetime_secs(r.form));
    let rot = if vel.length_squared() > 1e-4 { crate::geom::look_rotation(vel.normalize()) } else { Quat::IDENTITY };
    let id = w.constructs.spawn(SpawnParams {
        owner,
        recipe: r.clone(),
        shape: None,
        pos,
        rot,
        vel,
        target,
        cost: 0.0,
        integrity: integrity_for_cost(cost.max(6.0)),
        forming,
        life,
        tick: w.tick,
        colossal: false,
        model: None,
    });
    if let Some(c) = w.constructs.get_mut(id) {
        // No parent ⇒ its own parent: party spawns never count toward slot limits.
        c.parent = Some(parent.unwrap_or(id));
    }
    w.events.push(GameEvent::ConstructSpawned { id, owner, recipe: Box::new(r), pos, rot, forming, cost: 0.0 });
    Some(id)
}

// ================================================================================ shatter

/// Before shattered constructs are removed (called at the top of `reap_constructs`).
pub fn pre_reap(w: &mut World) {
    let Some(mut party) = w.party.take() else { return };
    if party.any_hooks & hb::ON_SHATTER == 0 {
        w.party = Some(party);
        return;
    }
    let mut scratch = std::mem::take(&mut party.shatter_scratch);
    scratch.clear();
    for c in w.constructs.iter() {
        if c.gone.is_some() && c.shatter.is_some() {
            let h = party.hooks_of(c.owner) & hb::ON_SHATTER;
            if h != 0 {
                scratch.push((c.id, h));
            }
        }
    }
    let now = w.tick;
    for &(cid, h) in &scratch {
        let Some(c) = w.constructs.get(cid) else { continue };
        let (owner, pos, vel, scale, attached, piloted) = (c.owner, c.center(), c.vel, c.half.max_element(), c.attached(), c.pilot.is_some());
        if h & hb::MAIN_CHARACTER != 0 && !party.staged.contains(&cid) && !attached && !piloted {
            // Stage one: survive, smaller, and split off a second half.
            party.staged.push(cid);
            let (recipe, remaining, max_i, rot) = {
                let Some(c) = w.constructs.get_mut(cid) else { continue };
                c.gone = None;
                c.shatter = None;
                c.integrity = c.max_integrity * 0.4;
                c.resize(0.8);
                (c.recipe.clone(), (c.life - c.age).max(1.0), c.max_integrity, c.rot)
            };
            let side = (rot * Vec3::X).normalize_or_zero();
            if let Some(half) = spawn_direct(w, owner, recipe, pos + side * (scale + 0.3), vel + side * 4.0 + Vec3::Y * 2.0, Some(cid), Some(remaining)) {
                party.staged.push(half);
                if let Some(h2) = w.constructs.get_mut(half) {
                    h2.resize(0.6);
                    h2.integrity = max_i * 0.3;
                    h2.max_integrity = max_i * 0.3;
                }
            }
            fx(w, owner, PartyFx::Split, pos, Some(cid));
            continue;
        }
        if h & hb::CONFETTI != 0 {
            let e = Explosion {
                pos,
                radius: (3.0 + scale).min(6.0),
                damage: 0.0,
                knockback: 14.0,
                source: Some(owner),
                construct: None,
                element: Element::Light,
                carve_radius: 0.0,
                self_damage: false,
            };
            party.scheduled.push(Scheduled::Explosion { due: now, e });
            fx(w, owner, PartyFx::Confetti, pos, Some(cid));
        }
        if h & hb::PORTAL_PAIR != 0 {
            party.scheduled.push(Scheduled::Portal { due: now, owner, pos });
        }
        if h & hb::BLACK_HOLE != 0 {
            party.scheduled.push(Scheduled::Field { due: now, owner, pos, radius: 9.0, strength: 26.0, secs: 1.5 });
            let e = Explosion { pos, radius: 4.0, damage: 12.0, knockback: 16.0, source: Some(owner), construct: None, element: Element::Shadow, carve_radius: 1.5, self_damage: false };
            party.scheduled.push(Scheduled::Explosion { due: now + ticks(1.5), e });
            fx(w, owner, PartyFx::BlackHole, pos, Some(cid));
        }
    }
    scratch.clear();
    party.shatter_scratch = scratch;
    w.party = Some(party);
}

// ================================================================================ end of tick

/// Every tick just before the reap (party only).
pub fn end_of_tick(w: &mut World, dt: f32) {
    let Some(mut party) = w.party.take() else { return };
    if party.any_hooks != 0 || !party.scheduled.is_empty() {
        scan_events(w, &mut party);
        run_scheduled(w, &mut party);
        per_player(w, &mut party, dt);
        follow_companions(w, &mut party, dt);
    }
    if !party.staged.is_empty() {
        let store = &w.constructs;
        party.staged.retain(|id| store.get(*id).is_some());
    }
    w.party = Some(party);
}

enum Seen {
    Hit { target: PlayerId, source: Option<PlayerId>, construct: Option<ConstructId>, damage: f32, point: Vec3 },
    Ko { victim: PlayerId, killer: Option<PlayerId>, pos: Vec3 },
    Fired { id: u16, owner: PlayerId },
    Ended { id: u16, pos: Vec3, normal: Vec3 },
}

fn scan_events(w: &mut World, party: &mut PartyState) {
    let want_hits = party.any_hooks & (hb::PINATA | hb::COPYCAT) != 0;
    let want_kos = party.any_hooks & (hb::KAIJU_HEART | hb::LAST_WORDS) != 0;
    let want_proj = party.any_hooks & hb::POPCORN != 0;
    if !(want_hits || want_kos || want_proj) {
        return;
    }
    let n = w.events.len();
    for k in 0..n {
        let seen = match &w.events[k] {
            GameEvent::Hit { target, source, construct, damage, point, .. } if want_hits => {
                Seen::Hit { target: *target, source: *source, construct: *construct, damage: *damage, point: *point }
            }
            GameEvent::Ko { victim, killer, pos, .. } if want_kos => Seen::Ko { victim: *victim, killer: *killer, pos: *pos },
            GameEvent::ProjectileFired { id, owner, .. } if want_proj => Seen::Fired { id: *id, owner: *owner },
            GameEvent::ProjectileEnded { id, pos, normal, hit_player: None } if want_proj => Seen::Ended { id: *id, pos: *pos, normal: *normal },
            _ => continue,
        };
        match seen {
            Seen::Hit { target, source, construct, damage, point } => on_hit(w, party, target, source, construct, damage, point),
            Seen::Ko { victim, killer, pos } => on_ko(w, party, victim, killer, pos),
            Seen::Fired { id, owner } => {
                party.proj_owner[party.proj_head] = (id, owner.0);
                party.proj_head = (party.proj_head + 1) % party.proj_owner.len();
            }
            Seen::Ended { id, pos, normal } => on_projectile_end(w, party, id, pos, normal),
        }
    }
}

fn on_hit(w: &mut World, party: &mut PartyState, target: PlayerId, source: Option<PlayerId>, construct: Option<ConstructId>, damage: f32, point: Vec3) {
    let Some(src) = source.filter(|s| *s != target) else { return };
    let hooks = party.hooks_of(target);
    if hooks & hb::PINATA != 0 && damage > 0.0 && party.player(target).is_some_and(|p| p.cd_pinata <= 0.0) {
        if let Some(p) = party.player_mut(target) {
            p.cd_pinata = PINATA_COOLDOWN;
        }
        let k = party.rng.below(party.pinata_pool.len() as u32) as usize;
        let ang = party.rng.range(0.0, std::f32::consts::TAU);
        let recipe = party.pinata_pool.get(k).cloned();
        let at = if point.is_finite() { point } else { w.player(target).map_or(Vec3::ZERO, |p| p.mv.pos) };
        if let Some(r) = recipe {
            let vel = Vec3::new(ang.cos() * 5.0, 7.0, ang.sin() * 5.0);
            let id = spawn_direct(w, src, r, at + Vec3::Y * 0.8, vel, None, Some(10.0));
            fx(w, target, PartyFx::Pinata, at, id);
        }
    }
    if hooks & hb::COPYCAT != 0 && party.player(target).is_some_and(|p| p.cd_copycat <= 0.0) {
        let Some(c) = construct.and_then(|c| w.constructs.get(c)).filter(|c| c.owner != target) else { return };
        let recipe = c.recipe.clone();
        let (shape, alt) = (c.shape.as_deref().cloned(), c.alt_shape.as_deref().cloned());
        let Some(me) = w.player(target).filter(|p| p.alive) else { return };
        let aim = Aim::new(me.eye(w.params()), me.look());
        let pos = me.mv.pos;
        if let Ok(id) = w.spawn_free(target, recipe, aim, None, shape, alt) {
            if let Some(p) = party.player_mut(target) {
                p.cd_copycat = COPYCAT_COOLDOWN;
            }
            fx(w, target, PartyFx::Copycat, pos, Some(id));
        }
    }
}

fn on_ko(w: &mut World, party: &mut PartyState, victim: PlayerId, killer: Option<PlayerId>, pos: Vec3) {
    if let Some(k) = killer.filter(|k| *k != victim && party.hooks_of(*k) & hb::KAIJU_HEART != 0) {
        let mut grew = None;
        for c in w.constructs.iter_mut() {
            if c.owner == k && c.gone.is_none() && c.category == FormCategory::Creature && c.size_scale < 2.5 {
                c.resize(KAIJU_STEP);
                grew = Some((c.id, c.center()));
            }
        }
        if let Some((id, at)) = grew {
            fx(w, k, PartyFx::KaijuGrow, at, Some(id));
        }
    }
    // Smash model: every KO happens in a blast zone, so the bomb goes where they last stood.
    let pos = if pos.is_finite() && w.in_play(pos, 0.0) { pos } else { w.player(victim).map_or(pos, |p| p.last_safe_pos) };
    if party.hooks_of(victim) & hb::LAST_WORDS != 0 && pos.is_finite() && w.in_play(pos, 0.0) {
        let e = Explosion { pos, radius: 4.0, damage: 20.0, knockback: 18.0, source: Some(victim), construct: None, element: Element::Fire, carve_radius: 1.5, self_damage: false };
        party.scheduled.push(Scheduled::Explosion { due: w.tick + ticks(LAST_WORDS_FUSE), e });
        fx(w, victim, PartyFx::LastWords, pos, None);
    }
}

fn on_projectile_end(w: &mut World, party: &mut PartyState, id: u16, pos: Vec3, normal: Vec3) {
    if normal == Vec3::ZERO || !pos.is_finite() {
        return; // timed out in the air
    }
    let Some(&(_, owner)) = party.proj_owner.iter().find(|(pid, o)| *pid == id && *o != u8::MAX) else { return };
    let owner = PlayerId(owner);
    if w.player(owner).is_none() || party.hooks_of(owner) & hb::POPCORN == 0 || party.player(owner).is_none_or(|p| p.cd_popcorn > 0.0) {
        return;
    }
    if let Some(p) = party.player_mut(owner) {
        p.cd_popcorn = POPCORN_COOLDOWN;
    }
    // Forget the id (ids wrap).
    for e in party.proj_owner.iter_mut() {
        if e.0 == id {
            *e = (0, u8::MAX);
        }
    }
    let n = normal.normalize_or_zero();
    for _ in 0..3 {
        let jitter = Vec3::new(party.rng.range(-0.7, 0.7), party.rng.range(0.2, 0.9), party.rng.range(-0.7, 0.7));
        let dir = (n + jitter).normalize_or_zero();
        let mut s = ProjectileSpawn::ring_blast(owner, pos + n * 0.3, dir);
        s.vel = dir * 22.0;
        s.damage = 4.0;
        s.knockback = 5.0;
        s.radius = 0.12;
        s.life = 0.7;
        w.spawn_projectile(s, None);
    }
    fx(w, owner, PartyFx::Popcorn, pos, None);
}

fn run_scheduled(w: &mut World, party: &mut PartyState) {
    let now = w.tick;
    let mut k = 0;
    while k < party.scheduled.len() {
        if party.scheduled[k].due() > now {
            k += 1;
            continue;
        }
        match party.scheduled.remove(k) {
            Scheduled::Cast { player, recipe, aim, parent, shapes, .. } => {
                let (shape, alt) = *shapes;
                if let Ok(id) = w.spawn_free(player, *recipe, aim, parent, shape, alt) {
                    let at = w.constructs.get(id).map_or(Vec3::ZERO, |c| c.center());
                    fx(w, player, PartyFx::Echo, at, Some(id));
                }
            }
            Scheduled::Explosion { mut e, .. } => {
                // The source may have left since (never credit a missing player).
                if e.source.is_some_and(|s| w.player(s).is_none()) {
                    e.source = None;
                }
                w.explode(e)
            }
            Scheduled::Portal { owner, pos, .. } => {
                let Some(p) = w.player(owner).filter(|p| p.alive) else { continue };
                let feet = p.mv.pos;
                let look = p.look();
                let fwd = Vec3::new(look.x, 0.0, look.z).normalize_or_zero();
                let fwd = if fwd == Vec3::ZERO { Vec3::NEG_Z } else { fwd };
                let to_owner = Vec3::new(feet.x - pos.x, 0.0, feet.z - pos.z).normalize_or_zero();
                let to_owner = if to_owner == Vec3::ZERO { Vec3::X } else { to_owner };
                let a = PortalEnd::new(pos + Vec3::Y * 0.6, to_owner, Vec3::Y, crate::space::PORTAL_RADIUS);
                let b = PortalEnd::new(feet + fwd * 3.0 + Vec3::Y * 1.2, -fwd, Vec3::Y, crate::space::PORTAL_RADIUS);
                if w.create_portal(owner, a, b, 8.0, None).is_some() {
                    fx(w, owner, PartyFx::PortalPals, pos, None);
                }
            }
            Scheduled::Field { owner, pos, radius, strength, secs, .. } => {
                if w.player(owner).is_some() {
                    w.create_field(owner, pos, radius, strength, secs, None);
                }
            }
        }
    }
}

/// Held forms fly off as the closest thrown thing.
fn slipped_form(f: Form) -> Form {
    match f {
        Form::Hammer | Form::Gauntlet => Form::Anvil,
        Form::Cannon => Form::Boulder,
        Form::Gun | Form::Bow | Form::Staff => Form::Rocket,
        _ => Form::Ball,
    }
}

fn per_player(w: &mut World, party: &mut PartyState, dt: f32) {
    let base = w.cfg.move_params.clone();
    for k in 0..party.players.len() {
        let id = party.players[k].id;
        let hooks = party.players[k].hooks;
        {
            let p = &mut party.players[k];
            p.cd_copycat = (p.cd_copycat - dt).max(0.0);
            p.cd_pinata = (p.cd_pinata - dt).max(0.0);
            p.cd_popcorn = (p.cd_popcorn - dt).max(0.0);
        }
        if hooks == 0 {
            continue;
        }
        let Some(pi) = w.players.iter().position(|p| p.id == id) else { continue };
        let alive = w.players[pi].alive;
        let live = w.match_state.live();
        // ---- Butterfingers
        if hooks & hb::BUTTERFINGERS != 0 {
            let mean = hook_param(party, id, |h| if let Hook::Butterfingers { mean_secs } = h { Some(*mean_secs) } else { None }).unwrap_or(9.0);
            let t = party.players[k].t_butter;
            if t == 0.0 {
                party.players[k].t_butter = mean * party.rng.range(0.5, 1.5);
            } else if t - dt <= 0.0 {
                party.players[k].t_butter = mean * party.rng.range(0.5, 1.5);
                if alive && live {
                    slip(w, party, pi);
                }
            } else {
                party.players[k].t_butter = t - dt;
            }
        }
        // ---- Hiccups
        if hooks & hb::HICCUPS != 0 {
            let mean = hook_param(party, id, |h| if let Hook::Hiccups { mean_secs } = h { Some(*mean_secs) } else { None }).unwrap_or(7.0);
            let t = party.players[k].t_hiccup;
            if t == 0.0 {
                party.players[k].t_hiccup = mean * party.rng.range(0.5, 1.5);
            } else if t - dt <= 0.0 {
                party.players[k].t_hiccup = mean * party.rng.range(0.5, 1.5);
                let p = &mut w.players[pi];
                if alive && p.seat.is_none_or(|s| s.suit) {
                    p.mv.vel.y = p.mv.vel.y.max(0.0) + 5.5;
                    p.mv.grounded = false;
                    let at = p.mv.pos;
                    fx(w, id, PartyFx::Hiccup, at, None);
                }
            } else {
                party.players[k].t_hiccup = t - dt;
            }
        }
        // ---- Sidekick
        if hooks & hb::SIDEKICK != 0 && alive && live {
            party.players[k].t_sidekick -= dt;
            if party.players[k].t_sidekick <= 0.0 {
                party.players[k].t_sidekick = 1.0;
                let gone = party.players[k].sidekick.is_none_or(|s| w.constructs.get(s).is_none_or(|c| c.gone.is_some()));
                if gone {
                    let p = &w.players[pi];
                    let look = p.look();
                    let aim = Aim::new(p.eye(&base), (look + Vec3::Y * 0.6).normalize_or_zero());
                    let mut r = crate::bots::recipe("sidekick spirit", Form::Blob, &[Mechanic::Flies], Element::Light, 0.7);
                    r.temperament = Temperament::Protective;
                    r.intensity = 0.9;
                    if let Ok(s) = w.spawn_free(id, r, aim, None, None, None) {
                        party.players[k].sidekick = Some(s);
                        let at = w.constructs.get(s).map_or(Vec3::ZERO, |c| c.center());
                        fx(w, id, PartyFx::Sidekick, at, Some(s));
                    }
                }
            }
        }
        // ---- Loud
        if hooks & hb::LOUD != 0 {
            party.players[k].t_loud -= dt;
            if party.players[k].t_loud <= 0.0 {
                party.players[k].t_loud = HONK_EVERY;
                let newest = w.constructs.iter().filter(|c| c.owner == id && c.active() && !c.attached()).last().map(|c| (c.id, c.center()));
                if let Some((cid, at)) = newest {
                    fx(w, id, PartyFx::Honk, at, Some(cid));
                }
            }
        }
        // ---- Growth Spurt
        if hooks & hb::GROWTH_SPURT != 0 {
            let every = hook_param(party, id, |h| if let Hook::GrowthSpurt { every } = h { Some(*every) } else { None }).unwrap_or(2.0).max(0.1);
            let mut grew: Option<(ConstructId, Vec3)> = None;
            for c in w.constructs.iter_mut() {
                if c.owner != id || !c.active() || c.attached() {
                    continue;
                }
                let step = (c.age / every).floor() as u32;
                let prev = ((c.age - dt).max(0.0) / every).floor() as u32;
                if step > prev && step <= GROWTH_STEPS {
                    let f = (1.0 + 0.25 * step as f32) / (1.0 + 0.25 * (step - 1) as f32);
                    c.resize(f);
                    grew = Some((c.id, c.center()));
                }
            }
            if let Some((cid, at)) = grew {
                fx(w, id, PartyFx::Grow, at, Some(cid));
            }
        }
        // ---- Magnetic Personality
        if hooks & hb::MAGNETIC != 0 && alive {
            let me = w.players[pi].hull(&base).center();
            for c in w.constructs.iter_mut() {
                if c.owner == id || !c.active() || c.attached() {
                    continue;
                }
                let d = me - c.center();
                let dist = d.length();
                if dist > 0.5 && dist < MAGNET_RADIUS {
                    let resist = if c.has(Mechanic::Heavy) { crate::space::FIELD_RESIST } else { 1.0 };
                    c.vel += d / dist * (MAGNET_ACCEL * resist * dt);
                }
            }
        }
        // ---- Reverse Card
        if hooks & hb::REVERSE_CARD != 0 && alive {
            let after = hook_param(party, id, |h| if let Hook::ReverseCard { after } = h { Some(*after) } else { None }).unwrap_or(1.0);
            let me = w.players[pi].hull(&base).center();
            let tick = w.tick;
            for k2 in 0..w.projectiles.len() {
                let pr = &mut w.projectiles[k2];
                if pr.spec.owner != id || pr.dead || pr.age < after {
                    continue;
                }
                let speed = pr.vel.length();
                let want = (me - pr.pos).normalize_or_zero();
                if speed < 1e-3 || want == Vec3::ZERO {
                    continue;
                }
                let dir = (pr.vel / speed + want * (REVERSE_TURN * dt)).normalize_or_zero();
                pr.vel = dir * speed;
                if tick.saturating_sub(pr.segment_tick) >= 6 {
                    // New replicated ballistic segment (like a field-bent shot).
                    pr.origin = pr.pos;
                    pr.vel0 = pr.vel;
                    pr.segment_tick = tick;
                    let (pid, kind, origin, vel) = (pr.id, pr.spec.kind, pr.pos, pr.vel);
                    w.events.push(GameEvent::ProjectileFired { id: pid, owner: id, kind, origin, vel });
                }
            }
        }
    }
}

/// Butterfingers: the held item flies out of the hand as a thrown thing.
fn slip(w: &mut World, party: &mut PartyState, pi: usize) {
    let id = w.players[pi].id;
    let Some(c) = w.constructs.held_by(id).filter(|c| c.active() && !c.suit) else { return };
    let (cid, mut recipe, at) = (c.id, c.recipe.clone(), c.center());
    let look = w.players[pi].look();
    let pvel = w.players[pi].mv.vel;
    let side = Vec3::new(-look.z, 0.0, look.x).normalize_or_zero() * party.rng.range(-4.0, 4.0);
    let vel = pvel + look * 6.0 + side + Vec3::Y * 5.0;
    if let Some(c) = w.constructs.get_mut(cid) {
        c.gone = Some(GoneReason::Spent);
    }
    recipe.form = slipped_form(recipe.form);
    recipe.model = ModelSource::Procedural { seed: crate::rng::word_hash(&recipe.name) as u32 };
    let new = spawn_direct(w, id, recipe, at, vel, None, Some(6.0));
    fx(w, id, PartyFx::Slip, at, new);
}

fn follow_companions(w: &mut World, party: &mut PartyState, dt: f32) {
    if party.companions.is_empty() {
        return;
    }
    let store = &w.constructs;
    party.companions.retain(|(m, o)| store.get(*m).is_some_and(|c| c.gone.is_none()) && store.get(*o).is_some_and(|c| c.gone.is_none()));
    for &(m, o) in &party.companions {
        let Some((to, target)) = w.constructs.get(o).map(|c| (c.center() + Vec3::Y * 1.5, c.target)) else { continue };
        if let Some(c) = w.constructs.get_mut(m) {
            c.target = target;
            let d = to - c.center();
            let dist = d.length();
            if dist > 2.5 && dist.is_finite() {
                let want = d / dist * 10.0;
                c.vel += (want - c.vel) * (4.0 * dt).min(1.0);
            }
        }
    }
}
