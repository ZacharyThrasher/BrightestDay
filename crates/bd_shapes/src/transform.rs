//! Transformers (`Mechanic::Transforms`, COMBAT.md §5d-bis): one body, two modes.
//!
//! [`transform_rig`] splits an alt-mode model (usually a vehicle) into ~10–24 rigid **panels** by region
//! (hood → chest, grille → abdomen, doors → back-wings, rear → thighs/shins, rear bumper → feet, wheels →
//! shoulders/calves/hips, front corners → fists, sills → arms, roof → backpack) and lays those same panels
//! out as a humanoid robot. Every panel keeps its voxels: the robot is the panels re-posed by exact
//! 90°-rotations + whole-voxel translations, so [`TransformRig::robot`] (a normal biped `VoxelModel` —
//! gait, attacks, hit reacts all work) contains exactly the panel voxels where `pose_transform(.., 1.0)`
//! leaves them: the client swaps models at the end without a pop. A small head unfolds from inside.
//!
//! Sequence (normalised t 0..1, [`TRANSFORM_SECONDS`] ≈ 0.9 s): panels start outside-in (staggered);
//! each does up to three discrete moves — unlatch/pop outward, hinge-rotate (single axis, snappy ease
//! with a little overshoot), slide/arc into its robot slot — then the whole robot squashes and **stands
//! up**. Reverse (robot → alt) plays the same moves backwards. [`sequence_events`] lists the moments for
//! sparks/clicks. Two-mode icons: [`transform_rig_authored`] takes the robot `ShapeSpec` (panels matched
//! by `part` id); its placement and roles win over the procedural layout.
//!
//! Combiners: [`combine_rigs`] stacks 3–5 alt models into one giant (legs, torso, arms + a head) with a
//! per-vehicle drive/flip/lift sequence ([`pose_combine`]).

use std::f32::consts::PI;

use bd_core::shape::{PartId, PartRole, ShapeSpec, Tint};
use glam::{Affine3A, IVec3, Mat3, Quat, Vec3};

use crate::ease::{Ease, smoothstep};
use crate::voxelize::{PartModel, VoxelModel, model_from_parts};

/// Real-time length of a full transformation (alt → robot), seconds.
pub const TRANSFORM_SECONDS: f32 = 0.9;
/// Real-time length of a combine sequence, seconds.
pub const COMBINE_SECONDS: f32 = 2.0;

/// Robot role of a panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RobotRole {
    Head,
    Chest,
    Abdomen,
    Pelvis,
    Back,
    WingL,
    WingR,
    ShoulderL,
    ShoulderR,
    UpperArmL,
    UpperArmR,
    ForearmL,
    ForearmR,
    FistL,
    FistR,
    HipL,
    HipR,
    ThighL,
    ThighR,
    ShinL,
    ShinR,
    CalfL,
    CalfR,
    FootL,
    FootR,
}

impl RobotRole {
    /// Which robot-model part the panel rides in (0 root, 1 head, 2/3 arms, 4/5 legs).
    fn group(self) -> usize {
        use RobotRole::*;
        match self {
            Head => 1,
            ShoulderL | UpperArmL | ForearmL | FistL => 2,
            ShoulderR | UpperArmR | ForearmR | FistR => 3,
            ThighL | ShinL | CalfL | FootL => 4,
            ThighR | ShinR | CalfR | FootR => 5,
            _ => 0,
        }
    }
    fn limb(self) -> bool {
        use RobotRole::*;
        matches!(self, UpperArmL | UpperArmR | ForearmL | ForearmR | ThighL | ThighR | ShinL | ShinR)
    }
}

/// One panel: part `index` of [`TransformRig::alt`], its robot role and exact robot placement
/// (`robot_voxel = rot · alt_voxel + offset`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Panel {
    pub role: RobotRole,
    pub rot: [[i32; 3]; 3],
    pub offset: IVec3,
    /// Rest centre (alt pose, m) and robot centre (m).
    pub alt_center: Vec3,
    pub robot_center: Vec3,
    /// Rotation as a quaternion + its axis/angle (for the hinge move).
    pub q: Quat,
    pub axis: Vec3,
    pub angle: f32,
    /// Outward unlatch direction and the normalised start/end of this panel's moves.
    pub pop: Vec3,
    pub start: f32,
    pub end: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransformRig {
    /// Alt mode split into panels: part 0 = empty root, parts 1.. = panels (same voxels as the alt
    /// model plus a hidden head). Pose with [`pose_transform`].
    pub alt: VoxelModel,
    /// Robot mode: biped rig (root torso, head, arms, legs) made of the same panel voxels.
    pub robot: VoxelModel,
    /// Index-aligned with `alt.parts` (entry 0 is a dummy for the root).
    pub panels: Vec<Panel>,
    /// Robot height (m) for the stand-up squash.
    pub height: f32,
}

// ------------------------------------------------------------------------------- 90° rotations

fn all_rotations() -> Vec<[[i32; 3]; 3]> {
    let mut out = Vec::with_capacity(24);
    let perms = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    for p in perms {
        for signs in 0..8 {
            let mut m = [[0i32; 3]; 3];
            for r in 0..3 {
                m[r][p[r]] = if signs & (1 << r) != 0 { -1 } else { 1 };
            }
            let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
            if det == 1 {
                out.push(m);
            }
        }
    }
    // Identity first (deterministic tie-breaks prefer "no rotation").
    out.sort_by_key(|m| (m[0][0] != 1 || m[1][1] != 1 || m[2][2] != 1) as i32);
    out
}

fn rot_apply(m: &[[i32; 3]; 3], v: IVec3) -> IVec3 {
    IVec3::new(
        m[0][0] * v.x + m[0][1] * v.y + m[0][2] * v.z,
        m[1][0] * v.x + m[1][1] * v.y + m[1][2] * v.z,
        m[2][0] * v.x + m[2][1] * v.y + m[2][2] * v.z,
    )
}

