//! Physical palm beams (`signature::palm`) and energy balls (`signature::orb`): sustained beams that
//! travel, bore terrain, get blocked, clash (push, bail out, detonate); hold-to-grow thrown energy
//! (tap, grow, knocked loose, overhead, pushed back by a beam, ball vs ball). COMBAT.md §5j.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{ConstructId, DT, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::GameEvent;
use bd_sim::signature::palm::{BeamPhase, BeamView};
use bd_sim::signature::{SigFx, SignatureCast, SignatureKind};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const PI: f32 = std::f32::consts::PI;

/// A and B on flat ground, B `dist` m in front of A (A looks down -Z, B looks back at A).
fn duel(dist: f32) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 160.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    for _ in 0..60 {
        w.tick(&[]);
    }
    let a = w.player(A).unwrap().mv.pos;
    w.player_mut(B).unwrap().mv.pos = a + Vec3::new(0.0, 0.0, -dist);
    for _ in 0..30 {
        w.tick(&[]);
    }
    for p in &mut w.players {
        p.willpower = 100.0;
        p.spawn_shield = 0.0;
    }
    w
}

fn inp(yaw: f32, pitch: f32, buttons: u16) -> PlayerInput {
    PlayerInput { yaw, pitch, buttons, ..Default::default() }
}

fn aim(w: &World, who: PlayerId, dir: Vec3) -> Aim {
    let p = w.player(who).unwrap();
    Aim::new(p.mv.pos + Vec3::Y * 1.6, dir.normalize())
}

fn hands(ult: bool) -> Recipe {
    recipe("Kamehameha", Form::Hands, &[Mechanic::Beam], Element::Light, 0.6).tap(|_| ()).with_ult(ult)
}

trait Tap: Sized {
    fn tap(self, f: impl FnOnce(&Self)) -> Self {
        f(&self);
        self
    }
    fn with_ult(self, _ult: bool) -> Self {
        self
    }
}
impl Tap for Recipe {}

/// `who` conjures palm energy (an ultimate signature, or a plain cast) facing `dir`.
fn give_hands(w: &mut World, who: PlayerId, dir: Vec3, ult: bool) -> ConstructId {
    let a = aim(w, who, dir);
    let id = if ult {
        w.cast_signature(who, hands(true), None, a, SignatureCast::new(SignatureKind::Beam, 1.0)).expect("cast")
    } else {
        w.cast(who, hands(false), None, a).expect("cast")
    };
    // Forming.
    for _ in 0..(0.8 / DT) as usize {
        w.tick(&[]);
    }
    assert_eq!(w.constructs.get(id).map(|c| c.recipe.form), Some(Form::Hands));
    w.player_mut(who).unwrap().willpower = 100.0;
    id
}

fn beams(evs: &[GameEvent]) -> Vec<BeamView> {
    evs.iter()
        .filter_map(|e| match e {
            GameEvent::PalmBeam(v) => Some(*v),
            _ => None,
        })
        .collect()
}

/// Tick with per-player inputs until `until` is true or `secs` pass; returns every event.
fn run(
    w: &mut World,
    secs: f32,
    mut f: impl FnMut(&World, f32) -> Vec<(PlayerId, PlayerInput)>,
    mut until: impl FnMut(&[GameEvent]) -> bool,
) -> Vec<GameEvent> {
    let mut all = Vec::new();
    let mut t = 0.0;
    while t < secs {
        let ins = f(w, t);
        let e = w.tick(&ins).to_vec();
        t += DT;
        let stop = until(&e);
        all.extend(e);
        if stop {
            break;
        }
    }
    all
}

// ================================================================================ palm beams

