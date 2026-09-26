//! Signature casts & spectacles (COMBAT.md §5j) — the payoff for yelling "DOMAIN EXPANSION".
//! **No freeze, no forced camera, no control taken away.** One generic path for every spectacle:
//! a `ConstructSpawned` that `bd_sim::signature::spectacle` flags (any signature cast, and anything
//! costing ≥ `tuning.signature.alert_cost` or ≥ `alert_scale` m: kaiju, no-limits moons).
//!
//! * **Cut-in DANGER alert** (`summon_ping::CutIn`, the shared picture-in-picture API): every player
//!   gets a live dramatic window of the construct erupting (low angle, orbiting), `Alert` style in the
//!   caster's corps colour with the technique name (the recipe name = the lexicon title) stamped under
//!   it, placed on the side of the screen the threat comes from. Severity escalates with distance:
//!   inside the danger radius (a domain's dome) → 1.0 + a red edge pulse while you stay inside
//!   (`ScreenFlash` vignette, comfort-gated); near → 0.75; far → 0.45. The caster sees their own (hype,
//!   top centre) plus a small FOV punch (`FovPunch`, comfort-gated).
//! * **Title letters**: a player's own typed / spoken phrase already erupts as shout words
//!   (present::shout) boosted by the construct's size; bots (who never type) get a world shout of
//!   the title here.
//! * **Screen grade**: an element-coloured tint (≤ 10 % alpha, eased, steady — never a flash) while
//!   the local player is inside a live domain.
//! * **Audio / music hooks**: [`SignatureCue`] messages (bd_audio / bd_music are owned elsewhere):
//!   `alert_danger`, `domain_open`, `domain_hum_loop` (start / stop), `domain_shatter`, `ult_charge`,
//!   `ult_fire`, `world_duck`, `music_stinger`.
//!
//! Dev: `BD_SIGNATURE_TEST="<secs>|me|<phrase>;<secs>|bot:<metres>@<yaw_deg>|<phrase>"` — cast through
//! the real console path (me) or make the first bot shout it from `metres` in front of the camera,
//! `yaw_deg` to the side (screenshots of the alert from outside / inside).

use super::constructs::ConstructEntities;
use super::players::PlayerBody;
use super::summon_ping::{CutIn, CutInFraming, CutInPlacement, CutInTarget};
use super::{PresentSet, bv, corps_rgb, gv, rgb_color};
use crate::camera::PlayerCamera;
use crate::juice::{FovPunch, ScreenFlash};
use crate::sim_bridge::{ActiveWorld, GameEvents, SimCommand, WorldViews};
use crate::state::AppState;
use bd_core::mechanics::{Element, Form, FormCategory};
use bd_core::{ConstructId, PlayerId};
use bd_sim::GameEvent;
use bd_sim::signature::{SignatureKind, Spectacle, spectacle};
use bevy::picking::Pickable;
use bevy::prelude::*;

// ------------------------------------------------------------------------------------ API

/// Audio / music hook (read by the audio + music layers; nothing here plays sound itself).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct SignatureCue {
    pub cue: Cue,
    /// World position of the source (the construct / caster).
    pub pos: Vec3,
    /// 0..1: how dangerous it is for the local player (inside the radius = 1).
    pub severity: f32,
    /// The local player cast it.
    pub mine: bool,
    pub element: Element,
    pub construct: ConstructId,
}

/// Wanted cue names (see [`Cue::name`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// Everyone: the alarm sting as the cut-in pops (louder / harsher with severity).
    AlertDanger,
    /// A domain's shell erupts (after the windup): boom + pressure.
    DomainOpen,
    /// Start / stop the domain drone loop (`on` = start), louder inside.
    DomainHum { on: bool },
    /// The shell shatters (broken or expired).
    DomainShatter,
    /// A beam / big summon charging up during its windup.
    UltCharge,
    /// The ultimate beam fires.
    UltFire,
    /// Duck the world mix ~400 ms under the eruption.
    WorldDuck,
    /// Beat-quantised music stinger.
    MusicStinger,
}