fn rot_quat(m: &[[i32; 3]; 3]) -> Quat {
    let c = |j: usize| Vec3::new(m[0][j] as f32, m[1][j] as f32, m[2][j] as f32);
    Quat::from_mat3(&Mat3::from_cols(c(0), c(1), c(2))).normalize()
}

const IDENTITY_ROT: [[i32; 3]; 3] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];

// ---------------------------------------------------------------------------------- building

struct Proto {
    role: RobotRole,
    voxels: Vec<(IVec3, Tint)>,
}

fn bbox(vs: &[(IVec3, Tint)]) -> (IVec3, IVec3) {
    let mut mn = IVec3::splat(i32::MAX);
    let mut mx = IVec3::splat(i32::MIN);
    for (v, _) in vs {
        mn = mn.min(*v);
        mx = mx.max(*v);
    }
    (mn, mx)
}

/// Region → role for one alt voxel. `u`,`v`,`w` = normalised x (left→right), y (bottom→top), z
/// (front→rear).
fn region(u: f32, v: f32, w: f32) -> RobotRole {
    use RobotRole::*;
    let left = u < 0.5;
    if w < 0.3 {
        if v >= 0.5 {
            Chest
        } else if u < 0.33 {
            FistL
        } else if u > 0.67 {
            FistR
        } else {
            Abdomen
        }
    } else if w < 0.62 {
        if v >= 0.62 {
            Back
        } else if u < 0.3 || u > 0.7 {
            if v >= 0.3 {
                if left { WingL } else { WingR }
            } else if w < 0.46 {
                if left { ForearmL } else { ForearmR }
            } else if left {
                UpperArmL
            } else {
                UpperArmR
            }
        } else {
            Pelvis
        }
    } else if w > 0.9 && v < 0.35 {
        if left { FootL } else { FootR }
    } else if w < 0.78 {
        if left { ThighL } else { ThighR }
    } else if left {
        ShinL
    } else {
        ShinR
    }
}

fn components(vox: &[(IVec3, Tint)]) -> Vec<Vec<(IVec3, Tint)>> {
    use std::collections::HashMap;
    let map: HashMap<IVec3, Tint> = vox.iter().copied().collect();
    let mut seen: std::collections::HashSet<IVec3> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (v, _) in vox {
        if seen.contains(v) {
            continue;
        }
        let mut comp = Vec::new();
        let mut stack = vec![*v];
        seen.insert(*v);
        while let Some(p) = stack.pop() {
            comp.push((p, map[&p]));
            for dz in -1..=1 {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let q = p + IVec3::new(dx, dy, dz);
                        if map.contains_key(&q) && seen.insert(q) {
                            stack.push(q);
                        }
                    }
                }
            }
        }
        comp.sort_by_key(|(v, _)| (v.z, v.y, v.x));
        out.push(comp);
    }
    out
}

fn defragment(by_role: Vec<(RobotRole, Vec<(IVec3, Tint)>)>) -> Vec<(RobotRole, Vec<(IVec3, Tint)>)> {
    let mut keep: Vec<(RobotRole, Vec<(IVec3, Tint)>)> = Vec::new();
    let mut loose: Vec<Vec<(IVec3, Tint)>> = Vec::new();
    for (role, vox) in by_role {
        let mut comps = components(&vox);
        comps.sort_by_key(|c| std::cmp::Reverse(c.len()));
        let big = comps.first().map_or(0, |c| c.len());
        let mut main = Vec::new();
        for c in comps {
            if c.len() * 4 >= big {
                main.extend(c);
            } else {
                loose.push(c);
            }
        }
        keep.push((role, main));
    }
    for frag in loose {
        let (a, b) = bbox(&frag);
        let c = (a + b) / 2;
        let best = keep
            .iter()
            .enumerate()
            .filter(|(_, (_, v))| !v.is_empty())
            .map(|(i, (_, v))| (i, v.iter().map(|(p, _)| (*p - c).length_squared()).min().unwrap_or(i32::MAX)))
            .min_by_key(|x| x.1);
        if let Some((i, _)) = best {
            keep[i].1.extend(frag);
        }
    }
    keep
}

