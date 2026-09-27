//! HOTBAR firing + AUTO-BIND intake.
//!
//! * [`step_for`]: a slot's stored reading → the `ToolStep` saying its phrase sends (a held power's
//!   move fires as the HELD move while you hold that power — held priority, like the shout; otherwise
//!   it is borrowed). The console's `Caster::fire_steps` sends it as the ordinary `Cmd::Toolbox`.
//! * [`intake`]: the console / voice announce every line they send ([`LineSent`]); when the HOST's
//!   `PowerEvent::Fired` (or a signature's construct) for the local player confirms a one-move line,
//!   it is auto-bound (`UiPrefs::auto_bind`) and always remembered for the Tab tray. Any fired move
//!   already on the bar flares its slot (voice or key: one bar).
//! * [`derive_readings`]: phrases loaded from prefs get their reading once the lexicon is ready.

use super::{Entry, Hotbar, SlotBind};
use crate::console::{Caster, RingConsole, RingLexicon};
use crate::lobby::prefs::{Action, KeyBinds, UiPrefs};
use crate::sim_bridge::{ActiveWorld, Cmd, GameEvents, SimCommand};
use crate::util::core_v3;
use bevy::ecs::system::SystemParam;
use crate::state::{AppState, InGameMenu};
use bd_core::PlayerId;
use bd_sim::powers::chain::{FireHow, ToolStep};
use bd_sim::powers::compose;
use bd_sim::powers::toolbox::{MoveIndex, MoveRef};
use bd_sim::powers::twist::{self, TwistBase};
use bd_sim::powers::{move_sel, MoveDef, PlayerPower, PowerDef, PowerEvent, PowerTable};
use bd_sim::GameEvent;
use bevy::prelude::*;

/// A line the console / voice sent (written by `console::Caster`): the auto-bind candidate.
#[derive(Message, Clone, Debug)]
pub struct LineSent {
    pub text: String,
    #[allow(dead_code)] // (typed vs spoken: for readers that care; the bar treats both alike)
    pub spoken: bool,
    /// The toolbox steps it sent (empty = not a toolbox line).
    pub steps: Vec<ToolStep>,
    /// A signature cast ("kamehameha"): a famous move the lexicon casts.
    pub signature: bool,
}

/// How long the host has to confirm a line (a buffered line waits for the active phase to end).
const CONFIRM_SECS: f32 = 3.0;

/// The line waiting for the host's confirmation.
#[derive(Resource, Default, Debug)]
pub struct Pending {
    pub line: Option<(String, ToolStep, f32)>,
    pub signature: Option<(String, f32)>,
    /// A plain line that may GRANT a power ("star platinum", "gear 5"): bound when `Gained` answers.
    pub grant: Option<(String, f32)>,
    /// A twisted move's `Twisted` event came first (its `Fired` follows): the interned def.
    twisted: Option<&'static MoveDef>,
    /// DISMISS BY SLOT: the last line's identity, for the construct it spawns (≤ CONFIRM_SECS).
    pub spawn_for: Option<(u64, f32)>,
}

pub fn build(app: &mut App) {
    app.add_message::<LineSent>()
        .add_message::<GameEvents>()
        .init_resource::<Pending>()
        .add_systems(Update, slot_keys.in_set(crate::lobby::UiSet::Input))
        .add_systems(Update, (derive_readings, intake).chain().after(crate::sim_bridge::BridgeSet::Views).in_set(crate::present::PresentSet::Events));
}

/// Selector of `mv` among the held powers (active = a plain slot, others = `MOVE_OF`), if held at
/// that tier.
pub fn held_sel(pw: &PlayerPower, mv: MoveRef) -> Option<u8> {
    for (n, h) in pw.held().enumerate() {
        if h.id != mv.power {
            continue;
        }
        let has_own = h.def.tier(h.tier).is_some_and(|t| !t.moves.is_empty());
        let tier = if has_own { h.tier } else { 0 };
        if tier != mv.tier || (mv.slot as usize) >= h.def.moves_at(h.tier).len() {
            return None;
        }
        return Some(if n == 0 { mv.slot } else { move_sel::move_of(n, mv.slot) });
    }
    None
}

