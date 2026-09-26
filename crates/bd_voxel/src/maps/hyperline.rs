//! **Hyperline** — an endless high-speed train chase (DESIGN.md "Maps"; feature `hyperline`).
//!
//! "The train stands still, the world moves." The map is one sleek white train on a glowing orange
//! guideway: a locomotive and seven cars in a line along **+X** (the nose at +X), 233 m long. The
//! voxels never move; the client scrolls an endless vertical city shaft, ring gates and speed
//! streaks past it (`bd_game::present::hyperline`), and the [`Slipstream`] wind makes the speed
//! physical: nothing on the roof, a gentle rearward drag in the air above it, and a strong
//! rearward + outward sweep off the sides / behind the tail that turns knock-offs into KOs.
//!
//! ```text
//!   tail ◄──────────────────────────────── 233 m ─────────────────────────────────► nose (+X)
//!  ┌─tail─┐ ┌─car6─┐ ┌─car5─┐ ┌─car4─┐ ┌─car3─┐ ┌─car2─┐ ┌─car1─┐ ┌───loco───╲
//!  └──────┘═└──────┘ └──────┘═└──────┘ └──────┘═└──────┘ └──────┘ └──────────╱   ═ gangway gap
//!  ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀ glowing guideway ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
//! ```
//!
//! - **Cross-section** (every car): 9 m wide, a 6 m flat walkable roof at 13 m with an orange centre
//!   stripe, and curved (50°) shoulders down to the sides — too steep to stand on; each shoulder
//!   carries a smooth hard-light surf line ([`ArenaInfo::surf_lines`]). Hollow interiors (8–12 m:
//!   4 m clear) with benches, grab poles and a light strip; two open side doors per side (orange
//!   glowing jambs — swing in!) and breakable glass windows between indestructible frame posts.
//! - **Gaps** of 3 m between cars; three of them carry an enclosed gangway (1.5 m below the roofs:
//!   a safe landing and a tunnel joining two interiors), the rest drop past a thin coupler to the
//!   guideway and the void.
//! - **Frame** (underframe, posts, ribs, nose, keel, guideway, door jambs) is indestructible; roof
//!   panels, walls, windows, benches break — punch holes into the cars.
//! - **Tether anchors**: antenna masts at every car end, pantographs on the loco and car 4, the tail
//!   fin, the grab poles inside — and anything else, tethers hook terrain.
//! - **KOs**: off the sides / behind the tail → the slipstream sweeps you into the side / rear blast
//!   zones or under the train into the void ([`VOID_Y`]); launched up → star KO. An attached tether
//!   still saves you (reel back in against the wind: it is scaled down on a rope).
//! - **Spawns**: eight on the roof centre line (one per unit) + two inside cars 2 and 5.

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::{ArenaInfo, SurfLine};
use crate::city::{City, vc};
use crate::material::mat;
use crate::noise::Rng;
use crate::slipstream::Slipstream;
use crate::world::VoxelWorld;

/// World dimensions in voxels (256 × 32 × 32 m). The side blast zones sit `blast_margin_side` past
/// ±16 m, the rear one past −128 m (the tail is at −116.5 m).
pub const HYPERLINE_SIZE_VOXELS: IVec3 = IVec3::new(512, 64, 64);
/// Walkable roof surface (m).
pub const ROOF_Y: f32 = 13.0;
/// Interior floor surface (m).
pub const FLOOR_Y: f32 = 8.0;
/// Interior ceiling (m, flat part).
pub const CEIL_Y: f32 = 12.0;
/// Underframe bottom (m): the car bodies span `FRAME_Y .. ROOF_Y`.
pub const FRAME_Y: f32 = 7.0;
/// Guideway top (m).
pub const RAIL_Y: f32 = 5.0;
/// Car half width (m) and the flat roof's half width.
pub const HALF_W: f32 = 4.5;
pub const FLAT_HW: f32 = 3.0;
/// Shoulder slope (from horizontal) and its tangent.
pub const SHOULDER_DEG: f32 = 50.0;
const TAN_SHOULDER: f32 = 1.191_753_6;
/// Car / locomotive length and the gap between units (m).
pub const CAR_LEN: f32 = 26.0;
pub const LOCO_LEN: f32 = 30.0;
pub const GAP: f32 = 3.0;
/// Units: the loco + 7 cars.
pub const UNITS: usize = 8;
/// Nose / tail x (m).
pub const NOSE_X: f32 = 116.5;
pub const TAIL_X: f32 = -116.5;
/// Falling below this is the void KO (17 m under the roofs).
pub const VOID_Y: f32 = -4.0;
/// The loco's raked nose (m of length) and the tail car's ramp.
pub const NOSE_RAKE: f32 = 10.0;
pub const TAIL_RAKE: f32 = 6.0;
/// Roof hatch slot: length along the car (m) and its z range (the +z side of the centre stripe).
pub const HATCH_LEN: f32 = 5.5;
pub const HATCH_Z: (f32, f32) = (1.25, 2.75);

