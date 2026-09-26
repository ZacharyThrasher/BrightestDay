//! FREEDOM bundle — client half (`bd_sim::freedom` is the sim half).
//!
//! * [`plan_cast`] (used by `console::Caster::cast`): "it" = your newest live construct (the rest of the
//!   phrase is grafted onto it; the host re-checks ownership), "again!" / "two more" re-cast your last
//!   phrase, "missile at Jake" aims at Jake (the name never reaches the resolver). Pure text steps in
//!   `bd_lexicon::refer`; anything that doesn't apply falls back to an ordinary cast.
//! * [`cast_energy`]: shout energy → the `u8` sent with the cast (`bd_sim::freedom::energy_byte`).
//!   Typed (CAPS / "!!!") and spoken (peak mic level) go through the same mapping and both saturate.
//! * Kill feed (top right): "Sam was flattened by KING KONG ON A SKATEBOARD" — the phrase as cast,
//!   slur-filtered with the lexicon on display; legendary constructs in gold.
//! * Legendary construct: a subtle positional `legendary_sting` + a gold burst, then a slow gold sparkle
//!   while it lives. A sputtering cast: a positional `sputter` (the flicker itself is `present/constructs.rs`).
//! * [`MatchHighlights`]: the end-of-match highlights the scoreboard's results panel shows.
//! * Dev: `BD_FREEDOM_TEST=1` (practice) — a legendary cast, a sputtering cast, two KOs and the match
//!   end, for `--screenshot` runs.

use crate::console::{Caster, RingLexicon};
use crate::lobby::UiSet;
use crate::present::sound::Sfx3d;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use bd_core::{ConstructId, PlayerId};
use bd_fx::{BurstFx, BurstKind, Emit, FxParticles, PFlags, PKind};
use bd_lexicon::{Lexicon, Refer};
use bd_sim::GameEvent;
use bd_sim::freedom::{FeedLine, FreedomEvent, Highlights};
use bd_sim::replicate::cflags;
use bevy::picking::Pickable;
use bevy::prelude::*;

/// Gold (sRGB) for legendary things in the UI.
pub const GOLD: Color = Color::srgb(1.0, 0.8, 0.25);
/// Legendary construct palette (linear): body + accent.
pub const GOLD_LINEAR: [f32; 3] = [1.0, 0.62, 0.08];
pub const GOLD_ACCENT_LINEAR: [f32; 3] = [1.0, 0.9, 0.55];
/// Feed lines shown at once / seconds each stays.
const FEED_MAX: usize = 5;
const FEED_SECS: f32 = 7.0;

// ------------------------------------------------------------------------------------ cast planning

/// The client's memory for "again!" (the spoken loudness comes from `SignatureVoice`'s peak tracker).
#[derive(Resource, Default)]
pub struct CastMemo {
    /// The last phrase cast (as typed / spoken) — what "again!" repeats.
    pub last: Option<String>,
}

/// What a phrase turned into before the resolver sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct CastPlan {
    /// Text for the resolver (names / "make it" removed).
    pub text: String,
    /// The words that go into the kill feed (the phrase as said; "again!" = the repeated phrase).
    pub said: String,
    /// "it": graft onto this own construct.
    pub graft_onto: Option<ConstructId>,
    /// "… at Jake": aim homing / autonomy at this player.
    pub target: Option<PlayerId>,
    /// "two more": copies.
    pub count: Option<u8>,
    /// This was "again" (don't remember it as the last phrase).
    pub repeat: bool,
}

impl CastPlan {
    pub fn plain(text: &str) -> Self {
        Self { text: text.to_string(), said: text.to_string(), graft_onto: None, target: None, count: None, repeat: false }
    }
}

/// The caster's most recent live construct (children — swarm copies, payloads — never count).
pub fn newest_own(world: &bd_sim::World, local: PlayerId) -> Option<ConstructId> {
    world.constructs.owned_by(local).filter(|c| c.parent.is_none()).max_by_key(|c| c.id.0).map(|c| c.id)
}

