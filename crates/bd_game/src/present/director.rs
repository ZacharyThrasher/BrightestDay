//! **Director camera** for spectator views (`--soak-view`, `--scenario brawl`, `--scenario showcase`).
//!
//! * **Interest**: every `GameEvent` adds heat to a *subject* (a player or a construct): big
//!   impacts, launches, punch-throughs, wall splats, casts forming (scaled by size), kaiju attacks,
//!   transformations, clashes, powers, KOs. Big constructs (kaiju, giants) carry a standing heat so
//!   the camera drifts back to them. Heat decays (half-life ≈ 1.4 s).
//! * **Shots**: hold a subject ≥ 2.5 s; cut early only on a big event; switch when something is much
//!   hotter (≥ 4 s) or the shot got long (≥ 9 s). A nearby new subject in clear view gets a smooth
//!   move instead of a cut. [`Director::direct`] lets a script (the showcase) choose the shot.
//! * **Framing**: 8–25 m depending on subject size (and wide enough to keep its partner — the player
//!   it is fighting / the construct's target — in frame), a 3/4 angle over the subject toward the
//!   partner, a slightly low angle for big things; 50° vertical FOV.
//! * **Never inside or behind geometry**: candidate angles are ray-cast from the framing centre to
//!   the camera and from the camera to subject and partner against the voxel world; the best clear
//!   one wins (with hysteresis), pulled in by a margin when needed. Every frame the smoothed camera
//!   is re-checked: an occluded boom pulls in instantly, a too-short one forces a re-solve + cut.
//! * **Slow motion**: a finishing launch (high % and fast) plays at 0.5× for a moment.
//! * HUD crosshair / viewmodel / first-person effects are off while the director owns the view
//!   (`CameraRig::Director`; there is no local player).

use super::bv;
use super::camera3p::ViewMode;
use super::PresentSet;
use crate::camera::{CameraUpdateSet, PlayerCamera};
use crate::juice::CameraShake;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::tick::{FrameSet, TickClock};
use crate::util::core_v3;
use bd_core::voxel::raycast;
use bd_core::{ConstructId, PlayerId};
use bd_sim::GameEvent;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::PrimaryWindow;

/// Vertical field of view of the director camera.
const VFOV_DEG: f32 = 46.0;
/// Minimum shot length before an interest cut.
const MIN_HOLD: f32 = 2.5;
/// Clearance kept between the camera and terrain (m).
const MARGIN: f32 = 1.0;

/// Something the camera can frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Subject {
    Player(PlayerId),
    Construct(ConstructId),
}

/// The second thing to keep in frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Focus {
    Subject(Subject),
    Point(Vec3),
}

/// A scripted shot ([`Director::direct`]).
#[derive(Clone, Copy, Debug)]
pub struct ScriptShot {
    pub subject: Subject,
    pub partner: Option<Focus>,
    /// Hard cut (else a smooth move when the new subject is close and visible).
    pub cut: bool,
    /// Framing distance override (m).
    pub dist: Option<f32>,
    /// Preferred horizontal direction from the subject to the camera.
    pub side: Option<Vec3>,
    /// Elevation override (degrees above the framing centre).
    pub elev_deg: Option<f32>,
}

