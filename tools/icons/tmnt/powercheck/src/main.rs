//! Standalone check of assets/powers.ron against bd_sim's PowerDef (data.rs included verbatim).
#![allow(dead_code)]

mod movement {
    #[derive(Default)]
    pub struct MoveModifiers {
        pub speed_mult: f32,
        pub gravity_scale: f32,
        pub jump_mult: f32,
        pub air_control: f32,
        pub accel_mult: f32,
        pub glide: f32,
        pub thrust: f32,
        pub thrust_max_speed: f32,
        pub slow_fall: f32,
    }
}
mod status {
    use serde::{Deserialize, Serialize};
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum StatusKind { Burn, Poison, Chill, Stun, Float, Shadow, Wet }
}
mod rng {
    pub fn word_hash(s: &str) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in s.bytes() {
            h ^= b.to_ascii_lowercase() as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
        h
    }
}
#[path = "../../../../../crates/bd_sim/src/powers/data.rs"]
mod data;

fn main() {
    let src = std::fs::read_to_string("assets/powers.ron").expect("run from the game root");
    let powers: Vec<data::PowerDef> = match ron::from_str(&src) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("PARSE ERROR: {e}");
            std::process::exit(1);
        }
    };
    let mut bad = 0;
    for (k, p) in powers.iter().enumerate() {
        let mut err = |m: String| {
            eprintln!("{}: {m}", p.id);
            bad += 1;
        };
        if powers.iter().skip(k + 1).any(|q| q.id == p.id) { err("duplicate id".into()); }
        if p.names.is_empty() { err("no names".into()); }
        if p.moves.is_empty() || p.moves.len() > 2 { err("1-2 moves".into()); }
        if !p.shouts.iter().all(|(_, s)| (*s as usize) < p.moves.len()) { err("shout slot".into()); }
        if p.kind.is_fruit() != p.companion.is_none() { err("companion rule".into()); }
        if p.kind.is_fruit() && p.incantations.is_empty() { err("fruit without incantation".into()); }
        for m in &p.moves {
            for op in &m.ops {
                if matches!(op, data::MoveOp::Transform) && p.transform.is_none() { err("Transform without transform".into()); }
                if let data::MoveOp::Cast { recipe, .. } = op {
                    let r = recipe.to_recipe();
                    if r.name.is_empty() || !r.scale.is_finite() { err("bad cast recipe".into()); }
                }
            }
        }
    }
    let fruits = powers.iter().filter(|p| p.kind.is_fruit()).count();
    println!("powers.ron: {} powers ({} fruits, {} stands), {bad} problems", powers.len(), fruits, powers.len() - fruits);
    std::process::exit(if bad > 0 { 1 } else { 0 });
}
