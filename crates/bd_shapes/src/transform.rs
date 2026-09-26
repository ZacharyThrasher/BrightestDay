//! Transformers (`Mechanic::Transforms`, COMBAT.md §5d-bis): one body, two modes.
//!
//! [`transform_rig`] splits an alt-mode model (usually a vehicle) into ~12–26 rigid **panels**
//! (connected pieces of regions: cab/windshield + hood → chest, grille → abdomen, doors → back
//! "kibble" wings, sills → arms, front corners → fists, rear → thighs/shins/feet, wheels → shoulder
//! pads / calves / hips, roof → backpack; crumbs < 12 voxels merge into a touching panel) and lays the
//! same panels out as a heroic humanoid (broad shoulders, strong chest, narrow waist, thick shins).
//! Every panel keeps its voxels: the robot is the panels re-posed by exact 90° rotations + whole-voxel
//! translations, then **snapped until every panel touches its body part and every limb meets the
//! torso** (no floating bits). [`TransformRig::robot`] is a normal biped `VoxelModel` built from
//! exactly those voxels, so at the end of [`pose_transform`] the client swaps models without a pop.
//! The head (Core visor) is new: it grows out of the chest last.
//!
//! Sequence, three beats over [`TRANSFORM_SECONDS`] (normalised t):
//! 1. **Unfold** (0–0.34): panels unlatch outside-in, pop outward and hinge-flip (snappy, overshoot).
//! 2. **Rearrange** (0.26–0.72): legs drop first, torso assembles, wings fold onto the back, arms
//!    extend, shoulder pads land — each sliding along a short arc.
//! 3. **Lock-in** (0.62–1.0): the head pops up, the body squashes and stands up with a spring.
//!
//! Reverse (robot → alt) plays it backwards. [`sequence_events`] lists unlatch / hinge / lock / head /
//! stand-up moments for sparks and clicks. Two-mode icons: [`transform_rig_authored`] uses the icon's
//! robot `ShapeSpec` (shared `part` ids = the same panel rigidly moved; robot-only parts such as head,
//! arms and weapons grow in; alt-only parts fold away) — the authored layout wins.
//!
//! Combiners: [`combine_rigs`] stacks 3–5 alt models into one giant (legs, torso, arms + a head) with a
//! per-vehicle drive/flip/lift sequence ([`pose_combine`]).

use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;

use bd_core::shape::{PartId, PartRole, ShapeSpec, Tint};
use glam::{Affine3A, IVec3, Mat3, Quat, Vec3};

use crate::ease::{Ease, smoothstep};
use crate::voxelize::{PartModel, VoxelModel, model_from_parts};