impl ScriptShot {
    pub fn on(subject: Subject) -> Self {
        Self { subject, partner: None, cut: true, dist: None, side: None, elev_deg: None }
    }
    pub fn with(mut self, partner: Focus) -> Self {
        self.partner = Some(partner);
        self
    }
    pub fn smooth(mut self) -> Self {
        self.cut = false;
        self
    }
    pub fn dist(mut self, d: f32) -> Self {
        self.dist = Some(d);
        self
    }
    pub fn side(mut self, dir: Vec3) -> Self {
        self.side = Some(dir);
        self
    }
    pub fn elev(mut self, deg: f32) -> Self {
        self.elev_deg = Some(deg);
        self
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Solve {
    /// Horizontal direction from the framing centre to the camera.
    dir: Vec3,
    elev: f32,
    dist: f32,
    score: f32,
}

/// Director state.
#[derive(Resource)]
pub struct Director {
    pub enabled: bool,
    /// Real seconds of 0.5× slow motion left.
    pub slowmo: f32,
    script: Option<ScriptShot>,
    subject: Option<Subject>,
    partner: Option<Focus>,
    shot_t: f32,
    heat: HashMap<Subject, f32>,
    /// A recent big event: (subject, age s).
    big: Option<(Subject, f32)>,
    /// Last exchange partner per player: (other, age s).
    partners: HashMap<PlayerId, (PlayerId, f32)>,
    /// Where the subject was last seen (a KO'd subject keeps its spot framed briefly).
    last_center: Vec3,
    last_radius: f32,
    /// Subject centre last frame (a teleport / respawn / scripted placement ⇒ cut).
    prev_center: Option<Vec3>,
    look: Vec3,
    look_vel: Vec3,
    offset: Vec3,
    offset_vel: Vec3,
    solve: Option<Solve>,
    solve_t: f32,
    check_t: f32,
    force_cd: f32,
    cut: bool,
    slowed: bool,
}

impl Default for Director {
    fn default() -> Self {
        Self {
            enabled: false,
            slowmo: 0.0,
            script: None,
            subject: None,
            partner: None,
            shot_t: 0.0,
            heat: HashMap::default(),
            big: None,
            partners: HashMap::default(),
            last_center: Vec3::ZERO,
            last_radius: 1.0,
            prev_center: None,
            look: Vec3::ZERO,
            look_vel: Vec3::ZERO,
            offset: Vec3::new(0.0, 6.0, 12.0),
            offset_vel: Vec3::ZERO,
            solve: None,
            solve_t: 0.0,
            check_t: 0.0,
            force_cd: 0.0,
            cut: true,
            slowed: false,
        }
    }
}

impl Director {
    /// Take the shot now (scripted). Holds until [`Self::release`] or another `direct`.
    pub fn direct(&mut self, shot: ScriptShot) {
        let same = self.subject == Some(shot.subject);
        self.script = Some(shot);
        self.subject = Some(shot.subject);
        self.partner = shot.partner;
        self.solve = None;
        self.solve_t = 0.0;
        if shot.cut && !same {
            self.cut = true;
        }
        if !same {
            self.shot_t = 0.0;
        }
    }

    /// Back to interest-driven shots (the current shot continues).
    pub fn release(&mut self) {
        self.script = None;
    }

}

pub struct DirectorPlugin;

impl Plugin for DirectorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Director>()
            .add_systems(Update, director_interest.in_set(PresentSet::Sync))
            .add_systems(Update, director_slowmo.in_set(FrameSet::Sim).before(crate::tick::run_sim_ticks))
            .add_systems(PostUpdate, place_director.after(CameraUpdateSet).before(TransformSystems::Propagate).before(bevy::camera::CameraUpdateSystems));
    }
}

// ------------------------------------------------------------------------------------ geometry

#[derive(Clone, Copy, Debug)]
struct Geo {
    center: Vec3,
    radius: f32,
    facing: Vec3,
    speed: f32,
}

fn yaw_forward(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
}

/// A held / worn / tiny construct is framed through its owner.
fn construct_owner_proxy(active: &ActiveWorld, views: &WorldViews, id: ConstructId) -> Option<PlayerId> {
    let c = active.world.constructs.get(id)?;
    let attached = views.construct(id).is_some_and(|v| v.attached());
    (attached || c.half.max_element() < 0.6).then_some(c.owner)
}

fn subject_geo(s: Subject, views: &WorldViews, active: &ActiveWorld) -> Option<Geo> {
    match s {
        Subject::Player(id) => {
            let v = views.player(id)?;
            if !v.alive() {
                return None;
            }
            // Seated: frame the vehicle / cockpit.
            if let Some((cid, _)) = v.seat
                && construct_owner_proxy(active, views, cid).is_none()
                && let Some(g) = subject_geo(Subject::Construct(cid), views, active)
                && g.radius > 1.5
            {
                return Some(g);
            }
            let size = v.size.max(0.3);
            Some(Geo { center: v.pos + Vec3::Y * 0.9 * size, radius: (0.9 * size).max(0.9), facing: yaw_forward(v.yaw), speed: v.vel.length() })
        }
        Subject::Construct(id) => {
            let c = active.world.constructs.get(id)?;
            if c.gone.is_some() {
                return None;
            }
            if let Some(owner) = construct_owner_proxy(active, views, id) {
                return subject_geo(Subject::Player(owner), views, active);
            }
            let pivot = views.construct(id).map(|v| v.pos).unwrap_or(bv(c.pos));
            let center = pivot + (bv(c.center()) - bv(c.pos));
            let vel = bv(c.vel);
            let rot = super::bq(c.rot);
            let facing = if vel.length_squared() > 1.0 { Vec3::new(vel.x, 0.0, vel.z).normalize_or(Vec3::Z) } else { (rot * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z) };
            let h = bv(c.half);
            Some(Geo { center, radius: h.max_element().max(0.6), facing, speed: vel.length() })
        }
    }
}

