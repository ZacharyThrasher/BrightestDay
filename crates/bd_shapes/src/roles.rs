//! Construct-animation roles: what each part of a model *is* for animation — turret, barrel,
//! hatch, wheel, rotor, piston, fin, thruster, head, jaw, eye, blade — so behaviours key off
//! **roles and semantic events, never words** (see [`crate::behaviours`]). A tank animates because
//! it has a turret + barrel + hatch + wheels, not because anyone authored "tank".
//!
//! Resolution per part, first match wins:
//! 1. the part's declared [`PartRole`] (bd_core v1.5 roles are tagged where parts are generated:
//!    forms, machine kit, costumes-free creature eyes, mechanic attachments, cockpits);
//! 2. its machine-kit motion (`Aim` yaw/pitch → turret/barrel, rolling `Spin` → wheel, free `Spin`
//!    → rotor, `Slide` → piston);
//! 3. its rig kind (disc → wheel, glow → exhaust, small glow on a head → eye, chain → cloth);
//! 4. **conservative inference** for hand-authored icons / grafts whose parts are only `Other`
//!    (skipped entirely when the model declares any v1.5 role — its author already said what is
//!    what). Every rule needs strong geometric evidence; when unsure the part stays [`Role::None`]
//!    (unanimated) rather than doing something wrong:
//!    * **Barrel**: ≥ 4 voxels long along Z, ≥ 2.5× longer than thick, pivot at its back end (it
//!      extends forward from the pivot), clear of the ground, not on a leg/arm/tail.
//!    * **Turret**: the non-root parent of a barrel that sits on the upper part of the model with a
//!      compact footprint (≤ 3:1).
//!    * **Hatch**: a thin plate (≤ 1/3 as tall as wide) lying on top of its parent, ≤ 15% of the
//!      model footprint, on a vehicle-like model (static gait or a pilot seat). Hinge = the part edge
//!      nearest its pivot.
//!      A plate with a mirrored twin is never a hatch (wings / doors).
//!    * **Fin**: a mirrored pair (same parent, x ≈ −x, same size/height) of thin side plates.
//!    * **Jaw**: the *one* smaller part hanging off a `Head` below and in front of its centre (several =
//!      tentacles / beard: left alone).
//!    * **Rotor**: a flat round-ish disc (thin in Y, ≥ 4×4 voxels) at the very top of a legless model.
//!
//! Pure and deterministic; allocates only in [`analyze`] (once per model).

use bd_core::shape::PartRole;
use glam::{IVec3, Vec3};

use crate::anim::{Gait, PartKind};
use crate::features::Motion;
use crate::voxelize::{PartModel, SEAT_PART, VoxelModel};

/// Animation role of one part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    /// Not animated by role behaviours (clips / machine kit may still move it).
    None,
    /// Rolls (machine kit / rig). Front wheels of a steerable vehicle steer.
    Wheel {
        radius: f32,
        steer: bool,
    },
    /// Yaws toward the aim.
    Turret,
    /// Pitches toward the aim; recoils along +Z (backward) on fire. `len` in metres.
    Barrel {
        len: f32,
    },
    /// Hinged panel: rotation of `open` rad about `axis` through `hinge` (model rest space, m) opens it.
    Hatch {
        hinge: Vec3,
        axis: Vec3,
        open: f32,
    },
    /// Spins about `axis` (unit). `machine` = the machine kit already spins it at `rate` rad/s (the
    /// behaviour then only reshapes the speed: spin-up, throttle).
    Rotor {
        axis: Vec3,
        rate: f32,
        machine: bool,
    },
    /// Pumps along `axis` by up to `amp` metres.
    Piston {
        axis: Vec3,
        amp: f32,
    },
    /// Flexes about `axis` (unit) with turns and speed.
    Fin {
        axis: Vec3,
    },
    /// Creature wing (the rig overlay flaps / folds it).
    Wing {
        side: f32,
    },
    /// Engine nozzle flame: flares with throttle. `dir` = exhaust direction (model space).
    Thruster {
        dir: Vec3,
    },
    /// Exhaust flicker / puff source.
    Exhaust {
        dir: Vec3,
    },
    Head,
    Jaw,
    Eye,
    /// Extends from its pivot along `axis` (0 = X, 1 = Y, 2 = Z).
    Blade {
        axis: u8,
    },
    /// Pilot seat marker (never drawn).
    Seat,
    /// Hand grip of a held item (forms first in the hand).
    Grip,
    /// Second-hand marker (never drawn).
    Support,
    /// Legs, arms, tails (assembly order only).
    Limb,
    /// Capes, sails, flags, chains (assembly order only).
    Cloth,
}

