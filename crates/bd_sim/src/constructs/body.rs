//! Generic rigid-body physics for constructs (framework; pipeline step 4 in `mod.rs`).
//!
//! Every non-attached construct is a box (`c.half`, `c.rot`, centre `c.center()`) with mass
//! (`c.mass()`, ×3 for Heavy). [`integrate`] moves it according to `c.body.loco`:
//!
//! * `Ballistic` — gravity × `body.gravity`, swept centre ray against voxels (reflect with
//!   `body.restitution`), footprint ground resolution (the lowest point of the box rests on the
//!   highest voxel top under its footprint), friction when supported (rollers keep momentum and
//!   accelerate down slopes: boulders follow slopes and fall into holes), angular `body.spin`
//!   (tumbling / toppling; a toppling body settles when it lies flat).
//! * `Walk` — gravity + ground following with step-up (hop onto ledges ≤ step height); blocked by
//!   taller walls.
//! * `Fly` — no gravity; stops against terrain (slides along it).
//! * `Static` — placed structures: never move (unless knocked `loose`, then Ballistic).
//! * `Kinematic` — the controller placed it; only contacts are recorded.
//!
//! Then contacts: against every other construct's [`Proxy`] (closest-point box test; solid ones
//! are resolved with a mass-weighted impulse — both bodies resolve their own share when they step,
//! so pushing, stacking and toppling fall out of it; immovable structures get knocked loose by big
//! impulses via [`Interaction::Loosen`]) and against players (recorded only; the world's movement
//! collides players with construct colliders). Bounded work: O(constructs) box tests + ≤ 5
//! bounded voxel column scans + 1–2 raycasts per body per tick; no allocation.

use bd_core::mechanics::{Form, FormCategory, Mechanic};
use bd_core::voxel::{VoxelQuery, raycast, world_to_voxel};
use bd_core::{IVec3, Quat, VOXEL_SIZE, Vec3};
use bd_voxel::VoxelWorld;

use super::{Construct, ConstructOut, Contact, ContactWith, Env, Interaction, Loco, Proxy};

/// m/s² for free bodies (objects fall a little faster than players for weight).
pub const GRAVITY: f32 = 20.0;
/// Speed cap for free bodies (m/s).
pub const MAX_SPEED: f32 = 90.0;
/// Horizontal friction (1/s) for resting bodies, and for rollers.
const FRICTION: f32 = 5.0;
const ROLL_FRICTION: f32 = 0.25;
/// A structure hit harder than this (m/s of velocity change it would get) is knocked loose.
pub const LOOSEN_DV: f32 = 3.0;

/// Mass used for contact impulses (Heavy triples it: anvils win shoving matches).
#[inline]
pub fn body_mass(c: &Construct) -> f32 {
    c.mass() * if c.has(Mechanic::Heavy) { 3.0 } else { 1.0 }
}

/// Mass of a proxy for impulses (0 inverse mass = immovable).
#[inline]
fn inv_mass(p: &Proxy) -> f32 {
    if !p.free {
        return 0.0;
    }
    let m = p.mass * if p.has(Mechanic::Heavy) { 3.0 } else { 1.0 };
    1.0 / m.max(0.5)
}

/// Does this form roll (keeps momentum on the ground, accelerates down slopes)?
#[inline]
pub fn roller(c: &Construct) -> bool {
    matches!(c.recipe.form, Form::Boulder | Form::Ball)
}

/// Half extent of `c`'s box along unit direction `n` (support radius).
#[inline]
pub fn support(half: Vec3, rot: Quat, n: Vec3) -> f32 {
    let (ax, ay, az) = (rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z);
    n.dot(ax).abs() * half.x + n.dot(ay).abs() * half.y + n.dot(az).abs() * half.z
}

