//! Ambience that makes each map a *place*: layered beds, positional loop emitters (neon hum, AC
//! units, surf, insects) placed from the voxel map itself, and sparse positional one-shots (birds,
//! gulls, waves, palm rustle, traffic, hull creaks) scattered in space around the listener.
//!
//! The world reacts: a big explosion scares the fauna (birds/insects go quiet for ~10 s, then creep
//! back); combat thins everything through the HDR window; altitude and speed drive the wind bed.
//! Data lives in `assets/audio/maps.ron` ([`MapAudio`]).

use std::collections::HashMap;

use bd_core::voxel::VoxelQuery;
use bd_voxel::{DebrisClass, mat, material_def};
use glam::{IVec3, Vec3};
use serde::Deserialize;

use crate::acoustics::{SpacePreset, occlusion};
use crate::dsp::Rng;
use crate::engine::{AudioEngine, PlayOpts, VoiceHandle};

/// Where an emitter can live (derived from the voxel map).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
pub enum Zone {
    /// Tree canopies.
    Leaves,
    Grass,
    /// Shoreline: land meeting the water line.
    WaterEdge,
    /// Open water surface.
    Water,
    /// Roads.
    Street,
    /// Self-lit (neon) voxels with air beside them.
    Neon,
    /// Building walls (AC units, pipes).
    Wall,
    /// Island edges over the void (wind gusts).
    CliffEdge,
    /// Anywhere around the listener, in the air (distant booms, birds overhead).
    Sky,
    /// Any walkable surface.
    Surface,
}

/// A linear dB curve over an input range (clamped).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Curve {
    pub x0: f32,
    pub x1: f32,
    pub db0: f32,
    pub db1: f32,
}

impl Curve {
    pub fn eval(&self, x: f32) -> f32 {
        let t = if (self.x1 - self.x0).abs() < 1e-6 { 1.0 } else { ((x - self.x0) / (self.x1 - self.x0)).clamp(0.0, 1.0) };
        self.db0 + (self.db1 - self.db0) * t
    }
}

/// A non-positional bed (stereo loop on the Ambience bus).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct BedDef {
    pub sound: String,
    pub gain_db: f32,
    /// dB by listener altitude (m).
    pub altitude: Option<Curve>,
    /// dB by listener speed (m/s).
    pub speed: Option<Curve>,
    /// dB by distance to the nearest emitter of `near_zone` (e.g. ocean louder near the beach).
    pub near: Option<Curve>,
    pub near_zone: Option<Zone>,
    /// dB by listener enclosure (0 open … 1 sealed room, from the acoustics estimate): outdoor
    /// beds fade inside, room tone fades in.
    pub enclosure: Option<Curve>,
    /// Fauna: silenced by explosions, creeps back.
    pub fauna: bool,
}

impl Default for BedDef {
    fn default() -> Self {
        Self { sound: String::new(), gain_db: 0.0, altitude: None, speed: None, near: None, near_zone: None, enclosure: None, fauna: false }
    }
}

/// Positional loops kept alive at the nearest emitters of a zone.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct LoopDef {
    pub sound: String,
    pub zone: Zone,
    pub max_active: u8,
    /// Activation radius (m).
    pub radius: f32,
    pub gain_db: f32,
    pub fauna: bool,
    /// Height above the emitter point (m).
    pub lift: f32,
}

impl Default for LoopDef {
    fn default() -> Self {
        Self { sound: String::new(), zone: Zone::Surface, max_active: 2, radius: 40.0, gain_db: 0.0, fauna: false, lift: 1.0 }
    }
}

/// Sparse positional one-shots.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct SparkleDef {
    pub sound: String,
    pub zone: Zone,
    /// Average plays per minute.
    pub per_min: f32,
    pub min_dist: f32,
    pub max_dist: f32,
    /// Random gain range (dB).
    pub gain_db: (f32, f32),
    pub fauna: bool,
    /// Height above the emitter point (m), random in range.
    pub lift: (f32, f32),
    /// dB by listener altitude.
    pub altitude: Option<Curve>,
}

