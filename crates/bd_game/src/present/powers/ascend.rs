//! **ASCEND** — the ONE transformation moment (owner: "do we have animations that can be used
//! without tying them to specific powers?"). Every form / tier / power-up in `assets/powers.ron`
//! drives it with data (`AscendDef`: colour, intensity, title, stable, style, tints, linger, music,
//! quiet, toon); there is no per-power animation code. The sim announces it
//! (`PowerEvent::Ascend { power, tier, transform, awakening }`), so it looks the same everywhere.
//!
//! 1. **Windup** (0.45 s; 0.9 s for an AWAKENING — the first time this player takes this form this
//!    round): the player keeps control. Third person: the power-up pose layer
//!    ([`AscendPose::by_player`] → `AnimState::PowerUp`); first person: the charge clench
//!    (viewmodel reads [`AscendPose::local_windup`]).
//! 2. **Ignite**: a bd_fx [`ChargeAuraFx`] in the form's colour, scaled by `intensity` (Steam =
//!    whitened calm clouds, Calm = still ring, unstable = flicker + crackle).
//! 3. **Burst**: [`AuraBurstFx`] — shockwave ring, ground crack ring, pebbles blasted outward, a flash
//!    near the player only (comfort-gated), loose debris shoved ([`DebrisImpulse`]), a world duck.
//! 4. **Title letters** (`title`): big hard-light shout words over the player.
//! 5. **Cosmetic tints**: [`AscendTints`] — per player, the hair / eye / skin tint and toon flag of
//!    the forms they hold. THE COSTUME HOOK: the costume system reads this resource (nothing here
//!    edits costumes).
//! 6. **Linger**: a low aura for as long as the form is held (`linger: false` = none).
//!
//! Awakening extras: a longer, stronger sequence, a camera FOV flourish for the caster
//! (comfort-gated in `juice`, never taking control), a music stinger ([`PowerCue::Awakening`]:
//! `audio.rs` plays the streak stinger for the caster, a sting at the spot for everyone else). `quiet`: the world goes quiet for the caster (a beat of
//! desaturation + an audio duck; comfort-gated). `music` (Gear Fifth's "drums"): a
//! [`PowerCue::Music`] on / off while the local player holds that form (the audio side maps it).
//! `toon`: squash-and-stretch on the body.

use super::super::sound::Sfx3d;
use super::super::{PresentSet, corps_rgb};
use crate::camera::PlayerCamera;
use crate::debris::DebrisImpulse;
use crate::juice::{CameraShakeEvent, FovPunch, ScreenFlash};
use crate::present::players::PlayerBody;
use crate::present::signature::{Cue, SignatureCue};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use crate::tick::FrameSet;
use bd_core::PlayerId;
use bd_core::mechanics::Element;
use bd_fx::{AuraBurstFx, ChargeAuraFx, ConstructVisual};
use bd_sim::GameEvent;
use bd_sim::powers::{AscendDef, AscendStyle, PowerEvent, PowerSnap, PowerTable, power_flags};
use bd_sim::replicate::pflags;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;

/// Windup seconds (normal / awakening).
pub const WINDUP: f32 = 0.45;
pub const WINDUP_AWAKENING: f32 = 0.9;

/// Cosmetic tint hooks for the costume system: per player, what the forms they hold ask for
/// (sRGB 0..1). Updated every frame from the replicated power state; read-only for others.
#[derive(Resource, Default, Debug)]
pub struct AscendTints(pub HashMap<PlayerId, AscendTint>);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AscendTint {
    pub hair: Option<[f32; 3]>,
    pub eyes: Option<[f32; 3]>,
    pub skin: Option<[f32; 3]>,
    /// Toon physics (cartoon eyes hook, squash-and-stretch).
    pub toon: bool,
}

/// Audio / music hooks (bd_audio / bd_music map them; nothing here plays music itself).
#[derive(Message, Clone, Debug)]
pub enum PowerCue {
    /// The first time `player` took this form this round (the bigger Ascend): a stinger.
    Awakening { player: PlayerId, power: u16, tier: u8, local: bool, pos: Vec3 },
    /// A held form's music layer ("drums" — Gear Fifth) turned on / off for the local player.
    Music { layer: String, on: bool },
    /// The burst of any Ascend (sub thump).
    Burst { player: PlayerId, pos: Vec3, intensity: f32, local: bool },
}

