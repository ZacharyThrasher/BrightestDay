//! Hit REACTIONS by weight (3P bodies): the victim's body sells the impact. Scaled from the sim's
//! own numbers (`Hit::impact.strength`, knockback, the tumble / launched flags, `WallSplat`), never
//! changing a sim outcome — render-side transforms on top of the rig's layers:
//!
//! | weight | reaction |
//! |---|---|
//! | light (< `STAGGER_MIN`) | **flinch** — the rig's `HitReact` layer (players.rs `react`), unchanged |
//! | medium, no launch | **stagger** — lean back off the hit, a stumble step, knees buckle, recover |
//! | launch | **tumble** — the rig's `Launched` / `Tumble` (sim flags), then a **get-up** crouch |
//! | into a wall | **wall splat** — flattened and spread against the wall, a beat, peel off |
//! | hard landing out of a launch | **ground bounce** — squash, a hop, then the get-up |
//!
//! `players::sync_bodies` reads [`HitReactions::pose`] (a feet-pivoted tilt, an offset, a squash
//! and an optional extra `HitReact` layer). The timeline math is pure ([`React::sample`]) and tested.

use super::super::{bv, PresentSet};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use bd_core::PlayerId;
use bd_sim::replicate::pflags;
use bd_sim::GameEvent;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// Hit strength at which a grounded, un-launched hit staggers instead of flinching.
pub const STAGGER_MIN: f32 = 0.35;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Lean back off the hit (dir = world hit direction), stumble, buckle, recover.
    Stagger,
    /// Flattened against a wall (dir = the wall normal, pointing out of the wall).
    WallSplat,
    /// Squash + hop off the ground after a launch (dir = up).
    Bounce,
    /// Crouched get-up after a tumble ends.
    GetUp,
}

#[derive(Clone, Copy, Debug)]
pub struct React {
    pub kind: Kind,
    pub dir: Vec3,
    pub strength: f32,
    pub t: f32,
}

/// The pose delta for this frame (world space; the tilt pivots about the feet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReactPose {
    pub offset: Vec3,
    pub tilt: Quat,
    /// Body-local scale.
    pub scale: Vec3,
    /// An extra `HitReact` layer: (world hit dir, strength, t).
    pub layer: Option<(Vec3, f32, f32)>,
}

impl Default for ReactPose {
    fn default() -> Self {
        Self { offset: Vec3::ZERO, tilt: Quat::IDENTITY, scale: Vec3::ONE, layer: None }
    }
}

impl Kind {
    pub fn duration(self, strength: f32) -> f32 {
        match self {
            Kind::Stagger => 0.45 + 0.35 * strength.clamp(0.0, 1.0),
            Kind::WallSplat => 0.5,
            Kind::Bounce => 0.34,
            Kind::GetUp => 0.42,
        }
    }
}

fn ease_out(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    1.0 - (1.0 - x) * (1.0 - x)
}

impl React {
    pub fn done(&self) -> bool {
        self.t >= self.kind.duration(self.strength)
    }

