//! "Animate the world" (COMBAT.md §3c): the construct model of a graft that lifted terrain.
//!
//! The sim carries the lifted voxels on the construct (`Construct::model`, 0.5 m terrain voxels with
//! their original materials, pivot = bottom centre). The client shows *those voxels* — in their own
//! material colours, tinted with hard light (bd_fx [`VoxelColors`]) — as the construct's body, and
//! grows the phrase's hard-light limbs (legs / wings / arms from the recipe's procedural model)
//! under and around it so "a tree + legs" walks away on hard-light legs.
//!
//! Pure function, run on a worker thread (see `constructs::start_build`).

use bd_core::mechanics::Recipe;
use bd_core::shape::Tint;
use bd_core::{IVec3, Vec3};
use bd_fx::VoxelColors;
use bd_shapes::{PartModel, VoxelModel};
use bd_sim::constructs::VoxelModel as TerrainModel;
use std::collections::HashMap;

/// Terrain voxel edge (m); the whole model is built at this size so limbs match the body's grain.
const VS: f32 = bd_core::VOXEL_SIZE;
/// Limb body width clamp (m): the procedural body the limbs are laid out for.
const MIN_BODY: f32 = 1.5;
const MAX_BODY: f32 = 8.0;

/// Deterministic per-voxel brightness jitter (same spirit as the terrain mesher's).
fn jitter(v: IVec3, amp: f32) -> f32 {
    let mut h = (v.x as u32).wrapping_mul(0x8da6_b343) ^ (v.y as u32).wrapping_mul(0xd816_3841) ^ (v.z as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    1.0 + amp * ((h >> 8) as f32 / 8_388_608.0 - 1.0)
}

/// sRGB colour of a terrain voxel (material colour with per-voxel jitter).
fn voxel_srgb(world_v: IVec3, m: bd_core::voxel::Material) -> [u8; 3] {
    let d = bd_voxel::material_def(m);
    let j = jitter(world_v, d.jitter);
    [0, 1, 2].map(|i| (d.color[i] as f32 * j).clamp(0.0, 255.0) as u8)
}

/// Build the rendered model of a terrain construct: the lifted voxels as the root part + the
/// recipe's procedural limbs. Returns the model and its per-voxel colours.
pub fn terrain_construct_model(recipe: &Recipe, terrain: &TerrainModel) -> (VoxelModel, VoxelColors) {
    // Footprint of the bottom layers (a tree's trunk, a boulder's base): the limbs are laid out for
    // a procedural body that wide, so legs end up under the piece instead of splayed around it.
    let (mut lo, mut hi) = (IVec3::MAX, IVec3::MIN);
    for (v, _) in terrain.voxels.iter().filter(|(v, _)| v.y <= terrain.min.y + 2) {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    let foot = if lo.x > hi.x { 1.0 } else { ((hi.x - lo.x + 1).max(hi.z - lo.z + 1)) as f32 * VS };
    let mut r = recipe.clone();
    r.scale = foot.clamp(MIN_BODY, MAX_BODY);
    let (spec, machine) = bd_shapes::generate_machine(&r);
    // 0.5 m voxels (k = 2 × the 0.25 m construct unit), same grain as the terrain.
    let mut model = bd_shapes::voxelize_at(&spec, (VS / bd_core::CONSTRUCT_VOXEL_SIZE).round() as u32);
    bd_shapes::attach_machine(&mut model, &machine);

    // Where the procedural body sat: the terrain's bottom centre goes there.
    let has_limbs = model.parts.iter().skip(1).any(|p| !p.voxels.is_empty());
    let body_bottom = match model.parts.first() {
        Some(p) if has_limbs && !p.voxels.is_empty() => {
            let a = p.aabb();
            Vec3::new((a.min.x + a.max.x) * 0.5, a.min.y, (a.min.z + a.max.z) * 0.5)
        }
        // No limbs (a boulder): the pivot is the model's bottom centre.
        _ => Vec3::ZERO,
    };
    let s = model.voxel_size.max(1e-3);
    // Terrain voxel centre relative to its bottom centre, in voxels (x/z may be half-integers —
    // every voxel shares the same fraction, so rounding shifts the whole piece consistently).
    let cx = (terrain.min.x + terrain.max.x) as f32 * 0.5;
    let cz = (terrain.min.z + terrain.max.z) as f32 * 0.5;
    let base = body_bottom / s;
    let mut voxels: Vec<(IVec3, Tint)> = Vec::with_capacity(terrain.voxels.len());
    let mut colors: HashMap<IVec3, [u8; 3]> = HashMap::with_capacity(terrain.voxels.len());
    for (v, m) in &terrain.voxels {
        let d = Vec3::new(v.x as f32 - cx, (v.y - terrain.min.y) as f32 + 0.5, v.z as f32 - cz) * (VS / s);
        let mv = (base + d).floor().as_ivec3();
        voxels.push((mv, Tint::Primary));
        colors.insert(mv, voxel_srgb(terrain.origin + *v, *m));
    }
    voxels.sort_by_key(|(v, _)| (v.x, v.y, v.z));
    voxels.dedup_by_key(|(v, _)| *v);

    let mut parts_colors = vec![HashMap::new(); model.parts.len().max(1)];
    if model.parts.is_empty() {
        model.parts.push(PartModel {
            id: 0,
            parent: 0,
            parent_index: 0,
            pivot: bd_core::Vec3::ZERO,
            role: None,
            voxels: Vec::new(),
            vmin: IVec3::ZERO,
            vmax: IVec3::ZERO,
            depth: 0,
            voxel_size: s,
        });
    }
    let root = &mut model.parts[0];
    root.vmin = voxels.iter().fold(IVec3::MAX, |a, (v, _)| a.min(*v));
    root.vmax = voxels.iter().fold(IVec3::MIN, |a, (v, _)| a.max(*v));
    root.voxels = voxels;
    root.voxel_size = s;
    parts_colors[0] = colors;
    // Weak points on the old body no longer mean anything.
    model.weak_points.retain(|w| w.part_index != 0);
    model.voxel_count = model.parts.iter().map(|p| p.voxels.len()).sum();
    let mut b = bd_core::voxel::Aabb { min: bd_core::Vec3::splat(f32::INFINITY), max: bd_core::Vec3::splat(f32::NEG_INFINITY) };
    for p in model.parts.iter().filter(|p| !p.voxels.is_empty()) {
        let a = p.aabb();
        b.min = b.min.min(a.min);
        b.max = b.max.max(a.max);
    }
    if b.min.is_finite() {
        model.bounds = b;
    }
    model.instances = bd_shapes::part_instances(&model.parts);
    (model, VoxelColors { parts: parts_colors })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::mechanics::{Element, Form, Mechanic};

    fn tree() -> TerrainModel {
        // A 1×1 trunk 6 tall with a 5×5×3 canopy on top (wood + leaves).
        let mut voxels = Vec::new();
        for y in 0..6 {
            voxels.push((IVec3::new(0, y, 0), bd_voxel::mat::WOOD));
        }
        for y in 6..9 {
            for x in -2..=2 {
                for z in -2..=2 {
                    voxels.push((IVec3::new(x, y, z), bd_voxel::mat::LEAVES_GREEN));
                }
            }
        }
        TerrainModel { origin: IVec3::new(10, 20, 30), voxels, min: IVec3::new(-2, 0, -2), max: IVec3::new(2, 8, 2) }
    }

    #[test]
    fn tree_with_legs_keeps_its_voxels_and_grows_limbs() {
        let t = tree();
        let r = bd_sim::bots::recipe("tree", Form::Blob, &[Mechanic::Walks], Element::Earth, 4.5);
        let (m, c) = terrain_construct_model(&r, &t);
        assert_eq!(m.parts[0].voxels.len(), t.voxels.len(), "every lifted voxel is drawn");
        assert_eq!(c.parts[0].len(), t.voxels.len());
        assert!(m.parts.len() > 1, "walking terrain grows hard-light legs");
        assert!(m.rig.legs.len() >= 2 || m.parts.iter().any(|p| p.role.is_some()));
        // Leaves stay green, the trunk stays brown.
        let top = m.parts[0].vmax.y;
        let leaf = c.parts[0].iter().find(|(v, _)| v.y == top).map(|(_, c)| *c).unwrap();
        assert!(leaf[1] > leaf[0] && leaf[1] > leaf[2], "{leaf:?}");
        // The body sits on the legs (above the feet).
        assert!(m.parts[0].vmin.y as f32 * m.voxel_size > 0.2);
        // Feet within ~a leg segment of the pivot, like the procedural creature the limbs come from.
        assert!(m.bounds.min.y > -1.0 && m.bounds.min.y < 0.6, "feet near the pivot: {:?}", m.bounds);
    }

    #[test]
    fn boulder_without_limbs_is_just_the_terrain() {
        let t = tree();
        let r = bd_sim::bots::recipe("rock", Form::Boulder, &[], Element::Earth, 4.5);
        let (m, _) = terrain_construct_model(&r, &t);
        let n: usize = m.parts.iter().map(|p| p.voxels.len()).sum();
        assert_eq!(n, t.voxels.len());
        assert!(m.parts[0].vmin.y.abs() <= 1);
    }
}

// ------------------------------------------------------------------------------ dev check

/// `BD_DEV_GRAFT=<phrase>` (e.g. `legs`): dev/screenshot check for the animated world — one second
/// into an offline match the local player grafts the phrase onto the nearest tree trunk within
/// ~20 m (prints where, for `BD_SHOT_CAM`). Never set in play.
pub fn dev_graft(
    time: bevy::prelude::Res<bevy::prelude::Time<bevy::prelude::Real>>,
    active: Option<bevy::prelude::Res<crate::sim_bridge::ActiveWorld>>,
    lex: Option<bevy::prelude::Res<crate::console::RingLexicon>>,
    mut cmds: bevy::prelude::MessageWriter<crate::sim_bridge::SimCommand>,
    mut st: bevy::prelude::Local<(f32, bool)>,
) {
    static PHRASE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let Some(phrase) = PHRASE.get_or_init(|| std::env::var("BD_DEV_GRAFT").ok()) else { return };
    let (Some(active), Some(lex)) = (active, lex) else { return };
    if st.1 {
        return;
    }
    st.0 += time.delta_secs();
    if st.0 < 1.0 {
        return;
    }
    st.1 = true;
    let Some(me) = active.local_player() else { return };
    let w = &active.world.voxels;
    let p = bd_core::voxel::world_to_voxel(me.mv.pos);
    use bd_voxel::mat;
    let mut best: Option<(i32, IVec3)> = None;
    for dz in -40..=40 {
        for dx in -40..=40 {
            for dy in -12..=12 {
                let v = p + IVec3::new(dx, dy, dz);
                let m = w.get(v);
                if ![mat::WOOD, mat::DARK_WOOD, mat::PALM_TRUNK].contains(&m) || w.get(v - IVec3::Y) == m {
                    continue;
                }
                let d = dx * dx + dz * dz + dy * dy;
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, v));
                }
            }
        }
    }
    let Some((_, base)) = best else {
        println!("DEVGRAFT no tree near {p:?}");
        return;
    };
    let target = bd_core::voxel::voxel_center(base + IVec3::new(0, 4, 0));
    let eye = me.mv.pos + Vec3::Y * 1.6;
    let dir = (target - eye).normalize();
    let res = lex.0.resolve(phrase);
    println!("DEVGRAFT {phrase:?} at {target:?} from {eye:?} kind={:?}", res.kind);
    let aim = bd_sim::Aim { origin: eye, dir, target_hint: None, modifier: true };
    cmds.write(crate::sim_bridge::SimCommand::local(crate::sim_bridge::Cmd::Cast { recipe: Box::new(res.recipe), shape: None, alt_shape: None, aim, incantation: false }));
}
