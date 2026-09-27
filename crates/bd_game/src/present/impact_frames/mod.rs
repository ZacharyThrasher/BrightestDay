//! IMPACT FRAMES (docs/impact/API.md): the anime freeze-frame on the hits that matter. For 2–5
//! frames (scaled by the hit's weight) the picture goes high-contrast duotone in the move's colours
//! (inverted for Black Flash), ink speed lines rush to the impact, attacker + victim burn in as hot
//! silhouettes, the channels split; then it snaps back with a zoom punch.
//!
//! * API: write [`ImpactFrameRequest`] (WEAPON FEEL, anyone). The rules ([`rules`]) decide who sees
//!   it (attacker + victim: full screen; bystanders in view: a world-anchored bubble), the flash cap
//!   (≤ 3 per second, WCAG 2.3.1) and the Reduce-flashes pref.
//! * Triggers wired here, all data-driven (`assets/tuning/impact_frames.ron`): a move's juice
//!   `impact_frame` key, power crits (`CritFx` → style), KO blows, Signature-tier hits, perfect
//!   counters, charged heavy-weapon hits, beam-clash detonations, domain sure-hits.
//! * Render-side only: one post pass ([`post`]) that exists only while a frame plays, plus sounds,
//!   a word and a few world zaps. Never touches the sim. No per-frame allocation.

pub mod post;
pub mod reactions;
pub mod rules;
pub mod scenario;
pub mod tuning;

use super::impact::HitFreeze;
use super::sound::{MixDuck, Sfx3d};
use super::{bv, PresentSet};
use crate::camera::PlayerCamera;
use crate::juice::{FovPunch, HitStop};
use crate::settings::{ComfortSettings, FlashReduction};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use bd_core::PlayerId;
use bd_fx::{OnoWord, SpawnOnomatopoeia, ZapFx};
use bd_sim::combat::HitKind;
use bd_sim::powers::{CritFx, PowerEvent, PowerTable};
use bd_sim::GameEvent;
use bevy::prelude::*;
use post::ImpactFramePost;
pub use rules::ImpactTrigger;
use rules::{Ask, Cap, Decision, Reduce, View};
use std::borrow::Cow;
use tuning::{ImpactFrameTuningRes, ImpactStyle};

// ------------------------------------------------------------------------------------ API

/// Ask for an impact frame (docs/impact/API.md). Everything but `trigger` + `point` is optional.
#[derive(Message, Clone, Debug)]
pub struct ImpactFrameRequest {
    pub trigger: ImpactTrigger,
    /// World-space impact point.
    pub point: Vec3,
    pub attacker: Option<PlayerId>,
    pub victim: Option<PlayerId>,
    /// Hit weight 0..1(+): hold 2..5 frames, intensity.
    pub weight: f32,
    /// World hit direction (chromatic split skew); may be zero.
    pub dir: Vec3,
    /// Style key in `impact_frames.ron` (`None` = the trigger's default).
    pub style: Option<Cow<'static, str>>,
    /// sRGB accent override (the move's colour).
    pub tint: Option<[f32; 3]>,
}

impl ImpactFrameRequest {
    pub fn new(trigger: ImpactTrigger, point: Vec3) -> Self {
        Self { trigger, point, attacker: None, victim: None, weight: 0.6, dir: Vec3::ZERO, style: None, tint: None }
    }
    pub fn between(mut self, attacker: Option<PlayerId>, victim: Option<PlayerId>) -> Self {
        self.attacker = attacker;
        self.victim = victim;
        self
    }
    pub fn weight(mut self, w: f32) -> Self {
        self.weight = if w.is_finite() { w.clamp(0.0, 1.5) } else { 0.6 };
        self
    }
    pub fn dir(mut self, d: Vec3) -> Self {
        self.dir = d.normalize_or_zero();
        self
    }
    pub fn style(mut self, key: impl Into<Cow<'static, str>>) -> Self {
        self.style = Some(key.into());
        self
    }
    #[allow(dead_code)] // API (WEAPON FEEL / other presenters)
    pub fn tint(mut self, c: [f32; 3]) -> Self {
        self.tint = Some(c);
        self
    }
}

