//! Shout callouts (game side): when a player makes a construct, their words burst out as big
//! dynamic hard-light letters (bd_fx [`bd_fx::Shout`]) that stream into the construct as it forms.
//! Owner: "when they yell they get big dynamic juicy words announcing their construct".
//!
//! * **Local**: [`ShoutCast`] (written by `console::Caster::cast` for typed, quick-cast and voice
//!   phrases) → sanitise + slur filter → a local shout (lower-right, never over the crosshair) and a
//!   [`ShoutLine`] to the network. Voice loudness = the peak mic level of the push-to-talk utterance
//!   (tracked here from [`VoiceUi::level`]).
//! * **Remote**: NET relays [`ShoutLine`]s (`C2S::Shout` → host → `S2C::Shout`, ≤ 48 chars + 1 byte).
//!   Every receiver re-sanitises and re-filters (slurs / hate speech never render; ordinary profanity
//!   is allowed) and shows a camera-facing world shout above the caster.
//! * **Binding**: the next construct entity of that caster (practice / host: the real one; client:
//!   the provisional one) becomes the shout's target → the letters' voxels stream into it and its size
//!   boosts the shout (a MOON shakes the word). A failed cast (`CastFailed`) fizzles it.
//! * **Settings**: `UiPrefs::shout` (On / Mine only / Off); comfort (shake or flashes off) → gentle.
//! * **Audio hook**: bd_fx writes [`bd_fx::ShoutSfx`] (erupt / slam / release).
//! * **Dev**: `BD_SHOUT_DEMO="<secs>|<flags>|<text>;…"` fires scripted casts in practice (flags:
//!   `v<level>` = spoken at that mic level, `r<metres>` = a bot shouts it from that distance in front
//!   of the camera). `BD_SHOUT_DEMO=spam` = a shout every 0.35 s (perf check); `BD_PERF_OFF=shout`
//!   hides every shout (A/B).

use super::PresentSet;
use super::constructs::{SimConstruct, hand_transform};
use super::corps_rgb;
use crate::camera::{PlayerCamera, ViewAnchor};
use crate::console::{Caster, RingLexicon};
use crate::input::LookAngles;
use crate::juice::CameraShakeEvent;
use crate::lobby::prefs::{ShoutPref, UiPrefs};
use crate::net::NetState;
use crate::net::proto::{C2S, S2C};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, Cmd, GameEvents, SimCommand, WorldViews};
use crate::voice_ui::VoiceUi;
use bd_core::PlayerId;
use bd_fx::{ConstructVisual, SHOUT_MAX_CHARS, Shout, ShoutSettings, ShoutTable, sanitize_shout, size_boost, typed_energy, voice_energy};
use bd_sim::GameEvent;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------------------------ API

/// A phrase left the local ring (typed, quick-cast or spoken). Written by `console::Caster::cast`.
#[derive(Message, Clone, Debug)]
pub struct ShoutCast {
    pub text: String,
    pub spoken: bool,
}

/// A shout on the wire: sanitised text + how loud it was spoken.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShoutLine {
    pub text: String,
    /// 0 = typed; 1..=255 = spoken, peak mic level (1 ≙ 0.0, 255 ≙ 1.0).
    pub voice: u8,
}

impl ShoutLine {
    pub fn new(text: &str, spoken: Option<f32>) -> Self {
        let voice = spoken.map_or(0, |l| 1 + (if l.is_finite() { l.clamp(0.0, 1.0) } else { 0.0 } * 254.0).round() as u8);
        ShoutLine { text: sanitize_shout(text, SHOUT_MAX_CHARS), voice }
    }
    /// Untrusted → safe (length cap, font characters only).
    pub fn sanitized(self) -> Self {
        ShoutLine { text: sanitize_shout(&self.text, SHOUT_MAX_CHARS), voice: self.voice }
    }
    /// Spoken mic level (None = typed).
    pub fn level(&self) -> Option<f32> {
        (self.voice > 0).then(|| (self.voice - 1) as f32 / 254.0)
    }
    pub fn energy(&self, table: &ShoutTable) -> f32 {
        match self.level() {
            Some(l) => voice_energy(l, &self.text, table),
            None => typed_energy(&self.text, table),
        }
    }
}