impl Role {
    /// Short name (debug overlays, galleries).
    pub fn name(&self) -> &'static str {
        match self {
            Role::None => "-",
            Role::Wheel { steer: true, .. } => "WHEEL/STEER",
            Role::Wheel { .. } => "WHEEL",
            Role::Turret => "TURRET",
            Role::Barrel { .. } => "BARREL",
            Role::Hatch { .. } => "HATCH",
            Role::Rotor { .. } => "ROTOR",
            Role::Piston { .. } => "PISTON",
            Role::Fin { .. } => "FIN",
            Role::Wing { .. } => "WING",
            Role::Thruster { .. } => "THRUSTER",
            Role::Exhaust { .. } => "EXHAUST",
            Role::Head => "HEAD",
            Role::Jaw => "JAW",
            Role::Eye => "EYE",
            Role::Blade { .. } => "BLADE",
            Role::Seat => "SEAT",
            Role::Grip => "GRIP",
            Role::Support => "SUPPORT",
            Role::Limb => "LIMB",
            Role::Cloth => "CLOTH",
        }
    }
}

/// Summon-assembly stage order: core → limbs/wheels → turret/head/wings → barrels/blades/jaw →
/// lock (hatches shut, eyes light).
pub const STAGE_CORE: u8 = 0;
pub const STAGE_LIMBS: u8 = 1;
pub const STAGE_UPPER: u8 = 2;
pub const STAGE_TOOLS: u8 = 3;
pub const STAGE_LOCK: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartRoleInfo {
    pub role: Role,
    /// Found by the geometric inference (not declared / machine / rig).
    pub inferred: bool,
    /// Compressed assembly stage (0 = core; the last stage is the lock).
    pub stage: u8,
    /// Rest-pose centre of the part's voxels (m, model space).
    pub center: Vec3,
    /// Direction the part arrives from at the end of its flight (unit, model space): outward from
    /// the model centre, biased up.
    pub out: Vec3,
}

/// Roles of every part of a model (index-aligned with `VoxelModel::parts`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Roles {
    pub parts: Vec<PartRoleInfo>,
    /// Number of (non-empty) assembly stages.
    pub stages: u8,
    /// Model rest bounds centre (m) and characteristic size (max extent, m).
    pub center: Vec3,
    pub size: f32,
    /// Engine-driven (wheels, rotors, thrusters, turret…): idles with a rumble.
    pub machine: bool,
    pub has_hatch: bool,
    pub has_turret: bool,
    pub has_head: bool,
}

impl Roles {
    pub fn role(&self, i: usize) -> Role {
        self.parts.get(i).map_or(Role::None, |p| p.role)
    }
    pub fn count(&self, f: impl Fn(&Role) -> bool) -> usize {
        self.parts.iter().filter(|p| f(&p.role)).count()
    }
}

fn ext_vox(p: &PartModel) -> Vec3 {
    if p.voxels.is_empty() { Vec3::ZERO } else { (p.vmax - p.vmin + IVec3::ONE).as_vec3() }
}

fn center_m(p: &PartModel) -> Vec3 {
    let b = p.aabb();
    (b.min + b.max) * 0.5
}

fn axis_vec(k: usize) -> Vec3 {
    match k {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    }
}

fn argmin(v: Vec3) -> usize {
    if v.x <= v.y && v.x <= v.z {
        0
    } else if v.y <= v.z {
        1
    } else {
        2
    }
}

fn argmax(v: Vec3) -> usize {
    if v.x >= v.y && v.x >= v.z {
        0
    } else if v.y >= v.z {
        1
    } else {
        2
    }
}

fn is_v15(r: PartRole) -> bool {
    matches!(
        r,
        PartRole::Hatch
            | PartRole::Turret
            | PartRole::Barrel
            | PartRole::Rotor
            | PartRole::Piston
            | PartRole::Fin
            | PartRole::Thruster
            | PartRole::Eye
            | PartRole::Blade
            | PartRole::Grip
            | PartRole::Support
    )
}

fn is_limb(r: PartRole) -> bool {
    matches!(
        r,
        PartRole::LegFrontLeft
            | PartRole::LegFrontRight
            | PartRole::LegBackLeft
            | PartRole::LegBackRight
            | PartRole::ArmLeft
            | PartRole::ArmRight
            | PartRole::Tail
    )
}

