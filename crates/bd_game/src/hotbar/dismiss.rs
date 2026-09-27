//! DISMISS BY SLOT (owner: "press dismiss key then ability key — if star platinum is on 3 and my
//! dismiss is x, I press x->3 to dismiss star platinum").
//!
//! * The DISMISS key (X, `Action::Dismiss` — the existing "dismiss every construct" key) opens a
//!   short window (≈1 s; the live slots pulse) when any slot has something out. A slot key in the
//!   window dismisses THAT slot's live things (its constructs; a power it granted is let go); X again
//!   dismisses everything; the window running out does what X always did (everything). Opening the
//!   ring / Tab cancels it. With nothing live on the bar, X acts at once, as before.
//! * Voice / typing: "dismiss 3", "dismiss slot three", "dismiss star platinum", "dismiss" /
//!   "dismiss everything" ([`voice_dismiss`], read by the console before anything else).
//! * Each dismissed thing plays its clean exit (the sim's `GoneReason::Dismissed`).

use super::{norm, Hotbar, SlotBind, SLOTS};
use crate::lobby::prefs::{Action, KeyBinds};
use crate::sim_bridge::{ActiveWorld, Cmd, SimCommand};
use crate::state::{AppState, InGameMenu};
use bevy::prelude::*;

/// How long the window waits for the slot key.
pub const WINDOW: f32 = 1.0;

/// What to dismiss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    Slot(usize),
    All,
}

/// A dismiss line said / typed ("dismiss 3"), sent by the ring (`console::Caster`) instead of a cast.
#[derive(Message, Clone, Debug)]
pub struct DismissReq(pub String);

/// The open window (seconds left).
#[derive(Resource, Default, Debug)]
pub struct DismissWindow {
    pub left: Option<f32>,
}

impl DismissWindow {
    pub fn open(&self) -> bool {
        self.left.is_some()
    }
}

pub fn build(app: &mut App) {
    app.init_resource::<DismissWindow>()
        .add_message::<DismissReq>()
        .add_systems(Update, dismiss_keys.in_set(crate::lobby::UiSet::Input).after(super::hud::tab_edit).before(super::fire::slot_keys));
}

const WORDS: [&str; 4] = ["dismiss", "dismissed", "unsummon", "unsummoned"];

/// Is this line a dismiss ("dismiss …" / "unsummon …")? The ring sends it here instead of casting.
pub fn is_dismiss_line(text: &str) -> bool {
    let n = norm(text);
    WORDS.contains(&n.split(' ').next().unwrap_or(""))
}

/// "dismiss 3" / "dismiss slot three" / "dismiss star platinum" / "dismiss everything" / "dismiss".
/// `None`: not a dismiss line.
pub fn voice_dismiss(text: &str, bar: &Hotbar) -> Option<What> {
    let n = norm(text);
    let (head, rest) = n.split_once(' ').unwrap_or((n.as_str(), ""));
    if !WORDS.contains(&head) {
        return None;
    }
    let rest = rest.strip_prefix("the ").unwrap_or(rest);
    let rest = rest.strip_prefix("slot ").or_else(|| rest.strip_prefix("number ")).or_else(|| rest.strip_prefix("key ")).unwrap_or(rest).trim();
    if rest.is_empty() || matches!(rest, "all" | "everything" | "all of them" | "it all" | "them all") {
        return Some(What::All);
    }
    let digit = match rest {
        "1" | "one" | "won" | "first" => Some(0),
        "2" | "two" | "to" | "too" | "second" => Some(1),
        "3" | "three" | "tree" | "third" => Some(2),
        "4" | "four" | "for" | "fore" | "fourth" => Some(3),
        _ => None,
    };
    if let Some(k) = digit {
        return Some(What::Slot(k));
    }
    // a slot's phrase / move / power name
    let named = |k: usize| {
        let s = &bar.slots[k];
        let mut names: Vec<String> = vec![norm(&s.phrase)];
        if let Some(m) = s.def {
            names.push(norm(&m.name));
        }
        if let Some(p) = s.power {
            names.push(norm(&p.name));
            names.extend(p.names.iter().map(|x| norm(x)));
        }
        names.iter().any(|x| !x.is_empty() && (x == rest || x.contains(rest) && rest.len() >= 4))
    };
    (0..SLOTS).find(|&k| !bar.slots[k].is_empty() && named(k)).map(What::Slot)
}

/// Does slot `k` have something out right now?
pub fn slot_live(bar: &Hotbar, k: usize, a: &ActiveWorld) -> bool {
    let s = &bar.slots[k];
    s.live.iter().any(|c| a.world.constructs.get(*c).is_some_and(|c| c.gone.is_none()))
        || s.granted.is_some_and(|id| a.local_player().is_some_and(|p| p.power.slot_of(id).is_some()))
}