/// Power-up pose (third person: seconds into the windup per player; first person: 0..1 weight).
#[derive(Resource, Default, Debug)]
pub struct AscendPose {
    pub by_player: HashMap<PlayerId, f32>,
    /// The local player's windup (seconds; 0 = none) — the viewmodel clench reads it.
    pub local_windup: f32,
}

struct Seq {
    player: PlayerId,
    entity: Entity,
    t: f32,
    windup: f32,
    color: [f32; 3],
    intensity: f32,
    stable: bool,
    calm: bool,
    awakening: bool,
    burst: bool,
    linger: bool,
    /// What keeps the linger alive: (power, tier, transform).
    form: Option<(u16, u8, bool)>,
    quiet: bool,
    k: f32,
    next_pulse: f32,
}

#[derive(Resource, Default)]
struct Seqs(Vec<Seq>);

/// "The world goes quiet" beat for the local caster (seconds left).
#[derive(Resource, Default)]
struct Quiet {
    t: f32,
    written: bool,
}

/// Last music layers the local player had on.
#[derive(Resource, Default)]
struct MusicState(Vec<String>);

pub struct AscendPlugin;

impl Plugin for AscendPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PowerCue>()
            .add_message::<SignatureCue>()
            .add_message::<AuraBurstFx>()
            .init_resource::<AscendTints>()
            .init_resource::<AscendPose>()
            .init_resource::<Seqs>()
            .init_resource::<Quiet>()
            .init_resource::<MusicState>()
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, ascend_events.in_set(PresentSet::Events))
            .add_systems(Update, (drive, body_glow, tints_and_music, quiet_beat).chain().in_set(FrameSet::Present).after(PresentSet::Sync));
    }
}

fn clear(mut commands: Commands, mut seqs: ResMut<Seqs>, mut tints: ResMut<AscendTints>, mut pose: ResMut<AscendPose>, mut music: ResMut<MusicState>, mut cues: MessageWriter<PowerCue>) {
    for s in seqs.0.drain(..) {
        commands.entity(s.entity).try_despawn();
    }
    tints.0.clear();
    pose.by_player.clear();
    pose.local_windup = 0.0;
    for layer in music.0.drain(..) {
        cues.write(PowerCue::Music { layer, on: false });
    }
}

/// sRGB 0..1 → linear.
pub fn lin(c: [f32; 3]) -> [f32; 3] {
    Color::srgb(c[0], c[1], c[2]).to_linear().to_f32_array_no_alpha()
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// The AscendDef an event means (`None` power = a plain power-up in corps colour).
pub fn ascend_def(power: Option<u16>, tier: u8, transform: bool) -> Option<&'static AscendDef> {
    let d = PowerTable::get().def(power?)?;
    if transform {
        return d.transform.as_ref().and_then(|t| t.ascend.as_ref());
    }
    d.ascend_at(tier)
}

/// Is `form` still held in this snapshot?
fn still_held(snap: &PowerSnap, form: (u16, u8, bool)) -> bool {
    let (power, tier, transform) = form;
    snap.held().any(|(id, t, x)| id == power && if transform { x } else { t == tier })
}