/// Split `alt` into panels (by region + wheel parts).
fn split(alt: &VoxelModel) -> Vec<Proto> {
    use RobotRole::*;
    let mut all: Vec<(IVec3, Tint, usize)> = Vec::new();
    for (i, p) in alt.parts.iter().enumerate() {
        for (v, t) in &p.voxels {
            all.push((*v, *t, i));
        }
    }
    let mut protos: Vec<Proto> = Vec::new();
    if all.is_empty() {
        return protos;
    }
    let mut mn = IVec3::splat(i32::MAX);
    let mut mx = IVec3::splat(i32::MIN);
    for (v, _, _) in &all {
        mn = mn.min(*v);
        mx = mx.max(*v);
    }
    let ext = (mx - mn).max(IVec3::ONE).as_vec3();
    let norm = |v: IVec3| (v - mn).as_vec3() / ext;
    // Wheel-like parts become their own panels.
    let is_wheel = |i: usize| {
        i > 0 && (alt.parts[i].role == Some(PartRole::Wheel) || matches!(alt.machine.motion(i), crate::features::Motion::Spin { .. }))
            && alt.parts[i].voxels.len() >= 3
    };
    let mut wheels: Vec<(usize, Vec3)> = Vec::new();
    for i in 0..alt.parts.len() {
        if is_wheel(i) {
            let (a, b) = bbox(&alt.parts[i].voxels);
            wheels.push((i, norm((a + b) / 2)));
        }
    }
    // Front/rear wheels per side → shoulders / calves; the rest → hips.
    let mut used = [0usize; 4];
    for (i, c) in &wheels {
        let left = c.x < 0.5;
        let front = c.z < 0.5;
        let slot = (front as usize) * 2 + left as usize;
        used[slot] += 1;
        let role = match (front, left, used[slot]) {
            (true, true, 1) => ShoulderL,
            (true, false, 1) => ShoulderR,
            (false, true, 1) => CalfL,
            (false, false, 1) => CalfR,
            (_, true, _) => HipL,
            (_, false, _) => HipR,
        };
        protos.push(Proto { role, voxels: alt.parts[*i].voxels.clone() });
    }
    let wheel_parts: Vec<usize> = wheels.iter().map(|w| w.0).collect();
    let mut by_role: Vec<(RobotRole, Vec<(IVec3, Tint)>)> = Vec::new();
    for (v, t, i) in all {
        if wheel_parts.contains(&i) {
            continue;
        }
        let n = norm(v);
        let r = region(n.x, n.y, n.z);
        match by_role.iter_mut().find(|(rr, _)| *rr == r) {
            Some((_, list)) => list.push((v, t)),
            None => by_role.push((r, vec![(v, t)])),
        }
    }
    // Small disconnected fragments of a region ride with the nearest big panel so nothing floats.
    let by_role = defragment(by_role);
    for (role, voxels) in by_role {
        protos.push(Proto { role, voxels });
    }
    // Guarantee a body: synthesize any missing structural limb inside the alt hull (hidden).
    let centre = (mn + mx) / 2;
    let s = ((ext.x.min(ext.y).min(ext.z)) * 0.18).round().max(1.0) as i32;
    for role in [Chest, Pelvis, UpperArmL, UpperArmR, ForearmL, ForearmR, ThighL, ThighR, ShinL, ShinR] {
        let n = protos.iter().filter(|p| p.role == role).map(|p| p.voxels.len()).sum::<usize>();
        if n >= 4 {
            continue;
        }
        let (len, w) = if role.limb() { (s * 3, s) } else { (s * 2, s * 2) };
        let mut vox = Vec::new();
        for y in 0..len {
            for x in 0..w.max(1) {
                for z in 0..w.max(1) {
                    vox.push((centre + IVec3::new(x - w / 2, y - len / 2, z - w / 2), Tint::Dark));
                }
            }
        }
        protos.push(Proto { role, voxels: vox });
    }
    // The head: helmet with crest, glowing eyes and "ear" fins, folded inside the chassis.
    let hs = (ext.x * 0.22).round().max(2.0) as i32;
    let mut head = Vec::new();
    for y in 0..hs + 1 {
        for x in -hs / 2..=hs / 2 {
            for z in -hs / 2..=hs / 2 {
                let edge = x.abs() == hs / 2 || z.abs() == hs / 2 || y == hs || y == 0;
                if !edge {
                    continue;
                }
                let t = if z == -hs / 2 && y == hs / 2 + (hs > 3) as i32 && x != 0 { Tint::Core } else if y == hs || (x == 0 && z == -hs / 2) { Tint::Accent } else { Tint::Primary };
                head.push((centre + IVec3::new(x, y - hs / 2, z), t));
            }
        }
    }
    for k in 0..2 {
        head.push((centre + IVec3::new(0, hs / 2 + 1 + k, -hs / 2 + 1), Tint::Accent));
        for sx in [-1, 1] {
            head.push((centre + IVec3::new(sx * (hs / 2 + 1), k, 0), Tint::Accent));
        }
    }
    protos.push(Proto { role: Head, voxels: head });
    protos.retain(|p| !p.voxels.is_empty());
    protos
}

struct Placed {
    proto: usize,
    rot: [[i32; 3]; 3],
    rmin: IVec3,
    at: IVec3,
}

/// Build the rig from any alt-mode model (see module docs). Deterministic.
pub fn transform_rig(alt: &VoxelModel, seed: u32) -> TransformRig {
    let protos = split(alt);
    let rots = all_rotations();
    let targets = skeleton(&protos);
    let mut placed: Vec<Placed> = Vec::with_capacity(protos.len());
    let mut seen: Vec<(RobotRole, i32)> = Vec::new();
    for (i, p) in protos.iter().enumerate() {
        let (tmin, tmax) = target_of(&targets, p.role);
        let text = (tmax - tmin + IVec3::ONE).as_vec3();
        let rot = orient_to(p.role, &p.voxels, text, &rots);
        let rv: Vec<(IVec3, Tint)> = p.voxels.iter().map(|(v, t)| (rot_apply(&rot, *v), *t)).collect();
        let (a, b) = bbox(&rv);
        let size = b - a + IVec3::ONE;
        let k = match seen.iter_mut().find(|(r, _)| *r == p.role) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                seen.push((p.role, 0));
                0
            }
        };
        // Centre on the target box (x/z), bottom-align limbs/feet, centre everything else.
        let tc = (tmin + tmax) / 2;
        let mut min = IVec3::new(tc.x - size.x / 2, tc.y - size.y / 2, tc.z - size.z / 2);
        use RobotRole::*;
        match p.role {
            FootL | FootR | ShinL | ShinR | ThighL | ThighR | Pelvis | Abdomen | Chest | Head => min.y = tmin.y,
            UpperArmL | UpperArmR | ForearmL | ForearmR | FistL | FistR => min.y = tmax.y - size.y + 1,
            _ => {}
        }
        // Extra panels of one role (several wheels on a hip) fan out along z.
        min.z += k * (size.z / 2 + 1) * if k % 2 == 1 { -1 } else { 1 };
        placed.push(Placed { proto: i, rot, rmin: a, at: min - a });
    }
    let low = placed.iter().map(|p| p.rmin.y + p.at.y).min().unwrap_or(0);
    for p in placed.iter_mut() {
        p.at.y -= low;
    }
    let _ = seed;
    assemble(alt, &protos, &placed)
}