/// "it" / "again" / player names — see the module docs. Never fails: at worst the plan is the text.
pub fn plan_cast(lex: &Lexicon, world: &bd_sim::World, local: PlayerId, text: &str, spoken: bool, last: Option<&str>) -> CastPlan {
    let mut plan = CastPlan::plain(text);
    if let Some(Refer::Again { count }) = lex.refer(text) {
        // No memory yet: the words are cast as they are (the ring never says no).
        if let Some(l) = last {
            plan = CastPlan { repeat: true, count, ..CastPlan::plain(l) };
        }
    }
    if let Some(Refer::It { rest }) = lex.refer(&plan.text) {
        // No live construct: a normal cast of the rest ("make it bigger" → something bigger).
        plan.graft_onto = newest_own(world, local);
        plan.text = rest;
    }
    let others: Vec<(PlayerId, &str)> = world.players.iter().filter(|p| p.id != local).map(|p| (p.id, p.name.as_str())).collect();
    let names: Vec<&str> = others.iter().map(|o| o.1).collect();
    if let Some(t) = lex.target_player(&plan.text, &names, spoken) {
        plan.target = others.get(t.player).map(|o| o.0);
        plan.text = t.rest;
    }
    plan
}

/// Shout energy byte for a cast: typed energy, or the spoken energy at `voice_peak`.
pub fn cast_energy(text: &str, voice_peak: Option<f32>, table: Option<&bd_fx::ShoutTable>) -> u8 {
    let default;
    let table = match table {
        Some(t) => t,
        None => {
            default = bd_fx::ShoutTable::default();
            &default
        }
    };
    let e = match voice_peak {
        Some(l) => bd_fx::voice_energy(l, text, table),
        None => bd_fx::typed_energy(text, table),
    };
    bd_sim::freedom::energy_byte(e)
}

// ------------------------------------------------------------------------------------ plugin

/// End-of-match highlights (shown by the scoreboard's results panel while the match is over).
#[derive(Resource, Default)]
pub struct MatchHighlights(pub Option<Highlights>);

#[derive(Resource, Default)]
struct KillFeed {
    /// (text, legendary, involves the local player, age s)
    lines: Vec<(String, bool, bool, f32)>,
    dirty: bool,
}

#[derive(Component)]
struct FeedRoot;
#[derive(Component)]
struct FeedEntry(usize);

pub struct FeedPlugin;

impl Plugin for FeedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CastMemo>()
            .init_resource::<MatchHighlights>()
            .init_resource::<KillFeed>()
            .add_systems(Startup, spawn_feed_ui)
            .init_resource::<FreedomTest>()
            .add_systems(Update, (freedom_test_world, freedom_test_cast).chain().in_set(UiSet::Input).run_if(freedom_test_on))
            .add_systems(Update, (read_events, draw_feed, construct_flair_fx).chain().in_set(UiSet::Draw))
            .add_systems(OnExit(AppState::InGame), clear_on_exit);
    }
}

fn clear_on_exit(mut feed: ResMut<KillFeed>, mut h: ResMut<MatchHighlights>, mut memo: ResMut<CastMemo>) {
    feed.lines.clear();
    feed.dirty = true;
    h.0 = None;
    memo.last = None;
}

fn spawn_feed_ui(mut commands: Commands) {
    commands.spawn((
        Name::new("KillFeed"),
        FeedRoot,
        Node {
            position_type: PositionType::Absolute,
            right: px(18),
            top: px(44),
            max_width: px(560),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: px(4),
            ..default()
        },
        Pickable::IGNORE,
        GlobalZIndex(38),
    ));
}

fn player_name(active: &ActiveWorld, id: Option<PlayerId>) -> Option<String> {
    let id = id?;
    Some(active.world.player(id).map_or_else(|| format!("P{}", id.0), |p| p.name.clone()))
}

