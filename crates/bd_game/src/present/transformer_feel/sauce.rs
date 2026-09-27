//! TRANSFORMERS V3 SAUCE (owner: "the transformer hit/shoot animations are lacking in sauce … make
//! EVERYTHING feel to the level of hollow purple"). Every robot strike, blaster / cannon shot,
//! transform attack and finisher gets the move grammar, keyed by DATA (the archetype's `fx` flavour
//! and transform attack → a [`Weapon`] identity), never by a character's name:
//!
//! * **GATHER** — a heavy's hold coils the servos (the body leans back, twists away, squashes, trembles
//!   near full) while motes and energy lines pull into the fist under a rising hum
//!   (`tfs_charge_hum`); a recharging blaster glows at the muzzle; the finisher's snap back to robot
//!   gathers light at the chest.
//! * **RELEASE** — a smear arc in the robot's colour (energon core by weapon) sweeps through the
//!   strike, the body lunges / twists / stretches into it (held through the hit-stop, then a damped
//!   overshoot back); heavies tear the air (a ring + `tfs_air_tear`). Shots: muzzle flash, a hot
//!   tracer, the weapon's own voice (tank shell thump, jet cannon chatter, heli chin gun, blaster zip,
//!   missile whoosh), ejected casings and a smoke ring on shells, a recoil rock of the body / hull.
//! * **WORLD REACTION** — metal sparks (bouncing, hot), shards in the robot's colour, a shockwave ring
//!   on heavies, ground cracks + a dust wall on slams, finishers and vehicle attacks; skid scars /
//!   tyre burns under wheels and treads through a vehicle attack; the heli's blade disc, the grinder's
//!   spark spray, the stampede's dust.
//! * **TIME** — tiered ASYMMETRIC hit-stop ([`tf_tier`], [`tf_hitstop`]: light 3–6 frames, heavy
//!   10–18, vehicle attack 15–22, finisher 20–25; the attacker cuts through 25 % sooner), impact
//!   frames + camera kick on heavies / vehicle attacks / finishers, and the finisher's metallic crunch
//!   + BASS DROP (`tfs_finisher_drop`).
//!
//! Presentation only (no sim or network change). Everything is GPU particles (bd_fx ring pool) and
//! `Sfx3d` cues; per-robot state lives on the [`TfSauce`] component. No per-frame allocation.
//! `BD_TF_SAUCE=0` turns the pass off (before / after strips).

use super::super::constructs::ConstructEntities;
use super::super::impact::HitFreeze;
use super::super::impact_frames::{ImpactFrameRequest, ImpactTrigger};
use super::super::powers::movefx::draw::{Look, Pen, mix3};
use super::super::powers::movefx::table::Body;
use super::super::sound::Sfx3d;
use super::super::{bv, corps_rgb, gv};
use super::{Jit, TfFeel, TfSet};
use crate::juice::{CameraShakeEvent, FovPunch};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use bd_core::ConstructId;
use bd_fx::{BurstFx, BurstKind, Emit, FxParticles, PFlags, PKind};
use bd_sim::GameEvent;
use bd_sim::combat::HitKind;
use bd_sim::constructs::brawl::{XAttack, XPhase};
use bd_sim::events::HeldFx;
use bd_sim::weapons::GunArchetype;
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

pub fn build(app: &mut App) {
    app.add_message::<ImpactFrameRequest>()
        .add_message::<CameraShakeEvent>()
        .add_message::<FovPunch>()
        .add_message::<BurstFx>()
        .add_message::<Sfx3d>();
    if std::env::var("BD_TF_SAUCE").is_ok_and(|v| v == "0") {
        info!("transformer sauce: OFF (BD_TF_SAUCE=0)");
        return;
    }
    app.add_systems(Update, (attach, events, connects, frame).chain().in_set(TfSet).after(super::combat::layers));
}

// ------------------------------------------------------------------------------------ identity

/// A transformer's weapon identity (look + voice), from its archetype row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weapon {
    /// Trucks / trains / boats: the energon blade (axe) — a humming cyan edge, a heavy ram.
    Blade,
    /// Cars: twin blasters, drift-ram tyre burns.
    Blaster,
    /// Jets: chattering cannons, the fly-by.
    Cannon,
    /// Helicopters: the chin gun, the rotor-blade sweep.
    ChinGun,
    /// Tanks / colossi: shells that thump, casings, smoke rings, the barrage / quake.
    Shell,
    /// Construction: the grinder's spark spray.
    Grinder,
    /// Bikes: quick blasters, the spin-out's burning circle.
    Spin,
    /// Dinos: jaws that crunch, the stampede.
    Jaws,
    /// Beasts: raking claws, the pounce.
    Claw,
}

/// Look + voice of a [`Weapon`] (sounds: `tools/sfxgen/sounds_tf_sauce.py` + the transformer feel set).
#[derive(Clone, Copy, Debug)]
pub struct WeaponStyle {
    /// Layered on every robot strike's swing (None = the feel pass's swing is enough).
    pub swing: Option<&'static str>,
    /// A shot's voice.
    pub shot: &'static str,
    /// Layered on every connect (bites / rakes / grinds), else None.
    pub connect: Option<&'static str>,
    /// The transform attack's launch voice.
    pub xattack: &'static str,
    /// Energy / muzzle colour (linear rgb): the smear's core, the muzzle flash, the gather.
    pub energy: [f32; 3],
    /// Muzzle flash size (m at a 6 m robot).
    pub muzzle: f32,
    /// Ejects casings + a smoke ring per shot.
    pub casing: bool,
    /// Wheels / treads leave skid scars through a vehicle attack.
    pub skid: bool,
    /// Smear arcs per strike (claws rake with three).
    pub arcs: u8,
}

impl Weapon {
    /// Every weapon (tests, docs).
    pub const ALL: [Weapon; 9] = [Weapon::Blade, Weapon::Blaster, Weapon::Cannon, Weapon::ChinGun, Weapon::Shell, Weapon::Grinder, Weapon::Spin, Weapon::Jaws, Weapon::Claw];