fn focus_pos(f: Focus, views: &WorldViews, active: &ActiveWorld) -> Option<Vec3> {
    match f {
        Focus::Point(p) => Some(p),
        Focus::Subject(s) => subject_geo(s, views, active).map(|g| g.center),
    }
}

fn valid(s: Subject, views: &WorldViews, active: &ActiveWorld) -> bool {
    subject_geo(s, views, active).is_some()
}

/// Map a construct to the subject that should get its heat.
fn construct_subject(active: &ActiveWorld, views: &WorldViews, id: ConstructId) -> Option<Subject> {
    active.world.constructs.get(id)?;
    Some(match construct_owner_proxy(active, views, id) {
        Some(owner) => Subject::Player(owner),
        None => Subject::Construct(id),
    })
}

// ------------------------------------------------------------------------------------ interest

#[allow(clippy::too_many_arguments)]
fn director_interest(time: Res<Time<Real>>, mut dir: ResMut<Director>, mut events: MessageReader<GameEvents>, views: Res<WorldViews>, active: Option<Res<ActiveWorld>>) {
    let Some(active) = active.filter(|_| dir.enabled) else {
        events.clear();
        return;
    };
    let dt = time.delta_secs().min(0.1);
    let d = &mut *dir;
    let decay = (-dt / 2.0).exp();
    for h in d.heat.values_mut() {
        *h *= decay;
    }
    d.heat.retain(|_, h| *h > 0.02);
    for p in d.partners.values_mut() {
        p.1 += dt;
    }
    d.partners.retain(|_, p| p.1 < 5.0);
    if let Some(b) = d.big.as_mut() {
        b.1 += dt;
    }
    // Big constructs are always somewhat interesting.
    for c in active.world.constructs.iter() {
        if c.gone.is_some() {
            continue;
        }
        let r = c.half.max_element();
        if r > 2.5 {
            let h = d.heat.entry(Subject::Construct(c.id)).or_insert(0.0);
            *h = h.max(0.6 + r * 0.12);
        }
    }
    let add = |d: &mut Director, s: Option<Subject>, w: f32, big: bool| {
        let Some(s) = s else { return };
        *d.heat.entry(s).or_insert(0.0) += w;
        if big && d.big.is_none_or(|(_, age)| age > 0.3) {
            d.big = Some((s, 0.0));
        }
    };
    let nearest = |p: Vec3, r: f32| -> Option<Subject> {
        let mut best: Option<(f32, Subject)> = None;
        for v in views.players.iter().filter(|v| v.alive()) {
            let dd = v.pos.distance(p);
            if dd < r && best.is_none_or(|b| dd < b.0) {
                best = Some((dd, Subject::Player(v.id)));
            }
        }
        best.map(|b| b.1)
    };
    let slowmo_ok = d.slowmo <= 0.0;
    for ev in events.read() {
        match &ev.event {
            GameEvent::Hit { target, source, impact, .. } => {
                let w = 0.4 + impact.strength * 1.4;
                add(d, Some(Subject::Player(*target)), w, false);
                if let Some(s) = source {
                    add(d, Some(Subject::Player(*s)), w * 0.6, false);
                    d.partners.insert(*s, (*target, 0.0));
                    d.partners.insert(*target, (*s, 0.0));
                }
            }
            GameEvent::Launched { player, source, speed, pct, .. } => {
                let big = *speed > 28.0;
                add(d, Some(Subject::Player(*player)), 1.5 + speed / 8.0, big);
                if let Some(s) = source {
                    d.partners.insert(*player, (*s, 0.0));
                }
                // The finishing blow: brief replay-speed moment.
                if *speed > 36.0 && *pct > 90.0 && slowmo_ok {
                    let on_screen = d.subject == Some(Subject::Player(*player)) || matches!(d.partner, Some(Focus::Subject(Subject::Player(p))) if p == *player);
                    if on_screen {
                        d.slowmo = 0.9;
                    }
                }
            }
            GameEvent::PunchThrough { player, .. } => add(d, Some(Subject::Player(*player)), 7.0, true),
            GameEvent::WallSplat { player, .. } => add(d, Some(Subject::Player(*player)), 3.0, false),
            GameEvent::Ko { killer, pos, .. } => {
                let s = killer.map(Subject::Player).or_else(|| nearest(bv(*pos), 25.0));
                add(d, s, 5.0, true);
            }
            GameEvent::ConstructSpawned { id, owner, recipe, .. } => {
                let big = recipe.scale >= 4.0;
                let s = construct_subject(&active, &views, *id).or(Some(Subject::Player(*owner)));
                add(d, s, 1.0 + recipe.scale.min(20.0) * 0.4, big);
            }
            GameEvent::Transformed { id, .. } => {
                let s = construct_subject(&active, &views, *id);
                add(d, s, 6.0, true);
            }
            GameEvent::CreatureAttack { id, impact, .. } if *impact > 0.0 => {
                let s = construct_subject(&active, &views, *id);
                add(d, s, 1.0 + impact * 2.0, *impact >= 1.8);
            }
            GameEvent::Clash { a, b, strength, .. } => {
                let w = 1.5 + strength;
                let (sa, sb) = (construct_subject(&active, &views, *a), construct_subject(&active, &views, *b));
                add(d, sa, w, false);
                add(d, sb, w * 0.7, false);
            }
            GameEvent::Explosion { pos, radius, source, .. } => {
                let s = source.map(Subject::Player).or_else(|| nearest(bv(*pos), 14.0));
                add(d, s, 1.2 + radius * 0.4, *radius > 5.0);
            }
            GameEvent::ConstructShattered { id, pos, .. } => {
                let s = construct_subject(&active, &views, *id).or_else(|| nearest(bv(*pos), 14.0));
                add(d, s, 1.0, false);
            }
            GameEvent::Grafted { id, .. } => {
                let s = construct_subject(&active, &views, *id);
                add(d, s, 2.0, false);
            }
            GameEvent::Seated { construct, .. } => {
                let s = construct_subject(&active, &views, *construct);
                add(d, s, 2.0, false);
            }
            GameEvent::PortalOpened { owner, .. } => add(d, Some(Subject::Player(*owner)), 3.0, false),
            // Through a portal: the subject jumps across the map — cut with it.
            GameEvent::Teleported { who: bd_sim::events::Traveller::Player(p), .. } => {
                if d.subject == Some(Subject::Player(*p)) {
                    d.cut = true;
                    d.solve = None;
                }
                add(d, Some(Subject::Player(*p)), 3.0, false);
            }
            GameEvent::Power(_) | GameEvent::Resized { .. } => {}
            _ => {}
        }
    }
    d.shot_t += dt;

    // ---- shot selection
    if let Some(script) = d.script {
        if valid(script.subject, &views, &active) {
            d.subject = Some(script.subject);
            d.partner = script.partner;
            return;
        }
        // The scripted subject is gone (shattered, KO'd): hold on its spot briefly, then its partner.
        if d.shot_t < 1.2 {
            return;
        }
        if let Some(Focus::Subject(p)) = script.partner
            && valid(p, &views, &active)
        {
            d.script = Some(ScriptShot { subject: p, partner: None, ..script });
            d.subject = Some(p);
            d.partner = None;
            d.shot_t = 0.0;
            return;
        }
        d.script = None;
    }
    let cur = d.subject.filter(|s| valid(*s, &views, &active));
    let cur_heat = cur.and_then(|s| d.heat.get(&s).copied()).unwrap_or(0.0);
    let best = d.heat.iter().filter(|(s, _)| valid(**s, &views, &active)).max_by(|a, b| a.1.total_cmp(b.1)).map(|(s, h)| (*s, *h));
    let big_other = d.big.filter(|(s, age)| *age < 0.6 && Some(*s) != cur && valid(*s, &views, &active)).map(|b| b.0);
    let pick = match (cur, best) {
        (None, _) if d.shot_t > 1.0 || d.subject.is_none() => best.map(|b| b.0).or_else(|| views.players.iter().find(|v| v.alive()).map(|v| Subject::Player(v.id))),
        (None, _) => None,
        (Some(_), _) if d.shot_t >= MIN_HOLD && big_other.is_some() => big_other,
        (Some(c), Some((b, h))) if b != c && d.shot_t >= 4.0 && h > cur_heat * 1.5 + 1.0 => Some(b),
        (Some(c), Some((b, h))) if b != c && d.shot_t >= 9.0 && h > 0.4 => Some(b),
        _ => None,
    };
    if let Some(p) = pick
        && Some(p) != d.subject
    {
        d.subject = Some(p);
        d.shot_t = 0.0;
        d.solve = None;
        d.big = None;
        // Smooth move when the new subject is near the current framing; a cut otherwise (decided in
        // `place_director`, which knows the camera).
        d.cut = subject_geo(p, &views, &active).is_none_or(|g| g.center.distance(d.look) > 16.0);
    }
    // Partner: whoever the subject is fighting.
    d.partner = d.subject.and_then(|s| partner_of(s, d, &views, &active));
}