/// Hinge of a hatch-like part: the edge nearest its pivot. Returns (hinge point, axis, signed open
/// angle) — positive rotation about `axis` through `hinge` by `open` swings it open (up and/or out).
pub fn hinge_of(p: &PartModel, model_center: Vec3) -> (Vec3, Vec3, f32) {
    let b = p.aabb();
    let (mn, mx) = (b.min, b.max);
    let e = mx - mn;
    let c = (mn + mx) * 0.5;
    let pv = p.pivot;
    // A part no taller than it is wide/deep is a lid (hinged along a horizontal edge).
    let thin = if e.y <= e.x.min(e.z) + 1e-4 { 1 } else { argmin(e) };
    // Nearest face to the pivot, among the faces whose normal is not the thin axis.
    let mut best = (f32::MAX, 2usize, 1.0f32);
    for a in 0..3 {
        if a == thin {
            continue;
        }
        for (face, s) in [(mn[a], -1.0f32), (mx[a], 1.0)] {
            let d = (pv[a] - face).abs();
            if d < best.0 {
                best = (d, a, s);
            }
        }
    }
    let (_, a, s) = best;
    let face = if s < 0.0 { mn[a] } else { mx[a] };
    // Hinge axis ⟂ the face normal and ⟂ the thin axis when possible (a lid hinges along its edge).
    let rest: Vec<usize> = (0..3).filter(|&k| k != a).collect();
    let ax = if rest.contains(&thin) {
        *rest.iter().find(|&&k| k != thin).unwrap_or(&0)
    } else if a == 1 {
        // Hinged at a top/bottom edge of a lid thin in Y can't happen (thin excluded); pick the
        // longer horizontal extent.
        if e.x >= e.z { 0 } else { 2 }
    } else {
        // Face normal horizontal: hinge along the other horizontal axis unless the panel is a side
        // door (then vertical).
        if rest.contains(&0) && rest.contains(&2) { if a == 0 { 2 } else { 0 } } else { rest[0] }
    };
    let axis = axis_vec(ax);
    let mut hinge = c;
    hinge[a] = face;
    // Keep the hinge on the pivot's side of the thin axis.
    hinge[thin] = pv[thin].clamp(mn[thin], mx[thin]);
    // Open direction: up, plus outward along the thin axis when the panel is off-centre.
    let mut want = Vec3::Y;
    if thin != 1 {
        let off = c[thin] - model_center[thin];
        if off.abs() > 0.05 {
            want += axis_vec(thin) * off.signum() * 1.2;
        }
    }
    let r = c - hinge;
    let sgn = if axis.cross(r).dot(want) >= 0.0 { 1.0 } else { -1.0 };
    let ang = if thin == 1 {
        1.9 // lid / top hatch
    } else if a == 1 {
        1.35 // swings down / up from a bottom or top edge
    } else {
        1.05 // canopy hinged at its rear
    };
    (hinge, axis, sgn * ang)
}