/// Slipstream tuning (see [`Slipstream`]): gentle drag over the roofs, a strong sweep outside.
pub const SLIP_DRAG: f32 = 1.5;
/// Calm boundary layer over the roofs (m): no drag below `ROOF_Y + SLIP_CALM` inside the footprint.
pub const SLIP_CALM: f32 = 2.5;
pub const SLIP_SWEEP_BACK: f32 = 28.0;
pub const SLIP_SWEEP_OUT: f32 = 14.0;
pub const SLIP_RAMP: f32 = 3.0;
pub const SLIP_TERMINAL_BACK: f32 = 60.0;
pub const SLIP_TERMINAL_OUT: f32 = 30.0;
pub const SLIP_TETHER_SCALE: f32 = 0.35;

/// What a unit of the train is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Loco,
    Car,
    /// The last car: a raked tail ramp and the tail fin.
    Tail,
}

/// One unit of the train (index 0 = the loco at the front, 7 = the tail car).
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub index: usize,
    pub kind: UnitKind,
    /// Rear / front end x (m).
    pub x0: f32,
    pub x1: f32,
    /// Side door centres (local x, m from the rear end); the same on both sides.
    pub doors: Vec<f32>,
    /// Gangway to the unit behind (at local x 0) / in front (at `len`).
    pub gang_rear: bool,
    pub gang_front: bool,
    /// Roof pod (local x centre, z centre) — low cover, never on the centre line.
    pub pod: Option<(f32, f32)>,
    /// Pantograph at local x.
    pub pantograph: Option<f32>,
    /// Roof hatch + interior stairs (side +z): `Some(true)` at the rear end, `Some(false)` at the front.
    pub hatch_rear: Option<bool>,
}

