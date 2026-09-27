//! `--scenario hotbar` (practice): the hotbar proof shots (docs/readability/hotbar_sheet.png).
//! Everything goes through the real paths: the ring (`Caster::cast`, typed), the host's
//! confirmation → auto-bind, the key fire (`fire::fire_slot`), the Tab editor's drop rules, the
//! TOTAL LIMIT rule and DISMISS BY SLOT's commands. Only the Tab key / cursor are scripted
//! (`hud::DevTab`) and two cooldowns are skipped so the key press shows within the shot.
//!
//! Timeline (s): 1.0 fresh bar · 2.0 say "reinhardt ultimate" (→ slot 4) · 3.2 say "bamf" (→ 3)
//! (+ Ultra Instinct held) · 4.6 press 4 · 6.0 Tab: drag 4 onto 1 · 7.1 drop + pin · 8.0 TOTAL
//! LIMIT 4: slot 2 = "star platinum", press 2, "a stone wall", "a wolf" · 9.6 "giant sword"
//! refused · 10.3 X (window) · 11.0 → 2 dismisses Star Platinum · 11.8 hover slot 1 (its tooltip).

use super::hud::{DevTab, Over, Src, TabEdit};
use super::{Entry, Hotbar, SlotBind};
use crate::console::Caster;
use crate::sim_bridge::{ActiveWorld, SimCommand};
use bevy::prelude::*;

#[derive(Resource, Default)]
struct Script {
    t: f32,
    step: u8,
    /// `--hotbar-shots <stem>`: the scenario takes its own shots (`<stem>_<n>.png`) and quits.
    shots: Option<String>,
    queue: Vec<(f32, u8)>,
    taken: u8,
}

pub fn build(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "hotbar") {
        return;
    }
    info!("hotbar scenario");
    let shots = args.windows(2).find(|w| w[0] == "--hotbar-shots").map(|w| w[1].clone());
    app.insert_resource(Script { shots, ..default() }).add_systems(Update, run.run_if(resource_exists::<ActiveWorld>).after(crate::lobby::UiSet::Input).before(crate::lobby::UiSet::Draw));
}

