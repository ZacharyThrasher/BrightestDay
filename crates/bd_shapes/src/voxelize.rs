//! `ShapeSpec` → `VoxelModel` at [`CONSTRUCT_VOXEL_SIZE`] (0.25 m).
//!
//! Conventions
//! * Voxel `v: IVec3` is the cube of edge 0.25 m **centred** on `v · 0.25` m in the model's rest pose
//!   (so a width-1 primitive centred on x = 0 is exactly one voxel wide and symmetric shapes stay
//!   symmetric).
//! * Every part keeps its voxels in *model* rest-pose coordinates; `anim::pose` returns one model-space
//!   `Affine3A` per part that moves those rest voxels to the animated pose.
//! * Additive prims paint into their own part's grid (later prims overwrite the tint of earlier ones —
//!   "paint by overlap"); `subtract` prims carve every part painted so far.
//! * Interiors are hollowed to a shell of [`SHELL`] voxels (6-connected distance), which is all a hard
//!   light construct needs for looks and shatter.

use bd_core::shape::{PartId, PartRole, PrimKind, ShapeSpec, Tint};
use bd_core::voxel::Aabb;
use bd_core::CONSTRUCT_VOXEL_SIZE;
use glam::{IVec3, Quat, Vec3};

use crate::anim::Rig;
use crate::build::quat_from_rot_deg;

/// Voxel cap for any construct that is not a kaiju.
pub const MAX_VOXELS: usize = 6_000;
/// Voxel cap for kaiju.
pub const MAX_VOXELS_KAIJU: usize = 40_000;
/// Primitives beyond this many are ignored (malformed specs).
pub const MAX_PRIMS: usize = 4096;
/// Declared parts beyond this many are ignored (their prims go to the root).
pub const MAX_PARTS: usize = 96;
/// Coordinates/sizes are clamped to this many construct units.
const MAX_COORD: f32 = 4096.0;
/// Per-axis grid limit (cells) per part.
const MAX_GRID: i32 = 192;
/// Shell thickness kept when hollowing.
pub const SHELL: u8 = 2;

#[derive(Clone, Debug, PartialEq)]
pub struct PartModel {
    pub id: PartId,
    /// Parent part id (the root's parent is itself, 0).
    pub parent: PartId,
    /// Index of the parent in `VoxelModel::parts` (parents always precede children; root → 0).
    pub parent_index: usize,
    /// Pivot in metres, model rest-pose space.
    pub pivot: Vec3,
    /// `None` for the root/body part.
    pub role: Option<PartRole>,
    /// Model-space rest-pose voxel coordinates (centre = v · 0.25 m) and palette slot. Sorted.
    pub voxels: Vec<(IVec3, Tint)>,
    /// Inclusive voxel bounds (meaningless when `voxels` is empty).
    pub vmin: IVec3,
    pub vmax: IVec3,
    /// Hierarchy depth (root = 0).
    pub depth: u8,
    /// Edge length of this model's voxels in metres (see [`VoxelModel::voxel_size`]).
    pub voxel_size: f32,
}

