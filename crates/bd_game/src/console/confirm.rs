//! CLICK TO CONFIRM (gameplay option, `UiPrefs::click_to_confirm`, off by default).
//!
//! Owner: "the ring will show a more translucent preview of what it will spawn/cost, you click to
//! make that preview become solid/real".
//!
//! * **Commit → ghost.** Committing a phrase (Enter, a quick-cast slot, the end of an utterance)
//!   runs the ordinary cast path ([`Caster`]) with its commands *captured* instead of sent: nothing
//!   reaches the sim, the network, the shout letters or the logs. The capture is the [`Pending`]
//!   ghost the ring projects ([`ghost`]: the construct's own model, translucent in the corps colour
//!   at its real size and spot, following the crosshair; a move's telegraph; a chain's pips —
//!   [`hud`]) and the spark gauge bites what it will cost (`power_hud` cost preview).
//! * **Confirm → real.** Primary click, or a confirm word said / typed as the whole line
//!   ([`is_confirm_word`]: "go", "now", "confirm", "do it" — `UiPrefs::confirm_words`), re-runs the
//!   SAME path live with the same words at the crosshair of that moment (so the ghost is aimed until
//!   the click). The sim only ever sees the confirmed command — no sim change, purely client-side
//!   until then (online too). With the option on a confirm word is only ever a confirm (nothing
//!   pending: nothing happens).
//! * **Drop.** Secondary click, Esc (console closed), or a new phrase (it replaces the ghost). A
//!   ghost left alone fades out after [`PREVIEW_SECS`]; death / leaving the match / the option going
//!   off drop it. Movement, aim, tethers and typing never wait on it; the pause menu freezes it.
//! * Stand-rush tokens stay live input (never previewed).
//! * No explaining text anywhere (mystery tenet): the ghost, the gauge bite and a small click glyph.
//!
//! Dev: `BD_CONFIRM_TEST=phrase@secs;!confirm@secs;!cancel@secs;!yaw@deg@secs` (scripted frame
//! sheets; turn the option on with a `BD_UI_PREFS` file).

pub mod ghost;
pub mod hud;

use super::{Caster, RingConsole};
use crate::audio::PlaySfx;
use crate::input::CursorCapture;
use crate::lobby::UiSet;
use crate::sim_bridge::Cmd;
use crate::state::{AppState, InGameMenu};
use bevy::input::InputSystems;
use bevy::prelude::*;

/// The default confirm words (`UiPrefs::confirm_words`).
pub const DEFAULT_WORDS: &[&str] = &["go", "now", "confirm", "do it"];

/// A ghost nobody confirms fades out after this long (s).
pub const PREVIEW_SECS: f32 = 12.0;
/// …over its last this-many seconds.
pub const FADE_SECS: f32 = 1.2;

/// How a ghost ended (the presentation's exit beat).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// Clicked / "go": it became real (the construct forms where the ghost stood).
    Confirmed,
    /// Secondary click / Esc / death / the option off.
    Cancelled,
    /// A new phrase took its place.
    Replaced,
    /// Timed out.
    Expired,
}

/// A projected, unconfirmed phrase.
#[derive(Clone, Debug)]
pub struct Pending {
    pub text: String,
    pub spoken: bool,
    /// Stand-rush tokens of this phrase already sent live while typing.
    pub rush_already: u16,
    pub heard_conf: Option<f32>,
    /// A spoken phrase's peak mic level (a spoken signature's power), kept for the confirm.
    pub voice_peak: Option<f32>,
    /// What the phrase would send right now (captured through the real path, aimed at commit).
    pub cmds: Vec<Cmd>,
    /// Seconds since it was projected (paused while a menu is open).
    pub age: f32,
    pub serial: u32,
}

impl Pending {
    /// Chain pips: a toolbox line's steps (1 for anything else).
    pub fn steps(&self) -> usize {
        self.cmds
            .iter()
            .find_map(|c| match c {
                Cmd::Toolbox { steps, .. } => Some(steps.len()),
                _ => None,
            })
            .unwrap_or(1)
    }