impl Cue {
    pub fn name(self) -> &'static str {
        match self {
            Cue::AlertDanger => "alert_danger",
            Cue::DomainOpen => "domain_open",
            Cue::DomainHum { .. } => "domain_hum_loop",
            Cue::DomainShatter => "domain_shatter",
            Cue::UltCharge => "ult_charge",
            Cue::UltFire => "ult_fire",
            Cue::WorldDuck => "world_duck",
            Cue::MusicStinger => "music_stinger",
        }
    }
}

// ------------------------------------------------------------------------------------ state

struct Pending {
    id: ConstructId,
    owner: PlayerId,
    sp: Spectacle,
    title: String,
    element: Element,
    form: Form,
    age: f32,
}

struct Live {
    id: ConstructId,
    owner: PlayerId,
    sp: Spectacle,
    element: Element,
    form: Form,
    age: f32,
    opened: bool,
    /// Seconds until the next "you're inside" pulse.
    pulse: f32,
}

#[derive(Resource, Default)]
struct Spectacles {
    pending: Vec<Pending>,
    live: Vec<Live>,
    tint: f32,
    tint_color: [f32; 3],
}

#[derive(Component)]
struct DomainTint;

/// Max tint alpha inside a domain (comfort: a steady grade, never a flash).
const TINT_MAX: f32 = 0.10;
/// Wait this long for the construct's entity before alerting on its position alone.
const ENTITY_WAIT: f32 = 0.3;

pub struct SignaturePlugin;

impl Plugin for SignaturePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SignatureCue>()
            .init_resource::<Spectacles>()
            .init_resource::<SigDev>()
            .add_systems(Startup, spawn_tint)
            .add_systems(Update, (spectacle_events.in_set(PresentSet::Events), (alert_spectacles, domain_state).chain().in_set(PresentSet::Sync)))
            .add_systems(Update, (dev_test, dev_world).chain().before(crate::tick::FrameSet::Input))
            .add_systems(OnExit(AppState::InGame), clear);
    }
}

fn clear(mut s: ResMut<Spectacles>, mut tint: Query<&mut BackgroundColor, With<DomainTint>>) {
    s.pending.clear();
    s.live.clear();
    s.tint = 0.0;
    for mut b in &mut tint {
        b.0 = Color::NONE;
    }
}

fn spawn_tint(mut commands: Commands) {
    commands.spawn((
        Name::new("DomainTint"),
        DomainTint,
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::NONE),
        GlobalZIndex(-20),
        Pickable::IGNORE,
    ));
}

/// Element → grade / cut-in accent (linear RGB); Light uses the caster's corps colour.
fn element_rgb(e: Element, corps: [f32; 3]) -> [f32; 3] {
    match e {
        Element::Light => corps,
        Element::Fire => [1.0, 0.32, 0.06],
        Element::Electric => [1.0, 0.92, 0.25],
        Element::Ice => [0.55, 0.85, 1.0],
        Element::Water => [0.15, 0.45, 1.0],
        Element::Wind => [0.65, 1.0, 0.8],
        Element::Earth => [0.55, 0.38, 0.18],
        Element::Poison => [0.4, 0.95, 0.15],
        Element::Psychic => [0.55, 0.15, 0.95],
        Element::Shadow => [0.06, 0.02, 0.12],
        Element::Atomic => [0.3, 1.0, 0.35],
    }
}

// ------------------------------------------------------------------------------------ systems