/// Robot skeleton boxes (voxel units, feet at y = 0, facing −Z), proportioned from the alt size so
/// the robot is about as tall as the vehicle is long.
fn skeleton(protos: &[Proto]) -> Vec<(RobotRole, IVec3, IVec3)> {
    use RobotRole::*;
    let all: Vec<(IVec3, Tint)> = protos.iter().filter(|p| p.role != Head).flat_map(|p| p.voxels.iter().copied()).collect();
    let (a, b) = if all.is_empty() { (IVec3::ZERO, IVec3::splat(8)) } else { bbox(&all) };
    let e = (b - a + IVec3::ONE).as_vec3();
    let h = (e.z.max(e.x) * 1.0).max(e.y * 1.6).max(10.0);
    let r = |x: f32| (x.round() as i32).max(1);
    let cw = r((e.x * 0.8).max(h * 0.34));
    let cd = r((h * 0.16).max(3.0));
    let lw = r(cw as f32 * 0.34).max(2);
    let aw = r(cw as f32 * 0.26).max(2);
    let (foot, shin, thigh) = (r(h * 0.06), r(h * 0.2), r(h * 0.19));
    let (pelvis, abd, chest, head) = (r(h * 0.08), r(h * 0.1), r(h * 0.19), r(h * 0.1));
    let (upper, fore, fist) = (r(h * 0.19), r(h * 0.17), r(h * 0.08));
    let y_shin = foot;
    let y_thigh = y_shin + shin;
    let y_pel = y_thigh + thigh;
    let y_abd = y_pel + pelvis;
    let y_chest = y_abd + abd;
    let top = y_chest + chest;
    let lx = cw / 4 + 1;
    let ax = cw / 2 + aw / 2 + 1;
    let bx = |cx: i32, y0: i32, w: i32, hh: i32, cz: i32, d: i32| (IVec3::new(cx - w / 2, y0, cz - d / 2), IVec3::new(cx - w / 2 + w - 1, y0 + hh - 1, cz - d / 2 + d - 1));
    let mut out = Vec::new();
    for s in [-1i32, 1] {
        let pick = |l: RobotRole, rr: RobotRole| if s < 0 { l } else { rr };
        let (a1, b1) = bx(s * lx, 0, lw + 1, foot, -1, lw + 3);
        out.push((pick(FootL, FootR), a1, b1));
        let (a1, b1) = bx(s * lx, y_shin, lw, shin, 0, lw);
        out.push((pick(ShinL, ShinR), a1, b1));
        let (a1, b1) = bx(s * (lx + lw / 2 + 1), y_shin + shin / 4, 2, shin / 2, 0, shin / 2);
        out.push((pick(CalfL, CalfR), a1, b1));
        let (a1, b1) = bx(s * lx, y_thigh, lw, thigh, 0, lw);
        out.push((pick(ThighL, ThighR), a1, b1));
        let (a1, b1) = bx(s * (cw / 2 + 1), y_pel, 2, pelvis, 0, pelvis);
        out.push((pick(HipL, HipR), a1, b1));
        let (a1, b1) = bx(s * ax, top - upper, aw, upper, 0, aw);
        out.push((pick(UpperArmL, UpperArmR), a1, b1));
        let (a1, b1) = bx(s * ax, top - upper - fore, aw, fore, -1, aw);
        out.push((pick(ForearmL, ForearmR), a1, b1));
        let (a1, b1) = bx(s * ax, top - upper - fore - fist, aw + 1, fist, -1, aw + 1);
        out.push((pick(FistL, FistR), a1, b1));
        let (a1, b1) = bx(s * (ax + aw / 2 + 1), top - upper / 2, 2, upper / 2, 0, upper / 2);
        out.push((pick(ShoulderL, ShoulderR), a1, b1));
        let (a1, b1) = bx(s * (cw / 4 + 1), y_chest + chest / 3, cw / 2, chest, cd / 2 + 2, 2);
        out.push((pick(WingL, WingR), a1, b1));
    }
    let (a1, b1) = bx(0, y_pel, cw - 2, pelvis, 0, cd - 1);
    out.push((Pelvis, a1, b1));
    let (a1, b1) = bx(0, y_abd, cw - 3, abd, 0, cd - 1);
    out.push((Abdomen, a1, b1));
    let (a1, b1) = bx(0, y_chest, cw, chest, -1, cd);
    out.push((Chest, a1, b1));
    let (a1, b1) = bx(0, y_chest, cw - 2, chest, cd / 2 + 1, 2);
    out.push((Back, a1, b1));
    let (a1, b1) = bx(0, top, head, head, -1, head);
    out.push((Head, a1, b1));
    out
}

fn target_of(t: &[(RobotRole, IVec3, IVec3)], role: RobotRole) -> (IVec3, IVec3) {
    t.iter().find(|x| x.0 == role).map(|x| (x.1, x.2)).unwrap_or((IVec3::ZERO, IVec3::splat(3)))
}

/// Pick the 90° rotation whose rotated box best matches the target box (limbs weigh length most;
/// plates prefer their outer face forward; wheels keep their axle across).
fn orient_to(role: RobotRole, vox: &[(IVec3, Tint)], target: Vec3, rots: &[[[i32; 3]; 3]]) -> [[i32; 3]; 3] {
    use RobotRole::*;
    let (a, b) = bbox(vox);
    let e = (b - a + IVec3::ONE).as_vec3();
    let w = if role.limb() { Vec3::new(1.0, 1.6, 1.0) } else { Vec3::ONE };
    let mut best = (f32::MAX, IDENTITY_ROT);
    for m in rots {
        let ea = Vec3::new(
            (m[0][0].abs() as f32) * e.x + (m[0][1].abs() as f32) * e.y + (m[0][2].abs() as f32) * e.z,
            (m[1][0].abs() as f32) * e.x + (m[1][1].abs() as f32) * e.y + (m[1][2].abs() as f32) * e.z,
            (m[2][0].abs() as f32) * e.x + (m[2][1].abs() as f32) * e.y + (m[2][2].abs() as f32) * e.z,
        );
        let up = rot_apply(m, IVec3::Y);
        let back = rot_apply(m, IVec3::Z);
        let mut cost = ((ea - target) * w).abs().element_sum();
        match role {
            Chest if up == IVec3::NEG_Z => cost -= 1.5,
            Back if up == IVec3::Z => cost -= 1.5,
            ShinL | ShinR | FootL | FootR if back == IVec3::NEG_Y => cost -= 1.0,
            ShoulderL | ShoulderR | CalfL | CalfR | HipL | HipR => cost = if rot_apply(m, IVec3::X).abs() == IVec3::X { cost * 0.1 } else { cost + 100.0 },
            _ => {}
        }
        if *m == IDENTITY_ROT {
            cost -= 1e-3;
        }
        if cost < best.0 - 1e-4 {
            best = (cost, *m);
        }
    }
    best.1
}

