//! CLICK TO CONFIRM: `World::preview_spawn` (the client's ghost placement) agrees with where the
//! real cast then stands — creatures, walls, domes, held items, kaiju, a wall in the way — and says
//! "no fresh construct" for a graft onto an own construct under the crosshair. Read-only: the world
//! state hash does not move.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::voxel::world_to_voxel;
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const G: f32 = 8.0;

fn flat() -> World {
    let mut w = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, ..SimConfig::default() });
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    let a = w.player_mut(A).unwrap();
    a.mv.pos = Vec3::new(10.0, G, 5.0);
    a.mv.grounded = true;
    w.player_mut(B).unwrap().mv.pos = Vec3::new(-40.0, G, -40.0);
    w
}

fn aim(w: &mut World, yaw: f32, pitch: f32) -> Aim {
    let p = w.player_mut(A).unwrap();
    p.input.yaw = yaw;
    p.input.pitch = pitch;
    let eye = w.player(A).unwrap().eye(w.params());
    Aim::new(eye, bd_core::input::look_dir(yaw, pitch))
}

/// Preview, then cast for real with the same aim: same spot, same facing.
fn agrees(w: &mut World, r: Recipe, yaw: f32, pitch: f32) {
    let a = aim(w, yaw, pitch);
    let before = w.state_hash();
    let pv = w.preview_spawn(A, r.clone(), None, a).unwrap_or_else(|| panic!("{}: a fresh construct", r.name));
    assert_eq!(w.state_hash(), before, "{}: the preview is read-only", r.name);
    let id = w.cast(A, r.clone(), None, a).unwrap();
    let c = w.constructs.get(id).unwrap();
    assert!(c.pos.distance(pv.pos) < 0.05, "{}: preview {} vs cast {}", r.name, pv.pos, c.pos);
    assert!(c.rot.dot(pv.rot).abs() > 0.999, "{}: facing", r.name);
    assert_eq!(pv.recipe.form, c.recipe.form);
    w.dismiss(A);
    w.tick(&[]);
}

#[test]
fn the_ghost_stands_where_the_cast_will() {
    let mut w = flat();
    let yaw = std::f32::consts::FRAC_PI_2;
    for r in [
        recipe("dragon", Form::Serpent, &[Mechanic::Flies], Element::Fire, 1.5),
        recipe("wolf", Form::Quadruped, &[], Element::Light, 1.0),
        recipe("wall", Form::Wall, &[], Element::Earth, 1.0),
        recipe("dome", Form::Dome, &[], Element::Light, 1.0),
        recipe("hammer", Form::Hammer, &[], Element::Light, 1.0),
        recipe("anvil", Form::Anvil, &[], Element::Light, 1.0),
        recipe("godzilla", Form::Kaiju, &[], Element::Fire, 1.0),
    ] {
        agrees(&mut w, r, yaw, -0.05);
    }
    // looking at the sky / at the floor
    agrees(&mut w, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.0), yaw, 1.2);
    agrees(&mut w, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.0), yaw, -1.4);
    // a wall in the way: walked back toward the caster exactly like the cast
    let lo = world_to_voxel(Vec3::new(3.0, G, -2.0));
    let hi = world_to_voxel(Vec3::new(3.9, G + 3.9, 12.0));
    w.voxels.fill_box(lo, hi, 3);
    agrees(&mut w, recipe("big wolf", Form::Quadruped, &[], Element::Light, 2.5), yaw, -0.05);
}

#[test]
fn aimed_at_an_own_construct_it_would_graft_so_no_ghost() {
    let mut w = flat();
    let yaw = std::f32::consts::FRAC_PI_2;
    let a = aim(&mut w, yaw, -0.05);
    let wall = w.cast(A, recipe("wall", Form::Wall, &[], Element::Light, 1.0), None, a).unwrap();
    w.tick(&[]);
    // aimed at it ("it", the crosshair on it): the real cast grafts, and the preview says so
    let eye = w.player(A).unwrap().eye(w.params());
    let at = w.constructs.get(wall).unwrap().center();
    let a = Aim { target_hint: Some(bd_sim::TargetHint::Construct(wall)), ..Aim::new(eye, (at - eye).normalize()) };
    let wings = recipe("wings", Form::Wings, &[Mechanic::Flies], Element::Light, 1.0);
    let pv = w.preview_spawn(A, wings.clone(), None, a);
    assert_eq!(w.cast(A, wings.clone(), None, a), Ok(wall), "the real cast grafts onto the wall");
    assert!(pv.is_none(), "{pv:?}");
    // whatever the crosshair is on, the preview and the cast agree on "fresh or graft"
    for pitch in [-0.4f32, -0.2, -0.05, 0.1] {
        let a = aim(&mut w, yaw, pitch);
        let pv = w.preview_spawn(A, wings.clone(), None, a);
        let id = w.cast(A, wings.clone(), None, a).unwrap();
        assert_eq!(pv.is_none(), id == wall, "pitch {pitch}: {pv:?} vs {id:?}");
        if id != wall {
            w.dismiss_one(A, id);
        }
        w.tick(&[]);
    }
    // a modifier phrase aimed at terrain animates it: no fresh construct either
    w.dismiss(A);
    w.tick(&[]);
    let a = aim(&mut w, yaw, -0.3);
    assert!(w.preview_spawn(A, recipe("alive", Form::Blob, &[Mechanic::Walks], Element::Light, 1.0), None, Aim { modifier: true, ..a }).is_none());
}