/// Real-time length of a full transformation (alt → robot), seconds.
pub const TRANSFORM_SECONDS: f32 = 0.9;
/// Real-time length of a combine sequence, seconds.
pub const COMBINE_SECONDS: f32 = 2.0;
/// Panels smaller than this merge into a touching neighbour (or are culled).
pub const MIN_PANEL_VOXELS: usize = 12;
/// Upper bound on panels per rig.
pub const MAX_PANELS: usize = 26;

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
    /// Which robot-model part the panel rides in (0 root/torso, 1 head, 2/3 arms, 4/5 legs).
    pub fn group(self) -> usize {
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
    /// Anchor preference inside its group (lower = anchor first).
    fn anchor_rank(self) -> u8 {
        use RobotRole::*;
        match self {
            Chest | Head | UpperArmL | UpperArmR | ThighL | ThighR => 0,
            Abdomen | ForearmL | ForearmR | ShinL | ShinR => 1,
            Pelvis | FistL | FistR | FootL | FootR => 2,
            _ => 3,
        }
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
    /// Outward unlatch direction.
    pub pop: Vec3,
    /// Normalised start of the unfold beat, start and length of the rearrange move.
    pub unfold: f32,
    pub move_start: f32,
    pub move_len: f32,
    /// 0 = normal, 1 = grows in (robot-only: head, authored arms/weapons), −1 = folds away (alt-only).
    pub scale_mode: i8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransformRig {
    /// Alt mode split into panels: part 0 = empty root, parts 1.. = panels. Pose with [`pose_transform`].
    pub alt: VoxelModel,
    /// Robot mode: biped rig (root torso, head, arms, legs) made of the same panel voxels.
    pub robot: VoxelModel,
    /// Index-aligned with `alt.parts` (entry 0 is a dummy for the root).
    pub panels: Vec<Panel>,
    /// Robot height (m).
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

/// Nearest 90° rotation to `q`.
fn snap_rot(q: Quat, rots: &[[[i32; 3]; 3]]) -> [[i32; 3]; 3] {
    let mut best = (f32::MIN, IDENTITY_ROT);
    for m in rots {
        let d = rot_quat(m).dot(q).abs();
        if d > best.0 {
            best = (d, *m);
        }
    }
    best.1
}

const IDENTITY_ROT: [[i32; 3]; 3] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];

// ---------------------------------------------------------------------------------- voxel sets

struct Proto {
    role: RobotRole,
    /// Alt-space voxels.
    voxels: Vec<(IVec3, Tint)>,
    scale_mode: i8,
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

const N26: [IVec3; 26] = {
    let mut out = [IVec3::ZERO; 26];
    let mut k = 0;
    let mut z = -1;
    while z <= 1 {
        let mut y = -1;
        while y <= 1 {
            let mut x = -1;
            while x <= 1 {
                if x != 0 || y != 0 || z != 0 {
                    out[k] = IVec3::new(x, y, z);
                    k += 1;
                }
                x += 1;
            }
            y += 1;
        }
        z += 1;
    }
    out
};

/// 26-connected components.
fn components(vox: &[(IVec3, Tint)]) -> Vec<Vec<(IVec3, Tint)>> {
    let map: HashMap<IVec3, Tint> = vox.iter().copied().collect();
    let mut seen: HashSet<IVec3> = HashSet::new();
    let mut out = Vec::new();
    let mut sorted: Vec<IVec3> = map.keys().copied().collect();
    sorted.sort_by_key(|v| (v.z, v.y, v.x));
    for v in sorted {
        if !seen.insert(v) {
            continue;
        }
        let mut comp = Vec::new();
        let mut stack = vec![v];
        while let Some(p) = stack.pop() {
            comp.push((p, map[&p]));
            for d in N26 {
                let q = p + d;
                if map.contains_key(&q) && seen.insert(q) {
                    stack.push(q);
                }
            }
        }
        comp.sort_by_key(|(v, _)| (v.z, v.y, v.x));
        out.push(comp);
    }
    out
}

/// Number of 26-connected components of a voxel set.
pub fn component_count(vox: &[IVec3]) -> usize {
    let t: Vec<(IVec3, Tint)> = vox.iter().map(|v| (*v, Tint::Primary)).collect();
    components(&t).len()
}

// ---------------------------------------------------------------------------------- splitting

/// Region → role for one alt voxel. `u`,`v`,`w` = normalised x (left→right), y (bottom→top), z
/// (front→rear).
fn region(u: f32, v: f32, w: f32) -> RobotRole {
    use RobotRole::*;
    let left = u < 0.5;
    let side = u < 0.28 || u > 0.72;
    if v >= 0.55 && w < 0.55 {
        // Hood + windshield/cab: the iconic chest.
        Chest
    } else if v >= 0.55 && w < 0.72 {
        Back
    } else if w < 0.28 {
        if u < 0.3 {
            FistL
        } else if u > 0.7 {
            FistR
        } else {
            Abdomen
        }
    } else if w < 0.62 {
        if side {
            if v >= 0.3 {
                if left { WingL } else { WingR }
            } else if w < 0.45 {
                if left { ForearmL } else { ForearmR }
            } else if left {
                UpperArmL
            } else {
                UpperArmR
            }
        } else {
            Pelvis
        }
    } else if w > 0.88 && v < 0.4 {
        if left { FootL } else { FootR }
    } else if w < 0.76 {
        if left { ThighL } else { ThighR }
    } else if left {
        ShinL
    } else {
        ShinR
    }
}

/// Split `alt` into connected panels (regions + wheel parts), merging/culling crumbs.
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
    let is_wheel = |i: usize| {
        i > 0 && (alt.parts[i].role == Some(PartRole::Wheel) || matches!(alt.machine.motion(i), crate::features::Motion::Spin { roll_k, .. } if roll_k > 0.0)) && alt.parts[i].voxels.len() >= MIN_PANEL_VOXELS
    };
    // Wheels: front → shoulder pads, rear → calves, extras → hips.
    let mut wheel_parts = Vec::new();
    let mut used = [0usize; 4];
    for i in 0..alt.parts.len() {
        if !is_wheel(i) {
            continue;
        }
        let (a, b) = bbox(&alt.parts[i].voxels);
        let c = norm((a + b) / 2);
        let (left, front) = (c.x < 0.5, c.z < 0.5);
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
        protos.push(Proto { role, voxels: alt.parts[i].voxels.clone(), scale_mode: 0 });
        wheel_parts.push(i);
    }
    // Regions → connected components.
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
    let mut comps: Vec<(RobotRole, Vec<(IVec3, Tint)>)> = Vec::new();
    for (role, vox) in by_role {
        for c in components(&vox) {
            comps.push((role, c));
        }
    }
    // Merge small pieces into a touching piece (any role); cull isolated crumbs; cap the count.
    loop {
        let wheel_n = protos.len();
        let total = comps.len() + wheel_n;
        let Some(k) = (0..comps.len()).filter(|&k| comps[k].1.len() < MIN_PANEL_VOXELS || total > MAX_PANELS).min_by_key(|&k| (comps[k].1.len(), k)) else { break };
        let owner: HashMap<IVec3, usize> = comps.iter().enumerate().filter(|(j, _)| *j != k).flat_map(|(j, (_, v))| v.iter().map(move |(p, _)| (*p, j))).collect();
        let mut target: Option<usize> = None;
        'find: for (p, _) in &comps[k].1 {
            for d in N26 {
                if let Some(&j) = owner.get(&(*p + d)) {
                    target = Some(j);
                    break 'find;
                }
            }
        }
        let small = std::mem::take(&mut comps[k].1);
        match target {
            Some(j) => comps[j].1.extend(small),
            None if small.len() < MIN_PANEL_VOXELS => {}
            None => {
                comps[k].1 = small;
                // Isolated but big: keep it and stop merging (nothing touches it).
                if total <= MAX_PANELS + 8 {
                    break;
                }
                let j = (0..comps.len()).filter(|&j| j != k && !comps[j].1.is_empty()).min_by_key(|&j| comps[j].1.len());
                match j {
                    Some(j) => {
                        let v = std::mem::take(&mut comps[k].1);
                        comps[j].1.extend(v);
                    }
                    None => break,
                }
            }
        }
        comps.retain(|c| !c.1.is_empty());
    }
    for (role, voxels) in comps {
        protos.push(Proto { role, voxels, scale_mode: 0 });
    }
    // Missing structural pieces grow in (no hidden geometry).
    let centre = (mn + mx) / 2;
    let s = ((ext.x.min(ext.y).min(ext.z)) * 0.18).round().max(1.0) as i32;
    for role in [Chest, Pelvis, UpperArmL, UpperArmR, ForearmL, ForearmR, ThighL, ThighR, ShinL, ShinR] {
        if protos.iter().any(|p| p.role == role) {
            continue;
        }
        let (len, w) = if role.limb() { (s * 3, s + 1) } else { (s * 2, s * 2) };
        protos.push(Proto { role, voxels: shell_box(centre, IVec3::new(w, len, w), Tint::Dark), scale_mode: 1 });
    }
    protos.push(Proto { role: Head, voxels: robot_head(centre, (ext.x * 0.2).round().max(3.0) as i32), scale_mode: 1 });
    protos.retain(|p| !p.voxels.is_empty());
    protos
}

fn shell_box(c: IVec3, size: IVec3, t: Tint) -> Vec<(IVec3, Tint)> {
    let size = size.max(IVec3::ONE);
    let mut out = Vec::new();
    for y in 0..size.y {
        for x in 0..size.x {
            for z in 0..size.z {
                if x == 0 || y == 0 || z == 0 || x == size.x - 1 || y == size.y - 1 || z == size.z - 1 {
                    let tint = if y == size.y - 1 { Tint::Accent } else { t };
                    out.push((c + IVec3::new(x, y, z) - size / 2, tint));
                }
            }
        }
    }
    out
}

/// Robot head: helmet shell with a central crest, "ear" fins and a glowing Core visor.
fn robot_head(c: IVec3, hs: i32) -> Vec<(IVec3, Tint)> {
    let hs = hs.max(3) | 1;
    let r = hs / 2;
    let mut out = Vec::new();
    for y in 0..=hs {
        for x in -r..=r {
            for z in -r..=r {
                if !(x.abs() == r || z.abs() == r || y == 0 || y == hs) {
                    continue;
                }
                let visor = z == -r && (y == hs / 2 || y == hs / 2 + 1) && x.abs() < r;
                let mouth = z == -r && y > 0 && y < hs / 2 - 1 && x.abs() <= r / 2;
                let t = if visor {
                    Tint::Core
                } else if mouth || y == 0 {
                    Tint::Dark
                } else if y == hs || (x == 0 && z == -r) {
                    Tint::Accent
                } else {
                    Tint::Primary
                };
                out.push((c + IVec3::new(x, y - r, z), t));
            }
        }
    }
    // Crest + ear fins.
    for k in 1..=2 {
        out.push((c + IVec3::new(0, hs - r + k, -r + 1), Tint::Accent));
        out.push((c + IVec3::new(0, hs - r + k - 1, -r), Tint::Accent));
        for sx in [-1, 1] {
            out.push((c + IVec3::new(sx * (r + 1), hs / 2 - r + k, 0), Tint::Accent));
            out.push((c + IVec3::new(sx * (r + 1), hs / 2 - r + k + 1, 1), Tint::Accent));
        }
    }
    out
}

// ------------------------------------------------------------------------------------- layout

struct Placed {
    proto: usize,
    rot: [[i32; 3]; 3],
    #[allow(dead_code)]
    rmin: IVec3,
    at: IVec3,
}

impl Placed {
    fn voxels(&self, p: &Proto) -> Vec<IVec3> {
        p.voxels.iter().map(|(v, _)| rot_apply(&self.rot, *v) + self.at).collect()
    }
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
        let tc = (tmin + tmax) / 2;
        let mut min = IVec3::new(tc.x - size.x / 2, tc.y - size.y / 2, tc.z - size.z / 2);
        use RobotRole::*;
        match p.role {
            FootL | FootR | ShinL | ShinR | ThighL | ThighR | Pelvis | Abdomen | Chest | Head => min.y = tmin.y,
            UpperArmL | UpperArmR | ForearmL | ForearmR | FistL | FistR => min.y = tmax.y - size.y + 1,
            // Door wings hang from the shoulder line, peeking just above it.
            WingL | WingR => min.y = tmax.y - size.y + 2,
            _ => {}
        }
        // Extra pieces of one role spread a little (they are snapped together afterwards).
        let spread = (k + 1) / 2 * if k % 2 == 1 { 1 } else { -1 };
        min.z += spread * (size.z / 3).max(1);
        placed.push(Placed { proto: i, rot, rmin: a, at: min - a });
    }
    connect(&protos, &mut placed);
    let _ = seed;
    assemble(alt.voxel_size, &protos, &placed, None)
}