fn assemble(alt: &VoxelModel, protos: &[Proto], placed: &[Placed]) -> TransformRig {
    let vs = alt.voxel_size.max(1e-3);
    // Alt panel model: root (empty) + one part per panel.
    let mut alt_parts = vec![PartModel { id: 0, parent: 0, parent_index: 0, pivot: Vec3::ZERO, role: None, voxels: Vec::new(), vmin: IVec3::ZERO, vmax: IVec3::ZERO, depth: 0, voxel_size: vs }];
    let mut panels = vec![Panel { role: RobotRole::Chest, rot: IDENTITY_ROT, offset: IVec3::ZERO, alt_center: Vec3::ZERO, robot_center: Vec3::ZERO, q: Quat::IDENTITY, axis: Vec3::Y, angle: 0.0, pop: Vec3::ZERO, start: 0.0, end: 0.0 }];
    // Robot parts: root torso, head, arms, legs.
    let mut groups: Vec<Vec<(IVec3, Tint)>> = vec![Vec::new(); 6];
    let mut group_boxes: Vec<(IVec3, IVec3)> = vec![(IVec3::splat(i32::MAX), IVec3::splat(i32::MIN)); 6];
    let (amn, amx) = bbox(&protos.iter().flat_map(|p| p.voxels.iter().copied()).collect::<Vec<_>>());
    let alt_c = (amn + amx).as_vec3() * 0.5 * vs;
    for (k, pl) in placed.iter().enumerate() {
        let pr = &protos[pl.proto];
        let (a, b) = bbox(&pr.voxels);
        let c0 = (a + b).as_vec3() * 0.5 * vs;
        let q = rot_quat(&pl.rot);
        let c1 = q * c0 + pl.at.as_vec3() * vs;
        let (axis, angle) = q.to_axis_angle();
        let out = c0 - alt_c;
        let pop = if out.length_squared() > 1e-6 { out.normalize() } else { Vec3::Y };
        alt_parts.push(PartModel {
            id: (k + 1).min(250) as PartId,
            parent: 0,
            parent_index: 0,
            pivot: c0,
            role: Some(PartRole::Other),
            voxels: pr.voxels.clone(),
            vmin: a,
            vmax: b,
            depth: 1,
            voxel_size: vs,
        });
        panels.push(Panel { role: pr.role, rot: pl.rot, offset: pl.at, alt_center: c0, robot_center: c1, q, axis: if axis.is_finite() { axis } else { Vec3::Y }, angle: if angle.is_finite() { angle } else { 0.0 }, pop, start: 0.0, end: 0.0 });
        let g = pr.role.group();
        for (v, t) in &pr.voxels {
            let r = rot_apply(&pl.rot, *v) + pl.at;
            groups[g].push((r, *t));
            group_boxes[g].0 = group_boxes[g].0.min(r);
            group_boxes[g].1 = group_boxes[g].1.max(r);
        }
    }
    // Stagger: outside-in (farthest from the alt centre first); the head unfolds last.
    let n = panels.len().saturating_sub(1).max(1);
    let mut order: Vec<usize> = (1..panels.len()).collect();
    order.sort_by(|&a, &b| {
        let da = if panels[a].role == RobotRole::Head { -1.0 } else { (panels[a].alt_center - alt_c).length() };
        let db = if panels[b].role == RobotRole::Head { -1.0 } else { (panels[b].alt_center - alt_c).length() };
        db.total_cmp(&da).then(a.cmp(&b))
    });
    for (rank, &i) in order.iter().enumerate() {
        let f = rank as f32 / n as f32;
        panels[i].start = 0.02 + 0.36 * f;
        panels[i].end = (panels[i].start + 0.44).min(0.84);
    }
    // Robot model.
    let pivot_of = |g: usize, bx: (IVec3, IVec3)| -> Vec3 {
        let (a, b) = bx;
        let c = (a + b).as_vec3() * 0.5;
        match g {
            1 => Vec3::new(c.x, a.y as f32 - 0.5, c.z) * vs,
            2..=5 => Vec3::new(c.x, b.y as f32, c.z) * vs,
            _ => Vec3::ZERO,
        }
    };
    let roles = [None, Some(PartRole::Head), Some(PartRole::ArmLeft), Some(PartRole::ArmRight), Some(PartRole::LegBackLeft), Some(PartRole::LegBackRight)];
    let mut robot_parts = Vec::new();
    for g in 0..6 {
        if g > 0 && groups[g].is_empty() {
            continue;
        }
        let id = robot_parts.len() as PartId;
        robot_parts.push(PartModel {
            id,
            parent: 0,
            parent_index: 0,
            pivot: if g == 0 { Vec3::ZERO } else { pivot_of(g, group_boxes[g]) },
            role: roles[g],
            voxels: std::mem::take(&mut groups[g]),
            vmin: IVec3::ZERO,
            vmax: IVec3::ZERO,
            depth: (g > 0) as u8,
            voxel_size: vs,
        });
    }
    let robot = model_from_parts(robot_parts, vs);
    let height = (robot.bounds.max.y - robot.bounds.min.y).max(0.25);
    let alt_model = model_from_parts(alt_parts, vs);
    TransformRig { alt: alt_model, robot, panels, height }
}