/// Resolve every part's role (see module docs).
pub fn analyze(model: &VoxelModel) -> Roles {
    let parts = &model.parts;
    let n = parts.len();
    let bmin = model.bounds.min;
    let bmax = model.bounds.max;
    let center = (bmin + bmax) * 0.5;
    let size = (bmax - bmin).max_element().max(0.25);
    let vs = model.voxel_size.max(1e-3);
    let any_v15 = parts.iter().any(|p| p.role.is_some_and(is_v15));
    let kind = |i: usize| model.rig.kinds.get(i).copied().unwrap_or(PartKind::Sway);

    let mut roles = vec![Role::None; n];
    let mut inferred = vec![false; n];
    for i in 1..n {
        let p = &parts[i];
        if p.id == SEAT_PART {
            roles[i] = Role::Seat;
            continue;
        }
        let e = ext_vox(p);
        let c = center_m(p);
        // 2. Machine motion.
        let m = model.machine.motion(i);
        let from_motion = match m {
            Motion::Aim { pitch: false } => Some(Role::Turret),
            Motion::Aim { pitch: true } => Some(Role::Barrel { len: (e.z * vs).max(vs) }),
            Motion::Spin { roll_k, .. } if roll_k.abs() > 1e-6 => Some(Role::Wheel { radius: 1.0 / roll_k.abs(), steer: false }),
            Motion::Spin { axis, rate, surf_k, tick, .. } if rate.abs() > 1e-3 && surf_k.abs() < 1e-6 && !tick => {
                Some(Role::Rotor { axis, rate, machine: true })
            }
            Motion::Slide { axis, amp, .. } => Some(Role::Piston { axis, amp }),
            _ => None,
        };
        if let Some(r) = from_motion {
            roles[i] = r;
            continue;
        }
        // 1. Declared role.
        let declared = match p.role {
            Some(PartRole::Hatch) => {
                let (hinge, axis, open) = hinge_of(p, center);
                Some(Role::Hatch { hinge, axis, open })
            }
            Some(PartRole::Turret) => Some(Role::Turret),
            Some(PartRole::Barrel) => Some(Role::Barrel { len: (e.z * vs).max(vs) }),
            Some(PartRole::Rotor) => Some(Role::Rotor { axis: axis_vec(argmin(e)), rate: 14.0, machine: false }),
            Some(PartRole::Piston) => Some(Role::Piston { axis: axis_vec(argmax(e)), amp: (e.max_element() * vs * 0.2).max(vs) }),
            Some(PartRole::Fin) => Some(Role::Fin { axis: fin_axis(e) }),
            Some(PartRole::Thruster) => Some(Role::Thruster { dir: (c - p.pivot).normalize_or(Vec3::Z) }),
            Some(PartRole::Exhaust) => Some(Role::Exhaust { dir: (c - p.pivot).normalize_or(Vec3::Z) }),
            Some(PartRole::Eye) => Some(Role::Eye),
            Some(PartRole::Blade) => Some(Role::Blade { axis: argmax(e) as u8 }),
            Some(PartRole::Grip) => Some(Role::Grip),
            Some(PartRole::Support) => Some(Role::Support),
            Some(PartRole::Head) => Some(Role::Head),
            Some(PartRole::Jaw) => Some(Role::Jaw),
            Some(PartRole::WingLeft) => Some(Role::Wing { side: -1.0 }),
            Some(PartRole::WingRight) => Some(Role::Wing { side: 1.0 }),
            Some(PartRole::Cloth) => Some(Role::Cloth),
            Some(r) if is_limb(r) => Some(Role::Limb),
            _ => None,
        };
        if let Some(r) = declared {
            roles[i] = r;
            continue;
        }
        // 3. Rig kind.
        match kind(i) {
            PartKind::Wheel(r) => roles[i] = Role::Wheel { radius: r, steer: false },
            PartKind::Flame => roles[i] = Role::Exhaust { dir: (c - p.pivot).normalize_or(Vec3::Z) },
            PartKind::Rigid if p.role == Some(PartRole::Other) && is_core(p) => roles[i] = Role::Eye,
            PartKind::Chain { .. } => roles[i] = Role::Cloth,
            PartKind::Machine => roles[i] = Role::Limb,
            _ => {}
        }
    }

    // 4. Conservative inference (untagged models only; the same pass `voxelize` ran to make these
    // parts rigid in the clips).
    if !any_v15 {
        for (i, r) in infer(
            parts,
            &model.rig.kinds,
            &model.machine,
            model.bounds.min,
            model.bounds.max,
            vs,
            model.rig.gait,
            model.rig.legs.is_empty(),
            model.seat.is_some(),
        ) {
            roles[i] = r;
            inferred[i] = true;
        }
    }

    // Steering: front wheels of a free-wheeled vehicle (no treads / cranks).
    let wheels: Vec<usize> = (1..n).filter(|&i| matches!(roles[i], Role::Wheel { .. })).collect();
    let tracked = model.machine.motions.iter().any(|m| matches!(m, Motion::Scroll { .. } | Motion::Crank { .. }));
    if !tracked && wheels.len() >= 3 && model.rig.gait == Gait::Static {
        let zs: Vec<f32> = wheels.iter().map(|&i| parts[i].pivot.z).collect();
        let zmin = zs.iter().copied().fold(f32::MAX, f32::min);
        let zmax = zs.iter().copied().fold(f32::MIN, f32::max);
        if zmax - zmin > 0.5 {
            for (k, &i) in wheels.iter().enumerate() {
                if zs[k] <= zmin + 0.2 * (zmax - zmin)
                    && let Role::Wheel { radius, .. } = roles[i]
                {
                    roles[i] = Role::Wheel { radius, steer: true };
                }
            }
        }
    }

    // Assembly stages.
    let mut stage = vec![STAGE_CORE; n];
    for i in 1..n {
        let s = match roles[i] {
            Role::Wheel { .. } | Role::Limb | Role::Piston { .. } | Role::Cloth => STAGE_LIMBS,
            Role::Grip | Role::Seat | Role::Support => STAGE_CORE,
            Role::Turret | Role::Head | Role::Wing { .. } | Role::Fin { .. } | Role::Rotor { .. } | Role::None => STAGE_UPPER,
            Role::Barrel { .. } | Role::Blade { .. } | Role::Jaw | Role::Thruster { .. } | Role::Exhaust { .. } => STAGE_TOOLS,
            // Hatches arrive with their parent (open) and shut at the lock stage (see behaviours).
            Role::Hatch { .. } => STAGE_UPPER,
            Role::Eye => STAGE_LOCK,
        };
        let pi = parts[i].parent_index.min(i.saturating_sub(1));
        stage[i] = s.max(if pi == 0 { 0 } else { stage[pi] });
        if matches!(roles[i], Role::Hatch { .. }) {
            stage[i] = stage[pi].max(STAGE_LIMBS);
        }
    }
    // Compress to the stages actually used (the lock stage stays last when present).
    let mut used = [false; 5];
    used[0] = true;
    for &s in &stage {
        used[s as usize] = true;
    }
    let has_hatch = roles.iter().any(|r| matches!(r, Role::Hatch { .. }));
    if has_hatch {
        used[STAGE_LOCK as usize] = true;
    }
    let mut remap = [0u8; 5];
    let mut k = 0u8;
    for s in 0..5 {
        if used[s] {
            remap[s] = k;
            k += 1;
        }
    }
    let stages = k.max(1);
    let infos = (0..n)
        .map(|i| {
            let c = center_m(&parts[i]);
            let radial = c - center;
            let horiz = Vec3::new(radial.x, 0.0, radial.z);
            let out = if i == 0 { Vec3::Y } else { (horiz.normalize_or(Vec3::ZERO) * 0.6 + Vec3::Y).normalize_or(Vec3::Y) };
            PartRoleInfo { role: roles[i], inferred: inferred[i], stage: remap[stage[i] as usize], center: c, out }
        })
        .collect();
    let machine = roles
        .iter()
        .any(|r| matches!(r, Role::Wheel { .. } | Role::Rotor { .. } | Role::Thruster { .. } | Role::Turret | Role::Piston { .. }));
    Roles {
        parts: infos,
        stages,
        center,
        size,
        machine,
        has_hatch,
        has_turret: roles.contains(&Role::Turret),
        has_head: roles.contains(&Role::Head),
    }
}