fn spectacle_events(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, mut s: ResMut<Spectacles>, mut cues: MessageWriter<SignatureCue>) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let t = &active.world.tuning().signature;
    let local = active.local;
    for ev in events.read() {
        match &ev.event {
            GameEvent::ConstructSpawned { id, owner, recipe, forming, cost, .. } => {
                if s.pending.iter().any(|p| p.id == *id) || s.live.iter().any(|l| l.id == *id) {
                    continue; // a late joiner's manifest / a client re-spawn
                }
                let Some(sp) = spectacle(recipe, *cost, *forming, t) else { continue };
                s.pending.push(Pending { id: *id, owner: *owner, sp, title: recipe.name.clone(), element: recipe.element, form: recipe.form, age: 0.0 });
            }
            GameEvent::ConstructGone { id, .. } => {
                if let Some(i) = s.live.iter().position(|l| l.id == *id) {
                    let l = s.live.swap_remove(i);
                    if l.form == Form::Dome {
                        let pos = active.world.constructs.get(*id).map_or(Vec3::ZERO, |c| bv(c.pos));
                        let mine = local == Some(l.owner);
                        for cue in [Cue::DomainHum { on: false }, Cue::DomainShatter] {
                            cues.write(SignatureCue { cue, pos, severity: 0.0, mine, element: l.element, construct: l.id });
                        }
                    }
                }
                s.pending.retain(|p| p.id != *id);
            }
            GameEvent::ShotFired { owner, construct: Some(c), muzzle, .. } => {
                if let Some(l) = s.live.iter().find(|l| l.id == *c && l.sp.kind == SignatureKind::Beam) {
                    cues.write(SignatureCue { cue: Cue::UltFire, pos: bv(*muzzle), severity: 0.8, mine: local == Some(*owner), element: l.element, construct: l.id });
                }
            }
            _ => {}
        }
    }
}

/// Where the viewer is relative to the danger: severity 0.45 (far) … 0.75 (near) … 1 (inside).
fn severity(eye: Vec3, center: Vec3, radius: f32) -> (f32, bool) {
    let d = Vec3::new(eye.x - center.x, 0.0, eye.z - center.z).length();
    let inside = d <= radius && (eye.y - center.y) < radius + 4.0;
    let s = if inside {
        1.0
    } else if d <= radius + 20.0 {
        0.75
    } else {
        0.45
    };
    (s, inside)
}

/// Frame centre on the side of the screen the threat comes from (0..1 screen units).
fn threat_side(cam: &GlobalTransform, center: Vec3) -> Vec2 {
    let to = center - cam.translation();
    let right = cam.right().as_vec3();
    let fwd = cam.forward().as_vec3();
    let x = right.dot(to);
    let z = fwd.dot(to).max(0.1);
    let side = (x / z).clamp(-1.5, 1.5) / 1.5;
    // Behind you: pinned hard to the side it's on.
    let side = if fwd.dot(to) < 0.0 { if x >= 0.0 { 1.0 } else { -1.0 } } else { side };
    Vec2::new(0.5 + 0.33 * side, 0.26)
}