    /// 1 → 0 over the last [`FADE_SECS`] before it times out.
    pub fn life(&self) -> f32 {
        ((PREVIEW_SECS - self.age) / FADE_SECS).clamp(0.0, 1.0)
    }
}

/// The ring's pending ghost (one at a time).
#[derive(Resource, Default)]
pub struct ConfirmGate {
    pub pending: Option<Pending>,
    /// The Caster is projecting: commands land here instead of the sim.
    pub(super) capture: Option<Vec<Cmd>>,
    /// A confirmed spoken phrase's peak mic level, while it re-runs.
    pub(super) peak: Option<f32>,
    serial: u32,
    /// The most recent ghost's end and its serial (the ghost's exit beat).
    pub ended: Option<(Ended, u32)>,
}

impl ConfirmGate {
    fn open(&mut self, mut p: Pending) {
        if let Some(old) = self.pending.take() {
            self.ended = Some((Ended::Replaced, old.serial));
        }
        self.serial = self.serial.wrapping_add(1).max(1);
        p.serial = self.serial;
        self.pending = Some(p);
    }

    fn end(&mut self, why: Ended) -> Option<Pending> {
        let p = self.pending.take()?;
        self.ended = Some((why, p.serial));
        Some(p)
    }
}

/// The whole line is a confirm word (case, punctuation and repeats ignored: "Go!", "go go", "do
/// it.", "go now"). Only consulted while a ghost is pending — otherwise the words cast as ever.
pub fn is_confirm_word(text: &str, words: &[String]) -> bool {
    let t = norm(text);
    if t.is_empty() {
        return false;
    }
    let listed = |s: &str| words.iter().any(|w| norm(w) == s);
    listed(&t) || t.split(' ').all(listed)
}

fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for w in s.split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(w.chars().filter(|c| *c != '\'').flat_map(char::to_lowercase));
    }
    out
}

/// A capture worth a ghost: anything but live stand-rush tokens (and the cast's words alone).
fn previewable(cmds: &[Cmd]) -> bool {
    cmds.iter().any(|c| !matches!(c, Cmd::StandRush(_) | Cmd::CastMeta(_)))
}

impl Caster<'_> {
    /// The ring's commit: confirm words confirm a pending ghost; with the option on a phrase is
    /// projected; otherwise it casts as ever.
    pub(super) fn cast_gated(&mut self, text: &str, spoken: bool, rush_already: u16, heard_conf: Option<f32>) -> bool {
        let t = text.trim();
        if t.is_empty() {
            return false;
        }
        let pending = self.gate.as_ref().is_some_and(|g| g.pending.is_some());
        if (pending || self.prefs.click_to_confirm) && is_confirm_word(t, &self.prefs.confirm_words) {
            // With the option on a confirm word is only ever a confirm: with nothing pending it does
            // nothing (never a ghost of whatever the lexicon would read into "confirm").
            return pending && self.confirm_pending();
        }
        if !self.prefs.click_to_confirm || self.gate.is_none() || self.capturing() {
            return self.cast_now(t, spoken, rush_already, heard_conf);
        }
        self.project(t, spoken, rush_already, heard_conf)
    }

    /// Run the cast path with its commands captured → the pending ghost.
    fn project(&mut self, text: &str, spoken: bool, rush_already: u16, heard_conf: Option<f32>) -> bool {
        if let Some(g) = self.gate.as_mut() {
            g.capture = Some(Vec::new());
        }
        let ok = self.cast_now(text, spoken, rush_already, heard_conf);
        let cmds = self.gate.as_mut().and_then(|g| g.capture.take()).unwrap_or_default();
        if !ok {
            return false;
        }
        if !previewable(&cmds) {
            // Stand-rush tokens are live input (typed ORAs go out as they complete): as ever.
            return self.cast_now(text, spoken, rush_already, heard_conf);
        }
        let voice_peak = spoken.then(|| self.voice_peak());
        let Some(g) = self.gate.as_mut() else { return false };
        g.open(Pending { text: text.to_string(), spoken, rush_already, heard_conf, voice_peak, cmds, age: 0.0, serial: 0 });
        // The projection's own soft note (the real "catch" plays on confirm).
        self.sfx.write(PlaySfx::new("cast_confirm").gain(0.16).pitch(1.45));
        true
    }

    /// Make the pending ghost real: the same words through the same path, live, at the crosshair
    /// of this moment. Returns false when nothing was pending.
    pub fn confirm_pending(&mut self) -> bool {
        let Some(p) = self.gate.as_mut().and_then(|g| g.end(Ended::Confirmed)) else { return false };
        if let Some(g) = self.gate.as_mut() {
            g.peak = p.voice_peak;
        }
        let ok = self.cast_now(&p.text, p.spoken, p.rush_already, p.heard_conf);
        if let Some(g) = self.gate.as_mut() {
            g.peak = None;
        }
        ok
    }

    /// Drop the pending ghost (nothing is sent). Returns false when nothing was pending.
    pub fn cancel_pending(&mut self, why: Ended) -> bool {
        let dropped = self.gate.as_mut().and_then(|g| g.end(why)).is_some();
        if dropped && why == Ended::Cancelled {
            self.sfx.write(PlaySfx::new("cast_confirm").gain(0.1).pitch(0.7));
        }
        dropped
    }

    /// The pending ghost, if any.
    pub fn pending(&self) -> Option<&Pending> {
        self.gate.as_ref().and_then(|g| g.pending.as_ref())
    }
}