#[allow(clippy::too_many_arguments)]
fn run(
    time: Res<Time<Real>>,
    lex: Option<Res<crate::console::RingLexicon>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    mut sc: ResMut<Script>,
    mut bar: ResMut<Hotbar>,
    mut edit: ResMut<TabEdit>,
    mut win: ResMut<super::dismiss::DismissWindow>,
    mut set: ParamSet<(Caster, super::fire::KeyFire, ResMut<ActiveWorld>, MessageWriter<SimCommand>, ResMut<crate::input::LookAngles>)>,
) {
    if lex.is_none() {
        return; // the ring can't read yet: the script starts when it can
    }
    sc.t += time.delta_secs();
    let t = sc.t;
    // scripted shots
    if let Some(stem) = sc.shots.clone() {
        let due: Vec<u8> = sc.queue.iter().filter(|q| q.0 <= t).map(|q| q.1).collect();
        sc.queue.retain(|q| q.0 > t);
        for n in due {
            let path = format!("{stem}_{n}.png");
            info!("hotbar scenario: shot {path}");
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
            sc.taken = sc.taken.max(n);
        }
        if sc.step >= 14 && sc.queue.is_empty() && t > 14.5 {
            exit.write(AppExit::Success);
        }
    }
    let step_before = sc.step;
    let at = |s: &Script, k: u8, when: f32| s.step == k && t >= when;
    // keep the scene calm and the sparks topped up (the shots are about the bar)
    {
        let mut a = set.p2();
        a.bots.clear();
        let pool = a.world.rules().willpower_max();
        if let Some(me) = a.local
            && let Some(p) = a.world.player_mut(me)
        {
            p.willpower = pool;
            p.spawn_shield = 0.0;
        }
    }
    if at(&sc, 0, 0.6) {
        // a fresh default loadout, auto-bind on
        let l = ["hammer".to_string(), "shield".into(), "wings".into(), "rocket launcher".into()];
        *bar = Hotbar::from_prefs(&l, &[false; 4]);
        set.p4().pitch = -0.08;
        sc.step = 1;
    } else if at(&sc, 1, 2.0) {
        set.p0().cast("reinhardt ultimate", false, 0);
        sc.step = 2;
    } else if at(&sc, 2, 3.2) {
        set.p0().cast("bamf", false, 0);
        // a held power too: its moves wait in the Tab tray ("drag a held power's move onto a slot")
        {
            let mut a = set.p2();
            if let Some(me) = a.local
                && !a.world.grant_power(me, "ultra_instinct")
            {
                warn!("hotbar scenario: no ultra_instinct");
            }
        }
        sc.step = 3;
    } else if at(&sc, 3, 4.6) {
        // skip the ultimate's cooldown so the key press shows in the shot
        {
            let mut a = set.p2();
            if let Some(me) = a.local
                && let Some(p) = a.world.player_mut(me)
            {
                for c in p.power.toolbox.cds.iter_mut() {
                    c.left = 0.0;
                }
                p.power.toolbox.active = 0.0;
            }
        }
        // press 4 (slot index 3): exactly what the key does
        let slot = bar.slots[3].clone();
        let step = super::fire::step_for(&slot.bind, set.p1().local_power());
        if let Some(st) = step {
            set.p1().fire_steps(vec![st], &slot.phrase);
        }
        bar.touch(3);
        sc.step = 4;
    } else if at(&sc, 4, 6.0) {
        edit.dev = Some(DevTab { cursor: None, target: Some(Over::Slot(0)), drag: Some(Src::Slot(3)) });
        sc.step = 5;
    } else if at(&sc, 5, 7.1) {
        let tray = edit.tray.clone();
        super::hud::drop_on(&mut bar, &tray, Src::Slot(3), Over::Slot(0), true);
        bar.toggle_pin(0);
        edit.dev = Some(DevTab { cursor: None, target: None, drag: None });
        sc.step = 6;
    } else if at(&sc, 6, 8.0) {
        edit.dev = None;
        {
            let mut a = set.p2();
            a.world.cfg.rules.active_limit = 4;
        }
        bar.place(1, &Entry::new("star platinum", SlotBind::Phrase { signature: false }));
        bar.slots[1].read = false; // let the reading (and its granted power) be derived
        sc.step = 7;
    } else if at(&sc, 7, 8.3) {
        let phrase = bar.slots[1].phrase.clone();
        bar.touch(1);
        set.p0().cast(&phrase, false, 0);
        sc.step = 8;
    } else if at(&sc, 8, 8.8) {
        set.p0().cast("a stone wall", false, 0);
        sc.step = 9;
    } else if at(&sc, 9, 9.2) {
        set.p0().cast("a wolf", false, 0);
        sc.step = 10;
    } else if at(&sc, 10, 9.6) {
        set.p0().cast("giant sword", false, 0); // refused: at the cap
        sc.step = 11;
    } else if at(&sc, 11, 10.3) {
        win.left = Some(super::dismiss::WINDOW); // X pressed: the live slots pulse
        sc.step = 12;
    } else if at(&sc, 12, 11.0) {
        win.left = None;
        let cmds = {
            let a = set.p2();
            super::dismiss::commands_for(&bar, super::dismiss::What::Slot(1), &a)
        };
        let mut w = set.p3();
        for c in cmds {
            w.write(SimCommand::local(c));
        }
        sc.step = 13;
    } else if at(&sc, 13, 11.8) {
        // READABILITY: hover slot 1 ("reinhardt ultimate", pinned since step 6) for its tooltip —
        // the owner's fix ("only have a tooltip come up when they hover over the actual ability").
        edit.dev = Some(DevTab { cursor: None, target: Some(Over::Slot(0)), drag: None });
        sc.step = 14;
    }
    // a shot a beat after each scripted moment
    if sc.step != step_before && sc.shots.is_some() {
        let (n, delay) = match sc.step {
            1 => (1, 0.9),
            2 => (2, 0.8),
            3 => (3, 0.8),
            4 => (4, 0.35),
            5 => (5, 0.5),
            6 => (6, 0.5),
            11 => (7, 0.45),
            13 => (8, 0.7),
            14 => (9, 0.6),
            _ => (0, 0.0),
        };
        if n > 0 {
            sc.queue.push((t + delay, n));
        }
    }
}
