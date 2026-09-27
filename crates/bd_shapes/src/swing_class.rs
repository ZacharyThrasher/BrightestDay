//! WEAPON FEEL: per-class **movesets** — swing curves that replace a held form's own curves when the
//! weapon's class (decided by bd_sim `constructs::weapon_class` from form, size, weight and
//! mechanics — never names) asks for a different body language than the form default.
//!
//! * [`MoveSet::Form`] — the form's own curves ([`crate::swing_curve`]); medium weapons (sword,
//!   katana, mace, hammer, axe) keep exactly what they had.
//! * [`MoveSet::Light`] — knives, daggers, rapiers, small blades: short flicks (right→left,
//!   left→right) and a straight stab finisher; the charged move is a spinning lunge cut. Tiny arcs,
//!   quick anticipation, short recovery.
//! * [`MoveSet::Heavy`] — greatswords, warhammers, great axes, anchors: a huge overhead chop you can
//!   read coming (the item goes all the way back over the shoulder), a wide horizontal sweep, an
//!   overhead finisher, and a charged slam with the deepest coil in the game. Long anticipation,
//!   big follow-through that buries the head in the ground, slow settle.
//! * [`MoveSet::Reach`] — spears, quarterstaffs, glaives: thrust, twisting thrust, then a wide
//!   low sweep at full length; the charged move is a long coiled thrust; RMB the lunge thrust.
//!
//! Sets only author the swings that differ ([`swing_curve_set`] returns `None` otherwise — the
//! caller falls back to the form curve: shield blocks, whip lashes, throws, the hammer's pound).
//! Same conventions as [`crate::swing`] (hand frame, Euler YXZ in degrees, stretch); mass
//! re-timing ([`crate::swing::reweight`]) applies on top, so a heavier greatsword is slower still.

use bd_core::mechanics::Form;
use glam::Vec3;

use crate::ease::Ease;
use crate::swing::{SwingCurve, SwingKey, SwingKind};

/// Which family of swing curves a held weapon uses (see the module docs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MoveSet {
    #[default]
    Form = 0,
    Light = 1,
    Heavy = 2,
    Reach = 3,
}

impl MoveSet {
    /// Every set with its own curves (not `Form`).
    pub const AUTHORED: [MoveSet; 3] = [MoveSet::Light, MoveSet::Heavy, MoveSet::Reach];
}

/// (t, pitch, yaw, roll, pos, stretch, ease)
type Kf = (f32, f32, f32, f32, [f32; 3], f32, Ease);