pub struct ConfirmPlugin;

impl Plugin for ConfirmPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConfirmGate>()
            // Before the shell samples the mouse (the confirming click never also fires PRIMARY) and
            // before the pause menu reads Esc.
            .add_systems(PreUpdate, confirm_input.after(InputSystems))
            .add_systems(Update, (dev_turns, dev_script).chain().in_set(UiSet::Input).run_if(dev_on))
            .add_systems(OnExit(AppState::InGame), drop_on_exit);
        ghost::build(app);
        hud::build(app);
    }
}

fn drop_on_exit(mut gate: ResMut<ConfirmGate>) {
    gate.end(Ended::Cancelled);
    gate.capture = None;
}

/// Click / right-click / Esc, the timeout, and the drops (death, the option off, left the match).
#[allow(clippy::too_many_arguments)]
fn confirm_input(
    time: Res<Time<Real>>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    grab: Res<CursorCapture>,
    console: Res<RingConsole>,
    oath: Res<crate::oath::OathState>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut caster: Caster,
) {
    let Some(age) = caster.pending().map(|p| p.age) else { return };
    let alive = caster.active.as_ref().and_then(|a| a.local_player()).is_some_and(|p| p.alive);
    if *state.get() != AppState::InGame || !caster.prefs.click_to_confirm || !alive {
        caster.cancel_pending(Ended::Cancelled);
        return;
    }
    if menu.0 || oath.open {
        return; // paused: the ghost waits
    }
    let age = age + time.delta_secs();
    if age >= PREVIEW_SECS {
        caster.cancel_pending(Ended::Expired);
        return;
    }
    if let Some(p) = caster.gate.as_mut().and_then(|g| g.pending.as_mut()) {
        p.age = age;
    }
    if !grab.grabbed {
        return;
    }
    if mouse.just_pressed(MouseButton::Left) {
        // The whole click is the confirm: swallowed until released (no swing / shot with it).
        mouse.reset(MouseButton::Left);
        caster.confirm_pending();
    } else if mouse.just_pressed(MouseButton::Right) {
        mouse.reset(MouseButton::Right);
        caster.cancel_pending(Ended::Cancelled);
    } else if !console.open && keys.just_pressed(KeyCode::Escape) {
        // Esc drops the ghost instead of opening the pause menu this time.
        keys.clear_just_pressed(KeyCode::Escape);
        caster.cancel_pending(Ended::Cancelled);
    }
}

// ------------------------------------------------------------------------------------------ dev

fn dev_plan() -> &'static [(String, f32, f32)] {
    static PLAN: std::sync::OnceLock<Vec<(String, f32, f32)>> = std::sync::OnceLock::new();
    PLAN.get_or_init(|| {
        std::env::var("BD_CONFIRM_TEST")
            .unwrap_or_default()
            .split(';')
            .filter_map(|s| {
                let parts: Vec<&str> = s.split('@').map(str::trim).collect();
                match parts.as_slice() {
                    [p, t] => Some((p.to_string(), 0.0, t.parse().ok()?)),
                    [p, a, t] => Some((p.to_string(), a.parse().ok()?, t.parse().ok()?)),
                    _ => None,
                }
            })
            .collect()
    })
}