// ------------------------------------------------------------------------------ grip / support

/// Principal axis (unit) of `pts` if clearly elongated (largest variance ≥ `ratio` × the next).
fn principal_axis(pts: &[Vec3], ratio: f32) -> Option<Vec3> {
    if pts.len() < 2 {
        return None;
    }
    let mean = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
    let mut cov = glam::Mat3::ZERO;
    for p in pts {
        let d = *p - mean;
        cov += glam::Mat3::from_cols(d * d.x, d * d.y, d * d.z);
    }
    // Power iteration (deterministic start, a handful of steps is plenty for 3x3).
    let mut v = Vec3::new(0.31, 0.53, 0.79).normalize();
    for _ in 0..24 {
        v = (cov * v).normalize_or(Vec3::Z);
    }
    let l1 = v.dot(cov * v);
    // Second eigenvalue: deflate.
    let c2 = cov - glam::Mat3::from_cols(v * (l1 * v.x), v * (l1 * v.y), v * (l1 * v.z));
    let mut u = Vec3::new(0.7, -0.2, 0.4).normalize();
    for _ in 0..24 {
        u = (c2 * u).normalize_or(Vec3::X);
    }
    let l2 = u.dot(c2 * u).max(0.0);
    (l1.is_finite() && l1 > 1e-6 && l1 >= ratio * l2).then_some(v)
}

fn orient_toward(axis: Vec3, from: Vec3, to: Vec3) -> Vec3 {
    let d = to - from;
    let s = axis.dot(d);
    if s.abs() > 1e-4 {
        axis * s.signum()
    } else if axis.y.abs() > 0.5 {
        axis * axis.y.signum()
    } else if axis.z > 0.0 {
        -axis
    } else {
        axis
    }
}

