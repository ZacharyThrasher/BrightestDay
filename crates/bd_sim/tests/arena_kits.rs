//! Arena-games content (assets/powers/games_arena.ron): golden phrase → move readings (kit
//! prefixes, borrowed shouts, Whisper mishearings), golden CHAINS split and resolved, and chains /
//! headline kit moves fired in a real world (reinhardt charge pins into a wall, the doomfist punch
//! splats, the nano-blade, tracer blink → doomfist punch → reinhardt ultimate).

use bd_core::corps::Corps;
use bd_core::{PlayerId, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::powers::chain::ToolStep;
use bd_sim::powers::ops::{OpEvent, RamPhase};
use bd_sim::powers::toolbox::{split_chain, MoveIndex, MoveRef, Reading};
use bd_sim::powers::{PowerEvent, PowerTable};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

/// What a phrase fires for a powerless player: (power id, move name), kit or borrowed.
fn reads(phrase: &str) -> Option<(String, String)> {
    let t = PowerTable::get();
    let m = MoveIndex::shipped().read(t, None, phrase).move_ref()?;
    let (def, mv) = m.resolve(t)?;
    Some((def.id.clone(), mv.name.clone()))
}

fn move_ref(phrase: &str) -> MoveRef {
    MoveIndex::shipped().read(PowerTable::get(), None, phrase).move_ref().unwrap_or_else(|| panic!("{phrase:?} is not a move"))
}

#[track_caller]
fn check(phrase: &str, id: &str, name: &str) {
    assert_eq!(reads(phrase), Some((id.to_string(), name.to_string())), "{phrase:?}");
}

#[test]
fn hero_moves_by_kit_name_slot_word_and_mishearing() {
    // Overwatch — the owner's own examples first.
    check("reinhardt ultimate", "ow_reinhardt", "earthshatter");
    check("reinhardt ult", "ow_reinhardt", "earthshatter");
    check("rein hart ultimate", "ow_reinhardt", "earthshatter");
    check("reinhardt charge", "ow_reinhardt", "charge");
    check("reinhardt's charge", "ow_reinhardt", "charge");
    check("reinhardt shield", "ow_reinhardt", "barrier field");
    check("doomfist punch", "ow_doomfist", "rocket punch");
    check("dooms fist punch", "ow_doomfist", "rocket punch");
    check("doom first punch", "ow_doomfist", "rocket punch");
    check("doomfist ultimate", "ow_doomfist", "meteor strike");
    check("doomfist slam", "ow_doomfist", "seismic slam");
    check("tracer blink", "ow_tracer", "blink");
    check("tracer recall", "ow_tracer", "recall");
    check("tracer ult", "ow_tracer", "pulse bomb");
    check("genji deflect", "ow_genji", "deflect");
    check("genji ultimate", "ow_genji", "dragonblade");
    check("genji dash", "ow_genji", "swift strike");
    check("hanzo ult", "ow_hanzo", "dragonstrike");
    check("mei wall", "ow_mei", "ice wall");
    check("mei ultimate", "ow_mei", "blizzard");
    check("winston leap", "ow_winston", "jump pack");
    check("winston ult", "ow_winston", "primal rage");
    check("reaper wraith", "ow_reaper", "wraith form");
    check("reaper ultimate", "ow_reaper", "death blossom");
    check("lucio boop", "ow_lucio", "soundwave");
    check("lucio wall ride", "ow_lucio", "wall ride");
    check("junkrat ult", "ow_junkrat", "rip tire");
    check("sombra hack", "ow_sombra", "hack");
    check("sombra translocator", "ow_sombra", "translocator");
    check("mccree ultimate", "ow_cassidy", "deadeye");
    check("cassidy deadeye", "ow_cassidy", "deadeye");
    check("pharah ult", "ow_pharah", "barrage");
    check("dva boosters", "ow_dva", "boosters");
    check("dva ultimate", "ow_dva", "self destruct");
    check("zarya ult", "ow_zarya", "graviton surge");
    check("ana sleep dart", "ow_ana", "sleep dart");
    check("ana nano boost", "ow_ana", "nano boost");
    check("kiriko swift step", "ow_kiriko", "swift step");
    check("kiriko ultimate", "ow_kiriko", "kitsune rush");
    check("soldier 76 ult", "ow_soldier_76", "tactical visor");
    check("widowmaker grapple", "ow_widowmaker", "grappling hook");
    check("roadhog hook", "ow_roadhog", "chain hook");
    // Borrowed shouts (anyone, no prefix).
    check("earthshatter", "ow_reinhardt", "earthshatter");
    check("rocket punch", "ow_doomfist", "rocket punch");
    check("pulse bomb", "ow_tracer", "pulse bomb");
    check("dragonblade", "ow_genji", "dragonblade");
    check("deadeye", "ow_cassidy", "deadeye");
    check("high noon", "ow_cassidy", "deadeye");
    check("graviton surge", "ow_zarya", "graviton surge");
    check("nano boost", "ow_ana", "nano boost");
    check("death blossom", "ow_reaper", "death blossom");
    // Valorant / Apex / Deadlock.
    check("jett dash", "val_jett", "tailwind");
    check("jett ultimate", "val_jett", "blade storm");
    check("raze satchel", "val_raze", "blast pack");
    check("showstopper", "val_raze", "showstopper");
    check("omen teleport", "val_omen", "shadow walk");
    check("reyna dismiss", "val_reyna", "dismiss");
    check("pathfinder grapple", "apex_pathfinder", "grapple");
    check("wraith portal", "apex_wraith", "dimensional rift");
    check("octane stim", "apex_octane", "stim");
    check("horizon ultimate", "apex_horizon", "black hole");
    check("abrams charge", "dl_abrams", "shoulder charge");
    check("paradox swap", "dl_paradox", "paradoxical swap");
    // MOBAs.
    check("summoner flash", "lol_summoner", "flash");
    check("yasuo wind wall", "lol_yasuo", "wind wall");
    check("yasuo ult", "lol_yasuo", "last breath");
    check("lee sin kick", "lol_lee_sin", "dragon's rage");
    check("insec", "lol_lee_sin", "dragon's rage");
    check("blitzcrank grab", "lol_blitzcrank", "rocket grab");
    check("rocket grab", "lol_blitzcrank", "rocket grab");
    check("thresh hook", "lol_thresh", "death sentence");
    check("ezreal ult", "lol_ezreal", "trueshot barrage");
    check("zed ultimate", "lol_zed", "death mark");
    check("malphite ult", "lol_malphite", "unstoppable force");
    check("blink dagger", "dota_items", "blink dagger");
    check("bkb", "dota_items", "black king bar");
    check("pudge hook", "dota_pudge", "meat hook");
    check("faceless void ultimate", "dota_faceless_void", "chronosphere");
    check("chronosphere", "dota_faceless_void", "chronosphere");
    check("echo slam", "dota_earthshaker", "echo slam");
    check("magnus ult", "dota_magnus", "reverse polarity");
    check("sun strike", "dota_invoker", "sun strike");
    check("culling blade", "dota_axe", "culling blade");
    // Fighting games.
    check("hadouken", "sf_ryu", "hadouken");
    check("hadoken", "sf_ryu", "hadouken");
    check("ha do ken", "sf_ryu", "hadouken");
    check("shoryuken", "sf_ryu", "shoryuken");
    check("show you can", "sf_ryu", "shoryuken");
    check("tatsumaki senpukyaku", "sf_ryu", "tatsumaki senpukyaku");
    check("ryu super", "sf_ryu", "shin shoryuken");
    check("spinning bird kick", "sf_chun_li", "spinning bird kick");
    check("lightning legs", "sf_chun_li", "lightning legs");
    check("guile sonic boom", "sf_guile", "sonic boom");
    check("flash kick", "sf_guile", "flash kick");
    check("yoga fire", "sf_dhalsim", "yoga fire");
    check("yoga teleport", "sf_dhalsim", "yoga teleport");
    check("spd", "sf_zangief", "spinning piledriver");
    check("spinning piledriver", "sf_zangief", "spinning piledriver");
    check("raging demon", "sf_akuma", "raging demon");
    check("hundred hand slap", "sf_e_honda", "hundred hand slap");
    check("psycho crusher", "sf_m_bison", "psycho crusher");
    check("get over here", "mk_scorpion", "spear");
    check("scorpion spear", "mk_scorpion", "spear");
    check("scorpion teleport", "mk_scorpion", "teleport punch");
    check("toasty", "mk_scorpion", "toasty");
    check("scorpion fatality", "mk_scorpion", "toasty");
    check("sub zero slide", "mk_sub_zero", "slide");
    check("sub zero ice clone", "mk_sub_zero", "ice clone");
    check("raiden torpedo", "mk_raiden", "torpedo");
    check("liu kang bicycle kick", "mk_liu_kang", "bicycle kick");
    check("kung lao hat", "mk_kung_lao", "hat throw");
    check("johnny cage shadow kick", "mk_johnny_cage", "shadow kick");
    check("finish him", "mk_flourish", "finish him");
    check("fatality", "mk_flourish", "fatality");
    check("ewgf", "tk_mishima", "electric wind god fist");
    check("electric wind god fist", "tk_mishima", "electric wind god fist");
    check("giant swing", "tk_king", "giant swing");
    check("falcon punch", "ssb_captain_falcon", "falcon punch");
    check("falcon pawnch", "ssb_captain_falcon", "falcon punch");
    check("captain falcon kick", "ssb_captain_falcon", "falcon kick");
    check("marth counter", "ssb_marth", "counter");
    check("warlock punch", "ssb_ganondorf", "warlock punch");
    check("final smash", "ssb_smash", "final smash");
    check("volcanic viper", "gg_sol_badguy", "volcanic viper");
    // Shooters / party.
    check("build a wall", "fn_builder", "build a wall");
    check("crank 90s", "fn_builder", "crank 90s");
    check("chug jug", "fn_builder", "chug jug");
    check("shockwave grenade", "fn_builder", "shockwave grenade");
    check("rocket jump", "tf2_soldier", "rocket jump");
    check("sandvich", "tf2_heavy", "sandvich");
    check("ubercharge", "tf2_medic", "ubercharge");
    check("backstab", "tf2_spy", "backstab");
    check("airblast", "tf2_pyro", "airblast");
    check("sticky jump", "tf2_demoman", "sticky jump");
}

#[test]
fn bare_hero_names_stay_costume_summons() {
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    for name in ["reinhardt", "doomfist", "tracer", "genji", "scorpion", "ryu", "zangief", "captain falcon", "pudge", "yasuo", "cassidy"] {
        assert_eq!(ix.read(t, None, name), Reading::Plain, "{name:?} must stay the costume summon");
        assert!(!ix.is_grant_name(name), "{name:?} is a kit name, never a grant");
    }
}

#[test]
fn golden_chains_split_and_resolve() {
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    let is_move = |s: &str| ix.read(t, None, s).is_move();
    let chains: &[(&str, &[(&str, &str)])] = &[
        ("tracer blink -> doomfist punch -> reinhardt ultimate", &[("ow_tracer", "blink"), ("ow_doomfist", "rocket punch"), ("ow_reinhardt", "earthshatter")]),
        ("reinhardt charge then earthshatter", &[("ow_reinhardt", "charge"), ("ow_reinhardt", "earthshatter")]),
        ("ana nano boost into genji dragonblade", &[("ow_ana", "nano boost"), ("ow_genji", "dragonblade")]),
        ("get over here, shoryuken", &[("mk_scorpion", "spear"), ("sf_ryu", "shoryuken")]),
        ("rocket grab > power fist", &[("lol_blitzcrank", "rocket grab"), ("lol_blitzcrank", "power fist")]),
        ("sonic wave -> resonating strike -> insec", &[("lol_lee_sin", "sonic wave"), ("lol_lee_sin", "resonating strike"), ("lol_lee_sin", "dragon's rage")]),
        ("gathering storm then last breath", &[("lol_yasuo", "gathering storm"), ("lol_yasuo", "last breath")]),
        ("invoker tornado -> invoker emp -> chaos meteor -> deafening blast",
         &[("dota_invoker", "tornado"), ("dota_invoker", "emp"), ("dota_invoker", "chaos meteor"), ("dota_invoker", "deafening blast")]),
        ("blink dagger -> echo slam", &[("dota_items", "blink dagger"), ("dota_earthshaker", "echo slam")]),
        ("raze satchel -> showstopper", &[("val_raze", "blast pack"), ("val_raze", "showstopper")]),
        ("hadouken, shoryuken, shinku hadouken", &[("sf_ryu", "hadouken"), ("sf_ryu", "shoryuken"), ("sf_ryu", "shinku hadouken")]),
        ("zarya ult and then pharah ult", &[("ow_zarya", "graviton surge"), ("ow_pharah", "barrage")]),
        ("sticky jump then market garden", &[("tf2_demoman", "sticky jump"), ("tf2_soldier", "market garden")]),
    ];
    for (line, want) in chains {
        let steps = split_chain(line, &is_move);
        let got: Vec<Option<(String, String)>> = steps.iter().map(|s| reads(s)).collect();
        let want: Vec<Option<(String, String)>> = want.iter().map(|(a, b)| Some((a.to_string(), b.to_string()))).collect();
        assert_eq!(got, want, "{line:?} split into {steps:?}");
    }
}

// ------------------------------------------------------------------------------------ in the world

fn duel() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=2u8 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
        p.willpower = 100.0;
    }
    stand(&mut w, A, 0.0, 0.0);
    stand(&mut w, B, 0.0, -30.0);
    w
}