#[test]
fn palm_beam_is_sustained_travels_and_hits_along_its_length() {
    let mut w = duel(30.0);
    give_hands(&mut w, A, Vec3::NEG_Z, false);
    let pct0 = w.player(B).unwrap().damage_pct;
    // Hold to charge; it fires by itself when full.
    let evs = run(&mut w, 2.0, |_, _| vec![(A, inp(0.0, -0.02, buttons::PRIMARY))], |e| !beams(e).is_empty());
    let first = *beams(&evs).first().expect("a beam starts");
    assert!(first.len < 3.0, "the head leaves the palms, not an instant ray: {}", first.len);
    assert!(first.radius > 0.2 && first.radius < 0.6, "plain width {}", first.radius);
    // Let go; watch it pour out and end.
    let mut hits_b = 0;
    let mut max_len = 0.0f32;
    let mut life = 0.0;
    let evs = run(
        &mut w,
        2.0,
        |_, _| vec![(A, inp(0.0, -0.02, 0))],
        |e| {
            life += DT;
            beams(e).iter().any(|v| v.phase == BeamPhase::End)
        },
    );
    for e in &evs {
        match e {
            GameEvent::Hit { target, .. } if *target == B => hits_b += 1,
            GameEvent::PalmBeam(v) => max_len = max_len.max(v.len),
            _ => {}
        }
    }
    assert!(max_len >= 28.0, "reached B: {max_len}");
    assert!((0.45..=0.95).contains(&life), "plain lives ~0.6 s: {life}");
    assert!(hits_b >= 2, "hit along its length, pulse after pulse: {hits_b}");
    assert!(w.player(B).unwrap().damage_pct > pct0);
}

fn fill(w: &mut World, from: Vec3, to: Vec3) {
    let vs = bd_core::VOXEL_SIZE;
    let v = |p: Vec3| bd_core::IVec3::new((p.x / vs).floor() as i32, (p.y / vs).floor() as i32, (p.z / vs).floor() as i32);
    w.voxels.fill_box(v(from), v(to), 3);
}

fn air_along(w: &World, a: Vec3, from: f32, to: f32) -> u32 {
    let vs = bd_core::VOXEL_SIZE;
    let mut air = 0;
    let mut z = from;
    while z <= to {
        let p = a + Vec3::new(0.0, 1.35, -z);
        let v = bd_core::IVec3::new((p.x / vs).floor() as i32, (p.y / vs).floor() as i32, (p.z / vs).floor() as i32);
        air += (w.voxels.get(v) == 0) as u32;
        z += 1.0;
    }
    air
}

#[test]
fn an_ultimate_bores_a_tunnel_a_plain_beam_chips() {
    let tunnel = |ult: bool| {
        let mut w = duel(60.0);
        let a = w.player(A).unwrap().mv.pos;
        fill(&mut w, a + Vec3::new(-6.0, 0.0, -20.0), a + Vec3::new(6.0, 6.0, -12.0));
        give_hands(&mut w, A, Vec3::NEG_Z, ult);
        run(&mut w, 1.5, |_, _| vec![(A, inp(0.0, -0.01, buttons::PRIMARY))], |e| !beams(e).is_empty());
        run(&mut w, 3.0, |_, _| vec![(A, inp(0.0, -0.01, 0))], |e| beams(e).iter().any(|v| v.phase == BeamPhase::End));
        for _ in 0..30 {
            w.tick(&[]);
        }
        air_along(&w, a, 12.5, 19.5)
    };
    let (ult, plain) = (tunnel(true), tunnel(false));
    assert!(ult >= 7, "the ultimate tunnels through the hill: {ult}/8 air");
    assert!(plain < ult, "a plain beam only chips: {plain} vs {ult}");
}