/// The global ref of a held selector (the move `sel` runs now).
pub fn held_ref(pw: &PlayerPower, sel: u8) -> Option<MoveRef> {
    if sel & move_sel::SELECT != 0 || sel == move_sel::CYCLE {
        return None;
    }
    let (held, slot) = move_sel::split_move_of(sel);
    let h = pw.held().nth(held)?;
    h.def.moves_at(h.tier).get(slot as usize)?;
    let has_own = h.def.tier(h.tier).is_some_and(|t| !t.moves.is_empty());
    Some(MoveRef::new(h.id, if has_own { h.tier } else { 0 }, slot))
}

/// The step a slot fires — exactly what saying its phrase sends (`None`: a phrase slot, cast as said).
pub fn step_for(bind: &SlotBind, held: Option<&PlayerPower>) -> Option<ToolStep> {
    match bind {
        SlotBind::Move { mv, kit } => Some(match held.and_then(|p| held_sel(p, *mv)) {
            Some(sel) => ToolStep::Held(sel),
            None => ToolStep::Move { mv: *mv, kit: *kit },
        }),
        SlotBind::Composed(c) => Some(ToolStep::Composed(*c)),
        SlotBind::Twist(t) => Some(ToolStep::Twist(t.clone())),
        SlotBind::Empty | SlotBind::Phrase { .. } => None,
    }
}

/// A one-step line's reading as a slot bind (held selectors become global refs so the slot keeps
/// naming the same move when the held set changes). `None`: not a single move.
pub fn bind_of(steps: &[ToolStep], held: Option<&PlayerPower>) -> Option<SlotBind> {
    let [step] = steps else { return None };
    match step {
        ToolStep::Move { mv, kit } => Some(SlotBind::Move { mv: *mv, kit: *kit }),
        ToolStep::Held(sel) => held.and_then(|p| held_ref(p, *sel)).map(|mv| SlotBind::Move { mv, kit: false }),
        ToolStep::Composed(c) => Some(SlotBind::Composed(*c)),
        ToolStep::Twist(t) => {
            let mut t = t.clone();
            t.shot = 0;
            for side in std::iter::once(&mut t.a).chain(t.b.as_mut()) {
                if let TwistBase::Held(sel) = side.base {
                    side.base = TwistBase::Move { mv: held.and_then(|p| held_ref(p, sel))?, kit: false };
                }
            }
            Some(SlotBind::Twist(t))
        }
        ToolStep::Cast { .. } => None,
    }
}

/// What a bind looks like (style / colour / cost / name come from its `MoveDef`).
pub fn present(bind: &SlotBind) -> (Option<&'static MoveDef>, Option<&'static PowerDef>) {
    let table: &'static PowerTable = PowerTable::get();
    let of_ref = |mv: MoveRef| mv.resolve(table).map(|(p, m)| (Some(m), Some(p))).unwrap_or((None, None));
    match bind {
        SlotBind::Move { mv, .. } => of_ref(*mv),
        SlotBind::Composed(c) => (compose::interned(c), None),
        SlotBind::Twist(t) => match &t.a.base {
            TwistBase::Move { mv, .. } => of_ref(*mv),
            TwistBase::Composed(s) => (compose::interned(&twist::split_composed(*s).0), None),
            _ => (None, None),
        },
        _ => (None, None),
    }
}

/// The phrase a held power's move is best called by (its first shout, else its name).
pub fn phrase_of(mv: MoveRef) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static FIRST: OnceLock<HashMap<MoveRef, String>> = OnceLock::new();
    let map = FIRST.get_or_init(|| {
        let mut all: Vec<(&str, MoveRef)> = MoveIndex::shipped().shouts().collect();
        all.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.len().cmp(&b.0.len())).then(a.0.cmp(b.0)));
        let mut m = HashMap::with_capacity(all.len());
        for (s, r) in all {
            m.entry(r).or_insert_with(|| s.to_string());
        }
        m
    });
    map.get(&mv).cloned().or_else(|| mv.resolve(PowerTable::get()).map(|(_, m)| m.name.clone()))
}