    /// From an archetype row: the creature flavours first (a dino's beast alt keeps its jaws), then
    /// the transform attack (every row has one).
    pub fn of(fx: &str, xattack: XAttack) -> Weapon {
        match fx {
            "dino" => return Weapon::Jaws,
            "beast" => return Weapon::Claw,
            _ => {}
        }
        match xattack {
            XAttack::Ram => Weapon::Blade,
            XAttack::DriftRam => Weapon::Blaster,
            XAttack::FlyBy => Weapon::Cannon,
            XAttack::RotorSweep => Weapon::ChinGun,
            XAttack::Barrage | XAttack::Quake => Weapon::Shell,
            XAttack::Grind => Weapon::Grinder,
            XAttack::SpinOut => Weapon::Spin,
            XAttack::Stampede => Weapon::Jaws,
            XAttack::Pounce => Weapon::Claw,
        }
    }

    pub fn style(self) -> WeaponStyle {
        let base = WeaponStyle { swing: None, shot: "tfs_blaster_zip", connect: None, xattack: "tfs_air_tear", energy: [1.0, 0.8, 0.35], muzzle: 0.9, casing: false, skid: false, arcs: 1 };
        match self {
            Weapon::Blade => WeaponStyle { swing: Some("tfs_blade_hum"), energy: [0.35, 0.85, 1.0], skid: true, ..base },
            Weapon::Blaster => WeaponStyle { energy: [1.0, 0.75, 0.25], skid: true, ..base },
            Weapon::Cannon => WeaponStyle { shot: "tfs_cannon_chatter", energy: [1.0, 0.6, 0.2], muzzle: 1.1, casing: true, ..base },
            Weapon::ChinGun => WeaponStyle { shot: "tfs_chin_gun", energy: [1.0, 0.7, 0.3], muzzle: 1.0, casing: true, ..base },
            Weapon::Shell => WeaponStyle { shot: "tfs_shell_thump", energy: [1.0, 0.5, 0.15], muzzle: 1.9, casing: true, skid: true, ..base },
            Weapon::Grinder => WeaponStyle { connect: Some("tfs_grind_screech"), xattack: "tfs_grind_screech", energy: [1.0, 0.65, 0.2], skid: true, ..base },
            Weapon::Spin => WeaponStyle { xattack: "tfs_spin_whirr", energy: [0.8, 0.4, 1.0], skid: true, ..base },
            Weapon::Jaws => WeaponStyle { connect: Some("tfs_bite_crunch"), energy: [1.0, 0.45, 0.15], arcs: 2, ..base },
            Weapon::Claw => WeaponStyle { connect: Some("tfs_claw_rake"), energy: [0.6, 1.0, 0.5], arcs: 3, ..base },
        }
    }
}

// ------------------------------------------------------------------------------------ time

/// Hit tier of a transformer's connect (asymmetric hit-stop, impact frames, camera kick).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TfTier {
    Light,
    Heavy,
    Vehicle,
    Finisher,
}

/// One frame at 60 fps (the hit-stop tiers are authored in frames).
pub const FRAME: f32 = 1.0 / 60.0;
/// The attacker's hold = the victim's × this (the attacker cuts through first).
pub const ATTACKER_FRAC: f32 = 0.75;

/// Tier of a transformer hit: `burst` / `finish` = the construct's transform-attack phase.
pub fn tf_tier(kind: HitKind, strength: f32, knockback: f32, lethal: bool, burst: bool, finish: bool) -> TfTier {
    let heavy_kind = matches!(kind, HitKind::HeavyMelee | HitKind::Crush | HitKind::Explosion);
    if lethal || knockback >= 45.0 || (finish && heavy_kind) {
        TfTier::Finisher
    } else if burst && kind == HitKind::HeavyMelee {
        TfTier::Vehicle
    } else if heavy_kind || burst || strength >= 1.5 {
        TfTier::Heavy
    } else {
        TfTier::Light
    }
}

/// (victim, attacker) hit-stop seconds: light 3–6 frames, heavy 10–18, vehicle attack 15–22,
/// finisher 20–25 (by the hit's impact strength inside the tier); the attacker's is 25 % shorter.
pub fn tf_hitstop(tier: TfTier, strength: f32) -> (f32, f32) {
    let s = if strength.is_finite() { strength } else { 1.0 };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
    let frames = match tier {
        TfTier::Light => lerp(3.0, 6.0, (s - 0.9) / 0.6),
        TfTier::Heavy => lerp(10.0, 18.0, (s - 1.5) / 1.0),
        TfTier::Vehicle => lerp(15.0, 22.0, (s - 2.0) / 1.0),
        TfTier::Finisher => lerp(20.0, 25.0, (s - 2.0) / 0.6),
    };
    let v = frames * FRAME;
    (v, v * ATTACKER_FRAC)
}

// ------------------------------------------------------------------------------------ pose

/// The body's strike pose (root-level: lean, twist, stretch — reads at 20–40 m).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pose {
    #[default]
    None,
    /// A string hit (light) / ender (`heavy`).
    Strike,
    /// Launcher / uppercut / finisher rising.
    Rise,
    /// Ground pound / slam.
    Slam,
    /// A shot's recoil (robot or hull).
    Recoil,
}

/// 1 while held, then a damped overshoot back to 0 over `dur`.
fn settle(u: f32, hold: f32, dur: f32) -> f32 {
    if u <= hold {
        return 1.0;
    }
    let x = ((u - hold) / dur.max(0.01)).min(1.0);
    (1.0 - x) * (1.0 - x) * (x * PI * 1.5).cos()
}

/// (lean forward deg, twist deg, vertical stretch) of a pose `u` s after its release; `k` = weight.
pub fn pose_at(p: Pose, u: f32, side: f32, k: f32) -> (f32, f32, f32) {
    match p {
        Pose::None => (0.0, 0.0, 1.0),
        Pose::Strike => {
            let s = settle(u, 0.05 + 0.05 * k, 0.24 + 0.14 * k);
            (11.0 * k * s, side * 18.0 * k * s, 1.0 + 0.05 * k * s)
        }
        Pose::Rise => {
            let s = settle(u, 0.12, 0.42);
            (-12.0 * k * s, side * 12.0 * k * s, 1.0 + 0.1 * k * s)
        }
        Pose::Slam => {
            let s = settle(u, 0.1, 0.45);
            (17.0 * k * s, 0.0, 1.0 - 0.12 * k * s)
        }
        Pose::Recoil => {
            let s = settle(u, 0.02, 0.22);
            (-7.0 * k * s, side * 3.0 * k * s, 1.0 - 0.04 * k * s)
        }
    }
}