/// Two-mode icon: `alt` and `robot` specs share panel ids (`Prim::part`). Each alt panel is placed
/// by the 90° rotation + translation that best matches the robot panel's box; the robot panel's
/// part role (Head / Arm* / Leg* / else torso) decides which robot part it rides in.
pub fn transform_rig_authored(alt: &ShapeSpec, robot: &ShapeSpec, cap: usize) -> TransformRig {
    use RobotRole::*;
    let am = crate::voxelize::voxelize_capped(alt, cap);
    let k = (am.voxel_size / bd_core::CONSTRUCT_VOXEL_SIZE).round().max(1.0) as u32;
    let rm = crate::voxelize::voxelize_at(robot, k);
    let rots = all_rotations();
    let mut protos = Vec::new();
    let mut placed = Vec::new();
    for ap in &am.parts {
        if ap.voxels.is_empty() {
            continue;
        }
        let Some(rp) = rm.parts.iter().find(|p| p.id == ap.id && !p.voxels.is_empty()) else { continue };
        let side_l = rp.pivot.x < 0.0;
        let role = match rp.role {
            Some(PartRole::Head) => Head,
            Some(PartRole::ArmLeft) => UpperArmL,
            Some(PartRole::ArmRight) => UpperArmR,
            Some(PartRole::LegBackLeft | PartRole::LegFrontLeft) => ThighL,
            Some(PartRole::LegBackRight | PartRole::LegFrontRight) => ThighR,
            _ if side_l => WingL,
            _ => Chest,
        };
        let target = (rp.vmax - rp.vmin + IVec3::ONE).as_vec3();
        let mut best = (f32::MAX, IDENTITY_ROT);
        for m in &rots {
            let rv: Vec<IVec3> = ap.voxels.iter().map(|(v, _)| rot_apply(m, *v)).collect();
            let mut mn = IVec3::splat(i32::MAX);
            let mut mx = IVec3::splat(i32::MIN);
            for v in &rv {
                mn = mn.min(*v);
                mx = mx.max(*v);
            }
            let d = ((mx - mn + IVec3::ONE).as_vec3() - target).abs().element_sum() + if *m == IDENTITY_ROT { -1e-3 } else { 0.0 };
            if d < best.0 {
                best = (d, *m);
            }
        }
        let rv: Vec<(IVec3, Tint)> = ap.voxels.iter().map(|(v, t)| (rot_apply(&best.1, *v), *t)).collect();
        let (a, b) = bbox(&rv);
        let at = rp.vmin - a;
        let _ = b;
        placed.push(Placed { proto: protos.len(), rot: best.1, rmin: a, at });
        protos.push(Proto { role, voxels: ap.voxels.clone() });
    }
    if protos.is_empty() {
        let (s, m) = (alt.clone(), am);
        let _ = s;
        return transform_rig(&m, 0);
    }
    let low = placed.iter().map(|p| p.rmin.y + p.at.y).min().unwrap_or(0);
    for p in placed.iter_mut() {
        p.at.y -= low;
    }
    assemble(&am, &protos, &placed)
}

// --------------------------------------------------------------------------------- sequence

fn ease(e: Ease, u: f32) -> f32 {
    e.apply(u.clamp(0.0, 1.0))
}

/// Pose the panel model at normalised time `t` (0 = alt, 1 = robot). `to_robot = false` plays the
/// reverse (robot → alt) with the same `t` direction (0 = robot, 1 = alt). Allocation-free;
/// `out` must hold `rig.alt.parts.len()` entries (model space, metres).
pub fn pose_transform(rig: &TransformRig, t: f32, to_robot: bool, out: &mut [Affine3A]) {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let t = if to_robot { t } else { 1.0 - t };
    let n = rig.panels.len().min(out.len());
    // Stand-up: squash while the panels land, spring up at the end.
    let crouch = smoothstep(0.3, 0.62, t) * (1.0 - ease(Ease::OutBack, (t - 0.8) / 0.2));
    let g = Affine3A::from_scale(Vec3::new(1.0 + 0.06 * crouch, 1.0 - 0.2 * crouch, 1.0 + 0.06 * crouch));
    let vs = rig.alt.voxel_size;
    for (i, o) in out.iter_mut().enumerate().take(n) {
        if i == 0 {
            *o = Affine3A::IDENTITY;
            continue;
        }
        let p = &rig.panels[i];
        let u = ((t - p.start) / (p.end - p.start).max(1e-3)).clamp(0.0, 1.0);
        // 1) unlatch: pop outward, 2) hinge, 3) slide/arc into place.
        let pop = p.pop * (vs * 1.5) * ease(Ease::OutCubic, u / 0.25) * (1.0 - smoothstep(0.55, 1.0, u));
        let r = ease(Ease::OutBack, (u - 0.18) / 0.45);
        let q = Quat::from_axis_angle(p.axis, p.angle * r);
        let m = ease(Ease::OutBack, (u - 0.32) / 0.68);
        let arc = Vec3::Y * ((p.robot_center - p.alt_center).length() * 0.25 * (PI * ((u - 0.32) / 0.68).clamp(0.0, 1.0)).sin());
        let c = p.alt_center.lerp(p.robot_center, m) + arc + pop;
        let a = Affine3A::from_translation(c) * Affine3A::from_quat(q) * Affine3A::from_translation(-p.alt_center);
        let a = if u >= 1.0 { exact(p, vs) } else { a };
        let blend = smoothstep(0.35, 0.7, t);
        let a = if blend > 0.0 { g_lerp(g, blend) * a } else { a };
        *o = if a.is_finite() { a } else { Affine3A::IDENTITY };
    }
}

fn g_lerp(g: Affine3A, w: f32) -> Affine3A {
    let s = Vec3::ONE.lerp(Vec3::new(g.matrix3.x_axis.x, g.matrix3.y_axis.y, g.matrix3.z_axis.z), w);
    Affine3A::from_scale(s)
}