/// Robot skeleton boxes (voxel units, feet at y = 0, facing −Z): heroic proportions — broad chest
/// and shoulders, narrow waist, thick shins and feet — scaled so the robot is about as tall as the
/// vehicle is long.
fn skeleton(protos: &[Proto]) -> Vec<(RobotRole, IVec3, IVec3)> {
    use RobotRole::*;
    let all: Vec<(IVec3, Tint)> = protos.iter().filter(|p| p.scale_mode == 0).flat_map(|p| p.voxels.iter().copied()).collect();
    let (a, b) = if all.is_empty() { (IVec3::ZERO, IVec3::splat(8)) } else { bbox(&all) };
    let e = (b - a + IVec3::ONE).as_vec3();
    let h = e.z.max(e.x).max(e.y * 1.6).max(12.0);
    let r = |x: f32| (x.round() as i32).max(1);
    let cw = r((e.x * 0.95).max(h * 0.42));
    let cd = r((h * 0.2).max(4.0));
    let tw = r(cw as f32 * 0.3).max(2);
    let sw = r(cw as f32 * 0.36).max(3);
    let aw = r(cw as f32 * 0.24).max(2);
    let (foot, shin, thigh) = (r(h * 0.06), r(h * 0.2), r(h * 0.18));
    let (pelvis, abd, chest, head) = (r(h * 0.08), r(h * 0.09), r(h * 0.21), r(h * 0.12));
    let (upper, fore, fist) = (r(h * 0.19), r(h * 0.17), r(h * 0.08));
    let pad = r(h * 0.14);
    let y_shin = foot;
    let y_thigh = y_shin + shin;
    let y_pel = y_thigh + thigh;
    let y_abd = y_pel + pelvis;
    let y_chest = y_abd + abd;
    let top = y_chest + chest;
    let pw = r(cw as f32 * 0.62);
    let lx = (pw / 2 - tw / 2).max(tw / 2 + 1);
    let ax = cw / 2 + aw / 2 + 1;
    let bx = |cx: i32, y0: i32, w: i32, hh: i32, cz: i32, d: i32| (IVec3::new(cx - w / 2, y0, cz - d / 2), IVec3::new(cx - w / 2 + w - 1, y0 + hh - 1, cz - d / 2 + d - 1));
    let mut out = Vec::new();
    for s in [-1i32, 1] {
        let pick = |l: RobotRole, rr: RobotRole| if s < 0 { l } else { rr };
        let (a1, b1) = bx(s * lx, 0, sw + 1, foot, -1, sw + 4);
        out.push((pick(FootL, FootR), a1, b1));
        let (a1, b1) = bx(s * lx, y_shin, sw, shin, 0, sw);
        out.push((pick(ShinL, ShinR), a1, b1));
        let (a1, b1) = bx(s * (lx + sw / 2 + 1), y_shin + shin / 4, 2, shin / 2, 0, shin / 2);
        out.push((pick(CalfL, CalfR), a1, b1));
        let (a1, b1) = bx(s * lx, y_thigh, tw, thigh, 0, tw);
        out.push((pick(ThighL, ThighR), a1, b1));
        let (a1, b1) = bx(s * (pw / 2 + 1), y_pel, 2, pelvis, 0, pelvis);
        out.push((pick(HipL, HipR), a1, b1));
        let (a1, b1) = bx(s * ax, top - upper, aw, upper, 0, aw);
        out.push((pick(UpperArmL, UpperArmR), a1, b1));
        let (a1, b1) = bx(s * ax, top - upper - fore, aw + 1, fore, -1, aw + 1);
        out.push((pick(ForearmL, ForearmR), a1, b1));
        let (a1, b1) = bx(s * ax, top - upper - fore - fist, aw + 1, fist, -1, aw + 1);
        out.push((pick(FistL, FistR), a1, b1));
        let (a1, b1) = bx(s * (ax + 1), top - pad / 2 - 1, pad, pad, 0, pad);
        out.push((pick(ShoulderL, ShoulderR), a1, b1));
        let (a1, b1) = bx(s * (cw / 4 + 1), y_chest + chest / 3, cw / 2, chest, cd / 2 + 2, 2);
        out.push((pick(WingL, WingR), a1, b1));
    }
    let (a1, b1) = bx(0, y_pel, pw, pelvis, 0, cd - 2);
    out.push((Pelvis, a1, b1));
    let (a1, b1) = bx(0, y_abd, r(cw as f32 * 0.55), abd, 0, cd - 2);
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
/// the chest keeps its windshield facing forward; wheels keep their axle across).
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
            Chest | Head if *m == IDENTITY_ROT => cost -= 4.0 + e.element_sum() * 0.25,
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

fn set_of(protos: &[Proto], placed: &[Placed], idx: &[usize]) -> HashSet<IVec3> {
    idx.iter().flat_map(|&i| placed[i].voxels(&protos[placed[i].proto])).collect()
}

fn vbox(v: &[IVec3]) -> (IVec3, IVec3) {
    let mut mn = IVec3::splat(i32::MAX);
    let mut mx = IVec3::splat(i32::MIN);
    for p in v {
        mn = mn.min(*p);
        mx = mx.max(*p);
    }
    (mn, mx)
}

/// Unit step moving box `a` toward box `b` (largest separating axis; centres if they overlap).
fn step_toward(a: (IVec3, IVec3), b: (IVec3, IVec3)) -> IVec3 {
    let mut best = (0, IVec3::ZERO);
    for k in 0..3 {
        let gap_pos = b.0[k] - a.1[k];
        let gap_neg = a.0[k] - b.1[k];
        let (g, dir) = if gap_pos > 0 { (gap_pos, 1) } else if gap_neg > 0 { (gap_neg, -1) } else { (0, 0) };
        if g > best.0 {
            let mut d = IVec3::ZERO;
            d[k] = dir * (g - 1).max(1);
            best = (g, d);
        }
    }
    if best.0 > 0 {
        return best.1;
    }
    let d = (b.0 + b.1) - (a.0 + a.1);
    let k = if d.x.abs() >= d.y.abs() && d.x.abs() >= d.z.abs() { 0 } else if d.y.abs() >= d.z.abs() { 1 } else { 2 };
    let mut s = IVec3::ZERO;
    s[k] = d[k].signum();
    s
}

fn touches(set: &HashSet<IVec3>, v: &[IVec3], reach: i32) -> bool {
    v.iter().any(|p| {
        if set.contains(p) {
            return true;
        }
        for z in -reach..=reach {
            for y in -reach..=reach {
                for x in -reach..=reach {
                    if set.contains(&(*p + IVec3::new(x, y, z))) {
                        return true;
                    }
                }
            }
        }
        false
    })
}

/// Snap panels so every body part is one connected piece and every part meets the torso
/// (limbs within a 1-voxel joint gap).
fn connect(protos: &[Proto], placed: &mut [Placed]) {
    let live: Vec<usize> = (0..placed.len()).filter(|&i| protos[placed[i].proto].scale_mode >= 0).collect();
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); 6];
    for &i in &live {
        groups[protos[placed[i].proto].role.group()].push(i);
    }
    for members in &mut groups {
        members.sort_by_key(|&i| (protos[placed[i].proto].role.anchor_rank(), std::cmp::Reverse(protos[placed[i].proto].voxels.len()), i));
        if members.is_empty() {
            continue;
        }
        let mut set: HashSet<IVec3> = placed[members[0]].voxels(&protos[placed[members[0]].proto]).into_iter().collect();
        let anchor_c = {
            let v: Vec<IVec3> = set.iter().copied().collect();
            let (a, b) = vbox(&v);
            (a + b) / 2
        };
        let mut rest: Vec<usize> = members[1..].to_vec();
        rest.sort_by_key(|&i| {
            let v = placed[i].voxels(&protos[placed[i].proto]);
            let (a, b) = vbox(&v);
            (((a + b) / 2) - anchor_c).length_squared()
        });
        for i in rest {
            for _ in 0..64 {
                let v = placed[i].voxels(&protos[placed[i].proto]);
                if touches(&set, &v, 1) {
                    break;
                }
                let sv: Vec<IVec3> = set.iter().copied().collect();
                placed[i].at += step_toward(vbox(&v), vbox(&sv));
            }
            set.extend(placed[i].voxels(&protos[placed[i].proto]));
        }
    }
    // Limbs and head meet the torso (joint gap ≤ 1 voxel).
    let torso = set_of(protos, placed, &groups[0]);
    if torso.is_empty() {
        return;
    }
    let tv: Vec<IVec3> = torso.iter().copied().collect();
    let tb = vbox(&tv);
    for members in groups.iter().skip(1) {
        if members.is_empty() {
            continue;
        }
        for _ in 0..64 {
            let gv: Vec<IVec3> = set_of(protos, placed, members).into_iter().collect();
            if touches(&torso, &gv, 2) {
                break;
            }
            let st = step_toward(vbox(&gv), tb);
            for &i in members {
                placed[i].at += st;
            }
        }
    }
}