impl Unit {
    pub fn len(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn is_empty(&self) -> bool {
        self.len() <= 0.0
    }
    pub fn name(&self) -> &'static str {
        const NAMES: [&str; UNITS] = ["loco", "car_1", "car_2", "car_3", "car_4", "car_5", "car_6", "tail"];
        NAMES[self.index.min(UNITS - 1)]
    }
    /// Centre x (m).
    pub fn cx(&self) -> f32 {
        (self.x0 + self.x1) * 0.5
    }
    /// Roof height at local x (the loco's nose and the tail ramp are raked; walkable slopes).
    pub fn roof_h(&self, lx: f32) -> f32 {
        let len = self.len();
        match self.kind {
            UnitKind::Loco if lx > len - NOSE_RAKE => ROOF_Y - (lx - (len - NOSE_RAKE)) * 0.35,
            UnitKind::Tail if lx < TAIL_RAKE => ROOF_Y - 2.0 + lx / TAIL_RAKE * 2.0,
            _ => ROOF_Y,
        }
    }
    /// Half width at local x (the nose tapers, the tail slightly).
    pub fn hw(&self, lx: f32) -> f32 {
        let len = self.len();
        match self.kind {
            UnitKind::Loco if lx > len - 8.0 => HALF_W - (lx - (len - 8.0)) / 8.0 * 1.5,
            UnitKind::Tail if lx < 3.0 => HALF_W - (3.0 - lx) / 3.0 * 0.75,
            _ => HALF_W,
        }
    }
    /// Outer body top at |z| = `az`, local x `lx`: the flat roof, then the 50° shoulder.
    pub fn top(&self, az: f32, lx: f32) -> f32 {
        let flat = self.hw(lx) - (HALF_W - FLAT_HW);
        self.roof_h(lx) - (az - flat).max(0.0) * TAN_SHOULDER
    }
    /// The level (unraked) stretch of roof, in world x.
    pub fn level_roof(&self) -> (f32, f32) {
        match self.kind {
            UnitKind::Loco => (self.x0, self.x1 - NOSE_RAKE),
            UnitKind::Tail => (self.x0 + TAIL_RAKE, self.x1),
            UnitKind::Car => (self.x0, self.x1),
        }
    }
    /// Local x range of the hollow interior.
    fn interior_x(&self) -> (f32, f32) {
        let len = self.len();
        match self.kind {
            UnitKind::Loco => (0.5, len - 6.0),
            _ => (0.5, len - 0.5),
        }
    }
    /// The roof hatch slot (local x range) and whether the stairs climb toward the rear.
    pub fn hatch(&self) -> Option<(f32, f32, bool)> {
        let rear = self.hatch_rear?;
        let len = self.len();
        Some(if rear { (1.0, HATCH_LEN + 1.0, true) } else { (len - 1.0 - HATCH_LEN, len - 1.0, false) })
    }
    /// Is the voxel centred at (az, lx, y) inside the hollow interior?
    fn interior(&self, az: f32, lx: f32, y: f32) -> bool {
        let (a, b) = self.interior_x();
        lx - 0.25 >= a && lx + 0.25 <= b && az + 0.25 <= self.hw(lx) - 0.5 && y - 0.25 >= FLOOR_Y && y + 0.25 <= CEIL_Y.min(self.top(az + 0.25, lx) - 1.0)
    }
}

/// The train's units, front (loco) to back (tail). `seed` varies gangways, pods and pantograph.
pub fn units(seed: u64) -> Vec<Unit> {
    let mut rng = Rng::new(seed ^ 0x6879_7065_726c_696e);
    // Gangways join 3 of the 7 gaps: either the odd or the even pattern, never two in a row.
    let gang_gaps: [bool; UNITS - 1] = if rng.next_u32() & 1 == 0 { [false, true, false, true, false, true, false] } else { [true, false, true, false, false, true, false] };
    let mut out = Vec::with_capacity(UNITS);
    let mut x1 = NOSE_X;
    for i in 0..UNITS {
        let (kind, len) = match i {
            0 => (UnitKind::Loco, LOCO_LEN),
            i if i == UNITS - 1 => (UnitKind::Tail, CAR_LEN),
            _ => (UnitKind::Car, CAR_LEN),
        };
        let x0 = x1 - len;
        let doors = match kind {
            UnitKind::Loco => vec![6.5, 15.5],
            UnitKind::Tail => vec![10.5, 19.5],
            UnitKind::Car => vec![6.5, 19.5],
        };
        let side = if rng.next_u32() & 1 == 0 { 1.5 } else { -1.5 };
        let pod = match kind {
            UnitKind::Loco => None,
            UnitKind::Tail => Some((10.0, side)),
            UnitKind::Car => Some((if i % 2 == 0 { 9.0 } else { 17.5 }, side)),
        };
        let pantograph = match i {
            0 => Some(3.5),
            4 => Some(20.5),
            _ => None,
        };
        out.push(Unit {
            index: i,
            kind,
            x0,
            x1,
            doors,
            gang_front: i > 0 && gang_gaps[i - 1],
            gang_rear: i < UNITS - 1 && gang_gaps[i],
            pod,
            pantograph,
            hatch_rear: match kind {
                UnitKind::Loco => None,
                UnitKind::Tail => Some(false),
                UnitKind::Car => Some(i % 2 == 1),
            },
        });
        x1 = x0 - GAP;
    }
    out
}