fn partner_of(s: Subject, d: &Director, views: &WorldViews, active: &ActiveWorld) -> Option<Focus> {
    let g = subject_geo(s, views, active)?;
    let alive = |id: PlayerId| views.player(id).is_some_and(|v| v.alive());
    let near_enemy = |exclude: PlayerId, r: f32| {
        views
            .players
            .iter()
            .filter(|v| v.alive() && v.id != exclude)
            .map(|v| (v.pos.distance(g.center), v.id))
            .filter(|(dd, _)| *dd < r)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    };
    let pick = match s {
        Subject::Player(id) => d.partners.get(&id).map(|p| p.0).filter(|p| alive(*p)).or_else(|| near_enemy(id, 18.0)),
        Subject::Construct(cid) => {
            let c = active.world.constructs.get(cid)?;
            let owner_target = active.world.player(c.owner).and_then(|p| p.last_target);
            c.target.filter(|t| alive(*t)).or(owner_target.filter(|t| alive(*t))).or_else(|| near_enemy(c.owner, 25.0))
        }
    }?;
    let p = views.player(pick)?;
    (p.pos.distance(g.center) < 32.0).then_some(Focus::Subject(Subject::Player(pick)))
}

// ------------------------------------------------------------------------------------ slow motion

fn director_slowmo(time: Res<Time<Real>>, mut dir: ResMut<Director>, mut clock: ResMut<TickClock>, mut virt: ResMut<Time<Virtual>>) {
    let dt = time.delta_secs().min(0.1);
    if dir.enabled && dir.slowmo > 0.0 {
        dir.slowmo -= dt;
        clock.time_scale *= 0.5;
        if !dir.slowed {
            virt.set_relative_speed(0.5);
            dir.slowed = true;
        }
    } else if dir.slowed {
        virt.set_relative_speed(1.0);
        dir.slowed = false;
    }
}