// ----------------------------------------------------------------------------------- assemble

fn assemble(vs: f32, protos: &[Proto], placed: &[Placed], pivots: Option<&[Option<Vec3>; 6]>) -> TransformRig {
    use RobotRole::*;
    let vs = vs.max(1e-3);
    let mut alt_parts = vec![PartModel { id: 0, parent: 0, parent_index: 0, pivot: Vec3::ZERO, role: None, voxels: Vec::new(), vmin: IVec3::ZERO, vmax: IVec3::ZERO, depth: 0, voxel_size: vs }];
    let dummy = Panel { role: Chest, rot: IDENTITY_ROT, offset: IVec3::ZERO, alt_center: Vec3::ZERO, robot_center: Vec3::ZERO, q: Quat::IDENTITY, axis: Vec3::Y, angle: 0.0, pop: Vec3::ZERO, unfold: 0.0, move_start: 0.0, move_len: 0.0, scale_mode: 0 };
    let mut panels = vec![dummy];
    let mut groups: Vec<Vec<(IVec3, Tint)>> = vec![Vec::new(); 6];
    let solid: Vec<(IVec3, Tint)> = protos.iter().filter(|p| p.scale_mode == 0).flat_map(|p| p.voxels.iter().copied()).collect();
    let (amn, amx) = if solid.is_empty() { (IVec3::ZERO, IVec3::ZERO) } else { bbox(&solid) };
    let alt_c = (amn + amx).as_vec3() * 0.5 * vs;
    for (k, pl) in placed.iter().enumerate() {
        let pr = &protos[pl.proto];
        let (a, b) = bbox(&pr.voxels);
        let c0 = (a + b).as_vec3() * 0.5 * vs;
        let q = rot_quat(&pl.rot);
        let c1 = q * c0 + pl.at.as_vec3() * vs;
        let (axis, angle) = q.to_axis_angle();
        let out = c0 - alt_c;
        let pop = if out.length_squared() > 1e-6 { (out.normalize() + Vec3::Y * 0.3).normalize() } else { Vec3::Y };
        alt_parts.push(PartModel { id: (k + 1).min(250) as PartId, parent: 0, parent_index: 0, pivot: c0, role: Some(PartRole::Other), voxels: pr.voxels.clone(), vmin: a, vmax: b, depth: 1, voxel_size: vs });
        panels.push(Panel {
            role: pr.role,
            rot: pl.rot,
            offset: pl.at,
            alt_center: c0,
            robot_center: c1,
            q,
            axis: if axis.is_finite() { axis } else { Vec3::Y },
            angle: if angle.is_finite() { angle } else { 0.0 },
            pop,
            unfold: 0.0,
            move_start: 0.0,
            move_len: 0.0,
            scale_mode: pr.scale_mode,
        });
        if pr.scale_mode >= 0 {
            let g = pr.role.group();
            for (v, t) in &pr.voxels {
                groups[g].push((rot_apply(&pl.rot, *v) + pl.at, *t));
            }
        }
    }
    // Beat timing: unfold outside-in, then legs → torso → wings/back → arms → shoulder pads, head last.
    let n = panels.len().saturating_sub(1).max(1);
    let mut order: Vec<usize> = (1..panels.len()).collect();
    order.sort_by(|&a, &b| (panels[b].alt_center - alt_c).length().total_cmp(&(panels[a].alt_center - alt_c).length()).then(a.cmp(&b)));
    for (rank, &i) in order.iter().enumerate() {
        let p = &mut panels[i];
        let f = rank as f32 / n as f32;
        p.unfold = 0.12 * f;
        let base = match p.role {
            ThighL | ThighR | ShinL | ShinR | FootL | FootR | CalfL | CalfR => 0.26,
            Pelvis | Abdomen | Chest | HipL | HipR => 0.3,
            Back | WingL | WingR => 0.34,
            UpperArmL | UpperArmR | ForearmL | ForearmR | FistL | FistR => 0.36,
            ShoulderL | ShoulderR => 0.42,
            Head => 0.64,
        };
        p.move_start = if p.scale_mode == 1 && p.role != Head { 0.4 } else { base } + 0.04 * f;
        p.move_len = if p.role == Head { 0.18 } else { 0.3 };
    }
    // Robot model.
    let roles = [None, Some(PartRole::Head), Some(PartRole::ArmLeft), Some(PartRole::ArmRight), Some(PartRole::LegBackLeft), Some(PartRole::LegBackRight)];
    let mut robot_parts = Vec::new();
    for g in 0..6 {
        if g > 0 && groups[g].is_empty() {
            continue;
        }
        let (a, b) = if groups[g].is_empty() { (IVec3::ZERO, IVec3::ZERO) } else { bbox(&groups[g]) };
        let c = (a + b).as_vec3() * 0.5;
        let auto = match g {
            0 => Vec3::ZERO,
            1 => Vec3::new(c.x, a.y as f32 - 0.5, c.z) * vs,
            _ => Vec3::new(c.x, b.y as f32, c.z) * vs,
        };
        let pivot = pivots.and_then(|p| p[g]).unwrap_or(auto);
        robot_parts.push(PartModel { id: robot_parts.len() as PartId, parent: 0, parent_index: 0, pivot, role: roles[g], voxels: std::mem::take(&mut groups[g]), vmin: IVec3::ZERO, vmax: IVec3::ZERO, depth: (g > 0) as u8, voxel_size: vs });
    }
    let robot = model_from_parts(robot_parts, vs);
    let height = (robot.bounds.max.y - robot.bounds.min.y).max(0.25);
    let alt_model = model_from_parts(alt_parts, vs);
    TransformRig { alt: alt_model, robot, panels, height }
}