/// The frame playing now (read by the scenario / tests; `None` = idle).
#[derive(Resource, Default)]
pub struct ImpactFrameState {
    pub active: Option<ActiveFrame>,
    cap: Cap,
    /// Frames shown / dropped since start (dev stats).
    pub shown: u32,
    pub dropped: u32,
    /// Moves that armed an impact frame: (attacker, style key, until, tint).
    armed: [Option<(PlayerId, Cow<'static, str>, f64, Option<[f32; 3]>)>; 8],
    /// Recent hits by attacker (a move's hit is reported before the move itself in a tick):
    /// (attacker, victim, point, weight, dir, time).
    recent: [Option<(PlayerId, PlayerId, Vec3, f32, Vec3, f64)>; 16],
    recent_head: usize,
    /// Explicit requests this frame (attacker, victim) — the automatic heavy-weapon route skips them.
    explicit: [Option<(Option<PlayerId>, Option<PlayerId>, f64)>; 4],
}

#[derive(Clone, Debug)]
pub struct ActiveFrame {
    #[allow(dead_code)] // read by dev tools / tests
    pub trigger: ImpactTrigger,
    pub style_key: String,
    pub view: View,
    pub point: Vec3,
    pub attacker: Option<PlayerId>,
    pub victim: Option<PlayerId>,
    pub dir: Vec3,
    pub weight: f32,
    pub t: f32,
    pub hold: f32,
    pub release: f32,
    pub reduced: bool,
    pub seed: f32,
    accent: [f32; 3],
    released: bool,
}

pub struct ImpactFramesPlugin;

impl Plugin for ImpactFramesPlugin {
    fn build(&self, app: &mut App) {
        post::build(app);
        app.insert_resource(ImpactFrameTuningRes(tuning::load()))
            .init_resource::<ImpactFrameState>()
            .init_resource::<HitFreeze>()
            .init_resource::<MixDuck>()
            .add_message::<ImpactFrameRequest>()
            .add_message::<Sfx3d>()
            .add_message::<HitStop>()
            .add_message::<FovPunch>()
            .add_message::<ZapFx>()
            .add_message::<SpawnOnomatopoeia>()
            .add_systems(Update, tuning::hot_reload)
            .add_systems(Update, (triggers, drive).chain().in_set(PresentSet::Events).run_if(not(in_state(AppState::Loading))))
            .add_systems(Update, warm_up.run_if(in_state(AppState::Loading)))
            .add_systems(OnExit(AppState::InGame), clear);
        reactions::build(app);
        scenario::build(app);
    }
}

pub fn reduce_of(c: &ComfortSettings) -> Reduce {
    if c.flashes <= 0.01 {
        return Reduce::None;
    }
    match c.reduce_flashes {
        FlashReduction::Off => Reduce::Off,
        FlashReduction::Reduced => Reduce::Reduced,
        FlashReduction::NoFlashes => Reduce::None,
    }
}

fn clear(mut st: ResMut<ImpactFrameState>, mut cams: Query<&mut ImpactFramePost>) {
    st.active = None;
    st.armed = Default::default();
    st.recent = Default::default();
    for mut p in &mut cams {
        *p = ImpactFramePost::default();
    }
}

/// Behind the loading screen: a near-zero frame so the pipeline compiles before the first hit.
fn warm_up(mut commands: Commands, mut cams: Query<(Entity, Option<&mut ImpactFramePost>), With<PlayerCamera>>) {
    for (e, p) in &mut cams {
        let warm = ImpactFramePost { b: Vec4::new(1e-5, 0.0, 1.0, 1.0), ..default() };
        match p {
            Some(mut p) => *p = warm,
            None => {
                commands.entity(e).insert(warm);
            }
        }
    }
}

fn weight_of(strength: f32) -> f32 {
    (strength / 1.2).clamp(0.0, 1.5)
}

fn crit_name(fx: CritFx) -> &'static str {
    match fx {
        CritFx::Spark => "Spark",
        CritFx::BlackFlash => "BlackFlash",
    }
}

/// The juice `impact_frame` key (+ colour) of a move that just fired.
fn move_impact_key(views: &WorldViews, player: PlayerId, power: u16, slot: u8) -> Option<(Cow<'static, str>, Option<[f32; 3]>)> {
    let tier = views.player(player).and_then(|v| v.power.held().find(|h| h.0 == power).map(|h| h.1)).unwrap_or(0);
    let d = PowerTable::get().def(power)?;
    let m = d.moves_at(tier).get(slot as usize)?;
    let j = m.juice.as_ref()?;
    let key = j.impact_frame.as_deref().filter(|k| !k.is_empty())?;
    Some((Cow::Owned(key.to_string()), j.color))
}

/// Game events → requests (every trigger but explicit API calls).
#[allow(clippy::too_many_arguments)]
fn triggers(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    tuning: Res<ImpactFrameTuningRes>,
    presence: Option<Res<super::presence::PresenceIndex>>,
    mut st: ResMut<ImpactFrameState>,
    mut events: MessageReader<GameEvents>,
    mut out: MessageWriter<ImpactFrameRequest>,
) {
    let Some(_active) = active else {
        events.clear();
        return;
    };
    let now = time.elapsed_secs_f64();
    let r = &tuning.0.rules;
    let ts = &r.trigger_styles;
    for ev in events.read() {
        match &ev.event {
            GameEvent::Hit { target, source, construct, kind, element, knockback, point, lethal, impact: fx, .. } => {
                let Some(src) = *source else { continue };
                if src == *target {
                    continue;
                }
                let p = bv(*point);
                let dir = bv(fx.dir);
                let strength = fx.strength * presence.as_ref().map_or(1.0, |pi| pi.impact_mult(*construct));
                let w = weight_of(strength);
                // Remember it (a move's own hit arrives before its Move event).
                let h = st.recent_head;
                st.recent[h] = Some((src, *target, p, w, dir, now));
                st.recent_head = (h + 1) % st.recent.len();
                // An armed move's connect.
                if let Some(k) = st.armed.iter().position(|a| a.as_ref().is_some_and(|a| a.0 == src && a.2 >= now)) {
                    let (_, key, _, tint) = st.armed[k].take().unwrap();
                    let mut q = ImpactFrameRequest::new(ImpactTrigger::Move, p).between(Some(src), Some(*target)).weight(w.max(0.6)).dir(dir).style(key);
                    q.tint = tint;
                    out.write(q);
                }
                // KO blow: a launch that KOs.
                let kw = (knockback / 45.0).clamp(0.0, 1.5).max(w);
                if *lethal && kw >= r.ko_min_weight {
                    out.write(ImpactFrameRequest::new(ImpactTrigger::Ko, p).between(Some(src), Some(*target)).weight(kw.max(0.8)).dir(dir).style(ts.ko.clone()));
                }
                // Signature-tier construct hits (Hollow Purple landing).
                if let Some(c) = construct
                    && presence.as_ref().and_then(|pi| pi.tier(*c)) == Some(super::presence::tier::Tier::Signature)
                {
                    let style = r.element_styles.get(element).cloned().unwrap_or_else(|| ts.signature.clone());
                    out.write(ImpactFrameRequest::new(ImpactTrigger::Signature, p).between(Some(src), Some(*target)).weight(w.max(0.8)).dir(dir).style(style));
                }
                // Fully charged heavy-weapon hits (unless the weapon announced its own).
                if *kind == HitKind::HeavyMelee
                    && fx.strength >= r.heavy_melee_min_strength
                    && !st.explicit.iter().flatten().any(|e| e.0 == Some(src) && e.1 == Some(*target) && now - e.2 < 0.1)
                {
                    out.write(ImpactFrameRequest::new(ImpactTrigger::HeavyWeapon, p).between(Some(src), Some(*target)).weight(w).dir(dir).style(ts.heavy_weapon.clone()));
                }
            }
            GameEvent::Power(PowerEvent::Crit { attacker, target, point, fx, mult }) => {
                let style = r.crit_styles.get(crit_name(*fx)).cloned().unwrap_or_else(|| ts.crit.clone());
                let w = ((mult - 1.0) / 2.0).clamp(0.4, 1.2);
                let dir = views.player(*target).zip(views.player(*attacker)).map_or(Vec3::ZERO, |(t, a)| t.pos - a.pos);
                out.write(ImpactFrameRequest::new(ImpactTrigger::Crit, bv(*point)).between(Some(*attacker), Some(*target)).weight(w).dir(dir).style(style));
            }
            GameEvent::Power(PowerEvent::Move { player, power, slot }) => {
                let Some((key, tint)) = move_impact_key(&views, *player, *power, *slot) else { continue };
                // Already connected this tick? Fire now; else arm for the move's window.
                let hit = st.recent.iter().flatten().filter(|h| h.0 == *player && now - h.5 < 0.15).last().copied();
                if let Some((a, v, p, w, dir, _)) = hit {
                    let mut q = ImpactFrameRequest::new(ImpactTrigger::Move, p).between(Some(a), Some(v)).weight(w.max(0.6)).dir(dir).style(key);
                    q.tint = tint;
                    out.write(q);
                } else {
                    let slot = st.armed.iter().position(|a| a.as_ref().is_none_or(|a| a.2 < now || a.0 == *player)).unwrap_or(0);
                    st.armed[slot] = Some((*player, key, now + r.move_window as f64, tint));
                }
            }
            GameEvent::Power(PowerEvent::Op(bd_sim::powers::ops::OpEvent::Parried { player, kind: bd_sim::powers::ops::StanceKind::Counter, attacker, pos, dir })) => {
                out.write(ImpactFrameRequest::new(ImpactTrigger::Counter, bv(*pos)).between(Some(*player), *attacker).weight(0.7).dir(bv(*dir)).style(ts.counter.clone()));
            }
            GameEvent::PalmBeam(b) => {
                if let bd_sim::signature::palm::BeamPhase::Detonate { point, radius } = b.phase {
                    // Both casters are involved; the owner reported here is one of them.
                    out.write(ImpactFrameRequest::new(ImpactTrigger::BeamClash, bv(point)).between(Some(b.owner), None).weight((radius / 6.0).clamp(0.6, 1.2)).style(ts.beam_clash.clone()));
                }
            }
            GameEvent::Domain(bd_sim::signature::domain::DomainEvent::Strike { id, pos, victim: Some(v), .. }) => {
                let owner = views.construct(*id).map(|c| c.owner);
                out.write(ImpactFrameRequest::new(ImpactTrigger::DomainSureHit, bv(*pos)).between(owner, Some(*v)).weight(0.8).style(ts.domain.clone()));
            }
            _ => {}
        }
    }
}

/// Requests → the frame on screen (+ the frame's hit-stop, silhouettes, sounds, word, zaps).
#[allow(clippy::too_many_arguments)]
fn drive(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    tuning: Res<ImpactFrameTuningRes>,
    comfort: Res<ComfortSettings>,
    mut st: ResMut<ImpactFrameState>,
    mut freeze: ResMut<HitFreeze>,
    mut duck: ResMut<MixDuck>,
    mut requests: MessageReader<ImpactFrameRequest>,
    mut commands: Commands,
    mut cams: Query<(Entity, &Camera, &GlobalTransform, &Projection, Option<&mut ImpactFramePost>), With<PlayerCamera>>,
    out: (MessageWriter<Sfx3d>, MessageWriter<HitStop>, MessageWriter<FovPunch>, MessageWriter<ZapFx>, MessageWriter<SpawnOnomatopoeia>),
) {
    let (mut sfx, mut hitstop, mut fov, mut zap, mut ono) = out;
    let now = time.elapsed_secs_f64();
    let dt = time.delta_secs().min(0.1);
    let t = &tuning.0;
    let r = &t.rules;
    let reduce = reduce_of(&comfort);
    let local = active.as_ref().and_then(|a| a.local);
    let Ok((cam_e, camera, cam_gt, proj, post)) = cams.single_mut() else {
        requests.clear();
        return;
    };
    let eye = cam_gt.translation();
    let to_view = cam_gt.affine().inverse();
    for q in requests.read() {
        // Explicit heavy-weapon calls are remembered so the automatic route skips the same hit.
        if q.trigger == ImpactTrigger::HeavyWeapon && q.style.is_some() {
            let k = st.explicit.iter().position(|e| e.is_none_or(|e| now - e.2 > 0.1)).unwrap_or(0);
            st.explicit[k] = Some((q.attacker, q.victim, now));
        }
        let vz = to_view.transform_point3(q.point).z;
        let visible = vz < -0.2 && camera.world_to_ndc(cam_gt, q.point).is_some_and(|n| n.x.abs() < 1.1 && n.y.abs() < 1.1);
        let ask = Ask { now, trigger: q.trigger, attacker: q.attacker, victim: q.victim, local, dist: eye.distance(q.point), visible, reduce };
        let view = match rules::decide(r, &st.cap, &ask) {
            Decision::Play(v) => v,
            Decision::Drop(why) => {
                st.dropped += 1;
                // Reduce flashes = None: the non-visual beats (hit-stop, sounds) still land.
                if why.starts_with("reduce") && (ask.local == q.attacker || ask.local == q.victim) && local.is_some() {
                    let fallback = t.trigger_default(q.trigger);
                    let (_, style) = t.style(q.style.as_deref().unwrap_or(""), fallback);
                    if let Some(s) = style {
                        play_sfx(&mut sfx, &s.sfx_in, None);
                        if s.hitstop > 0.0 {
                            hitstop.write(HitStop { secs: s.hitstop });
                        }
                    }
                }
                continue;
            }
        };
        let fallback = t.trigger_default(q.trigger);
        let (key, style) = t.style(q.style.as_deref().unwrap_or(""), fallback);
        let default_style;
        let s = match style {
            Some(s) => s,
            None => {
                default_style = ImpactStyle::default();
                &default_style
            }
        };
        let reduced = reduce == Reduce::Reduced;
        let hold = rules::hold_secs(r, q.weight, s.hold_mult);
        let release = if reduced { r.reduced_release_secs } else { r.release_secs };
        st.cap.record(now, now + (hold + release) as f64, q.attacker, q.trigger.priority());
        st.shown += 1;
        let seed = ((now * 7.31).fract() as f32) * 0.97 + 0.013;
        let accent = q.tint.map(srgb).unwrap_or(srgb(s.accent));
        let involved = view == View::Full;
        // The beat: sounds (the suck during the hold), hit-stop, the duck, a word, world zaps.
        play_sfx(&mut sfx, &s.sfx_in, if involved { None } else { Some(q.point) });
        if involved {
            if s.hitstop > 0.0 {
                hitstop.write(HitStop { secs: s.hitstop });
            }
            duck.duck(hold + 0.15);
        }
        if let Some(w) = s.word.as_ref().filter(|w| !w.is_empty()) {
            let mut o = SpawnOnomatopoeia::key("hit_heavy", q.point + Vec3::Y * 0.6, 1.3 + q.weight * 0.6, accent);
            o.word = OnoWord::Key(Cow::Owned(w.clone()));
            o.dir = q.dir;
            o.local_player = involved;
            // A finisher frame's word: top priority in the world-text budget.
            o.accent = true;
            o.event_id = (seed * 1e6) as u64;
            ono.write(o);
        }
        for k in 0..s.zaps.min(12) {
            let a = k as f32 * 2.399 + seed * 6.28;
            let d = Vec3::new(a.cos(), ((k * 37 % 11) as f32 / 11.0 - 0.4) * 1.3, a.sin()).normalize_or(Vec3::Y);
            let col = if k % 2 == 0 { accent } else { srgb(s.ink) };
            zap.write(ZapFx { from: q.point, to: q.point + d * (1.4 + 0.35 * k as f32), color: col, duration: hold + 0.12 });
        }
        st.active = Some(ActiveFrame {
            trigger: q.trigger,
            style_key: key.to_string(),
            view,
            point: q.point,
            attacker: q.attacker,
            victim: q.victim,
            dir: q.dir,
            weight: q.weight,
            t: 0.0,
            hold,
            release,
            reduced,
            seed,
            accent,
            released: false,
        });
    }
    // Advance the frame and write the uniform.
    let mut post_value = ImpactFramePost::default();
    let mut finished = false;
    if let Some(f) = st.active.as_mut() {
        let (_, style) = t.style(&f.style_key, "ink");
        let default_style;
        let s = match style {
            Some(s) => s,
            None => {
                default_style = ImpactStyle::default();
                &default_style
            }
        };
        let involved = f.view == View::Full && local.is_some();
        // Hold the pair and burn them in (flash → hot emissive keyed by the shader).
        // `BD_IF_BENCH=1`: pin the first frame at its hold forever (GPU cost measurement).
        if bench() {
            f.t = f.t.min(f.hold * 0.5);
        }
        if f.t < f.hold {
            let left = f.hold - f.t + 0.02;
            // The attacker's own first-person camera keeps flying (a lunge): holding the victim's
            // body there too would park the camera inside it, so from the attacker's eye the
            // victim keeps the ordinary connect hit-stop only.
            let attacker_eye = local.is_some() && local == f.attacker;
            for id in [f.attacker, f.victim].into_iter().flatten() {
                if !(attacker_eye && Some(id) == f.victim) {
                    freeze.hold_player(id, left + s.hitstop * 0.5);
                }
                // Reduced: no white body flash (the silhouettes aren't burned in).
                if !f.reduced {
                    freeze.set_flash(id, 1.0, left);
                }
            }
        } else if !f.released {
            f.released = true;
            play_sfx(&mut sfx, &s.sfx_out, if involved { None } else { Some(f.point) });
            if involved && r.release_fov != 0.0 {
                fov.write(FovPunch { degrees: r.release_fov * (0.6 + 0.4 * f.weight.min(1.0)) });
            }
        }
        let (env, zenv) = rules::envelope(f.t, f.hold, f.release, f.reduced, r.reduced_fade_in);
        f.t += dt;
        if f.t > f.hold + f.release {
            finished = true;
        } else {
            post_value = uniform(f, s, r, env, zenv, &comfort, camera, cam_gt, proj, &to_view);
        }
    }
    if finished {
        st.active = None;
    }
    if post_value.active() && debug_log() {
        info!("IF drive {cam_e:?}: strength {:.3} zoom {:.3} center {:?} r {:.3}", post_value.b.x, post_value.b.z, post_value.a.truncate(), post_value.a.z);
    }
    match post {
        Some(mut p) => {
            if *p != post_value {
                *p = post_value;
            }
        }
        None => {
            commands.entity(cam_e).insert(post_value);
        }
    }
}

fn bench() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BD_IF_BENCH").is_some())
}