/// Per-robot sauce state.
#[derive(Component)]
pub struct TfSauce {
    pub weapon: Weapon,
    pose: Pose,
    pose_u: f32,
    pose_k: f32,
    pose_dir: Vec3,
    side: f32,
    /// Anticipation coil 0..1 (a heavy's hold), smoothed.
    coil: f32,
    hum: bool,
    xatk: XPhase,
    skid_prev: Option<(Vec3, Vec3)>,
    /// Vehicle mode: the pilot's aim yaw last frame + the swivel cue's cooldown (turrets / chin guns).
    aim_yaw: Option<f32>,
    swivel_cd: f32,
    fx_t: f32,
    /// Robot colour (the owner's corps, linear).
    color: [f32; 3],
}

fn attach(mut commands: Commands, active: Option<Res<ActiveWorld>>, views: Res<WorldViews>, q: Query<(Entity, &TfFeel), Without<TfSauce>>) {
    let Some(active) = active else { return };
    for (e, f) in &q {
        let Some(c) = active.world.constructs.get(f.id) else { continue };
        let a = active.world.cfg.tuning.transformer.archetype_of(&c.recipe);
        let color = views.player(c.owner).map_or([1.0; 3], |v| corps_rgb(v.corps));
        commands.entity(e).try_insert(TfSauce {
            weapon: Weapon::of(&a.fx, a.xattack),
            pose: Pose::None,
            pose_u: 9.0,
            pose_k: 1.0,
            pose_dir: Vec3::NEG_Z,
            side: 1.0,
            coil: 0.0,
            hum: false,
            xatk: XPhase::None,
            skid_prev: None,
            aim_yaw: None,
            swivel_cd: 0.0,
            fx_t: 0.0,
            color,
        });
    }
}

fn look(color: [f32; 3], energy: [f32; 3], glow: f32, size: f32) -> Look {
    Look { body: Body::Energy, main: color, core: mix3(energy, [1.0; 3], 0.45), dark: [0.05; 3], glow, size, ..default() }
}

/// A smear arc through the strike: `n` parallel sweeps (claws rake three), core + robot-colour rim,
/// laid over ~70 ms so it reads as a sweep; world-sized to the robot.
#[allow(clippy::too_many_arguments)]
fn smear(pen: &mut Pen, c: Vec3, dir: Vec3, h: f32, side: f32, k: f32, color: [f32; 3], energy: [f32; 3], n: u8, rise: bool) {
    let d = Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::NEG_Z);
    let right = d.cross(Vec3::Y).normalize_or(Vec3::X);
    let r = h * (0.36 + 0.1 * k);
    let segs = 9;
    let thick = (0.07 + 0.045 * k) * h.sqrt();
    for j in 0..n.max(1) {
        let off = (j as f32 - (n.max(1) - 1) as f32 * 0.5) * h * 0.08;
        // Sweep plane: a flat arc across the front (strikes), a vertical one (rising).
        let (u, w) = if rise { (d, Vec3::Y) } else { (right * side, d) };
        let centre = c + Vec3::Y * off;
        let mut prev: Option<Vec3> = None;
        for i in 0..=segs {
            let t = i as f32 / segs as f32;
            let a = -1.1 + 2.2 * t;
            let p = centre + (u * a.sin() + w * a.cos()) * r;
            if let Some(q) = prev {
                let fade = 0.35 + 0.65 * t;
                let delay = t * 0.07;
                let life = 0.16 + 0.1 * k;
                pen.put(Emit { kind: PKind::Beam, flags: PFlags::PLAIN, pos: (p + q) * 0.5, vel: p - q, size: thick * 2.2 * fade, color, emissive: 2.6 + k, life, gravity: 0.0, delay, ..default() });
                pen.put(Emit { kind: PKind::Beam, flags: PFlags::PLAIN, pos: (p + q) * 0.5, vel: p - q, size: thick * fade, color: mix3(energy, [1.0; 3], 0.5), emissive: 5.0 + 2.0 * k, life: life * 0.7, gravity: 0.0, delay, ..default() });
            }
            prev = Some(p);
        }
    }
}

/// Hot metal sparks flung along `dir` (bounce on the floor at `floor`).
fn sparks(pen: &mut Pen, j: &mut Jit, p: Vec3, dir: Vec3, n: usize, speed: f32, floor: f32) {
    for _ in 0..n {
        let d = (dir * 0.8 + j.dir() * 0.75).normalize_or(Vec3::Y);
        let hot = j.f();
        pen.put(Emit {
            kind: PKind::Ballistic,
            flags: PFlags::STRETCH | PFlags::BOUNCE | PFlags::WHITE_BIRTH,
            pos: p,
            vel: d * speed * j.range(0.4, 1.2) + Vec3::Y * 2.0,
            size: j.range(0.03, 0.06),
            color: [1.0, 0.55 + 0.35 * hot, 0.15 + 0.2 * hot],
            emissive: 7.0,
            life: j.range(0.35, 0.8),
            gravity: 1.6,
            drag: 0.6,
            spin: 3.5,
            aux: floor,
            seed: j.0,
            ..default()
        });
    }
}

fn ground_y(active: &ActiveWorld, p: Vec3, depth: f32) -> Option<f32> {
    bd_core::voxel::raycast(&active.world.voxels, gv(p + Vec3::Y * 0.5), bd_core::Vec3::NEG_Y, depth + 0.5).map(|h| h.point.y)
}

