//! Hits and KOs (DESIGN.md "Impact", "Hit & KO confirmation", "KO streak sounds").
//!
//! * `Hit` → five channels scaled by the sim's `impact` numbers: render-side hit-stop for attacker +
//!   victim visuals ([`HitFreeze`], 30–140 ms, +80 ms on KO blows), `Impact` juice (shake along the
//!   hit vector, FOV punch, victim flash) when the local player is involved, hit sounds layered by
//!   weight, crit snap + stronger crosshair flare, shard burst, onomatopoeia (bd_fx applies the
//!   settings), debris inheritance ([`RecentPushes`]).
//! * `Ko` → the victim shatters instantly into corps voxels (see `players`), their construct loops
//!   cut off, the killer's streak step plays in their corps timbre (+ void / double / shutdown
//!   accents); on a KO *you* dealt the rest of the mix ducks ~250 ms and the flare blooms corps.
//! * `WallSplat`, `Blocked`, `Explosion`, `Respawn`.

use super::players::ShatterPlayer;
use super::sound::{MixDuck, Sfx3d};
use super::terrain::RecentPushes;
use super::{bv, corps_rgb, rgb_color, PresentSet};
use crate::camera::PlayerCamera;
use crate::debris::DebrisImpulse;
use crate::juice::{CameraShakeEvent, FovPunch, HitStop, Impact};
use crate::sim_bridge::{ActiveWorld, GameEvents};
use crate::state::AppState;
use bd_core::corps::Corps;
use bd_core::mechanics::Element;
use bd_core::{ConstructId, PlayerId};
use bd_fx::{BurstFx, BurstKind, SpawnOnomatopoeia};
use bd_sim::events::KoCause;
use bd_sim::smash::BlastZone;
use bd_sim::GameEvent;
use bevy::picking::Pickable;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// CONNECT juice: tiered asymmetric hit-stop, victim shake / flash, contact drag, sparks, finisher
/// slow-mo, lunge feel (`assets/tuning/juice.ron`).
#[path = "connect.rs"]
pub mod connect;

/// Render-side hit-stop: visuals of these players / constructs hold still until the given real
/// time (the rest of the world keeps moving, so it reads as contact, not lag).
#[derive(Resource, Default)]
pub struct HitFreeze {
    pub players: HashMap<PlayerId, f64>,
    pub constructs: HashMap<ConstructId, f64>,
    pub now: f64,
    /// CONNECT: victim hit-shake during the freeze: (dir, amplitude m, Hz, start, until).
    pub shakes: HashMap<PlayerId, (Vec3, f32, f32, f64, f64)>,
    /// CONNECT: victim white flash: (peak, start, until).
    pub flashes: HashMap<PlayerId, (f32, f64, f64)>,
}

impl HitFreeze {
    pub fn player(&self, id: PlayerId) -> bool {
        self.players.get(&id).is_some_and(|t| *t > self.now)
    }
    pub fn construct(&self, id: ConstructId) -> bool {
        self.constructs.get(&id).is_some_and(|t| *t > self.now)
    }
    fn freeze_player(&mut self, id: PlayerId, secs: f32) {
        let until = self.now + secs as f64;
        let e = self.players.entry(id).or_insert(0.0);
        *e = e.max(until);
    }
    /// Hold a player's visuals for `secs` (longest request wins).
    pub fn hold_player(&mut self, id: PlayerId, secs: f32) {
        self.freeze_player(id, secs);
    }
    pub fn hold_construct(&mut self, id: ConstructId, secs: f32) {
        let until = self.now + secs as f64;
        let e = self.constructs.entry(id).or_insert(0.0);
        *e = e.max(until);
    }
    /// Shake a frozen victim along `dir` (a jitter that decays over `secs`).
    pub fn set_shake(&mut self, id: PlayerId, dir: Vec3, amp: f32, hz: f32, secs: f32) {
        if amp > 0.0 && secs > 0.0 {
            self.shakes.insert(id, (dir.normalize_or(Vec3::X), amp, hz, self.now, self.now + secs as f64));
        }
    }
    /// Flash a victim white, fading over `secs`.
    pub fn set_flash(&mut self, id: PlayerId, peak: f32, secs: f32) {
        if peak > 0.0 && secs > 0.0 {
            let old = self.flash_of(id);
            self.flashes.insert(id, (peak.max(old), self.now, self.now + secs as f64));
        }
    }
    /// This frame's hit-shake offset of a player's body (zero when none).
    pub fn shake_offset(&self, id: PlayerId) -> Vec3 {
        let Some(&(dir, amp, hz, t0, t1)) = self.shakes.get(&id) else { return Vec3::ZERO };
        if self.now >= t1 || t1 <= t0 {
            return Vec3::ZERO;
        }
        let u = ((self.now - t0) / (t1 - t0)) as f32;
        let ph = (self.now - t0) as f32 * hz * std::f32::consts::TAU;
        dir * (amp * (1.0 - u) * ph.sin())
    }
    /// This frame's white flash (0..1) of a player's body.
    pub fn flash_of(&self, id: PlayerId) -> f32 {
        let Some(&(peak, t0, t1)) = self.flashes.get(&id) else { return 0.0 };
        if self.now >= t1 || t1 <= t0 {
            return 0.0;
        }
        let u = ((self.now - t0) / (t1 - t0)) as f32;
        peak * (1.0 - u) * (1.0 - u)
    }
}