    /// The pose at `self.t` for a body of height scale `size` whose yaw rotation is `body`.
    pub fn sample(&self, size: f32, body: Quat) -> ReactPose {
        let dur = self.kind.duration(self.strength);
        let u = (self.t / dur).clamp(0.0, 1.0);
        let s = self.strength.clamp(0.0, 1.5);
        match self.kind {
            Kind::Stagger => {
                // Snap back in 60 ms, hold, then recover with a small forward overshoot.
                let attack = ease_out(self.t / 0.06);
                let rec = if u < 0.35 { 1.0 } else { 1.0 - ease_out((u - 0.35) / 0.65) };
                let over = if u > 0.6 { -0.18 * (std::f32::consts::PI * (u - 0.6) / 0.4).sin() } else { 0.0 };
                let k = attack * rec + over;
                let flat = Vec3::new(self.dir.x, 0.0, self.dir.z).normalize_or(body * Vec3::Z);
                let axis = Vec3::Y.cross(flat).normalize_or(Vec3::X);
                let tilt = Quat::from_axis_angle(axis, (10.0 + 14.0 * s).to_radians() * k);
                // The stumble step (feet slide back along the hit) + knees buckle.
                let step = flat * (0.12 + 0.22 * s) * size * ease_out(self.t / 0.18) * (1.0 - ease_out((u - 0.5) / 0.5).max(0.0));
                let dip = -Vec3::Y * (0.05 + 0.08 * s) * size * attack * rec;
                let squash = 1.0 - 0.06 * s * attack * rec;
                ReactPose { offset: step + dip, tilt, scale: Vec3::new(1.0, squash, 1.0), layer: Some((self.dir, (0.8 + s).min(1.5), self.t)) }
            }
            Kind::WallSplat => {
                // Flat against the wall (squashed along the normal, spread wide), a beat, peel off.
                let n = Vec3::new(self.dir.x, 0.0, self.dir.z).normalize_or(Vec3::Z);
                let press = if u < 0.08 { ease_out(u / 0.08) } else if u < 0.55 { 1.0 } else { 1.0 - ease_out((u - 0.55) / 0.45) };
                // Body-local axis most aligned with the normal gets squashed.
                let ln = body.inverse() * n;
                let (ax, az) = (ln.x.abs(), ln.z.abs());
                let depth = 1.0 - 0.38 * press;
                let spread = 1.0 + 0.16 * press;
                let scale = if az >= ax { Vec3::new(spread, 1.0 + 0.05 * press, depth) } else { Vec3::new(depth, 1.0 + 0.05 * press, spread) };
                // Pressed into the wall, sliding down a little while it holds.
                let slide = -Vec3::Y * 0.25 * size * ease_out(u / 0.55) * press;
                let into = -n * 0.12 * size * press;
                // Lean the top into the wall.
                let axis = Vec3::Y.cross(n).normalize_or(Vec3::X);
                let tilt = Quat::from_axis_angle(axis, -8f32.to_radians() * press);
                ReactPose { offset: into + slide, tilt, scale, layer: Some((-n, 1.5, self.t.min(0.05))) }
            }
            Kind::Bounce => {
                // Squash on contact, overshoot stretch in the hop, settle.
                let sq = if u < 0.2 { ease_out(u / 0.2) } else { 1.0 - ease_out((u - 0.2) / 0.8) };
                let hop = (std::f32::consts::PI * ((u - 0.15) / 0.85).clamp(0.0, 1.0)).sin() * 0.28 * size * (0.5 + 0.5 * s);
                let y = 1.0 - 0.32 * sq + 0.1 * (1.0 - sq) * (u > 0.2) as u8 as f32 * (1.0 - u);
                let w = 1.0 + 0.2 * sq;
                ReactPose { offset: Vec3::Y * hop, tilt: Quat::IDENTITY, scale: Vec3::new(w, y, w), layer: Some((Vec3::NEG_Y, 1.2, self.t)) }
            }
            Kind::GetUp => {
                // Low crouch, chest forward, rising.
                let k = 1.0 - ease_out(u);
                let fwd = body * Vec3::NEG_Z;
                let axis = Vec3::Y.cross(fwd).normalize_or(Vec3::X);
                let tilt = Quat::from_axis_angle(axis, -22f32.to_radians() * k);
                ReactPose { offset: -Vec3::Y * 0.3 * size * k, tilt, scale: Vec3::new(1.0 + 0.06 * k, 1.0 - 0.12 * k, 1.0 + 0.06 * k), layer: None }
            }
        }
    }
}

/// Classify a hit by weight: `None` = a flinch (the rig's own layer handles it).
pub fn classify_hit(strength: f32, tumble: bool, launched: bool) -> Option<Kind> {
    if tumble || launched {
        return None; // the tumble / launch anim takes it; bounce + get-up follow
    }
    (strength >= STAGGER_MIN).then_some(Kind::Stagger)
}

#[derive(Default, Clone, Copy)]
struct Track {
    react: Option<React>,
    was_tumbling: bool,
    fall_speed: f32,
}

/// Live reactions per player body.
#[derive(Resource, Default)]
pub struct HitReactions {
    map: HashMap<PlayerId, Track>,
}

impl HitReactions {
    /// This frame's reaction pose for a body (feet-pivoted), if any.
    pub fn pose(&self, id: PlayerId, size: f32, body: Quat) -> Option<ReactPose> {
        self.map.get(&id).and_then(|t| t.react).filter(|r| !r.done()).map(|r| r.sample(size, body))
    }
    fn start(&mut self, id: PlayerId, r: React) {
        let t = self.map.entry(id).or_default();
        // A splat / bounce beats a stagger in progress; a new stagger restarts one.
        let keep = t.react.is_some_and(|o| !o.done() && matches!(o.kind, Kind::WallSplat | Kind::Bounce) && r.kind == Kind::Stagger);
        if !keep {
            t.react = Some(r);
        }
    }
}

pub fn build(app: &mut App) {
    app.init_resource::<HitReactions>().add_systems(Update, update.in_set(PresentSet::Events));
}