/// Where the hand holds a **held item** (origin = grip by convention): `(palm centre, handle axis)`
/// in model rest space (metres). The axis is a unit vector along the handle pointing from the hand
/// toward the bulk of the item (a sword hilt → toward the blade, a pistol grip → up into the gun).
/// The grip rides the root part: pose it with `pose[grip_part(model)]` (usually part 0's pose).
///
/// Generated held forms tag their handle as a [`PartRole::Grip`] part; untagged icons fall back to
/// a conservative inference (voxels near the origin; axis from their principal direction when it is
/// clearly elongated, else the held-item convention: along Z). `None` when nothing is near the
/// origin (not a held item).
pub fn grip_point(model: &VoxelModel) -> Option<(Vec3, Vec3)> {
    let bulk = (model.bounds.min + model.bounds.max) * 0.5;
    if let Some(i) = grip_part(model) {
        let p = &model.parts[i];
        let pts: Vec<Vec3> = p.voxels.iter().map(|(v, _)| p.voxel_center(*v)).collect();
        let axis = principal_axis(&pts, 1.2).or_else(|| {
            let e = ext_vox(p);
            (e.max_element() > e.min_element()).then(|| axis_vec(argmax(e)))
        });
        let axis = orient_toward(axis.unwrap_or(Vec3::NEG_Z), p.pivot, bulk);
        return Some((p.pivot, axis));
    }
    // Inference: the origin must be on the item.
    let vs = model.voxel_size.max(1e-3);
    let near = (2.0 * vs).max(0.2);
    let reach = (3.5 * vs).max(0.3);
    let mut pts: Vec<Vec3> = Vec::new();
    let mut touching = false;
    for p in &model.parts {
        for (v, _) in &p.voxels {
            let c = p.voxel_center(*v);
            let d = c.length();
            touching |= d <= near;
            if d <= reach {
                pts.push(c);
            }
        }
    }
    if !touching {
        return None;
    }
    let axis = principal_axis(&pts, 2.5).unwrap_or(Vec3::Z);
    Some((Vec3::ZERO, orient_toward(axis, Vec3::ZERO, bulk)))
}

/// Index of the tagged `Grip` part (`None` for untagged models: their grip rides the root, part 0).
pub fn grip_part(model: &VoxelModel) -> Option<usize> {
    model.parts.iter().position(|p| p.role == Some(PartRole::Grip))
}

/// Second-hand point of a two-handed held item (model rest space, metres): the tagged
/// [`PartRole::Support`] marker (rifles, launchers, cannons, staves, spears, big hammers/axes), else
/// — for long untagged icons (≥ 1.4 m along the item) — a point on the item 35% of the way from the
/// grip toward the far end, only if there is geometry there. `None` for one-handed items.
pub fn support_point(model: &VoxelModel) -> Option<Vec3> {
    if let Some(p) = model.parts.iter().find(|p| p.role == Some(PartRole::Support)) {
        return Some(p.pivot);
    }
    if model.parts.iter().any(|p| p.role == Some(PartRole::Grip)) {
        return None; // tagged one-hander
    }
    let (g, axis) = grip_point(model)?;
    let b = &model.bounds;
    let corners = [b.min, b.max, Vec3::new(b.min.x, b.min.y, b.max.z), Vec3::new(b.max.x, b.max.y, b.min.z)];
    let len = corners.iter().map(|c| (*c - g).dot(axis)).fold(0.0f32, f32::max);
    if len < 1.4 {
        return None;
    }
    let want = g + axis * (len * 0.35);
    let tol = (1.5 * model.voxel_size).max(0.15);
    let mut best: Option<(f32, Vec3)> = None;
    for p in &model.parts {
        for (v, _) in &p.voxels {
            let c = p.voxel_center(*v);
            let d = c.distance(want);
            if d <= tol && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, c));
            }
        }
    }
    best.map(|(_, c)| c)
}

fn is_core(p: &PartModel) -> bool {
    !p.voxels.is_empty() && p.voxels.iter().all(|(_, t)| *t == bd_core::shape::Tint::Core)
}