/// Crosshair flare state (the only 2D hit confirmation).
#[derive(Resource, Default)]
pub struct CrosshairFlare {
    pub intensity: f32,
    pub color: [f32; 3],
    /// Corps-emblem pulse on a KO you dealt (0..1).
    pub bloom: f32,
}

impl CrosshairFlare {
    fn hit(&mut self, strength: f32, color: [f32; 3]) {
        if strength >= self.intensity {
            self.color = color;
        }
        self.intensity = self.intensity.max(strength);
    }
}

#[derive(Component)]
struct FlareRing;
#[derive(Component)]
struct FlareBloom;

pub struct ImpactPlugin;

impl Plugin for ImpactPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(connect::ConnectPlugin)
            .init_resource::<HitFreeze>()
            .init_resource::<CrosshairFlare>()
            .add_systems(OnEnter(AppState::InGame), spawn_flare)
            .add_systems(OnExit(AppState::InGame), despawn_flare)
            .add_systems(Update, present_hits.in_set(PresentSet::Events))
            .add_systems(Update, (animate_flare, star_kos).in_set(PresentSet::Sync));
    }
}

fn element_word(e: Element) -> Option<&'static str> {
    Some(match e {
        Element::Fire => "fire",
        Element::Electric => "electric",
        Element::Ice => "ice",
        Element::Water => "water",
        _ => return None,
    })
}

fn event_hash(tick: u64, salt: u64) -> u64 {
    (tick.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)).rotate_left(17)
}