fn gameplay(state: &AppState, menu: &InGameMenu) -> bool {
    *state == AppState::InGame && !menu.0
}

/// What a key press needs to send a stored reading: the command queue, the crosshair and the
/// commit-frame feedback (the same ring flare / cast pose / confirm sound the console gives).
#[derive(SystemParam)]
pub struct KeyFire<'w> {
    active: Option<Res<'w, ActiveWorld>>,
    look: Res<'w, crate::input::LookAngles>,
    anchor: Res<'w, crate::camera::ViewAnchor>,
    aim_ray: Option<Res<'w, crate::present::camera3p::AimRay>>,
    cmds: MessageWriter<'w, SimCommand>,
    cues: MessageWriter<'w, crate::console::RingCue>,
    sfx: MessageWriter<'w, crate::audio::PlaySfx>,
    shouts: MessageWriter<'w, crate::present::shout::ShoutCast>,
}

impl KeyFire<'_> {
    /// The local player's powers (held priority for a bound held move).
    pub fn local_power(&self) -> Option<&PlayerPower> {
        let a = self.active.as_ref()?;
        a.local.and_then(|id| a.world.player(id)).map(|p| &p.power)
    }

    /// Send `steps` as the ordinary `Cmd::Toolbox` (what the phrase sends), aimed at the crosshair.
    pub fn fire_steps(&mut self, steps: Vec<ToolStep>, text: &str) -> bool {
        if self.active.as_ref().and_then(|a| a.local).is_none() {
            return false;
        }
        self.cues.write(crate::console::RingCue::Forming { voice: false });
        self.sfx.write(crate::audio::PlaySfx::new("cast_confirm").gain(0.55));
        self.shouts.write(crate::present::shout::ShoutCast { text: text.to_string(), spoken: false });
        let (eye, dir) = match self.aim_ray.as_ref() {
            Some(r) if r.dir.is_finite() && r.dir.length_squared() > 0.5 => (r.origin, r.dir),
            _ => (self.anchor.eye, self.look.forward()),
        };
        self.cmds.write(SimCommand::local(Cmd::Toolbox { steps, aim: Some(bd_sim::Aim::new(core_v3(eye), core_v3(dir))) }));
        true
    }
}

/// Keys 1–4 (console closed, Tab not held): fire the slot. (Under Tab they edit: `hud`; right
/// after the dismiss key they pick what to dismiss: `dismiss`.)
#[allow(clippy::too_many_arguments)]
pub(crate) fn slot_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    console: Res<RingConsole>,
    oath: Res<crate::oath::OathState>,
    focus: Res<crate::input::UiFocus>,
    party: Option<Res<crate::lobby::party::PartyUi>>,
    edit: Res<super::hud::TabEdit>,
    mut bar: ResMut<Hotbar>,
    mut fire: ParamSet<(Caster, KeyFire)>,
) {
    if console.open || oath.open || focus.typing || edit.on || !gameplay(state.get(), &menu) || party.as_ref().is_some_and(|p| p.phase == Some(bd_sim::party::PartyPhase::Pick)) {
        return;
    }
    for (i, a) in [Action::Slot1, Action::Slot2, Action::Slot3, Action::Slot4].into_iter().enumerate() {
        if binds.just_pressed(a, &keys, &mouse) {
            fire_slot(&mut bar, i, &mut fire);
        }
    }
}

/// Fire slot `i` through the normal path.
pub fn fire_slot(bar: &mut Hotbar, i: usize, fire: &mut ParamSet<(Caster, KeyFire)>) -> bool {
    let Some(s) = bar.slots.get(i) else { return false };
    if s.is_empty() {
        return false;
    }
    let phrase = s.phrase.clone();
    // the stored reading: the exact command the phrase sends (FIRST CALL / BURN ON DEATH gate it on
    // the host like any toolbox step — a claimed thing comes out as your rival version, a burned one
    // as its echo); phrases (constructs, signatures) cast as said
    let step = step_for(&s.bind, fire.p1().local_power());
    bar.touch(i);
    match step {
        Some(st) => fire.p1().fire_steps(vec![st], &phrase),
        None => fire.p0().cast(&phrase, false, 0),
    }
}