#[allow(clippy::too_many_arguments)]
fn alert_spectacles(
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    map: Res<ConstructEntities>,
    bodies: Query<(Entity, &PlayerBody)>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut commands: Commands,
    mut s: ResMut<Spectacles>,
    mut cut: MessageWriter<CutIn>,
    mut cues: MessageWriter<SignatureCue>,
    mut fov: MessageWriter<FovPunch>,
) {
    if s.pending.is_empty() {
        return;
    }
    let Some(active) = active else { return };
    let dt = time.delta_secs();
    let Ok(cam) = cam.single() else { return };
    let eye = cam.translation();
    let s = &mut *s;
    let mut i = 0;
    while i < s.pending.len() {
        s.pending[i].age += dt;
        let p = &s.pending[i];
        let entity = map.0.get(&p.id).copied();
        if entity.is_none() && p.age < ENTITY_WAIT {
            i += 1;
            continue;
        }
        let p = s.pending.swap_remove(i);
        let mine = active.local == Some(p.owner);
        let corps = corps_rgb(active.corps_of(p.owner));
        let center = views.construct(p.id).map(|c| c.pos).or_else(|| views.player(p.owner).map(|v| v.pos)).unwrap_or(eye);
        let (sev, inside) = if mine { (0.6, false) } else { severity(eye, center, p.sp.radius) };
        // What the window shows: the construct erupting; a held ultimate (a beam in someone's hands)
        // frames its caster instead.
        let held = matches!(p.form.category(), FormCategory::Held | FormCategory::Worn);
        let body = bodies.iter().find(|(_, b)| b.id == p.owner).map(|(e, _)| e);
        let target = match (held, body, entity) {
            (true, Some(b), _) => Some(CutInTarget::Entity { entity: b, radius: 2.4 }),
            (_, _, Some(e)) => Some(CutInTarget::Entity { entity: e, radius: p.sp.radius.min(20.0) }),
            (_, Some(b), None) => Some(CutInTarget::Entity { entity: b, radius: 2.4 }),
            _ => None,
        };
        if let Some(target) = target {
            let label = p.title.to_uppercase();
            let placement = if mine || inside { CutInPlacement::Auto } else { CutInPlacement::Screen(threat_side(cam, center)) };
            let duration = (p.sp.windup + 1.4).clamp(1.8, 3.2);
            cut.write(
                CutIn::alert(target, corps, sev, Some(label))
                    .with_framing(CutInFraming::Dramatic { low_angle: true, orbit_speed: 0.6 })
                    .with_duration(duration)
                    .with_priority((sev * 10.0) as u8 + 3)
                    .with_placement(placement),
            );
        }
        let cue = |c: Cue| SignatureCue { cue: c, pos: center, severity: sev, mine, element: p.element, construct: p.id };
        cues.write(cue(Cue::AlertDanger));
        cues.write(cue(Cue::MusicStinger));
        if p.sp.kind == SignatureKind::Beam || p.form.category() == FormCategory::Creature {
            cues.write(cue(Cue::UltCharge));
        }
        if mine {
            // Hype for the caster: a small FOV punch (comfort-gated in juice).
            fov.write(FovPunch { degrees: 2.5 + 2.0 * (p.sp.radius / 20.0).min(1.0) });
        }
        // Bots never type: their title erupts as world shout words over them.
        if !mine && active.bots.iter().any(|b| b.id == p.owner) && !p.title.is_empty() {
            if let Some(v) = views.player(p.owner) {
                let hand = v.pos + Vec3::Y * 1.3;
                let mut sh = bd_fx::Shout::new(bd_fx::sanitize_shout(&p.title, bd_fx::SHOUT_MAX_CHARS), corps, 2.0, false, hand);
                sh.head = v.pos + Vec3::Y * 1.9;
                sh.toward = center;
                sh.target = entity;
                sh.boost = 2.2;
                sh.seed = p.id.0 as u64;
                commands.spawn((Name::new("SignatureTitle"), sh));
            }
        }
        s.live.push(Live { id: p.id, owner: p.owner, sp: p.sp, element: p.element, form: p.form, age: 0.0, opened: false, pulse: 0.0 });
    }
}