#[allow(clippy::too_many_arguments)]
fn present_hits(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    mut events: MessageReader<GameEvents>,
    mut freeze: ResMut<HitFreeze>,
    mut flare: ResMut<CrosshairFlare>,
    mut pushes: ResMut<RecentPushes>,
    mut duck: ResMut<MixDuck>,
    juice: (MessageWriter<Impact>, MessageWriter<HitStop>, MessageWriter<CameraShakeEvent>, MessageWriter<FovPunch>),
    fx_out: (MessageWriter<Sfx3d>, MessageWriter<BurstFx>, MessageWriter<SpawnOnomatopoeia>, MessageWriter<ShatterPlayer>, MessageWriter<DebrisImpulse>),
    mut debris: MessageWriter<crate::debris::SpawnDebris>,
    mut rng: ResMut<crate::debris::VfxRng>,
    mut commands: Commands,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    (ads, juice_t, presence): (Option<Res<super::ads::AdsState>>, Res<connect::JuiceTuningRes>, Option<Res<super::presence::PresenceIndex>>),
) {
    let (mut impact, mut hitstop, mut shake, mut fov) = juice;
    let _ = &mut hitstop;
    // Orbit presents top / bottom KOs itself (`present::space`: drift into the stars, re-entry).
    let orbit = arena.is_some_and(|a| a.info.kind == crate::arena::MapKind::Orbit);
    let (mut sfx, mut burst, mut ono, mut shatter, mut impulses) = fx_out;
    freeze.now = time.elapsed_secs_f64();
    let Some(active) = active else {
        events.clear();
        return;
    };
    let local = active.local;
    let eye = cam.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    // ADS: hit words slide off the sight line (`ads::off_sight_line`).
    let ads_blend = ads.map_or(0.0, |a| a.blend.clamp(0.0, 1.0));
    let fwd = cam.single().map(|t| t.forward().as_vec3()).unwrap_or(Vec3::NEG_Z);
    let corps_of = |id: Option<PlayerId>| id.map_or(Corps::Life, |i| active.corps_of(i));
    let mut hit_sounds = 0;
    for ev in events.read() {
        match &ev.event {
            GameEvent::Hit { target, source, construct, kind, element, damage, knockback, point, tumble, lethal, crit, impact: fx, .. } => {
                let p = bv(*point);
                let dir = bv(fx.dir).normalize_or_zero();
                // PRESENCE: a titan's hits land bigger than a sword's (sparks, words, victim shake).
                let strength = fx.strength.max(0.05) * presence.as_ref().map_or(1.0, |p| p.impact_mult(*construct));
                let col = corps_rgb(corps_of(*source));
                // Time (tiered, asymmetric hit-stop), victim shake / flash, the attacker's camera
                // punch + contact drag, sparks, sub, finisher: `connect` (juice.ron).
                let tier = connect::tier(&juice_t.0, *kind, fx.strength, fx.heavy, *knockback, *lethal);
                let word_k = juice_t.0.word_scale[tier as usize].max(0.1);
                pushes.add(p, dir * knockback.min(40.0), 1.5 + strength);
                let local_victim = local == Some(*target);
                let local_attacker = source.is_some() && local == *source;
                if local_victim {
                    impact.write(Impact { impact: strength, dir, local_victim, color: rgb_color(col) });
                }
                if local_attacker {
                    flare.hit(if *crit { 1.0 } else { 0.45 + 0.3 * strength.min(1.0) }, if *crit { [1.0, 0.95, 0.8] } else { [1.0; 3] });
                    sfx.write(Sfx3d::ui(if strength > 0.8 { "hit_confirm_heavy" } else { "hit_confirm" }).gain(0.8));
                }
                if *crit {
                    // Crisp high snap over the dull thud (cuts through any mix).
                    let s = if local_attacker || local_victim { Sfx3d::ui("hit_crit") } else { Sfx3d::at("hit_crit", p) };
                    sfx.write(s.exempt());
                }
                if hit_sounds < 6 {
                    hit_sounds += 1;
                    let name = if strength > 1.0 || fx.heavy { "hit_heavy" } else if strength > 0.45 { "hit_medium" } else { "hit_light" };
                    sfx.write(Sfx3d::at(name, p).gain(0.6 + 0.4 * strength.min(1.0)));
                }
                burst.write(BurstFx { kind: BurstKind::Shards, pos: p, dir, scale: 0.4 + strength * 0.8, color: Some(col) });
                if let Some(w) = element_word(*element) {
                    burst.write(BurstFx { kind: BurstKind::Element(*element), pos: p, dir, scale: 0.5 + strength * 0.5, color: None });
                    let (wp, ws) = super::ads::off_sight_line(p, strength, eye, fwd, ads_blend);
                    let mut o = SpawnOnomatopoeia::key(w, wp, ws * word_k, col);
                    o.dir = dir;
                    o.event_id = event_hash(ev.tick, target.0 as u64);
                    o.local_player = local_victim || local_attacker;
                    ono.write(o);
                } else if fx.heavy || strength > 0.6 || *damage > 18.0 {
                    let (wp, ws) = super::ads::off_sight_line(p, strength, eye, fwd, ads_blend);
                    let mut o = SpawnOnomatopoeia::key(if strength > 0.9 { "hit_heavy" } else { "hit_light" }, wp, ws * word_k, col);
                    o.dir = dir;
                    o.event_id = event_hash(ev.tick, target.0 as u64);
                    o.local_player = local_victim || local_attacker;
                    ono.write(o);
                }
                let _ = tumble;
            }
            GameEvent::Blocked { target, source, point } => {
                let p = bv(*point);
                sfx.write(Sfx3d::at("shield_block", p));
                burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::ZERO, scale: 0.6, color: Some(corps_rgb(corps_of(Some(*target)))) });
                if source.is_some() && local == *source {
                    flare.hit(0.25, [0.6, 0.7, 1.0]);
                }
            }
            GameEvent::WallSplat { player, point, normal, speed, .. } => {
                let p = bv(*point);
                let n = bv(*normal);
                sfx.write(Sfx3d::at("wall_splat", p).gain((speed / 25.0).clamp(0.6, 1.2)));
                pushes.add(p, n * speed * 0.4, 2.0);
                let col = corps_rgb(corps_of(Some(*player)));
                burst.write(BurstFx { kind: BurstKind::Dust, pos: p, dir: n, scale: 1.2, color: None });
                let mut o = SpawnOnomatopoeia::key("wall_splat", p, (speed / 20.0).clamp(0.6, 1.6), col);
                o.dir = n;
                o.event_id = event_hash(ev.tick, 77 + player.0 as u64);
                o.local_player = local == Some(*player);
                ono.write(o);
                freeze.freeze_player(*player, 0.09);
                if local == Some(*player) {
                    impact.write(Impact { impact: (speed / 25.0).clamp(0.4, 1.4), dir: -n, local_victim: true, color: rgb_color(col) });
                }
            }
            GameEvent::Ko { victim, killer, cause, victim_corps, streak, double, void, shutdown, pos, vel, zone, poof_delay, .. } => {
                let p = bv(*pos);
                if *zone != Some(BlastZone::Top) {
                    shatter.write(ShatterPlayer { id: *victim, pos: p, vel: bv(*vel), void: *void });
                }
                sfx.write(Sfx3d::at("ko_shatter", p).gain(1.1));
                burst.write(BurstFx { kind: BurstKind::Shards, pos: p + Vec3::Y, dir: bv(*vel).normalize_or_zero(), scale: 1.6, color: Some(corps_rgb(*victim_corps)) });
                impulses.write(DebrisImpulse { center: p, radius: 5.0, strength: 6.0 });
                let killer_corps = killer.map(|k| active.corps_of(k));
                let local_killer = killer.is_some() && local == *killer;
                if let Some(kc) = killer_corps {
                    let step = (*streak).clamp(1, 5) as u8;
                    let name = format!("ko_streak_{}", kc.name().to_lowercase());
                    if local_killer {
                        // Negative space: everything else ducks so the note rings.
                        duck.duck(0.25);
                        sfx.write(Sfx3d::ui(name).step(step).exempt());
                        if *void {
                            sfx.write(Sfx3d::ui("ko_accent_void").exempt());
                        }
                        if *double {
                            sfx.write(Sfx3d::ui("ko_accent_double").exempt());
                        }
                        if *shutdown {
                            sfx.write(Sfx3d::ui("ko_streak_end").exempt());
                        }
                        flare.bloom = 1.0;
                        flare.hit(1.0, corps_rgb(kc));
                        let mut o = SpawnOnomatopoeia::key("ko", p + Vec3::Y * 1.2, 1.4, corps_rgb(kc));
                        o.event_id = event_hash(ev.tick, 991);
                        o.local_player = true;
                        ono.write(o);
                    } else {
                        // You can hear who is streaking.
                        sfx.write(Sfx3d::at(name, p).step(step).gain(0.55));
                    }
                }
                if local == Some(*victim) {
                    sfx.write(Sfx3d::ui("ko").exempt());
                    shake.write(CameraShakeEvent { trauma: 0.6, dir: bv(*vel).normalize_or_zero() });
                }
                match zone {
                    Some(BlastZone::Top) if orbit => {}
                    Some(BlastZone::Top) => {
                        // Star KO: they shrink into a twinkle in the sky; poof after the delay.
                        commands.spawn(StarKo { pos: p, t: 0.0, delay: poof_delay.max(0.3), color: corps_rgb(*victim_corps), poofed: false });
                        sfx.write(Sfx3d::at("ko_accent_void", p).pitch(1.5).exempt());
                    }
                    Some(BlastZone::Side) => {
                        let out = Vec3::new(vel.x, 0.0, vel.z).normalize_or_zero();
                        burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: out, scale: 3.0, color: Some(corps_rgb(*victim_corps)) });
                        burst.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: out, scale: 2.5, color: Some(corps_rgb(*victim_corps)) });
                        sfx.write(Sfx3d::at("explosion_big", p).gain(0.8).pitch(1.2));
                    }
                    Some(BlastZone::Bottom) | None if *cause == KoCause::Void || zone.is_some() => {
                        if eye.distance(p) < 120.0 {
                            burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::Y, scale: 3.0, color: Some(corps_rgb(*victim_corps)) });
                        }
                    }
                    _ => {}
                }
            }
            GameEvent::Launched { player, speed, dir, .. } => {
                if local == Some(*player) {
                    // Launch hits feel HUGE: an extra kick scaled by launch speed.
                    let k = (speed / 40.0).clamp(0.0, 1.5);
                    fov.write(FovPunch { degrees: 3.0 * k });
                    shake.write(CameraShakeEvent { trauma: (0.2 + 0.3 * k).min(0.8), dir: bv(*dir) });
                }
                freeze.freeze_player(*player, (0.03 + speed / 800.0).min(0.1));
            }
            GameEvent::Tech { player, pos, normal } => {
                let p = bv(*pos);
                burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: bv(*normal), scale: 0.8, color: Some([1.0, 1.0, 1.0]) });
                sfx.write(Sfx3d::at("transform_panel_click", p).gain(1.2).exempt());
                sfx.write(Sfx3d::at("shield_block", p).pitch(1.6).gain(0.6));
                let _ = player;
            }
            GameEvent::PunchThrough { player, entry, exit, material } => {
                let (a, b) = (bv(*entry), bv(*exit));
                let v = active.world.player(*player).map_or(b - a, |pl| bv(pl.mv.vel));
                let speed = v.length();
                let dir = (b - a).normalize_or(v.normalize_or(Vec3::Y));
                let def = bd_voxel::material_def(*material);
                let c = bd_voxel::material::linear_color(*material);
                let r = &mut rng.0;
                let mut spray = |at: Vec3, out: Vec3, n: usize, debris: &mut MessageWriter<crate::debris::SpawnDebris>| {
                    for _ in 0..n {
                        let j = 1.0 + (r.f32() - 0.5) * 2.0 * def.jitter;
                        debris.write(crate::debris::SpawnDebris {
                            pos: at + r.unit_vec3() * 0.6,
                            vel: out * speed * r.range(0.3, 0.8) + r.unit_vec3() * 4.0 + Vec3::Y * r.range(1.0, 4.0),
                            color: [c[0] * j, c[1] * j, c[2] * j],
                            size: bd_core::VOXEL_SIZE * r.range(0.4, 0.95),
                            emissive: def.emissive * 3.0,
                            lifetime: r.range(2.5, 5.0),
                        });
                    }
                };
                spray(a, dir, 140, &mut debris);
                // A trail through the wall.
                let len = a.distance(b);
                let steps = (len / 0.8).ceil().clamp(1.0, 12.0) as usize;
                for i in 0..steps {
                    let at = a.lerp(b, (i as f32 + 0.5) / steps as f32);
                    spray(at, dir * 0.5, 10, &mut debris);
                }
                spray(b, dir, 160, &mut debris);
                pushes.add(b, v * 0.8, 3.0);
                burst.write(BurstFx { kind: BurstKind::Dust, pos: a, dir: -dir, scale: 1.6, color: None });
                burst.write(BurstFx { kind: BurstKind::Dust, pos: b, dir, scale: 2.0, color: None });
                let word = match def.debris as usize {
                    0 => "stone",
                    2 => "wood",
                    3 => "leaves",
                    4 => "crystal",
                    _ => "hit_heavy",
                };
                let mut o = SpawnOnomatopoeia::key(word, a, (speed / 25.0).clamp(0.8, 2.0), corps_rgb(active.corps_of(*player)));
                o.dir = dir;
                o.event_id = event_hash(ev.tick, 4242 + player.0 as u64);
                o.local_player = local == Some(*player);
                ono.write(o);
                let brk = match def.debris as usize {
                    0 => "break_stone",
                    2 => "break_wood",
                    3 => "break_leaves",
                    4 => "break_crystal",
                    _ => "break_dirt",
                };
                sfx.write(Sfx3d::at(brk, a).gain(1.2));
                sfx.write(Sfx3d::at("hit_heavy", a));
                sfx.write(Sfx3d::at(brk, b).pitch(0.9));
                let d = eye.distance(a);
                let k = if local == Some(*player) { 1.0 } else { (1.0 - d / 50.0).clamp(0.0, 1.0) };
                if k > 0.0 {
                    shake.write(CameraShakeEvent { trauma: (0.25 + speed / 60.0).min(0.9) * k, dir });
                }
            }
            GameEvent::Respawn { player, pos, .. } => {
                let p = bv(*pos);
                sfx.write(Sfx3d::at("respawn", p).gain(if local == Some(*player) { 1.0 } else { 0.6 }));
                burst.write(BurstFx { kind: BurstKind::Flash, pos: p + Vec3::Y, dir: Vec3::Y, scale: 1.0, color: Some(corps_rgb(active.corps_of(*player))) });
            }
            GameEvent::Explosion { pos, radius, element, source } => {
                let p = bv(*pos);
                let r = *radius;
                sfx.write(Sfx3d::at(if r >= 4.0 { "explosion_big" } else { "explosion_small" }, p).gain(1.2));
                burst.write(BurstFx { kind: BurstKind::Element(*element), pos: p, dir: Vec3::Y, scale: r * 0.6, color: None });
                burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::Y, scale: r * 0.5, color: Some(corps_rgb(corps_of(*source))) });
                pushes.add(p, Vec3::ZERO, r);
                impulses.write(DebrisImpulse { center: p, radius: r * 3.0, strength: 10.0 });
                let mut o = SpawnOnomatopoeia::key("explosion", p + Vec3::Y * r * 0.5, (r / 3.0).clamp(0.6, 2.0), corps_rgb(corps_of(*source)));
                o.event_id = event_hash(ev.tick, (p.x * 13.0) as i64 as u64);
                ono.write(o);
                let c = corps_rgb(corps_of(*source));
                commands.spawn((
                    FlashLight { t: 0.0, life: 0.22, peak: 2.0e6 * (r / 3.0).clamp(0.3, 3.0) },
                    PointLight { color: Color::srgb(0.6 + c[0] * 0.4, 0.6 + c[1] * 0.4, 0.6 + c[2] * 0.4), intensity: 0.0, range: r * 8.0, shadow_maps_enabled: false, ..default() },
                    Transform::from_translation(p + Vec3::Y * 0.8),
                ));
                let d = eye.distance(p);
                let k = (1.0 - d / (r * 16.0)).clamp(0.0, 1.0);
                if k > 0.0 {
                    shake.write(CameraShakeEvent { trauma: 0.2 + 0.5 * k, dir: (p - eye).normalize_or_zero() });
                    fov.write(FovPunch { degrees: 2.0 * k });
                }
            }
            _ => {}
        }
    }
    let now = freeze.now;
    freeze.players.retain(|_, t| *t > now);
    freeze.constructs.retain(|_, t| *t > now);
    freeze.shakes.retain(|_, s| s.4 > now);
    freeze.flashes.retain(|_, f| f.2 > now);
}