/// Drop blocked (slur / hate) words; `None` if nothing allowed remains or the phrase as a whole is
/// blocked (e.g. a slur spelled across words). Profanity passes (owner's call).
pub fn filter_shout(text: &str, blocked: impl Fn(&str) -> bool) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    for w in text.split(' ').filter(|w| !w.is_empty()) {
        let core: String = w.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        if !core.is_empty() && blocked(&core) {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    let letters: String = out.chars().filter(|c| c.is_ascii_alphanumeric() || *c == ' ').collect();
    if letters.trim().is_empty() || blocked(&letters) {
        return None;
    }
    Some(out)
}

/// Host: accept at most this many shouts per second per player (burst 3).
const HOST_SHOUT_RATE: f32 = 2.0;

/// Shouts waiting to go out / arriving (NET glue; drained every frame).
#[derive(Resource, Default)]
pub struct ShoutNet {
    /// Local shouts to send (client → host, or host → everyone).
    pub outgoing: Vec<ShoutLine>,
    /// (speaker, line) received this frame.
    pub incoming: Vec<(PlayerId, ShoutLine)>,
    /// Host rate limit: (player, tokens, last refill).
    buckets: Vec<(PlayerId, f32, f64)>,
}

impl ShoutNet {
    fn allow(&mut self, who: PlayerId, now: f64) -> bool {
        let i = match self.buckets.iter().position(|b| b.0 == who) {
            Some(i) => i,
            None => {
                if self.buckets.len() >= 64 {
                    self.buckets.remove(0);
                }
                self.buckets.push((who, 3.0, now));
                self.buckets.len() - 1
            }
        };
        let b = &mut self.buckets[i];
        b.1 = (b.1 + (now - b.2).max(0.0) as f32 * HOST_SHOUT_RATE).min(3.0);
        b.2 = now;
        if b.1 < 1.0 {
            return false;
        }
        b.1 -= 1.0;
        true
    }
}

// ------------------------------------------------------------------------------------ plugin

struct Tracked {
    entity: Entity,
    owner: PlayerId,
    born: f64,
    bound: bool,
}

#[derive(Resource, Default)]
struct ShoutTracker {
    live: Vec<Tracked>,
    /// Voice: peak level of the current / last utterance.
    voice_peak: f32,
    was_listening: bool,
    /// Dev demo: next spoken cast uses this level.
    demo_level: Option<f32>,
}

/// A bound target within this long of the shout (s); later constructs are someone else's cast.
const BIND_WINDOW: f64 = 1.6;

pub struct ShoutPlugin;

impl Plugin for ShoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ShoutCast>()
            .init_resource::<ShoutNet>()
            .init_resource::<ShoutTracker>()
            .init_resource::<ShoutDemo>()
            .add_systems(Update, (shout_demo, shout_demo_world, local_shouts, net_shouts, remote_shouts).chain().in_set(PresentSet::Events))
            .add_systems(Update, (bind_shouts, follow_shouts).chain().in_set(PresentSet::Sync))
            .add_systems(PreUpdate, mirror_comfort)
            .add_systems(OnExit(crate::state::AppState::InGame), clear_shouts);
    }
}

fn clear_shouts(mut commands: Commands, mut tr: ResMut<ShoutTracker>, mut net: ResMut<ShoutNet>) {
    for t in tr.live.drain(..) {
        commands.entity(t.entity).try_despawn();
    }
    net.outgoing.clear();
    net.incoming.clear();
}

fn mirror_comfort(comfort: Res<ComfortSettings>, mut s: ResMut<ShoutSettings>) {
    let gentle = comfort.shake <= 0.01 || comfort.flashes <= 0.01;
    if s.gentle != gentle {
        s.gentle = gentle;
    }
}

/// The slur filter (no lexicon yet ⇒ everything is blocked: nothing unchecked ever renders).
fn blocked_by(lex: Option<&RingLexicon>) -> impl Fn(&str) -> bool + '_ {
    move |w: &str| lex.is_none_or(|l| l.0.is_blocked_name(w))
}