/// Domains: open boom after the windup, hum while alive, the inside grade + danger pulse.
#[allow(clippy::too_many_arguments)]
fn domain_state(
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    comfort: Res<crate::settings::ComfortSettings>,
    mut s: ResMut<Spectacles>,
    mut cues: MessageWriter<SignatureCue>,
    mut flash: MessageWriter<ScreenFlash>,
    mut tint: Query<&mut BackgroundColor, With<DomainTint>>,
) {
    let dt = time.delta_secs();
    let s = &mut *s;
    let me = active.as_ref().and_then(|a| a.local).and_then(|id| views.player(id)).filter(|v| v.alive());
    let mut want: Option<[f32; 3]> = None;
    if let Some(active) = active.as_ref() {
        for l in &mut s.live {
            l.age += dt;
            let Some(c) = views.construct(l.id) else { continue };
            let mine = active.local == Some(l.owner);
            if l.form != Form::Dome {
                continue;
            }
            let sev = me.map_or(0.0, |v| severity(v.pos + Vec3::Y * 1.6, c.pos, l.sp.radius).0);
            if !l.opened && l.age >= l.sp.windup {
                l.opened = true;
                for cue in [Cue::WorldDuck, Cue::DomainOpen, Cue::DomainHum { on: true }] {
                    cues.write(SignatureCue { cue, pos: c.pos, severity: sev, mine, element: l.element, construct: l.id });
                }
            }
            let inside = me.is_some_and(|v| severity(v.pos + Vec3::Y * 1.6, c.pos, l.sp.radius).1);
            if inside && l.opened {
                let corps = corps_rgb(active.corps_of(l.owner));
                want = Some(element_rgb(l.element, corps));
                // Someone else's domain around you: a slow red edge pulse (comfort: flashes gate it).
                l.pulse -= dt;
                if !mine && l.pulse <= 0.0 {
                    l.pulse = 1.1;
                    flash.write(ScreenFlash { color: Color::linear_rgb(1.0, 0.05, 0.02), intensity: 0.10, duration: 0.55, vignette: true });
                }
            }
        }
    }
    // The grade eases in / out; comfort "flashes off" halves it (it's steady, never flashing).
    let target = if want.is_some() { TINT_MAX * if comfort.flashes <= 0.0 { 0.5 } else { 1.0 } } else { 0.0 };
    if let Some(c) = want {
        s.tint_color = c;
    }
    let k = (dt / 0.35).min(1.0);
    let before = s.tint;
    s.tint += (target - s.tint) * k;
    if s.tint < 0.002 && target == 0.0 {
        s.tint = 0.0;
    }
    if s.tint != before || (s.tint == 0.0 && before != 0.0) {
        for mut b in &mut tint {
            let c = s.tint_color;
            b.0 = if s.tint > 0.0 { rgb_color(c).with_alpha(s.tint) } else { Color::NONE };
        }
    }
}

// ------------------------------------------------------------------------------------ dev

/// Dev plan state (`BD_SIGNATURE_TEST`).
#[derive(Resource, Default)]
struct SigDev {
    t: f32,
    next: usize,
    /// Bot pinned in front of the camera: (bot, spot, until).
    pin: Option<(PlayerId, Vec3, f32)>,
    /// Top the local player's willpower up before their test cast lands.
    boost_me: bool,
    boosted: Option<usize>,
    casts: Vec<(PlayerId, crate::sim_bridge::Cmd)>,
}

/// Dev: world side of the plan (pins, willpower, bot casts).
fn dev_world(active: Option<ResMut<ActiveWorld>>, mut dev: ResMut<SigDev>, mut cmds: MessageWriter<SimCommand>) {
    if dev.pin.is_none() && dev.casts.is_empty() && !dev.boost_me {
        return;
    }
    let Some(mut active) = active else { return };
    if std::mem::take(&mut dev.boost_me) {
        if let Some(p) = active.local.and_then(|me| active.world.player_mut(me)) {
            p.willpower = p.willpower.max(100.0);
        }
    }
    if let Some((bot, spot, until)) = dev.pin {
        if dev.t < until {
            if let Some(p) = active.world.player_mut(bot) {
                p.mv.pos = gv(spot);
                p.mv.vel = bd_core::Vec3::ZERO;
                p.willpower = p.willpower.max(100.0);
            }
        } else {
            dev.pin = None;
        }
    }
    for (bot, c) in dev.casts.drain(..) {
        cmds.write(SimCommand::remote(bot, c));
    }
}