// ------------------------------------------------------------------------------ authored icons

/// Two-mode icon: `alt` = vehicle/beast mode, `robot` = robot mode. Parts present in both are the
/// same panel rigidly moved (the transform is read from the prims, snapped to 90° / whole voxels);
/// robot-only parts (head, arms, weapons) grow in; alt-only parts fold away. The robot part roles
/// (Head / Arm* / Leg*, walking up the parent chain) decide which robot-model part each rides in.
pub fn transform_rig_authored(alt: &ShapeSpec, robot: &ShapeSpec, cap: usize) -> TransformRig {
    use RobotRole::*;
    let am = crate::voxelize::voxelize_capped(alt, cap);
    let k = (am.voxel_size / bd_core::CONSTRUCT_VOXEL_SIZE).round().max(1.0);
    let rm = crate::voxelize::voxelize_at(robot, k as u32);
    let rots = all_rotations();
    // Group of a robot part: walk the parent chain to the first limb/head role.
    let group_of = |id: PartId| -> (usize, bool) {
        let mut cur = id;
        for _ in 0..32 {
            let Some(p) = robot.parts.iter().find(|p| p.id == cur) else { return (0, false) };
            match p.role {
                PartRole::Head => return (1, false),
                PartRole::ArmLeft => return (2, true),
                PartRole::ArmRight => return (3, true),
                PartRole::LegBackLeft | PartRole::LegFrontLeft => return (4, true),
                PartRole::LegBackRight | PartRole::LegFrontRight => return (5, true),
                _ => {}
            }
            if p.parent == cur {
                break;
            }
            cur = p.parent;
        }
        (0, false)
    };
    let role_for = |g: usize| [Chest, Head, UpperArmL, UpperArmR, ThighL, ThighR][g];
    let first_prim = |s: &ShapeSpec, id: PartId| s.prims.iter().find(|p| p.part == id && !p.subtract).cloned();
    let mut protos = Vec::new();
    let mut placed = Vec::new();
    let alt_c = {
        let all: Vec<(IVec3, Tint)> = am.parts.iter().flat_map(|p| p.voxels.iter().copied()).collect();
        if all.is_empty() { IVec3::ZERO } else { let (a, b) = bbox(&all); (a + b) / 2 }
    };
    for ap in &am.parts {
        if ap.voxels.is_empty() {
            continue;
        }
        let rp = rm.parts.iter().find(|p| p.id == ap.id && !p.voxels.is_empty());
        match (rp, first_prim(alt, ap.id), first_prim(robot, ap.id)) {
            (Some(rp), Some(pa), Some(pr)) => {
                // Rigid move read from the prims: R = q_r · q_a⁻¹, t = c_r − R c_a (units → voxels /k).
                let q = crate::build::quat_from_rot_deg(pr.rot_deg) * crate::build::quat_from_rot_deg(pa.rot_deg).inverse();
                let rot = snap_rot(q, &rots);
                let rq = rot_quat(&rot);
                let t = (pr.center - rq * pa.center) / k;
                let at = t.round().as_ivec3();
                let rv: Vec<(IVec3, Tint)> = ap.voxels.iter().map(|(v, t)| (rot_apply(&rot, *v), *t)).collect();
                let (a, _) = bbox(&rv);
                let (g, _) = group_of(rp.id);
                placed.push(Placed { proto: protos.len(), rot, rmin: a, at });
                protos.push(Proto { role: role_for(g), voxels: ap.voxels.clone(), scale_mode: 0 });
            }
            _ => {
                // Alt-only: folds away into the body.
                placed.push(Placed { proto: protos.len(), rot: IDENTITY_ROT, rmin: IVec3::ZERO, at: IVec3::ZERO });
                protos.push(Proto { role: Back, voxels: ap.voxels.clone(), scale_mode: -1 });
            }
        }
    }
    // Robot-only parts grow in from the body centre.
    for rp in &rm.parts {
        if rp.voxels.is_empty() || am.parts.iter().any(|a| a.id == rp.id && !a.voxels.is_empty()) {
            continue;
        }
        let (a, b) = bbox(&rp.voxels);
        let shift = (a + b) / 2 - alt_c;
        let voxels: Vec<(IVec3, Tint)> = rp.voxels.iter().map(|(v, t)| (*v - shift, *t)).collect();
        let (g, _) = group_of(rp.id);
        let role = if g == 1 { Head } else { role_for(g) };
        placed.push(Placed { proto: protos.len(), rot: IDENTITY_ROT, rmin: a - shift, at: shift });
        protos.push(Proto { role, voxels, scale_mode: 1 });
    }
    if protos.iter().all(|p| p.scale_mode != 0) {
        return transform_rig(&am, 0);
    }
    // Feet on the ground, then the same no-floaters pass as procedural robots.
    let low = placed.iter().enumerate().filter(|(i, _)| protos[*i].scale_mode >= 0).map(|(i, p)| protos[i].voxels.iter().map(|(v, _)| rot_apply(&p.rot, *v).y + p.at.y).min().unwrap_or(0)).min().unwrap_or(0);
    for p in placed.iter_mut() {
        p.at.y -= low;
    }
    connect(&protos, &mut placed);
    // Authored limb pivots.
    let mut pivots: [Option<Vec3>; 6] = [None; 6];
    for p in &robot.parts {
        let (g, limb) = group_of(p.id);
        if (limb || g == 1) && pivots[g].is_none() && p.pivot.is_finite() {
            pivots[g] = Some((p.pivot / k - Vec3::Y * low as f32) * am.voxel_size);
        }
    }
    assemble(am.voxel_size, &protos, &placed, Some(&pivots))
}