/// Material of the unit's voxel centred at `c` (metres), or `AIR`.
fn unit_mat(u: &Unit, c: Vec3) -> Material {
    let lx = c.x - u.x0;
    let len = u.len();
    if lx < 0.0 || lx > len {
        return AIR;
    }
    let az = c.z.abs();
    // Under the body: keel and bogies.
    if c.y < FRAME_Y {
        let bogie = (2.0..5.0).contains(&lx) || (len - 5.0..len - 2.0).contains(&lx);
        if bogie && c.y >= 5.5 && az < 3.0 {
            return if az >= 2.5 && (6.0..6.5).contains(&c.y) { mat::EMBER_FRAME } else { mat::CYBER_FRAME };
        }
        if c.y >= RAIL_Y && az < 0.75 && (1.0..=len - 1.0).contains(&lx) {
            return mat::CYBER_FRAME;
        }
        return AIR;
    }
    let hw = u.hw(lx);
    if az + 0.25 > hw {
        return AIR;
    }
    let top = u.top(az + 0.25, lx);
    if c.y + 0.25 > top + 1e-4 {
        return AIR;
    }
    let side = az + 0.25 > hw - 0.5;
    // Underframe (indestructible) with the orange underglow along its outer edges.
    if c.y < 7.5 {
        return if side { mat::EMBER_FRAME } else { mat::HYPER_FRAME };
    }
    if u.interior(az, lx, c.y) {
        return AIR;
    }
    let end_rear = lx - 0.25 < 0.5;
    let end_front = lx + 0.25 > len - 0.5;
    // Roof hatch slot over the interior stairs.
    if let Some((a, b, _)) = u.hatch()
        && c.y >= 11.5
        && lx - 0.25 >= a
        && lx + 0.25 <= b
        && c.z - 0.25 >= HATCH_Z.0
        && c.z + 0.25 <= HATCH_Z.1
    {
        return AIR;
    }
    // Headlights across the nose tip.
    if u.kind == UnitKind::Loco && end_front && az < 1.5 && (8.0..8.5).contains(&c.y) {
        return mat::STATION_LIGHT;
    }
    if c.y < FLOOR_Y {
        return if side || end_rear || end_front { mat::HYPER_FRAME } else { mat::CYBER_PLATE_LIGHT };
    }
    // Side doors: open doorways framed by glowing (indestructible) jambs and lintel.
    if side && c.y < 11.5 {
        for &d in &u.doors {
            let o = (lx - d).abs();
            if o < 1.25 && c.y < 11.0 {
                return AIR;
            }
            if o < 1.75 {
                return mat::EMBER_FRAME;
            }
        }
    }
    // Gangway openings in the end walls.
    let gang_end = (end_rear && u.gang_rear) || (end_front && u.gang_front);
    if gang_end && c.y < 11.5 {
        if az < 1.25 && c.y < 11.0 {
            return AIR;
        }
        if az < 1.75 {
            return mat::EMBER_FRAME;
        }
    }
    // Frame: posts every 4 m in the side walls, corner posts, roof ribs every 8 m.
    if side && lx.rem_euclid(4.0) < 0.5 {
        return mat::HYPER_FRAME;
    }
    if (end_rear || end_front) && az + 0.25 > hw - 1.0 {
        return mat::HYPER_FRAME;
    }
    let top_layer = c.y + 0.25 > top - 0.5;
    if c.y >= 11.5 && lx.rem_euclid(8.0) < 0.5 && !(u.kind == UnitKind::Loco && lx > len - NOSE_RAKE) {
        return mat::HYPER_FRAME;
    }
    // Windows (breakable).
    if side && (9.0..11.0).contains(&c.y) {
        return mat::GLASS_TEAL;
    }
    if (end_rear || end_front) && !gang_end && az < 1.5 && (9.5..11.0).contains(&c.y) {
        return mat::GLASS_TEAL;
    }
    // The loco's windshield on the nose slope.
    if u.kind == UnitKind::Loco && (len - 9.5..len - 5.0).contains(&lx) && c.y + 0.25 > top - 1.0 && az < u.hw(lx) - 1.75 {
        return mat::GLASS_BLUE;
    }
    // The orange line down the middle of the roof.
    if top_layer && az < 0.5 && u.roof_h(lx) == ROOF_Y {
        return mat::SEAM_ORANGE;
    }
    mat::HYPER_PANEL
}