#[allow(clippy::too_many_arguments)]
fn local_shouts(
    mut commands: Commands,
    mut casts: MessageReader<ShoutCast>,
    time: Res<Time<Real>>,
    prefs: Res<UiPrefs>,
    lex: Option<Res<RingLexicon>>,
    voice: Option<Res<VoiceUi>>,
    table: Res<ShoutTable>,
    active: Option<Res<ActiveWorld>>,
    anchor: Res<ViewAnchor>,
    look: Res<LookAngles>,
    settings: Res<ShoutSettings>,
    mut tr: ResMut<ShoutTracker>,
    mut net: ResMut<ShoutNet>,
    mut shake: MessageWriter<CameraShakeEvent>,
) {
    // Voice: peak level of the utterance (fast-attack smoothed level, reset on a new press).
    if let Some(v) = voice.as_ref() {
        if v.listening && !tr.was_listening {
            tr.voice_peak = 0.0;
        }
        if v.listening {
            tr.voice_peak = tr.voice_peak.max(v.level);
        }
        tr.was_listening = v.listening;
    }
    let Some(active) = active else {
        casts.clear();
        return;
    };
    let Some(me) = active.local else {
        casts.clear();
        return;
    };
    for c in casts.read() {
        let level = c.spoken.then(|| tr.demo_level.take().unwrap_or(tr.voice_peak));
        let Some(text) = filter_shout(&sanitize_shout(&c.text, SHOUT_MAX_CHARS), blocked_by(lex.as_deref())) else { continue };
        let line = ShoutLine::new(&text, level);
        if line.text.is_empty() {
            continue;
        }
        net.outgoing.push(line.clone());
        if prefs.shout == ShoutPref::Off || super::perf_off("shout") {
            continue;
        }
        let energy = line.energy(&table);
        let color = corps_rgb(active.corps_of(me));
        let mut s = Shout::new(line.text, color, energy, true, anchor.eye);
        s.toward = anchor.eye + look.forward() * 12.0;
        s.seed = time.elapsed_secs_f64().to_bits();
        let e = commands.spawn((Name::new("Shout"), s)).id();
        tr.live.push(Tracked { entity: e, owner: me, born: time.elapsed_secs_f64(), bound: false });
        if energy > 1.6 && !settings.gentle {
            shake.write(CameraShakeEvent { trauma: ((energy - 1.6) * 0.18).min(0.2), dir: Vec3::ZERO });
        }
    }
}

/// NET glue: local shouts out, remote shouts in (host relays with a rate limit).
fn net_shouts(mut net_state: Option<ResMut<NetState>>, mut net: ResMut<ShoutNet>, time: Res<Time<Real>>) {
    let now = time.elapsed_secs_f64();
    let Some(ns) = net_state.as_deref_mut() else {
        net.outgoing.clear();
        return;
    };
    let net = &mut *net;
    if let Some(h) = ns.host.as_mut() {
        for line in net.outgoing.drain(..) {
            if h.in_match {
                h.session.broadcast(&S2C::Shout { player: h.me, line });
            }
        }
        for (from, line) in h.shouts_in.drain(..) {
            let line = line.sanitized();
            if line.text.is_empty() || !h.in_match || !net.allow(from, now) {
                continue;
            }
            h.session.broadcast_except(from, &S2C::Shout { player: from, line: line.clone() });
            net.incoming.push((from, line));
        }
    } else if let Some(c) = ns.client.as_mut() {
        for line in net.outgoing.drain(..) {
            let _ = c.session.send(&C2S::Shout(line));
        }
        let me = c.me;
        for (from, line) in c.shouts.drain(..) {
            if Some(from) != me {
                net.incoming.push((from, line.sanitized()));
            }
        }
    } else {
        net.outgoing.clear();
    }
}

#[allow(clippy::too_many_arguments)]
fn remote_shouts(
    mut commands: Commands,
    time: Res<Time<Real>>,
    prefs: Res<UiPrefs>,
    lex: Option<Res<RingLexicon>>,
    table: Res<ShoutTable>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mut tr: ResMut<ShoutTracker>,
    mut net: ResMut<ShoutNet>,
) {
    if net.incoming.is_empty() {
        return;
    }
    let show = prefs.shout == ShoutPref::On && !super::perf_off("shout");
    let Some(active) = active.filter(|_| show) else {
        net.incoming.clear();
        return;
    };
    for (from, line) in net.incoming.drain(..) {
        if Some(from) == active.local {
            continue;
        }
        let Some(v) = views.player(from) else { continue };
        // Re-filter on every receiver: a hostile peer can send anything. No lexicon yet → nothing shown.
        let Some(text) = filter_shout(&line.text, blocked_by(lex.as_deref())) else { continue };
        let line = ShoutLine { text, voice: line.voice };
        let energy = line.energy(&table);
        let hand = hand_transform(v).translation;
        let mut s = Shout::new(line.text, corps_rgb(v.corps), energy, false, hand);
        s.head = v.pos + Vec3::Y * 1.9 * v.size.max(0.2);
        s.toward = hand + (Quat::from_euler(EulerRot::YXZ, v.yaw, v.pitch, 0.0) * Vec3::NEG_Z) * 10.0;
        s.seed = (from.0 as u64) << 32 ^ time.elapsed_secs_f64().to_bits();
        let e = commands.spawn((Name::new("Shout"), s)).id();
        tr.live.push(Tracked { entity: e, owner: from, born: time.elapsed_secs_f64(), bound: false });
    }
}