/// The commands that dismiss `what` (constructs one by one; a granted power let go).
pub fn commands_for(bar: &Hotbar, what: What, a: &ActiveWorld) -> Vec<Cmd> {
    match what {
        What::All => vec![Cmd::Dismiss],
        What::Slot(k) => {
            let s = &bar.slots[k];
            let mut out: Vec<Cmd> = s.live.iter().filter(|c| a.world.constructs.get(**c).is_some_and(|c| c.gone.is_none())).map(|c| Cmd::DismissOne(*c)).collect();
            if let Some(id) = s.granted.filter(|id| a.local_player().is_some_and(|p| p.power.slot_of(*id).is_some())) {
                out.push(Cmd::DropPower(id));
            }
            out
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dismiss_keys(
    time: Res<Time<Real>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    focus: Res<crate::input::UiFocus>,
    console: Res<crate::console::RingConsole>,
    edit: Res<super::hud::TabEdit>,
    active: Option<Res<ActiveWorld>>,
    mut reqs: MessageReader<DismissReq>,
    mut win: ResMut<DismissWindow>,
    mut bar: ResMut<Hotbar>,
    mut cmds: MessageWriter<SimCommand>,
    mut sfx: MessageWriter<crate::audio::PlaySfx>,
) {
    let Some(a) = active.as_deref() else {
        win.left = None;
        reqs.clear();
        return;
    };
    let alive = a.local_player().is_some_and(|p| p.alive);
    let live_game = *state.get() == AppState::InGame && !menu.0 && alive;
    let send = |bar: &mut Hotbar, what: What, cmds: &mut MessageWriter<SimCommand>| {
        for c in commands_for(bar, what, a) {
            cmds.write(SimCommand::local(c));
        }
        if let What::Slot(k) = what {
            bar.slots[k].live.clear();
            bar.flash[k] = 1.0;
        }
    };
    for r in reqs.read() {
        // "dismiss <something not on the bar>": nothing to do (never a surprise dismiss-all)
        if live_game && let Some(what) = voice_dismiss(&r.0, &bar) {
            send(&mut bar, what, &mut cmds);
        }
    }
    if !live_game || focus.typing || console.open || edit.on {
        win.left = None;
        return;
    }
    if binds.just_pressed(Action::Dismiss, &keys, &mouse) {
        if win.open() {
            win.left = None;
            send(&mut bar, What::All, &mut cmds);
        } else if (0..SLOTS).any(|k| slot_live(&bar, k, a)) {
            win.left = Some(WINDOW);
            sfx.write(crate::audio::PlaySfx::new("needle_tick").gain(0.35).pitch(0.8));
        } else {
            send(&mut bar, What::All, &mut cmds);
        }
        return;
    }
    let Some(left) = win.left else { return };
    for (k, act) in [Action::Slot1, Action::Slot2, Action::Slot3, Action::Slot4].into_iter().enumerate() {
        if binds.just_pressed(act, &keys, &mouse) {
            // the slot key belongs to the dismiss: it must not also fire the slot
            if let Some(key) = binds.get(act).key() {
                keys.clear_just_pressed(key);
            }
            win.left = None;
            send(&mut bar, What::Slot(k), &mut cmds);
            return;
        }
    }
    let left = left - time.delta_secs();
    if left <= 0.0 {
        // nothing chosen: X did what it always did
        win.left = None;
        send(&mut bar, What::All, &mut cmds);
    } else {
        win.left = Some(left);
    }
}

/// Slots holding a Phrase that grants a power remember it (so "x → 3" lets Star Platinum go).
pub fn granted_power(lex: &bd_lexicon::Lexicon, phrase: &str, bind: &SlotBind) -> Option<u16> {
    if !matches!(bind, SlotBind::Phrase { .. }) {
        return None;
    }
    let g = lex.resolve(phrase).recipe.grants?;
    let key = g.split('@').next().unwrap_or(&g);
    bd_sim::powers::PowerTable::get().index_of(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_dismiss_reads_numbers_names_and_all() {
        let l = ["hammer".to_string(), "shield".into(), "star platinum".into(), "rocket launcher".into()];
        let bar = Hotbar::from_prefs(&l, &[false; 4]);
        assert_eq!(voice_dismiss("dismiss 3", &bar), Some(What::Slot(2)));
        assert_eq!(voice_dismiss("Dismiss slot three.", &bar), Some(What::Slot(2)));
        assert_eq!(voice_dismiss("dismiss number four", &bar), Some(What::Slot(3)));
        assert_eq!(voice_dismiss("dismiss star platinum", &bar), Some(What::Slot(2)));
        assert_eq!(voice_dismiss("dismiss the rocket launcher", &bar), Some(What::Slot(3)));
        assert_eq!(voice_dismiss("dismiss", &bar), Some(What::All));
        assert_eq!(voice_dismiss("dismiss everything!", &bar), Some(What::All));
        assert_eq!(voice_dismiss("dismissed", &bar), Some(What::All));
        assert_eq!(voice_dismiss("a dismissive hammer", &bar), None);
        assert_eq!(voice_dismiss("hammer", &bar), None);
        assert_eq!(voice_dismiss("dismiss the kraken", &bar), None, "nothing on the bar by that name");
    }
}