/// Kill-feed text for a line, slur-filtered with the lexicon (no lexicon yet: phrases are held back).
pub fn feed_line_text(line: &FeedLine, active: &ActiveWorld, lex: Option<&RingLexicon>) -> String {
    let victim = player_name(active, Some(line.victim)).unwrap_or_default();
    let killer = player_name(active, line.killer);
    bd_sim::freedom::feed_text(line, &victim, killer.as_deref(), |w| lex.is_none_or(|l| l.0.is_blocked_name(w)))
}

#[allow(clippy::too_many_arguments)]
fn read_events(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    lex: Option<Res<RingLexicon>>,
    mut feed: ResMut<KillFeed>,
    mut highlights: ResMut<MatchHighlights>,
    mut sfx: MessageWriter<Sfx3d>,
    mut burst: MessageWriter<BurstFx>,
    time: Res<Time<Real>>,
) {
    let dt = time.delta_secs();
    if !feed.lines.is_empty() {
        for l in &mut feed.lines {
            l.3 += dt;
        }
        let before = feed.lines.len();
        feed.lines.retain(|l| l.3 < FEED_SECS + 1.0);
        if feed.lines.len() != before {
            feed.dirty = true;
        }
    }
    let Some(active) = active else {
        events.clear();
        return;
    };
    for ev in events.read() {
        match &ev.event {
            GameEvent::Freedom(FreedomEvent::Feed(line)) => {
                let text = feed_line_text(line, &active, lex.as_deref());
                let mine = active.local.is_some_and(|me| line.victim == me || line.killer == Some(me));
                feed.lines.push((text, line.legendary, mine, 0.0));
                if feed.lines.len() > FEED_MAX {
                    feed.lines.remove(0);
                }
                feed.dirty = true;
            }
            GameEvent::Freedom(FreedomEvent::Legendary { id, owner }) => {
                // Subtle: a positional sting + a gold burst, no announcement (never says why).
                let mine = active.local == Some(*owner);
                if let Some(c) = active.world.constructs.get(*id) {
                    let pos = crate::util::v3(c.center());
                    sfx.write(Sfx3d::at("legendary_sting", pos).gain(if mine { 0.7 } else { 0.45 }));
                    burst.write(BurstFx { kind: BurstKind::Shards, pos, dir: Vec3::ZERO, scale: 2.5, color: Some(GOLD_ACCENT_LINEAR) });
                    burst.write(BurstFx { kind: BurstKind::Flash, pos, dir: Vec3::ZERO, scale: 1.5, color: Some(GOLD_LINEAR) });
                }
            }
            GameEvent::Freedom(FreedomEvent::Highlights(h)) => highlights.0 = Some((**h).clone()),
            GameEvent::MatchPhase { phase: bd_sim::MatchPhase::Live | bd_sim::MatchPhase::Warmup, .. } => highlights.0 = None,
            _ => {}
        }
    }
}

fn draw_feed(
    mut commands: Commands,
    mut feed: ResMut<KillFeed>,
    root: Query<Entity, With<FeedRoot>>,
    kids: Query<Entity, With<FeedEntry>>,
    mut texts: Query<(&FeedEntry, &mut TextColor)>,
) {
    let Ok(root) = root.single() else { return };
    if feed.dirty {
        feed.dirty = false;
        for e in &kids {
            commands.entity(e).despawn();
        }
        commands.entity(root).with_children(|r| {
            for (k, (text, legendary, mine, _)) in feed.lines.iter().enumerate() {
                let size = if *mine { 19.0 } else { 16.0 };
                r.spawn((
                    FeedEntry(k),
                    Text::new(text.clone()),
                    TextFont::from_font_size(size),
                    TextColor(if *legendary { GOLD } else { Color::WHITE }),
                    TextShadow { offset: Vec2::new(0.0, 1.5), color: Color::srgba(0.0, 0.0, 0.0, 0.75) },
                    TextLayout::justify(Justify::Right),
                    Pickable::IGNORE,
                ));
            }
        });
        return;
    }
    // Fade the last second.
    for (e, mut c) in &mut texts {
        if let Some(l) = feed.lines.get(e.0) {
            let a = (FEED_SECS + 1.0 - l.3).clamp(0.0, 1.0) * if l.2 { 1.0 } else { 0.85 };
            if (c.0.alpha() - a).abs() > 0.01 {
                c.0.set_alpha(a);
            }
        }
    }
}