/// Voxel index of metre coordinate `m`.
#[inline]
fn vi(m: f32) -> i32 {
    (m / 0.5).floor() as i32
}

fn build_unit(c: &mut City, u: &Unit) {
    let (x0, x1) = (vi(u.x0), vi(u.x1) - 1);
    let hz = vi(HALF_W) + 1;
    let (y0, y1) = (vi(RAIL_Y), vi(ROOF_Y + 0.5));
    for x in x0..=x1 {
        for z in -hz..hz {
            for y in y0..=y1 {
                let v = IVec3::new(x, y, z);
                let m = unit_mat(u, vc(v));
                if m != AIR {
                    c.w.set(v, m);
                }
            }
        }
    }
    let lx = |l: f32| u.x0 + l;
    // Interior: benches along the walls between the doors, grab poles, a light strip on the ceiling.
    let (ia, ib) = u.interior_x();
    let mut l = ia + 1.0;
    while l + 2.0 <= ib - 1.0 {
        let clear_of_doors = u.doors.iter().all(|&d| (l - d).abs() > 3.0 && (l + 2.0 - d).abs() > 3.0);
        if clear_of_doors {
            for s in [-1.0f32, 1.0] {
                let zi = s * (HALF_W - 0.5);
                let zo = s * (HALF_W - 1.5);
                c.fill(Vec3::new(lx(l), FLOOR_Y, zi.min(zo)), Vec3::new(lx(l + 2.0), FLOOR_Y + 0.5, zi.max(zo)), mat::HAZARD_ORANGE);
            }
        }
        l += 3.0;
    }
    let mut l = ia + 3.0;
    while l < ib - 1.0 {
        for s in [-1.0f32, 1.0] {
            let in_stairs = u.hatch().is_some_and(|(a, b, _)| s > 0.0 && l + 0.5 > a - 1.0 && l < b + 4.0);
            if in_stairs {
                continue;
            }
            let z = s * 1.75;
            c.fill(Vec3::new(lx(l), FLOOR_Y, z - 0.25), Vec3::new(lx(l) + 0.5, CEIL_Y, z + 0.25), mat::HYPER_FRAME);
        }
        l += 6.0;
    }
    // Stairs up to the roof hatch: ten 0.5 m risers, 0.8 m treads, the top step flush with the roof.
    if let Some((a, b, rear)) = u.hatch() {
        for k in 1..=10 {
            let d = (10 - k) as f32 * 0.8;
            let (s0, s1) = if rear { (a + d, a + d + 0.8) } else { (b - d - 0.8, b - d) };
            c.fill(Vec3::new(lx(s0), FLOOR_Y, HATCH_Z.0), Vec3::new(lx(s1), FLOOR_Y + 0.5 * k as f32, HATCH_Z.1), mat::CYBER_PLATE_LIGHT);
        }
    }
    // Ceiling light strip: the first solid voxel above the centre line of the interior.
    for x in vi(lx(ia)) + 1..vi(lx(ib)) - 1 {
        for z in [-1, 0] {
            let mut y = vi(FLOOR_Y) + 1;
            while y < vi(ROOF_Y) && c.w.get(IVec3::new(x, y, z)) == AIR {
                y += 1;
            }
            let v = IVec3::new(x, y, z);
            if c.w.get(v) == mat::HYPER_PANEL && y < vi(ROOF_Y) - 1 {
                c.w.set(v, mat::STATION_LIGHT);
            }
        }
    }
    // Roof pod: low cover (1 m), off the centre line.
    if let Some((pl, pz)) = u.pod {
        c.fill(Vec3::new(lx(pl - 1.5), ROOF_Y, pz - 0.75), Vec3::new(lx(pl + 1.5), ROOF_Y + 1.0, pz + 0.75), mat::HYPER_PANEL);
        c.fill(Vec3::new(lx(pl - 1.5), ROOF_Y + 0.5, pz - 0.75), Vec3::new(lx(pl + 1.5), ROOF_Y + 1.0, pz - 0.25), mat::MULLION);
    }
    // Antenna masts at both ends (indestructible stubs with a glowing tip) — tether anchors.
    let (la, lb) = (u.level_roof().0 - u.x0 + 1.0, u.level_roof().1 - u.x0 - 1.5);
    for (l, z) in [(la, -2.75f32), (lb, -2.75)] {
        let b = Vec3::new(lx(l), ROOF_Y, z);
        c.fill(b, b + Vec3::new(0.5, 2.5, 0.5), mat::HYPER_FRAME);
        c.fill(b + Vec3::new(0.0, 2.5, 0.0), b + Vec3::new(0.5, 3.0, 0.5), mat::STATION_LIGHT);
    }
    // Pantograph: two raked arms from the shoulders up to a crossbar 3 m over the roof.
    if let Some(p) = u.pantograph {
        for s in [-1.0f32, 1.0] {
            c.line(Vec3::new(lx(p) + 0.25, ROOF_Y + 0.25, s * 2.6), Vec3::new(lx(p) + 1.75, ROOF_Y + 3.25, s * 1.1), mat::HYPER_FRAME, true);
        }
        c.fill(Vec3::new(lx(p) + 1.5, ROOF_Y + 3.0, -2.25), Vec3::new(lx(p) + 2.0, ROOF_Y + 3.5, 2.25), mat::HYPER_FRAME);
        c.fill(Vec3::new(lx(p) + 1.5, ROOF_Y + 3.5, -2.25), Vec3::new(lx(p) + 2.0, ROOF_Y + 4.0, 2.25), mat::EMBER_FRAME);
    }
    // The tail fin: a swept stabiliser over the tail ramp.
    if u.kind == UnitKind::Tail {
        let mut l = 1.0;
        while l < 6.5 {
            let base = u.roof_h(l);
            let h = base + 0.75 + (6.5 - l) * 0.55;
            c.fill(Vec3::new(lx(l), base - 0.5, -0.25), Vec3::new(lx(l) + 0.5, h, 0.25), mat::HYPER_FRAME);
            c.fill(Vec3::new(lx(l), h, -0.25), Vec3::new(lx(l) + 0.5, h + 0.5, 0.25), mat::EMBER_FRAME);
            l += 0.5;
        }
    }
    // Collapse hints along the underframe.
    let mut l = 1.0;
    while l < u.len() {
        c.anchor(IVec3::new(vi(lx(l)), vi(7.25), 0));
        l += 8.0;
    }
}