#[test]
fn a_barrier_blocks_the_beam() {
    let mut w = duel(20.0);
    let a = w.player(A).unwrap().mv.pos;
    // B's wall between them (10 m out), B behind it.
    let b_eye = a + Vec3::new(0.0, 1.6, -20.0);
    w.cast(
        B,
        recipe("wall", Form::Wall, &[Mechanic::Barrier], Element::Light, 4.0),
        None,
        Aim::new(b_eye, (a + Vec3::new(0.0, -2.0, -10.0) - b_eye).normalize()),
    )
    .unwrap();
    for _ in 0..40 {
        w.tick(&[]);
    }
    let wall = w.constructs.iter().find(|c| c.owner == B && c.recipe.form == Form::Wall).map(|c| (c.id, c.integrity, c.pos)).expect("wall");
    give_hands(&mut w, A, Vec3::NEG_Z, true);
    let pct0 = w.player(B).unwrap().damage_pct;
    run(&mut w, 1.5, |_, _| vec![(A, inp(0.0, -0.02, buttons::PRIMARY))], |e| !beams(e).is_empty());
    // (A barrier holds for a while — it only takes `block_damage` of the beam.)
    let evs = run(&mut w, 0.8, |_, _| vec![(A, inp(0.0, -0.02, 0))], |_| false);
    let reach = beams(&evs).iter().map(|v| v.len).fold(0.0f32, f32::max);
    let to_wall = a.distance(wall.2);
    assert!(reach < to_wall + 2.5, "stopped at the wall: {reach} vs {to_wall}");
    assert!(w.player(B).unwrap().damage_pct <= pct0 + 1e-3, "B is safe behind it");
    assert!(w.constructs.get(wall.0).is_none_or(|c| c.integrity < wall.1 || c.gone.is_some()), "the wall took it");
}

/// Both cast ultimate palm energy and fire at each other; returns the world once a clash formed.
fn clash_setup() -> World {
    let mut w = duel(30.0);
    give_hands(&mut w, A, Vec3::NEG_Z, true);
    give_hands(&mut w, B, Vec3::Z, true);
    let evs = run(
        &mut w,
        3.0,
        |_, _| vec![(A, inp(0.0, 0.0, buttons::PRIMARY)), (B, inp(PI, 0.0, buttons::PRIMARY))],
        |e| beams(e).iter().any(|v| matches!(v.phase, BeamPhase::Clash { .. })),
    );
    assert!(beams(&evs).iter().any(|v| matches!(v.phase, BeamPhase::Clash { .. })), "a clash forms where the beams meet");
    w
}

fn clash_point(w: &World) -> Option<Vec3> {
    bd_sim::signature::palm::views(w).iter().find_map(|v| match v.phase {
        BeamPhase::Clash { point, .. } => Some(point),
        _ => None,
    })
}

#[test]
fn beam_clash_forms_and_the_stronger_push_wins() {
    let mut w = clash_setup();
    let (a, b) = (w.player(A).unwrap().mv.pos, w.player(B).unwrap().mv.pos);
    let p0 = clash_point(&w).expect("clash");
    let (pa0, pb0) = (w.player(A).unwrap().damage_pct, w.player(B).unwrap().damage_pct);
    // A pours willpower in (holds), B doesn't: the point drifts to B and blows up on them.
    let mut det = None;
    run(
        &mut w,
        6.0,
        |w, _| {
            let _ = w;
            vec![(A, inp(0.0, 0.0, buttons::PRIMARY)), (B, inp(PI, 0.0, 0))]
        },
        |e| {
            det = det.or(beams(e).iter().find_map(|v| match v.phase {
                BeamPhase::Detonate { point, .. } => Some(point),
                _ => None,
            }));
            det.is_some()
        },
    );
    let p = det.expect("it detonates");
    assert!(p.distance(b) < p0.distance(b), "pushed toward the weaker side: {p0:?} -> {p:?}");
    assert!(p.distance(b) < p.distance(a), "blew up on B's side");
    let (da, db) = (w.player(A).unwrap().damage_pct - pa0, w.player(B).unwrap().damage_pct - pb0);
    assert!(db > da, "the loser takes more: A +{da}, B +{db}");
}

