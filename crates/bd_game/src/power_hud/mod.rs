//! POWER HUD — what YOU have and how ready it is, with no words (owner: "users should have a way to
//! know what powers are active / their cooldowns"). Mystery tenet: it never lists what is possible,
//! only what you hold; teach by feel: readable at a glance mid-fight.
//!
//! Two layers show ONE state ([`PowerHudState`], derived in [`derive`] from the replicated
//! `PowerSnap` + `bd_sim::readiness::ReadySnap` + the recipient's `OwnHud`, so practice, host and
//! client are identical, and voice and typing are identical — it reads the sim, not the input):
//! * **Diegetic** ([`gems`], [`motes`]): first person, one glowing gem per held power rides the ring
//!   hand in that power's aura colour — it dims and drains when its move fires, refills visibly and
//!   FLARES with a soft ready chime pitched per power (an E-minor pentatonic step, so the set is
//!   learned by ear); an active form burns, with a draining ring for its time left. Third person and
//!   for OTHER players: small orbiting motes, one per held power, dim while cooling (opponents read
//!   your readiness — telegraphed power).
//! * **Minimal strip** ([`strip`], bottom centre, prefs toggle `UiPrefs::power_hud`, on by default):
//!   one icon per held power (colour + a glyph derived from its kind: never a name), radial cooldown
//!   sweep, ready flare, form duration ring; the slim willpower arc (+ a cost flicker for the phrase
//!   being typed); the carried map pickup; your statuses as small pips with duration sweeps. Hidden
//!   when you hold nothing; a soft pop the first time you gain a power.
//!
//! **Single source for the wordless-affordance pass:** the ring gauge that pass adds (willpower =
//! ring glow) should read [`PowerHudState`] (`willpower`, `cost`, `gems`) and extend [`gems`] rather
//! than derive its own copy.

pub mod derive;
mod gems;
mod motes;
pub mod scenario;
mod strip;

pub use derive::PowerHud;
/// The icon material (the Tab board's power chips reuse it).
pub use strip::{IconMaterial, IconParams};

use crate::console::{RingConsole, RingLexicon};
use crate::sim_bridge::{ActiveWorld, BridgeSet, ViewKeys};
use bd_sim::pickups::PickupPhase;
use bd_sim::powers::{move_sel, PowerPhrase};
use bd_sim::replicate::pflags;
use bevy::prelude::*;

/// The one derived power-HUD state (see the module docs). Other presenters read it.
#[derive(Resource, Default)]
pub struct PowerHudState(pub PowerHud);

/// Sound the ready chime is played with (pitched per power).
pub const READY_CHIME: &str = "power_ready";

pub struct PowerHudPlugin;

impl Plugin for PowerHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PowerHudState>()
            .init_resource::<CostPreview>()
            .add_message::<crate::audio::PlaySfx>()
            .add_systems(Update, (derive_state, cost_preview).chain().after(BridgeSet::Views).in_set(crate::present::PresentSet::Events));
        strip::build(app);
        gems::build(app);
        motes::build(app);
        scenario::build(app);
    }
}

/// Cost preview of the phrase in the ring console (typed; a spoken phrase has no text until it is
/// cast, so voice has no preview — the arc still drains identically after either).
#[derive(Resource, Default)]
struct CostPreview {
    hash: u64,
    since: f32,
    value: Option<f32>,
}

fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn derive_state(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    keys: Res<ViewKeys>,
    mut st: ResMut<PowerHudState>,
    mut sfx: MessageWriter<crate::audio::PlaySfx>,
) {
    let dt = time.delta_secs();
    let hud = &mut st.0;
    let me = active.as_ref().and_then(|a| a.local).filter(|_| keys.valid);
    let snap = &keys.curr;
    let row = me.and_then(|id| snap.players.binary_search_by_key(&id.0, |p| p.id).ok().map(|i| &snap.players[i])).filter(|p| p.has(pflags::ALIVE));
    let pool = active.as_ref().map_or(100.0, |a| a.world.rules().willpower_max()).max(1.0);
    let own = row.and(snap.own.as_ref()).and_then(|o| o.hud);
    let size = row.map_or(1.0, |p| p.size_scale());
    let edges = hud.update(row.map(|p| (&p.power, &p.ready, p.willpower as f32)), pool, own.as_ref(), size, dt);
    // The carried map pickup (+ its life left).
    hud.pickup = match (me, row, active.as_ref()) {
        (Some(id), Some(_), Some(a)) => snap.pickups.iter().find(|p| p.phase == PickupPhase::Held && p.holder == Some(id)).map(|p| {
            let life = a.world.cfg.tuning.pickups.held_life.max(0.1);
            let left = p.until.saturating_sub(snap.tick) as f32 * bd_core::DT;
            (p.kind, (left / life).clamp(0.0, 1.0))
        }),
        _ => None,
    };
    for k in 0..derive::SLOTS {
        if edges.ready & (1 << k) != 0 {
            let pitch = hud.gems[k].look.pitch;
            sfx.write(crate::audio::PlaySfx::new(READY_CHIME).pitch(pitch).gain(0.45));
        }
    }
}

/// Resolve the typed phrase's cost at most every 0.12 s, and only when the text changed.
fn cost_preview(
    time: Res<Time<Real>>,
    console: Option<Res<RingConsole>>,
    lex: Option<Res<RingLexicon>>,
    active: Option<Res<ActiveWorld>>,
    mut prev: ResMut<CostPreview>,
    mut st: ResMut<PowerHudState>,
) {
    prev.since += time.delta_secs();
    let (Some(console), Some(lex), Some(a)) = (console, lex, active) else {
        st.0.cost = None;
        return;
    };
    let text = console.buf.trim();
    if !console.open || text.is_empty() {
        prev.hash = 0;
        prev.value = None;
        st.0.cost = None;
        return;
    }
    let h = hash_str(text);
    if h != prev.hash && prev.since >= 0.12 {
        prev.hash = h;
        prev.since = 0.0;
        prev.value = a.local.and_then(|me| phrase_cost(&a, &lex, me, text));
    }
    st.0.cost = prev.value;
}

fn phrase_cost(a: &ActiveWorld, lex: &RingLexicon, me: bd_core::PlayerId, text: &str) -> Option<f32> {
    let w = &a.world;
    let pool = w.rules().willpower_max().max(1.0);
    let p = w.player(me)?;
    let cost = match w.parse_power_phrase(me, text) {
        PowerPhrase::Move(slot) => {
            if slot & move_sel::SELECT != 0 || slot == move_sel::CYCLE {
                0.0
            } else {
                let (held, s) = if slot & move_sel::MOVE_OF != 0 { (((slot >> 2) & 3) as usize, slot & 3) } else { (0, slot) };
                let h = p.power.held().find(|h| h.slot == held)?;
                h.def.moves_at(h.tier).get(s as usize).map_or(0.0, |m| m.cost)
            }
        }
        PowerPhrase::Incantation { rest } => w.preview_cost(me, lex.0.resolve(&rest).recipe).1,
        PowerPhrase::Plain => w.preview_cost(me, lex.0.resolve(text).recipe).1,
    };
    (cost > 0.0).then_some((cost / pool).clamp(0.0, 2.0))
}