#[allow(clippy::too_many_arguments)]
fn ascend_events(
    mut commands: Commands,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    comfort: Res<ComfortSettings>,
    mut seqs: ResMut<Seqs>,
    mut sfx: MessageWriter<Sfx3d>,
    mut cues: MessageWriter<PowerCue>,
    mut fov: MessageWriter<FovPunch>,
) {
    if active.is_none() {
        events.clear();
        return;
    }
    for ev in events.read() {
        let GameEvent::Power(PowerEvent::Ascend { player, power, tier, transform, awakening }) = &ev.event else { continue };
        let Some(v) = views.player(*player) else { continue };
        let def = ascend_def(*power, *tier, *transform);
        let fallback = AscendDef::default();
        let a = def.unwrap_or(&fallback);
        let corps = corps_rgb(v.corps);
        let mut color = a.color.map(lin).unwrap_or(corps);
        if a.style == AscendStyle::Steam {
            color = mix(color, [1.0, 1.0, 1.0], 0.55);
        }
        let intensity = if a.intensity.is_finite() { a.intensity.clamp(0.3, 3.0) } else { 1.0 };
        let windup = if *awakening { WINDUP_AWAKENING } else { WINDUP };
        // One sequence per player: a new form replaces the running one.
        if let Some(k) = seqs.0.iter().position(|s| s.player == *player) {
            let old = seqs.0.swap_remove(k);
            commands.entity(old.entity).try_despawn();
        }
        let mut aura = ChargeAuraFx::new(color, player.0 as u32 * 7919 + 101);
        aura.calm = a.style != AscendStyle::Flame || !comfort_ok(&comfort);
        let entity = commands.spawn((Name::new(format!("Ascend{}", player.0)), aura, Transform::from_translation(v.pos))).id();
        let form = power.map(|p| (p, if *transform { 0 } else { *tier }, *transform));
        seqs.0.push(Seq {
            player: *player,
            entity,
            t: 0.0,
            windup,
            color,
            intensity,
            stable: a.stable,
            calm: a.style != AscendStyle::Flame,
            awakening: *awakening,
            burst: false,
            linger: a.linger && power.is_some(),
            form,
            quiet: a.quiet,
            k: 0.0,
            next_pulse: 0.0,
        });
        let pitch = (1.15 - 0.12 * intensity).clamp(0.6, 1.2);
        sfx.write(Sfx3d::at(if *awakening { "presence_gather_titan" } else { "presence_gather_big" }, v.pos + Vec3::Y).pitch(pitch).gain(0.8));
        // Title letters: the form's name erupts over the player.
        if let Some(title) = a.title.as_deref().filter(|t| !t.is_empty()) {
            let energy = if *awakening { 2.1 } else { 1.3 };
            let hand = v.pos + Vec3::Y * 1.3 * v.size;
            let mut sh = bd_fx::Shout::new(bd_fx::sanitize_shout(title, bd_fx::SHOUT_MAX_CHARS), color, energy, v.is_local, hand);
            sh.head = v.pos + Vec3::Y * 1.95 * v.size;
            sh.toward = v.pos + Vec3::Y * 1.0;
            sh.boost = if *awakening { 2.4 } else { 1.5 };
            sh.seed = (player.0 as u64) << 32 ^ ev.tick;
            commands.spawn((Name::new("AscendTitle"), sh));
        }
        if *awakening {
            if let Some(power) = power {
                cues.write(PowerCue::Awakening { player: *player, power: *power, tier: *tier, local: v.is_local, pos: v.pos });
            }
            if v.is_local {
                fov.write(FovPunch { degrees: 5.0 });
            }
        }
    }
}

fn comfort_ok(c: &ComfortSettings) -> bool {
    c.flashes > 0.01
}