/// The gap behind unit `u` (between `u.x0` and the next unit's `x1`): a gangway or a coupler.
fn build_gap(c: &mut City, u: &Unit) {
    let (a, b) = (u.x0 - GAP, u.x0);
    if u.gang_rear {
        c.fill(Vec3::new(a, FRAME_Y, -1.75), Vec3::new(b, FLOOR_Y, 1.75), mat::HYPER_FRAME);
        for s in [-1.0f32, 1.0] {
            let (z0, z1) = if s > 0.0 { (1.25, 1.75) } else { (-1.75, -1.25) };
            c.fill(Vec3::new(a, FLOOR_Y, z0), Vec3::new(b, 11.0, z1), mat::CYBER_TRIM);
            // Bellows ribs.
            let mut x = a + 0.5;
            while x < b - 0.25 {
                c.fill(Vec3::new(x, FLOOR_Y, z0), Vec3::new(x + 0.5, 11.0, z1), mat::CYBER_FRAME);
                x += 1.0;
            }
        }
        c.fill(Vec3::new(a, 11.0, -1.75), Vec3::new(b, 11.5, 1.75), mat::CYBER_TRIM);
    } else {
        c.fill(Vec3::new(a, FRAME_Y, -0.25), Vec3::new(b, 7.5, 0.25), mat::CYBER_FRAME);
    }
}