/// Legendary constructs sparkle gold; the local player's sputtering casts play `sputter` once.
fn construct_flair_fx(
    views: Res<WorldViews>,
    active: Option<Res<ActiveWorld>>,
    fx: Option<ResMut<FxParticles>>,
    time: Res<Time>,
    mut sfx: MessageWriter<Sfx3d>,
    mut seen: Local<Vec<ConstructId>>,
    mut acc: Local<f32>,
) {
    let local = active.as_ref().and_then(|a| a.local);
    for c in &views.constructs {
        if c.flags & cflags::SPUTTER != 0 && !seen.contains(&c.id) {
            if seen.len() >= 32 {
                seen.remove(0);
            }
            seen.push(c.id);
            let pos = if c.attached() { views.player(c.owner).map_or(c.pos, |p| p.pos + Vec3::Y * 1.2) } else { c.pos };
            sfx.write(Sfx3d::at("sputter", pos).gain(if Some(c.owner) == local { 0.8 } else { 0.5 }));
        }
    }
    let Some(mut fx) = fx else { return };
    *acc += time.delta_secs();
    if *acc < 0.06 {
        return;
    }
    *acc = 0.0;
    for c in views.constructs.iter().filter(|c| c.flags & cflags::LEGENDARY != 0 && !c.attached()) {
        let half = active.as_ref().and_then(|a| a.world.constructs.get(c.id)).map_or(1.0, |k| k.half.max_element());
        let r = (half * 1.2).clamp(0.4, 12.0);
        let seed = fx.rng.next_u32();
        let off = Vec3::new(fx.rng.range(-r, r), fx.rng.range(0.0, r * 1.6), fx.rng.range(-r, r));
        fx.emit(Emit {
            kind: PKind::Rise,
            flags: PFlags::PLAIN,
            pos: c.pos + off,
            vel: Vec3::ZERO,
            size: 0.09,
            color: GOLD_ACCENT_LINEAR,
            emissive: 3.0,
            life: 1.1,
            gravity: 0.8,
            spin: 0.25,
            seed,
            ..Default::default()
        });
    }
}

// ------------------------------------------------------------------------------------ dev test plan

fn freedom_test_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("BD_FREEDOM_TEST").is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// Dev test: bot number `k` is hit by the local player's newest (legendary if `gold`) construct and
/// dropped out of the world (the real hit + blast-zone path, so the feed line is the sim's own).
fn ko_bot(active: &mut ActiveWorld, me: PlayerId, k: usize, gold: bool) {
    let cid = active
        .world
        .constructs
        .owned_by(me)
        .filter(|c| c.parent.is_none() && (c.flair & cflags::LEGENDARY != 0) == gold)
        .max_by_key(|c| c.id.0)
        .map(|c| c.id);
    let Some(bot) = active.bots.get(k).map(|b| b.id) else { return };
    let Some(pos) = active.world.player(bot).map(|p| p.mv.pos) else { return };
    let mut hit = bd_sim::HitInfo::new(bot, Some(me), bd_sim::HitKind::Creature, 25.0, 8.0, bd_core::Vec3::Y, pos);
    hit.construct = cid;
    if let Some(p) = active.world.player_mut(bot) {
        p.spawn_shield = 0.0;
    }
    active.world.apply_hit(hit);
    if let Some(p) = active.world.player_mut(bot) {
        p.mv.pos = bd_core::Vec3::new(pos.x, -500.0, pos.z);
    }
    info!("freedom test: KO bot {k} with {cid:?}");
}

/// Dev test plan state (`BD_FREEDOM_TEST`).
#[derive(Resource, Default)]
struct FreedomTest {
    t: f32,
    step: usize,
    saved: Option<bd_sim::tuning::CombatTuning>,
    cast: Option<String>,
}