/// A short hot point-light flash (explosions, muzzle flashes).
#[derive(Component)]
pub struct FlashLight {
    pub t: f32,
    pub life: f32,
    pub peak: f32,
}

pub fn fade_flash_lights(mut commands: Commands, time: Res<Time<Real>>, mut q: Query<(Entity, &mut FlashLight, &mut PointLight)>) {
    for (e, mut l, mut light) in &mut q {
        l.t += time.delta_secs();
        let k = 1.0 - l.t / l.life;
        if k <= 0.0 {
            commands.entity(e).despawn();
        } else {
            light.intensity = l.peak * k * k;
        }
    }
}

/// Star KO twinkle: a bright point high in the sky that sparkles, then poofs.
#[derive(Component)]
struct StarKo {
    pos: Vec3,
    t: f32,
    delay: f32,
    color: [f32; 3],
    poofed: bool,
}

fn star_kos(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut q: Query<(Entity, &mut StarKo)>,
    mut burst: MessageWriter<BurstFx>,
    mut sfx: MessageWriter<Sfx3d>,
    mut debris: MessageWriter<crate::debris::SpawnDebris>,
) {
    for (e, mut s) in &mut q {
        s.t += time.delta_secs();
        // Twinkle: a few glowing sparks per frame, flickering.
        if !s.poofed && ((s.t * 20.0) as i32) % 2 == 0 {
            debris.write(crate::debris::SpawnDebris { pos: s.pos, vel: Vec3::ZERO, color: s.color, size: 0.6 + 0.4 * (s.t * 17.0).sin().abs(), emissive: 25.0, lifetime: 0.06 });
        }
        if !s.poofed && s.t >= s.delay {
            s.poofed = true;
            burst.write(BurstFx { kind: BurstKind::Flash, pos: s.pos, dir: Vec3::Y, scale: 4.0, color: Some(s.color) });
            burst.write(BurstFx { kind: BurstKind::Shards, pos: s.pos, dir: Vec3::ZERO, scale: 3.0, color: Some(s.color) });
            sfx.write(Sfx3d::ui("ko_shatter").pitch(1.4).gain(0.8));
        }
        if s.t > s.delay + 0.5 {
            commands.entity(e).despawn();
        }
    }
}

