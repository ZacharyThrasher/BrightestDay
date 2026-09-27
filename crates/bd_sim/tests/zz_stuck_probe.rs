//! TEMP probe (terrain-stuck worker): removed after the investigation.
use std::collections::VecDeque;

use bd_core::voxel::VoxelQuery;
use bd_sim::bots;
use bd_sim::world::{ArenaSource, SimConfig, World};

#[test]
fn probe_under_floor() {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 5, check_invariants: false, ..SimConfig::default() };
    cfg.timings.warmup_secs = 0.1;
    let mut w = World::new(cfg);
    w.cfg.tuning.assist.aim = bd_sim::aim_assist::AimTuning::neutral();
    let wb = w.voxels.bounds();
    eprintln!("world bounds {:?} void {} zones {:?}", wb, w.arena.void_y, w.blast_zones());
    let mut brains = bots::add_bots(&mut w, 0, 8, 0.9, 5);
    for b in &mut brains {
        b.casts_enabled = false;
    }
    let mut inputs = Vec::with_capacity(8);
    let mut hist: Vec<VecDeque<String>> = vec![VecDeque::new(); 8];
    let mut reported = [false; 8];
    for t in 0..2400 {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        let ev = w.tick(&inputs).to_vec();
        for (i, p) in w.players.iter().enumerate() {
            let evs: Vec<String> = ev.iter().map(|e| format!("{e:?}")).filter(|s| s.contains(&format!("PlayerId({i})"))).map(|s| s.chars().take(160).collect()).collect();
            let solid = w.voxels.is_solid(bd_core::voxel::world_to_voxel(p.mv.pos + bd_core::Vec3::Y * 0.1));
            hist[i].push_back(format!(
                "t{t} alive={} pos={:.2?} vel={:.2?} gr={} launch={} punch={} seat={:?} solid_at_feet={} ev={:?}",
                p.alive, p.mv.pos, p.mv.vel, p.mv.grounded, p.launch.is_some(), p.punch.is_some(), p.seat.map(|s| s.construct), solid, evs
            ));
            if hist[i].len() > 40 {
                hist[i].pop_front();
            }
            if p.alive && p.mv.pos.y < 7.0 && !reported[i] {
                reported[i] = true;
                eprintln!("==== player {i} under floor at t{t} ==== {}", hist[i].back().unwrap());
            }
        }
    }
    for (i, p) in w.players.iter().enumerate() {
        eprintln!("final {i} alive={} pos={:.2?} vel={:.2?}", p.alive, p.mv.pos, p.mv.vel);
    }
}