/// Phrases loaded from prefs (and Customize edits) get their reading once the lexicon is ready.
fn derive_readings(lex: Option<Res<RingLexicon>>, active: Option<Res<ActiveWorld>>, mut bar: ResMut<Hotbar>) {
    if bar.slots.iter().all(|s| s.read) {
        return;
    }
    let (Some(lex), Some(a)) = (lex, active) else { return };
    let local = a.local;
    let held = local.and_then(|id| a.world.player(id)).map(|p| &p.power);
    let powers_on = a.world.cfg.rules.powers_enabled;
    let roster: Vec<(PlayerId, &str)> = a.world.players.iter().filter(|p| Some(p.id) != local).map(|p| (p.id, p.name.as_str())).collect();
    for s in bar.slots.iter_mut().filter(|s| !s.read) {
        s.read = true;
        if s.phrase.trim().is_empty() {
            s.bind = SlotBind::Empty;
            continue;
        }
        let steps = crate::toolbox::command_for(&lex.0, held, powers_on, &s.phrase, false, &roster);
        s.bind = match steps.as_deref().and_then(|st| bind_of(st, held)) {
            Some(b) => b,
            None => SlotBind::Phrase { signature: lex.0.resolve(&s.phrase).signature.is_some() },
        };
        (s.def, s.power) = present(&s.bind);
        // DISMISS BY SLOT: a phrase that grants a power ("star platinum") lets it go on "x → k"
        s.granted = super::dismiss::granted_power(&lex.0, &s.phrase, &s.bind);
        if let Some(id) = s.granted {
            s.power = PowerTable::get().def(id); // the power's look on its slot
        }
    }
}