// --------------------------------------------------------------------------------- sequence

fn ease(e: Ease, u: f32) -> f32 {
    e.apply(u.clamp(0.0, 1.0))
}

/// Pose the panel model at normalised time `t` (0 = alt, 1 = robot) — three beats: unfold,
/// rearrange, lock-in (see module docs). `to_robot = false` plays robot → alt (t: 0 = robot, 1 =
/// alt). Allocation-free; `out` must hold `rig.alt.parts.len()` entries (model space, metres).
pub fn pose_transform(rig: &TransformRig, t: f32, to_robot: bool, out: &mut [Affine3A]) {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let t = if to_robot { t } else { 1.0 - t };
    let n = rig.panels.len().min(out.len());
    // Lock-in: squash while the last panels land, then spring up (stretch overshoot) and settle.
    let squash = smoothstep(0.5, 0.8, t) * (1.0 - ease(Ease::OutBack, (t - 0.82) / 0.18));
    let g = Affine3A::from_scale(Vec3::new(1.0 + 0.07 * squash, 1.0 - 0.16 * squash, 1.0 + 0.07 * squash));
    let vs = rig.alt.voxel_size;
    for (i, o) in out.iter_mut().enumerate().take(n) {
        if i == 0 {
            *o = Affine3A::IDENTITY;
            continue;
        }
        let p = &rig.panels[i];
        if t >= 1.0 {
            *o = if p.scale_mode < 0 { Affine3A::from_scale(Vec3::splat(1e-3)) } else { exact(p, vs) };
            continue;
        }
        // Beat 1: unlatch + pop + hinge flip.
        let u1 = (t - p.unfold) / 0.2;
        let pop = p.pop * (vs * 2.0) * ease(Ease::OutCubic, u1) * (1.0 - smoothstep(0.5, 0.75, t));
        let r = ease(Ease::OutBack, (t - p.unfold - 0.04) / 0.24);
        let q = Quat::from_axis_angle(p.axis, p.angle * r);
        // Beat 2: slide along an arc into the robot slot.
        let um = (t - p.move_start) / p.move_len.max(1e-3);
        let m = ease(Ease::OutBack, um);
        let arc = Vec3::Y * ((p.robot_center - p.alt_center).length() * 0.18 * (PI * um.clamp(0.0, 1.0)).sin());
        let c = p.alt_center.lerp(p.robot_center, m) + arc + pop;
        let s = match p.scale_mode {
            1 => ease(Ease::OutBack, um).max(1e-3),
            -1 => (1.0 - smoothstep(0.2, 0.55, t)).max(1e-3),
            _ => 1.0,
        };
        let a = Affine3A::from_translation(c) * Affine3A::from_quat(q) * Affine3A::from_scale(Vec3::splat(s)) * Affine3A::from_translation(-p.alt_center);
        let a = g * a;
        *o = if a.is_finite() { a } else { Affine3A::IDENTITY };
    }
}