/// New construct entities → the caster's newest unbound shout; `CastFailed` → fizzle.
#[allow(clippy::too_many_arguments)]
fn bind_shouts(
    time: Res<Time<Real>>,
    mut events: MessageReader<GameEvents>,
    new: Query<(Entity, &SimConstruct, &ConstructVisual, &Transform), Added<SimConstruct>>,
    mut shouts: Query<&mut Shout>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    settings: Res<ShoutSettings>,
    active: Option<Res<ActiveWorld>>,
    mut tr: ResMut<ShoutTracker>,
    mut shake: MessageWriter<CameraShakeEvent>,
) {
    let now = time.elapsed_secs_f64();
    let tr = &mut *tr;
    tr.live.retain(|t| shouts.contains(t.entity));
    for ev in events.read() {
        if let GameEvent::CastFailed { player, .. } = &ev.event
            && let Some(t) = tr.live.iter_mut().rev().find(|t| t.owner == *player && !t.bound && now - t.born < BIND_WINDOW)
        {
            t.bound = true;
            if let Ok(mut s) = shouts.get_mut(t.entity) {
                s.fizzle = true;
            }
        }
    }
    if tr.live.is_empty() {
        return;
    }
    let eye = cam.single().map(|g| g.translation()).unwrap_or(Vec3::ZERO);
    let local = active.as_ref().and_then(|a| a.local);
    for (e, sc, vis, tf) in &new {
        let Some(t) = tr.live.iter_mut().rev().find(|t| t.owner == sc.owner && !t.bound && now - t.born < BIND_WINDOW) else { continue };
        t.bound = true;
        let Ok(mut s) = shouts.get_mut(t.entity) else { continue };
        let b = &vis.model.bounds;
        let size = (b.max - b.min).max_element() * tf.scale.max_element();
        s.target = Some(e);
        s.toward = tf.translation;
        s.boost = size_boost(size);
        // A kaiju / no-limits construct shakes the screen (comfort: never).
        let intensity = bd_fx::shout_intensity(s.energy, s.boost);
        let near = Some(sc.owner) == local || tf.translation.distance(eye) < 30.0;
        if near && s.boost > 1.6 && intensity > 2.2 && !settings.gentle {
            shake.write(CameraShakeEvent { trauma: ((intensity - 2.2) * 0.12).min(0.3), dir: Vec3::ZERO });
        }
    }
}

/// World shouts ride above their caster.
fn follow_shouts(views: Res<WorldViews>, tr: Res<ShoutTracker>, mut shouts: Query<&mut Shout>) {
    for t in &tr.live {
        let Ok(mut s) = shouts.get_mut(t.entity) else { continue };
        if s.local {
            continue;
        }
        if let Some(v) = views.player(t.owner) {
            let hand = hand_transform(v).translation;
            let head = v.pos + Vec3::Y * 1.9 * v.size.max(0.2);
            if s.anchor != hand {
                s.anchor = hand;
            }
            if s.head != head {
                s.head = head;
            }
        }
    }
}

// ------------------------------------------------------------------------------------ dev demo

struct DemoEntry {
    at: f32,
    text: String,
    voice: Option<f32>,
    remote: Option<f32>,
    done: bool,
}

#[derive(Resource, Default)]
struct ShoutDemo {
    parsed: bool,
    entries: Vec<DemoEntry>,
    spam: bool,
    t: f32,
    next_spam: f32,
    n: u32,
    /// Bot pinned in front of the camera until this demo time: (bot, spot, until).
    pin: Option<(PlayerId, Vec3, f32)>,
    /// Bot casts to send (the world system writes them).
    casts: Vec<(PlayerId, Cmd)>,
}