#[allow(clippy::too_many_arguments)]
fn drive(
    mut commands: Commands,
    time: Res<Time<Real>>,
    views: Res<WorldViews>,
    comfort: Res<ComfortSettings>,
    mode: Res<super::super::camera3p::ViewMode>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    mut seqs: ResMut<Seqs>,
    mut pose: ResMut<AscendPose>,
    mut quiet: ResMut<Quiet>,
    mut q: Query<(&mut ChargeAuraFx, &mut Transform)>,
    out: (
        MessageWriter<AuraBurstFx>,
        MessageWriter<DebrisImpulse>,
        MessageWriter<SignatureCue>,
        MessageWriter<PowerCue>,
        MessageWriter<CameraShakeEvent>,
        MessageWriter<ScreenFlash>,
        MessageWriter<Sfx3d>,
    ),
) {
    let (mut burst, mut debris, mut sig, mut cues, mut shake, mut flash, mut sfx) = out;
    let dt = time.delta_secs().min(0.1);
    let cam = cams.iter().next().map(|g| g.translation()).unwrap_or(Vec3::ZERO);
    let third = mode.third_person();
    let flashes = comfort_ok(&comfort);
    pose.by_player.clear();
    pose.local_windup = 0.0;
    let now = time.elapsed_secs();
    seqs.0.retain_mut(|s| {
        s.t += dt;
        let Some(v) = views.player(s.player) else {
            commands.entity(s.entity).try_despawn();
            return false;
        };
        let held = s.form.is_some_and(|f| still_held(&v.power, f)) && v.alive();
        // Target strength by phase: windup ramps, burst peaks, linger holds low while the form lasts.
        let (target, rate) = if s.t < s.windup {
            let u = s.t / s.windup;
            (u * u * (3.0 - 2.0 * u), 12.0)
        } else if s.t < s.windup + 0.5 {
            (1.0, 12.0)
        } else if s.linger && held {
            (0.32, 3.0)
        } else {
            (0.0, 3.5)
        };
        s.k += (target - s.k) * (1.0 - (-rate * dt).exp());
        if s.t < s.windup {
            pose.by_player.insert(s.player, s.t);
            if v.is_local {
                pose.local_windup = s.t.max(1e-3);
            }
        }
        // Burst.
        if !s.burst && s.t >= s.windup {
            s.burst = true;
            let grounded = v.has(pflags::GROUNDED);
            let near = v.is_local || v.pos.distance(cam) < 22.0;
            let big = s.intensity * if s.awakening { 1.4 } else { 1.0 };
            burst.write(AuraBurstFx {
                pos: v.pos,
                color: s.color,
                height: 1.8 * v.size,
                scale: (0.7 + 0.45 * big).min(2.6),
                flash: flashes && near,
                broken: false,
                ground: grounded.then(|| (v.pos.y, [0.36, 0.3, 0.24])),
            });
            debris.write(DebrisImpulse { center: v.pos + Vec3::Y * 0.3, radius: 5.0 + 2.5 * big, strength: 7.0 + 4.0 * big });
            sig.write(SignatureCue { cue: Cue::WorldDuck, pos: v.pos, severity: 0.5, mine: v.is_local, element: Element::Light, construct: bd_core::ConstructId(u32::MAX) });
            cues.write(PowerCue::Burst { player: s.player, pos: v.pos, intensity: big, local: v.is_local });
            sfx.write(Sfx3d::at(if s.awakening { "presence_manifest_titan" } else { "presence_manifest_big" }, v.pos + Vec3::Y).pitch((1.1 - 0.1 * big).clamp(0.6, 1.2)));
            if near {
                shake.write(CameraShakeEvent { trauma: (0.1 + 0.06 * big).min(0.35), dir: Vec3::ZERO });
            }
            if v.is_local && s.awakening && flashes {
                flash.write(ScreenFlash { color: Color::linear_rgb(s.color[0], s.color[1], s.color[2]), intensity: 0.18, duration: 0.25, vignette: true });
            }
            if v.is_local && s.quiet {
                quiet.t = 0.9;
            }
        }
        let Ok((mut aura, mut tf)) = q.get_mut(s.entity) else { return false };
        tf.translation = v.pos;
        let flicker = if s.stable || !flashes { 1.0 } else { 0.78 + 0.22 * ((now * 31.0 + s.player.0 as f32).sin() * (now * 17.3).cos()).abs() };
        aura.intensity = (s.k * flicker).clamp(0.0, 1.0);
        aura.level = (0.25 + 0.35 * s.intensity * if s.awakening { 1.3 } else { 1.0 }).clamp(0.0, 1.0) * (0.55 + 0.45 * s.k);
        aura.height = 1.8 * v.size;
        aura.ground = v.has(pflags::GROUNDED).then_some(v.pos.y);
        aura.view_distance = cam.distance(v.pos);
        aura.eye = cam;
        aura.first_person = v.is_local && !third;
        aura.calm = s.calm || !flashes;
        // Shockwave pulses through the windup (faster for stronger forms), sparse in the linger.
        s.next_pulse -= dt;
        if s.next_pulse <= 0.0 && s.k > 0.2 {
            aura.pulse = true;
            s.next_pulse = if s.t < s.windup { 0.16 } else if s.stable { 1.2 } else { 0.45 };
        }
        let done = s.burst && s.k < 0.01 && !(s.linger && held);
        if done {
            commands.entity(s.entity).try_despawn();
        }
        !done
    });
}