/// Exact robot placement of a panel (maps alt voxel centres onto robot voxel centres).
fn exact(p: &Panel, vs: f32) -> Affine3A {
    Affine3A::from_translation(p.offset.as_vec3() * vs) * Affine3A::from_quat(p.q)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransformEventKind {
    /// Panel unlatches (click + spark).
    Unlatch,
    /// Hinge flip peaks (whirr).
    Hinge,
    /// Panel locks into its slot (clack + spark burst).
    Lock,
    /// The head pops up (visor flash).
    HeadUp,
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
    /// Where it happens (model space, metres; alt-side for unlatch, robot-side for lock).
    pub pos: Vec3,
}

/// Events of the alt → robot sequence, sorted by time (reverse: use `1 - time`).
pub fn sequence_events(rig: &TransformRig) -> Vec<TransformEvent> {
    let mut ev = Vec::new();
    for (i, p) in rig.panels.iter().enumerate().skip(1) {
        if p.scale_mode == 0 {
            ev.push(TransformEvent { time: p.unfold, panel: i, kind: TransformEventKind::Unlatch, pos: p.alt_center });
            if p.angle.abs() > 0.1 {
                ev.push(TransformEvent { time: p.unfold + 0.12, panel: i, kind: TransformEventKind::Hinge, pos: p.alt_center + p.pop * 0.3 });
            }
        }
        let end = (p.move_start + p.move_len).min(1.0);
        let kind = if p.role == RobotRole::Head { TransformEventKind::HeadUp } else { TransformEventKind::Lock };
        if p.scale_mode >= 0 {
            ev.push(TransformEvent { time: end, panel: i, kind, pos: p.robot_center });
        }
    }
    ev.push(TransformEvent { time: 0.9, panel: 0, kind: TransformEventKind::StandUp, pos: Vec3::ZERO });
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