const SPAM: [&str; 8] = [
    "sword",
    "GIANT HAMMER!!!",
    "giant eeyore shooting laser beams",
    "MEGA SHIELD",
    "wolf",
    "ROCKET LAUNCHER!!",
    "gomu gomu no pistol",
    "ULTRA DRAGON",
];

fn parse_demo(v: &str) -> (bool, Vec<DemoEntry>) {
    let mut out = Vec::new();
    for e in v.split(';') {
        let mut parts: Vec<&str> = e.split('|').collect();
        let Some(at) = parts.first().and_then(|t| t.trim().parse::<f32>().ok()) else { continue };
        let text = parts.pop().unwrap_or("").trim().to_string();
        let mut entry = DemoEntry { at, text, voice: None, remote: None, done: false };
        for f in parts.iter().skip(1) {
            let f = f.trim();
            if let Some(l) = f.strip_prefix('v') {
                entry.voice = l.parse().ok();
            } else if let Some(d) = f.strip_prefix('r') {
                entry.remote = d.parse().ok();
            }
        }
        out.push(entry);
    }
    (v.trim() == "spam", out)
}

#[allow(clippy::too_many_arguments)]
fn shout_demo(
    time: Res<Time<Real>>,
    mut demo: ResMut<ShoutDemo>,
    mut caster: Caster,
    active: Option<Res<ActiveWorld>>,
    lex: Option<Res<RingLexicon>>,
    anchor: Res<ViewAnchor>,
    look: Res<LookAngles>,
    mut tr: ResMut<ShoutTracker>,
    mut net: ResMut<ShoutNet>,
) {
    if !demo.parsed {
        demo.parsed = true;
        if let Ok(v) = std::env::var("BD_SHOUT_DEMO") {
            (demo.spam, demo.entries) = parse_demo(&v);
        }
    }
    if demo.entries.is_empty() && !demo.spam {
        return;
    }
    let Some(active) = active else { return };
    demo.t += time.delta_secs();
    let t = demo.t;
    let Some(lex) = lex else { return };
    let fire: Option<(String, Option<f32>, Option<f32>)> = if demo.spam {
        if t >= demo.next_spam {
            demo.next_spam = t + 0.35;
            demo.n += 1;
            let n = demo.n as usize;
            // Spectator (brawl bench): every shout comes from a bot.
            let remote = n % 2 == 0 || active.local.is_none();
            Some((SPAM[n % SPAM.len()].to_string(), (n % 3 == 0).then_some(0.95), remote.then_some(12.0 + (n % 5) as f32 * 6.0)))
        } else {
            None
        }
    } else {
        demo.entries.iter_mut().find(|e| !e.done && t >= e.at).map(|e| {
            e.done = true;
            (e.text.clone(), e.voice, e.remote)
        })
    };
    let Some((text, voice, remote)) = fire else { return };
    info!("shout demo t={t:.2}: {text:?} voice={voice:?} remote={remote:?}");
    match remote {
        None => {
            tr.demo_level = voice;
            caster.cast(&text, voice.is_some(), 0);
        }
        Some(d) => {
            // A bot shouts it from `d` metres in front of the camera: pinned there, casting past us.
            let n = demo.n as usize;
            let Some(bot) = active.bots.get(if demo.spam { n % active.bots.len().max(1) } else { 0 }).map(|b| b.id) else { return };
            let fwd = look.forward();
            let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::NEG_Z);
            let spot = anchor.eye + flat * d + Vec3::Y * -1.2;
            if !demo.spam {
                demo.pin = Some((bot, spot, t + 3.0));
            }
            let res = lex.0.resolve(&text);
            let eye = spot + Vec3::Y * 1.6;
            let dir = (anchor.eye - eye).normalize_or(Vec3::NEG_Z);
            let side = Vec3::Y.cross(dir).normalize_or(Vec3::X);
            // Cast across our view (not at us: the demo is for looking at the words).
            let aim = bd_sim::Aim::new(super::gv(eye), super::gv((side + dir * 0.25).normalize_or(side)));
            demo.casts.push((
                bot,
                Cmd::Cast { recipe: Box::new(res.recipe), shape: res.shape, alt_shape: res.alt_shape, aim, incantation: false },
            ));
            net.incoming.push((bot, ShoutLine::new(&text, voice)));
        }
    }
}