fn build(name: &str, kind: SwingKind, ant: f32, act: f32, rec: f32, hold_at: f32, keys: &[Kf]) -> SwingCurve {
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

/// The class curve for `kind`, or `None` when the set keeps the form's own curve for it.
/// `form` only matters for the rest pose of a set (a pole rests pointing forward).
pub fn swing_curve_set(set: MoveSet, form: Form, kind: SwingKind) -> Option<SwingCurve> {
    use Ease::*;
    use SwingKind as S;
    let _ = form;
    match set {
        MoveSet::Form => None,
        // ------------------------------------------------------------------ light
        MoveSet::Light => {
            const REST: [f32; 3] = [0.0, 0.0, 0.0];
            Some(match kind {
                S::Primary | S::Combo(0) => build("light_flick_rl", kind, 0.035, 0.06, 0.1, 0.035, &[
                    (0.0, 35.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.035, 12.0, -70.0, -90.0, [0.08, 0.03, 0.02], 0.94, OutCubic),
                    (0.095, -6.0, 72.0, -90.0, [-0.09, 0.0, -0.15], 1.12, OutQuad),
                    (0.12, -7.0, 82.0, -85.0, [-0.1, 0.0, -0.13], 0.98, OutQuad),
                    (0.195, 35.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                S::Combo(1) => build("light_flick_lr", kind, 0.035, 0.06, 0.1, 0.035, &[
                    (0.0, 35.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.035, 22.0, 66.0, 90.0, [-0.07, 0.08, 0.0], 0.94, OutCubic),
                    (0.095, -14.0, -76.0, 90.0, [0.1, -0.05, -0.15], 1.12, OutQuad),
                    (0.12, -16.0, -84.0, 86.0, [0.11, -0.05, -0.13], 0.98, OutQuad),
                    (0.195, 35.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                // Finisher: pull back, then a straight stab (the crit).
                S::Combo(_) => build("light_stab_finisher", kind, 0.05, 0.06, 0.13, 0.05, &[
                    (0.0, 35.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.05, 4.0, 4.0, 0.0, [0.02, 0.02, 0.24], 0.88, OutCubic),
                    (0.11, -2.0, 0.0, 0.0, [0.0, 0.0, -0.58], 1.22, OutQuad),
                    (0.14, -2.0, 0.0, 0.0, [0.0, 0.0, -0.52], 1.0, OutQuad),
                    (0.24, 35.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                // Charged: a spinning lunge cut that carries on round to rest (a full turn).
                S::Heavy => build("light_spin_cut", kind, 0.14, 0.1, 0.2, 0.14, &[
                    (0.0, 35.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.14, 12.0, -150.0, -90.0, [0.15, -0.04, 0.16], 0.86, OutCubic),
                    (0.24, -6.0, 170.0, -90.0, [-0.2, 0.0, -0.3], 1.2, InQuad),
                    (0.29, -8.0, 215.0, -85.0, [-0.12, 0.0, -0.18], 1.0, OutQuad),
                    (0.44, 35.0, 360.0, 0.0, REST, 1.0, InOutSine),
                ]),
                _ => return None,
            })
        }
        // ------------------------------------------------------------------ heavy
        MoveSet::Heavy => {
            const REST: [f32; 3] = [0.0, 0.0, 0.0];
            Some(match kind {
                // The overhead chop: all the way back over the shoulder, then down through the floor.
                S::Primary | S::Combo(0) => build("heavy_overhead", kind, 0.16, 0.1, 0.3, 0.16, &[
                    (0.0, 40.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.16, 192.0, 12.0, 0.0, [0.06, 0.34, 0.26], 0.78, OutCubic),
                    (0.26, -86.0, -4.0, 0.0, [0.0, -0.42, -0.5], 1.32, InCubic),
                    (0.34, -80.0, -3.0, 0.0, [0.0, -0.38, -0.48], 0.9, OutQuad),
                    (0.56, 40.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                // The wide sweep: coiled far right, the whole body swings it through.
                S::Combo(1) => build("heavy_wide_sweep", kind, 0.15, 0.12, 0.3, 0.15, &[
                    (0.0, 40.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.15, 4.0, -165.0, -90.0, [0.26, 0.02, 0.2], 0.8, OutCubic),
                    (0.27, -12.0, 150.0, -90.0, [-0.26, -0.08, -0.3], 1.3, InQuad),
                    (0.35, -14.0, 178.0, -85.0, [-0.28, -0.08, -0.25], 0.95, OutQuad),
                    (0.57, 40.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                S::Combo(_) => build("heavy_overhead_finisher", kind, 0.18, 0.1, 0.34, 0.18, &[
                    (0.0, 40.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.18, 198.0, 6.0, 0.0, [0.0, 0.38, 0.28], 0.75, OutCubic),
                    (0.28, -90.0, 0.0, 0.0, [0.0, -0.46, -0.55], 1.34, InCubic),
                    (0.37, -84.0, 0.0, 0.0, [0.0, -0.42, -0.52], 0.88, OutQuad),
                    (0.62, 40.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                // Charged slam: the deepest coil (held at `hold_at`), then the ground breaks.
                S::Heavy => build("heavy_charged_slam", kind, 0.3, 0.1, 0.38, 0.3, &[
                    (0.0, 40.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.3, 205.0, 0.0, 0.0, [0.0, 0.44, 0.32], 0.72, OutCubic),
                    (0.4, -96.0, 0.0, 0.0, [0.0, -0.52, -0.62], 1.36, InCubic),
                    (0.5, -90.0, 0.0, 0.0, [0.0, -0.48, -0.6], 0.88, OutQuad),
                    (0.78, 40.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                _ => return None,
            })
        }
        // ------------------------------------------------------------------ reach
        MoveSet::Reach => {
            const REST: [f32; 3] = [0.0, 0.0, 0.1];
            Some(match kind {
                S::Primary | S::Combo(0) => build("reach_thrust", kind, 0.07, 0.07, 0.15, 0.07, &[
                    (0.0, 5.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.07, 3.0, 0.0, 0.0, [0.02, 0.03, 0.4], 0.9, OutCubic),
                    (0.14, -2.0, 0.0, 0.0, [0.0, 0.0, -1.05], 1.18, OutQuad),
                    (0.18, -2.0, 0.0, 0.0, [0.0, 0.0, -1.1], 1.0, OutQuad),
                    (0.29, 5.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                // Second thrust: a little off-line with a twist of the shaft (reads as a new jab).
                S::Combo(1) => build("reach_thrust_twist", kind, 0.07, 0.07, 0.15, 0.07, &[
                    (0.0, 5.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.07, 6.0, -8.0, -60.0, [0.06, 0.06, 0.38], 0.9, OutCubic),
                    (0.14, -4.0, 3.0, 120.0, [-0.03, -0.02, -1.12], 1.2, OutQuad),
                    (0.18, -4.0, 3.0, 140.0, [-0.03, -0.02, -1.16], 1.0, OutQuad),
                    (0.29, 5.0, 0.0, 180.0, REST, 1.0, InOutSine),
                ]),
                // Finisher: a wide, low sweep at full length (spacing: the tip is the sweet spot).
                S::Combo(_) => build("reach_sweep", kind, 0.12, 0.13, 0.26, 0.12, &[
                    (0.0, 5.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.12, 10.0, -140.0, -90.0, [0.2, -0.08, 0.14], 0.9, OutCubic),
                    (0.25, 2.0, 130.0, -90.0, [-0.2, -0.14, -0.36], 1.15, InQuad),
                    (0.32, 2.0, 156.0, -85.0, [-0.22, -0.14, -0.3], 1.0, OutQuad),
                    (0.51, 5.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                // Charged: drawn far back along the aim, then the longest thrust.
                S::Heavy => build("reach_charged_thrust", kind, 0.2, 0.1, 0.26, 0.2, &[
                    (0.0, 5.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.2, 4.0, 0.0, 0.0, [0.04, 0.05, 0.62], 0.85, OutCubic),
                    (0.3, -2.0, 0.0, 0.0, [0.0, 0.0, -1.45], 1.25, OutQuad),
                    (0.36, -2.0, 0.0, 0.0, [0.0, 0.0, -1.5], 1.0, OutQuad),
                    (0.56, 5.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                // RMB: the lunge thrust (the wielder dashes along the aim with it).
                S::Secondary => build("reach_lunge", kind, 0.1, 0.2, 0.2, 0.1, &[
                    (0.0, 5.0, 0.0, 0.0, REST, 1.0, Linear),
                    (0.1, 2.0, 0.0, 0.0, [0.0, 0.0, 0.3], 0.9, OutCubic),
                    (0.16, -2.0, 0.0, 0.0, [0.0, 0.0, -1.0], 1.2, OutQuad),
                    (0.3, -2.0, 0.0, 0.0, [0.0, 0.0, -1.05], 1.1, Linear),
                    (0.5, 5.0, 0.0, 0.0, REST, 1.0, InOutSine),
                ]),
                _ => return None,
            })
        }
    }
}

/// [`swing_curve_set`] re-timed for a weight (the same rules as [`crate::swing_curve_weighted`]).
pub fn swing_curve_set_weighted(set: MoveSet, form: Form, kind: SwingKind, weight: f32) -> Option<SwingCurve> {
    swing_curve_set(set, form, kind).map(|c| crate::swing::reweight(c, weight))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [SwingKind; 7] = [SwingKind::Primary, SwingKind::Heavy, SwingKind::Combo(0), SwingKind::Combo(1), SwingKind::Combo(2), SwingKind::Secondary, SwingKind::Throw];

    #[test]
    fn authored_sets_are_well_formed() {
        for set in MoveSet::AUTHORED {
            for k in KINDS {
                let Some(c) = swing_curve_set(set, Form::Sword, k) else { continue };
                assert!(c.keys.len() >= 3, "{}", c.name);
                assert!(c.keys.windows(2).all(|w| w[1].t > w[0].t), "{}: keys ascend", c.name);
                let last = c.keys[c.keys.len() - 1].t;
                assert!((last - c.duration()).abs() < 1e-4, "{}: last key = duration ({last} vs {})", c.name, c.duration());
                assert!(c.hold_at <= c.anticipation + 1e-5, "{}", c.name);
                // Weighted versions stay monotonic and end exactly at the duration.
                for w in [0.6, 1.0, 1.6, 2.2] {
                    let r = swing_curve_set_weighted(set, Form::Sword, k, w).expect("curve");
                    assert!(r.keys.windows(2).all(|p| p[1].t >= p[0].t), "{} @ {w}", r.name);
                    assert!((r.keys[r.keys.len() - 1].t - r.duration()).abs() < 1e-4);
                }
            }
        }
        assert!(swing_curve_set(MoveSet::Form, Form::Sword, SwingKind::Primary).is_none());
    }

    /// Light flicks are over before a heavy has finished winding up; heavy strikes are readable.
    #[test]
    fn light_is_quick_and_heavy_is_readable() {
        let light = swing_curve_set(MoveSet::Light, Form::Sword, SwingKind::Combo(0)).expect("light");
        let heavy = swing_curve_set(MoveSet::Heavy, Form::Hammer, SwingKind::Combo(0)).expect("heavy");
        let medium = crate::swing_curve(Form::Sword, SwingKind::Combo(0));
        assert!(light.duration() < medium.duration());
        assert!(light.anticipation < medium.anticipation);
        assert!(heavy.anticipation >= 2.0 * medium.anticipation, "a heavy wind-up you can read and dodge");
        assert!(heavy.duration() > 1.8 * light.duration());
        let slam = swing_curve_set(MoveSet::Heavy, Form::Hammer, SwingKind::Heavy).expect("slam");
        assert!(slam.anticipation > heavy.anticipation && slam.recovery > heavy.recovery);
    }
}