/// The conservative geometric inference (module docs, step 4) over untagged `Other` / `Weapon`
/// parts: `(part index, role)` for every part it is sure about. Run by `voxelize` (the parts it
/// names become rigid in the clips — a turret must not sway like cloth) and by [`analyze`].
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
pub(crate) fn infer(
    parts: &[PartModel],
    kinds: &[PartKind],
    machine: &crate::features::MachineRig,
    bmin: Vec3,
    bmax: Vec3,
    vs: f32,
    gait: Gait,
    legless: bool,
    has_seat: bool,
) -> Vec<(usize, Role)> {
    let n = parts.len();
    let mut out: Vec<(usize, Role)> = Vec::new();
    if n < 2 || parts.iter().any(|p| p.role.is_some_and(is_v15)) {
        return out;
    }
    let vs = vs.max(1e-3);
    let height = (bmax.y - bmin.y).max(0.25);
    let center = (bmin + bmax) * 0.5;
    let limb_ancestor = |i: usize| {
        let mut k = i;
        let mut guard = 0;
        while k != 0 && guard < 64 {
            if parts[k].role.is_some_and(is_limb) {
                return true;
            }
            k = parts[k].parent_index.min(k.saturating_sub(1));
            guard += 1;
        }
        false
    };
    let taken = |out: &[(usize, Role)], i: usize| out.iter().any(|(j, _)| *j == i);
    let candidate = |out: &[(usize, Role)], i: usize| -> bool {
        let p = &parts[i];
        !taken(out, i)
            && p.id != SEAT_PART
            && matches!(p.role, Some(PartRole::Other) | Some(PartRole::Weapon))
            && matches!(kinds.get(i), Some(PartKind::Sway | PartKind::Chain { .. } | PartKind::Rigid))
            && machine.motion(i) == Motion::None
            && p.voxels.len() >= 2
            && !is_core(p)
            && !limb_ancestor(i)
    };
    // Barrels: long along Z, thin, extending forward from the pivot, clear of the ground.
    for i in 1..n {
        if !candidate(&out, i) {
            continue;
        }
        let p = &parts[i];
        let e = ext_vox(p);
        let b = p.aabb();
        let len = e.z;
        let thick = e.x.max(e.y);
        let forward_from_pivot = (p.pivot.z - b.min.z) >= 0.6 * (b.max.z - b.min.z);
        let clear = b.min.y >= bmin.y + 0.25 * height;
        if len >= 4.0 && len >= 2.5 * thick && forward_from_pivot && clear {
            out.push((i, Role::Barrel { len: len * vs }));
        }
    }
    // Turrets: the compact, high, non-root parent of a barrel.
    let barrels: Vec<usize> = out.iter().map(|(i, _)| *i).collect();
    for i in barrels {
        let pi = parts[i].parent_index;
        if pi == 0 || pi >= i || !candidate(&out, pi) {
            continue;
        }
        let e = ext_vox(&parts[pi]);
        let b = parts[pi].aabb();
        let compact = e.x.max(e.z) <= 3.0 * e.x.min(e.z).max(1.0);
        if compact && b.min.y >= bmin.y + 0.3 * height {
            out.push((pi, Role::Turret));
        }
    }
    // Hatches: thin, small plates lying on top of their parent, on vehicle-like models.
    if gait == Gait::Static || has_seat {
        let footprint = ((bmax.x - bmin.x) / vs).max(1.0) * ((bmax.z - bmin.z) / vs).max(1.0);
        for i in 1..n {
            if !candidate(&out, i) {
                continue;
            }
            let p = &parts[i];
            let e = ext_vox(p);
            let b = p.aabb();
            let parent_top = parts[p.parent_index.min(i - 1)].aabb().max.y;
            let thin = e.y <= 1.0 || e.y * 3.0 <= e.x.min(e.z);
            let small = e.x * e.z <= 0.15 * footprint && e.x >= 2.0 && e.z >= 2.0;
            let on_top = b.min.y >= parent_top - 2.0 * vs && b.min.y >= bmin.y + 0.4 * height;
            // A mirrored twin makes it a wing / fin / door pair, not a hatch.
            let c = center_m(p);
            let twin = c.x.abs() >= vs
                && (1..n).any(|j| {
                    j != i && parts[j].parent_index == p.parent_index && {
                        let cj = center_m(&parts[j]);
                        (c.x + cj.x).abs() <= vs * 1.01 && (c.y - cj.y).abs() <= vs * 1.01 && (c.z - cj.z).abs() <= vs * 1.01
                    }
                });
            if thin && small && on_top && !twin {
                let (hinge, axis, open) = hinge_of(p, center);
                out.push((i, Role::Hatch { hinge, axis, open }));
            }
        }
    }
    // Fins: mirrored pairs of thin side plates (same parent, x ≈ −x, same size and height).
    for i in 1..n {
        if !candidate(&out, i) {
            continue;
        }
        let (ei, ci) = (ext_vox(&parts[i]), center_m(&parts[i]));
        if !(ei.y * 2.5 <= ei.x && ei.x >= 3.0 && ci.x.abs() >= vs) {
            continue;
        }
        for j in i + 1..n {
            if !candidate(&out, j) || parts[j].parent_index != parts[i].parent_index {
                continue;
            }
            let (ej, cj) = (ext_vox(&parts[j]), center_m(&parts[j]));
            let mirrored = (ci.x + cj.x).abs() <= vs * 1.01 && (ci.y - cj.y).abs() <= vs * 1.01 && (ci.z - cj.z).abs() <= vs * 1.01;
            let same = (ei - ej).abs().max_element() <= 1.0;
            if mirrored && same {
                out.push((i, Role::Fin { axis: Vec3::Z }));
                out.push((j, Role::Fin { axis: Vec3::Z }));
                break;
            }
        }
    }
    // Jaws: the one smaller part hanging below and in front of a head's centre (several such parts
    // are tentacles / a beard: ambiguous, left alone).
    let mut jaws: Vec<(usize, usize)> = Vec::new();
    for i in 1..n {
        if !candidate(&out, i) {
            continue;
        }
        let pi = parts[i].parent_index;
        if pi == 0 || pi >= i || parts[pi].role != Some(PartRole::Head) {
            continue;
        }
        let (c, hc) = (center_m(&parts[i]), center_m(&parts[pi]));
        let smaller = parts[i].voxels.len() * 2 <= parts[pi].voxels.len();
        if smaller && c.y < hc.y - 0.5 * vs && c.z < hc.z {
            jaws.push((pi, i));
        }
    }
    for &(head, i) in &jaws {
        if jaws.iter().filter(|(h, _)| *h == head).count() == 1 {
            out.push((i, Role::Jaw));
        }
    }
    // Rotors: a flat, round-ish disc hubbed at its pivot on the very top of a legless, wingless
    // (static-gait) model.
    if legless && gait == Gait::Static {
        for i in 1..n {
            if !candidate(&out, i) {
                continue;
            }
            let p = &parts[i];
            let e = ext_vox(p);
            let b = p.aabb();
            let c = center_m(p);
            // Blade-thin (a parachute / umbrella dome is far thicker than a rotor disc).
            let flat =
                e.y <= 3.0 && e.y * 4.0 <= e.x.min(e.z) && e.x >= 5.0 && e.z >= 5.0 && (e.x - e.z).abs() <= 0.25 * e.x.max(e.z) + 1.0;
            let top = b.max.y >= bmax.y - vs * 1.01;
            let hub = (Vec3::new(p.pivot.x - c.x, 0.0, p.pivot.z - c.z)).length() <= 1.5 * vs;
            if flat && top && hub {
                out.push((i, Role::Rotor { axis: Vec3::Y, rate: 14.0, machine: false }));
            }
        }
    }
    out
}