/// Highest voxel top in column (x, z) between `bottom` and `top` (world y), bounded scan.
pub fn ground_top(v: &VoxelWorld, x: f32, z: f32, top: f32, bottom: f32) -> Option<f32> {
    if !(x.is_finite() && z.is_finite() && top.is_finite() && bottom.is_finite()) {
        return None;
    }
    let cx = (x / VOXEL_SIZE).floor() as i32;
    let cz = (z / VOXEL_SIZE).floor() as i32;
    let y0 = (top / VOXEL_SIZE).floor() as i32;
    let y1 = ((bottom / VOXEL_SIZE).floor() as i32).max(y0 - 96);
    let mut y = y0;
    while y >= y1 {
        if v.is_solid(IVec3::new(cx, y, cz)) {
            return Some((y + 1) as f32 * VOXEL_SIZE);
        }
        y -= 1;
    }
    None
}

/// Move `c` for one tick according to `c.body.loco`, colliding with voxels, other constructs and
/// players; records contacts in `c.body.contacts`.
pub fn integrate(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    if c.attached() || c.gone.is_some() {
        return;
    }
    let dt = env.dt();
    let pre_vel = c.vel;
    c.body.supported = false;
    match c.body.loco {
        Loco::Kinematic => {}
        Loco::Static => c.vel = Vec3::ZERO,
        Loco::Fly => fly(c, env, dt),
        Loco::Walk => walk(c, env, dt),
        Loco::Ballistic => ballistic(c, env, dt),
    }
    if c.body.loco != Loco::Kinematic && c.body.loco != Loco::Static {
        spin(c, dt);
    }
    if !c.vel.is_finite() {
        c.vel = Vec3::ZERO;
    }
    if c.vel.length_squared() > MAX_SPEED * MAX_SPEED {
        c.vel = c.vel.normalize_or_zero() * MAX_SPEED;
    }
    if c.forming_t <= 0.0 {
        construct_contacts(c, env, pre_vel, out);
        player_contacts(c, env, pre_vel);
    }
}

fn spin(c: &mut Construct, dt: f32) {
    let w = c.body.spin;
    if w == Vec3::ZERO || !w.is_finite() {
        c.body.spin = Vec3::ZERO;
        return;
    }
    c.rot = (Quat::from_scaled_axis(w * dt) * c.rot).normalize();
    // A toppling structure settles once it lies flat on the ground.
    if c.body.loose && c.body.supported {
        let up = c.rot * Vec3::Y;
        let flat = up.y.abs() < 0.2 || (c.half.y < c.half.x.min(c.half.z) && up.y.abs() > 0.97);
        if flat {
            c.body.spin = Vec3::ZERO;
        } else {
            c.body.spin *= (1.0 - 1.5 * dt).max(0.0);
            // Gravity keeps a tipping body going (it pivots on its bottom edge).
            let tip = w.normalize_or_zero() * (2.5 * dt);
            c.body.spin += tip;
        }
    } else if c.body.supported {
        c.body.spin *= (1.0 - 4.0 * dt).max(0.0);
        if c.body.spin.length_squared() < 0.01 {
            c.body.spin = Vec3::ZERO;
        }
    }
}