impl PartModel {
    pub fn is_empty(&self) -> bool {
        self.voxels.is_empty()
    }
    /// Rest-pose AABB of this part in metres.
    pub fn aabb(&self) -> Aabb {
        if self.voxels.is_empty() {
            return Aabb { min: self.pivot, max: self.pivot };
        }
        let h = self.voxel_size * 0.5;
        Aabb {
            min: self.vmin.as_vec3() * self.voxel_size - Vec3::splat(h),
            max: self.vmax.as_vec3() * self.voxel_size + Vec3::splat(h),
        }
    }
    /// Rest-pose centre of voxel `v` in metres.
    #[inline]
    pub fn voxel_center(&self, v: IVec3) -> Vec3 {
        v.as_vec3() * self.voxel_size
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VoxelModel {
    /// Root first, parents before children.
    pub parts: Vec<PartModel>,
    /// Rest-pose bounds in metres.
    pub bounds: Aabb,
    pub voxel_count: usize,
    /// Voxel edge in metres: 0.25 (`CONSTRUCT_VOXEL_SIZE`) for normal constructs, doubling (0.5, 1, 2)
    /// for colossal ones so counts stay within the caps (COMBAT.md §5e).
    pub voxel_size: f32,
    /// Animation rig derived from the part roles (see `anim`).
    pub rig: Rig,
    /// Pilot seat (Mechanic::Piloted), if the spec declares one (part id [`SEAT_PART`]).
    pub seat: Option<Seat>,
    /// Weak points (joints, neck/nape, cockpit): x3 Blade/Pierce, subtle glow on the client.
    pub weak_points: Vec<WeakPoint>,
    /// Metres per ShapeSpec unit of the spec this model was built from (0.25 for constructs unless
    /// the cap forced a shrink; `costume::COSTUME_UNIT` for costumes).
    pub unit_m: f32,
    /// Machine-kit motion rig (wheels, cranks, rotors…; see `features`). Index-aligned with `parts`.
    pub machine: crate::features::MachineRig,
    /// Mesh sharing: `instances[i]` says part `i` has the same voxels as part `source` shifted by
    /// `offset` (m, rest pose). Canonical parts point at themselves with a zero offset.
    pub instances: Vec<PartInstance>,
}

/// Mesh-instancing info for one part: render part `i` as `pose[i] * T(offset) * mesh[source]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartInstance {
    pub source: usize,
    pub offset: Vec3,
}

/// Parts with identical voxels (up to a translation) share the first such part's mesh.
pub fn part_instances(parts: &[PartModel]) -> Vec<PartInstance> {
    let mut out: Vec<PartInstance> = Vec::with_capacity(parts.len());
    for (i, p) in parts.iter().enumerate() {
        let mut inst = PartInstance { source: i, offset: Vec3::ZERO };
        if !p.voxels.is_empty() {
            for (j, q) in parts.iter().enumerate().take(i) {
                if out[j].source != j || q.voxels.len() != p.voxels.len() || q.vmax - q.vmin != p.vmax - p.vmin || q.voxel_size != p.voxel_size {
                    continue;
                }
                let d = p.vmin - q.vmin;
                if q.voxels.iter().zip(&p.voxels).all(|((a, ta), (b, tb))| *a + d == *b && ta == tb) {
                    inst = PartInstance { source: j, offset: d.as_vec3() * p.voxel_size };
                    break;
                }
            }
        }
        out.push(inst);
    }
    out
}

/// Pilot seat anchor. Posed seat transform = `pose[part_index] * Affine3A::from_translation(pos)`;
/// the pilot faces the model's -Z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seat {
    pub part_index: usize,
    /// Metres, rest-pose model space.
    pub pos: Vec3,
}

/// A weak-point sphere attached to a part (rest-pose model space, metres; pose with `pose[part_index]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeakPoint {
    pub part: PartId,
    pub part_index: usize,
    pub center: Vec3,
    pub radius: f32,
    pub kind: WeakKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WeakKind {
    /// Hip joint of a leg.
    LegJoint,
    /// Shoulder of an arm / pincer.
    ArmJoint,
    /// Wing root.
    WingRoot,
    /// Neck / "nape" behind the head (the AoT spot).
    Nape,
    /// Tail base.
    TailBase,
    /// Pilot cockpit.
    Cockpit,
}

/// Reserved part id for the pilot-seat marker: a part with this id (any role, no prims needed) marks
/// the seat at its pivot, parented to the part it rides on. Never rendered or animated on its own.
pub const SEAT_PART: PartId = 255;

impl VoxelModel {
    pub fn part_index(&self, id: PartId) -> Option<usize> {
        self.parts.iter().position(|p| p.id == id)
    }
    /// Rest-pose centre of voxel `v` in metres.
    #[inline]
    pub fn voxel_center(&self, v: IVec3) -> Vec3 {
        v.as_vec3() * self.voxel_size
    }
    /// Posed seat transform (pilot faces -Z of the result).
    pub fn seat_transform(&self, pose: &[glam::Affine3A]) -> Option<glam::Affine3A> {
        let s = self.seat?;
        Some(pose.get(s.part_index).copied().unwrap_or(glam::Affine3A::IDENTITY) * glam::Affine3A::from_translation(s.pos))
    }
}

/// Rest-pose centre of voxel `v` in metres for a 0.25 m model (prefer [`VoxelModel::voxel_center`]).
#[inline]
pub fn voxel_center_m(v: IVec3) -> Vec3 {
    v.as_vec3() * CONSTRUCT_VOXEL_SIZE
}