fn spawn_flare(mut commands: Commands) {
    let ring = |size: f32| Node {
        position_type: PositionType::Absolute,
        left: percent(50),
        top: percent(50),
        width: px(size),
        height: px(size),
        margin: UiRect { left: px(-size * 0.5), top: px(-size * 0.5), ..default() },
        border: UiRect::all(px(2.0)),
        border_radius: BorderRadius::MAX,
        ..default()
    };
    commands.spawn((FlareRing, ring(22.0), BorderColor::all(Color::NONE), Pickable::IGNORE, GlobalZIndex(40)));
    commands.spawn((FlareBloom, ring(10.0), BackgroundColor(Color::NONE), BorderColor::all(Color::NONE), Pickable::IGNORE, GlobalZIndex(39)));
}

fn despawn_flare(mut commands: Commands, q: Query<Entity, Or<(With<FlareRing>, With<FlareBloom>)>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn animate_flare(
    time: Res<Time<Real>>,
    mut flare: ResMut<CrosshairFlare>,
    mut ring: Query<(&mut Node, &mut BorderColor), (With<FlareRing>, Without<FlareBloom>)>,
    mut bloom: Query<(&mut Node, &mut BackgroundColor), (With<FlareBloom>, Without<FlareRing>)>,
) {
    let dt = time.delta_secs();
    flare.intensity = (flare.intensity - dt * 5.0).max(0.0);
    flare.bloom = (flare.bloom - dt * 2.2).max(0.0);
    let k = flare.intensity;
    if let Ok((mut n, mut c)) = ring.single_mut() {
        let size = 16.0 + 18.0 * k;
        n.width = px(size);
        n.height = px(size);
        n.margin = UiRect { left: px(-size * 0.5), top: px(-size * 0.5), ..default() };
        let col = flare.color;
        *c = BorderColor::all(Color::srgba(col[0], col[1], col[2], (k * 0.9).min(0.9)));
    }
    if let Ok((mut n, mut bg)) = bloom.single_mut() {
        let b = flare.bloom;
        let size = 8.0 + 46.0 * (1.0 - b) * (b > 0.0) as u8 as f32;
        n.width = px(size);
        n.height = px(size);
        n.margin = UiRect { left: px(-size * 0.5), top: px(-size * 0.5), ..default() };
        let col = flare.color;
        bg.0 = Color::linear_rgba(col[0], col[1], col[2], b * b * 0.45);
    }
}