/// Swept move of the centre with voxel collision (reflect), then footprint ground resolution.
fn ballistic(c: &mut Construct, env: &Env<'_>, dt: f32) {
    c.vel.y -= GRAVITY * c.body.gravity * env.ctx.gravity_at(c.pos.y) * dt;
    let v = env.ctx.voxels;
    sweep(c, env, dt);
    // Ground under the footprint.
    let b = c.bounds();
    let bottom = b.min.y;
    let h = b.half();
    let cx = b.center();
    let fx = (h.x * 0.8).min(6.0);
    let fz = (h.z * 0.8).min(6.0);
    let step = (h.y * 0.5).clamp(0.3, 2.0);
    let down = (-c.vel.y * dt).max(0.0) + 0.06;
    let mut floor: Option<f32> = None;
    for (ox, oz) in [(0.0, 0.0), (fx, fz), (-fx, fz), (fx, -fz), (-fx, -fz)] {
        if let Some(g) = ground_top(v, cx.x + ox, cx.z + oz, bottom + step, bottom - down) {
            floor = Some(floor.map_or(g, |f: f32| f.max(g)));
        }
    }
    if let Some(f) = floor {
        if bottom <= f + 0.02 {
            c.pos.y += f - bottom;
            let vy = c.vel.y;
            if vy < 0.0 {
                let e = c.body.restitution;
                c.body.contacts.push(Contact { with: ContactWith::Terrain, point: Vec3::new(cx.x, f, cx.z), normal: Vec3::Y, speed: -vy, vel: c.vel });
                c.vel.y = if -vy > 3.0 && e > 0.05 { -vy * e } else { 0.0 };
            }
            c.body.supported = c.vel.y <= 0.5;
        }
    }
    if c.body.supported {
        let fr = if roller(c) { ROLL_FRICTION } else { FRICTION };
        let k = (1.0 - fr * dt).max(0.0);
        c.vel.x *= k;
        c.vel.z *= k;
        if roller(c) {
            // Follow the slope: accelerate downhill (boulders follow slopes and holes).
            let p = Vec3::new(cx.x, bottom, cx.z);
            let hs = |dx: f32, dz: f32| ground_top(v, p.x + dx, p.z + dz, p.y + 1.5, p.y - 3.0).unwrap_or(p.y - 3.0);
            let gx = hs(0.75, 0.0) - hs(-0.75, 0.0);
            let gz = hs(0.0, 0.75) - hs(0.0, -0.75);
            let grad = Vec3::new(gx, 0.0, gz) / 1.5;
            if grad.is_finite() {
                c.vel -= grad.clamp_length_max(2.0) * (GRAVITY * 0.6 * dt);
            }
            // Roll visually.
            let hv = Vec3::new(c.vel.x, 0.0, c.vel.z);
            let r = c.half.y.max(0.2);
            c.body.spin = Vec3::Y.cross(hv) / r;
        }
    }
}

/// Swept centre ray: stop at terrain, reflect the normal component with restitution.
fn sweep(c: &mut Construct, env: &Env<'_>, dt: f32) {
    let d = c.vel * dt;
    let len = d.length();
    if len < 1e-6 {
        return;
    }
    let from = c.center();
    let r = c.half.min_element().clamp(0.1, 1.5);
    match raycast(env.ctx.voxels, from, d, len + r) {
        Some(h) if h.t > 0.0 => {
            let n = h.normal.as_vec3();
            let n = if n == Vec3::ZERO { -d / len } else { n };
            let travel = (h.t - r).max(0.0);
            c.pos += d / len * travel;
            let vn = c.vel.dot(n);
            if vn < 0.0 {
                c.body.contacts.push(Contact { with: ContactWith::Terrain, point: h.point, normal: n, speed: -vn, vel: c.vel });
                let e = c.body.restitution;
                c.vel -= n * vn * (1.0 + e);
            }
            if n.y > 0.6 {
                c.body.supported = true;
            }
        }
        Some(_) => {
            // Centre inside terrain (spawned in a wall / carried into one): pop up.
            c.pos.y += (VOXEL_SIZE).min(len.max(0.1));
            if c.vel.y < 0.0 {
                c.vel.y = 0.0;
            }
        }
        None => c.pos += d,
    }
}