#[test]
fn letting_go_ends_the_clash_and_the_other_beam_blasts_through() {
    let mut w = clash_setup();
    let pb0 = w.player(B).unwrap().damage_pct;
    // B dodges out (control is never taken): their beam ends, A's carries on to B.
    let evs = run(
        &mut w,
        1.5,
        |_, t| vec![(A, inp(0.0, 0.0, buttons::PRIMARY)), (B, inp(PI, 0.0, if t < 0.1 { buttons::DODGE } else { 0 }))],
        |_| false,
    );
    assert!(beams(&evs).iter().any(|v| v.owner == B && v.phase == BeamPhase::End), "B's beam ends");
    assert!(!beams(&evs).iter().any(|v| matches!(v.phase, BeamPhase::Detonate { .. })), "no detonation");
    assert!(beams(&evs).iter().any(|v| v.owner == A && v.phase == BeamPhase::Live), "A's beam is free again");
    assert!(w.player(B).unwrap().damage_pct > pb0, "and reaches B");
}

#[test]
fn an_even_clash_detonates_when_time_runs_out() {
    let mut w = clash_setup();
    let t = w.tuning().signature.palm.clash_max;
    let mut det = false;
    let mut time = 0.0;
    run(
        &mut w,
        t + 1.5,
        |w, _| {
            let _ = w;
            vec![(A, inp(0.0, 0.0, buttons::PRIMARY)), (B, inp(PI, 0.0, buttons::PRIMARY))]
        },
        |e| {
            time += DT;
            det |= beams(e).iter().any(|v| matches!(v.phase, BeamPhase::Detonate { .. }));
            det
        },
    );
    // (Both keep pouring; top them up so the timer, not the bar, ends it.)
    assert!(det, "detonates");
    assert!(time <= t + 0.2, "by the clash timer: {time}");
    let _ = &mut w;
}

#[test]
fn palm_beams_are_deterministic_and_the_caster_is_predicted() {
    let run_once = || {
        let mut w = clash_setup();
        for _ in 0..(2.0 / DT) as usize {
            w.tick(&[(A, inp(0.0, 0.0, buttons::PRIMARY)), (B, inp(PI, 0.0, 0))]);
        }
        (w.state_hash(), w.voxels.content_hash())
    };
    assert_eq!(run_once(), run_once());
    // The caster strafes while pouring a beam; a replica predicts them exactly.
    let mut host = duel(30.0);
    give_hands(&mut host, A, Vec3::NEG_Z, true);
    let mut client = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 160.0 }, check_invariants: true, ..SimConfig::default() });
    for e in host.construct_manifest() {
        client.apply_event(&e);
    }
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    let mut bad = 0;
    for k in 0..300u32 {
        let i = PlayerInput {
            move_x: if k % 80 < 40 { 1 } else { -1 },
            buttons: if k < 100 { buttons::PRIMARY } else { 0 },
            ..Default::default()
        };
        let mut s = client.player(A).unwrap().mv;
        client.predict_player(A, &mut s, &i);
        for e in host.tick(&[(A, i)]).to_vec() {
            client.apply_event(&e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        let h = host.player(A).unwrap().mv;
        if (h.pos - s.pos).length() > 1e-4 || (h.vel - s.vel).length() > 1e-3 {
            bad += 1;
        }
    }
    assert_eq!(bad, 0, "prediction diverged on {bad} ticks");
}

// ================================================================================ energy balls

fn fireball() -> Recipe {
    recipe("fireball", Form::Ball, &[Mechanic::Explodes], Element::Fire, 1.0)
}

fn launch_of(evs: &[GameEvent], id: ConstructId) -> Option<f32> {
    evs.iter().find_map(|e| match e {
        GameEvent::SignatureFx { id: i, fx: SigFx::Launch { radius, .. }, .. } if *i == id => Some(*radius),
        _ => None,
    })
}

#[test]
fn a_tap_throws_the_base_size_holding_grows_it_and_costs_more() {
    let throw = |hold: f32| {
        let mut w = duel(60.0);
        let id = w.cast(A, fireball(), None, aim(&w, A, Vec3::NEG_Z)).unwrap();
        let base = w.constructs.get(id).unwrap().half.max_element();
        w.player_mut(A).unwrap().willpower = 100.0;
        let evs = run(
            &mut w,
            3.0 + hold,
            |_, t| vec![(A, inp(0.0, 0.0, if t < hold { buttons::PRIMARY } else { 0 }))],
            |e| launch_of(e, id).is_some(),
        );
        let r = launch_of(&evs, id).expect("thrown");
        (r / base, w.player(A).unwrap().willpower)
    };
    let (tap, wp_tap) = throw(0.0);
    let (held, wp_held) = throw(1.6);
    assert!((tap - 1.0).abs() < 0.05, "a tap throws the base size: ×{tap}");
    assert!(held > 1.6, "holding grows it: ×{held}");
    assert!(wp_held < wp_tap - 3.0, "and costs willpower: {wp_held} vs {wp_tap}");
}

#[test]
fn a_hit_while_holding_knocks_it_loose() {
    let mut w = duel(20.0);
    let id = w.cast(A, fireball(), None, aim(&w, A, Vec3::NEG_Z)).unwrap();
    run(&mut w, 0.8, |_, _| vec![(A, inp(0.0, 0.0, buttons::PRIMARY))], |_| false);
    assert!(w.constructs.get(id).is_some_and(|c| c.gone.is_none()), "held");
    let c = w.player(A).unwrap().mv.pos + Vec3::Y;
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::Shot, 12.0, 4.0, Vec3::X, c));
    let evs = run(&mut w, 0.2, |_, _| vec![(A, inp(0.0, 0.0, buttons::PRIMARY))], |_| false);
    assert!(launch_of(&evs, id).is_none(), "never thrown");
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Explosion { pos, .. } if pos.distance(c) < 4.0)), "it burst in the hands");
    assert!(w.constructs.get(id).is_none_or(|c| c.gone.is_some()));
}