// ------------------------------------------------------------------------------------ events

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn events(
    mut events: MessageReader<GameEvents>,
    mut shots_in: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    map: Res<ConstructEntities>,
    anchor: Res<crate::camera::ViewAnchor>,
    time: Res<Time<Real>>,
    mut q: Query<(&TfFeel, &mut TfSauce)>,
    mut fx: Option<ResMut<FxParticles>>,
    out: (MessageWriter<Sfx3d>, MessageWriter<CameraShakeEvent>, MessageWriter<BurstFx>),
    mut shot_ids: Local<Vec<ConstructId>>,
    mut blasters: Local<Vec<ConstructId>>,
) {
    let (mut sfx, mut shake, mut bursts) = out;
    let Some(active) = active else {
        events.clear();
        shots_in.clear();
        return;
    };
    let Some(fx) = fx.as_deref_mut() else {
        events.clear();
        shots_in.clear();
        return;
    };
    let eye = anchor.eye;
    let mut j = Jit((time.elapsed_secs() * 1000.0) as u32 | 1);
    shot_ids.clear();
    // Constructs that fired this batch: their `PilotAttack` is the blaster, not a swing.
    blasters.clear();
    for ev in shots_in.read() {
        if let GameEvent::ShotFired { construct: Some(id), .. } = &ev.event
            && !blasters.contains(id)
        {
            blasters.push(*id);
        }
    }
    // Pass 1: shots (a blaster's `PilotAttack` is not a swing).
    for ev in events.read() {
        match &ev.event {
            GameEvent::ShotFired { construct: Some(id), archetype, muzzle, dir, .. } => {
                let Some(&e) = map.0.get(id) else { continue };
                let Ok((f, mut s)) = q.get_mut(e) else { continue };
                let st = s.weapon.style();
                let p = bv(*muzzle);
                let d = bv(*dir).normalize_or(Vec3::NEG_Z);
                let first = !shot_ids.contains(id);
                let w = f.weight();
                let mut pen = Pen { fx: &mut *fx, motes: 1.0, scale: 1.0, cam: eye, avoid: None };
                let size = st.muzzle * (0.5 + 0.5 * w);
                // Muzzle flash: a white-hot core in the weapon's fire + a spark cone + a hot tracer.
                pen.flash(p, size, st.energy, 9.0, 0.07);
                pen.flash(p + d * size * 0.4, size * 0.5, [1.0, 0.97, 0.9], 14.0, 0.05);
                pen.beam(p, p + d * (6.0 + 4.0 * w), 0.05 * size, mix3(st.energy, [1.0; 3], 0.6), 8.0, 0.06);
                for _ in 0..5 {
                    let sd = (d + j.dir() * 0.35).normalize_or(d);
                    pen.put(Emit { flags: PFlags::STRETCH | PFlags::PLAIN, pos: p, vel: sd * j.range(10.0, 22.0), size: 0.035 * size, color: st.energy, emissive: 8.0, life: 0.1, gravity: 0.0, spin: 3.0, ..default() });
                }
                if st.casing || matches!(archetype, GunArchetype::Cannon | GunArchetype::Minigun) {
                    // Blast: a smoke ring ⟂ the barrel + an ejected casing tumbling out the side.
                    let smoke = Look { body: Body::Smoke, main: [0.5, 0.48, 0.45], dark: [0.2; 3], glow: 0.8, size: 0.6 * size, ..default() };
                    pen.ring(&smoke, p, d, 0.2 * size, 4.0 + 3.0 * size, 10.0, 0.6);
                    let side = d.cross(Vec3::Y).normalize_or(Vec3::X);
                    let floor = ground_y(&active, p, 12.0).unwrap_or(p.y - 3.0);
                    pen.put(Emit { flags: PFlags::BOUNCE, pos: p - d * size * 0.8, vel: side * j.range(3.0, 5.0) + Vec3::Y * j.range(3.0, 5.0), size: 0.06 * size, color: [0.85, 0.62, 0.22], emissive: 1.2, life: 1.6, gravity: 1.0, drag: 0.2, spin: 12.0, aux: floor, seed: j.0, ..default() });
                    if matches!(s.weapon, Weapon::Shell) && first && p.y - floor < 4.0 {
                        bursts.write(BurstFx { kind: BurstKind::Dust, pos: Vec3::new(p.x, floor, p.z), dir: Vec3::Y, scale: 0.9 + 0.5 * w, color: None });
                    }
                }
                if *archetype == GunArchetype::Rocket {
                    // Missile: an ignition puff trail.
                    let smoke = Look { body: Body::Smoke, main: [0.6, 0.58, 0.55], dark: [0.25; 3], glow: 0.8, size: 0.5, ..default() };
                    pen.burst(&smoke, p, 8.0, 3.0, Some(-d), 0.6, 0.9);
                }
                if first {
                    shot_ids.push(*id);
                    // Vehicle weapons too (turret cannon, MG, missiles, chin gun): the archetype picks
                    // the voice; an MG's rounds keep the gun presentation's own crack.
                    let name = match archetype {
                        GunArchetype::Rocket => Some("tfs_missile_whoosh"),
                        GunArchetype::Minigun => None,
                        GunArchetype::Cannon if !f.robot => Some("tfs_shell_thump"),
                        _ => Some(st.shot),
                    };
                    let pitch = (f.pitch() * 0.9 + 0.1).clamp(0.6, 1.4);
                    if let Some(name) = name {
                        sfx.write(Sfx3d::at(name, p).pitch(pitch).gain((0.7 + 0.15 * w).min(1.1)));
                    }
                    // Recoil rock (robot or hull), a small kick for the local pilot.
                    s.pose = Pose::Recoil;
                    s.pose_u = 0.0;
                    s.pose_k = if *archetype == GunArchetype::Cannon || matches!(s.weapon, Weapon::Shell) { 1.8 } else if *archetype == GunArchetype::Minigun { 0.35 } else { 1.0 };
                    s.pose_dir = d;
                    if f.local {
                        shake.write(CameraShakeEvent { trauma: if matches!(s.weapon, Weapon::Shell) { 0.22 } else { 0.07 }, dir: -d });
                    }
                }
            }
            GameEvent::HeldFx { id, fx: kind, dir, strength, .. } if matches!(kind, HeldFx::PilotAttack | HeldFx::Uppercut | HeldFx::GroundPound | HeldFx::TransformSlam) => {
                if blasters.contains(id) && *kind == HeldFx::PilotAttack {
                    continue;
                }
                let Some(&e) = map.0.get(id) else { continue };
                let Ok((f, mut s)) = q.get_mut(e) else { continue };
                if !f.robot {
                    continue;
                }
                let st = s.weapon.style();
                let k = strength.clamp(0.8, 2.6);
                let heavy = k >= 1.6 || *kind != HeldFx::PilotAttack;
                let h = f.height;
                let centre = super::centre_of(&active, *id, Vec3::ZERO);
                let d = bv(*dir);
                let dflat = Vec3::new(d.x, 0.0, d.z).normalize_or(s.pose_dir);
                s.side = -s.side;
                s.pose = match kind {
                    HeldFx::Uppercut => Pose::Rise,
                    HeldFx::GroundPound | HeldFx::TransformSlam => Pose::Slam,
                    _ => Pose::Strike,
                };
                s.pose_u = 0.0;
                s.pose_k = if heavy { 1.5 + 0.2 * (k - 1.6).max(0.0) } else { 0.8 + 0.25 * (k - 1.0).max(0.0) };
                s.pose_dir = dflat;
                s.coil = 0.0;
                s.hum = false;
                let side = s.side;
                let color = s.color;
                let mut pen = Pen { fx: &mut *fx, motes: 1.0, scale: 1.0, cam: eye, avoid: None };
                let chest = centre + Vec3::Y * h * 0.12 + dflat * h * 0.12;
                smear(&mut pen, chest, dflat, h, side, if heavy { 1.6 } else { 1.0 }, color, st.energy, st.arcs, s.pose == Pose::Rise);
                let pitch = f.pitch();
                if let Some(sw) = st.swing {
                    sfx.write(Sfx3d::at(sw, chest).pitch((pitch * if heavy { 0.85 } else { 1.0 }).clamp(0.55, 1.5)).gain(if heavy { 0.8 } else { 0.55 }));
                }
                if heavy {
                    // Air tear: a ring ⟂ the strike at the fist + a flash.
                    let fist = chest + dflat * h * 0.45;
                    let lk = look(color, st.energy, 4.0, 0.9 + 0.3 * f.weight());
                    let normal = if s.pose == Pose::Rise { Vec3::Y } else { dflat };
                    pen.ring(&lk, fist, normal, 0.2 * h * 0.3, 10.0 + 4.0 * f.weight(), 16.0, 0.22);
                    pen.flash(fist, 0.35 * h * 0.35, st.energy, 6.0, 0.08);
                    sfx.write(Sfx3d::at("tfs_air_tear", fist).pitch(pitch.clamp(0.6, 1.4)).gain(0.8));
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------------------------------ connects

/// Everything that answers a transformer's connect: tiered asymmetric hit-stop, sparks / shards /
/// shockwave / cracks, impact frames + camera kick, the finisher's crunch + bass drop.
#[allow(clippy::too_many_arguments)]
pub fn connects(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    map: Res<ConstructEntities>,
    anchor: Res<crate::camera::ViewAnchor>,
    time: Res<Time<Real>>,
    mut freeze: ResMut<HitFreeze>,
    q: Query<(&TfFeel, &TfSauce)>,
    mut fx: Option<ResMut<FxParticles>>,
    out: (MessageWriter<Sfx3d>, MessageWriter<ImpactFrameRequest>, MessageWriter<CameraShakeEvent>, MessageWriter<FovPunch>, MessageWriter<BurstFx>),
) {
    let (mut sfx, mut frames, mut shake, mut fov, mut bursts) = out;
    let Some(active) = active else {
        events.clear();
        return;
    };
    let Some(fx) = fx.as_deref_mut() else {
        events.clear();
        return;
    };
    let eye = anchor.eye;
    let local = active.local;
    let mut j = Jit((time.elapsed_secs() * 733.0) as u32 | 3);
    for ev in events.read() {
        let GameEvent::Hit { target, source, construct: Some(id), kind, knockback, point, lethal, impact, .. } = &ev.event else { continue };
        let Some(&e) = map.0.get(id) else { continue };
        let Ok((f, s)) = q.get(e) else { continue };
        let c = active.world.constructs.get(*id);
        let phase = c.map_or(XPhase::None, |c| c.brawl.xatk);
        let tier = tf_tier(*kind, impact.strength, *knockback, *lethal, phase == XPhase::Burst, phase == XPhase::Finish);
        // TIME: asymmetric hit-stop (longest request wins over the generic connect tier).
        let (vs, as_) = tf_hitstop(tier, impact.strength);
        freeze.now = time.elapsed_secs_f64();
        freeze.hold_player(*target, vs);
        if let Some(src) = source {
            freeze.hold_player(*src, as_);
        }
        freeze.hold_construct(*id, as_);
        let st = s.weapon.style();
        let p = bv(*point);
        let dir = bv(impact.dir).normalize_or(s.pose_dir);
        let w = f.weight();
        let floor = ground_y(&active, p, 8.0);
        let near_ground = floor.is_some_and(|g| p.y - g < 2.5);
        let floor_y = floor.unwrap_or(p.y - 4.0);
        let involved = local.is_some() && (local == Some(*target) || *source == local || f.local);
        let dist = eye.distance(p);
        let mut pen = Pen { fx: &mut *fx, motes: 1.0, scale: 1.0, cam: eye, avoid: None };
        let t = tier as usize;
        // WORLD REACTION: hot metal sparks + shards in the robot's colour.
        if dist < 90.0 {
            sparks(&mut pen, &mut j, p, dir, [10, 24, 34, 48][t], [9.0, 14.0, 17.0, 20.0][t], floor_y);
            bursts.write(BurstFx { kind: BurstKind::Shards, pos: p, dir, scale: [0.5, 0.9, 1.2, 1.5][t] * (0.7 + 0.3 * w), color: Some(s.color) });
            let lk = look(s.color, st.energy, 4.5, 1.0 + 0.3 * w);
            pen.flash(p, [0.5, 0.9, 1.2, 1.6][t] * (0.6 + 0.4 * w), mix3(st.energy, [1.0; 3], 0.5), 8.0, 0.07);
            if tier >= TfTier::Heavy {
                // Shockwave ring ⟂ the hit + an outward star of energy lines.
                pen.ring(&lk, p, dir, 0.3, [0.0, 16.0, 20.0, 24.0][t], [0.0, 18.0, 24.0, 32.0][t], 0.26);
                for _ in 0..[0, 6, 9, 12][t] {
                    let d = (dir + j.dir() * 0.9).normalize_or(dir);
                    let len = j.range(1.2, 2.6) * (0.6 + 0.4 * w);
                    pen.beam(p + d * 0.3, p + d * (0.3 + len), 0.05, mix3(s.color, [1.0; 3], 0.4), 6.0, 0.1);
                }
            }
            if tier >= TfTier::Vehicle || (tier == TfTier::Heavy && near_ground && matches!(kind, HitKind::Crush | HitKind::Explosion)) || (tier == TfTier::Heavy && near_ground && s.pose == Pose::Slam) {
                // Ground cracks radiating from under the hit + a dust wall.
                let g = Vec3::new(p.x, floor_y + 0.02, p.z);
                let cracks = if tier == TfTier::Finisher { 6 } else { 4 };
                let clk = look([0.95, 0.75, 0.4], st.energy, 3.0, 1.0);
                for i in 0..cracks {
                    let a = i as f32 / cracks as f32 * TAU + j.range(-0.3, 0.3);
                    pen.crack(&clk, g, Vec3::new(a.cos(), 0.0, a.sin()), j.range(2.0, 3.8) * (0.7 + 0.3 * w), j.0.wrapping_add(i));
                }
                pen.dust_wall(&lk, g, 1.2 + 0.4 * w, 5.0, 18.0);
            }
        }
        // Voice: the weapon's connect (bites crunch, claws rake, grinders screech) + the finisher drop.
        if let Some(name) = st.connect
            && (tier >= TfTier::Heavy || !matches!(s.weapon, Weapon::Grinder))
        {
            sfx.write(Sfx3d::at(name, p).pitch((f.pitch() * 0.95).clamp(0.6, 1.4)).gain(0.85));
        }
        if tier == TfTier::Finisher {
            let s3 = if involved { Sfx3d::ui("tfs_finisher_drop").exempt() } else { Sfx3d::at("tfs_finisher_drop", p) };
            sfx.write(s3.gain(1.0));
        } else if tier == TfTier::Vehicle {
            sfx.write(Sfx3d::at("robot_finisher_crunch", p).pitch((f.pitch() * 1.1).clamp(0.6, 1.3)).gain(0.8));
        }
        // Impact frames + camera kick on heavies / vehicle attacks / finishers.
        if tier >= TfTier::Heavy {
            let (style, weight) = match tier {
                TfTier::Finisher => ("comic", 1.1),
                TfTier::Vehicle => ("comic", 0.8),
                _ => ("ink", 0.5),
            };
            frames.write(ImpactFrameRequest::new(ImpactTrigger::HeavyWeapon, p).between(*source, Some(*target)).weight(weight).dir(dir).style(style).tint(st.energy));
            let trauma = [0.0, 0.25, 0.4, 0.55][t];
            if involved {
                shake.write(CameraShakeEvent { trauma, dir });
                fov.write(FovPunch { degrees: [0.0, -3.0, -5.0, -7.0][t] });
            } else if dist < 35.0 {
                shake.write(CameraShakeEvent { trauma: trauma * 0.5 * (1.0 - dist / 35.0), dir });
            }
        }
    }
}

// ------------------------------------------------------------------------------------ per frame

/// Pose (anticipation coil → strike → overshoot), gathers (heavy hold, blaster recharge, the
/// finisher's snap back), and the vehicle attack's running world reaction.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn frame(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    anchor: Res<crate::camera::ViewAnchor>,
    freeze: Res<HitFreeze>,
    mut q: Query<(&TfFeel, &mut TfSauce, &mut Transform)>,
    mut fx: Option<ResMut<FxParticles>>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else { return };
    let Some(fx) = fx.as_deref_mut() else { return };
    let dt = time.delta_secs().min(0.1);
    let eye = anchor.eye;
    let tt = &active.world.cfg.tuning.transformer;
    for (f, mut s, mut tf) in &mut q {
        let Some(c) = active.world.constructs.get(f.id) else { continue };
        let attached = views.construct(f.id).is_some_and(|v| v.attached());
        let frozen = freeze.construct(f.id) || (attached && freeze.player(c.owner));
        let h = f.height;
        let centre = bv(c.center());
        let fwd = bv(c.forward());
        let fwd = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::NEG_Z);
        let row = tt.robot_row(c);
        let st = s.weapon.style();
        let mut j = Jit(f.id.0.wrapping_mul(0x9E37_79B9) ^ (time.elapsed_secs() * 211.0) as u32 | 1);
        let mut pen = Pen { fx: &mut *fx, motes: 1.0, scale: 1.0, cam: eye, avoid: None };
        s.fx_t += dt;
        // ---- Turret swivel (vehicle mode, a turret / chin gun following a fast aim change).
        s.swivel_cd = (s.swivel_cd - dt).max(0.0);
        if !f.robot && matches!(s.weapon, Weapon::Shell | Weapon::ChinGun) {
            if let Some(yaw) = views.player(c.owner).filter(|p| p.seat.is_some_and(|(sid, _)| sid == f.id)).map(|p| p.yaw) {
                if let Some(prev) = s.aim_yaw {
                    let dy = (yaw - prev + PI).rem_euclid(TAU) - PI;
                    if dy.abs() / dt.max(1e-3) > 1.6 && s.swivel_cd <= 0.0 {
                        s.swivel_cd = 0.45;
                        sfx.write(Sfx3d::at("tfs_turret_swivel", centre + Vec3::Y * c.half.y).pitch(f.pitch().clamp(0.7, 1.3)).gain(0.6));
                    }
                }
                s.aim_yaw = Some(yaw);
            }
        } else {
            s.aim_yaw = None;
        }
        // ---- GATHER: a heavy's hold (servo coil + motes and energy lines into the fist + hum).
        let b = &c.brawl;
        let want_coil = if f.robot && f.settled && !b.heavy_done && b.hold > 0.06 && row.heavy_hold > 0.0 { (b.hold / row.heavy_hold).clamp(0.0, 1.0) } else { 0.0 };
        s.coil += (want_coil - s.coil) * (dt * 18.0).min(1.0);
        if want_coil > 0.0 && !frozen {
            let fist = centre + Vec3::Y * h * 0.1 + fwd * h * 0.3 - fwd.cross(Vec3::Y) * s.side * h * 0.15;
            let lk = look(s.color, st.energy, 3.5 + 3.0 * want_coil, 0.8);
            if j.f() < 0.5 + want_coil {
                pen.implode(&lk, fist, h * 0.45, 0.16, 2.0);
            }
            if want_coil > 0.35 && j.f() < want_coil {
                let d = j.dir();
                let a = fist + d * h * j.range(0.5, 0.8);
                pen.beam(a, fist + d * h * 0.1, 0.03 * h.sqrt(), mix3(st.energy, [1.0; 3], 0.4), 6.0, 0.07);
            }
            pen.flash(fist, 0.12 * h * 0.35 * (0.4 + want_coil), st.energy, 3.0 + 6.0 * want_coil, 0.05);
            if !s.hum && want_coil > 0.15 {
                s.hum = true;
                sfx.write(Sfx3d::at("tfs_charge_hum", fist).pitch((f.pitch() * 0.9).clamp(0.6, 1.4)).gain(0.7));
            }
        } else if want_coil == 0.0 {
            s.hum = false;
        }
        // ---- GATHER: the blaster recharging glows at the muzzle.
        if f.robot && f.settled && b.blaster_cd > 0.0 && b.blaster_cd < 0.3 && !matches!(s.weapon, Weapon::Jaws | Weapon::Claw) && !frozen {
            let m = centre + Vec3::Y * (c.half.y * 0.35) + fwd * (c.half.x.max(c.half.z) + 0.5);
            let u = 1.0 - b.blaster_cd / 0.3;
            pen.flash(m, 0.15 * st.muzzle * u, st.energy, 3.0 + 5.0 * u, 0.04);
            if j.f() < 0.6 {
                let lk = look(s.color, st.energy, 4.0, 0.5);
                pen.implode(&lk, m, 0.6 + 0.2 * h * 0.2, 0.12, 1.0);
            }
        }
        // ---- TRANSFORM ATTACK phases: launch voice, the finisher's gather at the chest.
        if b.xatk != s.xatk {
            match b.xatk {
                XPhase::Burst => {
                    sfx.write(Sfx3d::at(st.xattack, centre).pitch(f.pitch().clamp(0.6, 1.4)).gain(0.85));
                    let lk = look(s.color, st.energy, 4.0, 1.0 + 0.2 * f.weight());
                    pen.ring(&lk, centre, fwd, 0.4, 14.0, 20.0, 0.25);
                }
                XPhase::Finish => {
                    let lk = look(s.color, st.energy, 6.0, 1.1);
                    pen.implode(&lk, centre + Vec3::Y * h * 0.15, h * 0.7, 0.2, 16.0);
                    sfx.write(Sfx3d::at("tfs_charge_hum", centre).pitch((f.pitch() * 1.1).clamp(0.6, 1.5)).gain(0.8));
                }
                XPhase::None => {}
            }
            s.xatk = b.xatk;
            s.skid_prev = None;
        }
        if b.xatk == XPhase::Burst && !frozen {
            let spd = f.vel.length();
            let a = tt.robot_row(c).xattack;
            // Skid scars / tyre burns under wheels and treads.
            if st.skid && spd > 5.0 {
                let right = fwd.cross(Vec3::Y).normalize_or(Vec3::X);
                let half_w = c.half.x.min(c.half.z).max(0.3) * 0.8;
                let rear = centre - fwd * c.half.x.max(c.half.z) * 0.6;
                if let Some(g) = ground_y(&active, rear, c.half.y + 2.0) {
                    let (l, r) = (Vec3::new(rear.x, g + 0.03, rear.z) - right * half_w, Vec3::new(rear.x, g + 0.03, rear.z) + right * half_w);
                    if let Some((pl, pr)) = s.skid_prev {
                        for (a0, b0) in [(pl, l), (pr, r)] {
                            let n = ((a0.distance(b0) / 0.22).ceil() as usize).clamp(1, 12);
                            for i in 0..n {
                                let p = a0.lerp(b0, (i as f32 + 0.5) / n as f32);
                                pen.put(Emit { flags: PFlags::SMOKE | PFlags::PLAIN, pos: p, vel: Vec3::ZERO, size: 0.2, color: [0.05, 0.045, 0.04], emissive: 0.0, life: 4.0, gravity: 0.0, ..default() });
                            }
                        }
                    }
                    s.skid_prev = Some((l, r));
                    if matches!(s.weapon, Weapon::Blaster | Weapon::Spin) && j.f() < 0.5 {
                        let smoke = Look { body: Body::Smoke, main: [0.55, 0.55, 0.56], dark: [0.25; 3], glow: 0.8, size: 0.7, ..default() };
                        pen.burst(&smoke, if j.f() < 0.5 { l } else { r } + Vec3::Y * 0.2, 2.0, 1.5, Some(Vec3::Y), 0.8, 1.2);
                    }
                }
            }
            match a {
                XAttack::RotorSweep => {
                    // The blade disc: a spinning ring of hot arcs.
                    let r = h * 0.75;
                    let ph = s.fx_t * 22.0;
                    for i in 0..6 {
                        let a0 = ph + i as f32 * TAU / 6.0;
                        let p0 = centre + Vec3::new(a0.cos(), 0.0, a0.sin()) * r;
                        let p1 = centre + Vec3::new((a0 + 0.5).cos(), 0.0, (a0 + 0.5).sin()) * r;
                        pen.beam(p0, p1, 0.06 * h.sqrt(), mix3(s.color, [1.0; 3], 0.3), 5.0, 0.04);
                    }
                }
                XAttack::Grind => {
                    let front = centre + fwd * (c.half.x.max(c.half.z) + 0.3);
                    let floor = ground_y(&active, front, 6.0).unwrap_or(front.y - 2.0);
                    sparks(&mut pen, &mut j, front, fwd + Vec3::Y * 0.6, 4, 12.0, floor);
                }
                XAttack::Ram | XAttack::DriftRam | XAttack::SpinOut if spd > 8.0 && j.f() < 0.35 => {
                    let front = centre + fwd * (c.half.x.max(c.half.z) + 0.2) - Vec3::Y * c.half.y * 0.6;
                    sparks(&mut pen, &mut j, front, -fwd + Vec3::Y * 0.4, 2, 8.0, front.y - 0.6);
                }
                XAttack::Stampede if s.fx_t > 0.18 => {
                    s.fx_t = 0.0;
                    if let Some(g) = ground_y(&active, centre, c.half.y + 2.0) {
                        let lk = look(s.color, st.energy, 2.0, 1.0);
                        pen.dust_wall(&lk, Vec3::new(centre.x, g, centre.z), c.half.x.max(c.half.z), 4.0, 8.0);
                    }
                }
                _ => {}
            }
        }
        // ---- POSE: lean / twist / stretch about the feet (held through the hit-stop).
        if frozen || (attached && f.local) {
            continue;
        }
        s.pose_u += dt;
        let (mut lean, mut twist, mut sy) = pose_at(s.pose, s.pose_u, s.side, s.pose_k);
        if s.pose_u > 1.5 {
            s.pose = Pose::None;
        }
        // Anticipation coil (a heavy's hold): lean back, twist away, squash; trembles near full.
        if s.coil > 0.01 {
            let tremble = if s.coil > 0.7 { (s.fx_t * 60.0).sin() * 0.8 * (s.coil - 0.7) / 0.3 } else { 0.0 };
            lean += -9.0 * s.coil + tremble;
            twist += -s.side * 22.0 * s.coil;
            sy *= 1.0 - 0.08 * s.coil;
        }
        // Re-chamber between string hits (the recovery anticipates the next hit).
        if s.pose == Pose::Strike && b.step > 0 && b.open > 0.0 && s.pose_u > 0.2 {
            let k = ((s.pose_u - 0.2) / 0.12).clamp(0.0, 1.0);
            lean += -3.0 * k;
            twist += -s.side * 9.0 * k;
        }
        if lean.abs() < 0.05 && twist.abs() < 0.05 && (sy - 1.0).abs() < 1e-3 {
            continue;
        }
        let d = if s.pose == Pose::None { fwd } else { s.pose_dir };
        let axis = Vec3::Y.cross(d).normalize_or(Vec3::X);
        let rot = Quat::from_axis_angle(Vec3::Y, twist.to_radians()) * Quat::from_axis_angle(axis, lean.to_radians());
        let feet = centre - Vec3::Y * c.half.y.max(0.2);
        let rel = tf.translation - feet;
        let sxz = 1.0 / sy.max(0.5).sqrt();
        let rel = rot * Vec3::new(rel.x * sxz, rel.y * sy, rel.z * sxz);
        tf.translation = feet + rel;
        tf.rotation = rot * tf.rotation;
        tf.scale *= Vec3::new(sxz, sy, sxz);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hitstop_tiers_and_asymmetry() {
        let fr = |s: f32| s / FRAME;
        // Light 3–6 frames, heavy 10–18, vehicle 15–22, finisher 20–25, over the whole strength range.
        for s in [0.5, 1.0, 1.3, 1.6, 2.0, 2.4, 3.0, 5.0, f32::NAN] {
            for (tier, lo, hi) in [(TfTier::Light, 3.0, 6.0), (TfTier::Heavy, 10.0, 18.0), (TfTier::Vehicle, 15.0, 22.0), (TfTier::Finisher, 20.0, 25.0)] {
                let (v, a) = tf_hitstop(tier, s);
                assert!(fr(v) >= lo - 1e-3 && fr(v) <= hi + 1e-3, "{tier:?} @ {s}: {} frames", fr(v));
                assert!((a / v - 0.75).abs() < 1e-4, "attacker 25 % shorter");
            }
        }
        // Monotone by tier at the same strength.
        let mut prev = 0.0;
        for tier in [TfTier::Light, TfTier::Heavy, TfTier::Vehicle, TfTier::Finisher] {
            let (v, _) = tf_hitstop(tier, 2.0);
            assert!(v > prev);
            prev = v;
        }
    }

    #[test]
    fn tiers_from_hits() {
        use HitKind::*;
        assert_eq!(tf_tier(Melee, 1.1, 8.0, false, false, false), TfTier::Light);
        assert_eq!(tf_tier(HeavyMelee, 1.6, 20.0, false, false, false), TfTier::Heavy, "string ender / heavy");
        assert_eq!(tf_tier(HeavyMelee, 2.0, 30.0, false, true, false), TfTier::Vehicle, "vehicle attack");
        assert_eq!(tf_tier(Melee, 2.0, 6.0, false, true, false), TfTier::Heavy, "rotor / grind tick");
        assert_eq!(tf_tier(HeavyMelee, 2.4, 30.0, false, false, true), TfTier::Finisher, "finisher");
        assert_eq!(tf_tier(Melee, 1.0, 10.0, true, false, false), TfTier::Finisher, "KO blow");
    }

    /// Every shipped archetype row has a weapon identity whose sounds exist; the headline rows read
    /// distinct (tank shells thump, jet cannons chatter, truck blade hums, dino jaws crunch).
    #[test]
    fn every_archetype_has_a_weapon_style() {
        let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/sfx/manifest.ron")).expect("manifest");
        let has = |id: &str| manifest.contains(&format!("\"{id}\": ("));
        for w in Weapon::ALL {
            let s = w.style();
            for id in [Some(s.shot), s.swing, s.connect, Some(s.xattack)].into_iter().flatten() {
                assert!(has(id), "{w:?}: sound {id} missing from assets/sfx/manifest.ron");
            }
        }
        for id in ["tfs_charge_hum", "tfs_air_tear", "tfs_finisher_drop", "tfs_missile_whoosh", "tfs_turret_swivel", "robot_finisher_crunch"] {
            assert!(has(id), "{id}");
        }
        let rows = bd_sim::constructs::brawl::TransformerTuning::default().archetypes;
        assert!(rows.len() >= 10, "the shipped archetype rows");
        let mut kinds: Vec<Weapon> = rows.iter().map(|r| Weapon::of(&r.fx, r.xattack)).collect();
        kinds.dedup();
        assert!(kinds.len() >= 6, "archetypes read as distinct weapons: {kinds:?}");
        assert_eq!(Weapon::of("exhaust", XAttack::Ram), Weapon::Blade);
        assert_eq!(Weapon::of("afterburner", XAttack::FlyBy), Weapon::Cannon);
        assert_eq!(Weapon::of("tread_dust", XAttack::Barrage), Weapon::Shell);
        assert_eq!(Weapon::of("dino", XAttack::Stampede), Weapon::Jaws);
        assert_eq!(Weapon::of("dino", XAttack::Ram), Weapon::Jaws, "the dino's beast alt keeps its jaws");
        assert_eq!(Weapon::of("beast", XAttack::Pounce), Weapon::Claw);
        assert_eq!(Weapon::of("rotor_wash", XAttack::RotorSweep), Weapon::ChinGun);
        let shots: Vec<&str> = [Weapon::Shell, Weapon::Cannon, Weapon::ChinGun, Weapon::Blaster].iter().map(|w| w.style().shot).collect();
        assert_eq!(shots, ["tfs_shell_thump", "tfs_cannon_chatter", "tfs_chin_gun", "tfs_blaster_zip"]);
        assert_eq!(Weapon::Blade.style().swing, Some("tfs_blade_hum"));
        assert_eq!(Weapon::Jaws.style().connect, Some("tfs_bite_crunch"));
    }

    #[test]
    fn poses_hold_then_settle() {
        for p in [Pose::Strike, Pose::Rise, Pose::Slam, Pose::Recoil] {
            let (l0, _, _) = pose_at(p, 0.0, 1.0, 1.5);
            assert!(l0.abs() > 5.0, "{p:?} reads big at release");
            let (l1, t1, s1) = pose_at(p, 2.0, 1.0, 1.5);
            assert!(l1.abs() < 1e-3 && t1.abs() < 1e-3 && (s1 - 1.0).abs() < 1e-3, "{p:?} settles");
        }
        // The follow-through overshoots past rest.
        assert!((0..60).any(|i| pose_at(Pose::Strike, 0.1 + i as f32 * 0.01, 1.0, 1.0).0 < -0.2));
    }
}