fn debug_log() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BD_IF_DEBUG").is_some())
}

fn play_sfx(sfx: &mut MessageWriter<Sfx3d>, list: &[(String, f32, f32)], at: Option<Vec3>) {
    for (name, gain, pitch) in list {
        let s = match at {
            Some(p) => Sfx3d::at(name.clone(), p),
            None => Sfx3d::ui(name.clone()).exempt(),
        };
        sfx.write(s.gain(*gain).pitch(*pitch));
    }
}

fn srgb(c: [f32; 3]) -> [f32; 3] {
    Color::srgb(c[0], c[1], c[2]).to_linear().to_f32_array_no_alpha()
}

#[allow(clippy::too_many_arguments)]
fn uniform(
    f: &ActiveFrame,
    s: &ImpactStyle,
    r: &tuning::ImpactRules,
    env: f32,
    zenv: f32,
    comfort: &ComfortSettings,
    camera: &Camera,
    cam_gt: &GlobalTransform,
    proj: &Projection,
    to_view: &bevy::math::Affine3A,
) -> ImpactFramePost {
    let aspect = camera.logical_viewport_size().map_or(16.0 / 9.0, |s| s.x / s.y.max(1.0));
    let vz = to_view.transform_point3(f.point).z;
    let ndc = if vz < -0.2 { camera.world_to_ndc(cam_gt, f.point) } else { None };
    let (center, on_screen) = match ndc {
        Some(n) if n.x.abs() < 1.2 && n.y.abs() < 1.2 => (Vec2::new(n.x * 0.5 + 0.5, 0.5 - n.y * 0.5), true),
        _ => (Vec2::splat(0.5), false),
    };
    let bubble = f.view == View::Bubble;
    let vfov = match proj {
        Projection::Perspective(p) => p.fov,
        _ => 1.2,
    };
    let radius = if bubble && on_screen { (r.bubble_radius / ((-vz).max(0.5) * 2.0 * (vfov * 0.5).tan())).clamp(0.03, 0.6) } else { 0.0 };
    if bubble && !on_screen {
        return ImpactFramePost::default();
    }
    let flashes = comfort.flashes.clamp(0.0, 1.0);
    let mut strength = env * flashes * if bubble { r.bubble_strength } else { 1.0 };
    let mut invert = s.invert as u8 as f32;
    let mut chroma = s.chroma;
    let mut contrast = 1.0;
    let mut glitch = s.glitch;
    let mut lightning = s.lightning;
    let mut zoom = s.zoom;
    if f.reduced {
        strength *= r.reduced_strength;
        invert = 0.0;
        chroma = 0.0;
        contrast = r.reduced_contrast;
        glitch = 0.0;
        lightning *= 0.5;
        zoom *= 0.5;
    }
    // Screen-space hit direction (for the split skew).
    let sd = to_view.transform_vector3(f.dir);
    let sdir = Vec2::new(sd.x, -sd.y).normalize_or_zero();
    let ink = srgb(s.ink);
    let paper = srgb(s.paper);
    let life = (f.hold + f.release).max(1e-3);
    ImpactFramePost {
        a: Vec4::new(center.x, center.y, radius, 0.35),
        b: Vec4::new(strength.max(0.0), if f.t < f.hold { f.t / f.hold.max(1e-3) } else { 1.0 + (f.t - f.hold) / f.release.max(1e-3) }, 1.0 + zoom * zenv, aspect),
        ink: Vec4::new(ink[0], ink[1], ink[2], invert),
        paper: Vec4::new(paper[0], paper[1], paper[2], s.threshold.clamp(0.02, 0.98)),
        accent: Vec4::new(f.accent[0], f.accent[1], f.accent[2], chroma.clamp(0.0, 0.05)),
        fx1: Vec4::new(s.lines.clamp(0.0, 1.0), s.line_density.clamp(0.0, 1.0), lightning.clamp(0.0, 1.0), s.crack.clamp(0.0, 1.0)),
        fx2: Vec4::new(s.halftone.clamp(0.0, 1.0), glitch.clamp(0.0, 1.0), s.brush.clamp(0.0, 1.0), f.seed),
        fx3: Vec4::new(sdir.x, sdir.y, r.hot_luma.max(0.5), s.lightning_ink_core as u8 as f32),
        fx4: Vec4::new(s.vignette.clamp(0.0, 1.0), if bubble { (f.t / life).clamp(0.0, 1.0) } else { -1.0 }, s.exposure.clamp(0.2, 3.0), contrast),
        fx5: Vec4::new(s.levels.clamp(2.0, 6.0), s.softness.clamp(0.0, 0.3), s.edges.clamp(0.0, 1.0), s.panel.clamp(0.0, 1.0)),
    }
}