/// Ground walker: gravity + ground following with step-up.
fn walk(c: &mut Construct, env: &Env<'_>, dt: f32) {
    let v = env.ctx.voxels;
    c.vel.y -= GRAVITY * c.body.gravity * env.ctx.gravity_at(c.pos.y) * dt;
    // Pivot → feet offset (objects pivot at their centre, structures / creatures at their feet).
    let foot = (c.pos.y - c.bounds().min.y).max(0.0);
    c.pos.y -= foot;
    let step_up = (c.half.y * 0.5).clamp(0.6, 4.0);
    let next = c.pos + c.vel * dt;
    // Horizontal wall check at knee height (taller than a hop: blocked).
    let hv = Vec3::new(c.vel.x, 0.0, c.vel.z) * dt;
    let hl = hv.length();
    let mut blocked = false;
    if hl > 1e-5 {
        let knee = c.pos + Vec3::Y * (step_up + 0.3);
        let reach = hl + (c.half.x.min(c.half.z) * 0.5).clamp(0.1, 1.0);
        if raycast(v, knee, hv, reach).is_some() {
            blocked = true;
        }
    }
    let target = if blocked { Vec3::new(c.pos.x, next.y, c.pos.z) } else { next };
    if blocked {
        let vy = c.vel.y;
        c.vel = Vec3::new(0.0, vy, 0.0);
        c.body.contacts.push(Contact { with: ContactWith::Terrain, point: c.pos, normal: -hv.normalize_or_zero(), speed: hl / dt, vel: c.vel });
    }
    let down = (-c.vel.y * dt).max(0.0) + 0.1;
    match ground_top(v, target.x, target.z, target.y + step_up, target.y - down) {
        Some(g) if c.vel.y <= 0.5 && g >= target.y - down => {
            if c.vel.y < -3.0 {
                c.body.contacts.push(Contact { with: ContactWith::Terrain, point: Vec3::new(target.x, g, target.z), normal: Vec3::Y, speed: -c.vel.y, vel: c.vel });
            }
            c.pos = Vec3::new(target.x, g, target.z);
            c.vel.y = 0.0;
            c.grounded = true;
            c.body.supported = true;
        }
        _ => {
            c.pos = target;
            c.grounded = false;
        }
    }
    c.pos.y += foot;
}

/// Self-propelled flier: no gravity, slides along terrain.
fn fly(c: &mut Construct, env: &Env<'_>, dt: f32) {
    let d = c.vel * dt;
    let len = d.length();
    c.grounded = false;
    if len < 1e-6 {
        return;
    }
    let from = c.center();
    let r = c.half.min_element().clamp(0.1, 1.5);
    match raycast(env.ctx.voxels, from, d, len + r) {
        Some(h) if h.t > 0.0 => {
            let n = h.normal.as_vec3();
            let n = if n == Vec3::ZERO { -d / len } else { n };
            let travel = (h.t - r).max(0.0);
            c.pos += d / len * travel;
            let vn = c.vel.dot(n);
            if vn < 0.0 {
                c.body.contacts.push(Contact { with: ContactWith::Terrain, point: h.point, normal: n, speed: -vn, vel: c.vel });
                c.vel -= n * vn;
            }
        }
        Some(_) => {
            c.pos += Vec3::Y * VOXEL_SIZE;
            c.vel = Vec3::ZERO;
        }
        None => c.pos += d,
    }
}

/// Is this construct able to move from contact impulses?
pub fn is_free(c: &Construct) -> bool {
    !c.attached()
        && c.pilot.is_none()
        && match c.body.loco {
            Loco::Static | Loco::Kinematic => false,
            _ => c.category != FormCategory::Structure || c.body.loose || c.has(Mechanic::Flies) || c.has(Mechanic::Walks) || c.has(Mechanic::Charge) || c.has(Mechanic::Homing),
        }
}