fn stand(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, GROUND, z);
    p.mv.vel = Vec3::ZERO;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
}

fn wall(w: &mut World, a: Vec3, b: Vec3) {
    let lo = bd_core::voxel::world_to_voxel(a.min(b));
    let hi = bd_core::voxel::world_to_voxel(a.max(b));
    w.voxels.fill_box(lo, hi, bd_voxel::mat::STONE);
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn say(w: &mut World, who: PlayerId, line: &str) -> Vec<GameEvent> {
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    let is_move = |s: &str| ix.read(t, None, s).is_move();
    let steps: Vec<ToolStep> = split_chain(line, &is_move).iter().map(|s| ToolStep::Move { mv: move_ref(s), kit: true }).collect();
    w.toolbox(who, steps, None).unwrap();
    // (the first step's events are injected now and come out of the next tick)
    Vec::new()
}

fn fired_names(evs: &[GameEvent]) -> Vec<String> {
    evs.iter()
        .filter_map(|e| match e {
            GameEvent::Power(PowerEvent::Fired { mv, .. }) => mv.resolve(PowerTable::get()).map(|(_, m)| m.name.clone()),
            _ => None,
        })
        .collect()
}

fn pct(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().damage_pct
}

#[test]
fn reinhardt_charge_pins_and_carries_into_the_wall() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -3.0);
    wall(&mut w, Vec3::new(-4.0, GROUND, -14.5), Vec3::new(4.0, GROUND + 4.0, -15.5));
    let mut evs = say(&mut w, A, "reinhardt charge");
    evs.extend(run(&mut w, 200));
    let slam = evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Op(OpEvent::Ram { phase: RamPhase::Slam, target: Some(t), .. })) if *t == B));
    assert!(slam, "Reinhardt carried B into the wall");
    assert!(pct(&w, B) >= 20.0, "the slam hurts: {}", pct(&w, B));
}