/// Feet position standing at (x, z) on the first solid below `from_y`, with a 3×3-voxel footprint
/// of ground and 3 m of head room; `None` otherwise.
pub fn stand_at(w: &VoxelWorld, x: f32, z: f32, from_y: f32) -> Option<Vec3> {
    let (cx, cz) = (vi(x), vi(z));
    let top = (w.voxel_min().y..=vi(from_y)).rev().find(|&y| w.get(IVec3::new(cx, y, cz)) != AIR)?;
    for dz in -1..=1 {
        for dx in -1..=1 {
            if w.get(IVec3::new(cx + dx, top, cz + dz)) == AIR {
                return None;
            }
            for dy in 1..=6 {
                if w.get(IVec3::new(cx + dx, top + dy, cz + dz)) != AIR {
                    return None;
                }
            }
        }
    }
    Some(Vec3::new((cx as f32 + 0.5) * 0.5, (top + 1) as f32 * 0.5, (cz as f32 + 0.5) * 0.5))
}

fn pick_spawns(w: &VoxelWorld, us: &[Unit]) -> Vec<Vec3> {
    let mut out = Vec::new();
    // Roof centre line, one per unit, clear of pods / masts / pantographs (searched along x).
    for u in us {
        let (a, b) = u.level_roof();
        let mid = (a + b) * 0.5;
        let found = [0.0f32, 2.0, -2.0, 4.0, -4.0, 6.0, -6.0].iter().find_map(|&o| {
            let p = stand_at(w, mid + o + 0.25, 0.25, ROOF_Y + 6.0)?;
            (p.y == ROOF_Y).then_some(p)
        });
        if let Some(p) = found {
            out.push(p);
        }
    }
    // Two inside (cars 2 and 5), between the doors.
    for i in [2usize, 5] {
        if let Some(u) = us.get(i)
            && let Some(p) = stand_at(w, u.x0 + 10.25, 0.25, CEIL_Y - 0.5)
            && p.y == FLOOR_Y
        {
            out.push(p);
        }
    }
    out
}

/// The slipstream for the train's footprint.
pub fn slipstream() -> Slipstream {
    Slipstream {
        min: Vec2::new(TAIL_X, -HALF_W),
        max: Vec2::new(NOSE_X, HALF_W),
        // The boundary layer: ordinary hops over the roof (apex ≈ 1.8 m) never feel the drag;
        // double jumps, launches and swings above it drift back.
        roof_y: ROOF_Y + SLIP_CALM,
        drag: SLIP_DRAG,
        sweep_back: SLIP_SWEEP_BACK,
        sweep_out: SLIP_SWEEP_OUT,
        ramp: SLIP_RAMP,
        terminal_back: SLIP_TERMINAL_BACK,
        terminal_out: SLIP_TERMINAL_OUT,
        tether_scale: SLIP_TETHER_SCALE,
    }
}

/// Smooth hard-light surf lines along both roof shoulders of every unit's level roof.
pub fn surf_lines(us: &[Unit]) -> Vec<SurfLine> {
    let w = 1.5 / 0.642_787_6 + 0.1; // shoulder run / cos 50° (+ a lip past the side wall)
    let mut out = Vec::new();
    for u in us {
        let (a, b) = u.level_roof();
        for s in [-1.0f32, 1.0] {
            out.push(SurfLine { a: Vec3::new(a + 0.5, ROOF_Y + 0.02, s * FLAT_HW), b: Vec3::new(b - 0.5, ROOF_Y + 0.02, s * FLAT_HW), face_deg: SHOULDER_DEG, width: w });
        }
    }
    out
}

