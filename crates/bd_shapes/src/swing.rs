//! Held-item swing curves as **timed data**. The client animates the item with `sample(t)` and the sim
//! sweeps the item's hitbox along the *same* curve during the active window (use `sweep` to substep
//! fast arcs so nothing tunnels).
//!
//! Hand frame: origin = the wielder's hand, −Z = look direction, +Y up, +X right. `sample(t)` returns
//! the item transform relative to the hand (metres); the item's own model space has the grip at the
//! origin, pointing −Z, strike side −Y (see `forms::held`).
//!
//! Rotation per key = `Ry(yaw) · Rx(pitch) · Rz(roll)` with Euler angles interpolated numerically (so
//! arcs larger than 180° go the long way, as authored). `stretch` scales the item along its axis
//! (volume-preserving squash & stretch: X/Y get `1/sqrt(stretch)`).

use bd_core::mechanics::{Form, FormCategory};
use glam::{Affine3A, EulerRot, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::ease::Ease;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwingKind {
    /// Tap LMB.
    Primary,
    /// Hold-release LMB (charge: hold `t` at `hold_at`, then play on).
    Heavy,
    /// Nth hit of a combo (swords/gauntlets); other forms treat it as Primary.
    Combo(u8),
    /// RMB (parry / block / uppercut / aim / lunge…).
    Secondary,
    /// Throw (release at the end of the active window).
    Throw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwingPhase {
    Anticipation,
    Active,
    Recovery,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwingKey {
    /// Seconds from the start of the swing.
    pub t: f32,
    /// Degrees: x = pitch (about X), y = yaw (about Y), z = roll (about the item axis Z).
    pub euler_deg: Vec3,
    /// Hand-relative offset (m).
    pub pos: Vec3,
    /// Along-axis stretch (1 = rest).
    pub stretch: f32,
    /// Ease used to arrive at this key from the previous one.
    pub ease: Ease,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwingCurve {
    pub name: String,
    pub kind: SwingKind,
    pub anticipation: f32,
    pub active: f32,
    pub recovery: f32,
    /// Where to freeze `t` while a charge/draw/block is held (the coiled pose).
    pub hold_at: f32,
    pub keys: Vec<SwingKey>,
}

impl SwingCurve {
    pub fn duration(&self) -> f32 {
        self.anticipation + self.active + self.recovery
    }
    /// [start, end) of the hitbox window.
    pub fn active_window(&self) -> (f32, f32) {
        (self.anticipation, self.anticipation + self.active)
    }
    pub fn is_active(&self, t: f32) -> bool {
        t >= self.anticipation && t < self.anticipation + self.active
    }
    pub fn phase(&self, t: f32) -> SwingPhase {
        if t < self.anticipation {
            SwingPhase::Anticipation
        } else if t < self.anticipation + self.active {
            SwingPhase::Active
        } else if t < self.duration() {
            SwingPhase::Recovery
        } else {
            SwingPhase::Done
        }
    }

    fn eval(&self, t: f32) -> (Vec3, Vec3, f32) {
        let k = &self.keys;
        if k.is_empty() {
            return (Vec3::ZERO, Vec3::ZERO, 1.0);
        }
        if t <= k[0].t {
            return (k[0].euler_deg, k[0].pos, k[0].stretch);
        }
        for w in k.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if t <= b.t {
                let e = b.ease.apply((t - a.t) / (b.t - a.t).max(1e-6));
                return (a.euler_deg.lerp(b.euler_deg, e), a.pos.lerp(b.pos, e), a.stretch + (b.stretch - a.stretch) * e);
            }
        }
        let l = &k[k.len() - 1];
        (l.euler_deg, l.pos, l.stretch)
    }

    /// Item rotation at `t` (hand frame).
    pub fn rotation(&self, t: f32) -> Quat {
        let (e, _, _) = self.eval(t);
        Quat::from_euler(EulerRot::YXZ, e.y.to_radians(), e.x.to_radians(), e.z.to_radians())
    }

    /// Item transform relative to the hand at time `t` (clamped to the curve).
    pub fn sample(&self, t: f32) -> Affine3A {
        let (e, p, s) = self.eval(t);
        let q = Quat::from_euler(EulerRot::YXZ, e.y.to_radians(), e.x.to_radians(), e.z.to_radians());
        let s = s.max(0.05);
        let side = 1.0 / s.sqrt();
        Affine3A::from_scale_rotation_translation(Vec3::new(side, side, s), q, p)
    }

    /// Substeps covering `(t0, t1]` such that the item turns at most `max_step_rad` per step — sweep
    /// hitboxes through these so fast arcs never tunnel. Yields `(t, transform)`.
    pub fn sweep(&self, t0: f32, t1: f32, max_step_rad: f32) -> impl Iterator<Item = (f32, Affine3A)> + '_ {
        let ang = self.rotation(t0).angle_between(self.rotation(t1));
        let travel = (self.eval(t1).1 - self.eval(t0).1).length();
        let n = ((ang / max_step_rad.max(0.01)).ceil() as usize).max((travel / 0.15).ceil() as usize).clamp(1, 64);
        (1..=n).map(move |i| {
            let t = t0 + (t1 - t0) * i as f32 / n as f32;
            (t, self.sample(t))
        })
    }
}

/// (t, pitch, yaw, roll, pos, stretch, ease)
type Kf = (f32, f32, f32, f32, [f32; 3], f32, Ease);

fn curve(name: &str, kind: SwingKind, ant: f32, act: f32, rec: f32, hold_at: f32, keys: &[Kf]) -> SwingCurve {
    SwingCurve {
        name: name.to_string(),
        kind,
        anticipation: ant,
        active: act,
        recovery: rec,
        hold_at,
        keys: keys
            .iter()
            .map(|&(t, p, y, r, pos, s, ease)| SwingKey { t, euler_deg: Vec3::new(p, y, r), pos: Vec3::from_array(pos), stretch: s, ease })
            .collect(),
    }
}

/// The swing curve for a held form (non-held forms get a generic shove).
pub fn swing_curve(form: Form, kind: SwingKind) -> SwingCurve {
    use Ease::*;
    use Form::*;
    use SwingKind as S;
    const Z: [f32; 3] = [0.0, 0.0, 0.0];
    let kind = match (form, kind) {
        (Sword | Gauntlet, k) => k,
        (_, S::Combo(_)) => S::Primary,
        (_, k) => k,
    };
    let form = if form.category() == FormCategory::Held { form } else { Blob0 };
    match (form, kind) {
        // ------------------------------------------------------------------ hammer
        (Hammer, S::Primary | S::Combo(_)) => curve("hammer_overhead", kind, 0.07, 0.09, 0.16, 0.07, &[
            (0.0, 40.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.07, 165.0, 10.0, 0.0, [0.0, 0.18, 0.12], 0.82, OutCubic),
            (0.16, -65.0, -5.0, 0.0, [0.0, -0.22, -0.3], 1.22, InQuad),
            (0.21, -50.0, -3.0, 0.0, [0.0, -0.16, -0.25], 0.9, OutQuad),
            (0.32, 40.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Hammer, S::Heavy) => curve("hammer_heavy", kind, 0.22, 0.11, 0.3, 0.22, &[
            (0.0, 40.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.22, 185.0, 15.0, 0.0, [0.0, 0.3, 0.22], 0.75, OutCubic),
            (0.33, -80.0, -5.0, 0.0, [0.0, -0.35, -0.45], 1.3, InCubic),
            (0.40, -62.0, -3.0, 0.0, [0.0, -0.25, -0.4], 0.86, OutQuad),
            (0.63, 40.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Hammer | Axe, S::Secondary | S::Throw) => curve("overhead_throw", kind, 0.15, 0.08, 0.2, 0.15, &[
            (0.0, 40.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.15, 175.0, 20.0, 0.0, [0.1, 0.25, 0.2], 0.85, OutCubic),
            (0.23, -10.0, 0.0, 0.0, [0.0, 0.0, -0.5], 1.25, InCubic),
            (0.43, 40.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        // ------------------------------------------------------------------- sword
        (Sword, S::Primary | S::Combo(0)) => curve("sword_slash_rl", kind, 0.06, 0.08, 0.14, 0.06, &[
            (0.0, 35.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.06, 10.0, -115.0, -90.0, [0.12, 0.05, 0.05], 0.9, OutCubic),
            (0.14, -5.0, 95.0, -90.0, [-0.12, 0.0, -0.18], 1.18, OutQuad),
            (0.18, -8.0, 118.0, -82.0, [-0.14, 0.0, -0.15], 0.97, OutQuad),
            (0.28, 35.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Sword, S::Combo(1)) => curve("sword_slash_lr", kind, 0.06, 0.08, 0.14, 0.06, &[
            (0.0, 35.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.06, 15.0, 105.0, 90.0, [-0.1, 0.08, 0.0], 0.9, OutCubic),
            (0.14, -8.0, -100.0, 90.0, [0.12, 0.0, -0.18], 1.18, OutQuad),
            (0.18, -10.0, -120.0, 85.0, [0.14, 0.0, -0.15], 0.97, OutQuad),
            (0.28, 35.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Sword, S::Combo(_)) => curve("sword_overhead_finisher", kind, 0.09, 0.08, 0.22, 0.09, &[
            (0.0, 35.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.09, 170.0, 5.0, 0.0, [0.0, 0.2, 0.1], 0.85, OutCubic),
            (0.17, -60.0, 0.0, 0.0, [0.0, -0.2, -0.3], 1.2, InQuad),
            (0.21, -48.0, 0.0, 0.0, [0.0, -0.15, -0.28], 0.94, OutQuad),
            (0.39, 35.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Sword, S::Heavy) => curve("sword_spin_slash", kind, 0.2, 0.12, 0.28, 0.2, &[
            (0.0, 35.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.2, 5.0, -150.0, -90.0, [0.2, 0.05, 0.15], 0.8, OutCubic),
            (0.32, -5.0, 140.0, -90.0, [-0.2, 0.0, -0.25], 1.3, InQuad),
            (0.38, -8.0, 160.0, -85.0, [-0.22, 0.0, -0.2], 0.95, OutQuad),
            (0.6, 35.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Sword, _) => curve("sword_parry", kind, 0.05, 0.15, 0.15, 0.05, &[
            (0.0, 35.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.05, 80.0, 30.0, 90.0, [-0.1, 0.15, -0.1], 1.0, OutCubic),
            (0.2, 82.0, 28.0, 90.0, [-0.1, 0.16, -0.12], 1.0, Linear),
            (0.35, 35.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        // --------------------------------------------------------------------- axe
        (Axe, S::Primary | S::Combo(_)) => curve("axe_cleave", kind, 0.1, 0.1, 0.2, 0.1, &[
            (0.0, 40.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.1, 5.0, -130.0, -90.0, [0.15, 0.05, 0.1], 0.85, OutCubic),
            (0.2, -10.0, 110.0, -90.0, [-0.15, -0.05, -0.2], 1.2, InQuad),
            (0.25, -10.0, 125.0, -85.0, [-0.16, -0.05, -0.18], 0.95, OutQuad),
            (0.4, 40.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Axe, S::Heavy) => curve("axe_heavy_chop", kind, 0.22, 0.11, 0.3, 0.22, &[
            (0.0, 40.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.22, 180.0, 10.0, 0.0, [0.0, 0.3, 0.2], 0.78, OutCubic),
            (0.33, -75.0, -5.0, 0.0, [0.0, -0.35, -0.45], 1.28, InCubic),
            (0.4, -60.0, -3.0, 0.0, [0.0, -0.25, -0.4], 0.88, OutQuad),
            (0.63, 40.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        // ------------------------------------------------------------------- spear
        (Spear, S::Primary | S::Combo(_) | S::Heavy) => {
            let (a, c, r, reach) = if kind == S::Heavy { (0.18, 0.1, 0.25, 1.3) } else { (0.08, 0.08, 0.18, 0.95) };
            curve("spear_thrust", kind, a, c, r, a, &[
                (0.0, 0.0, 0.0, 0.0, [0.0, 0.0, 0.1], 1.0, Linear),
                (a, 3.0, 0.0, 0.0, [0.0, 0.02, 0.38], 0.9, OutCubic),
                (a + c, -2.0, 0.0, 0.0, [0.0, 0.0, -reach], 1.15, OutQuad),
                (a + c + 0.04, -2.0, 0.0, 0.0, [0.0, 0.0, -reach - 0.07], 1.0, OutQuad),
                (a + c + r, 0.0, 0.0, 0.0, [0.0, 0.0, 0.1], 1.0, InOutSine),
            ])
        }
        (Spear, _) => curve("spear_lunge", kind, 0.1, 0.2, 0.2, 0.1, &[
            (0.0, 0.0, 0.0, 0.0, [0.0, 0.0, 0.1], 1.0, Linear),
            (0.1, 2.0, 0.0, 0.0, [0.0, 0.0, 0.3], 0.9, OutCubic),
            (0.16, -2.0, 0.0, 0.0, [0.0, 0.0, -1.0], 1.2, OutQuad),
            (0.3, -2.0, 0.0, 0.0, [0.0, 0.0, -1.05], 1.1, Linear),
            (0.5, 0.0, 0.0, 0.0, [0.0, 0.0, 0.1], 1.0, InOutSine),
        ]),
        // ------------------------------------------------------------------ shield
        (Shield, S::Secondary) => curve("shield_block", kind, 0.06, 0.15, 0.12, 0.06, &[
            (0.0, 0.0, 0.0, 0.0, [-0.1, 0.0, -0.1], 1.0, Linear),
            (0.06, 0.0, -10.0, 0.0, [-0.15, 0.1, -0.3], 1.0, OutBack),
            (0.21, 0.0, -10.0, 0.0, [-0.15, 0.1, -0.3], 1.0, Linear),
            (0.33, 0.0, 0.0, 0.0, [-0.1, 0.0, -0.1], 1.0, InOutSine),
        ]),
        (Shield, _) => curve("shield_bash", kind, 0.08, 0.08, 0.16, 0.08, &[
            (0.0, 0.0, 0.0, 0.0, [-0.1, 0.0, -0.1], 1.0, Linear),
            (0.08, 0.0, 25.0, 0.0, [0.0, 0.0, 0.18], 0.9, OutCubic),
            (0.16, 0.0, 0.0, 0.0, [0.0, 0.0, -0.65], 1.1, OutQuad),
            (0.2, 0.0, 0.0, 0.0, [0.0, 0.0, -0.6], 0.95, OutQuad),
            (0.32, 0.0, 0.0, 0.0, [-0.1, 0.0, -0.1], 1.0, InOutSine),
        ]),
        // ---------------------------------------------------------------- gauntlet
        (Gauntlet, S::Secondary | S::Heavy) => curve("gauntlet_uppercut", kind, 0.1, 0.09, 0.2, 0.1, &[
            (0.0, 0.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.1, -40.0, 0.0, 0.0, [0.0, -0.35, 0.05], 0.9, OutCubic),
            (0.19, 70.0, 0.0, 0.0, [0.0, 0.45, -0.3], 1.2, OutQuad),
            (0.23, 78.0, 0.0, 0.0, [0.0, 0.5, -0.3], 1.0, OutQuad),
            (0.39, 0.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Gauntlet, S::Combo(1)) => curve("gauntlet_hook", kind, 0.06, 0.08, 0.15, 0.06, &[
            (0.0, 0.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.06, 0.0, -55.0, 0.0, [0.25, 0.0, 0.05], 0.9, OutCubic),
            (0.14, 0.0, 40.0, 0.0, [-0.25, 0.0, -0.45], 1.2, OutQuad),
            (0.18, 0.0, 50.0, 0.0, [-0.28, 0.0, -0.4], 1.0, OutQuad),
            (0.29, 0.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Gauntlet, _) => curve("gauntlet_jab", kind, 0.05, 0.07, 0.13, 0.05, &[
            (0.0, 0.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.05, 0.0, -12.0, 0.0, [0.0, -0.02, 0.25], 0.9, OutCubic),
            (0.12, 0.0, 0.0, 0.0, [0.0, 0.0, -0.75], 1.2, OutQuad),
            (0.15, 0.0, 0.0, 0.0, [0.0, 0.0, -0.7], 1.0, OutQuad),
            (0.25, 0.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        // -------------------------------------------------------------------- whip
        (Whip, _) => curve("whip_crack", kind, 0.12, 0.1, 0.2, 0.12, &[
            (0.0, 20.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.12, 130.0, 0.0, 0.0, [0.0, 0.15, 0.1], 1.0, OutCubic),
            (0.22, -20.0, 0.0, 0.0, [0.0, -0.05, -0.2], 1.0, InQuad),
            (0.27, -30.0, 0.0, 0.0, [0.0, -0.05, -0.2], 1.0, OutQuad),
            (0.42, 20.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        // --------------------------------------------------------------------- gun
        (Gun, S::Secondary) => curve("gun_aim", kind, 0.12, 0.0, 0.12, 0.12, &[
            (0.0, 0.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.12, 0.0, 0.0, 0.0, [-0.22, 0.1, 0.1], 1.0, OutCubic),
            (0.24, 0.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Gun, _) => curve("gun_recoil", kind, 0.0, 0.02, 0.08, 0.0, &[
            (0.0, 0.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.02, 12.0, 0.0, 0.0, [0.0, 0.01, 0.1], 0.92, OutQuad),
            (0.1, 0.0, 0.0, 0.0, Z, 1.0, OutCubic),
        ]),
        // --------------------------------------------------------------------- bow
        (Bow, _) => curve("bow_draw_release", kind, 0.45, 0.03, 0.2, 0.45, &[
            (0.0, 0.0, 0.0, -10.0, Z, 1.0, Linear),
            (0.45, 0.0, 0.0, -10.0, [0.0, 0.0, -0.05], 1.0, OutCubic),
            (0.48, -5.0, 0.0, -10.0, [0.0, 0.0, -0.12], 1.0, OutQuad),
            (0.68, 0.0, 0.0, -10.0, Z, 1.0, InOutSine),
        ]),
        // ------------------------------------------------------------------ cannon
        (Cannon, _) => curve("cannon_fire", kind, 0.1, 0.03, 0.45, 0.1, &[
            (0.0, 0.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.1, -4.0, 0.0, 0.0, [0.0, -0.05, 0.0], 0.95, OutCubic),
            (0.13, 25.0, 0.0, 0.0, [0.0, 0.05, 0.45], 1.1, OutQuad),
            (0.3, 8.0, 0.0, 0.0, [0.0, 0.02, 0.2], 0.98, OutCubic),
            (0.58, 0.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        // ------------------------------------------------------------------- staff
        (Staff, S::Secondary) => curve("staff_beam", kind, 0.15, 1.0, 0.2, 0.15, &[
            (0.0, 60.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.15, 0.0, 0.0, 0.0, [0.0, 0.0, -0.25], 1.05, OutBack),
            (1.15, 0.0, 0.0, 0.0, [0.0, 0.0, -0.25], 1.0, Linear),
            (1.35, 60.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        (Staff, _) => curve("staff_cast", kind, 0.1, 0.06, 0.2, 0.1, &[
            (0.0, 60.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.1, 75.0, 0.0, 0.0, [0.0, 0.15, 0.1], 0.95, OutCubic),
            (0.16, 5.0, 0.0, 0.0, [0.0, 0.0, -0.35], 1.1, OutQuad),
            (0.2, 0.0, 0.0, 0.0, [0.0, 0.0, -0.38], 1.0, OutQuad),
            (0.36, 60.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        // -------------------------------------------------------------------- disc
        (Disc, _) => curve("disc_throw", kind, 0.12, 0.06, 0.15, 0.12, &[
            (0.0, 0.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.12, 0.0, 120.0, 0.0, [-0.3, 0.0, 0.1], 1.0, OutCubic),
            (0.18, 0.0, -30.0, 0.0, [0.1, 0.0, -0.4], 1.0, OutQuad),
            (0.33, 0.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
        // ------------------------------------------------------ generic (non-held)
        _ => curve("shove", kind, 0.08, 0.08, 0.16, 0.08, &[
            (0.0, 0.0, 0.0, 0.0, Z, 1.0, Linear),
            (0.08, 0.0, 0.0, 0.0, [0.0, 0.0, 0.2], 0.9, OutCubic),
            (0.16, 0.0, 0.0, 0.0, [0.0, 0.0, -0.5], 1.1, OutQuad),
            (0.32, 0.0, 0.0, 0.0, Z, 1.0, InOutSine),
        ]),
    }
}

/// A swing curve re-timed and re-shaped for a construct's weight (see `forms::weight_of`; 1 = the
/// authored curve exactly). Heavier: longer, deeper wind-up (more coil/stretch), an active window that
/// stays short (slightly *snappier*), a bigger follow-through past the strike and a slower settle.
/// Lighter: everything quicker and tighter. The sim must sweep hitboxes along this same curve.
pub fn swing_curve_weighted(form: Form, kind: SwingKind, weight: f32) -> SwingCurve {
    reweight(swing_curve(form, kind), weight)
}

/// **Telescoping weapons** (`Mechanic::Extends`: Ruyi Jingu Bang, power pole, extending sword…),
/// whatever their form: a pole twirled round the hand in big arcs. Combo 0 / 1 are wide horizontal
/// sweeps (right→left, left→right) whose recovery keeps spinning the pole a full turn back to rest
/// (the twirl); combo 2 is an overhead crash that wheels on round in a vertical windmill; Heavy is the
/// coil (pole drawn back along the aim, `hold_at`) that a charged release turns into the extend thrust
/// or the falling pillar (the sim plays those itself, not this curve); Secondary is the plant / jab.
pub fn swing_curve_extends(kind: SwingKind) -> SwingCurve {
    use Ease::*;
    use SwingKind as S;
    const REST: [f32; 3] = [0.0, 0.0, 0.05];
    match kind {
        S::Primary | S::Combo(0) | S::Throw => curve("staff_twirl_sweep_rl", kind, 0.09, 0.1, 0.24, 0.09, &[
            (0.0, 35.0, 0.0, 0.0, REST, 1.0, Linear),
            (0.09, 4.0, -150.0, 0.0, [0.12, 0.04, 0.12], 0.9, OutCubic),
            (0.19, -2.0, 130.0, 0.0, [-0.12, 0.0, -0.12], 1.12, InQuad),
            (0.25, 0.0, 205.0, 0.0, [-0.08, 0.02, -0.05], 1.0, OutQuad),
            (0.43, 35.0, 360.0, 0.0, REST, 1.0, InOutSine),
        ]),
        S::Combo(1) => curve("staff_twirl_sweep_lr", kind, 0.09, 0.1, 0.24, 0.09, &[
            (0.0, 35.0, 0.0, 0.0, REST, 1.0, Linear),
            (0.09, 6.0, 150.0, 0.0, [-0.12, 0.06, 0.12], 0.9, OutCubic),
            (0.19, -2.0, -130.0, 0.0, [0.14, 0.0, -0.12], 1.12, InQuad),
            (0.25, 0.0, -205.0, 0.0, [0.1, 0.02, -0.05], 1.0, OutQuad),
            (0.43, 35.0, -360.0, 0.0, REST, 1.0, InOutSine),
        ]),
        S::Combo(_) => curve("staff_overhead_windmill", kind, 0.13, 0.1, 0.34, 0.13, &[
            (0.0, 35.0, 0.0, 0.0, REST, 1.0, Linear),
            (0.13, 175.0, 8.0, 0.0, [0.15, 0.25, 0.2], 0.85, OutCubic),
            (0.23, -28.0, 0.0, 0.0, [0.12, -0.18, -0.3], 1.2, InCubic),
            (0.3, -70.0, 0.0, 0.0, [0.22, -0.2, -0.2], 1.0, OutQuad),
            (0.47, -325.0, 0.0, 0.0, REST, 1.0, InOutSine),
        ]),
        S::Heavy => curve("staff_coil_thrust", kind, 0.2, 0.08, 0.3, 0.2, &[
            (0.0, 35.0, 0.0, 0.0, REST, 1.0, Linear),
            (0.2, 2.0, 0.0, 0.0, [0.06, 0.06, 0.55], 0.88, OutCubic),
            (0.28, -1.0, 0.0, 0.0, [0.0, 0.0, -1.2], 1.15, OutQuad),
            (0.34, -1.0, 0.0, 0.0, [0.0, 0.0, -1.25], 1.0, OutQuad),
            (0.58, 35.0, 0.0, 0.0, REST, 1.0, InOutSine),
        ]),
        S::Secondary => curve("staff_plant", kind, 0.05, 0.12, 0.22, 0.05, &[
            (0.0, 35.0, 0.0, 0.0, REST, 1.0, Linear),
            (0.05, 5.0, 0.0, 0.0, [0.0, 0.02, 0.3], 0.92, OutCubic),
            (0.11, -2.0, 0.0, 0.0, [0.0, 0.0, -0.9], 1.12, OutQuad),
            (0.17, -2.0, 0.0, 0.0, [0.0, 0.0, -0.95], 1.0, Linear),
            (0.39, 35.0, 0.0, 0.0, REST, 1.0, InOutSine),
        ]),
    }
}

/// [`swing_curve_extends`] re-timed for a weight (same rules as [`swing_curve_weighted`]).
pub fn swing_curve_extends_weighted(kind: SwingKind, weight: f32) -> SwingCurve {
    reweight(swing_curve_extends(kind), weight)
}

pub(crate) fn reweight(c: SwingCurve, weight: f32) -> SwingCurve {
    let mut c = c;
    let w = if weight.is_finite() { weight.clamp(0.5, 2.5) } else { 1.0 };
    if (w - 1.0).abs() < 1e-4 || c.keys.len() < 2 {
        return c;
    }
    let (a, act, r) = (c.anticipation, c.active, c.recovery);
    let mut ka = w.powf(0.75);
    let kc = w.powf(-0.3).clamp(0.8, 1.15);
    let mut kr = w.powf(0.7);
    // Keep the whole swing under 1.55 s.
    let total = a * ka + act * kc + r * kr;
    if total > 1.55 {
        let f = ((1.55 - act * kc) / (a * ka + r * kr)).max(0.1);
        ka *= f;
        kr *= f;
    }
    let remap = |t: f32| -> f32 {
        if t <= a {
            t * ka
        } else if t <= a + act {
            a * ka + (t - a) * kc
        } else {
            a * ka + act * kc + (t - a - act) * kr
        }
    };
    let rest = c.keys[0];
    let strike_t = a + act;
    let strike = c.keys.iter().copied().min_by(|x, y| (x.t - strike_t).abs().total_cmp(&(y.t - strike_t).abs())).unwrap_or(rest);
    let last_t = c.keys[c.keys.len() - 1].t;
    let wind = 1.0 + 0.3 * (w - 1.0);
    let over = 1.0 + 0.35 * (w - 1.0);
    for k in c.keys.iter_mut() {
        let t0 = k.t;
        if (t0 - a).abs() < 1e-4 && a > 0.0 {
            // Deeper coil.
            k.euler_deg = rest.euler_deg + (k.euler_deg - rest.euler_deg) * wind;
            k.pos = rest.pos + (k.pos - rest.pos) * wind;
            k.stretch = 1.0 + (k.stretch - 1.0) * (1.0 + 0.6 * (w - 1.0));
        } else if (t0 - strike.t).abs() < 1e-4 {
            k.stretch = 1.0 + (k.stretch - 1.0) * (1.0 + 0.5 * (w - 1.0));
        } else if t0 > strike.t && t0 < last_t - 1e-4 {
            // Follow-through past the strike, then settle.
            k.euler_deg = rest.euler_deg + (k.euler_deg - rest.euler_deg) * over;
            k.pos = rest.pos + (k.pos - rest.pos) * over;
            k.stretch = 1.0 + (k.stretch - 1.0) * over;
        }
        k.t = remap(t0);
    }
    c.hold_at = remap(c.hold_at);
    c.anticipation = a * ka;
    c.active = act * kc;
    c.recovery = r * kr;
    // Exact end = duration (keys are monotonic after a monotonic remap).
    let d = c.duration();
    if let Some(l) = c.keys.last_mut() {
        l.t = d;
    }
    c
}