#[allow(clippy::too_many_arguments)]
fn dev_test(
    time: Res<Time<Real>>,
    state: Res<State<AppState>>,
    active: Option<Res<ActiveWorld>>,
    lex: Option<Res<crate::console::RingLexicon>>,
    anchor: Res<crate::camera::ViewAnchor>,
    look: Res<crate::input::LookAngles>,
    mut caster: crate::console::Caster,
    mut dev: ResMut<SigDev>,
) {
    static PLAN: std::sync::OnceLock<Vec<(f32, String, String)>> = std::sync::OnceLock::new();
    let plan = PLAN.get_or_init(|| {
        std::env::var("BD_SIGNATURE_TEST")
            .unwrap_or_default()
            .split(';')
            .filter_map(|e| {
                let mut it = e.splitn(3, '|');
                let at: f32 = it.next()?.trim().parse().ok()?;
                let who = it.next()?.trim().to_string();
                let phrase = it.next()?.trim().to_string();
                (!phrase.is_empty()).then_some((at, who, phrase))
            })
            .collect()
    });
    if plan.is_empty() || *state.get() != AppState::InGame {
        return;
    }
    let (Some(active), Some(lex)) = (active, lex) else { return };
    dev.t += time.delta_secs();
    while let Some((at, who, phrase)) = plan.get(dev.next) {
        if dev.t < *at {
            break;
        }
        // "me": top up one frame, cast the next (so the cast lands on a full bar).
        if who == "me" && dev.boosted != Some(dev.next) {
            dev.boost_me = true;
            dev.boosted = Some(dev.next);
            break;
        }
        dev.next += 1;
        if who == "me" {
            info!("signature test: I cast {phrase:?}");
            caster.cast(phrase, false, 0);
            continue;
        }
        let spec = who.strip_prefix("bot:").unwrap_or("25");
        let mut parts = spec.split('@');
        let dist: f32 = parts.next().and_then(|d| d.parse().ok()).unwrap_or(25.0);
        let yaw: f32 = parts.next().and_then(|d| d.parse().ok()).unwrap_or(0.0);
        let Some(bot) = active.bots.first().map(|b| b.id) else { continue };
        let fwd = look.forward();
        let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::NEG_Z);
        let dir = Quat::from_rotation_y(yaw.to_radians()) * flat;
        let spot = anchor.eye + dir * dist - Vec3::Y * 1.6;
        dev.pin = Some((bot, spot, dev.t + 6.0));
        let res = lex.0.resolve(phrase);
        let eye = spot + Vec3::Y * 1.6;
        let aim = bd_sim::Aim::new(gv(eye), gv((anchor.eye - eye).normalize_or(Vec3::NEG_Z)));
        info!("signature test: bot {bot:?} casts {phrase:?} from {dist} m @ {yaw}°");
        let cmd = match res.signature.as_ref() {
            Some(s) => crate::console::signature_cmd(res.recipe.clone(), res.shape.clone(), aim, s, phrase, None),
            None => crate::sim_bridge::Cmd::Cast { recipe: Box::new(res.recipe), shape: res.shape, alt_shape: res.alt_shape, aim, incantation: false },
        };
        dev.casts.push((bot, cmd));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_escalates_inside() {
        let c = Vec3::ZERO;
        assert_eq!(severity(Vec3::new(5.0, 1.6, 0.0), c, 18.0), (1.0, true));
        assert_eq!(severity(Vec3::new(25.0, 1.6, 0.0), c, 18.0).0, 0.75);
        assert_eq!(severity(Vec3::new(80.0, 1.6, 0.0), c, 18.0).0, 0.45);
    }

    #[test]
    fn threat_side_follows_the_threat() {
        let cam = GlobalTransform::from(Transform::from_xyz(0.0, 0.0, 0.0).looking_to(Vec3::NEG_Z, Vec3::Y));
        assert!(threat_side(&cam, Vec3::new(20.0, 0.0, -10.0)).x > 0.6, "right");
        assert!(threat_side(&cam, Vec3::new(-20.0, 0.0, -10.0)).x < 0.4, "left");
        assert!(threat_side(&cam, Vec3::new(-5.0, 0.0, 30.0)).x < 0.2, "behind-left is hard left");
    }

    #[test]
    fn cue_names_are_the_wishlist() {
        let names: Vec<&str> = [Cue::AlertDanger, Cue::DomainOpen, Cue::DomainHum { on: true }, Cue::DomainShatter, Cue::UltCharge, Cue::UltFire, Cue::WorldDuck, Cue::MusicStinger]
            .iter()
            .map(|c| c.name())
            .collect();
        assert_eq!(names, ["alert_danger", "domain_open", "domain_hum_loop", "domain_shatter", "ult_charge", "ult_fire", "world_duck", "music_stinger"]);
    }
}