fn update(time: Res<Time<Real>>, active: Option<Res<ActiveWorld>>, views: Res<WorldViews>, mut st: ResMut<HitReactions>, mut events: MessageReader<GameEvents>) {
    let dt = time.delta_secs().min(0.1);
    if active.is_none() {
        events.clear();
        st.map.clear();
        return;
    }
    for ev in events.read() {
        match &ev.event {
            GameEvent::Hit { target, tumble, impact, .. } => {
                let launched = views.player(*target).is_some_and(|v| v.has(pflags::LAUNCHED));
                if let Some(kind) = classify_hit(impact.strength, *tumble, launched) {
                    st.start(*target, React { kind, dir: bv(impact.dir), strength: impact.strength, t: 0.0 });
                }
            }
            GameEvent::WallSplat { player, normal, speed, .. } => {
                st.start(*player, React { kind: Kind::WallSplat, dir: bv(*normal), strength: (speed / 25.0).clamp(0.4, 1.5), t: 0.0 });
            }
            _ => {}
        }
    }
    for v in &views.players {
        let t = st.map.entry(v.id).or_default();
        if let Some(r) = t.react.as_mut() {
            r.t += dt;
        }
        let tumbling = v.has(pflags::TUMBLING) || v.has(pflags::LAUNCHED);
        let grounded = v.has(pflags::GROUNDED);
        if !v.alive() {
            *t = Track::default();
            continue;
        }
        if tumbling {
            t.fall_speed = (-v.vel.y).max(0.0).max(t.fall_speed * 0.9);
        }
        // Out of a launch: a hard landing bounces, then the get-up; a soft one just gets up.
        if t.was_tumbling && (grounded || !tumbling) {
            if grounded && t.fall_speed > 9.0 && t.react.is_none_or(|r| r.done() || r.kind != Kind::WallSplat) {
                t.react = Some(React { kind: Kind::Bounce, dir: Vec3::Y, strength: (t.fall_speed / 25.0).clamp(0.3, 1.5), t: 0.0 });
            } else if !tumbling && t.react.is_none_or(|r| r.done()) {
                t.react = Some(React { kind: Kind::GetUp, dir: Vec3::Y, strength: 1.0, t: 0.0 });
            }
            t.fall_speed = 0.0;
        }
        // The bounce hands over to the get-up.
        if let Some(r) = t.react
            && r.kind == Kind::Bounce
            && r.done()
            && !tumbling
        {
            t.react = Some(React { kind: Kind::GetUp, dir: Vec3::Y, strength: 1.0, t: 0.0 });
        }
        t.was_tumbling = tumbling && !grounded;
        if t.react.is_some_and(|r| r.done()) {
            t.react = None;
        }
    }
    st.map.retain(|id, _| views.player(*id).is_some());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_picks_the_reaction() {
        assert_eq!(classify_hit(0.1, false, false), None, "light: flinch (the rig's layer)");
        assert_eq!(classify_hit(0.5, false, false), Some(Kind::Stagger));
        assert_eq!(classify_hit(1.2, true, false), None, "launch: tumble anim");
        assert_eq!(classify_hit(1.2, false, true), None);
    }

    #[test]
    fn reactions_start_neutral_peak_and_return_to_neutral() {
        let body = Quat::IDENTITY;
        for kind in [Kind::Stagger, Kind::WallSplat, Kind::Bounce, Kind::GetUp] {
            let mut r = React { kind, dir: Vec3::Z, strength: 1.0, t: 0.0 };
            let dur = kind.duration(1.0);
            let mut peak: f32 = 0.0;
            let steps = 60;
            for i in 0..=steps {
                r.t = dur * i as f32 / steps as f32;
                let p = r.sample(1.0, body);
                assert!(p.offset.is_finite() && p.scale.is_finite() && p.tilt.is_finite(), "{kind:?}");
                assert!(p.scale.min_element() > 0.5 && p.scale.max_element() < 1.5, "{kind:?} {:?}", p.scale);
                let dev = p.offset.length() + (p.scale - Vec3::ONE).length() + p.tilt.angle_between(Quat::IDENTITY);
                peak = peak.max(dev);
            }
            r.t = dur;
            let end = r.sample(1.0, body);
            let dev = end.offset.length() + (end.scale - Vec3::ONE).length() + end.tilt.angle_between(Quat::IDENTITY);
            assert!(peak > 0.1, "{kind:?} visibly reacts ({peak})");
            assert!(dev < 0.05, "{kind:?} recovers to neutral ({dev})");
            assert!(r.done());
        }
    }

    #[test]
    fn a_stagger_leans_away_from_the_hit_and_heavier_is_bigger() {
        let hit_dir = Vec3::NEG_Z; // shoved toward −Z
        let light = React { kind: Kind::Stagger, dir: hit_dir, strength: 0.4, t: 0.1 }.sample(1.0, Quat::IDENTITY);
        let heavy = React { kind: Kind::Stagger, dir: hit_dir, strength: 1.0, t: 0.1 }.sample(1.0, Quat::IDENTITY);
        // The head (feet-pivoted) moves along the hit.
        let head = |p: ReactPose| p.tilt * Vec3::Y * 1.7 + p.offset;
        assert!(head(heavy).z < -0.2, "{:?}", head(heavy));
        assert!(heavy.tilt.angle_between(Quat::IDENTITY) > light.tilt.angle_between(Quat::IDENTITY));
        assert!(heavy.offset.length() > light.offset.length());
    }

    #[test]
    fn a_wall_splat_flattens_along_the_wall_normal() {
        // Wall normal +Z (the body faces −Z by default): depth (z) squashes, width spreads.
        let p = React { kind: Kind::WallSplat, dir: Vec3::Z, strength: 1.0, t: 0.15 }.sample(1.0, Quat::IDENTITY);
        assert!(p.scale.z < 0.8 && p.scale.x > 1.05, "{:?}", p.scale);
        // Turned 90°: the body-local x axis faces the wall instead.
        let p = React { kind: Kind::WallSplat, dir: Vec3::Z, strength: 1.0, t: 0.15 }.sample(1.0, Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        assert!(p.scale.x < 0.8 && p.scale.z > 1.05, "{:?}", p.scale);
    }
}