// ---------------------------------------------------------------------------------------------

pub(crate) struct PrimEval {
    kind: PrimKind,
    c: Vec3,
    inv: Quat,
    e: Vec3,
    pub lo: IVec3,
    pub hi: IVec3,
}

fn eff(h: f32) -> f32 {
    if h < 0.51 { 0.51 } else { h - 1e-3 }
}

impl PrimEval {
    pub fn new(kind: PrimKind, center: Vec3, size: Vec3, rot_deg: Vec3) -> Self {
        let size = size.abs();
        let e = Vec3::new(eff(size.x * 0.5), eff(size.y * 0.5), eff(size.z * 0.5));
        let q = quat_from_rot_deg(rot_deg);
        let mut mn = Vec3::splat(f32::MAX);
        let mut mx = Vec3::splat(f32::MIN);
        for i in 0..8 {
            let corner = Vec3::new(
                if i & 1 == 0 { -e.x } else { e.x },
                if i & 2 == 0 { -e.y } else { e.y },
                if i & 4 == 0 { -e.z } else { e.z },
            );
            let p = q * corner + center;
            mn = mn.min(p);
            mx = mx.max(p);
        }
        PrimEval {
            kind,
            c: center,
            inv: q.inverse(),
            e,
            lo: (mn - Vec3::splat(1e-4)).ceil().as_ivec3(),
            hi: (mx + Vec3::splat(1e-4)).floor().as_ivec3(),
        }
    }

    #[inline]
    pub fn inside(&self, p: Vec3) -> bool {
        let l = self.inv * (p - self.c);
        let e = self.e;
        match self.kind {
            PrimKind::Box => l.x.abs() <= e.x && l.y.abs() <= e.y && l.z.abs() <= e.z,
            PrimKind::Ellipsoid => (l / e).length_squared() <= 1.0,
            PrimKind::Cylinder => {
                l.y.abs() <= e.y && (l.x / e.x).powi(2) + (l.z / e.z).powi(2) <= 1.0
            }
            PrimKind::Cone => {
                if l.y.abs() > e.y {
                    return false;
                }
                let f = (l.y + e.y) / (2.0 * e.y);
                let rx = (e.x * (1.0 - f)).max(0.45);
                let rz = (e.z * (1.0 - f)).max(0.45);
                (l.x / rx).powi(2) + (l.z / rz).powi(2) <= 1.0
            }
        }
    }
}

/// Dense per-part grid: 0 = empty, 1..=4 = tint + 1.
pub(crate) struct Grid {
    pub min: IVec3,
    pub dim: IVec3,
    pub data: Vec<u8>,
}

impl Grid {
    pub fn new(min: IVec3, max: IVec3) -> Self {
        let dim = (max - min + IVec3::ONE).max(IVec3::ZERO);
        let n = (dim.x as usize) * (dim.y as usize) * (dim.z as usize);
        Grid { min, dim, data: vec![0; n] }
    }
    #[inline]
    pub fn idx(&self, p: IVec3) -> Option<usize> {
        let l = p - self.min;
        if l.x < 0 || l.y < 0 || l.z < 0 || l.x >= self.dim.x || l.y >= self.dim.y || l.z >= self.dim.z {
            return None;
        }
        Some((l.x + self.dim.x * (l.y + self.dim.y * l.z)) as usize)
    }
    #[inline]
    pub fn get(&self, p: IVec3) -> u8 {
        self.idx(p).map_or(0, |i| self.data[i])
    }
}

pub(crate) fn tint_code(t: Tint) -> u8 {
    match t {
        Tint::Primary => 1,
        Tint::Accent => 2,
        Tint::Dark => 3,
        Tint::Core => 4,
    }
}
pub(crate) fn code_tint(c: u8) -> Tint {
    match c {
        2 => Tint::Accent,
        3 => Tint::Dark,
        4 => Tint::Core,
        _ => Tint::Primary,
    }
}

const N6: [IVec3; 6] = [
    IVec3::new(1, 0, 0),
    IVec3::new(-1, 0, 0),
    IVec3::new(0, 1, 0),
    IVec3::new(0, -1, 0),
    IVec3::new(0, 0, 1),
    IVec3::new(0, 0, -1),
];