/// Exact robot placement of a panel (maps alt voxel centres onto robot voxel centres).
fn exact(p: &Panel, vs: f32) -> Affine3A {
    Affine3A::from_translation(p.offset.as_vec3() * vs) * Affine3A::from_quat(p.q)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransformEventKind {
    /// Panel unlatches (click + spark).
    Unlatch,
    /// Hinge swing peaks (whirr).
    Hinge,
    /// Panel locks into its slot (clack + spark burst).
    Lock,
    /// The robot stands up / the vehicle settles (thud).
    StandUp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformEvent {
    /// Normalised time 0..1 (× [`TRANSFORM_SECONDS`]).
    pub time: f32,
    /// Panel part index in `TransformRig::alt` (0 for whole-body events).
    pub panel: usize,
    pub kind: TransformEventKind,
    /// Posed position at the event (robot-bound direction), metres.
    pub pos: Vec3,
}

/// Events of the alt → robot sequence, sorted by time (reverse: `1 - time`).
pub fn sequence_events(rig: &TransformRig) -> Vec<TransformEvent> {
    let mut ev = Vec::new();
    for (i, p) in rig.panels.iter().enumerate().skip(1) {
        let w = p.end - p.start;
        ev.push(TransformEvent { time: p.start, panel: i, kind: TransformEventKind::Unlatch, pos: p.alt_center });
        if p.angle.abs() > 0.1 {
            ev.push(TransformEvent { time: p.start + w * 0.4, panel: i, kind: TransformEventKind::Hinge, pos: p.alt_center.lerp(p.robot_center, 0.3) });
        }
        ev.push(TransformEvent { time: p.end, panel: i, kind: TransformEventKind::Lock, pos: p.robot_center });
    }
    ev.push(TransformEvent { time: 0.92, panel: 0, kind: TransformEventKind::StandUp, pos: Vec3::ZERO });
    ev.sort_by(|a, b| a.time.total_cmp(&b.time));
    ev
}

// ----------------------------------------------------------------------------------- combiner

/// 3–5 vehicles become one giant: 0 = torso, 1/2 = legs, 3/4 = arms (synthesised if absent), + head.
#[derive(Clone, Debug, PartialEq)]
pub struct CombinerRig {
    /// The combined robot (Kaiju-style biped; same voxels as the sources in their final placement).
    pub giant: VoxelModel,
    /// Per source: final model-space transform of its alt model inside the giant.
    pub placements: Vec<Affine3A>,
    /// Per source: start transform (vehicles parked in a row around the combine point).
    pub starts: Vec<Affine3A>,
    /// Per source: normalised start time of its move.
    pub delays: Vec<f32>,
}

/// Build a combiner from the sources' alt models.
pub fn combine_rigs(rigs: &[TransformRig]) -> CombinerRig {
    let srcs: Vec<&VoxelModel> = rigs.iter().take(5).map(|r| &r.alt).collect();
    let vs = srcs.iter().map(|m| m.voxel_size).fold(bd_core::CONSTRUCT_VOXEL_SIZE, f32::max);
    // Each vehicle stands on its tail (long axis up, front up); limbs put their wide side front-to-back,
    // the torso puts it across the shoulders.
    let rots = all_rotations();
    let pick_rot = |m: &VoxelModel, torso: bool| -> [[i32; 3]; 3] {
        let all: Vec<(IVec3, Tint)> = m.parts.iter().flat_map(|p| p.voxels.iter().copied()).collect();
        if all.is_empty() {
            return IDENTITY_ROT;
        }
        let (a, b) = bbox(&all);
        let e = (b - a + IVec3::ONE).as_vec3();
        let mut best = (f32::MIN, IDENTITY_ROT);
        for r in &rots {
            let ea = Vec3::new(
                (r[0][0].abs() as f32) * e.x + (r[0][1].abs() as f32) * e.y + (r[0][2].abs() as f32) * e.z,
                (r[1][0].abs() as f32) * e.x + (r[1][1].abs() as f32) * e.y + (r[1][2].abs() as f32) * e.z,
                (r[2][0].abs() as f32) * e.x + (r[2][1].abs() as f32) * e.y + (r[2][2].abs() as f32) * e.z,
            );
            let front_up = rot_apply(r, IVec3::NEG_Z) == IVec3::Y;
            // Torso: lies across the shoulders (long axis → X, roof facing forward); limbs stand up.
            let roof_fwd = rot_apply(r, IVec3::Y) == IVec3::NEG_Z;
            let score = if torso { ea.x * 3.0 + ea.y - ea.z * 0.5 + if roof_fwd { 0.5 } else { 0.0 } } else { ea.y * 3.0 + ea.z - ea.x + if front_up { 0.5 } else { 0.0 } };
            if score > best.0 + 1e-4 {
                best = (score, *r);
            }
        }
        best.1
    };
    let ups: Vec<[[i32; 3]; 3]> = srcs.iter().enumerate().map(|(i, m)| pick_rot(m, i == 0)).collect();
    let mut parts_vox: Vec<Vec<(IVec3, Tint)>> = vec![Vec::new(); 6];
    let mut placements = Vec::new();
    let mut starts = Vec::new();
    let mut delays = Vec::new();
    // Column sizes.
    let dims: Vec<(IVec3, IVec3)> = srcs
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let up = ups[i];
            let all: Vec<(IVec3, Tint)> = m.parts.iter().flat_map(|p| p.voxels.iter().map(|(v, t)| (rot_apply(&up, scale_iv(*v, m.voxel_size, vs)), *t))).collect();
            if all.is_empty() { (IVec3::ZERO, IVec3::ZERO) } else { bbox(&all) }
        })
        .collect();
    let size = |i: usize| dims.get(i).map_or(IVec3::new(4, 10, 4), |(a, b)| *b - *a + IVec3::ONE);
    let leg_h = size(1).y.max(size(2).y);
    let leg_w = size(1).x.max(size(2).x);
    let torso = size(0);
    let leg_x = (torso.x / 2 - leg_w / 2).max(leg_w / 2 + 1);
    let arm_x = torso.x / 2 + size(3).x.max(size(4).x).max(3) / 2 + 1;
    for (i, m) in srcs.iter().enumerate() {
        let up = ups[i];
        let (a, b) = dims[i];
        let sz = b - a + IVec3::ONE;
        let (cx, y0) = match i {
            0 => (0, leg_h - 1),
            1 => (-leg_x, leg_h - sz.y),
            2 => (leg_x, leg_h - sz.y),
            3 => (-arm_x, leg_h + torso.y - sz.y),
            _ => (arm_x, leg_h + torso.y - sz.y),
        };
        let target_min = IVec3::new(cx - sz.x / 2, y0, -sz.z / 2);
        let at = target_min - a;
        let group = match i {
            0 => 0,
            1 => 4,
            2 => 5,
            3 => 2,
            _ => 3,
        };
        for p in &m.parts {
            for (v, t) in &p.voxels {
                parts_vox[group].push((rot_apply(&up, scale_iv(*v, m.voxel_size, vs)) + at, *t));
            }
        }
        // A shorter leg stands on a hydraulic strut down to the ground.
        if (i == 1 || i == 2) && y0 > 0 {
            let w = (sz.x.min(sz.z) / 2).max(2);
            for y in 0..y0 {
                for x in 0..w {
                    for z in 0..w {
                        if x == 0 || z == 0 || x == w - 1 || z == w - 1 || y == 0 {
                            let tint = if y % 3 == 0 { Tint::Accent } else { Tint::Dark };
                            parts_vox[group].push((IVec3::new(cx - w / 2 + x, y, z - w / 2), tint));
                        }
                    }
                }
            }
        }
        let k = m.voxel_size / vs;
        placements.push(Affine3A::from_translation(at.as_vec3() * vs) * Affine3A::from_quat(rot_quat(&up)) * Affine3A::from_scale(Vec3::splat(k.max(1e-3))));
        let span = (m.bounds.max.x - m.bounds.min.x).max(1.0) + 1.0;
        let x = (i as f32 - (srcs.len() as f32 - 1.0) * 0.5) * span * 1.1;
        starts.push(Affine3A::from_translation(Vec3::new(x, 0.0, span * 0.8)));
        delays.push([0.25, 0.0, 0.08, 0.45, 0.5][i]);
    }
    // Synthesised arms (3–4 vehicles) + head.
    let top = leg_h + torso.y;
    for (g, sx) in [(2usize, -1i32), (3, 1)] {
        if !parts_vox[g].is_empty() {
            continue;
        }
        let w = (torso.x / 4).max(3);
        let l = (torso.y * 3 / 4).max(4);
        for y in 0..l {
            for x in 0..w {
                for z in 0..w {
                    let edge = x == 0 || z == 0 || x == w - 1 || z == w - 1 || y == 0 || y == l - 1;
                    if edge {
                        let tint = if y < 2 { Tint::Accent } else { Tint::Dark };
                        parts_vox[g].push((IVec3::new(sx * (torso.x / 2 + 1 + w / 2) + x - w / 2, top - l + y, z - w / 2), tint));
                    }
                }
            }
        }
    }
    let hs = (torso.x / 3).max(3);
    for y in 0..hs {
        for x in 0..hs {
            for z in 0..hs {
                let edge = x == 0 || z == 0 || x == hs - 1 || z == hs - 1 || y == hs - 1;
                if edge {
                    let tint = if z == 0 && y == hs / 2 && x > 0 && x < hs - 1 { Tint::Core } else if y == hs - 1 { Tint::Accent } else { Tint::Primary };
                    parts_vox[1].push((IVec3::new(x - hs / 2, top + y, z - hs / 2), tint));
                }
            }
        }
    }
    let roles = [None, Some(PartRole::Head), Some(PartRole::ArmLeft), Some(PartRole::ArmRight), Some(PartRole::LegBackLeft), Some(PartRole::LegBackRight)];
    let mut parts = Vec::new();
    for g in 0..6 {
        if g > 0 && parts_vox[g].is_empty() {
            continue;
        }
        let (a, b) = if parts_vox[g].is_empty() { (IVec3::ZERO, IVec3::ZERO) } else { bbox(&parts_vox[g]) };
        let c = (a + b).as_vec3() * 0.5;
        let pivot = match g {
            0 => Vec3::ZERO,
            1 => Vec3::new(c.x, a.y as f32 - 0.5, c.z) * vs,
            _ => Vec3::new(c.x, b.y as f32, c.z) * vs,
        };
        parts.push(PartModel { id: parts.len() as PartId, parent: 0, parent_index: 0, pivot, role: roles[g], voxels: std::mem::take(&mut parts_vox[g]), vmin: IVec3::ZERO, vmax: IVec3::ZERO, depth: (g > 0) as u8, voxel_size: vs });
    }
    let giant = model_from_parts(parts, vs);
    CombinerRig { giant, placements, starts, delays }
}