/// AUTO-BIND: pair each sent line with the host's confirmation.
#[allow(clippy::too_many_arguments)]
fn intake(
    time: Res<Time<Real>>,
    mut lines: MessageReader<LineSent>,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    prefs: Res<UiPrefs>,
    mut pending: ResMut<Pending>,
    mut bar: ResMut<Hotbar>,
    mut sfx: MessageWriter<crate::audio::PlaySfx>,
) {
    let dt = time.delta_secs().min(0.1);
    bar.tick(dt);
    let local = active.as_ref().and_then(|a| a.local);
    let held = active.as_ref().and_then(|a| local.and_then(|id| a.world.player(id))).map(|p| &p.power);
    for l in lines.read() {
        pending.twisted = None;
        pending.line = None;
        pending.signature = None;
        pending.grant = None;
        if l.signature {
            pending.signature = Some((l.text.clone(), 0.0));
            pending.spawn_for = Some((super::identity(&l.text, &SlotBind::Phrase { signature: true }), 0.0));
            continue;
        }
        match l.steps.as_slice() {
            [step] if !matches!(step, ToolStep::Cast { .. }) => {
                pending.line = Some((l.text.clone(), step.clone(), 0.0));
                pending.spawn_for = bind_of(&l.steps, held).map(|b| (super::identity(&l.text, &b), 0.0));
            }
            // a plain phrase (a construct, or a power's name) — its construct belongs to the slot with
            // that phrase; a power it grants binds it
            [] => {
                pending.spawn_for = Some((super::identity(&l.text, &SlotBind::Phrase { signature: false }), 0.0));
                pending.grant = Some((l.text.clone(), 0.0));
            }
            _ => pending.spawn_for = None,
        }
    }
    let age = |p: &mut Option<(String, ToolStep, f32)>| {
        if let Some(x) = p.as_mut() {
            x.2 += dt;
            if x.2 > CONFIRM_SECS {
                *p = None;
            }
        }
    };
    age(&mut pending.line);
    if let Some(s) = pending.signature.as_mut() {
        s.1 += dt;
        if s.1 > CONFIRM_SECS {
            pending.signature = None;
        }
    }
    if let Some(s) = pending.grant.as_mut() {
        s.1 += dt;
        if s.1 > CONFIRM_SECS {
            pending.grant = None;
        }
    }
    if let Some(s) = pending.spawn_for.as_mut() {
        s.1 += dt;
        if s.1 > CONFIRM_SECS {
            pending.spawn_for = None;
        }
    }
    let Some(me) = local else {
        events.clear();
        return;
    };
    for ev in events.read() {
        match &ev.event {
            GameEvent::Power(PowerEvent::Op(bd_sim::powers::ops::OpEvent::Twisted { player, a, b, mods, .. })) if *player == me => {
                pending.twisted = twist::exec::interned(*a, *b, mods);
            }
            GameEvent::Power(PowerEvent::Fired { player, mv, how, .. }) if *player == me => {
                let twisted = pending.twisted.take();
                let confirmed = match pending.line.as_ref() {
                    Some((text, step, _)) => match (step, how) {
                        (ToolStep::Move { mv: m, kit }, FireHow::Borrowed | FireHow::Kit) if m == mv && twisted.is_none() => Some((text.clone(), SlotBind::Move { mv: *m, kit: *kit })),
                        (ToolStep::Held(_), FireHow::Held) if twisted.is_none() => Some((text.clone(), SlotBind::Move { mv: *mv, kit: false })),
                        (ToolStep::Composed(c), FireHow::Composed) if twisted.is_none() => Some((text.clone(), SlotBind::Composed(*c))),
                        (ToolStep::Twist(_), _) if twisted.is_some() => bind_of(std::slice::from_ref(step), held).map(|b| (text.clone(), b)),
                        _ => None,
                    },
                    None => None,
                };
                if let Some((text, bind)) = confirmed {
                    pending.line = None;
                    let (def, power) = present(&bind);
                    let e = Entry { def: twisted.or(def), power, ..Entry::new(&text, bind) };
                    confirm(&mut bar, &e, prefs.auto_bind);
                } else {
                    // Not a line of ours (a G move, a chain link, a key press): flare its slot if it's on the bar.
                    let bind = SlotBind::Move { mv: *mv, kit: *how == FireHow::Kit };
                    if let Some(i) = bar.slots.iter().position(|s| s.bind.move_ref() == Some(*mv) || s.bind == bind) {
                        bar.flash[i] = 1.0;
                    }
                }
            }
            GameEvent::ConstructSpawned { id, owner, .. } if *owner == me => {
                if let Some((text, _)) = pending.signature.take() {
                    let e = Entry::new(&text, SlotBind::Phrase { signature: true });
                    confirm(&mut bar, &e, prefs.auto_bind);
                }
                if let Some((ident, _)) = pending.spawn_for
                    && let Some(i) = bar.slots.iter().position(|s| super::identity(&s.phrase, &s.bind) == ident)
                {
                    let live = &mut bar.slots[i].live;
                    live.retain(|c| active.as_ref().is_some_and(|a| a.world.constructs.get(*c).is_some()));
                    if !live.contains(id) {
                        live.push(*id);
                    }
                    bar.flash[i] = bar.flash[i].max(0.8);
                }
            }
            // a power's name said → you hold it now: a slot calls it back (and DISMISS BY SLOT lets it go)
            GameEvent::Power(PowerEvent::Gained { player, power, .. }) if *player == me => {
                if let Some((text, _)) = pending.grant.take() {
                    let e = Entry { power: PowerTable::get().def(*power), ..Entry::new(&text, SlotBind::Phrase { signature: false }) };
                    if let Some(i) = confirm(&mut bar, &e, prefs.auto_bind) {
                        bar.slots[i].granted = Some(*power);
                        bar.slots[i].power = e.power;
                    }
                }
            }
            GameEvent::CastFailed { player, reason } if *player == me => {
                pending.signature = None;
                // TOTAL LIMIT: refused at the cap (free) — the ring sputters, live slots + the limit
                // pips pulse toward the dismiss key (hud), a soft "full" sound
                if *reason == bd_sim::events::CastError::AtLimit {
                    if bar.refused < 0.5 {
                        sfx.write(crate::audio::PlaySfx::new("sputter").gain(0.5).pitch(0.8));
                    }
                    bar.refused = 1.0;
                }
            }
            _ => {}
        }
    }
}