#[test]
fn doomfist_punch_launches_them_into_a_wall_splat() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -4.0);
    wall(&mut w, Vec3::new(-4.0, GROUND, -11.5), Vec3::new(4.0, GROUND + 6.0, -12.5));
    w.player_mut(B).unwrap().damage_pct = 60.0;
    let mut evs = say(&mut w, A, "doomfist punch");
    evs.extend(run(&mut w, 120));
    assert!(evs.iter().any(|e| matches!(e, GameEvent::WallSplat { player, .. } if *player == B)), "punched into the wall");
}

#[test]
fn the_owners_chain_fires_every_link_in_order() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -14.0);
    let mut evs = say(&mut w, A, "tracer blink -> doomfist punch -> reinhardt ultimate");
    evs.extend(run(&mut w, 360));
    let names = fired_names(&evs);
    assert_eq!(names, vec!["blink", "rocket punch", "earthshatter"], "{names:?}");
    assert!(pct(&w, B) > 0.0, "the chain connected");
}

#[test]
fn the_nano_blade_chain() {
    let mut w = duel();
    let mut evs = say(&mut w, A, "ana nano boost into genji dragonblade");
    evs.extend(run(&mut w, 120));
    assert_eq!(fired_names(&evs), vec!["nano boost", "dragonblade"]);
}