fn scale_iv(v: IVec3, from: f32, to: f32) -> IVec3 {
    if (from - to).abs() < 1e-6 { v } else { (v.as_vec3() * (from / to)).round().as_ivec3() }
}

/// Per-source transforms at normalised combine time `t` (0 = parked, 1 = combined). Each vehicle
/// drives in, flips upright on a hinge and locks into place; legs first, then torso, then arms.
/// Allocation-free; `out.len()` = number of sources.
pub fn pose_combine(c: &CombinerRig, t: f32, out: &mut [Affine3A]) {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    for (i, o) in out.iter_mut().enumerate().take(c.placements.len()) {
        let d = c.delays[i];
        let u = ((t - d) / 0.5).clamp(0.0, 1.0);
        let (s0, r0, p0) = c.starts[i].to_scale_rotation_translation();
        let (s1, r1, p1) = c.placements[i].to_scale_rotation_translation();
        let flip = ease(Ease::OutBack, (u - 0.25) / 0.45);
        let slide = ease(Ease::OutBack, (u - 0.15) / 0.85);
        let lift = Vec3::Y * ((PI * ((u - 0.15) / 0.85).clamp(0.0, 1.0)).sin() * 0.3 * p1.y.max(1.0));
        let r = r0.slerp(r1, flip.clamp(0.0, 1.0));
        let r = if flip > 1.0 { r1 * Quat::from_axis_angle(Vec3::X, (flip - 1.0) * 0.6) } else { r };
        let a = Affine3A::from_scale_rotation_translation(s0.lerp(s1, slide.clamp(0.0, 1.0)), r, p0.lerp(p1, slide) + lift);
        let a = if u >= 1.0 { c.placements[i] } else { a };
        *o = if a.is_finite() { a } else { Affine3A::IDENTITY };
    }
}