#[test]
fn a_big_ball_is_held_overhead_and_slows_the_caster() {
    let mut w = duel(40.0);
    let bomb = recipe("Spirit Bomb", Form::Ball, &[Mechanic::Explodes, Mechanic::DropFromSky, Mechanic::Heavy], Element::Light, 8.0);
    let id = w.cast_signature(A, bomb, None, aim(&w, A, Vec3::NEG_Z), SignatureCast::new(SignatureKind::Summon, 0.5)).unwrap();
    run(&mut w, 0.4, |_, _| vec![(A, inp(0.0, 0.0, buttons::PRIMARY))], |_| false);
    let c = w.constructs.get(id).unwrap();
    let h = bd_sim::signature::orb::hold_view(c).expect("in the hands");
    assert!(h.overhead, "past the size threshold it goes overhead: {h:?}");
    let eye = w.player(A).unwrap().mv.pos + Vec3::Y * 1.6;
    assert!(c.pos.y > eye.y + 1.0, "above the head: {} vs {}", c.pos.y, eye.y);
    assert!(!c.recipe.has(Mechanic::DropFromSky), "a summoned ball of energy is thrown, not dropped");
    // Running forward while holding it is slower than empty-handed.
    let speed = |w: &mut World| {
        let mut v = 0.0f32;
        for _ in 0..60 {
            w.tick(&[(A, PlayerInput { move_z: 1, buttons: buttons::PRIMARY, ..Default::default() })]);
            v = w.player(A).unwrap().mv.vel.length();
        }
        v
    };
    let slow = speed(&mut w);
    let mut free = duel(40.0);
    let fast = speed(&mut free);
    assert!(slow < fast * 0.8, "slower as it grows: {slow} vs {fast}");
}