// ------------------------------------------------------------------------------------ placement

fn smooth_damp(cur: Vec3, target: Vec3, vel: &mut Vec3, smooth_time: f32, dt: f32) -> Vec3 {
    let omega = 2.0 / smooth_time.max(1e-3);
    let x = omega * dt;
    let e = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let change = cur - target;
    let temp = (*vel + omega * change) * dt;
    *vel = (*vel - omega * temp) * e;
    target + (change + temp) * e
}

struct Vox<'a>(&'a bd_voxel::VoxelWorld);

impl Vox<'_> {
    /// Distance to the first solid voxel from `a` toward `b` (None = clear).
    fn hit(&self, a: Vec3, b: Vec3) -> Option<f32> {
        let d = b - a;
        let len = d.length();
        if len < 1e-3 {
            return None;
        }
        raycast(self.0, core_v3(a), core_v3(d / len), len).map(|h| h.t)
    }
    /// [`Self::hit`] ignoring the first `skip` m (terrain inside a big subject's own volume, or
    /// a framing centre that sits inside a tree / wall, must not veto every angle).
    fn hit_skip(&self, a: Vec3, b: Vec3, skip: f32) -> Option<f32> {
        let d = b - a;
        let len = d.length();
        if len <= skip + 1e-3 {
            return None;
        }
        let dir = d / len;
        self.hit(a + dir * skip, b).map(|t| t + skip)
    }
    /// Like [`Self::hit`], also telling whether the face hit is a floor (top face).
    fn hit_floor(&self, a: Vec3, b: Vec3) -> Option<(f32, bool)> {
        let d = b - a;
        let len = d.length();
        if len < 1e-3 {
            return None;
        }
        raycast(self.0, core_v3(a), core_v3(d / len), len).map(|h| (h.t, h.normal.y > 0))
    }
    fn solid(&self, p: Vec3) -> bool {
        self.0.get_at(core_v3(p)) != 0
    }
    /// Is there solid terrain within ~0.7 m of `p`?
    fn cramped(&self, p: Vec3) -> bool {
        const O: [Vec3; 7] = [Vec3::ZERO, Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z];
        O.iter().any(|o| self.solid(p + *o * 0.7))
    }
}

/// Penalty for terrain close to the camera inside the view frustum (rays to 8 points across the
/// frame at the framing centre's depth).
fn foreground_penalty(vox: &Vox, cam: Vec3, look: Vec3, tan_h: f32) -> f32 {
    let fwd = look - cam;
    let depth = fwd.length();
    if depth < 0.5 {
        return 0.0;
    }
    let f = fwd / depth;
    let right = f.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = right.cross(f);
    let tan_v = (VFOV_DEG.to_radians() * 0.5).tan();
    let mut pen = 0.0;
    for y in [-0.7f32, 0.0, 0.7] {
        for x in [-0.88f32, -0.45, 0.0, 0.45, 0.88] {
            if x == 0.0 && y == 0.0 {
                continue;
            }
            let p = look + right * (x * tan_h * depth) + up * (y * tan_v * depth);
            let len = (p - cam).length();
            if let Some((t, floor)) = vox.hit_floor(cam, p) {
                // The ground in front of the subject is fine (unless it's a hill right at the lens).
                if floor && t > len * 0.35 {
                    continue;
                }
                // Anything between the camera and the subject's depth covers part of the frame;
                // the nearer, the more.
                let near = 1.0 - (t / (len * 0.8)).clamp(0.0, 1.0);
                if near > 0.0 {
                    pen += 0.6 + 2.0 * near;
                }
            }
        }
    }
    pen
}