/// Build Hyperline. Deterministic for a given seed.
pub fn generate_hyperline(seed: u64) -> (VoxelWorld, MapInfo) {
    let us = units(seed);
    let mut c = City::new(VoxelWorld::centered(HYPERLINE_SIZE_VOXELS), seed ^ 0x7472_6169_6e21);
    // The guideway under the whole train (dark body, glowing top).
    c.fill(Vec3::new(TAIL_X, 3.5, -1.0), Vec3::new(NOSE_X, 4.5, 1.0), mat::CYBER_FRAME);
    c.fill(Vec3::new(TAIL_X, 4.5, -1.0), Vec3::new(NOSE_X, RAIL_Y, 1.0), mat::EMBER_FRAME);
    let mut x = TAIL_X + 4.0;
    while x < NOSE_X {
        c.anchor(IVec3::new(vi(x), vi(4.0), 0));
        x += 16.0;
    }
    for u in &us {
        build_unit(&mut c, u);
        if u.index < UNITS - 1 {
            build_gap(&mut c, u);
        }
    }
    let spawns = pick_spawns(&c.w, &us);
    let (world, pruned) = c.finish_counted();

    let wb = world.bounds();
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb { min: Vec3::new(wb.min.x - 24.0, VOID_Y, wb.min.z - 24.0), max: Vec3::new(wb.max.x + 24.0, wb.max.y + 40.0, wb.max.z + 24.0) },
        void_y: VOID_Y,
        center: Vec3::new(0.0, ROOF_Y, 0.0),
        // High, slightly ahead: the roofs gleam, the shaft glows below.
        sun_dir: Vec3::new(0.35, 0.86, 0.36).normalize(),
        sky_zenith: [0.20, 0.52, 0.82],
        sky_horizon: [0.60, 0.86, 0.96],
        fog_color: [0.50, 0.80, 0.93],
        ambient: 0.72,
        gravity: None,
        surf_lines: surf_lines(&us),
        water_sheets: Vec::new(),
        water_walk: false,
        interactables: Default::default(),
        slipstream: Some(slipstream()),
        mechanics: Default::default(),
    };
    let districts = us
        .iter()
        .map(|u| District { name: u.name(), center: Vec3::new(u.cx(), ROOF_Y, 0.0), area: [u.x0, -HALF_W, u.x1, HALF_W] })
        .collect();
    let routes = routes(&us);
    let info = MapInfo {
        name: "Hyperline",
        arena,
        // No sea: the shaft's depths are drawn by the client.
        water_y: -1000.0,
        water_color: [0.30, 0.70, 0.90],
        water_deep: [0.08, 0.30, 0.52],
        sun_color: [1.0, 0.96, 0.88],
        sky_mid: [0.40, 0.72, 0.92],
        fog_start: 45.0,
        fog_end: 420.0,
        neon_boost: 0.8,
        districts,
        routes,
        pruned_voxels: pruned,
    };
    (world, info)
}

fn routes(us: &[Unit]) -> Vec<Route> {
    let mut out = Vec::new();
    // Roof run: the tail ramp to the loco's nose, along the centre line, over every gap.
    let mut wp = Vec::new();
    for u in us.iter().rev() {
        let (a, b) = u.level_roof();
        wp.push(Vec3::new(a + 1.0, ROOF_Y, 0.25));
        wp.push(Vec3::new(b - 1.0, ROOF_Y, 0.25));
    }
    out.push(Route { name: "roof_run", from: "tail", to: "loco", kind: RouteKind::Ground, waypoints: wp, anchors: vec![] });
    // Gap jumps: car 3 → car 2 (a plain gap or a gangway, both jumpable at a run).
    if let (Some(a), Some(b)) = (us.get(3), us.get(2)) {
        out.push(Route {
            name: "gap_jump",
            from: a.name(),
            to: b.name(),
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(a.x1 - 8.0, ROOF_Y, 0.25), Vec3::new(b.x0 + 4.0, ROOF_Y, 0.25)],
            anchors: vec![],
        });
    }
    // Swing in: from the roof of car 4, off the side, tether the roof edge and swing through a door.
    if let Some(u) = us.get(4) {
        let d = u.x0 + u.doors[0];
        out.push(Route {
            name: "swing_in",
            from: u.name(),
            to: u.name(),
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(d + 6.0, ROOF_Y, 2.0), Vec3::new(d, FLOOR_Y, HALF_W - 1.5)],
            anchors: vec![Vec3::new(d - 1.0, ROOF_Y - 0.25, FLAT_HW - 0.5)],
        });
    }
    // Knocked off: blown off the side of car 3, tether its roof and reel back aboard.
    if let Some(u) = us.get(3) {
        out.push(Route {
            name: "tether_back",
            from: u.name(),
            to: u.name(),
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(u.cx(), ROOF_Y, 3.0), Vec3::new(u.cx() - 4.0, ROOF_Y, 0.25)],
            anchors: vec![Vec3::new(u.cx(), ROOF_Y - 0.25, 0.0)],
        });
    }
    out
}