/// Ascending bodies pop (brighter palette + sheen), toon forms squash and stretch.
fn body_glow(
    seqs: Res<Seqs>,
    tints: Res<AscendTints>,
    time: Res<Time<Real>>,
    views: Res<WorldViews>,
    mut bodies: Query<(&PlayerBody, &mut ConstructVisual, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    for (b, mut cv, mut tf) in &mut bodies {
        if let Some(s) = seqs.0.iter().find(|s| s.player == b.id) {
            let k = s.k.clamp(0.0, 1.0);
            if k > 0.01 {
                let boost = 1.0 + k * (0.35 + 0.25 * s.intensity.min(2.0));
                let c = cv.corps_color;
                cv.corps_color = [c[0] * boost, c[1] * boost, c[2] * boost];
                cv.flash = cv.flash.max(k * 0.06);
            }
        }
        // Toon physics: squash on landing / stretch when flying (bouncy, cartoonish).
        if tints.0.get(&b.id).is_some_and(|x| x.toon)
            && let Some(v) = views.player(b.id)
        {
            let vy = v.vel.y.clamp(-25.0, 25.0);
            let stretch = 1.0 + (vy.abs() / 25.0) * 0.28 + 0.04 * (t * 9.0 + b.id.0 as f32).sin();
            let s = tf.scale;
            tf.scale = Vec3::new(s.x / stretch.sqrt(), s.y * stretch, s.z / stretch.sqrt());
        }
    }
}

/// The costume tint hook + the local music layers.
fn tints_and_music(views: Res<WorldViews>, mut tints: ResMut<AscendTints>, mut music: ResMut<MusicState>, mut cues: MessageWriter<PowerCue>) {
    tints.0.clear();
    let mut layers: Vec<String> = Vec::new();
    for v in &views.players {
        if !v.alive() || v.power.id == 0 || v.power.has(power_flags::DRAINED) {
            continue;
        }
        let mut tint = AscendTint::default();
        let mut any = false;
        for (id, tier, transformed) in v.power.held() {
            let Some(d) = PowerTable::get().def(id) else { continue };
            let form = if transformed { d.transform.as_ref().and_then(|t| t.ascend.as_ref()) } else { None };
            // A base technique counts as a form (you ARE super saiyan); a fruit's base doesn't.
            let tiered = d.tier(tier).and_then(|t| t.ascend.as_ref());
            let base = if d.kind.is_technique() && tier == 0 { d.ascend.as_ref() } else { None };
            for a in [tiered, form, base].into_iter().flatten() {
                any = true;
                tint.hair = tint.hair.or(a.hair_tint);
                tint.eyes = tint.eyes.or(a.eye_glow);
                tint.skin = tint.skin.or(a.skin_tint);
                tint.toon |= a.toon;
                if v.is_local
                    && let Some(m) = a.music.as_ref()
                    && !layers.contains(m)
                {
                    layers.push(m.clone());
                }
            }
        }
        if any {
            tints.0.insert(v.id, tint);
        }
    }
    for l in &layers {
        if !music.0.contains(l) {
            cues.write(PowerCue::Music { layer: l.clone(), on: true });
        }
    }
    for l in &music.0 {
        if !layers.contains(l) {
            cues.write(PowerCue::Music { layer: l.clone(), on: false });
        }
    }
    music.0 = layers;
}

/// The Rinnegan / Ultra Instinct "world goes quiet" beat for the caster: saturation dips for
/// ~0.9 s (the caster's own body is boosted by `body_glow`). Comfort: off with flashes off.
fn quiet_beat(time: Res<Time<Real>>, comfort: Res<ComfortSettings>, mut quiet: ResMut<Quiet>, mut cams: Query<&mut ColorGrading, With<PlayerCamera>>) {
    if quiet.t <= 0.0 {
        if quiet.written {
            quiet.written = false;
            for mut cg in &mut cams {
                cg.global.post_saturation = 1.04;
            }
        }
        return;
    }
    quiet.t = (quiet.t - time.delta_secs()).max(0.0);
    if !comfort_ok(&comfort) {
        return;
    }
    let u = quiet.t / 0.9;
    let dip = (u * std::f32::consts::PI).sin();
    for mut cg in &mut cams {
        cg.global.post_saturation = 1.04 * (1.0 - 0.55 * dip);
    }
    quiet.written = true;
}