/// Demo: pin the shouting bot in view, send its casts.
fn shout_demo_world(mut demo: ResMut<ShoutDemo>, active: Option<ResMut<ActiveWorld>>, mut cmds: MessageWriter<SimCommand>) {
    if demo.pin.is_none() && demo.casts.is_empty() {
        return;
    }
    let Some(mut active) = active else { return };
    for (bot, c) in demo.casts.drain(..) {
        cmds.write(SimCommand::remote(bot, c));
    }
    if let Some((bot, spot, until)) = demo.pin {
        if demo.t < until {
            if let Some(p) = active.world.player_mut(bot) {
                p.mv.pos = super::gv(spot);
                p.mv.vel = bd_core::Vec3::ZERO;
            }
        } else {
            demo.pin = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_blocked(w: &str) -> bool {
        let w = w.to_ascii_lowercase();
        w.contains("slurx") || w.replace(' ', "").contains("badword")
    }

    #[test]
    fn filter_drops_blocked_words_keeps_profanity() {
        assert_eq!(filter_shout("GIANT SLURX HAMMER", fake_blocked).as_deref(), Some("GIANT HAMMER"));
        assert_eq!(filter_shout("fucking huge hammer!!!", fake_blocked).as_deref(), Some("fucking huge hammer!!!"));
        assert_eq!(filter_shout("slurx", fake_blocked), None, "nothing left");
        assert_eq!(filter_shout("bad word", fake_blocked), None, "spelled across words");
        assert_eq!(filter_shout("", fake_blocked), None);
        assert_eq!(filter_shout("!!!", fake_blocked), None);
    }

    #[test]
    fn filter_with_real_lexicon() {
        let path = crate::assets_dir().join("lexicon.bin");
        let Ok(lex) = bd_lexicon::Lexicon::load(&path) else { return };
        let f = |t: &str| filter_shout(&sanitize_shout(t, SHOUT_MAX_CHARS), |w| lex.is_blocked_name(w));
        assert_eq!(f("KKK hammer").as_deref(), Some("hammer"));
        assert_eq!(f("n4zi"), None);
        assert_eq!(f("GIANT HAMMER!!!").as_deref(), Some("GIANT HAMMER!!!"));
        assert_eq!(f("shit sword").as_deref(), Some("shit sword"), "profanity is allowed");
    }

    #[test]
    fn line_caps_and_levels() {
        let l = ShoutLine::new(&"giant eeyore shooting laser beams ".repeat(20), None);
        assert!(l.text.len() <= SHOUT_MAX_CHARS && !l.text.is_empty());
        assert_eq!(l.level(), None);
        let v = ShoutLine::new("hammer", Some(1.0));
        assert_eq!(v.voice, 255);
        assert!((v.level().unwrap_or(0.0) - 1.0).abs() < 1e-6);
        assert_eq!(ShoutLine::new("x", Some(f32::NAN)).level(), Some(0.0));
        assert_eq!(ShoutLine::new("x", Some(0.0)).voice, 1);
        // Hostile wire data is re-sanitised.
        let h = ShoutLine { text: "\u{7}A".repeat(5000), voice: 9 }.sanitized();
        assert!(h.text.len() <= SHOUT_MAX_CHARS && h.text.is_ascii() && !h.text.contains('\u{7}'));
        let t = ShoutTable::default();
        assert!(ShoutLine::new("HAMMER", Some(1.0)).energy(&t) > ShoutLine::new("hammer", None).energy(&t) * 3.0);
    }

    #[test]
    fn line_roundtrips_on_the_wire() {
        let m = S2C::Shout { player: PlayerId(3), line: ShoutLine::new("GOMU GOMU NO PISTOL!!", Some(0.9)) };
        let back: S2C = bd_net::codec::decode(&bd_net::codec::encode(&m)).expect("decodes");
        assert!(matches!(back, S2C::Shout { player: PlayerId(3), line } if line.text == "GOMU GOMU NO PISTOL!!"));
        let c = C2S::Shout(ShoutLine::new("sword", None));
        let bytes = bd_net::codec::encode(&c);
        assert!(bytes.len() < 32, "small on the wire: {}", bytes.len());
    }

    #[test]
    fn host_rate_limit() {
        let mut n = ShoutNet::default();
        let who = PlayerId(1);
        let ok = (0..10).filter(|_| n.allow(who, 5.0)).count();
        assert_eq!(ok, 3, "burst of 3");
        assert!(n.allow(who, 5.6), "refills");
        assert!(n.allow(PlayerId(2), 5.6), "per player");
    }
}