/// A move the host confirmed: tray always; the bar when AUTO-BIND is on (else only a refresh of a
/// slot that already has it).
pub fn confirm(bar: &mut Hotbar, e: &Entry, auto: bool) -> Option<usize> {
    let slot = if auto {
        bar.auto_bind(e).slot()
    } else {
        bar.note_recent(e);
        let i = bar.find(&e.phrase, &e.bind);
        if let Some(i) = i {
            bar.touch(i);
        }
        i
    };
    if let Some(i) = slot {
        bar.flash[i] = 1.0;
    }
    slot
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::corps::Corps;
    use bd_core::Vec3;
    use bd_lexicon::Lexicon;
    use bd_sim::world::{ArenaSource, SimConfig, World};

    const ME: PlayerId = PlayerId(1);

    fn duel() -> World {
        let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, ..SimConfig::default() };
        let mut w = World::new(cfg);
        for k in 1..=2u8 {
            w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
        }
        w.start_live();
        w.tick(&[]);
        for (k, p) in w.players.iter_mut().enumerate() {
            p.spawn_shield = 0.0;
            p.mv.pos = Vec3::new(0.0, 8.0, -4.0 * k as f32);
            p.mv.vel = Vec3::ZERO;
            p.willpower = 100.0;
        }
        w.tick(&[]);
        w
    }

    fn lex() -> Option<Lexicon> {
        Lexicon::load(crate::assets_dir().join("lexicon.bin")).ok()
    }

    /// Everything the host did for one command: sparks spent, the fired moves and the cooldown row.
    fn run(w: &mut World, steps: Vec<ToolStep>) -> (f32, Vec<(MoveRef, FireHow)>) {
        let before = w.player(ME).unwrap().willpower;
        let aim = bd_sim::Aim::new(w.player(ME).unwrap().eye(w.params()), Vec3::NEG_Z);
        let _ = w.toolbox(ME, steps, Some(aim));
        let mut fired = Vec::new();
        for _ in 0..(bd_core::TICK_HZ as usize) {
            for e in w.tick(&[]) {
                if let GameEvent::Power(PowerEvent::Fired { player, mv, how, .. }) = e
                    && *player == ME
                {
                    fired.push((*mv, *how));
                }
            }
        }
        (before - w.player(ME).unwrap().willpower, fired)
    }

    /// The owner's example end to end: "reinhardt ultimate" said, confirmed, bound to 4; pressing 4
    /// sends the SAME command, costs the SAME sparks (+ the same staleness on a repeat) and cools the
    /// SAME cooldown — typed or spoken, key or voice.
    #[test]
    fn key_fire_is_spoken_fire_cost_cooldown_staleness() {
        let Some(lex) = lex() else { return };
        for (text, spoken) in [("reinhardt ultimate", false), ("rein hart ultimate.", true), ("bamf", false), ("fire uppercut", false)] {
            let mut said = duel();
            let held = said.player(ME).map(|p| p.power.clone());
            let steps = crate::toolbox::command_for(&lex, held.as_ref(), true, text, spoken, &[]).unwrap_or_else(|| panic!("{text:?} is a move"));
            let bind = bind_of(&steps, held.as_ref()).unwrap_or_else(|| panic!("{text:?} binds"));
            // the bar: a fresh default loadout → slot 4
            let mut bar = Hotbar::from_prefs(&["hammer".into(), "shield".into(), "wings".into(), "rocket launcher".into()], &[false; 4]);
            assert_eq!(confirm(&mut bar, &Entry::new(text, bind.clone()), true), Some(3), "{text:?} lands on 4");
            let key = step_for(&bar.slots[3].bind, held.as_ref()).unwrap();
            assert_eq!(vec![key.clone()], steps, "{text:?}: the key sends what the phrase sends");
            // run both twice in fresh worlds: identical price, staleness and cooldown
            let mut keyed = duel();
            let a1 = run(&mut said, steps.clone());
            let b1 = run(&mut keyed, vec![key.clone()]);
            assert_eq!(a1, b1, "{text:?}: first fire");
            assert!(!a1.1.is_empty(), "{text:?}: it fired");
            let wait = |w: &mut World| {
                for _ in 0..(bd_core::TICK_HZ as usize * 25) {
                    w.tick(&[]);
                    w.player_mut(ME).unwrap().willpower = 100.0;
                }
            };
            wait(&mut said);
            wait(&mut keyed);
            let a2 = run(&mut said, steps.clone());
            let b2 = run(&mut keyed, vec![key]);
            assert_eq!(a2, b2, "{text:?}: the repeat (staleness) prices the same");
            let snap = |w: &World| w.toolbox_view(ME).map(|s| s.cds);
            assert_eq!(snap(&said), snap(&keyed), "{text:?}: same cooldown / repeat row");
        }
    }

    /// A key press can't dodge a cooldown: pressed again at once, nothing fires and nothing is paid.
    #[test]
    fn key_spam_hits_the_cooldown_and_pays_nothing() {
        let mv = MoveIndex::shipped().shout(&bd_sim::powers::norm_words("earthshatter")).expect("earthshatter");
        let step = step_for(&SlotBind::Move { mv, kit: false }, None).unwrap();
        let mut w = duel();
        let (paid, fired) = run(&mut w, vec![step.clone()]);
        assert!(paid > 0.0 && fired.len() == 1);
        let (paid2, fired2) = run(&mut w, vec![step]);
        assert!(paid2 <= 0.0 && fired2.is_empty(), "cooling: refused, free ({paid2}, {fired2:?})");
    }

    /// A held power's move bound to a slot fires as the HELD move while held (its own cooldowns; never a
    /// second, borrowed cooldown to double-dip), and is borrowed once the power is gone.
    #[test]
    fn held_moves_fire_held_while_held() {
        let mut w = duel();
        let key = PowerTable::get().powers.iter().find(|p| p.moves.len() >= 2 && matches!(p.kind, bd_sim::powers::PowerKind::Paramecia) && p.transform.is_none()).map(|p| p.id.clone()).expect("a power with moves");
        assert!(w.grant_power(ME, &key));
        let pw = w.player(ME).unwrap().power.clone();
        let r = held_ref(&pw, 1).expect("slot 1 of the active power");
        assert_eq!(held_sel(&pw, r), Some(1));
        assert_eq!(step_for(&SlotBind::Move { mv: r, kit: false }, Some(&pw)), Some(ToolStep::Held(1)));
        assert_eq!(bind_of(&[ToolStep::Held(1)], Some(&pw)), Some(SlotBind::Move { mv: r, kit: false }));
        assert_eq!(step_for(&SlotBind::Move { mv: r, kit: false }, None), Some(ToolStep::Move { mv: r, kit: false }), "not held: borrowed");
    }

    #[test]
    fn auto_bind_off_only_feeds_the_tray() {
        let mut bar = Hotbar::from_prefs(&["hammer".into(), String::new(), String::new(), String::new()], &[false; 4]);
        let e = Entry::new("bamf", SlotBind::Move { mv: MoveRef::new(5, 0, 0), kit: false });
        assert_eq!(confirm(&mut bar, &e, false), None);
        assert!(bar.slots[1].is_empty());
        assert_eq!(bar.tray(&[], 8), vec![e.clone()]);
        // on: the first empty slot
        assert_eq!(confirm(&mut bar, &e, true), Some(1));
    }

    /// Chains and plain constructs never bind; described / twisted moves do.
    #[test]
    fn only_single_moves_bind() {
        let Some(lex) = lex() else { return };
        let read = |t: &str| crate::toolbox::command_for(&lex, None, true, t, false, &[]).and_then(|s| bind_of(&s, None));
        assert!(read("a dragon").is_none(), "a construct");
        assert!(read("bamf -> earthshatter").is_none(), "a chain");
        assert!(matches!(read("shadow teleport behind him"), Some(SlotBind::Composed(_))), "{:?}", read("shadow teleport behind him"));
        assert!(matches!(read("homing rasengan"), Some(SlotBind::Twist(_))), "{:?}", read("homing rasengan"));
        assert!(matches!(read("doomfist punch"), Some(SlotBind::Move { kit: true, .. })), "{:?}", read("doomfist punch"));
    }
}