#[test]
fn a_beam_pushes_back_a_thrown_ball() {
    let mut w = duel(45.0);
    give_hands(&mut w, A, Vec3::NEG_Z, true);
    // B conjures a ball of lightning and holds it; A fires; then B throws it straight down the beam.
    let ball = w.cast(B, recipe("ball of lightning", Form::Ball, &[], Element::Electric, 2.0), None, aim(&w, B, Vec3::Z)).unwrap();
    run(&mut w, 1.5, |_, _| vec![(A, inp(0.0, 0.0, buttons::PRIMARY)), (B, inp(PI, 0.0, buttons::PRIMARY))], |e| !beams(e).is_empty());
    let a = w.player(A).unwrap().mv.pos;
    let mut clash = false;
    let mut ended_at = None;
    run(
        &mut w,
        4.0,
        |w, _| {
            let _ = w;
            vec![(A, inp(0.0, 0.0, buttons::PRIMARY)), (B, inp(PI, 0.0, 0))]
        },
        |e| {
            clash |= beams(e).iter().any(|v| matches!(v.phase, BeamPhase::Clash { .. }));
            if let Some(p) = e.iter().find_map(|x| match x {
                GameEvent::SignatureFx { id, fx: SigFx::End, pos, .. } if *id == ball => Some(*pos),
                _ => None,
            }) {
                ended_at = Some(p);
            }
            ended_at.is_some()
        },
    );
    assert!(clash, "the beam catches the ball: a struggle");
    let p = ended_at.expect("the ball is beaten");
    assert!(p.distance(a) > 12.0, "pushed back and burst far from A: {:.1} m", p.distance(a));
}

#[test]
fn two_thrown_balls_meet_and_burst() {
    let mut w = duel(40.0);
    let fa = w.cast(A, fireball(), None, aim(&w, A, Vec3::NEG_Z)).unwrap();
    let fb = w.cast(B, fireball(), None, aim(&w, B, Vec3::Z)).unwrap();
    let a = w.player(A).unwrap().mv.pos;
    let mut blast = None;
    run(
        &mut w,
        3.0,
        |_, _| vec![(A, inp(0.0, 0.0, 0)), (B, inp(PI, 0.0, 0))],
        |e| {
            blast = blast.or(e.iter().find_map(|x| match x {
                GameEvent::Explosion { pos, .. } => Some(*pos),
                _ => None,
            }));
            blast.is_some()
        },
    );
    let p = blast.expect("they burst");
    let mid = (p - a).dot(Vec3::NEG_Z);
    assert!((10.0..30.0).contains(&mid), "at the meeting point, not on a player: {mid:.1} m out");
    assert!(w.constructs.get(fa).is_none_or(|c| c.gone.is_some()) && w.constructs.get(fb).is_none_or(|c| c.gone.is_some()));
}

/// Tick cost (`--release -- --ignored --nocapture`): the same duel idle vs. two ultimates clashing
/// (pouring, boring, pulsing) plus a held ball growing.
#[test]
#[ignore = "perf probe (release build)"]
fn energy_tick_cost() {
    let run = |energy: bool| {
        let mut w = duel(30.0);
        w.cfg.rules.willpower = bd_sim::rules::WillpowerMode::Infinite;
        if energy {
            give_hands(&mut w, A, Vec3::NEG_Z, true);
            give_hands(&mut w, B, Vec3::Z, true);
        }
        let mut times = Vec::new();
        for k in 0..1800u32 {
            let b = if energy { buttons::PRIMARY } else { 0 };
            if energy && k == 600 {
                // A third caster's ball appears mid-fight (held, growing).
                let _ = w.cast(A, fireball(), None, aim(&w, A, Vec3::NEG_Z));
            }
            let t0 = std::time::Instant::now();
            w.tick(&[(A, inp(0.0, 0.0, b)), (B, inp(PI, 0.0, b))]);
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
            for p in &mut w.players {
                p.willpower = 100.0;
            }
        }
        times.sort_by(|a, b| a.total_cmp(b));
        (times[times.len() / 2], times[times.len() * 99 / 100], times[times.len() - 1])
    };
    let off = run(false);
    let on = run(true);
    eprintln!("ENERGY tick ms (p50, p99, max): idle {off:?}, beams clashing + ball {on:?}");
}