fn construct_contacts(c: &mut Construct, env: &Env<'_>, pre_vel: Vec3, out: &mut ConstructOut) {
    let me_free = is_free(c) && c.body.loco != Loco::Kinematic;
    let my_mass = body_mass(c);
    let my_im = if me_free { 1.0 / my_mass.max(0.5) } else { 0.0 };
    let mut center = c.center();
    let mut bounds = c.bounds();
    for o in env.others {
        if o.id == c.id || !o.active || o.attached || o.attached_to == Some(c.id) || c.comp.attached_to == Some(o.id) {
            continue;
        }
        if !bounds.intersects(&o.bounds()) {
            continue;
        }
        // Closest point of o's box to my centre.
        let inv = o.rot.inverse();
        let l = inv * (center - o.center);
        let q = l.clamp(-o.half, o.half);
        let (n, pen) = if (l - q).length_squared() < 1e-8 {
            // Centre inside o: least-penetration axis.
            let pa = o.half - l.abs();
            let (axis, depth) = if pa.x <= pa.y && pa.x <= pa.z {
                (Vec3::X * l.x.signum(), pa.x)
            } else if pa.y <= pa.z {
                (Vec3::Y * l.y.signum(), pa.y)
            } else {
                (Vec3::Z * l.z.signum(), pa.z)
            };
            let n = (o.rot * axis).normalize_or(Vec3::Y);
            (n, depth + support(c.half, c.rot, n))
        } else {
            let d = l - q;
            let dist = d.length();
            let n = (o.rot * (d / dist)).normalize_or(Vec3::Y);
            (n, support(c.half, c.rot, n) - dist)
        };
        if pen <= 0.0 {
            continue;
        }
        let rel = pre_vel - o.vel;
        let closing = (-rel.dot(n)).max(0.0);
        c.body.contacts.push(Contact { with: ContactWith::Construct(o.id), point: o.center + o.rot * q, normal: n, speed: closing, vel: pre_vel });
        if !o.solid || !me_free {
            // Immovable me hitting o: o resolves its own share when it steps.
            continue;
        }
        let o_im = inv_mass(o);
        let w = my_im + o_im;
        if w <= 0.0 {
            continue;
        }
        let share = my_im / w;
        let corr = (pen * share).min(0.5);
        c.pos += n * corr;
        center += n * corr;
        bounds = c.bounds();
        // Resting on it (a ramp is surfable: things dropped on it slide and keep going).
        if n.y > 0.6 && o.form != Form::Ramp {
            c.body.supported = true;
            c.grounded = true;
        }
        let vn = (c.vel - o.vel).dot(n);
        if vn < 0.0 {
            let e = if o.has(Mechanic::Bounce) || o.form == Form::Trampoline { 1.1 } else { c.body.restitution.min(0.2) };
            let j = -(1.0 + e) * vn / w;
            c.vel += n * (j * my_im);
            if o.has(Mechanic::Launch) {
                let fwd = o.rot * Vec3::NEG_Z;
                c.vel += fwd * 14.0 + Vec3::Y * 10.0;
            }
            // Knock an immovable structure loose when rammed hard enough.
            if o_im == 0.0 && o.category == FormCategory::Structure && o.form != Form::Platform && o.form != Form::Pillar {
                let dv_o = j / (o.mass * if o.has(Mechanic::Heavy) { 3.0 } else { 1.0 }).max(1.0);
                if dv_o > LOOSEN_DV {
                    let axis = Vec3::Y.cross(-n).normalize_or_zero();
                    out.queue.push(Interaction::Loosen { id: o.id, spin: axis * dv_o.min(6.0) * 0.6 });
                    out.queue.push(Interaction::Push { id: o.id, dv: -n * dv_o.min(12.0) });
                }
            }
        }
    }
}

fn player_contacts(c: &mut Construct, env: &Env<'_>, pre_vel: Vec3) {
    let center = c.center();
    let b = c.bounds();
    for p in env.ctx.players {
        if !p.alive || p.seat.is_some_and(|s| s == c.id) {
            continue;
        }
        let hull = crate::geom::inflate(&p.hull, 0.15);
        if !b.intersects(&hull) {
            continue;
        }
        // Closest point of my box to the hull centre, tested against the (inflated) hull.
        let hc = hull.center();
        let l = c.rot.inverse() * (hc - center);
        let q = center + c.rot * l.clamp(-c.half, c.half);
        let qc = q.clamp(hull.min, hull.max);
        if (qc - q).length_squared() > 1e-6 {
            continue;
        }
        let n = (center - hc).normalize_or(Vec3::Y);
        let closing = (-(pre_vel - p.vel).dot(n)).max(0.0);
        c.body.contacts.push(Contact { with: ContactWith::Player(p.id), point: q, normal: n, speed: closing, vel: pre_vel });
    }
}

/// Top surface height of the terrain under `p` (search ±`range`).
pub fn ground_below(v: &VoxelWorld, p: Vec3, range: f32) -> Option<f32> {
    ground_top(v, p.x, p.z, p.y + range, p.y - range)
}

/// Is there void (no ground within `depth`) under `p`?
pub fn void_under(v: &VoxelWorld, p: Vec3, depth: f32) -> bool {
    ground_top(v, p.x, p.z, p.y + 2.0, p.y - depth).is_none()
}

/// Voxel coordinate helper.
#[inline]
pub fn cell(p: Vec3) -> IVec3 {
    world_to_voxel(p)
}