impl Default for SparkleDef {
    fn default() -> Self {
        Self {
            sound: String::new(),
            zone: Zone::Surface,
            per_min: 6.0,
            min_dist: 8.0,
            max_dist: 60.0,
            gain_db: (-3.0, 0.0),
            fauna: false,
            lift: (0.5, 2.0),
            altitude: None,
        }
    }
}

/// Everything audio knows about one map.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct MapAudio {
    pub space: SpacePreset,
    pub beds: Vec<BedDef>,
    pub loops: Vec<LoopDef>,
    pub sparkles: Vec<SparkleDef>,
    /// Fauna scare: seconds of silence after a big blast, then recovery time.
    pub scare_hold: f32,
    pub scare_recover: f32,
}

/// `assets/audio/maps.ron`: map name → audio.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct MapAudioTable(pub HashMap<String, MapAudio>);

impl MapAudioTable {
    pub fn from_ron(t: &str) -> Result<Self, String> {
        ron::from_str(t).map_err(|e| e.to_string())
    }
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let t = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_ron(&t)
    }
    pub fn get(&self, map: &str) -> MapAudio {
        self.0.get(map).cloned().unwrap_or_default()
    }
}

/// Emitter candidate points found in the voxel map.
#[derive(Clone, Debug, Default)]
pub struct EmitterMap {
    pub zones: HashMap<Zone, Vec<Vec3>>,
}

impl EmitterMap {
    pub fn points(&self, z: Zone) -> &[Vec3] {
        self.zones.get(&z).map_or(&[], |v| v.as_slice())
    }
    pub fn count(&self) -> usize {
        self.zones.values().map(|v| v.len()).sum()
    }
    /// Distance to the nearest point of a zone (∞ if none).
    pub fn nearest(&self, z: Zone, p: Vec3) -> f32 {
        self.points(z).iter().map(|q| q.distance_squared(p)).fold(f32::INFINITY, f32::min).sqrt()
    }