fn dir_from(h: Vec3, elev_deg: f32) -> Vec3 {
    let e = elev_deg.to_radians();
    (h * e.cos() + Vec3::Y * e.sin()).normalize_or(Vec3::Y)
}

fn rotate_y(v: Vec3, deg: f32) -> Vec3 {
    Quat::from_rotation_y(deg.to_radians()) * v
}

/// Evaluate camera candidates around `look` and return the best.
#[allow(clippy::too_many_arguments)]
fn solve_shot(vox: &Vox, look: Vec3, subj: &Geo, partner: Option<Vec3>, base: Vec3, elev: f32, dist: f32, current_cam: Option<Vec3>, prev: Option<Solve>, tan_h: f32, skip: f32) -> Solve {
    const YAWS: [f32; 12] = [0.0, 25.0, -25.0, 50.0, -50.0, 80.0, -80.0, 115.0, -115.0, 150.0, -150.0, 180.0];
    const ELEVS: [f32; 4] = [0.0, 12.0, 24.0, 38.0];
    let mut best = Solve { dir: base, elev: elev + 45.0, dist: dist.min(8.0), score: f32::MAX };
    let mut best_fg = 0.0;
    let mut evaluated = 0;
    // Fallback when nothing meets the bar: the longest clear boom.
    let mut fallback: Option<(f32, Solve)> = None;
    for (yi, yaw) in YAWS.iter().enumerate() {
        let h = rotate_y(base, *yaw);
        for (ei, de) in ELEVS.iter().enumerate() {
            let e = (elev + de).min(55.0);
            let d = dir_from(h, e);
            let want = look + d * dist;
            let avail = vox.hit_skip(look, want + d * MARGIN, skip).map_or(dist, |t| t - MARGIN);
            if avail > 2.5 && fallback.is_none_or(|f| avail > f.0) && !vox.cramped(look + d * avail.min(dist)) {
                fallback = Some((avail, Solve { dir: h, elev: e, dist: avail.min(dist), score: 99.0 }));
            }
            if avail < (dist * 0.55).max(subj.radius + 4.0) {
                continue;
            }
            let used = avail.min(dist);
            let cam = look + d * used;
            if vox.cramped(cam) {
                continue;
            }
            let mut score = yaw.abs() / 180.0 * 1.2 + de / 38.0 * 1.5 + yi as f32 * 0.001 + ei as f32 * 0.001;
            score += (dist - used) / dist * 2.0;
            // The subject itself must be visible (not just the framing centre).
            let to_subj = subj.center - cam;
            let reach = (to_subj.length() - subj.radius * 0.6).max(0.5);
            if vox.hit(cam, cam + to_subj.normalize_or_zero() * reach).is_some() {
                score += 3.0;
            }
            if let Some(p) = partner
                && vox.hit(cam, p + Vec3::Y * 0.2).is_some_and(|t| t < cam.distance(p) - 0.8)
            {
                score += 1.2;
            }
            // Foreground occluders: rays toward the frame's corners must not hit anything near the
            // camera (a trunk or wall filling part of the frame).
            let fg = foreground_penalty(vox, cam, look, tan_h);
            score += fg;
            // Ground under a very low camera: prefer a little air below.
            if vox.solid(cam - Vec3::Y * 1.4) {
                score += 0.4;
            }
            if let Some(c) = current_cam {
                score += (cam - c).length() / dist.max(1.0) * 0.5;
            }
            if let Some(pv) = prev
                && pv.dir.dot(h) > 0.97
                && (pv.elev - e).abs() < 1.0
            {
                score -= 0.35; // hysteresis: keep the current angle unless clearly worse
            }
            if score < best.score {
                best = Solve { dir: h, elev: e, dist: used, score };
                best_fg = fg;
            }
            evaluated += 1;
        }
    }
    if evaluated == 0
        && let Some((_, f)) = fallback
    {
        best = f;
    }
    if debug() {
        info!("director solve: look {look:.1} dist {dist:.1} r {:.1} evaluated {evaluated} best score {:.2} fg {best_fg:.2} elev {:.0} used {:.1}", subj.radius, best.score, best.elev, best.dist);
    }
    best
}

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| std::env::var_os("BD_DIRECTOR_DEBUG").is_some())
}