/// Topologically ordered part table: (id, parent id, parent index, pivot (voxel units), role, depth).
struct PartRow {
    id: PartId,
    parent: PartId,
    parent_index: usize,
    pivot: Vec3,
    role: Option<PartRole>,
    depth: u8,
}

fn sane(v: Vec3) -> Vec3 {
    if v.is_finite() { v.clamp(Vec3::splat(-MAX_COORD), Vec3::splat(MAX_COORD)) } else { Vec3::ZERO }
}

fn order_parts(spec: &ShapeSpec) -> Vec<PartRow> {
    let mut rows = vec![PartRow { id: 0, parent: 0, parent_index: 0, pivot: Vec3::ZERO, role: None, depth: 0 }];
    // Unique declared parts (first declaration wins, id 0 is reserved for the root).
    let mut decl: Vec<&bd_core::shape::Part> = Vec::new();
    for p in &spec.parts {
        if decl.len() >= MAX_PARTS {
            break;
        }
        if p.id != 0 && !decl.iter().any(|d| d.id == p.id) {
            decl.push(p);
        }
    }
    let mut placed = vec![false; decl.len()];
    // BFS by parent; anything unreachable (missing parent / cycle) hangs off the root.
    loop {
        let mut progressed = false;
        for (i, p) in decl.iter().enumerate() {
            if placed[i] {
                continue;
            }
            if let Some(pi) = rows.iter().position(|r| r.id == p.parent) {
                let depth = rows[pi].depth.saturating_add(1);
                rows.push(PartRow { id: p.id, parent: p.parent, parent_index: pi, pivot: sane(p.pivot), role: Some(p.role), depth });
                placed[i] = true;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    for (i, p) in decl.iter().enumerate() {
        if !placed[i] {
            rows.push(PartRow { id: p.id, parent: 0, parent_index: 0, pivot: sane(p.pivot), role: Some(p.role), depth: 1 });
        }
    }
    rows
}

/// Voxelise a shape spec at 0.25 m (no cap). See module docs for conventions. Absurdly large specs
/// (longest extent > 192 construct units = 48 m) are voxelised coarser instead of allocating huge grids.
pub fn voxelize(spec: &ShapeSpec) -> VoxelModel {
    let b = crate::attach::spec_bounds(spec);
    let ext = (b.max - b.min).max_element();
    let mut k = 1u32;
    while ext / k as f32 > MAX_GRID as f32 && k < 64 {
        k *= 2;
    }
    voxelize_at(spec, k)
}

/// Voxelise with voxels `k` construct units wide (`k` = 1, 2, 4, 8 -> 0.25, 0.5, 1, 2 m).
pub fn voxelize_at(spec: &ShapeSpec, k: u32) -> VoxelModel {
    let k = k.max(1);
    if k == 1 {
        return voxelize_grid(spec, CONSTRUCT_VOXEL_SIZE);
    }
    let mut s = spec.clone();
    scale_spec(&mut s, 1.0 / k as f32);
    let mut m = voxelize_grid(&s, CONSTRUCT_VOXEL_SIZE * k as f32);
    m.unit_m = CONSTRUCT_VOXEL_SIZE;
    m
}

fn voxelize_grid(spec: &ShapeSpec, voxel_size: f32) -> VoxelModel {
    let rows = order_parts(spec);
    let n = rows.len();
    let part_of = |id: PartId| rows.iter().position(|r| r.id == id).unwrap_or(0);

    let evals: Vec<(usize, bool, Tint, PrimEval)> = spec
        .prims
        .iter()
        .filter(|p| p.size.is_finite() && p.center.is_finite() && p.rot_deg.is_finite())
        .take(MAX_PRIMS)
        .map(|p| {
            let c = p.center.clamp(Vec3::splat(-MAX_COORD), Vec3::splat(MAX_COORD));
            let s = p.size.abs().min(Vec3::splat(MAX_COORD));
            (part_of(p.part), p.subtract, p.tint, PrimEval::new(p.kind, c, s, p.rot_deg))
        })
        .collect();

    // Per-part bounds from additive prims.
    let mut lo = vec![IVec3::splat(i32::MAX); n];
    let mut hi = vec![IVec3::splat(i32::MIN); n];
    for (pi, sub, _, e) in &evals {
        if !*sub {
            lo[*pi] = lo[*pi].min(e.lo);
            hi[*pi] = hi[*pi].max(e.hi);
        }
    }
    // Guard against absurd specs (keeps allocation bounded: 256³ per part max).
    let mut grids: Vec<Grid> = (0..n)
        .map(|i| {
            if lo[i].x > hi[i].x {
                Grid::new(IVec3::ZERO, IVec3::splat(-1))
            } else {
                let h = hi[i].min(lo[i] + IVec3::splat(MAX_GRID - 1));
                Grid::new(lo[i], h)
            }
        })
        .collect();

    for (pi, sub, tint, e) in &evals {
        if *sub {
            for g in grids.iter_mut() {
                if g.data.is_empty() {
                    continue;
                }
                let gmax = g.min + g.dim - IVec3::ONE;
                let a = e.lo.max(g.min);
                let b = e.hi.min(gmax);
                for z in a.z..=b.z {
                    for y in a.y..=b.y {
                        for x in a.x..=b.x {
                            let p = IVec3::new(x, y, z);
                            if e.inside(p.as_vec3()) {
                                if let Some(i) = g.idx(p) {
                                    g.data[i] = 0;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            let g = &mut grids[*pi];
            if g.data.is_empty() {
                continue;
            }
            let gmax = g.min + g.dim - IVec3::ONE;
            let a = e.lo.max(g.min);
            let b = e.hi.min(gmax);
            let code = tint_code(*tint);
            for z in a.z..=b.z {
                for y in a.y..=b.y {
                    for x in a.x..=b.x {
                        let p = IVec3::new(x, y, z);
                        if e.inside(p.as_vec3()) {
                            if let Some(i) = g.idx(p) {
                                g.data[i] = code;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut parts = Vec::with_capacity(n);
    let mut count = 0usize;
    let mut bmin = Vec3::splat(f32::MAX);
    let mut bmax = Vec3::splat(f32::MIN);
    for (i, row) in rows.iter().enumerate() {
        let g = &grids[i];
        let voxels = hollow_extract(g);
        let mut vmin = IVec3::splat(i32::MAX);
        let mut vmax = IVec3::splat(i32::MIN);
        for (v, _) in &voxels {
            vmin = vmin.min(*v);
            vmax = vmax.max(*v);
        }
        if voxels.is_empty() {
            vmin = IVec3::ZERO;
            vmax = IVec3::ZERO;
        } else {
            let h = voxel_size * 0.5;
            bmin = bmin.min(vmin.as_vec3() * voxel_size - Vec3::splat(h));
            bmax = bmax.max(vmax.as_vec3() * voxel_size + Vec3::splat(h));
        }
        count += voxels.len();
        parts.push(PartModel {
            id: row.id,
            parent: row.parent,
            parent_index: row.parent_index,
            pivot: row.pivot * voxel_size,
            role: row.role,
            voxels,
            vmin,
            vmax,
            depth: row.depth,
            voxel_size,
        });
    }
    grids.clear();
    if count == 0 {
        bmin = Vec3::ZERO;
        bmax = Vec3::ZERO;
    }
    let bounds = Aabb { min: bmin, max: bmax };
    let rig = Rig::analyze(&parts, &bounds);
    let seat = parts.iter().position(|p| p.id == SEAT_PART).map(|i| Seat { part_index: parts[i].parent_index, pos: parts[i].pivot });
    let weak_points = weak_points(&parts, &rig, seat);
    let machine = crate::features::MachineRig::infer(&parts);
    let instances = part_instances(&parts);
    let mut rig = rig;
    for (i, m) in machine.motions.iter().enumerate() {
        if *m != crate::features::Motion::None {
            rig.kinds[i] = crate::anim::PartKind::Machine;
        }
    }
    VoxelModel { parts, bounds, voxel_count: count, voxel_size, rig, seat, weak_points, unit_m: voxel_size, machine, instances }
}

/// Assemble a model from explicit parts (voxels in model space, pivots in metres). Parts must be
/// ordered parents-first with valid `parent_index`; voxels are sorted/deduplicated here.
pub(crate) fn model_from_parts(mut parts: Vec<PartModel>, voxel_size: f32) -> VoxelModel {
    let mut count = 0;
    let mut bmin = Vec3::splat(f32::MAX);
    let mut bmax = Vec3::splat(f32::MIN);
    for p in &mut parts {
        p.voxels.sort_by_key(|(v, _)| (v.z, v.y, v.x));
        p.voxels.dedup_by_key(|(v, _)| *v);
        p.voxel_size = voxel_size;
        let mut vmin = IVec3::splat(i32::MAX);
        let mut vmax = IVec3::splat(i32::MIN);
        for (v, _) in &p.voxels {
            vmin = vmin.min(*v);
            vmax = vmax.max(*v);
        }
        if p.voxels.is_empty() {
            vmin = IVec3::ZERO;
            vmax = IVec3::ZERO;
        } else {
            let h = voxel_size * 0.5;
            bmin = bmin.min(vmin.as_vec3() * voxel_size - Vec3::splat(h));
            bmax = bmax.max(vmax.as_vec3() * voxel_size + Vec3::splat(h));
        }
        p.vmin = vmin;
        p.vmax = vmax;
        count += p.voxels.len();
    }
    if count == 0 {
        bmin = Vec3::ZERO;
        bmax = Vec3::ZERO;
    }
    let bounds = Aabb { min: bmin, max: bmax };
    let mut rig = Rig::analyze(&parts, &bounds);
    let weak_points = weak_points(&parts, &rig, None);
    let machine = crate::features::MachineRig::infer(&parts);
    for (i, m) in machine.motions.iter().enumerate() {
        if *m != crate::features::Motion::None {
            rig.kinds[i] = crate::anim::PartKind::Machine;
        }
    }
    let instances = part_instances(&parts);
    VoxelModel { parts, bounds, voxel_count: count, voxel_size, rig, seat: None, weak_points, unit_m: voxel_size, machine, instances }
}

/// Voxelise a spec whose unit is `unit_m` metres, one voxel per unit (e.g. costumes at 1/16 m).
pub fn voxelize_with_unit(spec: &ShapeSpec, unit_m: f32) -> VoxelModel {
    let u = if unit_m.is_finite() { unit_m.clamp(0.01, 4.0) } else { CONSTRUCT_VOXEL_SIZE };
    voxelize_grid(spec, u)
}

fn weak_points(parts: &[PartModel], rig: &Rig, seat: Option<Seat>) -> Vec<WeakPoint> {
    use crate::anim::PartKind;
    let mut out = Vec::new();
    let thick = |p: &PartModel| {
        if p.voxels.is_empty() {
            return 0.3;
        }
        let mut e = (p.vmax - p.vmin + IVec3::ONE).as_vec3() * p.voxel_size;
        // Average of the two smaller extents ~ limb thickness.
        let mx = e.max_element();
        if e.x == mx {
            e.x = 0.0
        } else if e.y == mx {
            e.y = 0.0
        } else {
            e.z = 0.0
        }
        ((e.x + e.y + e.z) * 0.5 * 0.6).max(0.3)
    };
    for (i, p) in parts.iter().enumerate() {
        let kind = match rig.kinds.get(i) {
            Some(PartKind::Leg(_)) => WeakKind::LegJoint,
            Some(PartKind::Arm(_)) => WeakKind::ArmJoint,
            Some(PartKind::Wing(_)) => WeakKind::WingRoot,
            Some(PartKind::Head) => WeakKind::Nape,
            Some(PartKind::Tail { k: 0, .. }) => WeakKind::TailBase,
            _ => continue,
        };
        let r = thick(p);
        let mut c = p.pivot;
        if kind == WeakKind::Nape {
            // Behind and slightly above the neck pivot.
            c += Vec3::new(0.0, r * 0.4, r * 0.6);
        }
        out.push(WeakPoint { part: p.id, part_index: i, center: c, radius: r, kind });
    }
    if let Some(s) = seat {
        let r = (rig.height * 0.08).max(0.4);
        out.push(WeakPoint { part: parts[s.part_index].id, part_index: s.part_index, center: s.pos, radius: r, kind: WeakKind::Cockpit });
    }
    out
}

/// Keep cells whose 6-connected distance to empty space is ≤ SHELL; return them sorted.
fn hollow_extract(g: &Grid) -> Vec<(IVec3, Tint)> {
    if g.data.is_empty() {
        return Vec::new();
    }
    let mut dist = vec![0u8; g.data.len()];
    // Layer 1: touching empty / outside.
    let mut layer = Vec::new();
    for z in 0..g.dim.z {
        for y in 0..g.dim.y {
            for x in 0..g.dim.x {
                let p = g.min + IVec3::new(x, y, z);
                let Some(i) = g.idx(p) else { continue };
                if g.data[i] == 0 {
                    continue;
                }
                if N6.iter().any(|d| g.get(p + *d) == 0) {
                    dist[i] = 1;
                    layer.push(p);
                }
            }
        }
    }
    for level in 2..=SHELL {
        let mut next = Vec::new();
        for p in &layer {
            for d in N6 {
                let q = *p + d;
                if let Some(i) = g.idx(q) {
                    if g.data[i] != 0 && dist[i] == 0 {
                        dist[i] = level;
                        next.push(q);
                    }
                }
            }
        }
        layer = next;
    }
    let mut out = Vec::new();
    for z in 0..g.dim.z {
        for y in 0..g.dim.y {
            for x in 0..g.dim.x {
                let p = g.min + IVec3::new(x, y, z);
                let Some(i) = g.idx(p) else { continue };
                if g.data[i] != 0 && dist[i] != 0 {
                    out.push((p, code_tint(g.data[i])));
                }
            }
        }
    }
    out
}

/// Uniformly scale a spec (centres, sizes, pivots) — e.g. to fit an icon to `Recipe::scale`.
pub fn scale_spec(spec: &mut ShapeSpec, factor: f32) {
    for p in &mut spec.prims {
        p.center *= factor;
        p.size *= factor;
    }
    for p in &mut spec.parts {
        p.pivot *= factor;
    }
}

/// Voxel multiple for a spec extent: the smallest power of two keeping the longest extent within 64
/// voxels (128 for kaiju-capped models).
pub fn voxel_k_for_extent(extent_units: f32, cap: usize) -> u32 {
    let lim = if cap >= MAX_VOXELS_KAIJU { 128.0 } else { 64.0 };
    let mut k = 1u32;
    while extent_units / k as f32 > lim && k < 8 {
        k *= 2;
    }
    k
}

/// Voxelise within `cap`: picks the voxel size (0.25 -> 2 m, doubling) from the spec extent, coarsens
/// further while over the cap, and only as a last resort shrinks the spec.
pub fn voxelize_capped(spec: &ShapeSpec, cap: usize) -> VoxelModel {
    let b = crate::attach::spec_bounds(spec);
    let mut k = voxel_k_for_extent((b.max - b.min).max_element(), cap);
    let mut model = voxelize_at(spec, k);
    while model.voxel_count > cap && k < 8 {
        k *= 2;
        model = voxelize_at(spec, k);
    }
    let mut s = spec.clone();
    let mut tries = 0;
    let mut shrink = 1.0f32;
    while model.voxel_count > cap && tries < 6 {
        let f = ((cap as f32 / model.voxel_count as f32).sqrt() * 0.97).clamp(0.3, 0.97);
        scale_spec(&mut s, f);
        shrink *= f;
        model = voxelize_at(&s, k);
        tries += 1;
    }
    model.unit_m = CONSTRUCT_VOXEL_SIZE * shrink;
    enforce_cap(&mut model, cap);
    model
}

/// Last-resort guarantee: thin every part evenly (deterministic stride) until the total fits.
fn enforce_cap(model: &mut VoxelModel, cap: usize) {
    if model.voxel_count <= cap || model.voxel_count == 0 {
        return;
    }
    let keep = cap as f64 / model.voxel_count as f64;
    let mut total = 0;
    for p in &mut model.parts {
        let n = p.voxels.len();
        let target = ((n as f64) * keep).floor() as usize;
        if target < n {
            let mut acc = 0.0f64;
            let v: Vec<(IVec3, Tint)> = p
                .voxels
                .iter()
                .filter(|_| {
                    acc += keep;
                    if acc >= 1.0 {
                        acc -= 1.0;
                        true
                    } else {
                        false
                    }
                })
                .copied()
                .collect();
            p.voxels = v;
        }
        total += p.voxels.len();
    }
    model.voxel_count = total;
    model.instances = part_instances(&model.parts);
}