    /// Scan a map once at load: columns every `step` m, top-down, classifying surfaces, water
    /// edges, walls, neon and cliff edges. `min`/`max` are the playable bounds (m).
    pub fn scan<W: VoxelQuery + ?Sized>(world: &W, min: Vec3, max: Vec3, water_y: Option<f32>, step: f32) -> EmitterMap {
        let vs = bd_core::VOXEL_SIZE;
        let step_v = ((step / vs).round() as i32).max(1);
        let (x0, x1) = ((min.x / vs) as i32, (max.x / vs) as i32);
        let (z0, z1) = ((min.z / vs) as i32, (max.z / vs) as i32);
        let (y0, y1) = ((min.y / vs).floor() as i32, (max.y / vs) as i32);
        let nx = ((x1 - x0) / step_v + 1).max(1) as usize;
        let nz = ((z1 - z0) / step_v + 1).max(1) as usize;
        // Surface height per column (None = nothing: void or open water).
        let mut top: Vec<Option<(i32, u8)>> = vec![None; nx * nz];
        let mut zones: HashMap<Zone, Vec<Vec3>> = HashMap::new();
        let mut push = |z: Zone, p: Vec3| zones.entry(z).or_default().push(p);
        let water_v = water_y.map(|w| (w / vs).floor() as i32);
        for iz in 0..nz {
            for ix in 0..nx {
                let (vx, vz) = (x0 + ix as i32 * step_v, z0 + iz as i32 * step_v);
                let mut surface = None;
                let mut neon_done = false;
                let mut y = y1;
                while y >= y0 {
                    let m = world.material(IVec3::new(vx, y, vz));
                    if m != 0 {
                        if surface.is_none() {
                            if water_v.is_some_and(|w| y < w) {
                                break; // under water: open water column
                            }
                            surface = Some((y, m));
                        }
                        let d = material_def(m);
                        if !neon_done && d.emissive >= 0.5 && y > surface.map_or(y, |s| s.0) - 60 {
                            // Self-lit with air beside it: a sign / strip.
                            let side = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
                                .iter()
                                .find(|o| world.material(IVec3::new(vx, y, vz) + **o) == 0)
                                .copied();
                            if let Some(o) = side {
                                push(Zone::Neon, (IVec3::new(vx, y, vz) + o).as_vec3() * vs + Vec3::splat(vs * 0.5));
                                neon_done = true;
                            }
                        }
                        if surface.is_some() && neon_done {
                            break;
                        }
                        if surface.is_some_and(|s| s.0 - y > 60) {
                            break;
                        }
                    }
                    y -= 1;
                }
                top[iz * nx + ix] = surface;
                if let Some((sy, m)) = surface {
                    let p = Vec3::new((vx as f32 + 0.5) * vs, (sy + 1) as f32 * vs, (vz as f32 + 0.5) * vs);
                    let d = material_def(m);
                    let z = match d.debris {
                        DebrisClass::Leaves => Zone::Leaves,
                        DebrisClass::Grass => Zone::Grass,
                        _ if matches!(m, mat::ASPHALT | mat::PAINT_WHITE | mat::PAINT_YELLOW) => Zone::Street,
                        _ => Zone::Surface,
                    };
                    push(z, p);
                    if z != Zone::Surface {
                        push(Zone::Surface, p);
                    }
                } else if let Some(w) = water_y {
                    push(Zone::Water, Vec3::new((vx as f32 + 0.5) * vs, w, (vz as f32 + 0.5) * vs));
                }
            }
        }
        // Neighbour passes: water edges, cliff edges, walls.
        for iz in 0..nz {
            for ix in 0..nx {
                let Some((sy, _)) = top[iz * nx + ix] else { continue };
                let p = Vec3::new(
                    ((x0 + ix as i32 * step_v) as f32 + 0.5) * vs,
                    (sy + 1) as f32 * vs,
                    ((z0 + iz as i32 * step_v) as f32 + 0.5) * vs,
                );
                let mut water_n = false;
                let mut void_n = false;
                let mut tall_drop = 0i32;
                for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let (jx, jz) = (ix as i32 + dx, iz as i32 + dz);
                    if jx < 0 || jz < 0 || jx >= nx as i32 || jz >= nz as i32 {
                        continue;
                    }
                    match top[jz as usize * nx + jx as usize] {
                        None if water_y.is_some() => water_n = true,
                        None => void_n = true,
                        Some((ny, _)) => tall_drop = tall_drop.max(sy - ny),
                    }
                }
                if water_n
                    && let Some(w) = water_y
                    && p.y - w < 4.0
                {
                    push(Zone::WaterEdge, Vec3::new(p.x, w + 0.3, p.z));
                }
                if void_n {
                    push(Zone::CliffEdge, p);
                }
                // A column ≥ 8 m above a neighbour: a building wall (AC units live 3 m up).
                if tall_drop as f32 * vs >= 8.0 {
                    let h = (sy - tall_drop) as f32 * vs + 3.0;
                    push(Zone::Wall, Vec3::new(p.x, h, p.z));
                }
            }
        }
        EmitterMap { zones }
    }
}

struct ActiveLoop {
    def: usize,
    point: Vec3,
    handle: VoiceHandle,
    alive: bool,
}

/// Runs a map's ambience. Game-thread only; cheap per frame (loops re-evaluated at 4 Hz).
pub struct AmbienceDirector {
    pub map: MapAudio,
    /// Listener enclosure 0..1 (set from [`crate::acoustics::ListenerSpace`] each frame).
    pub enclosure: f32,
    pub emitters: EmitterMap,
    beds: Vec<Option<VoiceHandle>>,
    loops: Vec<ActiveLoop>,
    sparkle_clock: Vec<f32>,
    rng: Rng,
    /// Fauna suppression 0..1 and its timer.
    scare: f32,
    scare_t: f32,
    loop_timer: f32,
    started: bool,
    near_cache: Vec<f32>,
}