/// Flex axis of a fin: roll about Z for horizontal plates, yaw about Y for vertical ones.
fn fin_axis(e: Vec3) -> Vec3 {
    if e.y < e.x.min(e.z) { Vec3::Z } else { Vec3::Y }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::shape::{Part, Prim, PrimKind, ShapeSpec, Tint};

    fn bx(c: Vec3, s: Vec3, part: u8) -> Prim {
        Prim { kind: PrimKind::Box, center: c, size: s, rot_deg: Vec3::ZERO, tint: Tint::Primary, part, subtract: false }
    }

    #[test]
    fn hinge_opens_upward() {
        let spec = ShapeSpec {
            prims: vec![
                bx(Vec3::new(0.0, 2.0, 0.0), Vec3::new(8.0, 4.0, 12.0), 0),
                bx(Vec3::new(0.0, 4.5, 0.0), Vec3::new(3.0, 1.0, 3.0), 1),
            ],
            parts: vec![Part { id: 1, parent: 0, pivot: Vec3::new(0.0, 4.5, 1.5), role: PartRole::Hatch }],
        };
        let m = crate::voxelize(&spec);
        let r = analyze(&m);
        let Role::Hatch { hinge, axis, open } = r.role(1) else { panic!("{:?}", r.role(1)) };
        // Rotating the lid's centre about the hinge by `open` must lift it.
        let c = m.parts[1].aabb();
        let c = (c.min + c.max) * 0.5;
        let q = glam::Quat::from_axis_angle(axis, open * 0.5);
        let moved = hinge + q * (c - hinge);
        assert!(moved.y > c.y + 0.05, "{moved:?} vs {c:?}");
        assert!((hinge.z - 1.5 * 0.25).abs() < 0.2, "hinge at the rear edge: {hinge:?}");
    }
}