#[allow(clippy::too_many_arguments)]
fn place_director(
    time: Res<Time<Real>>,
    mut dir: ResMut<Director>,
    views: Res<WorldViews>,
    active: Option<Res<ActiveWorld>>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    shake: Res<CameraShake>,
    mode: Res<ViewMode>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut cam: Query<(&mut Transform, &mut Projection), With<PlayerCamera>>,
) {
    if !dir.enabled || !mode.director {
        return;
    }
    let Ok((mut tf, mut proj)) = cam.single_mut() else { return };
    let Some(active) = active else { return };
    let Some(arena) = arena else { return };
    let dt = time.delta_secs().min(0.1);
    let aspect = window.map_or(16.0 / 9.0, |w| w.width().max(1.0) / w.height().max(1.0));
    if let Projection::Perspective(p) = proj.as_mut() {
        p.fov = VFOV_DEG.to_radians();
    }
    let hfov_half = ((VFOV_DEG.to_radians() * 0.5).tan() * aspect).atan();
    let guard = arena.read();
    let vox = Vox(&guard);
    let d = &mut *dir;

    // ---- what to frame
    let geo = d.subject.and_then(|s| subject_geo(s, &views, &active));
    let geo = match geo {
        Some(g) => {
            d.last_center = g.center;
            d.last_radius = g.radius;
            if d.prev_center.is_some_and(|p| p.distance(g.center) > 9.0) {
                d.cut = true;
                d.solve = None;
            }
            d.prev_center = Some(g.center);
            g
        }
        None => Geo { center: d.last_center, radius: d.last_radius, facing: Vec3::Z, speed: 0.0 },
    };
    if d.subject.is_none() && d.last_center == Vec3::ZERO {
        d.last_center = arena.info.center + Vec3::Y * 4.0;
    }
    let partner = d.partner.and_then(|f| focus_pos(f, &views, &active)).filter(|p| p.distance(geo.center) < 34.0);
    let r = geo.radius;
    let script = d.script;
    let (look, fit) = match partner {
        Some(p) => {
            let w = 0.38 / (1.0 + r / 5.0);
            let s = p.distance(geo.center);
            (geo.center.lerp(p, w), (s * 0.52 + r + 0.6) / (hfov_half * 0.9).tan())
        }
        None => (geo.center, 0.0),
    };
    // Rays from the framing centre ignore what's inside the subject (and a centre stuck in a wall).
    let skip = {
        let mut k = (r * 0.7).max(0.4);
        if vox.solid(look) {
            k = k.max(r + 1.5);
        }
        k
    };
    // Kaiju-sized subjects need room: up to ~40 m.
    let max_d = if r > 6.0 { 40.0 } else { 25.0 };
    let size_dist = (r * 2.2 + 4.5).clamp(6.5, max_d);
    let dist = script.and_then(|s| s.dist).unwrap_or(size_dist.max(fit).clamp(6.5, max_d.max(28.0)));
    let big = ((r - 1.5) / 4.5).clamp(0.0, 1.0);
    let big = big * big * (3.0 - 2.0 * big);
    let elev = script.and_then(|s| s.elev_deg).unwrap_or(22.0 + (-7.0 - 22.0) * big);
    // Base 3/4 direction (horizontal, from the framing centre toward the camera).
    let base = if let Some(side) = script.and_then(|s| s.side) {
        side.with_y(0.0).normalize_or(Vec3::Z)
    } else if let Some(p) = partner {
        let l = (p - geo.center).with_y(0.0).normalize_or(geo.facing);
        let perp = Vec3::new(-l.z, 0.0, l.x);
        // Behind the subject, 60° off the line toward the partner; keep the side we're on.
        let a = (-l * 0.5 + perp * 0.866).normalize();
        let b = (-l * 0.5 - perp * 0.866).normalize();
        let cur = (d.look + d.offset - look).with_y(0.0).normalize_or(a);
        if cur.dot(a) >= cur.dot(b) { a } else { b }
    } else {
        let f = geo.facing.with_y(0.0).normalize_or(Vec3::Z);
        let perp = Vec3::new(-f.z, 0.0, f.x);
        let a = (f * 0.7 + perp * 0.7).normalize();
        let b = (f * 0.7 - perp * 0.7).normalize();
        let cur = (d.look + d.offset - look).with_y(0.0).normalize_or(a);
        if cur.dot(a) >= cur.dot(b) { a } else { b }
    };

    // ---- solve (every 0.3 s, on a cut, or when the current angle is blocked)
    let cur_cam = d.look + d.offset;
    d.solve_t -= dt;
    let blocked_now = d.solve.is_some_and(|s| {
        let c = look + dir_from(s.dir, s.elev) * s.dist;
        vox.hit_skip(look, c, skip).is_some() || vox.cramped(c)
    });
    if d.solve.is_none() || d.cut || d.solve_t <= 0.0 || blocked_now {
        d.solve_t = 0.3;
        let prev = if d.cut { None } else { d.solve };
        let s = solve_shot(&vox, look, &geo, partner, base, elev, dist, (!d.cut).then_some(cur_cam), prev, hfov_half.tan(), skip);
        // A big angle change mid-shot would swing through the scene: cut instead.
        if let Some(old) = d.solve
            && !d.cut
            && (old.dir.dot(s.dir) < 0.35 || blocked_now && old.dir.dot(s.dir) < 0.8)
            && d.shot_t > 0.8
        {
            d.cut = true;
        }
        d.solve = Some(s);
    }
    let s = d.solve.unwrap_or_default();
    // Gentle drift keeps held shots alive.
    let drift = rotate_y(s.dir, (d.shot_t * 3.0).min(20.0) * if base.cross(s.dir).y >= 0.0 { 1.0 } else { -1.0 });
    let drift_ok = vox.hit_skip(look, look + dir_from(drift, s.elev) * (s.dist + MARGIN), skip).is_none()
        && foreground_penalty(&vox, look + dir_from(drift, s.elev) * s.dist, look, hfov_half.tan()) < 0.5;
    let want_off = dir_from(if drift_ok { drift } else { s.dir }, s.elev) * s.dist;

    // ---- smooth
    if d.cut {
        d.cut = false;
        d.look = look;
        d.offset = want_off;
        d.look_vel = Vec3::ZERO;
        d.offset_vel = Vec3::ZERO;
    } else {
        let t_look = 0.14 / (1.0 + geo.speed / 8.0);
        d.look = smooth_damp(d.look, look, &mut d.look_vel, t_look, dt);
        d.offset = smooth_damp(d.offset, want_off, &mut d.offset_vel, if geo.speed > 12.0 { 0.35 } else { 0.6 }, dt);
    }
    // ---- never inside / behind geometry: validate the final camera; if it fails, re-solve and cut
    // this very frame (a bad frame is never shown).
    let subject_c = geo.center;
    let bad = |pos: Vec3, look: Vec3| -> bool {
        if vox.cramped(pos) {
            return true;
        }
        let back = pos - look;
        if vox.hit_skip(look, pos + back.normalize_or_zero() * MARGIN * 0.5, skip).is_some() {
            return true;
        }
        let to = subject_c - pos;
        let reach = (to.length() - r * 0.6).max(0.5);
        vox.hit(pos, pos + to.normalize_or_zero() * reach).is_some_and(|t| t < reach * 0.8)
    };
    let mut pos = d.look + d.offset;
    // Pull in along the boom first (an occluder behind the camera's line).
    let len = d.offset.length().max(0.1);
    let odir = d.offset / len;
    if let Some(t) = vox.hit_skip(d.look, pos + odir * MARGIN, skip) {
        pos = d.look + odir * (t - MARGIN).max(0.3);
    }
    d.check_t -= dt;
    d.force_cd -= dt;
    let foreground = if d.check_t <= 0.0 && d.force_cd <= 0.0 {
        d.check_t = 0.1;
        let fg = foreground_penalty(&vox, pos, d.look, hfov_half.tan());
        if debug() {
            info!("director frame fg {fg:.2} pos {pos:.1} look {:.1}", d.look);
        }
        fg > 3.5
    } else {
        false
    };
    if bad(pos, d.look) || pos.distance(d.look) < (r + 2.5).min(5.0) || foreground {
        d.force_cd = 1.5;
        let s = solve_shot(&vox, look, &geo, partner, base, elev, dist, None, None, hfov_half.tan(), skip);
        d.solve = Some(s);
        d.solve_t = 0.3;
        d.look = look;
        d.offset = dir_from(s.dir, s.elev) * s.dist;
        d.look_vel = Vec3::ZERO;
        d.offset_vel = Vec3::ZERO;
        pos = d.look + d.offset;
    }
    let target = d.look + Vec3::Y * r * 0.08;
    let mut t = Transform::from_translation(pos).looking_at(target, Vec3::Y);
    // A touch of the impact shake (no roll).
    t.translation += t.rotation * shake.offset * 0.5;
    t.rotation *= shake.rotation;
    *tf = t;
}