fn dev_on() -> bool {
    !dev_plan().is_empty()
}

/// `BD_CONFIRM_TEST` turns (`!yaw@deg@secs`, `!pitch@deg@secs`), seconds after entering the game.
fn dev_turns(time: Res<Time<Real>>, state: Res<State<AppState>>, mut look: ResMut<crate::input::LookAngles>, mut st: Local<(f32, usize)>) {
    if *state.get() != AppState::InGame {
        return;
    }
    st.0 += time.delta_secs();
    let plan = dev_plan();
    while let Some((what, arg, at)) = plan.get(st.1) {
        if st.0 < *at {
            break;
        }
        st.1 += 1;
        match what.as_str() {
            "!yaw" => look.yaw += arg.to_radians(),
            "!pitch" => look.pitch = (look.pitch + arg.to_radians()).clamp(-1.5, 1.5),
            _ => {}
        }
    }
}

/// `BD_CONFIRM_TEST`: scripted commits / confirms / cancels (seconds after entering the game).
fn dev_script(time: Res<Time<Real>>, state: Res<State<AppState>>, mut st: Local<(f32, usize)>, mut caster: Caster) {
    if *state.get() != AppState::InGame {
        return;
    }
    st.0 += time.delta_secs();
    if !caster.lexicon_ready() {
        return;
    }
    let plan = dev_plan();
    while let Some((what, _, at)) = plan.get(st.1) {
        if st.0 < *at {
            break;
        }
        st.1 += 1;
        match what.as_str() {
            "!confirm" => {
                caster.confirm_pending();
            }
            "!cancel" => {
                caster.cancel_pending(Ended::Cancelled);
            }
            w if w.starts_with('!') => {}
            phrase => {
                info!("confirm test: {phrase:?} at {:.1} s", st.0);
                caster.cast(phrase, false, 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words() -> Vec<String> {
        DEFAULT_WORDS.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn confirm_words_match_the_whole_line_only() {
        let w = words();
        for s in ["go", "Go!", "GO.", "now", "Confirm", "do it", "Do it!", "do it.", "go go", "go now", "  go  "] {
            assert!(is_confirm_word(s, &w), "{s:?}");
        }
        for s in ["", "go go gadget arm", "gojo", "go kart", "now you see me", "do it again", "confirmed kill", "dog", "summon a dragon", "going"] {
            assert!(!is_confirm_word(s, &w), "{s:?}");
        }
        // configurable
        let mine = vec!["engage".to_string()];
        assert!(is_confirm_word("Engage!", &mine) && !is_confirm_word("go", &mine));
    }

    /// A headless app with the real `Caster` (lexicon, a live world, prefs): `run` calls `f` once
    /// and returns the commands it sent (Debug strings: `Cmd` has no `PartialEq`).
    struct Rig {
        app: App,
    }

    impl Rig {
        fn new(lex: std::sync::Arc<bd_lexicon::Lexicon>, on: bool) -> Self {
            use crate::sim_bridge::{ActiveWorld, SimCommand, SimRole, WorldViews};
            // SAFETY: test-only; the intent log is a local playtest file.
            unsafe { std::env::set_var("BD_NO_INTENT_LOG", "1") };
            let cfg = bd_sim::SimConfig { arena: bd_sim::ArenaSource::Flat { size_m: 96.0 }, ..Default::default() };
            let mut w = bd_sim::World::new(cfg);
            let me = bd_core::PlayerId(1);
            w.add_player(me, "Caster".into(), bd_core::corps::Corps::from_index(1)).unwrap();
            w.add_player(bd_core::PlayerId(2), "Ethan".into(), bd_core::corps::Corps::from_index(2)).unwrap();
            w.start_live();
            w.tick(&[]);
            let mut app = App::new();
            app.add_message::<SimCommand>()
                .add_message::<super::super::RingCue>()
                .add_message::<PlaySfx>()
                .add_message::<crate::present::shout::ShoutCast>()
                .add_message::<crate::hotbar::fire::LineSent>()
                .add_message::<crate::hotbar::dismiss::DismissReq>()
                .insert_resource(super::super::RingLexicon(lex))
                .insert_resource(ActiveWorld::new(w, Some(me), SimRole::Offline))
                .insert_resource(WorldViews::default())
                .insert_resource(crate::input::LookAngles { yaw: 0.4, pitch: -0.1 })
                .insert_resource(crate::camera::ViewAnchor { eye: Vec3::new(0.0, 9.6, 0.0), velocity: Vec3::ZERO })
                .insert_resource(crate::lobby::prefs::UiPrefs { click_to_confirm: on, ..Default::default() })
                .init_resource::<ConfirmGate>();
            Rig { app }
        }

        fn run(&mut self, f: impl FnOnce(&mut Caster) + Send + Sync + 'static) -> Vec<String> {
            use bevy::ecs::system::RunSystemOnce;
            let f = std::sync::Mutex::new(Some(f));
            self.app
                .world_mut()
                .run_system_once(move |mut c: Caster| {
                    if let Some(f) = f.lock().unwrap().take() {
                        f(&mut c);
                    }
                })
                .unwrap();
            self.app.world_mut().resource_mut::<bevy::ecs::message::Messages<crate::sim_bridge::SimCommand>>().drain().map(|m| format!("{:?}", m.cmd)).collect()
        }

        fn gate(&self) -> &ConfirmGate {
            self.app.world().resource::<ConfirmGate>()
        }
    }

    fn lexicon() -> Option<std::sync::Arc<bd_lexicon::Lexicon>> {
        bd_lexicon::Lexicon::load(crate::assets_dir().join("lexicon.bin")).ok().map(std::sync::Arc::new)
    }

    /// Confirm path == direct path: the ghost sends nothing; the click sends exactly what the
    /// option-off cast sends (constructs, signatures, moves, chains, described / twisted moves,
    /// typed and spoken).
    #[test]
    fn confirming_sends_exactly_the_direct_cast() {
        let Some(lex) = lexicon() else { return };
        for (text, spoken) in [
            ("summon a dragon", false),
            ("a big flaming hammer", false),
            ("kamehameha", false),
            ("almighty push", false),
            ("summon a dragon, rasengan", false),
            ("bamf -> almighty push -> gum gum pistol", false),
            ("gravity punch", false),
            ("giant rasengan", false),
            ("summon a dragon", true),
            ("Almighty push!", true),
        ] {
            let direct = Rig::new(lex.clone(), false).run(move |c| {
                assert!(c.cast_heard(text, spoken, 0, None));
            });
            assert!(!direct.is_empty(), "{text:?}");
            let mut rig = Rig::new(lex.clone(), true);
            let ghost = rig.run(move |c| {
                assert!(c.cast_heard(text, spoken, 0, None));
            });
            assert!(ghost.is_empty(), "{text:?}: the ghost is client-side only: {ghost:?}");
            let p = rig.gate().pending.as_ref().expect("a pending ghost");
            assert_eq!(p.cmds.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(), direct, "{text:?}: the ghost previews the direct cast");
            let confirmed = rig.run(|c| {
                assert!(c.confirm_pending());
            });
            assert_eq!(confirmed, direct, "{text:?}: confirm == direct");
            assert!(rig.gate().pending.is_none());
            assert!(matches!(rig.gate().ended, Some((Ended::Confirmed, _))));
        }
    }

    /// Voice parity: a confirm word said (or typed) as the whole line confirms the pending ghost.
    #[test]
    fn a_spoken_confirm_word_confirms() {
        let Some(lex) = lexicon() else { return };
        let direct = Rig::new(lex.clone(), false).run(|c| {
            c.cast_heard("summon a dragon", true, 0, Some(0.9));
        });
        for word in ["Go.", "do it!", "Now", "confirm", "go go"] {
            for spoken in [true, false] {
                let mut rig = Rig::new(lex.clone(), true);
                let ghost = rig.run(|c| {
                    c.cast_heard("summon a dragon", true, 0, Some(0.9));
                });
                assert!(ghost.is_empty());
                let sent = rig.run(move |c| {
                    assert!(c.cast_heard(word, spoken, 0, None));
                });
                assert_eq!(sent, direct, "{word:?} (spoken {spoken})");
            }
        }
    }

    /// Cancel paths: secondary click / Esc (`cancel_pending`), a new phrase (replaces), nothing
    /// pending. Nothing is ever sent for a dropped ghost.
    #[test]
    fn dropped_ghosts_send_nothing() {
        let Some(lex) = lexicon() else { return };
        let mut rig = Rig::new(lex.clone(), true);
        let ghost = rig.run(|c| {
            c.cast("summon a dragon", false, 0);
        });
        assert!(ghost.is_empty());
        let dropped = rig.run(|c| {
            assert!(c.cancel_pending(Ended::Cancelled));
            assert!(!c.cancel_pending(Ended::Cancelled), "nothing left");
            assert!(!c.confirm_pending(), "nothing to confirm");
        });
        assert!(dropped.is_empty());
        assert!(matches!(rig.gate().ended, Some((Ended::Cancelled, _))));
        // a new phrase replaces the ghost; confirming casts the NEW one only
        let ghost = rig.run(|c| {
            c.cast("summon a dragon", false, 0);
            c.cast("a wall", false, 0);
        });
        assert!(ghost.is_empty());
        assert_eq!(rig.gate().pending.as_ref().map(|p| p.text.as_str()), Some("a wall"));
        assert!(matches!(rig.gate().ended, Some((Ended::Replaced, _))));
        let wall = Rig::new(lex.clone(), false).run(|c| {
            c.cast("a wall", false, 0);
        });
        let sent = rig.run(|c| {
            c.confirm_pending();
        });
        assert_eq!(sent, wall);
    }

    /// The confirm words never steal a cast: none of them is a move, a power, a signature or a stand
    /// rush, typed or spoken (the option on swallows them when nothing is pending — that must never
    /// hide a real move); a real phrase is never mistaken for a confirm
    /// (`confirm_words_match_the_whole_line_only`); with the option on and no ghost, a confirm word
    /// does nothing at all (no ghost of the lexicon's loose reading: "confirm" alone reads as an icon).
    #[test]
    fn confirm_words_do_not_collide_with_casts() {
        let Some(lex) = lexicon() else { return };
        for w in DEFAULT_WORDS {
            for spoken in [false, true] {
                assert!(crate::toolbox::command(&lex, None, true, w, spoken).is_none(), "{w:?} is a move");
                let r = if spoken { lex.resolve_spoken(w) } else { lex.resolve(w) };
                assert!(r.signature.is_none() && r.recipe.grants.is_none() && r.stand_rush.is_none(), "{w:?} → {:?}", r.recipe.name);
                let mut rig = Rig::new(lex.clone(), true);
                let sent = rig.run(move |c| {
                    assert!(!c.cast_heard(w, spoken, 0, None), "{w:?}: nothing to confirm");
                });
                assert!(sent.is_empty() && rig.gate().pending.is_none(), "{w:?} (spoken {spoken}) with nothing pending: {sent:?}");
            }
        }
    }

    #[test]
    fn a_new_ghost_replaces_the_old_one() {
        let mut g = ConfirmGate::default();
        let p = |t: &str| Pending { text: t.into(), spoken: false, rush_already: 0, heard_conf: None, voice_peak: None, cmds: vec![Cmd::Dismiss], age: 0.0, serial: 0 };
        g.open(p("a dragon"));
        let first = g.pending.as_ref().unwrap().serial;
        g.open(p("a wall"));
        assert_eq!(g.ended, Some((Ended::Replaced, first)));
        let second = g.pending.as_ref().unwrap().serial;
        assert_ne!(first, second);
        assert_eq!(g.end(Ended::Cancelled).map(|p| p.text), Some("a wall".into()));
        assert_eq!(g.ended, Some((Ended::Cancelled, second)));
        assert!(g.end(Ended::Cancelled).is_none(), "nothing left to drop");
        assert!(!previewable(&[Cmd::StandRush(3)]), "stand rush stays live input");
    }
}