/// `BD_FREEDOM_TEST=1` (practice): t=2.5 s a (forced) legendary "KING KONG ON A SKATEBOARD!!!", 6 s a
/// bot KO'd by it, 7 s a sputtering "GIANT BEAR" at 1.5 willpower, 8.9 s a bot KO'd by that, 11.5 s
/// the match ends (highlights). Screenshots at ~5.8 / 8.3 / 10 / 13.
fn freedom_test_world(
    time: Res<Time<Real>>,
    state: Res<State<AppState>>,
    lex: Option<Res<RingLexicon>>,
    active: Option<ResMut<ActiveWorld>>,
    mut st: ResMut<FreedomTest>,
) {
    if *state.get() != AppState::InGame || lex.is_none() {
        return;
    }
    let Some(mut active) = active else { return };
    let Some(me) = active.local else { return };
    st.t += time.delta_secs();
    let t = st.t;
    match st.step {
        0 if t >= 2.5 => {
            let saved = active.world.tuning().clone();
            let mut forced = saved.clone();
            forced.freedom.legendary.base = 1.0;
            forced.freedom.legendary.cap = 1.0;
            active.world.set_tuning(forced);
            st.saved = Some(saved);
            // A kaiju costs more than a full bar: infinite willpower for this part (else it sputters —
            // and starves once the bar is empty, which is the honest rule, just not this shot).
            active.world.cfg.rules.willpower = bd_sim::WillpowerMode::Infinite;
            info!("freedom test: legendary cast");
            st.cast = Some("KING KONG ON A SKATEBOARD!!!".into());
            st.step = 1;
        }
        1 if t >= 3.0 => {
            if let Some(saved) = st.saved.take() {
                active.world.set_tuning(saved);
            }
            st.step = 2;
        }
        2 if t >= 6.0 => {
            ko_bot(&mut active, me, 0, true);
            st.step = 3;
        }
        3 if t >= 7.0 => {
            // Clear the view (dismiss the kaiju), back to the normal economy, almost empty.
            let gold: Vec<ConstructId> = active.world.constructs.owned_by(me).map(|c| c.id).collect();
            for id in gold {
                active.world.dismiss_one(me, id);
            }
            active.world.cfg.rules.willpower = bd_sim::WillpowerMode::Normal;
            if let Some(p) = active.world.player_mut(me) {
                p.willpower = 1.5;
            }
            info!("freedom test: sputtering cast");
            st.cast = Some("GIANT BEAR".into());
            st.step = 4;
        }
        4 if t >= 8.9 => {
            ko_bot(&mut active, me, 1, false);
            st.step = 5;
        }
        5 if t >= 11.5 => {
            active.world.cfg.rules.ko_target = 1;
            info!("freedom test: match end");
            st.step = 6;
        }
        _ => {}
    }
}

/// The plan's casts go through the real console path (`Caster`: refer / names / energy / meta).
fn freedom_test_cast(mut st: ResMut<FreedomTest>, mut caster: Caster) {
    if let Some(text) = st.cast.take() {
        caster.cast(&text, false, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_voice_reach_the_same_max_energy() {
        let t = bd_fx::ShoutTable::default();
        let typed = cast_energy("HAMMER!!!", None, Some(&t));
        let yelled = cast_energy("hammer", Some(1.0), Some(&t));
        assert_eq!(typed, 255, "a full typed shout");
        assert_eq!(yelled, 255, "a full yell");
        assert_eq!(cast_energy("hammer", None, Some(&t)), 0, "calm typing");
        assert_eq!(cast_energy("hammer", Some(0.5), Some(&t)), 0, "calm talking");
        let bonus = |b: u8| bd_sim::freedom::energy_bonus(b, &bd_sim::freedom::FreedomTuning::default());
        assert_eq!(bonus(typed), bonus(yelled));
        assert!((bonus(typed) - 0.15).abs() < 1e-6);
    }
}