impl tuning::ImpactFrameTuning {
    /// The default style key of a trigger.
    pub fn trigger_default(&self, trigger: ImpactTrigger) -> &str {
        let ts = &self.rules.trigger_styles;
        match trigger {
            ImpactTrigger::Move => &ts.r#move,
            ImpactTrigger::Crit => &ts.crit,
            ImpactTrigger::Ko => &ts.ko,
            ImpactTrigger::Signature => &ts.signature,
            ImpactTrigger::Counter => &ts.counter,
            ImpactTrigger::HeavyWeapon => &ts.heavy_weapon,
            ImpactTrigger::BeamClash => &ts.beam_clash,
            ImpactTrigger::DomainSureHit => &ts.domain,
            ImpactTrigger::Custom => &ts.custom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tuning::{parse, ImpactFrameTuning};

    fn shipped() -> ImpactFrameTuning {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/impact_frames.ron");
        let text = std::fs::read_to_string(path).expect("assets/tuning/impact_frames.ron");
        parse(&text).expect("impact_frames.ron parses")
    }

    #[test]
    fn the_shipped_table_has_every_style_and_trigger_default() {
        let t = shipped();
        for k in ["black_flash", "holy", "void", "ink", "comic", "glitch"] {
            assert!(t.styles.contains_key(k), "missing style {k}");
        }
        for trig in [
            super::ImpactTrigger::Move,
            super::ImpactTrigger::Crit,
            super::ImpactTrigger::Ko,
            super::ImpactTrigger::Signature,
            super::ImpactTrigger::Counter,
            super::ImpactTrigger::HeavyWeapon,
            super::ImpactTrigger::BeamClash,
            super::ImpactTrigger::DomainSureHit,
            super::ImpactTrigger::Custom,
        ] {
            let k = t.trigger_default(trig);
            assert!(t.styles.contains_key(k), "{trig:?} → unknown style {k}");
        }
        for k in t.rules.crit_styles.values().chain(t.rules.element_styles.values()) {
            assert!(t.styles.contains_key(k.as_str()), "unknown style {k}");
        }
        assert_eq!(t.rules.crit_styles.get("BlackFlash").map(String::as_str), Some("black_flash"));
        assert!(t.rules.max_per_sec <= 3, "WCAG 2.3.1: ≤ 3 flashes per second");
        assert!(t.styles["black_flash"].invert, "Black Flash is the inverted one");
    }

    #[test]
    fn every_impact_frame_key_in_the_powers_data_exists() {
        let t = shipped();
        let table = bd_sim::powers::PowerTable::get();
        let mut n = 0;
        for d in &table.powers {
            for tier in 0..=d.tiers.len() as u8 {
                for m in d.moves_at(tier) {
                    if let Some(k) = m.juice.as_ref().and_then(|j| j.impact_frame.as_deref()) {
                        assert!(t.styles.contains_key(k), "{} / {}: unknown impact_frame style {k}", d.id, m.name);
                        n += 1;
                    }
                }
            }
        }
        assert!(n >= 1, "black flash uses one");
    }
}