impl AmbienceDirector {
    pub fn new(map: MapAudio, emitters: EmitterMap, seed: u64) -> Self {
        let nb = map.beds.len();
        let ns = map.sparkles.len();
        Self {
            map,
            enclosure: 0.0,
            emitters,
            beds: vec![None; nb],
            loops: Vec::with_capacity(32),
            sparkle_clock: vec![0.0; ns],
            rng: Rng::new(seed ^ 0xa3b1),
            scare: 0.0,
            scare_t: 0.0,
            loop_timer: 0.0,
            started: false,
            near_cache: vec![f32::INFINITY; nb],
        }
    }

    /// Current fauna suppression (0 = birds singing, 1 = silent).
    pub fn scared(&self) -> f32 {
        self.scare
    }

    /// Something loud happened at `pos` (explosion radius / impact strength ≈ `strength` 0..1+).
    pub fn scare(&mut self, pos: Vec3, strength: f32, listener: Vec3) {
        let d = pos.distance(listener);
        let s = (strength * (1.0 - d / 150.0)).clamp(0.0, 1.0);
        if s > self.scare * 0.8 {
            self.scare = self.scare.max(s);
            self.scare_t = 0.0;
        }
    }

    /// Stop everything (map change / menu).
    pub fn stop(&mut self, engine: &mut AudioEngine) {
        for h in self.beds.iter_mut().filter_map(Option::take) {
            engine.stop(h, 1.5);
        }
        for l in self.loops.drain(..) {
            engine.stop(l.handle, 1.0);
        }
        self.started = false;
    }

    /// Per frame. `world` (optional) refreshes loop occlusion.
    pub fn update<W: VoxelQuery + ?Sized>(&mut self, dt: f32, ear: Vec3, vel: Vec3, engine: &mut AudioEngine, world: Option<&W>) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
        // Fauna recovery: hold, then creep back.
        self.scare_t += dt;
        let hold = if self.map.scare_hold > 0.0 { self.map.scare_hold } else { 3.0 };
        let rec = if self.map.scare_recover > 0.0 { self.map.scare_recover } else { 8.0 };
        if self.scare_t > hold {
            self.scare = (self.scare - dt / rec).max(0.0);
        }
        let fauna_db = -36.0 * self.scare.powf(0.5);
        let speed = vel.length();

        // Beds.
        let refresh_near = !self.started || self.loop_timer <= 0.0;
        for (i, b) in self.map.beds.iter().enumerate() {
            let mut db = b.gain_db;
            if let Some(c) = b.altitude {
                db += c.eval(ear.y);
            }
            if let Some(c) = b.speed {
                db += c.eval(speed);
            }
            if let (Some(c), Some(z)) = (b.near, b.near_zone) {
                if refresh_near {
                    self.near_cache[i] = self.emitters.nearest(z, ear);
                }
                db += c.eval(self.near_cache[i]);
            }
            if let Some(c) = b.enclosure {
                db += c.eval(self.enclosure);
            }
            if b.fauna {
                db += fauna_db;
            }
            match self.beds[i] {
                None => {
                    self.beds[i] = engine.play(&b.sound, PlayOpts::ui().gain_db(db));
                }
                Some(h) => engine.update(h, ear, db, 1.0, 0.0),
            }
        }

