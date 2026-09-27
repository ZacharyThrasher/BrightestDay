//! LEGENDARY micro-benchmark: world tick time with eight players all wielding legendary-rule
//! weapons (storm calls, waves, leaps, chains, needles, streams, BFG orbs…) vs the same fight with
//! the plain versions of those forms (no rule fires). Run with `--release`:
//!
//!     cargo run -p bd_sim --release --example legend_bench [-- --ticks 12000]
//!
//! Prints per-tick p50 / p99 / max (µs) for both, and the event count (the rules are firing).

use std::time::Instant;

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

fn r(name: &str, form: Form, m: &[Mechanic], e: Element) -> Recipe {
    recipe(name, form, m, e, bd_sim::constructs::form_spec(form).default_scale)
}

fn legendary() -> Vec<Recipe> {
    let mut needler = r("Needler", Form::Gun, &[Mechanic::Shoots], Element::Psychic);
    needler.payload = Some(Box::new(r("needle", Form::Ball, &[Mechanic::Explodes, Mechanic::Homing, Mechanic::Sticky], Element::Psychic)));
    let mut bfg = r("BFG", Form::Cannon, &[Mechanic::Shoots, Mechanic::Heavy], Element::Atomic);
    bfg.payload = Some(Box::new(r("orb", Form::Ball, &[Mechanic::Beam, Mechanic::Explodes], Element::Atomic)));
    vec![
        r("Mjolnir", Form::Hammer, &[Mechanic::Blunt, Mechanic::Returning, Mechanic::Heavy], Element::Electric),
        r("Buster Sword", Form::Sword, &[Mechanic::Blade, Mechanic::Heavy], Element::Light),
        r("Blades of Chaos", Form::Whip, &[Mechanic::Blade, Mechanic::Pull], Element::Fire),
        r("Master Sword", Form::Sword, &[Mechanic::Blade, Mechanic::Shoots], Element::Light),
        r("Cap's shield", Form::Shield, &[Mechanic::Guard, Mechanic::Returning, Mechanic::Ricochet], Element::Light),
        needler,
        r("Proton Pack", Form::Gun, &[Mechanic::Beam, Mechanic::Pull], Element::Atomic),
        bfg,
    ]
}

fn plain() -> Vec<Recipe> {
    vec![
        r("hammer", Form::Hammer, &[Mechanic::Blunt], Element::Light),
        r("hammer", Form::Hammer, &[Mechanic::Blunt], Element::Light),
        r("whip", Form::Whip, &[], Element::Light),
        r("hammer", Form::Hammer, &[Mechanic::Blunt], Element::Light),
        r("shield", Form::Shield, &[Mechanic::Guard], Element::Light),
        r("gun", Form::Gun, &[Mechanic::Shoots], Element::Light),
        r("rifle", Form::Gun, &[Mechanic::Pierce], Element::Light),
        r("rocket launcher", Form::Gun, &[Mechanic::Explodes], Element::Light),
    ]
}

fn run(items: Vec<Recipe>, ticks: u32) -> (Vec<u32>, usize) {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 128.0 }, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    w.cfg.tuning.freedom.legendary.base = 0.0;
    w.cfg.tuning.freedom.legendary.cap = 0.0;
    let n = items.len() as u8;
    for k in 1..=n {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    // A ring, everyone facing the middle.
    for k in 1..=n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        let p = w.player_mut(PlayerId(k)).unwrap();
        p.mv.pos = Vec3::new(a.cos() * 12.0, 8.0, a.sin() * 12.0);
        p.spawn_shield = 0.0;
    }
    for (k, item) in items.into_iter().enumerate() {
        let id = PlayerId(k as u8 + 1);
        let eye = w.player(id).unwrap().eye(w.params());
        let _ = w.cast(id, item, None, Aim::new(eye, Vec3::NEG_Z));
    }
    for _ in 0..40 {
        w.tick(&[]);
    }
    let mut times = Vec::with_capacity(ticks as usize);
    let mut events = 0;
    let mut inputs = Vec::with_capacity(n as usize);
    for t in 0..ticks {
        inputs.clear();
        for k in 1..=n {
            let id = PlayerId(k);
            let Some(p) = w.player(id) else { continue };
            // Face the middle; a scripted rhythm of holds, taps and alt-fire per player.
            let to = -p.mv.pos;
            let yaw = (-to.x).atan2(-to.z) + 0.2 * ((t as f32 * 0.01) + k as f32).sin();
            let phase = (t + k as u32 * 37) % 360;
            let b = match phase {
                0..=150 => buttons::PRIMARY,
                170..=175 => buttons::SECONDARY | buttons::ADS,
                200..=260 => buttons::SECONDARY,
                280 | 300 | 320 => buttons::PRIMARY,
                _ => 0,
            };
            inputs.push((id, PlayerInput { yaw, pitch: -0.05, buttons: b, move_z: if phase % 90 < 30 { 1 } else { 0 }, ..Default::default() }));
        }
        let t0 = Instant::now();
        events += w.tick(&inputs).len();
        times.push(t0.elapsed().as_micros() as u32);
        // Keep everyone in the ring (KOs respawn elsewhere).
        if t % 600 == 599 {
            for k in 1..=n {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                if let Some(p) = w.player_mut(PlayerId(k)) {
                    p.mv.pos = Vec3::new(a.cos() * 12.0, 8.0, a.sin() * 12.0);
                    p.mv.vel = Vec3::ZERO;
                }
            }
        }
    }
    (times, events)
}

fn stats(mut v: Vec<u32>) -> (u32, u32, u32) {
    v.sort_unstable();
    let at = |q: f64| v[((v.len() as f64 - 1.0) * q) as usize];
    (at(0.5), at(0.99), *v.last().unwrap_or(&0))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ticks = args.iter().position(|a| a == "--ticks").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(12_000u32);
    let (pt, pe) = run(plain(), ticks);
    let (lt, le) = run(legendary(), ticks);
    let (p50, p99, pmax) = stats(pt);
    let (l50, l99, lmax) = stats(lt);
    println!("plain     : p50 {p50} us  p99 {p99} us  max {pmax} us  ({pe} events)");
    println!("legendary : p50 {l50} us  p99 {l99} us  max {lmax} us  ({le} events)");
}