#[test]
fn get_over_here_drags_them_in_for_the_uppercut() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -14.0);
    let mut evs = say(&mut w, A, "get over here, shoryuken");
    evs.extend(run(&mut w, 240));
    assert_eq!(fired_names(&evs), vec!["spear", "shoryuken"]);
    let d = (w.player(B).unwrap().mv.pos - w.player(A).unwrap().mv.pos).length();
    assert!(d < 8.0, "B was dragged in ({d} m)");
    assert!(pct(&w, B) >= 15.0, "spear + uppercut: {}", pct(&w, B));
}

#[test]
#[ignore = "debug print"]
fn debug_doomfist_punch() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -4.0);
    wall(&mut w, Vec3::new(-4.0, GROUND, -11.5), Vec3::new(4.0, GROUND + 6.0, -12.5));
    w.player_mut(B).unwrap().damage_pct = 60.0;
    say(&mut w, A, "doomfist punch");
    for t in 0..60 {
        let evs = w.tick(&[]).to_vec();
        let b = w.player(B).unwrap();
        let a = w.player(A).unwrap();
        eprintln!("{t} A {:?} B {:?} vel {:?} pct {}", a.mv.pos, b.mv.pos, b.mv.vel, b.damage_pct);
        for e in &evs {
            if matches!(e, GameEvent::Hit { .. } | GameEvent::WallSplat { .. } | GameEvent::Power(_)) {
                eprintln!("   {e:?}");
            }
        }
    }
}