        // Positional loops (4 Hz): nearest emitters within radius.
        self.loop_timer -= dt;
        if self.loop_timer <= 0.0 {
            self.loop_timer = 0.25;
            for l in self.loops.iter_mut() {
                l.alive = false;
            }
            for (di, d) in self.map.loops.iter().enumerate() {
                let pts = self.emitters.points(d.zone);
                let r2 = d.radius * d.radius;
                // Up to max_active nearest (small fixed array, insertion sort).
                let mut best: [(f32, usize); 8] = [(f32::INFINITY, usize::MAX); 8];
                let k = (d.max_active as usize).min(8);
                for (pi, p) in pts.iter().enumerate() {
                    let q = *p + Vec3::Y * d.lift;
                    let dd = q.distance_squared(ear);
                    if dd > r2 || k == 0 || dd >= best[k - 1].0 {
                        continue;
                    }
                    let mut j = k - 1;
                    best[j] = (dd, pi);
                    while j > 0 && best[j].0 < best[j - 1].0 {
                        best.swap(j, j - 1);
                        j -= 1;
                    }
                }
                for &(dd, pi) in best.iter().take(k) {
                    if pi == usize::MAX || !dd.is_finite() {
                        continue;
                    }
                    let point = pts[pi] + Vec3::Y * d.lift;
                    let occ = world.map_or(0.0, |w| occlusion(w, ear, point));
                    let gain = d.gain_db + if d.fauna { fauna_db } else { 0.0 };
                    if let Some(l) = self.loops.iter_mut().find(|l| l.def == di && l.point == point) {
                        l.alive = true;
                        engine.update(l.handle, point, gain, 1.0, occ);
                    } else if let Some(h) = engine.play(&d.sound, PlayOpts::at(point).gain_db(gain).occlusion(occ)) {
                        self.loops.push(ActiveLoop { def: di, point, handle: h, alive: true });
                    }
                }
            }
            self.loops.retain(|l| {
                if !l.alive {
                    engine.stop(l.handle, 1.2);
                }
                l.alive
            });
        }

        // Sparkles: Poisson arrivals, placed on a zone point in the distance band.
        let scare = self.scare;
        let started = self.started;
        for (i, s) in self.map.sparkles.iter().enumerate() {
            let mut rate = s.per_min / 60.0;
            if s.fauna {
                rate *= 1.0 - scare;
            }
            self.sparkle_clock[i] -= dt;
            if self.sparkle_clock[i] > 0.0 {
                continue;
            }
            // Next arrival (exponential inter-arrival), never faster than 0.3 s.
            let u = self.rng.f32().max(1e-4);
            self.sparkle_clock[i] = if rate > 1e-4 { (-u.ln() / rate).max(0.3) } else { 1.0 };
            if rate <= 1e-4 || !started {
                continue;
            }
            let Some(p) = pick_point(&self.emitters, &mut self.rng, s.zone, ear, s.min_dist, s.max_dist) else { continue };
            let p = p + Vec3::Y * self.rng.range(s.lift.0, s.lift.1);
            let mut gain = self.rng.range(s.gain_db.0, s.gain_db.1);
            if let Some(c) = s.altitude {
                gain += c.eval(ear.y);
            }
            let occ = world.map_or(0.0, |w| occlusion(w, ear, p));
            engine.play(&s.sound, PlayOpts::at(p).gain_db(gain).occlusion(occ));
        }
        self.started = true;
    }
}

fn pick_point(emitters: &EmitterMap, rng: &mut Rng, zone: Zone, ear: Vec3, dmin: f32, dmax: f32) -> Option<Vec3> {
    if zone == Zone::Sky {
        let a = rng.range(0.0, std::f32::consts::TAU);
        let r = rng.range(dmin, dmax);
        return Some(ear + Vec3::new(a.cos() * r, rng.range(4.0, 20.0), a.sin() * r));
    }
    let pts = emitters.points(zone);
    if pts.is_empty() {
        return None;
    }
    // A few random candidates; the first inside the band wins.
    for _ in 0..24 {
        let p = pts[rng.below(pts.len())];
        let d = p.distance(ear);
        if d >= dmin && d <= dmax {
            return Some(p);
        }
    }
    None
}
